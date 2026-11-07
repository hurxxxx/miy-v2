"""Validate imported server TLS material without modifying the host trust store."""
import ipaddress
from pathlib import Path
import shutil
import ssl
import subprocess
import urllib.parse
import urllib.request


def validate_certificate(origins, certificate, key, ca_file=None):
    if not shutil.which('openssl'):
        raise ValueError('openssl is required to validate server TLS material')
    certificate, key = Path(certificate), Path(key)
    for path in [certificate, key, *([Path(ca_file)] if ca_file else [])]:
        if not path.is_file():
            raise ValueError('A configured certificate, key or CA file is missing')

    def run(arguments, message):
        result = subprocess.run(['openssl', *map(str, arguments)], capture_output=True,
                                timeout=15, stdin=subprocess.DEVNULL)
        if result.returncode:
            # Key/certificate parser output may include private file content.
            raise ValueError(message)
        return result.stdout

    public = run(['x509', '-in', certificate, '-pubkey', '-noout'], 'Invalid server certificate')
    private_public = run(['pkey', '-in', key, '-passin', 'pass:', '-pubout'],
                         'Invalid or encrypted server key; provide a service-readable PEM key')
    if public.strip() != private_public.strip():
        raise ValueError('The server certificate and private key do not match')
    run(['x509', '-in', certificate, '-checkend', '0', '-noout'], 'The server certificate has expired')
    # A fullchain PEM can supply intermediates; trust must still terminate in the
    # selected private CA or system roots. Never make the leaf a trust anchor.
    trust = ['-CAfile', ca_file] if ca_file else []
    for origin in origins:
        parsed = urllib.parse.urlsplit(origin)
        if parsed.scheme != 'https' or not parsed.hostname:
            raise ValueError('TLS validation requires HTTPS origins')
        try:
            ipaddress.ip_address(parsed.hostname)
            identity = ['-verify_ip', parsed.hostname]
        except ValueError:
            identity = ['-verify_hostname', parsed.hostname]
        run(['verify', '-x509_strict', '-purpose', 'sslserver', *trust, *identity,
             '-untrusted', certificate, certificate],
            'Certificate verification failed: check CA trust, chain, hostname/IP SAN and validity')


def check_connections(origins, ca_file=None):
    """Optional explicit live check: verify both origins using normal TLS trust."""
    context = ssl.create_default_context(cafile=str(ca_file) if ca_file else None)

    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *_args, **_kwargs):
            return None

    opener = urllib.request.build_opener(urllib.request.HTTPSHandler(context=context), NoRedirect())
    for origin in origins:
        with opener.open(origin + '/api/health', timeout=15) as response:
            if response.status != 200:
                raise ValueError('A configured HTTPS origin did not return a healthy response')
