#!/usr/bin/env python3
"""Run up.sh with owned temporary state and mocked build/service commands.

Inline Python still normalizes/reads paths. Docker, Cargo, pnpm, bootstrap and
service commands are replaced by Bash functions, even if up.sh prepends Cargo's
real directory to PATH. No daemon, network, real build or operating state is used.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
STUB = r'''
import json, os, sys
from pathlib import Path
name, args = sys.argv[1], sys.argv[2:]
root = Path(os.environ['MIY_UP_TEST_ROOT'])
state = os.environ.get('MIY_STATE_DIR')
with Path(os.environ['MIY_UP_TEST_LOG']).open('a') as log:
    log.write(json.dumps({'name': name, 'args': args, 'state': state, 'cwd': str(Path.cwd())}) + '\n')

if name == 'python3':
    if args[0] in ('-c', '-'):
        # Execute up.sh's real path normalization and settings reader only.
        if args[0] == '-c' and os.environ.get('MIY_UP_TEST_FAIL_NORMALIZATION'):
            sys.exit(23)
        os.execv(sys.executable, [sys.executable, *args])
    if args[0] == 'scripts/bootstrap.py':
        selected = state
        for index, arg in enumerate(args[1:], 1):
            if arg == '--state-dir': selected = args[index + 1]
            elif arg.startswith('--state-dir='): selected = arg.split('=', 1)[1]
        target = Path(selected or root / '.local').resolve()
        assert target.is_relative_to(root), 'fixture cannot write outside its temporary checkout'
        target.mkdir(parents=True)
        settings = {'origin': 'https://portal.invalid', 'consoleOrigin': 'https://console.invalid',
            'devPortalOrigin': 'https://dev.invalid', 'identityAdminUrl': 'http://127.0.0.1:18181/identity',
            'proxyMode': 'external', 'developmentLogin': False}
        (target / 'server.json').write_text(json.dumps(settings))
        (target / 'compose.env').write_text('# fixture only\n')
        # Model bootstrap's absolute MIY_FRAME_RUNTIME_DIR contract independently
        # of the path that up.sh later passes to pnpm.
        (root / 'core-frame-path.txt').write_text(str(target / 'frame-runtime'))
    elif args in (['scripts/configure-identity.py'], ['scripts/services.py', 'start']):
        pass
    else:
        raise AssertionError('unexpected Python script')
elif name == 'pnpm':
    if args in (['install', '--frozen-lockfile'], ['build']):
        pass
    else:
        assert args[:5] == ['--filter', '@miy/frame-runtime', 'exec', 'vite', 'build']
        out = Path(args[args.index('--outDir') + 1])
        # pnpm --filter ... exec runs from the package, not the repository root.
        package = root / 'web/frame-runtime'
        destination = (package / out).resolve()
        assert destination.is_relative_to(root), 'frame build escaped its fixture'
        destination.mkdir(parents=True)
        (destination / 'index.html').write_text('fixture frame runtime\n')
elif name == 'docker':
    assert args[0] == 'compose' and args[1] == '--env-file'
    assert args[3:] == ['up', '-d', 'postgres', 'keycloak']
    assert Path(args[2]).is_file()
elif name == 'cargo':
    assert args == ['build', '--locked']
elif name == 'bash':
    assert args in (['scripts/run-service.sh', 'migrate'], ['scripts/run-service.sh', 'seed'])
elif name == 'curl':
    assert args[-1].endswith(('/.well-known/openid-configuration', '/api/health'))
else:
    raise AssertionError('unexpected external command')
'''


class UpStateDirectoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='miy-up-state-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / 'scripts').mkdir()
        shutil.copyfile(ROOT / 'scripts/up.sh', self.root / 'scripts/up.sh')
        self.stub = self.root / 'stub.py'
        self.stub.write_text(STUB)
        self.log = self.root / 'commands.jsonl'
        self.startup = self.root / 'stub-functions.sh'
        self.startup.write_text(
            'miy_up_stub() { command "$MIY_UP_TEST_PYTHON" "$MIY_UP_TEST_STUB" "$@"; }\n'
            + ''.join(f'{name}() {{ miy_up_stub {name} "$@"; }}\n'
                      for name in ('python3', 'pnpm', 'docker', 'cargo', 'bash', 'curl'))
        )
        self.env = {key: value for key, value in os.environ.items() if not key.startswith('MIY_')}
        self.env.update({'BASH_ENV': str(self.startup), 'MIY_UP_TEST_PYTHON': sys.executable,
                         'MIY_UP_TEST_STUB': str(self.stub), 'MIY_UP_TEST_LOG': str(self.log),
                         'MIY_UP_TEST_ROOT': str(self.root)})

    def run_setup(self, args, expected, env_state=None):
        if env_state is not None:
            self.env['MIY_STATE_DIR'] = env_state
        # Start outside the checkout too: up.sh is responsible for selecting it.
        result = subprocess.run(['/usr/bin/bash', str(self.root / 'scripts/up.sh'),
                                 '--profile', 'server', '--origin', 'https://portal.invalid', *args],
                                cwd=self.root.parent, env=self.env, capture_output=True, text=True,
                                timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = (self.root / expected).resolve()
        core_frame = Path((self.root / 'core-frame-path.txt').read_text())
        self.assertEqual(core_frame, expected / 'frame-runtime')
        self.assertTrue((core_frame / 'index.html').is_file(), 'Core cannot find the built frame runtime')
        events = [json.loads(line) for line in self.log.read_text().splitlines()]
        build = next(event for event in events if event['name'] == 'pnpm' and '--outDir' in event['args'])
        self.assertEqual(build['args'][build['args'].index('--outDir') + 1], str(core_frame))
        for event in events:
            # The normalization command itself runs before the final export.
            if event['name'] == 'python3' and event['args'][0] == '-c':
                continue
            self.assertEqual(event['state'], str(expected), event)
        self.assertIn('Platform ready: https://portal.invalid', result.stdout)
        self.assertEqual([event['args'] for event in events if event['name'] == 'bash'],
                         [['scripts/run-service.sh', 'migrate'], ['scripts/run-service.sh', 'seed']])

    def test_separate_relative_option(self):
        self.run_setup(['--state-dir', 'state with spaces'], 'state with spaces')

    def test_equals_relative_option(self):
        self.run_setup(['--state-dir=var/../state'], 'state')

    def test_relative_environment(self):
        self.run_setup([], 'env state', env_state='env state')

    def test_absolute_option(self):
        self.run_setup(['--state-dir', str(self.root / 'absolute state')], 'absolute state')

    def test_default_checkout_state(self):
        self.run_setup([], '.local')

    def test_explicit_option_overrides_environment(self):
        self.run_setup(['--state-dir=chosen'], 'chosen', env_state='ignored')
        self.assertFalse((self.root / 'ignored').exists())

    def test_path_resolution_failure_stops_before_any_setup(self):
        self.env.update({'MIY_STATE_DIR': 'state', 'MIY_UP_TEST_FAIL_NORMALIZATION': '1'})
        result = subprocess.run(['/usr/bin/bash', str(self.root / 'scripts/up.sh')],
                                cwd=self.root.parent, env=self.env, capture_output=True,
                                timeout=15)
        self.assertEqual(result.returncode, 23)
        events = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]['args'][0], '-c')
        self.assertFalse((self.root / 'core-frame-path.txt').exists())

    def test_empty_explicit_paths_are_rejected_before_setup(self):
        for args in (['--state-dir', ''], ['--state-dir=']):
            with self.subTest(args=args):
                result = subprocess.run(['/usr/bin/bash', str(self.root / 'scripts/up.sh'), *args],
                                        cwd=self.root.parent, env=self.env, capture_output=True,
                                        timeout=15)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.log.exists())


if __name__ == '__main__':
    unittest.main()
