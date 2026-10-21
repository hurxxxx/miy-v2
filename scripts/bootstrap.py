#!/usr/bin/env python3
"""Generate local-only credentials once. Never print credentials."""
import json, os, secrets
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCAL = ROOT / '.local'
LOCAL.mkdir(mode=0o700, exist_ok=True)
os.umask(0o077)
secret_file = LOCAL / 'credentials.json'
if secret_file.exists():
    s = json.loads(secret_file.read_text())
else:
    s = {k: secrets.token_hex(24) for k in ['pg_admin', 'core', 'identity', 'identity_admin', 'web_client', 'resource_client', 'admin', 'developer', 'alice', 'bob']}
    s['encryption_key'] = secrets.token_hex(32)
    secret_file.write_text(json.dumps(s, indent=2))
origin = 'https://localhost:18443'
(LOCAL / 'compose.env').write_text('\n'.join([
    'MIY_PG_ADMIN_PASSWORD='+s['pg_admin'], 'MIY_IDENTITY_PASSWORD='+s['identity'],
    'MIY_IDENTITY_ADMIN_PASSWORD='+s['identity_admin'],
])+'\n')
(LOCAL / 'runtime.env').write_text('\n'.join([
    'DATABASE_URL=postgres://miy_core:'+s['core']+'@127.0.0.1:15432/miy_core',
    'PROVISION_DATABASE_URL=postgres://postgres:'+s['pg_admin']+'@127.0.0.1:15432/postgres',
    'MIY_ORIGIN='+origin, 'MIY_ISSUER='+origin+'/identity/realms/miy',
    'MIY_WEB_SECRET='+s['web_client'], 'MIY_RESOURCE_SECRET='+s['resource_client'],
    'MIY_ENCRYPTION_KEY='+s['encryption_key'],
    'MIY_CA_FILE='+str(LOCAL/'caddy/caddy/pki/authorities/local/root.crt'),
    'MIY_ARTIFACTS='+str(LOCAL/'artifacts'),
])+'\n')
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
realm = {'realm':'miy','enabled':True,'sslRequired':'external','registrationAllowed':False,'accessTokenLifespan':45,'ssoSessionIdleTimeout':1800,'revokeRefreshToken':True,'refreshTokenMaxReuse':0,
 'clients':clients,'clientScopes':[{'name':n,'protocol':'openid-connect','attributes':{'include.in.token.scope':'true'},'protocolMappers':[subject]} for n in ['miy.read','miy.dev','offline_access']],
 'roles':{'realm':[{'name':'offline_access','description':'Allow explicit developer OAuth refresh grants'}]},
 'users':[{'id':f'00000000-0000-4000-8000-{i:012d}','username':n,'enabled':True,'emailVerified':True,'firstName':n,'lastName':'Validation','email':n+'@miy.invalid','realmRoles':['offline_access'] if n=='developer' else [],'credentials':[{'type':'password','value':s[n],'temporary':False}]} for i,n in enumerate(['admin','developer','alice','bob'],1)]}
(LOCAL / 'realm.json').write_text(json.dumps(realm,indent=2))
(LOCAL/'artifacts').mkdir(exist_ok=True)
print('Local credentials and service configuration ready (.local/, private).')
