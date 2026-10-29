#!/usr/bin/env python3
"""Install the verified 0.6.3 server/UI/index set; restore previous set on failure."""
import hashlib,json,os,pathlib,shutil,subprocess,tempfile,time,urllib.request
RUN=pathlib.Path('/projects/miy-cli-experience-run-20261029')
SOURCE=pathlib.Path('/projects/miy-v2-cli-experience-20261029')
LIVE=pathlib.Path('/projects/miy-v2')
BACKUP=RUN/'private/rollback'
COMMIT='8d7d04271f9467d8bbdd16308e73715b33e4dc0c'
REPORT={'startedAt':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'commands':[],'rollbackAvailable':str(BACKUP),'rollbackUsed':False}
def save(): (RUN/'evidence/deployment.json').write_text(json.dumps(REPORT,indent=2)+'\n')
def call(args):
 p=subprocess.run(args,cwd=SOURCE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 REPORT['commands'].append({'command':args,'exitCode':p.returncode})
 # Only this script's fixed public verification/service commands are captured.
 (RUN/'evidence/deployment-commands.log').open('a').write(p.stdout)
 save()
 if p.returncode: raise RuntimeError('command_failed')
 return p.stdout.strip()
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def atomic_copy(source,destination):
 fd,name=tempfile.mkstemp(prefix=destination.name+'.cli063-',dir=destination.parent);os.close(fd)
 temporary=pathlib.Path(name)
 try:
  shutil.copy2(source,temporary)
  with temporary.open('rb') as f: os.fsync(f.fileno())
  os.replace(temporary,destination)
  fd=os.open(destination.parent,os.O_RDONLY|os.O_DIRECTORY)
  try: os.fsync(fd)
  finally: os.close(fd)
 finally: temporary.unlink(missing_ok=True)
def restore():
 REPORT['rollbackUsed']=True
 call(['sudo','-n','systemctl','stop','miy-v2-serve','miy-v2-worker'])
 for rel in ['.local/bin/miy-server','.local/cli-downloads/release-index.json','.local/cli-installers/installer-index.json']:
  atomic_copy(BACKUP/rel,LIVE/rel)
 for f in (BACKUP/'platform-dist').rglob('*'):
  if f.is_file():
   dest=LIVE/'web/platform/dist'/f.relative_to(BACKUP/'platform-dist');dest.parent.mkdir(parents=True,exist_ok=True);atomic_copy(f,dest)
 call(['sudo','-n','systemctl','start','miy-v2-serve','miy-v2-worker'])
 REPORT['status']='ROLLED_BACK';save()
cli=RUN/'staging/cli-downloads';installer=RUN/'staging/cli-installers'
ci=json.loads((cli/'release-index.json').read_text());wi=json.loads((installer/'installer-index.json').read_text())
win=next(a for a in ci['assets'] if a['target']=='x86_64-pc-windows-msvc')
assert ci['version']==wi['version']=='0.6.3' and ci['commit']==wi['cliCommit']==wi['installerCommit']==COMMIT
assert win['sha256']==wi['archiveSha256'] and win['binarySha256']==wi['cliBinarySha256']
manifest=json.loads((RUN/'evidence/rollback-manifest.json').read_text())
for rel,hashvalue in manifest['files'].items(): assert digest(LIVE/rel)==hashvalue, 'live_file_changed_since_backup'
binary=RUN/'artifacts/miy-0.6.3-linux-server'
assert digest(binary)=='4573260257147c2563ba5ffae177b9ac6c6b0a331d2a0b1fef1cf6e60d10064e'
assert call([str(binary),'--version'])=='miy 0.6.3'
REPORT['serverSha256']=digest(binary);REPORT['cliCommit']=COMMIT
REPORT['uiFiles']={str(f.relative_to(SOURCE/'web/platform/dist')):digest(f) for f in (SOURCE/'web/platform/dist').rglob('*') if f.is_file()}
save()
try:
 call(['sudo','-n','systemctl','stop','miy-v2-serve','miy-v2-worker'])
 call(['python3','scripts/stage-cli-release.py','--source',str(cli/COMMIT),'--destination',str(LIVE/'.local/cli-downloads'),'--commit',COMMIT])
 call(['python3','scripts/stage-windows-installer.py','--source',str(installer/COMMIT),'--destination',str(LIVE/'.local/cli-installers'),'--installer-commit',COMMIT,'--cli-commit',COMMIT])
 atomic_copy(binary,LIVE/'.local/bin/miy-server')
 dist=SOURCE/'web/platform/dist'
 files=sorted((f for f in dist.rglob('*') if f.is_file()),key=lambda f:f.name=='index.html')
 for f in files:
  dest=LIVE/'web/platform/dist'/f.relative_to(dist);dest.parent.mkdir(parents=True,exist_ok=True);atomic_copy(f,dest)
 call(['sudo','-n','systemctl','start','miy-v2-serve','miy-v2-worker'])
 for _ in range(40):
  try:
   with urllib.request.urlopen('https://v2.dwdcc.kr/api/health',timeout=3) as r: health=json.load(r)
   with urllib.request.urlopen('https://v2.dwdcc.kr/api/v1/cli/install',timeout=3) as r: guide=json.load(r)
   assert health=={'status':'ok'} and guide['cliVersion']=='0.6.3' and guide['sourceCommit']==COMMIT
   assert guide['windowsInstaller']['sha256']==wi['sha256']
   REPORT['health']=health;REPORT['installGuide']=guide;break
  except Exception: time.sleep(1)
 else: raise RuntimeError('post_deployment_health_failed')
 call(['systemctl','is-active','miy-v2-serve','miy-v2-worker'])
 REPORT['status']='PASS'
except Exception as e:
 REPORT['failureCategory']=str(e) if str(e) in ['command_failed','post_deployment_health_failed'] else type(e).__name__;save();restore();raise
finally:
 REPORT['finishedAt']=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime());save()
print(json.dumps({'status':REPORT['status'],'version':'0.6.3','serverSha256':REPORT['serverSha256'],'rollbackUsed':REPORT['rollbackUsed']}))
