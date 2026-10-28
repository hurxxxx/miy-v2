#!/usr/bin/env python3
"""Generate private server configuration; reuse credentials on every invocation."""
import argparse, ipaddress, json, os, re, secrets, shlex, urllib.parse
from pathlib import Path


def canonical_origin(value, allow_http):
    parsed = urllib.parse.urlsplit(value)
    if (parsed.scheme not in ('http', 'https') or not parsed.hostname
            or parsed.username is not None or parsed.password is not None
            or parsed.path not in ('', '/') or parsed.query or parsed.fragment
            or any(c.isspace() or c == chr(92) for c in value)):
        raise ValueError('Use an explicit http:// or https:// origin without credentials or a path')
    hostname = parsed.hostname.encode('idna').decode('ascii').lower()
    try:
        address = ipaddress.ip_address(hostname)
        loopback = address.is_loopback
        hostname = '[' + address.compressed + ']' if address.version == 6 else str(address)
    except ValueError:
        if not re.fullmatch(r'[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?\.?', hostname):
            raise ValueError('Invalid server hostname') from None
        loopback = hostname == 'localhost'
    if parsed.scheme == 'http' and not loopback and not allow_http:
        raise ValueError('HTTP outside loopback requires MIY_ALLOW_HTTP=1 or --allow-http')
    port = parsed.port
    if port == 0:
        raise ValueError('Server port must be between 1 and 65535')
    suffix = '' if port in (None, 443 if parsed.scheme == 'https' else 80) else ':' + str(port)
    return parsed.scheme + '://' + hostname + suffix


def port(value):
    value = int(value)
    if not 1 <= value <= 65535:
        raise argparse.ArgumentTypeError('Port must be between 1 and 65535')
    return value


def private_write(path, value):
    path.write_text(value, encoding='utf-8')
    path.chmod(0o600)


def env_text(values):
    return ''.join(f'{name}={shlex.quote(str(value))}\n' for name, value in values.items())

ROOT = Path(__file__).resolve().parent.parent
initial = argparse.ArgumentParser(add_help=False)
initial.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
state_options, _ = initial.parse_known_args()
previous_file = state_options.state_dir.resolve() / 'server.json'
previous = json.loads(previous_file.read_text()) if previous_file.exists() else {}
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--profile', choices=['local', 'server'], default=os.getenv('MIY_SERVER_PROFILE', previous.get('profile', 'local')))
parser.add_argument('--origin', default=os.getenv('MIY_ORIGIN', previous.get('origin')))
parser.add_argument('--allow-http', action='store_true', default=os.getenv('MIY_ALLOW_HTTP', '1' if previous.get('allowHttp') else '0') == '1')
parser.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
parser.add_argument('--bind', default=os.getenv('MIY_BIND_ADDR', previous.get('bind', '0.0.0.0:18080')))
parser.add_argument('--postgres-port', type=port, default=port(os.getenv('MIY_POSTGRES_PORT', previous.get('postgresPort', 15432))))
parser.add_argument('--identity-port', type=port, default=port(os.getenv('MIY_IDENTITY_PORT', previous.get('identityPort', 18181))))
parser.add_argument('--http-port', type=port, default=port(os.getenv('MIY_HTTP_PORT', previous.get('httpPort', 18000))))
parser.add_argument('--https-port', type=port, default=port(os.getenv('MIY_HTTPS_PORT', previous.get('httpsPort', 18443))))
parser.add_argument('--proxy-bind', default=os.getenv('MIY_PROXY_BIND', previous.get('proxyBind', '127.0.0.1')))
parser.add_argument('--ca-file', type=Path, default=os.getenv('MIY_CA_FILE') or None)
args = parser.parse_args()
if args.profile == 'server' and not args.origin:
    parser.error('--profile server requires --origin URL or MIY_ORIGIN; no server address is assumed')
try:
    origin = canonical_origin(args.origin or f'https://localhost:{args.https_port}', args.allow_http)
    bind = urllib.parse.urlsplit('//' + args.bind)
    if not bind.hostname or bind.port is None or bind.path or bind.username is not None:
        raise ValueError('Use --bind HOST:PORT')
    port(bind.port)
    ipaddress.ip_address(args.proxy_bind)
except (ValueError, argparse.ArgumentTypeError) as error:
    parser.error(str(error))
