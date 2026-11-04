#!/usr/bin/env python3
import argparse, hashlib, http.server, importlib.util, json, os, pathlib, secrets, shutil, socket, subprocess, sys, tempfile, threading, time, urllib.error, urllib.parse, urllib.request, uuid
sys.dont_write_bytecode=True
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=pathlib.Path,required=True);parser.add_argument('--source-label',default='working-fix');args=parser.parse_args()
ROOT=pathlib.Path('/projects/miy-v2'); WORK=pathlib.Path(tempfile.mkdtemp(prefix='miy-delegated-disabled-fixed-review-')); WORK.chmod(0o700)
BINARY_SOURCE=args.binary
LABEL='miy.delegated-disabled-review'; OWNER=secrets.token_hex(16)
REPORT={'sourceLabel':args.source_label,'evidenceDirectory':str(WORK),'workerStarted':False,'appContainersCreated':False,'scope':'Independent fixed-source HTTP verification of disabled delegated runtime submission and current-authority job visibility','observations':{},'status':'RUNNING'}
container=None; core=None; identity=None;stage='copy_pinned_binary'
class Failure(Exception):pass
def run(args,*,env=None,input=None,timeout=90):
    p=subprocess.run(args,cwd=WORK,env=env,input=input,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,timeout=timeout)
    if p.returncode:raise Failure('command_failed')
    return p.stdout.strip()
def sql(query):return run(['docker','exec','-i',container,'psql','-X','-qAt','-v','ON_ERROR_STOP=1','-U','postgres','-d','miy_review'],input=query)
def freeport():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def request(token,method,path,value=None):
    headers={'Authorization':'Bearer '+token}
    if value is not None:headers['Content-Type']='application/json'
    req=urllib.request.Request(origin+path,data=None if value is None else json.dumps(value).encode(),headers=headers,method=method)
    try:
        with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
def safe(status,body):return {'httpStatus':status,'errorCode':body.get('error',{}).get('code')}
def require(ok,code):
    if not ok:raise Failure(code)
