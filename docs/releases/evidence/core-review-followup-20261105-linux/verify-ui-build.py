#!/usr/bin/env python3
"""Use synthetic files only; every generated output and fake live tree stays below /tmp."""
import hashlib, importlib.util, json, os, pathlib, subprocess, sys, tempfile
from types import SimpleNamespace
from unittest.mock import patch
sys.dont_write_bytecode = True
repo = pathlib.Path('/projects/miy-v2')
tool_sha256 = hashlib.sha256((repo/'scripts/ui-rollout.py').read_bytes()).hexdigest()
spec = importlib.util.spec_from_file_location('review_ui_rollout', repo / 'scripts/ui-rollout.py')
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)
base = pathlib.Path(tempfile.mkdtemp(prefix='miy-ui-real-vite-fixed-review-'))
base.chmod(0o700)
source = base / 'source'
app = source / 'web/platform'
app.mkdir(parents=True)
(app / 'public').mkdir()
(source / '.gitignore').write_text('.env\n.env.*\nnode_modules\n')
(source / 'package.json').write_text(json.dumps({'name': 'review-only', 'private': True, 'packageManager': 'pnpm@10.34.5'}))
(source / 'pnpm-workspace.yaml').write_text('packages:\n  - web/*\n')
(app / 'package.json').write_text(json.dumps({'name': '@miy/platform', 'private': True}))
(app / 'index.html').write_text('<!doctype html><html><body><script type="module" src="/main.js"></script></body></html>')
(app / 'main.js').write_text('document.body.dataset.review = "synthetic";\n')
(app / 'public/public-marker.txt').write_text('NONSECRET tracked public marker\n')
(app / 'public/.env').write_text('NONSECRET_REVIEW_FIXTURE=ignored-public-file\n')
(source / 'node_modules').symlink_to(repo / 'node_modules', target_is_directory=True)
(app / 'node_modules').symlink_to(repo / 'web/platform/node_modules', target_is_directory=True)
live = base / 'live'
live.mkdir()
(live / 'old.js').write_text('window.fixtureOld = true;\n')
(live / 'index.html').write_text('<!doctype html><html><body><script src="/old.js"></script></body></html>')
old_index = (live / 'index.html').read_bytes()
environment = {'PATH': '/usr/bin:/bin', 'COREPACK_ENABLE_NETWORK': '0', 'COREPACK_ENABLE_AUTO_PIN': '0'}
def git(*args):
    result = subprocess.run(['git', *args], cwd=source, env=environment, capture_output=True, text=True, check=True)
    return result.stdout.strip()
git('init', '-q')
git('add', '.')
git('-c', 'user.name=Isolated Review', '-c', 'user.email=review@example.invalid', 'commit', '-qm', 'synthetic reviewed source')
commit = git('rev-parse', 'HEAD')
assert git('status', '--porcelain') == ''
assert git('check-ignore', 'web/platform/public/.env') == 'web/platform/public/.env'
result = {'fixtureRoot': str(base), 'sourceCommit': commit, 'toolSha256': tool_sha256, 'checks': {}}
def args(name, action='build'):
    return SimpleNamespace(action=action, release_dir=base / '.local' / name, source_root=source, commit=commit, live=live,
                       manifest=None, candidate=None, backup=None, expected_commit=commit)
# First prove the real build input path, independently of rollout's preflight.
subprocess.run(['pnpm','--filter','@miy/platform','exec','vite','build','--mode','production','--outDir',str(base/'direct-vite-output'),'--emptyOutDir'], cwd=source, env=environment, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
result['checks']['realViteCopiesGitIgnoredPublicFile']=(base/'direct-vite-output/.env').read_bytes()==(app/'public/.env').read_bytes()
with patch.object(ui, 'ROOT', base), patch.dict(os.environ, environment, clear=True):
    try:
        ui.run(args('rejected-public'))
    except RuntimeError as e:
        result['publicRejection']=str(e)
    else:
        result['publicRejection']='NOT_REJECTED'
    result['checks']['ignoredPublicRejectedBeforeBuild']=result['publicRejection']=='untracked public build input: web/platform/public/.env' and not (base/'.local/rejected-public/dist').exists()
    result['checks']['liveUnchangedAfterRejectedBuild']=(live/'index.html').read_bytes()==old_index and not (live/'.env').exists()
    (app/'public/.env').unlink()
    (app/'.env.production.local').write_text('VITE_NONSECRET_REVIEW_FIXTURE=ambient\n')
    try:
        ui.run(args('rejected-dotenv'))
    except RuntimeError as e:
        result['dotenvRejection']=str(e)
    else:
        result['dotenvRejection']='NOT_REJECTED'
    result['checks']['ambientDotenvRejectedBeforeBuild']=result['dotenvRejection']!='NOT_REJECTED' and not (base/'.local/rejected-dotenv/dist').exists()
    (app/'.env.production.local').unlink()
    valid=args('tracked-public')
    ui.run(valid)
    valid.action='inspect'; inspected=ui.run(valid)
    valid.action='apply'; applied=ui.run(valid)
    result['checks']['trackedPublicBuildInspectApply']=isinstance(inspected,dict) and applied['phase']=='applied' and (live/'public-marker.txt').read_bytes()==(app/'public/public-marker.txt').read_bytes()
    result['checks']['ignoredFileNeverPublished']=not (live/'.env').exists()
    valid.action='rollback'; rolled=ui.run(valid)
    result['checks']['rollbackRestoresIndexKeepsAddedAssets']=rolled['phase']=='rolled_back' and (live/'index.html').read_bytes()==old_index and (live/'public-marker.txt').exists()
result['checks']['fixtureTrackedSourceRemainsClean']=git('status','--porcelain')==''
result['status']='PASS' if all(result['checks'].values()) else 'FAIL'
(base/'review-result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
sys.exit(0 if result['status']=='PASS' else 1)
