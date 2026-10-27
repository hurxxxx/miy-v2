#!/usr/bin/env python3
"""Manage only native Rust processes belonging to this checkout."""
import os,shlex,signal,subprocess,sys,time
from pathlib import Path
root=Path(__file__).resolve().parent.parent
local=Path(os.getenv('MIY_STATE_DIR',str(root/'.local'))).resolve()
binary=Path(os.getenv('MIY_BINARY',str(root/'target/debug/miy'))).resolve()
action=sys.argv[1]
names=sys.argv[2:] or ['serve','worker']
env=os.environ.copy()
for line in (local/'runtime.env').read_text().splitlines():
 if line and not line.startswith('#'):
  k,v=line.split('=',1);env[k]=' '.join(shlex.split(v))
env.setdefault('RUST_LOG','miy=info')
for name in names:
 if name not in ['serve','worker']:raise SystemExit('unknown service')
 pidfile=local/(name+'.pid')
 pid=int(pidfile.read_text()) if pidfile.exists() else None
 running=False
 if pid:
  result=subprocess.run(['ps','-p',str(pid),'-o','command='],capture_output=True,text=True)
  running=str(binary)+' '+name in result.stdout
 if action=='start':
  if running:print(name+' already running');continue
  with (local/(name+'.log')).open('a') as log:
   service_env=env.copy()
   for key in (['PROVISION_DATABASE_URL'] if name=='serve' else ['MIY_WEB_SECRET','MIY_RESOURCE_SECRET']):service_env.pop(key,None)
   child=subprocess.Popen([str(binary),name],cwd=root,env=service_env,stdout=log,stderr=log,start_new_session=True)
  pidfile.write_text(str(child.pid));print(name+' started')
 elif action=='stop':
  if running:
   os.kill(pid,signal.SIGTERM)
   for _ in range(30):
    try:os.kill(pid,0)
    except ProcessLookupError:break
    time.sleep(.1)
  pidfile.unlink(missing_ok=True);print(name+' stopped')
 else:raise SystemExit('use start or stop')