try:
    binary=WORK/'miy';shutil.copyfile(BINARY_SOURCE,binary);binary.chmod(0o700)
    with binary.open('rb') as f:REPORT['binarySha256']=hashlib.file_digest(f,'sha256').hexdigest()
    stage='start_owned_postgres'
    password=secrets.token_hex(24)
    container=run(['docker','run','--pull=never','-d','--label',LABEL+'='+OWNER,'--memory','512m','--cpus','1','--pids-limit','128','--tmpfs','/var/lib/postgresql:rw,size=512m','-e','POSTGRES_PASSWORD','-e','POSTGRES_DB=miy_review','-p','127.0.0.1::5432','postgres:18.3'],env={**os.environ,'POSTGRES_PASSWORD':password})
    REPORT['ownedContainerId']=container
    for _ in range(120):
        if subprocess.run(['docker','exec',container,'pg_isready','-U','postgres','-h','127.0.0.1'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:break
        time.sleep(.25)
    else:raise Failure('postgres_not_ready')
    dbport=int(run(['docker','port',container,'5432/tcp']).rsplit(':',1)[1]);apiport=freeport();issuerport=freeport()
    origin='http://127.0.0.1:'+str(apiport);issuer='http://127.0.0.1:'+str(issuerport)+'/identity/realms/miy'
    class Discovery(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            d={'issuer':issuer,'authorization_endpoint':issuer+'/protocol/openid-connect/auth','token_endpoint':issuer+'/protocol/openid-connect/token','introspection_endpoint':issuer+'/protocol/openid-connect/token/introspect','jwks_uri':issuer+'/protocol/openid-connect/certs','end_session_endpoint':issuer+'/protocol/openid-connect/logout'} if self.path.endswith('/.well-known/openid-configuration') else {'keys':[]}
            body=json.dumps(d).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def log_message(self,*a):pass
    identity=http.server.ThreadingHTTPServer(('127.0.0.1',issuerport),Discovery);threading.Thread(target=identity.serve_forever,daemon=True).start()
    env={k:v for k,v in os.environ.items() if not k.startswith(('MIY_','PG')) and k not in ('DATABASE_URL','PROVISION_DATABASE_URL','RUST_LOG')}
    (WORK/'artifacts').mkdir()
    env.update(DATABASE_URL=f'postgres://postgres:{password}@127.0.0.1:{dbport}/miy_review',PROVISION_DATABASE_URL=f'postgres://postgres:{password}@127.0.0.1:{dbport}/postgres',MIY_DATABASE_CONTAINER=container,MIY_STATE_DIR=str(WORK/'state'),MIY_ORIGIN=origin,MIY_ISSUER=issuer,MIY_ALLOW_HTTP='1',MIY_DEV_LOGIN='1',MIY_DEV_ORIGIN=origin,MIY_BIND_ADDR='127.0.0.1:'+str(apiport),MIY_WEB_SECRET=secrets.token_hex(32),MIY_RESOURCE_SECRET=secrets.token_hex(32),MIY_ENCRYPTION_KEY=secrets.token_hex(32),MIY_ARTIFACTS=str(WORK/'artifacts'),RUST_LOG='error')
    stage='migrate_owned_database';run([str(binary),'migrate'],env=env)
    stage='synthetic_fixture'
    app='disabled-review-'+secrets.token_hex(4);release=str(uuid.uuid4());installation=str(uuid.uuid4())
    admin='review-admin-'+str(uuid.uuid4());ordinary='review-user-'+str(uuid.uuid4())
    credentials={}
    for actor,is_admin in [(admin,True),(ordinary,False)]:
        token='miy_access_'+secrets.token_urlsafe(32);session=str(uuid.uuid4());credentials[actor]=token
        sql(f"INSERT INTO users(id,name,admin) VALUES('{actor}','Isolated reviewer',{str(is_admin).lower()}); INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES('{session}','{actor}','{uuid.uuid4()}','Isolated review'); INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES('{hashlib.sha256(token.encode()).hexdigest()}','{session}',now()+interval '10 minutes');")
    manifest={'contractVersion':'miy.app/v1alpha1','id':app,'name':'Isolated runtime review','version':'1.0.0','runtime':{'profile':'rust-web-v1'},'ui':{'delivery':'module','moduleEntry':'mf-manifest.json','moduleExport':'./AppRoot','contractVersion':'miy.ui/v1alpha1'},'resources':{'database':'postgresql'},'migrations':{'directory':'migrations'},'requestedPermissions':[]}
    sql(f"INSERT INTO apps(id,name) VALUES('{app}','Isolated runtime review'); INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES('{release}','{app}','1.0.0','{'a'*64}','{json.dumps(manifest)}'::jsonb); INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_image) VALUES('{installation}','{app}','prod','{release}','ready','running','running','sha256:{'a'*64}'); INSERT INTO grants(user_id,app_id,environment,permission) VALUES('{ordinary}','{app}','prod','use');")
    stage='start_owned_http_core'
    core=subprocess.Popen([str(binary),'serve'],cwd=WORK,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    for _ in range(100):
        require(core.poll() is None,'core_exited')
        try:
            with urllib.request.urlopen(origin+'/api/health',timeout=.3) as r:require(r.status==200,'health_failed')
            break
        except (urllib.error.URLError,TimeoutError):time.sleep(.1)
    else:raise Failure('core_not_ready')
    stage='create_real_http_delegation'
    status,issued=request(credentials[admin],'POST','/api/v1/delegations',{'name':'Isolated stop agent','appId':app,'environment':'prod','actions':['read','stop'],'expiresIn':3600})
    REPORT['observations']['delegationCreation']={'httpStatus':status,'actions':['read','stop'],'environment':'prod'}
    require(status==200,'delegation_creation_failed');delegated=issued['credential']['accessToken']
    negative_delegates={}
    for label,environment,actions in [('missingRead','prod',['stop']),('wrongEnvironment','dev',['read','stop'])]:
        status,issued=request(credentials[admin],'POST','/api/v1/delegations',{'name':'Isolated negative '+label,'appId':app,'environment':environment,'actions':actions,'expiresIn':3600})
        require(status==200,'negative_delegation_creation_failed');negative_delegates[label]=issued['credential']['accessToken']
    status,body=request(credentials[admin],'PUT','/api/v1/apps/'+app+'/control',{'disabled':True});require(status==200,'disable_failed')
    stage='delegated_preflight_and_submission'
    status,preflight=request(delegated,'POST','/api/v1/apps/'+app+'/runtime/preflight',{'target':{'appId':app,'environment':'prod'},'action':'stop'})
    REPORT['observations']['delegatedPreflight']={'httpStatus':status,'canSubmit':preflight.get('canSubmit')}
    require(status==200 and preflight.get('canSubmit') is True,'preflight_failed')
    status,denied_plan=request(negative_delegates['missingRead'],'POST','/api/v1/apps/'+app+'/runtime/preflight',{'target':{'appId':app,'environment':'prod'},'action':'stop'})
    REPORT['observations']['missingReadPreflight']=safe(status,denied_plan)
    denied_key='no-read-'+uuid.uuid4().hex
    status,denied_submit=request(negative_delegates['missingRead'],'POST','/api/v1/apps/'+app+'/runtime',{'action':'stop','environment':'prod','idempotencyKey':denied_key,'confirmed':True,'expectedState':preflight['expectedState']})
    REPORT['observations']['missingReadSubmit']=safe(status,denied_submit)
    REPORT['observations']['missingReadQueuedRows']=int(sql(f"SELECT count(*) FROM runtime_jobs WHERE actor='{admin}' AND idempotency_key='{denied_key}'"))
    key='isolated-'+uuid.uuid4().hex
    body={'action':'stop','environment':'prod','idempotencyKey':key,'confirmed':True,'expectedState':preflight['expectedState']}
    status,result=request(delegated,'POST','/api/v1/apps/'+app+'/runtime',body)
    REPORT['observations']['delegatedSubmit']=safe(status,result)
    queued=json.loads(sql(f"SELECT json_build_object('count',count(*),'queuedCount',count(*)FILTER(WHERE status='queued'),'delegatedCount',count(*)FILTER(WHERE delegation_id IS NOT NULL)) FROM runtime_jobs WHERE actor='{admin}' AND app_id='{app}' AND idempotency_key='{key}'"))
    REPORT['observations']['committedJobRows']=queued
    require(queued['count']==1,'expected_job_not_found')
    job=sql(f"SELECT id FROM runtime_jobs WHERE actor='{admin}' AND idempotency_key='{key}'")
    stage='read_job_and_events'
    for label,token,path in [('delegatedJob',delegated,'/api/v1/runtime/jobs/'+job),('delegatedEvents',delegated,'/api/v1/runtime/jobs/'+job+'/events'),('adminJob',credentials[admin],'/api/v1/runtime/jobs/'+job),('useOnlyJob',credentials[ordinary],'/api/v1/runtime/jobs/'+job),('useOnlyEvents',credentials[ordinary],'/api/v1/runtime/jobs/'+job+'/events')]:
        status,value=request(token,'GET',path);REPORT['observations'][label]=safe(status,value)
    for label,token in [('useOnlyJobList',credentials[ordinary]),('delegatedJobList',delegated)]:
        status,value=request(token,'GET','/api/v1/runtime/jobs?appId='+app+'&environment=prod&limit=200')
        REPORT['observations'][label]={'httpStatus':status,'matchingQueuedJobCount':sum(r.get('id')==job for r in value.get('jobs',[]))}
    for label,token in negative_delegates.items():
        for suffix,tail in [('Job',''),('Events','/events')]:
            status,value=request(token,'GET','/api/v1/runtime/jobs/'+job+tail)
            REPORT['observations'][label+suffix]=safe(status,value)
    stage='same_idempotency_retry'
    status,value=request(delegated,'POST','/api/v1/apps/'+app+'/runtime',body);REPORT['observations']['delegatedIdenticalRetry']=safe(status,value)
    REPORT['observations']['rowsAfterRetry']=int(sql(f"SELECT count(*) FROM runtime_jobs WHERE actor='{admin}' AND idempotency_key='{key}'"))
    stage='reenable_differential'
    status,value=request(credentials[admin],'PUT','/api/v1/apps/'+app+'/control',{'disabled':False});require(status==200,'reenable_failed')
    status,value=request(delegated,'GET','/api/v1/runtime/jobs/'+job);REPORT['observations']['delegatedJobAfterReenable']=safe(status,value)
    # A cursor obtained while enabled must not restore access after a later disable.
    extra=str(uuid.uuid4())
    sql(f"INSERT INTO runtime_jobs(id,app_id,environment,actor,action,idempotency_key,input_hash,status,step) VALUES('{extra}','{app}','prod','{admin}','stop','cursor-review-{extra}','fixture','succeeded','done')")
    status,page=request(credentials[ordinary],'GET','/api/v1/runtime/jobs?appId='+app+'&environment=prod&limit=1')
    require(status==200 and page.get('nextCursor'),'enabled_cursor_missing')
    cursor=page['nextCursor']
    status,value=request(credentials[admin],'PUT','/api/v1/apps/'+app+'/control',{'disabled':True});require(status==200,'redisable_failed')
    status,page=request(credentials[ordinary],'GET','/api/v1/runtime/jobs?appId='+app+'&environment=prod&limit=1&cursor='+urllib.parse.quote(cursor,safe=''))
    REPORT['observations']['useOnlyExistingCursorAfterDisable']={'httpStatus':status,'jobCount':len(page.get('jobs',[]))}
    o=REPORT['observations']
    checks={
      'delegatedSubmitAndRead':o['delegatedSubmit']['httpStatus']==200 and all(o[k]['httpStatus']==200 for k in ['delegatedJob','delegatedEvents','adminJob']),
      'singleCommittedJob':queued=={'count':1,'queuedCount':1,'delegatedCount':1},
      'useOnlyReadBlocked':all(o[k]=={'httpStatus':403,'errorCode':'app_disabled'} for k in ['useOnlyJob','useOnlyEvents']),
      'useOnlyListFiltered':o['useOnlyJobList']=={'httpStatus':200,'matchingQueuedJobCount':0},
      'delegatedListVisible':o['delegatedJobList']=={'httpStatus':200,'matchingQueuedJobCount':1},
      'missingReadDeniedBeforeCommit':o['missingReadPreflight']['httpStatus']==403 and o['missingReadSubmit']['httpStatus']==403 and o['missingReadQueuedRows']==0,
      'readAndEnvironmentScopeEnforced':all(o[label+suffix]['httpStatus']==403 for label in negative_delegates for suffix in ['Job','Events']),
      'idempotentRetry':o['delegatedIdenticalRetry']['httpStatus']==200 and o['rowsAfterRetry']==1,
      'reenableRead':o['delegatedJobAfterReenable']['httpStatus']==200,
      'cursorRechecksDisable':o['useOnlyExistingCursorAfterDisable']=={'httpStatus':200,'jobCount':0},
    }
    REPORT['checks']=checks
    REPORT['status']='PASS' if all(checks.values()) else 'FAIL'

except BaseException as e:
    REPORT.update(status='HARNESS_FAILED',failedStage=stage,failureType=type(e).__name__)
    if isinstance(e,Failure):REPORT['safeFailure']=str(e)
finally:
    if core and core.poll() is None:
        core.terminate()
        try:core.wait(timeout=5)
        except subprocess.TimeoutExpired:core.kill();core.wait(timeout=5)
    if identity:identity.shutdown();identity.server_close()
    REPORT['coreExited']=core is None or core.poll() is not None
    if container:
        spec=importlib.util.spec_from_file_location('review_cleanup',ROOT/'scripts/core-backup.py');cleanup=importlib.util.module_from_spec(spec);spec.loader.exec_module(cleanup)
        REPORT['cleanup']=cleanup.cleanup_container(container,LABEL,OWNER)
    result=WORK/'result.json';result.write_text(json.dumps(REPORT,indent=2)+'\n');result.chmod(0o600)
    print(json.dumps(REPORT,indent=2))

sys.exit(0 if REPORT['status']=='PASS' else 1)
