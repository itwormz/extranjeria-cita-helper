from datetime import datetime, timedelta, timezone
from pathlib import Path
import pytest
from cryptography import x509
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
from app.desktop import certificates

@pytest.fixture
def chain():
    now=datetime.now(timezone.utc)
    keys=[rsa.generate_private_key(public_exponent=65537,key_size=2048) for _ in range(3)]
    names=[x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,n)]) for n in ['Root','Intermediate','example.test']]
    certs=[]
    for i in range(3):
        issuer=max(0,i-1)
        b=(x509.CertificateBuilder().subject_name(names[i]).issuer_name(names[issuer])
           .public_key(keys[i].public_key()).serial_number(i+1)
           .not_valid_before(now-timedelta(days=1)).not_valid_after(now+timedelta(days=1))
           .add_extension(x509.BasicConstraints(ca=i<2,path_length=None),True)
           .add_extension(x509.KeyUsage(digital_signature=True,content_commitment=False,key_encipherment=i==2,
              data_encipherment=False,key_agreement=False,key_cert_sign=i<2,crl_sign=i<2,encipher_only=False,decipher_only=False),True)
           .add_extension(x509.SubjectKeyIdentifier.from_public_key(keys[i].public_key()),False)
           .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(keys[issuer].public_key()),False))
        if i==2:
            b=b.add_extension(x509.SubjectAlternativeName([x509.DNSName('example.test')]),False)
            b=b.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]),False)
        certs.append(b.sign(keys[issuer],hashes.SHA256()))
    return certs


def test_only_verified_intermediate_is_cached(chain):
    root, intermediate, leaf=chain
    assert certificates.verify_chain(leaf,[intermediate,root],[root],'example.test') == [intermediate]


def test_wrong_hostname_and_untrusted_chain_are_rejected(chain):
    root, intermediate, leaf=chain
    with pytest.raises(certificates.VerificationError): certificates.verify_chain(leaf,[intermediate],[root],'other.test')
    with pytest.raises(certificates.VerificationError): certificates.verify_chain(leaf,[],[root],'example.test')
    with pytest.raises(certificates.VerificationError): certificates.verify_chain(leaf,[intermediate],[],'example.test')


def test_expired_chain_is_rejected(chain):
    root, intermediate, leaf=chain
    with pytest.raises(certificates.VerificationError):
        certificates.verify_chain(leaf,[intermediate],[root],'example.test',datetime.now(timezone.utc)+timedelta(days=2))


def test_cached_chain_avoids_download(tmp_path,monkeypatch,chain):
    root, intermediate, leaf=chain
    certificates.write_cache(tmp_path,[intermediate])
    monkeypatch.setattr(certificates,'get_chain',lambda host:[leaf])
    monkeypatch.setattr(certificates,'trusted_roots',lambda:[root])
    monkeypatch.setattr(certificates,'fetch_issuers',lambda leaf:(_ for _ in ()).throw(AssertionError('download')))
    assert certificates.resolve_intermediates(tmp_path,'example.test') == [intermediate]


def test_failed_verification_does_not_write_cache(tmp_path,monkeypatch,chain):
    root, intermediate, leaf=chain
    monkeypatch.setattr(certificates,'get_chain',lambda host:[leaf])
    monkeypatch.setattr(certificates,'trusted_roots',lambda:[])
    monkeypatch.setattr(certificates,'fetch_issuers',lambda leaf:[intermediate])
    with pytest.raises(certificates.VerificationError): certificates.resolve_intermediates(tmp_path,'example.test')
    assert not list(tmp_path.glob('*.pem'))


def test_complete_server_chain_needs_no_issuer_download(tmp_path,monkeypatch,chain):
    root, intermediate, leaf=chain
    monkeypatch.setattr(certificates,'get_chain',lambda host:[leaf,intermediate])
    monkeypatch.setattr(certificates,'trusted_roots',lambda:[root])
    monkeypatch.setattr(certificates,'fetch_issuers',lambda leaf:(_ for _ in ()).throw(AssertionError('download')))
    assert certificates.resolve_intermediates(tmp_path,'example.test') == [intermediate]


def test_build_removes_local_certificate_installation_policy(tmp_path):
    import json
    from scripts.build_native import sanitize_certificate_policy
    path=tmp_path/'distribution'/'policies.json'
    path.parent.mkdir()
    path.write_text(json.dumps({'policies':{'DisableTelemetry':True,'Certificates':{'Install':['/private/old.pem']}}}))
    sanitize_certificate_policy(tmp_path)
    assert json.loads(path.read_text()) == {'policies':{'DisableTelemetry':True}}


def test_linux_import_uses_smime_decoder(tmp_path, monkeypatch, chain):
    from types import SimpleNamespace
    from unittest.mock import Mock
    nss = SimpleNamespace(**{name: Mock(return_value=result) for name, result in {
        'NSS_InitReadWrite': 0, 'NSS_Shutdown': 0, 'PK11_GetInternalKeySlot': 1,
        'PK11_FreeSlot': None, 'CERT_DestroyCertificate': None, 'PK11_ImportCert': 0,
    }.items()})
    smime = SimpleNamespace(CERT_DecodeCertFromPackage=Mock(return_value=2))
    loaded = []
    def load(path, **kwargs):
        loaded.append(Path(path).name)
        return smime if Path(path).name == 'libsmime3.so' else nss
    monkeypatch.setattr(certificates.sys, 'platform', 'linux')
    monkeypatch.setattr(certificates.ctypes, 'CDLL', load)
    assert certificates.import_intermediates(tmp_path / 'profile', tmp_path / 'camoufox-bin', [chain[1]]) == 1
    assert loaded == ['libnss3.so', 'libsmime3.so']
    smime.CERT_DecodeCertFromPackage.assert_called_once()
    nss.PK11_ImportCert.assert_called_once()
