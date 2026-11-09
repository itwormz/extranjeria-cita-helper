# TLS intermediate

`rapidssl-tls-rsa-ca-g1.der` is the public RapidSSL TLS RSA CA G1 intermediate from https://cacerts.digicert.com/RapidSSLTLSRSACAG1.crt (HTTPS-verified download).

SHA-256: `4422e963ee53cd58cc9f85cd40bf5ffec0095fdf1a154535661c1c06bcadc69b`. Expires 2027-11-02. Issuer: DigiCert Global Root G2.

Imported only into CEH-owned Firefox profiles, with `includeTrust = false`. No root trust, certificate exception, system store or personal browser profile is changed. Firefox itself validates the server hostname, validity and chain to an existing trusted root.

Update this public intermediate when the site's issuer changes. Failure must not disable TLS verification. Dynamic issuer discovery from the Python implementation remains a later parity task.
