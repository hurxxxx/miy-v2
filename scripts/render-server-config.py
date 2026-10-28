#!/usr/bin/env python3
"""Render host Nginx/systemd files without installing or restarting any service."""
import argparse
import json
import os
from pathlib import Path
import pwd
import re
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent


def quoted(value):
    value = str(value)
    if any(c in value for c in '\n\r\0'):
        raise ValueError('Configuration values cannot contain control characters')
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def systemd(value, command=False):
    value = quoted(value).replace('%', '%%')
    return value.replace('$', '$$') if command else value


def systemd_directory(value):
    # WorkingDirectory is a single path, not a quoted argument list.
    value = str(value)
    if any(c in value for c in '\n\r\0'):
        raise ValueError('Configuration values cannot contain control characters')
    return value.replace('\\', '\\\\').replace('%', '%%')


def nginx(value):
    if '$' in str(value):
        raise ValueError('Nginx paths must not contain variable expansion characters')
    return quoted(value)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
parser.add_argument('--binary', type=Path, default=Path(os.getenv('MIY_BINARY', str(ROOT / 'target/release/miy'))))
parser.add_argument('--user', default=pwd.getpwuid(os.getuid()).pw_name)
parser.add_argument('--service-prefix', default='miy-v2')
parser.add_argument('--certificate', type=Path)
parser.add_argument('--key', type=Path)
args = parser.parse_args()
if not re.fullmatch(r'[a-z][a-z0-9-]{0,47}', args.service_prefix):
    parser.error('Use a lowercase service prefix')
try:
    pwd.getpwnam(args.user)
except KeyError:
    parser.error('--user must name an existing service account')
state = args.state_dir.resolve()
settings = json.loads((state / 'server.json').read_text())
origin = urllib.parse.urlsplit(settings['origin'])
if origin.scheme == 'https' and (args.certificate is None or args.key is None):
    parser.error('HTTPS requires --certificate PATH and --key PATH')
if origin.scheme not in ('http', 'https'):
    parser.error('Unsupported configured origin')
if origin.scheme == 'http' and not settings.get('allowHttp'):
    parser.error('The server HTTP profile must explicitly allow HTTP')
bind = urllib.parse.urlsplit('//' + settings['bind'])
core_host = {'0.0.0.0': '127.0.0.1', '::': '::1'}.get(bind.hostname, bind.hostname)
core_host = '[' + core_host + ']' if ':' in core_host else core_host
core = f'http://{core_host}:{bind.port}'
identity = f"http://127.0.0.1:{settings['identityPort']}"
public_port = origin.port or (443 if origin.scheme == 'https' else 80)
public_host = '[' + origin.hostname + ']' if ':' in origin.hostname else origin.hostname
public_listen = f'[::]:{public_port}' if ':' in origin.hostname else str(public_port)
upgrade_variable = '$' + args.service_prefix.replace('-', '_') + '_connection_upgrade'
tls = ''
if origin.scheme == 'https':
    tls = f'''    ssl_certificate {nginx(args.certificate.resolve())};
    ssl_certificate_key {nginx(args.key.resolve())};
    ssl_protocols TLSv1.2 TLSv1.3;
'''
forwarding = '''        proxy_set_header Host $http_host;
        proxy_set_header X-Forwarded-Host $http_host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-For $remote_addr;
'''
nginx_text = f'''# Generated from this deployment's explicit MIY_ORIGIN. Install only this site.
map $http_upgrade {upgrade_variable} {{
    default upgrade;
    '' close;
}}
server {{
    listen {public_listen}{' ssl' if tls else ''};
    server_name {nginx(public_host)};
{tls}    client_max_body_size 1024m;
    root {nginx(ROOT / 'web/platform/dist')};
    index index.html;

    location /identity/ {{
        proxy_pass {identity};
{forwarding}    }}
    location ~ ^/(api(/|$)|auth(/|$)|mcp(/|$)|\\.well-known(/|$)|artifacts(/|$)|cli/(downloads|installers)(/|$)) {{
        proxy_pass {core};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection {upgrade_variable};
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_buffering off;
{forwarding}    }}
    location / {{
        try_files $uri $uri/ /index.html;
    }}
}}
'''
(state / 'nginx-miy.conf').write_text(nginx_text)
for service in ['serve', 'worker']:
    text = f'''# Generated file. run-service.sh reads private runtime.env and removes unused secrets.
[Unit]
Description=MIY {service} ({args.service_prefix})
After=network-online.target docker.service
Wants=network-online.target
Requires=docker.service

[Service]
Type=simple
User={args.user}
WorkingDirectory={systemd_directory(ROOT)}
Environment={systemd('MIY_STATE_DIR=' + str(state))}
Environment={systemd('MIY_BINARY=' + str(args.binary.resolve()))}
ExecStart=/usr/bin/bash {systemd(ROOT / 'scripts/run-service.sh', command=True)} {service}
Restart=on-failure
RestartSec=3
TimeoutStopSec=30
KillMode=control-group
UMask=0077

[Install]
WantedBy=multi-user.target
'''
    (state / f'{args.service_prefix}-{service}.service').write_text(text)
print('Nginx and systemd files rendered in the private state directory; no services were changed.')
