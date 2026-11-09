#!/usr/bin/env python3
"""Render host Nginx/systemd files without installing or restarting any service."""
import argparse
import json
import os
from pathlib import Path
import pwd
import re
import urllib.parse
from server_tls import validate_certificate

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
parser.add_argument('--ca-file', type=Path, help='Trusted private CA PEM; otherwise use configured/system roots')
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
if not all(settings.get(key) for key in ('consoleOrigin', 'consoleBind', 'devPortalOrigin', 'devPortalBind',
                                         'appFrameOrigin', 'appFrameBind')):
    parser.error('Run bootstrap with distinct portal/console/dev portal origins, app frame origins and internal binds first')
# Business app frames (docs/APP-FRAMES.md): a {key} host pattern (domain) or a
# {port} pool on one host; Core accepts only origins registered to an installation.
frame_template = settings['appFrameOrigin']
frame_scheme, _, frame_rest = frame_template.partition('://')
frame_ports = None
if '{port}' in frame_template:
    first, _, last = (settings.get('appFramePorts') or '').partition('-')
    frame_ports = (int(first), int(last))
    frame_sample = frame_template.replace('{port}', first)
    frame_sources = ' '.join(frame_template.replace('{port}', str(value))
                             for value in range(frame_ports[0], frame_ports[1] + 1))
else:
    frame_sample = frame_template.replace('{key}', '0' * 32)
    frame_sources = f"{frame_scheme}://*.{frame_rest.split('.', 1)[-1]}"
if frame_scheme != origin.scheme or frame_sample in [settings['origin'], settings['consoleOrigin'], settings['devPortalOrigin']]:
    parser.error('App frame origins must use the portal transport and never a platform origin')
console_origin = urllib.parse.urlsplit(settings['consoleOrigin'])
dev_portal_origin = urllib.parse.urlsplit(settings['devPortalOrigin'])
browser_origins = [settings['origin'], settings['consoleOrigin'], settings['devPortalOrigin']]
if len(set(browser_origins)) != 3 or {console_origin.scheme, dev_portal_origin.scheme} != {origin.scheme}:
    parser.error('Portal, console and dev portal origins must be distinct and use the same transport')
if origin.scheme == 'https' and (args.certificate is None or args.key is None):
    parser.error('HTTPS requires --certificate PATH and --key PATH')
if origin.scheme not in ('http', 'https'):
    parser.error('Unsupported configured origin')
if origin.scheme == 'http' and not settings.get('allowHttp'):
    import ipaddress
    def loopback(public):
        if public.hostname == 'localhost':
            return True
        try:
            return ipaddress.ip_address(public.hostname).is_loopback
        except ValueError:
            return False
    if not all(loopback(value) for value in [origin, console_origin, dev_portal_origin]):
        parser.error('The non-loopback HTTP profile must explicitly allow HTTP')
if origin.scheme == 'https':
    try:
        validate_certificate(browser_origins + [frame_sample], args.certificate,
                             args.key, args.ca_file or settings.get('caFile'))
    except (ValueError, OSError) as error:
        parser.error(str(error))
core = 'http://' + settings['bind']
console_core = 'http://' + settings['consoleBind']
dev_portal_core = 'http://' + settings['devPortalBind']
frame_core = 'http://' + settings['appFrameBind']
identity = f"http://127.0.0.1:{settings['identityPort']}"
upgrade_variable = '$' + args.service_prefix.replace('-', '_') + '_connection_upgrade'
tls = ''
if origin.scheme == 'https':
    tls = f'''    ssl_certificate {nginx(args.certificate.resolve())};
    ssl_certificate_key {nginx(args.key.resolve())};
    ssl_protocols TLSv1.2 TLSv1.3;
'''


def forwarding(public):
    return f'''        proxy_set_header Host {nginx(public.netloc)};
        proxy_set_header X-Forwarded-Host {nginx(public.netloc)};
        proxy_set_header X-Forwarded-Proto {public.scheme};
        proxy_set_header X-Forwarded-Port {public.port or (443 if public.scheme == 'https' else 80)};
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header Forwarded "";
'''


def security_headers(indent='    ', console=False, portal=False):
    policy = ("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; "
              "img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; "
              "base-uri 'none'; frame-ancestors 'none'; worker-src 'none'; form-action 'self'"
              if console else "frame-ancestors 'none'; worker-src 'none'")
    extra = ''
    if portal:
        # Portals frame only app frame origins. The launch form POST carries the
        # portal Origin only under this referrer policy (not no-referrer).
        policy += f'; frame-src {frame_sources}'
        extra = f'{indent}add_header Referrer-Policy strict-origin-when-cross-origin always;\n'
    return (f'{indent}add_header X-Content-Type-Options nosniff always;\n'
            f'{indent}add_header X-Frame-Options DENY always;\n'
            f'{indent}add_header Content-Security-Policy "{policy}" always;\n' + extra)


