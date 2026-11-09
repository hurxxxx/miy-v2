#!/usr/bin/env python3
"""Offline tests for scripts/dev-portal-settings.py: no Keycloak, Nginx, sudo or live state."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('dev_portal_settings', ROOT / 'scripts/dev-portal-settings.py')
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

PORTAL, CONSOLE, DEV = 'https://v2.example.test', 'https://v2.example.test:9443', 'https://v2.example.test:9444'
LIVE_NGINX = '# Generated from fixed portal/console settings.\nmap $x $y {}\nserver {\n  portal\n}\nserver {\n  console\n}\n'


class FakeIdentity:
    clients = {}

    def __init__(self, state):
        self.state = state

    def snapshot(self):
        client = copy.deepcopy(self.clients['miy-web'])
        return {'client': client, 'clientSecretSha256': 's' * 64}

    def update(self, client_id, body):
        assert client_id == 'web-id'
        self.clients['miy-web'].update(copy.deepcopy(body))


class DevPortalSettingsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='miy-dev-portal-test-'))
        self.state = self.root / 'state'
        self.state.mkdir(mode=0o700)
        settings = {'origin': PORTAL, 'consoleOrigin': CONSOLE, 'bind': '127.0.0.1:18180',
                    'consoleBind': '127.0.0.1:18182', 'identityAdminUrl': 'http://127.0.0.1:18181/identity'}
        (self.state / 'server.json').write_text(json.dumps(settings, indent=2) + '\n')
        (self.state / 'runtime.env').write_text("DATABASE_URL='private-sentinel'\nMIY_ORIGIN=" + PORTAL + '\n')
        (self.state / 'compose.env').write_text('MIY_PG_ADMIN_PASSWORD=private-sentinel\nMIY_ORIGIN=' + PORTAL + '\n')
        for name in tool.FILES:
            (self.state / name).chmod(0o600)
        FakeIdentity.clients = {'miy-web': {'id': 'web-id', 'clientId': 'miy-web',
                                            'redirectUris': [PORTAL + '/auth/callback', CONSOLE + '/auth/callback'],
                                            'webOrigins': [PORTAL, CONSOLE],
                                            'attributes': {'post.logout.redirect.uris': PORTAL + '/##' + CONSOLE + '/',
                                                           'pkce.code.challenge.method': 'S256'}}}
        self.release = self.state / 'release-dev-portal'

    def tearDown(self):
        shutil.rmtree(self.root)

    def apply(self, origin=DEV, bind='127.0.0.1:18183'):
        return tool.apply(self.state, self.release, origin, bind, identity=FakeIdentity, probe=lambda _port: True)

    def test_apply_adds_only_the_dev_portal_and_keeps_every_other_line_and_secret(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        result = self.apply()
        self.assertEqual(result['status'], 'APPLIED')
        runtime = (self.state / 'runtime.env').read_text()
        self.assertTrue(runtime.startswith(before['runtime.env'].decode()))
        self.assertIn('MIY_DEV_PORTAL_ORIGIN=' + DEV, runtime)
        self.assertIn('MIY_DEV_PORTAL_BIND_ADDR=127.0.0.1:18183', runtime)
        compose = (self.state / 'compose.env').read_text()
        self.assertTrue(compose.startswith(before['compose.env'].decode()))
        self.assertIn('MIY_DEV_PORTAL_PORT=9444', compose)
        settings = json.loads((self.state / 'server.json').read_text())
        self.assertEqual((settings['devPortalOrigin'], settings['devPortalBind']), (DEV, '127.0.0.1:18183'))
        self.assertEqual({name: (self.state / name).stat().st_mode & 0o777 for name in tool.FILES},
                         {name: 0o600 for name in tool.FILES})
        client = FakeIdentity.clients['miy-web']
        self.assertEqual(client['redirectUris'][-1], DEV + '/auth/callback')
        self.assertEqual(client['webOrigins'], [PORTAL, CONSOLE])
        self.assertTrue(client['attributes']['post.logout.redirect.uris'].endswith('##' + DEV + '/'))
        self.assertEqual(client['attributes']['pkce.code.challenge.method'], 'S256')
        record = (self.release / 'settings.json').read_text()
        self.assertNotIn('private-sentinel', record)

    def test_restore_returns_files_and_identity_exactly(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        self.apply()
        self.assertEqual(tool.restore(self.state, self.release, identity=FakeIdentity)['status'], 'RESTORED')
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
        client = FakeIdentity.clients['miy-web']
        self.assertEqual(client['redirectUris'], client_before['redirectUris'])
        self.assertEqual(client['attributes'], client_before['attributes'])

    def test_restore_refuses_a_file_changed_after_apply(self):
        self.apply()
        with (self.state / 'runtime.env').open('a') as handle:
            handle.write('OTHER=1\n')
        with self.assertRaisesRegex(RuntimeError, 'settings_changed_since_apply:runtime.env'):
            tool.restore(self.state, self.release, identity=FakeIdentity)

    def test_invalid_or_conflicting_targets_change_nothing(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        cases = [
            (PORTAL, '127.0.0.1:18183', 'dev_portal_origin_must_be_distinct_with_same_transport'),
            (CONSOLE, '127.0.0.1:18183', 'dev_portal_origin_must_be_distinct_with_same_transport'),
            ('http://v2.example.test:9444', '127.0.0.1:18183', 'dev_portal_origin_must_be_distinct_with_same_transport'),
            (DEV + '/path', '127.0.0.1:18183', 'invalid_dev_portal_origin'),
            (DEV, '0.0.0.0:18183', 'dev_portal_bind_must_be_loopback'),
            (DEV, '127.0.0.1:18182', 'dev_portal_bind_must_be_distinct'),
        ]
        for origin, bind, code in cases:
            with self.subTest(code=code, origin=origin, bind=bind):
                with self.assertRaisesRegex(RuntimeError, code):
                    self.apply(origin, bind)
                self.assertFalse(self.release.exists())
        with self.assertRaisesRegex(RuntimeError, 'dev_portal_port_in_use'):
            tool.apply(self.state, self.release, DEV, '127.0.0.1:18183', identity=FakeIdentity, probe=lambda _p: False)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)

    def test_identity_secret_change_is_refused(self):
        class Rotated(FakeIdentity):
            calls = 0

            def snapshot(self):
                Rotated.calls += 1
                value = super().snapshot()
                if Rotated.calls > 1:
                    value['clientSecretSha256'] = 'r' * 64
                return value
        with self.assertRaisesRegex(RuntimeError, 'identity_id_or_secret_changed'):
            tool.apply(self.state, self.release, DEV, '127.0.0.1:18183', identity=Rotated, probe=lambda _p: True)

    def nginx(self, candidate_blocks=3, test_ok=True):
        site = self.root / 'site.conf'
        site.write_text(LIVE_NGINX)
        (self.state / 'nginx-miy.conf').write_text(LIVE_NGINX)
        (self.state / 'bin').mkdir(exist_ok=True)
        self.apply()
        calls = []

        def fake_run(args, **_kwargs):
            args = [str(a) for a in args]
            calls.append(args[0] if args[0] != 'install' else 'install')
            if args[1].endswith('render-server-config.py'):
                work = Path(args[args.index('--state-dir') + 1])
                extra = ''.join('server {\n  dev\n}\n' for _ in range(candidate_blocks - 2))
                (work / 'nginx-miy.conf').write_text('# new header\n' + LIVE_NGINX.split('\n', 1)[1] + extra)
                return ''
            if args[0] == 'cat':
                return Path(args[1]).read_text()
            if args[0] == 'install':
                shutil.copyfile(args[-2], args[-1])
                return ''
            if args[0] == 'nginx' and not test_ok:
                raise RuntimeError('command_failed:nginx')
            if args[0] == 'rm':
                shutil.rmtree(args[-1])
            return ''
        return site, calls, fake_run

    def test_nginx_apply_adds_one_block_and_restore_reinstalls_the_previous_file(self):
        site, calls, fake_run = self.nginx()
        with patch.object(tool, 'run', fake_run):
            result = tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
            self.assertEqual(result['status'], 'NGINX_APPLIED')
            self.assertIn('dev', site.read_text())
            self.assertIn('systemctl', calls)
            self.assertEqual(tool.nginx_restore(self.state, self.release, site=site, sudo=())['status'], 'NGINX_RESTORED')
        self.assertEqual(site.read_text(), LIVE_NGINX)
        self.assertEqual((self.state / 'nginx-miy.conf').read_text(), LIVE_NGINX)

    def test_nginx_apply_refuses_unexpected_changes(self):
        site, calls, fake_run = self.nginx(candidate_blocks=4)
        with patch.object(tool, 'run', fake_run), \
                self.assertRaisesRegex(RuntimeError, 'rendered_nginx_changes_more_than_the_dev_portal'):
            tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
        self.assertEqual(site.read_text(), LIVE_NGINX)
        self.assertNotIn('install', calls)

    def test_nginx_test_failure_reinstalls_the_live_file_without_reload(self):
        site, calls, fake_run = self.nginx(test_ok=False)
        with patch.object(tool, 'run', fake_run), self.assertRaisesRegex(RuntimeError, 'nginx_test_failed_restored'):
            tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
        self.assertEqual(site.read_text(), LIVE_NGINX)
        self.assertNotIn('systemctl', calls)

    def test_nginx_refuses_a_live_file_that_differs_from_the_rendered_copy(self):
        site, _calls, fake_run = self.nginx()
        site.write_text(LIVE_NGINX + '# manual edit\n')
        with patch.object(tool, 'run', fake_run), self.assertRaisesRegex(RuntimeError, 'live_nginx_differs_from_rendered_copy'):
            tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())


if __name__ == '__main__':
    unittest.main(verbosity=2)
