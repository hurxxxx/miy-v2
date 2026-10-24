"""Inject setup failures into the real runner/cleanup using an isolated Docker stub."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = pathlib.Path(__file__).resolve().parent
DOCKER = r'''#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:]; path=pathlib.Path(os.environ['STUB_STATE'])
s=json.loads(path.read_text()); mode=os.environ['STUB_FAILURE']
def save():path.write_text(json.dumps(s))
def fail():save();sys.exit(41)
def descriptor(name):
 return {'Id':name,'Name':'/'+name,'Image':'fixture','State':{'Running':True,'StartedAt':'unchanged'},'Config':{'Labels':{'miy.audit':s['prefix']} if name!='baseline' else {}},'NetworkSettings':{'Ports':{'5432/tcp':[{'HostPort':'19999'}]}}}
if args[:2]==['image','inspect'] and args[2] in ['postgres:18.3','python:3.14-slim']:
 print('[]')
elif args[0]=='ps':
 if s['baseline']: print(json.dumps({'ID':'baseline'}))
elif args[0]=='inspect':
 print(json.dumps([descriptor(name) for name in args[1:]]))
elif args[:2]==['network','create']:
 s['prefix']=args[-1];s['networks']=[args[-1]];print('fixture-network')
elif args[0]=='run':
 if mode=='before-db':fail()
 s['containers']=[args[args.index('--name')+1]];print('fixture-db')
elif args[0]=='exec':
 if 'pg_isready' in args: pass
 elif mode=='after-db':fail()
elif args[0]=='build':
 s['images']=[args[args.index('-t')+1]];fail()
elif args[:2]==['container','ls']:
 print('\n'.join(s['containers']))
elif args[:2]==['container','inspect']:
 assert args[2]=='baseline' or args[2] in s['containers']
 print(json.dumps([descriptor(args[2])]))
elif args[0]=='rm':
 assert args[-1] in s['containers'];s['containers'].remove(args[-1])
elif args[:2]==['network','ls']:
 for name in s['networks']:print(json.dumps({'Name':name}))
elif args[:2]==['network','inspect']:
 assert args[2] in s['networks'];print(json.dumps([{'Id':args[2],'Labels':{'miy.audit':s['prefix']},'Containers':{}}]))
elif args[:2]==['network','rm']:
 s['networks'].remove(args[2])
elif args[:2]==['image','ls']:
 print('\n'.join(s['images']))
elif args[:2]==['image','inspect']:
 assert args[2] in s['images'];print(json.dumps([{'Config':{'Labels':{'miy.audit':s['prefix']}},'RepoTags':[args[2]]}]))
elif args[:2]==['image','rm']:
 for tag in args[2:]:s['images'].remove(tag)
else:
 raise RuntimeError('unexpected stub command: '+args[0])
save()
'''


class CleanupTests(unittest.TestCase):
    def test_partial_setup_is_cleaned_and_repeat_cleanup_is_safe(self):
        for mode in ["before-db", "after-db", "after-build"]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                root = pathlib.Path(temp)
                (root / "bin").mkdir()
                (root / "tmp").mkdir()
                binary = root / "bin/docker"
                binary.write_text(DOCKER)
                binary.chmod(0o755)
                state = root / "state.json"
                state.write_text(json.dumps({"baseline": mode != "before-db", "containers": [], "networks": [], "images": []}))
                env = {**os.environ, "PATH": str(root / "bin") + os.pathsep + os.environ["PATH"], "TMPDIR": str(root / "tmp"), "STUB_STATE": str(state), "STUB_FAILURE": mode}
                result = subprocess.run([sys.executable, str(SCRIPTS / "run.py")], env=env, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0, "setup failure must fail the overall run")
                runs = list((root / "tmp").glob("miy-runtime-regression-*"))
                self.assertEqual(len(runs), 1)
                summary = json.loads((runs[0] / "evidence/cleanup.json").read_text())
                self.assertEqual(summary["errors"], [], result.stdout + result.stderr)
                self.assertTrue(summary["removed"])
                if mode == "after-db":
                    self.assertTrue(summary["notes"], "an unavailable DB must still be cleaned")
                for kind in ["containers", "networks", "images"]:
                    self.assertEqual(json.loads(state.read_text())[kind], [])
                repeated = subprocess.run([sys.executable, str(SCRIPTS / "cleanup.py")], env={**env, "MIY_AUDIT_ROOT": str(runs[0]), "AUDIT_REAL_DOCKER": str(binary)}, capture_output=True, text=True)
                self.assertEqual(repeated.returncode, 0, repeated.stdout + repeated.stderr)
                self.assertEqual(json.loads(repeated.stdout)["removed"], [])


if __name__ == "__main__":
    unittest.main()
