import json, os, pathlib, secrets, subprocess, time, hashlib

ROOT = pathlib.Path(os.environ['MIY_AUDIT_ROOT']).resolve()
EV = ROOT / 'evidence'
PREFIX = 'miy-audit-1024-' + secrets.token_hex(3)

def cmd(*args, input=None):
    return subprocess.check_output(args, input=input, stderr=subprocess.STDOUT).decode().strip()

def snapshot():
    rows=json.loads(cmd('docker','ps','--format','{{json .}}').replace('\n',',').join(['[',']']))
    ids=[r['ID'] for r in rows]
    return [{k:m[k] for k in ['Id','Name','Image','State']} for m in json.loads(cmd('docker','inspect',*ids))] if ids else []

baseline=snapshot()
(EV/'baseline-containers.json').write_text(json.dumps(baseline,indent=2))
password=secrets.token_hex(24)
db=PREFIX+'-db'
cfg={'prefix':PREFIX,'databaseContainer':db,'images':{},'network':PREFIX,'baselineIds':[x['Id'] for x in baseline]}
config=ROOT/'config.json'
config.write_text(json.dumps(cfg,indent=2))
cmd('docker','network','create','--label','miy.audit='+PREFIX,PREFIX)
cmd('docker','run','-d','--name',db,'--label','miy.audit='+PREFIX,'--network',PREFIX,'--memory','512m','--cpus','1','--pids-limit','128','-p','127.0.0.1::5432','-e','POSTGRES_PASSWORD='+password,'-e','POSTGRES_DB=miy_audit_core','--tmpfs','/var/lib/postgresql:rw,size=768m','postgres:18.3')
for _ in range(60):
    r=subprocess.run(['docker','exec',db,'pg_isready','-h','127.0.0.1','-U','postgres'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    if r.returncode==0: break
    time.sleep(.25)
else: raise RuntimeError('isolated postgres not ready')
meta=json.loads(cmd('docker','inspect',db))[0]
port=meta['NetworkSettings']['Ports']['5432/tcp'][0]['HostPort']
cfg['databasePort']=int(port)
cmd('docker','exec','-i',db,'psql','-U','postgres','-d','postgres','-v','ON_ERROR_STOP=1',input=b'REVOKE CONNECT ON DATABASE postgres FROM PUBLIC; REVOKE CONNECT ON DATABASE template1 FROM PUBLIC; REVOKE CONNECT ON DATABASE miy_audit_core FROM PUBLIC;')
subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(ROOT/'fixture-ca.key'),'-out',str(ROOT/'fixture-ca.pem'),'-days','1','-subj','/CN=Isolated runtime audit'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
env={'MIY_CA_FILE':str(ROOT/'fixture-ca.pem'),'DATABASE_URL':f'postgres://postgres:{password}@127.0.0.1:{port}/miy_audit_core','PROVISION_DATABASE_URL':f'postgres://postgres:{password}@127.0.0.1:{port}/postgres','MIY_DATABASE_CONTAINER':db,'MIY_ARTIFACTS':str(ROOT/'artifacts'),'MIY_ENCRYPTION_KEY':secrets.token_hex(32),'MIY_AUDIT_ROOT':str(ROOT)}
(ROOT/'private-env.json').write_text(json.dumps(env,indent=2));os.chmod(ROOT/'private-env.json',0o600)
app='audit-runtime'
commit='a'*40
server='''import http.server,json,os,pathlib
class H(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  unhealthy=pathlib.Path('/tmp/unhealthy').exists()
  self.send_response(503 if unhealthy and self.path=='/health' else 200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'releaseId':os.environ.get('MIY_RELEASE_ID'),'version':os.environ.get('AUDIT_VERSION'),'path':self.path}).encode())
 def log_message(self,*args): pass
http.server.HTTPServer(('0.0.0.0',8080),H).serve_forever()
'''
v1='CREATE TABLE app.records(id integer PRIMARY KEY, value text NOT NULL);\n'
v2='ALTER TABLE app.records ADD COLUMN tag text;\n'
bad='ALTER TABLE app.records ADD COLUMN failed_column text; SELECT audit_intentionally_missing_function();\n'
fix='ALTER TABLE app.records ADD COLUMN recovered text;\n'
variants=[('v1','1.0.0',[v1],app),('v1-conflict','1.0.0',[v1],app),('v2','1.1.0',[v1,v2],app),('bad','1.2.0',[v1,v2,bad],app),('recovery','1.2.1',[v1,v2,fix],app),('wrong-app','1.3.0',[v1], 'audit-other')]
for label,version,sqls,manifest_app in variants:
    path=ROOT/'images'/label
    (path/'miy/ui').mkdir(parents=True);(path/'miy/migrations').mkdir()
    manifest={'contractVersion':'miy.app/v1alpha1','id':manifest_app,'name':'Runtime audit probe','version':version,'runtime':{'profile':'rust-web-v1'},'ui':{'delivery':'module','moduleEntry':'mf-manifest.json','moduleExport':'./AppRoot','contractVersion':'miy.ui/v1alpha1'},'resources':{'database':'postgresql'},'migrations':{'directory':'migrations'},'requestedPermissions':['database']}
    (path/'miy/manifest.json').write_text(json.dumps(manifest))
    (path/'miy/build.json').write_text(json.dumps({'sourceCommit':commit,'appId':manifest_app,'version':version,'bundleVersion':'0.6.1'}))
    (path/'miy/ui/mf-manifest.json').write_text(json.dumps({'metaData':{'remoteEntry':{'type':'module','name':'entry.js'}},'exposes':[{'path':'./AppRoot'}]}))
    (path/'miy/ui/entry.js').write_text('export default {};// '+label)
    for i,sql in enumerate(sqls,1): (path/'miy/migrations'/f'{i:04d}_audit.sql').write_text(sql)
    (path/'server.py').write_text(server)
    (path/'Dockerfile').write_text(f'FROM python:3.14-slim\nLABEL org.miy.app.id="{manifest_app}" org.opencontainers.image.revision="{commit}" miy.audit="{PREFIX}"\nENV AUDIT_VERSION={version}\nCOPY miy /miy\nCOPY server.py /server.py\nCMD ["python","-B","/server.py"]\n')
    tag=PREFIX+':'+label
    with (EV/('build-'+label+'.log')).open('w') as log:
        subprocess.run(['docker','build','--pull=false','--network=none','-t',tag,str(path)],stdout=log,stderr=subprocess.STDOUT,check=True)
    cfg['images'][label]={'tag':tag,'id':cmd('docker','image','inspect',tag,'--format','{{.Id}}'),'version':version}
    config.write_text(json.dumps(cfg,indent=2))
print(json.dumps({'database':db,'port':port,'images':cfg['images']},indent=2))
