#!/usr/bin/env python3
"""Offline tests for scripts/console-origin-move.py: no Keycloak, Nginx, systemd or live state."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('console_origin_move', ROOT / 'scripts/console-origin-move.py')
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

PORTAL, OLD, NEW, DEV = 'https://v2.example.test', 'https://v2.example.test:9443', 'https://v2-console.example.test', 'https://v2-dev.example.test'


def site_text(console):
    host, _, port = console.split('://', 1)[1].partition(':')
    return (f'# header\nmap $a $b {{}}\nserver {{\n  portal\n  return 302 {console}/cli/$1/$2;\n}}\nserver {{\n  listen {port or 443} ssl;\n'
            f'  server_name "{host}";\n}}\nserver {{\n  dev\n}}\n')


class FakeIdentity:
    client = None
    realm_value = None
    secret = 's' * 64

    def __init__(self, state):
        pass

    def snapshot(self):
        return {'client': copy.deepcopy(FakeIdentity.client), 'clientSecretSha256': FakeIdentity.secret}

    def update(self, client_id, body):
        assert client_id == 'web-id'
        FakeIdentity.client.update(copy.deepcopy(body))

    def realm(self):
        return copy.deepcopy(FakeIdentity.realm_value)

    def update_realm(self, body):
        FakeIdentity.realm_value.update(copy.deepcopy(body))

    def discovery_issuer(self):
        return FakeIdentity.realm_value['attributes']['frontendUrl'] + '/realms/miy'


class FakeCore:
    def __init__(self, test):
        self.test = test
        self.events = []
        self.running = True
        self.fail_health = False

    def require_core_state(self, expected):
        assert expected == 'active' and self.running

    def idle(self):
        return {'runtime': 0}

    def service_states(self):
        return {'serve': 'active' if self.running else 'inactive'}

    def stop_core(self):
        self.events.append('stop')
        self.running = False

    def start_core(self):
        self.events.append('start')
        self.running = True

    def wait(self, check, _seconds, failure):
        if not check():
            raise RuntimeError(failure)

    def get_json(self, url):
        settings = json.loads((self.test.state / 'server.json').read_text())
        console = settings['consoleOrigin']
        if url.endswith('/api/health'):
            served = url.startswith((PORTAL, console)) and not (self.fail_health and console == NEW)
            return {'status': 'ok'} if self.running and served else {'status': 'down'}
        if url.endswith('/auth/options'):
            return {'consoleOrigin': console}
        if url.endswith('/oauth-protected-resource/mcp'):
            return {'authorization_servers': [settings_runtime(self.test.state)['MIY_ISSUER']]}
        raise AssertionError(url)


def settings_runtime(state):
    return tool.env_values((state / 'runtime.env').read_bytes())


class ConsoleMoveTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='miy-console-move-'))
        self.state = self.root / 'state'
        (self.state / 'bin').mkdir(parents=True)
        settings = {'origin': PORTAL, 'consoleOrigin': OLD, 'devPortalOrigin': DEV, 'bind': '127.0.0.1:18180',
                    'consoleBind': '127.0.0.1:18182', 'devPortalBind': '127.0.0.1:18183'}
        (self.state / 'server.json').write_text(json.dumps(settings, indent=2) + '\n')
        (self.state / 'runtime.env').write_text(f"DATABASE_URL='private-sentinel'\nMIY_CONSOLE_ORIGIN={OLD}\n"
                                                f"MIY_ISSUER={OLD}/identity/realms/miy\n")
        (self.state / 'compose.env').write_text(f"MIY_PG_ADMIN_PASSWORD=private-sentinel\nMIY_CONSOLE_ORIGIN={OLD}\n"
                                                "MIY_CONSOLE_HOST=v2.example.test:9443\nMIY_CONSOLE_PORT=9443\n")
        web = {'clientId': 'miy-web', 'redirectUris': [p + '/auth/callback' for p in (PORTAL, OLD, DEV)],
               'webOrigins': [PORTAL, OLD], 'attributes': {'post.logout.redirect.uris': f'{PORTAL}/##{OLD}/##{DEV}/'}}
        (self.state / 'realm.json').write_text(json.dumps({'realm': 'miy', 'attributes': {'frontendUrl': OLD + '/identity'},
                                                           'clients': [copy.deepcopy(web)]}, indent=2) + '\n')
        for name in tool.FILES:
            (self.state / name).chmod(0o600)
        FakeIdentity.client = {'id': 'web-id', **copy.deepcopy(web)}
        FakeIdentity.realm_value = {'realm': 'miy', 'attributes': {'frontendUrl': OLD + '/identity', 'other': 'kept'}}
        self.site = self.root / 'site.conf'
        self.site.write_text(site_text(OLD))
        (self.state / 'nginx-miy.conf').write_text(site_text(OLD))
        self.enabled = self.root / 'enabled'
        self.enabled.mkdir()
        self.legacy_site = self.root / 'legacy.conf'
        self.legacy_link = self.enabled / 'legacy-link'
        self.release = self.state / 'release-console'
        self.core = FakeCore(self)
        self.nginx_ok = True

    def tearDown(self):
        shutil.rmtree(self.root)

    def fake_run(self, args, **_kwargs):
        args = [str(a) for a in args]
        if args[1:2] and args[1].endswith('render-server-config.py'):
            work = Path(args[args.index('--state-dir') + 1])
            settings = json.loads((work / 'server.json').read_text())
            (work / 'nginx-miy.conf').write_text(site_text(settings['consoleOrigin']))
        elif args[0] == 'cat':
            return Path(args[1]).read_text()
        elif args[0] == 'install':
            shutil.copyfile(args[-2], args[-1])
        elif args[0] == 'ln':
            Path(args[-1]).unlink(missing_ok=True)
            Path(args[-1]).symlink_to(args[-2])
        elif args[0] == 'rm' and args[1] == '-f':
            for item in args[2:]:
                Path(item).unlink(missing_ok=True)
        elif args[0] == 'rm':
            shutil.rmtree(args[-1], ignore_errors=True)
        elif args[0] == 'nginx' and not self.nginx_ok and 'v2-console' in self.site.read_text():
            raise RuntimeError('command_failed:nginx')
        return ''

    def options(self):
        return dict(core=self.core, identity=FakeIdentity, site=self.site, sudo=(), enabled=self.enabled)

    def apply(self):
        with patch.object(tool, 'run', self.fake_run):
            return tool.apply(self.state, self.release, NEW, 'cert', 'key', legacy_site_path=self.legacy_site,
                              legacy_link=self.legacy_link, **self.options())

    def test_move_replaces_only_console_entries_and_redirects_the_old_origin(self):
        before_env = (self.state / 'runtime.env').read_text()
        result = self.apply()
        self.assertEqual((result['status'], result['to']), ('MOVED', NEW))
        self.assertEqual(self.core.events, ['stop', 'start'])
        client = FakeIdentity.client
        self.assertEqual(client['redirectUris'], [p + '/auth/callback' for p in (PORTAL, NEW, DEV)])
        self.assertEqual(client['webOrigins'], [PORTAL, NEW])
        self.assertEqual(client['attributes']['post.logout.redirect.uris'], f'{PORTAL}/##{NEW}/##{DEV}/')
        self.assertEqual(FakeIdentity.realm_value['attributes'], {'frontendUrl': NEW + '/identity', 'other': 'kept'})
        runtime = settings_runtime(self.state)
        self.assertEqual((runtime['MIY_CONSOLE_ORIGIN'], runtime['MIY_ISSUER']), (NEW, NEW + '/identity/realms/miy'))
        self.assertEqual(runtime['DATABASE_URL'], 'private-sentinel')
        self.assertEqual(len((self.state / 'runtime.env').read_text().splitlines()), len(before_env.splitlines()))
        self.assertIn('v2-console.example.test', self.site.read_text())
        self.assertIn(f'return 301 {NEW}$request_uri;', self.legacy_site.read_text())
        self.assertTrue(self.legacy_link.is_symlink())
        realm = json.loads((self.state / 'realm.json').read_text())
        self.assertEqual(realm['attributes']['frontendUrl'], NEW + '/identity')
        self.assertNotIn('private-sentinel', (self.release / 'console-move.json').read_text())

    def test_failed_start_restores_identity_files_and_nginx_on_the_old_origin(self):
        files = {name: (self.state / name).read_bytes() for name in tool.FILES}
        client, realm = copy.deepcopy(FakeIdentity.client), copy.deepcopy(FakeIdentity.realm_value)
        self.core.fail_health = True
        with self.assertRaisesRegex(RuntimeError, 'rolled_back:console_health_failed'):
            self.apply()
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, files)
        self.assertEqual((FakeIdentity.client, FakeIdentity.realm_value), (client, realm))
        self.assertEqual(self.site.read_text(), site_text(OLD))
        self.assertFalse(self.legacy_link.exists() or self.legacy_site.exists())
        self.assertEqual(self.core.events, ['stop', 'start', 'stop', 'start'])
        self.assertTrue(self.core.running)

    def test_failed_nginx_test_restores_before_the_core_starts_on_the_new_origin(self):
        self.nginx_ok = False
        files = {name: (self.state / name).read_bytes() for name in tool.FILES}
        with self.assertRaisesRegex(RuntimeError, 'rolled_back:command_failed:nginx'):
            self.apply()
        self.assertEqual(FakeIdentity.realm_value['attributes']['frontendUrl'], OLD + '/identity')
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, files)
        self.assertEqual(self.site.read_text(), site_text(OLD))
        # The core never started on the new origin; it starts once, on the old one.
        self.assertEqual(self.core.events, ['stop', 'start'])

    def test_restore_after_a_move_returns_everything(self):
        files = {name: (self.state / name).read_bytes() for name in tool.FILES}
        self.apply()
        with patch.object(tool, 'run', self.fake_run):
            result = tool.full_restore(self.state, self.release, self.core, FakeIdentity, self.site, (),
                                       self.legacy_site, self.legacy_link)
        self.assertEqual(result['status'], 'RESTORED')
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, files)
        self.assertEqual(FakeIdentity.client['webOrigins'], [PORTAL, OLD])
        self.assertFalse(self.legacy_link.exists())

    def test_refusals_change_nothing(self):
        (self.enabled / 'other').write_text('server {\n  server_name v2-claimed.example.test;\n}\n')
        files = {name: (self.state / name).read_bytes() for name in tool.FILES}
        for origin, code in [(PORTAL, 'console_origin_must_be_distinct'), (DEV, 'console_origin_must_be_distinct'),
                             ('http://v2-console.example.test', 'console_origin_must_be_distinct'),
                             ('https://v2-claimed.example.test', 'console_hostname_unavailable'),
                             (OLD, 'console_already_there')]:
            with self.subTest(origin=origin), patch.object(tool, 'run', self.fake_run), \
                    self.assertRaisesRegex(RuntimeError, code):
                tool.apply(self.state, self.release, origin, 'cert', 'key', legacy_site_path=self.legacy_site,
                           legacy_link=self.legacy_link, **self.options())
        self.site.write_text(site_text(OLD) + '# manual edit\n')
        with patch.object(tool, 'run', self.fake_run), self.assertRaisesRegex(RuntimeError, 'live_nginx_differs'):
            self.apply()
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, files)
        self.assertEqual(self.core.events, [])
        self.assertFalse(self.release.exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
