#!/usr/bin/env python3
"""Validate imported TLS material; --connect also checks both configured HTTPS origins."""
import argparse
import json
import os
from pathlib import Path
import ssl
import urllib.error
from server_tls import check_connections, validate_certificate

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
parser.add_argument('--certificate', type=Path, required=True, help='PEM leaf certificate followed by intermediates')
parser.add_argument('--key', type=Path, required=True)
parser.add_argument('--ca-file', type=Path, help='Trusted private CA PEM; otherwise use configured/system roots')
parser.add_argument('--connect', action='store_true', help='Explicitly check portal and console /api/health over TLS')
args = parser.parse_args()
try:
    settings = json.loads((args.state_dir / 'server.json').read_text())
    origins = [settings['origin'], settings['consoleOrigin']]
    ca = args.ca_file or settings.get('caFile')
    validate_certificate(origins, args.certificate, args.key, ca)
    if args.connect:
        check_connections(origins, ca)
except (OSError, KeyError, ValueError, ssl.SSLError, urllib.error.URLError) as error:
    parser.error(str(error))
print('PASS certificate/key match, chain trust, validity and portal/console hostname or IP SAN')
if args.connect:
    print('PASS HTTPS trust and health for both configured origins')
print('Client trust is separate: import the CA into the OS/browser trust store and pass --ca-file to miy context add/init. No trust store was changed.')