LOCAL = args.state_dir.resolve()
LOCAL.mkdir(mode=0o700, parents=True, exist_ok=True)
LOCAL.chmod(0o700)
os.umask(0o077)
secret_file = LOCAL / 'credentials.json'
if secret_file.exists():
    s = json.loads(secret_file.read_text())
else:
    s = {k: secrets.token_hex(24) for k in ['pg_admin', 'core', 'identity', 'identity_admin', 'web_client', 'resource_client', 'admin', 'developer', 'alice', 'bob']}
    s['encryption_key'] = secrets.token_hex(32)
    secret_file.write_text(json.dumps(s, indent=2))
parsed = urllib.parse.urlsplit(origin)
local_tls = args.profile == 'local' and parsed.scheme == 'https'
previous_ca = previous.get('caFile') if origin == previous.get('origin') and parsed.scheme == 'https' else None
ca_file = args.ca_file.resolve() if args.ca_file else (Path(previous_ca) if previous_ca else (LOCAL / 'caddy/caddy/pki/authorities/local/root.crt' if local_tls else None))
development_origin = f'http://localhost:{args.http_port}' if args.profile == 'local' else origin
try:
    is_local = parsed.hostname == 'localhost' or ipaddress.ip_address(parsed.hostname).is_loopback
except ValueError:
    is_local = False
development_login = args.profile == 'local' and is_local
config = {
    'profile': args.profile, 'origin': origin, 'allowHttp': args.allow_http,
    'bind': args.bind, 'postgresPort': args.postgres_port, 'identityPort': args.identity_port,
    'httpPort': args.http_port, 'httpsPort': args.https_port, 'proxyBind': args.proxy_bind,
    'identityAdminUrl': f'http://127.0.0.1:{args.identity_port}/identity',
    'developmentOrigin': development_origin, 'developmentLogin': development_login,
    'caFile': str(ca_file) if ca_file else None,
    'proxyMode': 'local' if args.profile == 'local' else 'external',
    'cliReleaseDir': os.getenv('MIY_CLI_RELEASE_DIR', previous.get('cliReleaseDir', str(LOCAL / 'cli-downloads'))),
    'cliInstallerDir': os.getenv('MIY_CLI_INSTALLER_DIR', previous.get('cliInstallerDir', str(LOCAL / 'cli-installers'))),
}
private_write(LOCAL / 'server.json', json.dumps(config, indent=2) + '\n')
tls_host = '[' + parsed.hostname + ']' if ':' in parsed.hostname else parsed.hostname
private_write(LOCAL / 'compose.env', env_text({
    'MIY_PG_ADMIN_PASSWORD': s['pg_admin'], 'MIY_IDENTITY_PASSWORD': s['identity'],
    'MIY_IDENTITY_ADMIN_PASSWORD': s['identity_admin'],
    'MIY_ORIGIN': origin, 'MIY_STATE_DIR': LOCAL, 'MIY_POSTGRES_PORT': args.postgres_port,
    'MIY_IDENTITY_PORT': args.identity_port, 'MIY_HTTP_PORT': args.http_port,
    'MIY_HTTPS_PORT': args.https_port, 'MIY_PROXY_BIND': args.proxy_bind,
    'MIY_CORE_UPSTREAM': f'host.docker.internal:{bind.port}',
    'MIY_PROXY_TLS_SITE': f'https://{tls_host}:18443',
    'MIY_CADDY_CONFIG': ROOT / 'ops' / ('Caddyfile' if local_tls else 'Caddyfile.http'),
}))
runtime = {
    'DATABASE_URL': f"postgres://miy_core:{s['core']}@127.0.0.1:{args.postgres_port}/miy_core",
    'PROVISION_DATABASE_URL': f"postgres://postgres:{s['pg_admin']}@127.0.0.1:{args.postgres_port}/postgres",
    'MIY_ORIGIN': origin, 'MIY_ISSUER': origin + '/identity/realms/miy',
    'MIY_ALLOW_HTTP': '1' if args.allow_http else '0', 'MIY_BIND_ADDR': args.bind,
    'MIY_DEV_LOGIN': '1' if development_login else '0', 'MIY_DEV_ORIGIN': development_origin,
    'MIY_WEB_SECRET': s['web_client'], 'MIY_RESOURCE_SECRET': s['resource_client'],
    'MIY_ENCRYPTION_KEY': s['encryption_key'], 'MIY_ARTIFACTS': LOCAL / 'artifacts',
    'MIY_CLI_RELEASE_DIR': config['cliReleaseDir'],
    'MIY_CLI_INSTALLER_DIR': config['cliInstallerDir'],
}
if ca_file:
    runtime['MIY_CA_FILE'] = ca_file