def server(public, trusted=False, dev_portal=False):
    public_port = public.port or (443 if public.scheme == 'https' else 80)
    public_host = '[' + public.hostname + ']' if ':' in public.hostname else public.hostname
    public_listen = f'[::]:{public_port}' if ':' in public.hostname else str(public_port)
    upstream = console_core if trusted else dev_portal_core if dev_portal else core
    if dev_portal:
        # Dev app code runs here. Browser sign-in and dev app use only: no identity
        # pages, CLI approval, MCP or OAuth metadata (CLI and MCP use the portal).
        boundaries = '''    if ($http_service_worker = "script") { return 403; }
    location /identity { return 404; }
    location ~ ^/(cli/(authorize|device)|auth/cli|mcp|\\.well-known)(/|$) { return 404; }
'''
    elif trusted:
        boundaries = f'''    # No app content, even through direct URLs or SPA fallback.
    if ($http_service_worker = "script") {{ return 403; }}
    location ~ ^/(artifacts(/|$)|preview(/|$)|((environments/[^/]+/)?apps)(/|$)|api/v1/(preview(/|$)|installations/[^/]+/(api(/|$)|connections/[^/]+/api(/|$)))) {{
        return 404;
    }}
    # Keep Keycloak's own script CSP and add the anti-frame policy.
    location ~ ^/identity/(realms/miy(/|$)|resources/) {{
        proxy_pass {identity};
{forwarding(public)}    }}
    location /identity {{ return 404; }}
'''
    else:
        boundaries = f'''    if ($http_service_worker = "script") {{ return 403; }}
    location /identity {{ return 404; }}
    # Preserve the relative CLI verificationPath via a fixed trusted origin.
    # Pass only the public UUID, never a query string or a return URL.
    location ~ "^/cli/(authorize|device)/([0-9a-fA-F]{{8}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{12}})$" {{
        return 302 {settings['consoleOrigin']}/cli/$1/$2;
    }}
    location ~ ^/cli/(authorize|device)(/|$) {{ return 404; }}
'''
    return f'''server {{
    listen {public_listen}{' ssl' if tls else ''};
    server_name {nginx(public_host)};
{tls}    client_max_body_size 1024m;
    root {nginx(ROOT / 'web/platform/dist')};
    index index.html;
    proxy_hide_header X-Content-Type-Options;
    proxy_hide_header X-Frame-Options;
{security_headers()}
{boundaries}    location ~ ^/(api(/|$)|auth(/|$)|mcp(/|$)|\\.well-known(/|$)|artifacts(/|$)|cli/(downloads|installers)(/|$)) {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection {upgrade_variable};
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_buffering off;
{forwarding(public)}    }}
    location / {{
{security_headers('        ', console=trusted, portal=not trusted)}        try_files $uri $uri/ /index.html;
    }}
}}
'''


def frame_server():
    """All app frame origins: one wildcard/regex host or one port-range listener."""
    if frame_ports:
        host = urllib.parse.urlsplit(frame_sample).hostname
        listen = f"{'[::]:' if ':' in host else ''}{frame_ports[0]}-{frame_ports[1]}"
        name = '[' + host + ']' if ':' in host else host
    else:
        public = urllib.parse.urlsplit(frame_sample)
        listen = str(public.port or (443 if public.scheme == 'https' else 80))
        prefix, suffix = frame_rest.split('{key}', 1)
        name = '~^' + re.escape(prefix) + '[0-9a-f]{32}' + re.escape(suffix) + '$'
    return f'''server {{
    listen {listen}{' ssl' if tls else ''};
    server_name {nginx(name)};
{tls}    client_max_body_size 8m;
    # Asset paths carry a frame capability: never write them to access logs.
    access_log off;
    # No anti-frame headers: frame documents name their portal in frame-ancestors.
    if ($http_service_worker = "script") {{ return 403; }}
    location / {{
        proxy_pass {frame_core};
        proxy_http_version 1.1;
        proxy_read_timeout 60s;
        proxy_buffering off;
        proxy_set_header Host $http_host;
        proxy_set_header X-Forwarded-Proto {frame_scheme};
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Host "";
        proxy_set_header Forwarded "";
    }}
}}
'''


nginx_text = f'''# Generated from fixed portal/console/dev-portal settings. One listener per browser purpose.
map $http_upgrade {upgrade_variable} {{
    default upgrade;
    '' close;
}}
''' + server(origin) + server(console_origin, trusted=True) + server(dev_portal_origin, dev_portal=True) + frame_server()
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
