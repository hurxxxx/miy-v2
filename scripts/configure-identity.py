#!/usr/bin/env python3
"""Idempotently apply this validation realm's clients/scopes using Admin API."""
import ipaddress,json,os,ssl,urllib.request,urllib.parse
from pathlib import Path
root=Path(__file__).resolve().parent.parent
local=Path(os.getenv('MIY_STATE_DIR',str(root/'.local'))).resolve()
secrets=json.loads((local/'credentials.json').read_text())
realm=json.loads((local/'realm.json').read_text())
settings=json.loads((local/'server.json').read_text())
# Administrative credentials only cross a loopback connection. The public
# issuer stays canonical even when the application's realm permits HTTP.
origin=settings['identityAdminUrl']
endpoint=urllib.parse.urlsplit(origin)
if endpoint.scheme not in ('http','https') or not endpoint.hostname or not ipaddress.ip_address(endpoint.hostname).is_loopback:
 raise SystemExit('Identity administration must use a loopback endpoint')
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,req,fp,code,msg,headers,newurl):return None
handlers=[NoRedirect()]
if endpoint.scheme=='https':
 handlers.append(urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=settings.get('caFile'))))
opener=urllib.request.build_opener(*handlers)
form=urllib.parse.urlencode({'client_id':'admin-cli','grant_type':'password','username':'bootstrap','password':secrets['identity_admin']}).encode()
token=json.load(opener.open(urllib.request.Request(origin+'/realms/master/protocol/openid-connect/token',data=form),timeout=15))['access_token']
def call(path,method='GET',body=None):
 r=urllib.request.Request(origin+'/admin/realms/miy'+path,method=method,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'},data=json.dumps(body).encode() if body is not None else None)
 with opener.open(r,timeout=15) as response:
  data=response.read();return json.loads(data) if data else None
# Realm imports do not update existing realms. Apply the selected HTTP policy
# explicitly on every configuration run without changing the master realm.
call('', 'PUT', {'sslRequired':realm['sslRequired']})
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
existing={x['clientId']:x for x in call('/clients')}
for client in realm['clients']:
 old=existing.get(client['clientId'])
 if old:call('/clients/'+old['id'],'PUT',{**client,'id':old['id']})
 else:call('/clients','POST',client)
existing={x['clientId']:x for x in call('/clients')}
for client in realm['clients']:
 mappers('/clients/'+existing[client['clientId']]['id'],client.get('protocolMappers',[]))
 for kind,key in [('default','defaultClientScopes'),('optional','optionalClientScopes')]:
  for name in client[key]:call(f"/clients/{existing[client['clientId']]['id']}/{kind}-client-scopes/{scopes[name]}",'PUT')
print('Validation identity clients and scopes synchronized.')
