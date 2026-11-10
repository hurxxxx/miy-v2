#!/usr/bin/env python3
"""Offline tests for scripts/dev-portal-settings.py: no Keycloak, Nginx, sudo or live state."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
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

    def test_identity_failure_before_or_after_update_leaves_a_restorable_plan(self):
        for failure in ('before_update', 'after_update', 'verification'):
            with self.subTest(failure=failure):
                before = {name: (self.state / name).read_bytes() for name in tool.FILES}
                client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])

                class FailingIdentity(FakeIdentity):
                    updated = False

                    def update(self, client_id, body):
                        if failure != 'before_update':
                            super().update(client_id, body)
                            self.updated = True
                        if failure == 'verification':
                            return
                        raise RuntimeError('injected_identity_failure')

                    def snapshot(self):
                        if self.updated:
                            raise RuntimeError('injected_identity_failure')
                        return super().snapshot()

                release = self.state / ('identity-failure-' + failure)
                with self.assertRaisesRegex(RuntimeError, 'injected_identity_failure'):
                    tool.apply(self.state, release, DEV, '127.0.0.1:18183',
                               identity=FailingIdentity, probe=lambda _p: True)
                self.assertEqual(tool.restore(self.state, release, identity=FakeIdentity)['status'], 'RESTORED')
                self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
                self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_backup_or_manifest_failure_changes_nothing(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        private_write = tool.private_write
        for target in ('settings-backup/compose.env', 'settings.json'):
            with self.subTest(target=target):
                self.release = self.state / target.replace('/', '-')

                def fail_preparation(path, data):
                    if path == self.release / target:
                        raise OSError('injected_preparation_failure')
                    private_write(path, data)

                with patch.object(tool, 'private_write', fail_preparation), \
                        self.assertRaisesRegex(OSError, 'injected_preparation_failure'):
                    self.apply()
                self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
                self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_failure_to_record_completion_still_allows_restore(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        private_write = tool.private_write

        def fail_completion(path, data):
            if path == self.release / 'settings.json' and json.loads(data)['phase'] == 'applied':
                raise OSError('injected_completion_failure')
            private_write(path, data)

        with patch.object(tool, 'private_write', fail_completion), \
                self.assertRaisesRegex(OSError, 'injected_completion_failure'):
            self.apply()
        tool.restore(self.state, self.release, identity=FakeIdentity)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
        self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_interrupted_apply_and_restore_can_be_restored_repeatedly(self):
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        private_write = tool.private_write

        def fail_after_writing_one_file(path, data):
            if path == self.state / 'runtime.env':
                raise OSError('injected_write_failure')
            private_write(path, data)

        with patch.object(tool, 'private_write', fail_after_writing_one_file), \
                self.assertRaisesRegex(OSError, 'injected_write_failure'):
            self.apply()
        tool.restore(self.state, self.release, identity=FakeIdentity)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
        self.assertEqual(FakeIdentity.clients['miy-web'], client_before)
        self.release = self.state / 'second-apply'
        self.apply()
        with patch.object(tool, 'private_write', fail_after_writing_one_file), \
                self.assertRaisesRegex(OSError, 'injected_write_failure'):
            tool.restore(self.state, self.release, identity=FakeIdentity)
        for _ in range(2):
            tool.restore(self.state, self.release, identity=FakeIdentity)
            self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
            self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_reapply_and_restore_preserves_an_existing_dev_portal(self):
        self.apply()
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        self.release = self.state / 'reapply'
        self.apply()
        tool.restore(self.state, self.release, identity=FakeIdentity)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
        self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_restore_preserves_each_existing_uri_and_unrelated_identity_changes(self):
        original = copy.deepcopy(FakeIdentity.clients['miy-web'])
        for callback, logout in ((True, False), (False, True), (True, True), (False, False)):
            with self.subTest(callback=callback, logout=logout):
                FakeIdentity.clients['miy-web'] = copy.deepcopy(original)
                client = FakeIdentity.clients['miy-web']
                if callback:
                    client['redirectUris'].insert(0, DEV + '/auth/callback')
                if logout:
                    client['attributes']['post.logout.redirect.uris'] = DEV + '/##' + \
                        client['attributes']['post.logout.redirect.uris']
                self.release = self.state / f'uris-{callback}-{logout}'
                self.apply()
                client['redirectUris'].append('https://another.example.test/auth/callback')
                client['webOrigins'].append('https://another.example.test')
                client['attributes']['other'] = 'preserved'
                client['attributes']['post.logout.redirect.uris'] += '##https://another.example.test/'
                tool.restore(self.state, self.release, identity=FakeIdentity)
                self.assertEqual(DEV + '/auth/callback' in client['redirectUris'], callback)
                self.assertEqual(DEV + '/' in tool.logout_uris(client), logout)
                self.assertIn('https://another.example.test/auth/callback', client['redirectUris'])
                self.assertIn('https://another.example.test/', tool.logout_uris(client))
                self.assertIn('https://another.example.test', client['webOrigins'])
                self.assertEqual(client['attributes']['other'], 'preserved')

    def test_restore_validates_all_backups_before_changing_identity_or_files(self):
        self.apply()
        before = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client_before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        (self.release / 'settings-backup' / 'compose.env').write_text('changed\n')
        with self.assertRaisesRegex(RuntimeError, 'settings_backup_changed:compose.env'):
            tool.restore(self.state, self.release, identity=FakeIdentity)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)
        self.assertEqual(FakeIdentity.clients['miy-web'], client_before)

    def test_restore_does_not_leave_a_new_logout_attribute(self):
        del FakeIdentity.clients['miy-web']['attributes']['post.logout.redirect.uris']
        before = copy.deepcopy(FakeIdentity.clients['miy-web'])
        self.apply()
        tool.restore(self.state, self.release, identity=FakeIdentity)
        self.assertEqual(FakeIdentity.clients['miy-web'], before)

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
        with self.assertRaisesRegex(RuntimeError, 'dev_portal_address_in_use'):
            tool.apply(self.state, self.release, DEV, '127.0.0.1:18183', identity=FakeIdentity, probe=lambda _p: False)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, before)

    def test_a_separate_hostname_shares_the_https_listener_unless_another_site_claims_it(self):
        enabled = self.root / 'sites-enabled'
        enabled.mkdir()
        (enabled / 'other').write_text('server {\n    listen 443 ssl;\n    server_name other.example.test;\n}\n')
        host = 'https://v2-dev.example.test'
        # The shared port is in use (probe False), but name-based hosting needs only the name.
        result = tool.apply(self.state, self.release, host, '127.0.0.1:18183', identity=FakeIdentity,
                            probe=lambda _p: False, enabled=enabled)
        self.assertEqual(result['devPortalOrigin'], host)
        self.assertIn('MIY_DEV_PORTAL_HOST=v2-dev.example.test', (self.state / 'compose.env').read_text())
        tool.restore(self.state, self.release, identity=FakeIdentity)
        shutil.rmtree(self.release)
        (enabled / 'claimed').write_text('server {\n    server_name v2-dev.example.test www.example.test;\n}\n')
        with self.assertRaisesRegex(RuntimeError, 'dev_portal_address_in_use'):
            tool.apply(self.state, self.release, host, '127.0.0.1:18183', identity=FakeIdentity,
                       probe=lambda _p: True, enabled=enabled)

    def test_hostname_check_handles_quotes_multiline_and_comments(self):
        enabled = self.root / 'sites-enabled'
        enabled.mkdir()
        site = enabled / 'other'
        for directive in (
                'server_name "v2-dev.example.test";',
                "server_name 'v2-dev.example.test' alias.example.test;",
                'server_name\n # comment\n "v2-dev.example.test"\n alias.example.test;',
                'server_name V2-DEV.EXAMPLE.TEST;'):
            with self.subTest(directive=directive):
                site.write_text('server { ' + directive + ' }')
                self.assertFalse(tool.name_unclaimed('v2-dev.example.test', enabled))
        site.write_text('# server_name "v2-dev.example.test";\nserver { server_name other.example.test; }')
        self.assertTrue(tool.name_unclaimed('v2-dev.example.test', enabled))
        site.write_text('server { server_name "unterminated; }')
        with self.assertRaisesRegex(RuntimeError, 'nginx_site_unparseable'):
            tool.name_unclaimed('v2-dev.example.test', enabled)

    def test_hostname_check_rejects_names_in_the_actual_rendered_config(self):
        rendered = self.root / 'rendered'
        rendered.mkdir()
        hosts = ('portal.example.test', 'console.example.test', 'dev.example.test')
        settings = {'origin': 'http://' + hosts[0], 'consoleOrigin': 'http://' + hosts[1],
                    'devPortalOrigin': 'http://' + hosts[2], 'allowHttp': True,
                    'bind': '127.0.0.1:18180', 'consoleBind': '127.0.0.1:18182',
                    'devPortalBind': '127.0.0.1:18183', 'identityPort': 18181}
        (rendered / 'server.json').write_text(json.dumps(settings))
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'),
                                 '--state-dir', str(rendered)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        enabled = self.root / 'sites-enabled'
        enabled.mkdir()
        shutil.copyfile(rendered / 'nginx-miy.conf', enabled / 'miy')
        for hostname in hosts:
            with self.subTest(hostname=hostname):
                self.assertFalse(tool.name_unclaimed(hostname, enabled))
        self.assertTrue(tool.name_unclaimed('unused.example.test', enabled))

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
