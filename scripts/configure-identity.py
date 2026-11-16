#!/usr/bin/env python3
"""Idempotently apply this validation realm's clients/scopes using Admin API."""
import argparse,ipaddress,json,os,ssl,urllib.request,urllib.parse
from pathlib import Path
from server_addresses import canonical_origin
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--previous-origin', help='Previous portal origin when renaming the MCP resource client')
args=parser.parse_args()
previous_origin=canonical_origin(args.previous_origin, True) if args.previous_origin else None
root=Path(__file__).resolve().parent.parent
local=Path(os.getenv('MIY_STATE_DIR',str(root/'.local'))).resolve()
secrets=json.loads((local/'credentials.json').read_text())
realm=json.loads((local/'realm.json').read_text())
settings=json.loads((local/'server.json').read_text())
# Reject stale generated clients before making an administrative request. Keep
# the realm, existing miy-web client and user subjects; only its exact approved
# redirect/logout origins expand to the three browser purposes. Web origins stay
# portal+console: the confidential client exchanges codes on the server.
origins=[settings['origin'],settings.get('consoleOrigin'),settings.get('devPortalOrigin')]
if not all(origins) or len(set(origins))!=3:
 raise SystemExit('Identity requires distinct portal, console and dev portal origins; rerun bootstrap')
web=next((client for client in realm['clients'] if client['clientId']=='miy-web'),None)
if (not web or web['redirectUris']!=[value+'/auth/callback' for value in origins]
    or web['webOrigins']!=origins[:2]
    or web['attributes']['post.logout.redirect.uris']!='##'.join(value+'/' for value in origins)):
 raise SystemExit('Generated identity client does not match the three exact configured origins')
# Administrative credentials only cross a loopback connection. The public
# issuer stays canonical even when the application's realm permits HTTP.
origin=settings['identityAdminUrl']
endpoint=urllib.parse.urlsplit(origin)
if endpoint.scheme not in ('http','https') or not endpoint.hostname or not ipaddress.ip_address(endpoint.hostname).is_loopback:
 raise SystemExit('Identity administration must use a loopback endpoint')
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,req,fp,code,msg,headers,newurl):return None
handlers=[urllib.request.ProxyHandler({}),NoRedirect()]
if endpoint.scheme=='https':
 handlers.append(urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=settings.get('caFile'))))
opener=urllib.request.build_opener(*handlers)
form=urllib.parse.urlencode({'client_id':'admin-cli','grant_type':'password','username':'bootstrap','password':secrets['identity_admin']}).encode()
token=json.load(opener.open(urllib.request.Request(origin+'/realms/master/protocol/openid-connect/token',data=form),timeout=15))['access_token']
def call(path,method='GET',body=None):
 r=urllib.request.Request(origin+'/admin/realms/miy'+path,method=method,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'},data=json.dumps(body).encode() if body is not None else None)
 with opener.open(r,timeout=15) as response:
  data=response.read();return json.loads(data) if data else None
existing_clients={x['clientId']:x for x in call('/clients')}
resource=settings['origin']+'/mcp'
if previous_origin and previous_origin!=settings['origin']:
 old_resource=previous_origin+'/mcp'
 old=existing_clients.get(old_resource)
 if old:
  if resource in existing_clients or old.get('publicClient') is not False or old.get('attributes',{}).get('resource_url')!=old_resource:
   raise SystemExit('MCP resource client conflict; no resource client was renamed')
  # The PUT below changes clientId in place instead of leaving an old audience
  # client behind. The new realm uses the same resource_client secret.
  existing_clients[resource]=old
 elif resource not in existing_clients:
  raise SystemExit('Previous MCP resource client was not found')

# Imports do not update existing realms. Replace a stale frontendUrl too, while
# retaining unrelated attributes and every realm/user/client subject.
current_realm=call('')
call('', 'PUT', {'sslRequired':realm['sslRequired'], 'attributes':{
 **current_realm.get('attributes',{}), 'frontendUrl':settings['consoleOrigin']+'/identity'}})
existing={x['name']:x for x in call('/client-scopes')}
for scope in realm['clientScopes']:
 old=existing.get(scope['name'])
 if old:call('/client-scopes/'+old['id'],'PUT',{**scope,'id':old['id']})
 else:call('/client-scopes','POST',scope)
scopes={x['name']:x['id'] for x in call('/client-scopes')}
def mappers(path,desired):
 old={x['name']:x for x in call(path+'/protocol-mappers/models')}
 for name,m in old.items():
  if name not in {x['name'] for x in desired}:call(path+'/protocol-mappers/models/'+m['id'],'DELETE')
 for m in desired:
  if m['name'] in old:
   mid=old[m['name']]['id'];call(path+'/protocol-mappers/models/'+mid,'PUT',{**m,'id':mid})
  else:call(path+'/protocol-mappers/models','POST',m)
for scope in realm['clientScopes']:mappers('/client-scopes/'+scopes[scope['name']],scope.get('protocolMappers',[]))
roles={r['name']:r for r in call('/roles')}
if 'offline_access' not in roles:call('/roles','POST',{'name':'offline_access'})
offline=call('/roles/offline_access')
call('/client-scopes/'+scopes['offline_access']+'/scope-mappings/realm','POST',[offline])
call('/users/00000000-0000-4000-8000-000000000002/role-mappings/realm','POST',[offline])
for client in realm['clients']:
 old=existing_clients.get(client['clientId'])
 if old:call('/clients/'+old['id'],'PUT',{**client,'id':old['id']})
 else:call('/clients','POST',client)
existing={x['clientId']:x for x in call('/clients')}
for client in realm['clients']:
 mappers('/clients/'+existing[client['clientId']]['id'],client.get('protocolMappers',[]))
 for kind,key in [('default','defaultClientScopes'),('optional','optionalClientScopes')]:
  for name in client[key]:call(f"/clients/{existing[client['clientId']]['id']}/{kind}-client-scopes/{scopes[name]}",'PUT')
print('Identity clients and scopes synchronized; existing realm and user subjects were preserved.')
