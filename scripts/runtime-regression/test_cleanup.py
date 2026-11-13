"""Inject setup failures into the real runner/cleanup using an isolated Docker stub."""
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
import tracemalloc
import unittest
from workdir import create_owner, file_sha256, lock, prune

SCRIPTS = pathlib.Path(__file__).resolve().parent
DOCKER = r'''#!/usr/bin/env python3
import json,os,pathlib,sys,subprocess,time
args=sys.argv[1:]; path=pathlib.Path(os.environ['STUB_STATE'])
s=json.loads(path.read_text()); mode=os.environ['STUB_FAILURE']
def save():
 staged=path.with_suffix('.tmp');staged.write_text(json.dumps(s));os.replace(staged,path)
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
 if mode=='wait-setup':
  root=pathlib.Path(os.environ['MIY_AUDIT_ROOT'])
  (root/'bin/fixture').write_bytes(b'x'*4096)
  child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)',str(root)])
  s['childPid']=child.pid;save()
  time.sleep(60)
elif args[0]=='exec':
 if 'pg_isready' in args: pass
 elif mode=='after-db':fail()
elif args[0]=='build':
 root=pathlib.Path(os.environ['MIY_AUDIT_ROOT'])
 context=pathlib.Path(args[-1])
 s['fixtureModes']={name:(context/name).stat().st_mode&0o777 for name in ['miy','miy/ui','miy/migrations','server.py','miy/manifest.json','miy/ui/entry.js']}
 s['workspaceMode']=root.stat().st_mode&0o777
 s['privateEnvMode']=(root/'private-env.json').stat().st_mode&0o777
 (root/'bin/fixture').write_bytes(b'x'*4096)
 (root/'evidence/native-image.tar').write_bytes(b'reproducible fixture archive')
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
    def test_real_setup_fixture_remains_readable_by_nonroot_app(self):
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            (root / 'bin').mkdir()
            binary = root / 'bin/docker'
            binary.write_text(DOCKER)
            binary.chmod(0o755)
            state = root / 'state.json'
            state.write_text(json.dumps({'baseline': False, 'containers': [], 'networks': [], 'images': []}))
            env = {**os.environ, 'PATH': str(root / 'bin') + os.pathsep + os.environ['PATH'],
                   'MIY_RUNTIME_REGRESSION_DIR': str(root / 'runs'), 'STUB_STATE': str(state), 'STUB_FAILURE': 'after-build'}
            env.pop('MIY_RUNTIME_KEEP_WORK', None)
            result = subprocess.run([sys.executable, str(SCRIPTS / 'run.py')], env=env,
                                    capture_output=True, text=True, umask=0o022)
            self.assertNotEqual(result.returncode, 0, 'stub stops after inspecting the real setup build context')
            observed = json.loads(state.read_text())
            self.assertEqual(observed['workspaceMode'], 0o700, 'host execution root remains private')
            self.assertEqual(observed['privateEnvMode'], 0o600, 'fixture secrets remain private')
            for name in ('miy', 'miy/ui', 'miy/migrations'):
                self.assertEqual(observed['fixtureModes'][name] & 0o005, 0o005, name)
            for name in ('server.py', 'miy/manifest.json', 'miy/ui/entry.js'):
                self.assertEqual(observed['fixtureModes'][name] & 0o004, 0o004, name)

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
                env = {**os.environ, "PATH": str(root / "bin") + os.pathsep + os.environ["PATH"], "MIY_RUNTIME_REGRESSION_DIR": str(root / "tmp"), "STUB_STATE": str(state), "STUB_FAILURE": mode}
                env.pop('MIY_RUNTIME_KEEP_WORK', None)
                result = subprocess.run([sys.executable, str(SCRIPTS / "run.py")], env=env, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0, "setup failure must fail the overall run")
                runs = list((root / "tmp").glob("miy-runtime-regression-*"))
                self.assertEqual(len(runs), 1)
                summary = json.loads((runs[0] / "evidence/cleanup.json").read_text())
                self.assertEqual(summary["errors"], [], result.stdout + result.stderr)
                self.assertTrue(summary["removed"])
                self.assertEqual(summary['workspace']['status'], 'PASS')
                for name in ('bin', 'artifacts', 'images', 'private-env.json', 'fixture-ca.key', 'evidence/native-image.tar'):
                    self.assertFalse((runs[0] / name).exists(), name)
                self.assertTrue((runs[0] / 'evidence/runner-error.txt').is_file())
                if mode == 'after-build':
                    self.assertGreater(summary['workspace']['removedBytes'], 4096)
                    archive = next(row for row in summary['workspace']['removed'] if row['path'] == 'evidence/native-image.tar')
                    self.assertEqual(len(archive['sha256']), 64)
                if mode == "after-db":
                    self.assertTrue(summary["notes"], "an unavailable DB must still be cleaned")
                for kind in ["containers", "networks", "images"]:
                    self.assertEqual(json.loads(state.read_text())[kind], [])
                repeated = subprocess.run([sys.executable, str(SCRIPTS / "cleanup.py")], env={**env, "MIY_AUDIT_ROOT": str(runs[0]), "AUDIT_REAL_DOCKER": str(binary)}, capture_output=True, text=True)
                self.assertEqual(repeated.returncode, 0, repeated.stdout + repeated.stderr)
                self.assertEqual(json.loads(repeated.stdout)["removed"], [])

    def test_sigterm_reaps_owned_child_group_and_still_cleans_work(self):
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            (root / 'bin').mkdir()
            binary = root / 'bin/docker'
            binary.write_text(DOCKER)
            binary.chmod(0o755)
            state = root / 'state.json'
            state.write_text(json.dumps({'baseline': False, 'containers': [], 'networks': [], 'images': []}))
            env = {**os.environ, 'PATH': str(root / 'bin') + os.pathsep + os.environ['PATH'],
                   'MIY_RUNTIME_REGRESSION_DIR': str(root / 'runs'), 'STUB_STATE': str(state), 'STUB_FAILURE': 'wait-setup'}
            env.pop('MIY_RUNTIME_KEEP_WORK', None)
            runner = subprocess.Popen([sys.executable, str(SCRIPTS / 'run.py')], env=env,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                deadline = time.monotonic() + 10
                while 'childPid' not in (current := json.loads(state.read_text())):
                    self.assertLess(time.monotonic(), deadline, 'runner did not enter setup')
                    time.sleep(.02)
                runner.send_signal(signal.SIGTERM)
                stdout, stderr = runner.communicate(timeout=15)
                self.assertEqual(runner.returncode, 143, stdout + stderr)
                child = pathlib.Path('/proc') / str(current['childPid']) / 'status'
                self.assertTrue(not child.exists() or 'State:\tZ' in child.read_text(), 'owned child must not remain running')
                run = next((root / 'runs').iterdir())
                summary = json.loads((run / 'evidence/cleanup.json').read_text())
                self.assertEqual(summary['errors'], [], stdout + stderr)
                self.assertEqual(summary['workspace']['status'], 'PASS')
                self.assertFalse((run / 'bin').exists())
                self.assertEqual(json.loads(state.read_text())['containers'], [])
            finally:
                if runner.poll() is None:
                    runner.kill()
                runner.communicate()


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='miy-runtime-regression-')
        self.root = pathlib.Path(self.temp.name)
        create_owner(self.root)
        (self.root / 'bin').mkdir()
        (self.root / 'bin/fixture').write_bytes(b'fixture binary')
        (self.root / 'evidence').mkdir()
        (self.root / 'evidence/failure.json').write_text('{"status":"FAIL"}')

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def no_containers(*args):
        assert args == ('container', 'ls', '--all', '--quiet', '--no-trunc')
        return ''

    def test_finished_workspace_retains_failure_evidence_and_repeat_is_empty(self):
        result = prune(self.root, self.no_containers)
        self.assertEqual(result['removedBytes'], len(b'fixture binary'))
        self.assertEqual(json.loads((self.root / 'evidence/failure.json').read_text()), {'status': 'FAIL'})
        self.assertEqual(prune(self.root, self.no_containers)['removed'], [])

    def test_running_runner_lock_refuses_external_cleanup(self):
        with lock(self.root):
            result = subprocess.run([sys.executable, str(SCRIPTS / 'cleanup.py')],
                                    env={**os.environ, 'MIY_AUDIT_ROOT': str(self.root)}, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('workspace_in_use', result.stderr)
        self.assertTrue((self.root / 'bin/fixture').exists())

    def test_debug_retention_is_explicit_and_keeps_payloads(self):
        result = subprocess.run([sys.executable, str(SCRIPTS / 'cleanup.py')],
                                env={**os.environ, 'MIY_AUDIT_ROOT': str(self.root),
                                     'MIY_RUNTIME_KEEP_WORK': '1'}, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['workspace']['status'], 'KEPT')
        self.assertTrue((self.root / 'bin/fixture').exists())

    def test_cleanup_retry_preserves_the_prior_failure_record(self):
        with tempfile.TemporaryDirectory() as outside:
            binary = pathlib.Path(outside) / 'docker'
            binary.write_text('#!' + sys.executable + '\nimport sys\nassert sys.argv[1:]==["container","ls","--all","--quiet","--no-trunc"]\n')
            binary.chmod(0o755)
            link = self.root / 'bin/linked'
            link.symlink_to(outside, target_is_directory=True)
            env = {**os.environ, 'MIY_AUDIT_ROOT': str(self.root), 'AUDIT_REAL_DOCKER': str(binary)}
            env.pop('MIY_RUNTIME_KEEP_WORK', None)
            first = subprocess.run([sys.executable, str(SCRIPTS / 'cleanup.py')], env=env, capture_output=True, text=True)
            self.assertEqual(first.returncode, 1, first.stdout + first.stderr)
            failed = json.loads(first.stdout)
            self.assertEqual(failed['workspace']['status'], 'FAIL')
            link.unlink()
            second = subprocess.run([sys.executable, str(SCRIPTS / 'cleanup.py')], env=env, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            passed = json.loads(second.stdout)
            self.assertNotEqual(failed['attemptFile'], passed['attemptFile'])
            self.assertEqual(json.loads((self.root / 'evidence' / failed['attemptFile']).read_text()), failed)
            self.assertEqual(json.loads((self.root / 'evidence/cleanup.json').read_text()), passed)

    def test_root_symlink_does_not_adopt_another_directory(self):
        with tempfile.TemporaryDirectory() as other:
            alias = pathlib.Path(other) / 'miy-runtime-regression-alias'
            alias.symlink_to(self.root, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, 'workspace_symlink_refused'):
                prune(alias, self.no_containers)
            self.assertTrue((self.root / 'bin/fixture').exists())

    def test_symlink_payload_does_not_touch_external_files(self):
        with tempfile.TemporaryDirectory() as outside:
            victim = pathlib.Path(outside) / 'preserved'
            victim.write_text('keep')
            (self.root / 'bin/linked').symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, 'payload_special_file_refused'):
                prune(self.root, self.no_containers)
            self.assertEqual(victim.read_text(), 'keep')
            self.assertTrue((self.root / 'bin/fixture').exists())

    def test_marker_cannot_redirect_cleanup_to_a_different_root(self):
        marker = self.root / '.miy-runtime-owner.json'
        data = json.loads(marker.read_text())
        data['root'] = '/'
        marker.write_text(json.dumps(data))
        with self.assertRaisesRegex(RuntimeError, 'workspace_marker_mismatch'):
            prune(self.root, self.no_containers)
        self.assertTrue((self.root / 'bin/fixture').exists())

    def test_open_payload_descriptor_prevents_pruning(self):
        child = subprocess.Popen([sys.executable, '-c',
            'import os,time; f=open(os.environ["TEST_PAYLOAD"]); print("ready",flush=True); time.sleep(30)'],
            env={**os.environ, 'TEST_PAYLOAD': str(self.root / 'bin/fixture')}, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(child.stdout.readline(), 'ready\n')
            with self.assertRaisesRegex(RuntimeError, 'payload_process_still_running'):
                prune(self.root, self.no_containers)
            self.assertTrue((self.root / 'bin/fixture').exists())
        finally:
            child.terminate()
            child.wait(timeout=5)
            child.stdout.close()

    def test_orphan_with_run_environment_prevents_pruning(self):
        child = subprocess.Popen([sys.executable, '-c',
            'import time; print("ready",flush=True); time.sleep(30)'],
            env={**os.environ, 'MIY_AUDIT_ROOT': str(self.root)}, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(child.stdout.readline(), 'ready\n')
            with self.assertRaisesRegex(RuntimeError, 'payload_process_still_running'):
                prune(self.root, self.no_containers)
            self.assertTrue((self.root / 'bin/fixture').exists())
        finally:
            child.terminate()
            child.wait(timeout=5)
            child.stdout.close()

    def test_container_bind_to_workspace_or_ancestor_prevents_pruning(self):
        for source in (str(self.root / 'bin'), str(self.root.parent), '/'):
            with self.subTest(source=source):
                def docker(*args):
                    if args[:2] == ('container', 'ls'):
                        return 'fixture'
                    return json.dumps([{'Mounts': [{'Type': 'bind', 'Source': source}]}])
                with self.assertRaisesRegex(RuntimeError, 'payload_container_mount_exists'):
                    prune(self.root, docker)
                self.assertTrue((self.root / 'bin/fixture').exists())

    def test_unavailable_container_inventory_keeps_payloads(self):
        def unavailable(*args):
            raise RuntimeError('docker_unavailable')
        with self.assertRaisesRegex(RuntimeError, 'docker_unavailable'):
            prune(self.root, unavailable)
        self.assertTrue((self.root / 'bin/fixture').exists())

    def test_hashing_does_not_allocate_the_entire_binary(self):
        path = self.root / 'bin/large'
        with path.open('wb') as handle:
            handle.truncate(32 * 1024 * 1024)
        tracemalloc.start()
        try:
            self.assertEqual(len(file_sha256(path)), 64)
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertLess(peak, 2 * 1024 * 1024)


if __name__ == "__main__":
    unittest.main()
