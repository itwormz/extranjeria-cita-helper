"""Validate current ICP intermediates and cache them in Camoufox's NSS profile.

An unauthenticated TLS handshake supplies discovery data only. No data from it
is trusted or persisted until the complete chain verifies against public roots.
"""
import ctypes
import hashlib
import os
import re
import socket
import ssl
import sys
from pathlib import Path
from urllib.parse import urlsplit
from urllib.request import urlopen

import certifi
from cryptography import x509
from cryptography.hazmat.primitives.serialization import Encoding
from cryptography.x509.oid import AuthorityInformationAccessOID
from cryptography.x509.verification import PolicyBuilder, Store, VerificationError

ICP_HOST = 'icp.administracionelectronica.gob.es'
MAX_CERT_BYTES = 1024 * 1024


def trusted_roots():
    return x509.load_pem_x509_certificates(Path(certifi.where()).read_bytes())


def get_chain(host):
    if not re.fullmatch(r'[a-zA-Z0-9.-]{1,253}', host):
        raise ValueError('Invalid TLS hostname')
    # Discovery only: validation happens in verify_chain before use or storage.
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    with socket.create_connection((host, 443), timeout=10) as tcp:
        with context.wrap_socket(tcp, server_hostname=host) as connection:
            if hasattr(connection, 'get_unverified_chain'):
                return [x509.load_der_x509_certificate(data) for data in connection.get_unverified_chain()]
            # Python 3.12 exposes the same OpenSSL data through its SSL object.
            if hasattr(connection._sslobj, 'get_unverified_chain'):
                return [x509.load_pem_x509_certificate(cert.public_bytes().encode())
                        for cert in connection._sslobj.get_unverified_chain()]
            return [x509.load_der_x509_certificate(connection.getpeercert(binary_form=True))]


def verify_chain(leaf, intermediates, roots, host, at_time=None):
    if not roots:
        raise VerificationError('No trusted roots available')
    builder = PolicyBuilder().store(Store(roots))
    if at_time is not None:
        builder = builder.time(at_time)
    chain = builder.build_server_verifier(x509.DNSName(host)).verify(leaf, intermediates)
    return list(chain[1:-1])  # Never cache the leaf or a trust anchor.


def fetch_issuers(leaf):
    certificates = []
    current = leaf
    seen = set()
    for _ in range(5):
        try:
            aia = current.extensions.get_extension_for_class(x509.AuthorityInformationAccess).value
        except x509.ExtensionNotFound:
            break
        urls = [entry.access_location.value for entry in aia
                if entry.access_method == AuthorityInformationAccessOID.CA_ISSUERS
                and isinstance(entry.access_location, x509.UniformResourceIdentifier)]
        issuer = None
        for url in urls:
            parts = urlsplit(url)
            if parts.scheme not in {'http', 'https'} or not parts.hostname or parts.username or url in seen:
                continue
            seen.add(url)
            with urlopen(url, timeout=10) as response:
                if urlsplit(response.url).scheme not in {'http', 'https'}:
                    raise RuntimeError('Invalid certificate redirect')
                data = response.read(MAX_CERT_BYTES + 1)
            if len(data) > MAX_CERT_BYTES:
                raise RuntimeError('Certificate response too large')
            issuer = (x509.load_pem_x509_certificate(data) if b'-----BEGIN' in data
                      else x509.load_der_x509_certificate(data))
            break
        if issuer is None:
            break
        certificates.append(issuer)
        if issuer.subject == issuer.issuer:
            break
        current = issuer
    return certificates


def write_cache(directory, certificates):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    for certificate in certificates:
        data = certificate.public_bytes(Encoding.PEM)
        path = directory / (hashlib.sha256(data).hexdigest() + '.pem')
        temp = path.with_suffix('.tmp')
        temp.write_bytes(data)
        temp.replace(path)


def resolve_intermediates(directory, host=ICP_HOST):
    presented = get_chain(host)
    leaf = presented[0]
    cached = []
    for path in Path(directory).glob('*.pem'):
        try:
            cached.append(x509.load_pem_x509_certificate(path.read_bytes()))
        except ValueError:
            continue
    roots = trusted_roots()
    try:
        verified = verify_chain(leaf, presented[1:] + cached, roots, host)
    except VerificationError:
        verified = verify_chain(leaf, fetch_issuers(leaf), roots, host)
    write_cache(directory, verified)
    return verified


def import_intermediates(profile, executable, certificates):
    """Import with no trust flags using the NSS library shipped with Camoufox."""
    if not certificates:
        return 0
    folder = Path(executable).parent
    library = folder / {'win32':'nss3.dll', 'darwin':'libnss3.dylib'}.get(sys.platform, 'libnss3.so')
    dll_directory = os.add_dll_directory(str(folder)) if sys.platform == 'win32' else None
    try:
        if sys.platform == 'darwin':
            ctypes.CDLL(str(folder / 'libmozglue.dylib'), mode=ctypes.RTLD_GLOBAL)
        nss = ctypes.CDLL(str(library))
        # Linux ships the certificate package decoder in libsmime3, unlike
        # the merged NSS library used by the macOS/Windows browser builds.
        decoder = ctypes.CDLL(str(folder / 'libsmime3.so')) if sys.platform == 'linux' else nss
        def function(name, result, *arguments, library=nss):
            method = getattr(library, name)
            method.restype = result
            method.argtypes = arguments
            return method
        init = function('NSS_InitReadWrite', ctypes.c_int, ctypes.c_char_p)
        shutdown = function('NSS_Shutdown', ctypes.c_int)
        slot_get = function('PK11_GetInternalKeySlot', ctypes.c_void_p)
        slot_free = function('PK11_FreeSlot', None, ctypes.c_void_p)
        decode = function('CERT_DecodeCertFromPackage', ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, library=decoder)
        destroy = function('CERT_DestroyCertificate', None, ctypes.c_void_p)
        add = function('PK11_ImportCert', ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p,
                       ctypes.c_ulong, ctypes.c_char_p, ctypes.c_int)
        profile = Path(profile)
        profile.mkdir(parents=True, exist_ok=True)
        if init(os.fsencode('sql:' + str(profile))) != 0:
            raise RuntimeError('Could not open Camoufox certificate cache')
        slot = None
        try:
            slot = slot_get()
            if not slot:
                raise RuntimeError('Could not open NSS certificate slot')
            for certificate in certificates:
                data = certificate.public_bytes(Encoding.DER)
                decoded = decode(data, len(data))
                if not decoded:
                    raise RuntimeError('NSS could not decode intermediate certificate')
                try:
                    nickname = ('cita-intermediate-' + hashlib.sha256(data).hexdigest()).encode()
                    if add(slot, decoded, 0, nickname, 0) != 0:
                        raise RuntimeError('NSS could not cache intermediate certificate')
                finally:
                    destroy(decoded)
        finally:
            if slot:
                slot_free(slot)
            shutdown()
    finally:
        if dll_directory:
            dll_directory.close()
    return len(certificates)


def prepare_intermediate_cache(profile, executable, directory, host=ICP_HOST):
    return import_intermediates(profile, executable, resolve_intermediates(directory, host))
