"""Opt-in development HTTPS fixture; never imported by the application.

Only a marker-bearing temporary profile below .local/builds/work/browser-spikes
can receive its generated CA. The system trust store and user profiles are unused.
CONNECT is terminated locally, never forwarded. No request headers/bodies logged.
"""
import ctypes
import json
import os
from pathlib import Path
import socketserver
import ssl
import subprocess
import sys
import threading

HOST = "icp.administracionelectronica.gob.es"
os.umask(0o077)
profile = Path(sys.argv[2]).resolve(strict=True)
root = Path(".local/builds/work/browser-spikes").resolve(strict=True)
if profile.parent != root or (profile / ".ceh-owned-fixture").read_text() != "synthetic TLS fixture":
    raise SystemExit("Only an owned temporary fixture profile is permitted")

def initialize(binary):
    if (profile / "cert9.db").exists() or (profile / ".parentlock").exists():
        raise RuntimeError("Fixture trust must be initialized before browser startup")
    def openssl(*args):
        subprocess.run(["openssl", *args], cwd=profile, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=CEH temporary offline test CA", "-addext", "basicConstraints=critical,CA:TRUE", "-keyout", "ca.key", "-out", "ca.pem")
    openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=" + HOST, "-keyout", "server.key", "-out", "server.csr")
    (profile / "leaf.ext").write_text("subjectAltName=DNS:" + HOST + "\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
    openssl("x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-days", "1", "-extfile", "leaf.ext", "-out", "server.pem")
    openssl("x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der")
    folder = Path(binary).resolve(strict=True).parent
    if sys.platform != "darwin":
        raise RuntimeError("This native TLS fixture is currently verified only on macOS")
    ctypes.CDLL(str(folder / "libmozglue.dylib"), mode=ctypes.RTLD_GLOBAL)
    nss = ctypes.CDLL(str(folder / "libnss3.dylib"))
    class Trust(ctypes.Structure):
        _fields_ = [("ssl", ctypes.c_uint), ("email", ctypes.c_uint), ("objects", ctypes.c_uint)]
    def fn(name, result, *args):
        value = getattr(nss, name)
        value.restype, value.argtypes = result, args
        return value
    init = fn("NSS_InitReadWrite", ctypes.c_int, ctypes.c_char_p)
    shutdown = fn("NSS_Shutdown", ctypes.c_int)
    slot_get = fn("PK11_GetInternalKeySlot", ctypes.c_void_p)
    slot_free = fn("PK11_FreeSlot", None, ctypes.c_void_p)
    need_init = fn("PK11_NeedUserInit", ctypes.c_int, ctypes.c_void_p)
    init_pin = fn("PK11_InitPin", ctypes.c_int, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p)
    check_password = fn("PK11_CheckUserPassword", ctypes.c_int, ctypes.c_void_p, ctypes.c_char_p)
    decode = fn("CERT_DecodeCertFromPackage", ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int)
    destroy = fn("CERT_DestroyCertificate", None, ctypes.c_void_p)
    add = fn("PK11_ImportCert", ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ulong, ctypes.c_char_p, ctypes.c_int)
    default_db = fn("CERT_GetDefaultCertDB", ctypes.c_void_p)
    find = fn("CERT_FindCertByNickname", ctypes.c_void_p, ctypes.c_void_p, ctypes.c_char_p)
    last_error = fn("PR_GetError", ctypes.c_int)
    decode_trust = fn("CERT_DecodeTrustString", ctypes.c_int, ctypes.POINTER(Trust), ctypes.c_char_p)
    change_trust = fn("CERT_ChangeCertTrust", ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(Trust))
    if init(os.fsencode("sql:" + str(profile))) != 0:
        raise RuntimeError("Cannot initialize fixture NSS database")
    slot, cert = None, None
    try:
        slot = slot_get()
        if not slot: raise RuntimeError("Cannot open fixture token")
        if need_init(slot) and init_pin(slot, None, b"") != 0:
            raise RuntimeError("Cannot initialize empty fixture token")
        if check_password(slot, b"") != 0:
            raise RuntimeError("Cannot unlock empty fixture token")
        data = (profile / "ca.der").read_bytes()
        cert = decode(data, len(data))
        if not slot or not cert or add(slot, cert, 0, b"ceh-owned-offline-fixture", 0) != 0:
            raise RuntimeError("Cannot import fixture CA")
        stored = find(default_db(), b"ceh-owned-offline-fixture")
        if not stored: raise RuntimeError("Cannot find imported fixture CA")
        try:
            trust = Trust()
            if decode_trust(ctypes.byref(trust), b"C,,") != 0:
                raise RuntimeError("Cannot decode fixture trust: " + str(last_error()))
            if change_trust(default_db(), stored, ctypes.byref(trust)) != 0:
                raise RuntimeError("Cannot trust fixture CA: " + str(last_error()))
        finally: destroy(stored)
    finally:
        if cert: destroy(cert)
        if slot: slot_free(slot)
        shutdown()

lock = threading.Lock()
def serve():
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(profile / "server.pem", profile / "server.key")
    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            try:
                self.request.settimeout(4)
                first = self.request.makefile("rb")
                request = first.readline(8193)
                for _ in range(128):
                    line = first.readline(8193)
                    if line in (b"\r\n", b"\n", b""): break
                    if len(line) > 8192: return
                else: return
                first.close()
                if request.strip() not in (("CONNECT " + HOST + ":443 HTTP/1.1").encode(), ("CONNECT " + HOST + ":443 HTTP/1.0").encode()):
                    self.request.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    return
                self.request.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
                with context.wrap_socket(self.request, server_side=True) as tls:
                    stream = tls.makefile("rb")
                    method, path, _ = stream.readline(8193).decode("ascii").split()
                    length = 0
                    for _ in range(128):
                        line = stream.readline(8193)
                        if line in (b"\r\n", b"\n", b""): break
                        if len(line) > 8192: return
                        if line.lower().startswith(b"content-length:"): length = int(line.split(b":", 1)[1])
                    else: return
                    if not 0 <= length <= 8192: return
                    stream.read(length)  # Deliberately discard request data.
                    routes = json.loads((profile / "routes.json").read_text())
                    body = routes.get(method + " " + path)
                    status = 200 if body is not None else 404
                    data = (body or "Offline fixture: unmatched route").encode()
                    with lock:
                        with (profile / "requests.jsonl").open("a") as out:
                            out.write(json.dumps({"method": method, "path": path, "status": status}) + "\n")
                    header = ("HTTP/1.1 " + str(status) + " Fixture\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: " + str(len(data)) + "\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n").encode()
                    tls.sendall(header + data)
            except (OSError, ValueError, ssl.SSLError) as error:
                with lock:
                    with (profile / "server-errors.jsonl").open("a") as out:
                        out.write(json.dumps({"type":type(error).__name__,"reason":getattr(error,"reason",None)}) + "\n")
                return
    class Server(socketserver.ThreadingTCPServer):
        allow_reuse_address = True
        daemon_threads = True
    with Server(("127.0.0.1", 0), Handler) as server:
        (profile / "proxy-port").write_text(str(server.server_address[1]))
        server.serve_forever()

if sys.argv[1] == "init": initialize(sys.argv[3])
elif sys.argv[1] == "serve": serve()
else: raise SystemExit("Unknown fixture mode")
