"""Security policy at the certificate discovery transport boundary."""
import ssl
from contextlib import nullcontext
from app.desktop import certificates


def test_discovery_sets_tls12_minimum_before_handshake(monkeypatch):
    class DiscoveryComplete(Exception):
        pass

    class Context:
        minimum_version = None

        def wrap_socket(self, tcp, *, server_hostname):
            assert self.minimum_version == ssl.TLSVersion.TLSv1_2
            assert server_hostname == "example.test"
            raise DiscoveryComplete

    context = Context()
    monkeypatch.setattr(certificates.ssl, "SSLContext", lambda protocol: context)
    monkeypatch.setattr(certificates.socket, "create_connection", lambda *args, **kwargs: nullcontext(object()))
    import pytest
    with pytest.raises(DiscoveryComplete):
        certificates.get_chain("example.test")
