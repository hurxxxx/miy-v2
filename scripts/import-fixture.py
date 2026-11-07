#!/usr/bin/env python3
"""Import a reviewed UI/migration fixture. Not an OCI deployment."""
import hashlib,json,os,shutil,sys,tempfile
from pathlib import Path
root=Path(__file__).resolve().parent.parent
if not all(os.getenv(key) for key in ['MIY_STATE_DIR','MIY_FIXTURE_DIST','MIY_DATABASE_CONTAINER']):
 raise SystemExit('Fixture import requires explicit MIY_STATE_DIR, MIY_FIXTURE_DIST and MIY_DATABASE_CONTAINER for an isolated stack')
state=Path(os.environ['MIY_STATE_DIR']).resolve()
dist=Path(os.environ['MIY_FIXTURE_DIST']).resolve()
if state==(root/'.local').resolve() or os.environ['MIY_DATABASE_CONTAINER'].startswith('miy-v2-validation-'):
 raise SystemExit('Fixture import refuses the running platform state/database')
if not state.is_dir() or not (state/'server.json').is_file():
 raise SystemExit('Bootstrap the isolated fixture state before importing a fixture')
app=sys.argv[1] if len(sys.argv)>1 else 'reference'
version=sys.argv[2] if len(sys.argv)>2 else '1.0.0'
profile='reference' if app.startswith('reference') else 'diagnosis'
manifest={'contractVersion':'miy.app/v1alpha1','id':app,'name':'기준정보' if app=='reference' else 'UI 진단','version':version,'runtime':{'profile':'rust-web-v1'},'ui':{'delivery':'module','moduleEntry':'mf-manifest.json','moduleExport':'./AppRoot','contractVersion':'miy.ui/v1alpha1'},'resources':{'database':'postgresql'},'migrations':{'directory':'migrations'},'requestedPermissions':['database']}
with tempfile.TemporaryDirectory() as tmp:
 p=Path(tmp);shutil.copytree(dist/profile,p/'ui');shutil.copytree(root/'fixtures/migrations',p/'migrations');(p/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,sort_keys=True))
 entries=[(f.relative_to(p).as_posix(),hashlib.sha256(f.read_bytes()).hexdigest()) for f in p.rglob('*') if f.is_file()]
 h=hashlib.sha256(''.join(f'{n}\0{v}\n' for n,v in sorted(entries)).encode()).hexdigest();dest=state/'artifacts'/h
 if not dest.resolve().is_relative_to(state):raise SystemExit('Fixture artifacts path escaped the isolated state')
 if not dest.exists():shutil.copytree(p,dest)
 print(json.dumps({'appId':app,'version':version,'artifactHash':h}))