private_write(LOCAL / 'runtime.env', env_text(runtime))
(LOCAL / 'init.sql').write_text(f"""
REVOKE CONNECT, TEMPORARY ON DATABASE postgres FROM PUBLIC;
REVOKE CONNECT, TEMPORARY ON DATABASE template1 FROM PUBLIC;
CREATE ROLE miy_core LOGIN PASSWORD '{s['core']}' NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE miy_identity LOGIN PASSWORD '{s['identity']}' NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE DATABASE miy_core OWNER miy_core;
CREATE DATABASE miy_identity OWNER miy_identity;
REVOKE ALL ON DATABASE miy_core FROM PUBLIC;
REVOKE ALL ON DATABASE miy_identity FROM PUBLIC;
GRANT CONNECT ON DATABASE miy_core TO miy_core;
GRANT CONNECT ON DATABASE miy_identity TO miy_identity;
""")
audience = {'name':'audience','protocol':'openid-connect','protocolMapper':'oidc-audience-mapper','config':{'included.custom.audience':origin+'/mcp','access.token.claim':'true','id.token.claim':'false'}}
subject = {'name':'subject','protocol':'openid-connect','protocolMapper':'oidc-sub-mapper','config':{'access.token.claim':'true','introspection.token.claim':'true'}}
clients = [
    {'clientId':'miy-web','secret':s['web_client'],'publicClient':False,'standardFlowEnabled':True,'redirectUris':[origin+'/auth/callback'],'webOrigins':[origin],'attributes':{'pkce.code.challenge.method':'S256','post.logout.redirect.uris':origin+'/'},'protocolMappers':[{'name':'web-audience','protocol':'openid-connect','protocolMapper':'oidc-audience-mapper','config':{'included.custom.audience':'miy-web','access.token.claim':'true','id.token.claim':'false'}}]},
    {'clientId':'miy-codex','publicClient':True,'standardFlowEnabled':True,'directAccessGrantsEnabled':False,'redirectUris':['http://127.0.0.1:18555/callback'],'attributes':{'pkce.code.challenge.method':'S256'},'protocolMappers':[audience]},
    {'clientId':origin+'/mcp','secret':s['resource_client'],'publicClient':False,'standardFlowEnabled':False,'serviceAccountsEnabled':False,'attributes':{'resource_url':origin+'/mcp'}},
]
for c in clients:
    c.update({'enabled':True,'protocol':'openid-connect','fullScopeAllowed':False,'defaultClientScopes':['miy.read'],'optionalClientScopes':['miy.dev','offline_access']})
realm = {'realm':'miy','enabled':True,'sslRequired':'none' if parsed.scheme == 'http' else 'external','registrationAllowed':False,'accessTokenLifespan':45,'ssoSessionIdleTimeout':1800,'revokeRefreshToken':True,'refreshTokenMaxReuse':0,
 'clients':clients,'clientScopes':[{'name':n,'protocol':'openid-connect','attributes':{'include.in.token.scope':'true'},'protocolMappers':[subject]} for n in ['miy.read','miy.dev','offline_access']],
 'roles':{'realm':[{'name':'offline_access','description':'Allow explicit developer OAuth refresh grants'}]},
 'users':[{'id':f'00000000-0000-4000-8000-{i:012d}','username':n,'enabled':True,'emailVerified':True,'firstName':n,'lastName':'Validation','email':n+'@miy.invalid','realmRoles':['offline_access'] if n=='developer' else [],'credentials':[{'type':'password','value':s[n],'temporary':False}]} for i,n in enumerate(['admin','developer','alice','bob'],1)]}
(LOCAL / 'realm.json').write_text(json.dumps(realm,indent=2))
(LOCAL / 'credentials.json').chmod(0o600)
# These individual files are read-only container mounts under other UIDs. Their
# host directory remains 0700; other host users cannot traverse to these files.
for filename in ['init.sql','realm.json']:
    (LOCAL / filename).chmod(0o644)
(LOCAL/'artifacts').mkdir(exist_ok=True)
print('Private server configuration ready. Existing credentials were preserved.')
