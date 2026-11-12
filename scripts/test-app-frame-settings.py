#!/usr/bin/env python3
"""Offline tests for scripts/app-frame-settings.py: no Nginx, sudo, services or live state."""
import getpass
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
spec = importlib.util.spec_from_file_location('app_frame_settings', ROOT / 'scripts/app-frame-settings.py')
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

PORTAL, CONSOLE, DEV = 'https://v2.example.test', 'https://v2-console.example.test', 'https://v2-dev.example.test'
TEMPLATE, BIND = 'https://v2-app-{key}.example.test', '127.0.0.1:18184'
SOURCES = 'https://*.example.test'
CSP = 'add_header Content-Security-Policy "frame-ancestors \'none\'; worker-src \'none\'" always;'
CONSOLE_CSP = 'add_header Content-Security-Policy "default-src \'self\'; frame-ancestors \'none\'" always;'
LIVE_NGINX = ('# Generated from fixed portal/console/dev-portal settings.\nmap $x $y {}\n'
              f'server {{\n  portal\n    {CSP}\n}}\nserver {{\n  console\n    {CONSOLE_CSP}\n}}\n'
              f'server {{\n  dev\n    {CSP}\n}}\n')
FRAME_BLOCK = ('server {\n    listen 443 ssl;\n    access_log off;\n    error_log /dev/null;\n    location / {\n'
               '        proxy_pass http://127.0.0.1:18184;\n    }\n}\n')
REFERRER = '    add_header Referrer-Policy strict-origin-when-cross-origin always;\n'


def candidate_nginx(console_changed=False, extra=''):
    portal_csp = CSP.replace("'none'\" always;", f"'none'; frame-src {SOURCES} blob:\" always;")
    console = CONSOLE_CSP.replace('default-src', 'default-src https:') if console_changed else CONSOLE_CSP
    return ('# Generated from fixed portal/console/dev-portal settings.\nmap $x $y {}\n'
            f'server {{\n  portal\n    {portal_csp}\n{REFERRER}}}\nserver {{\n  console\n    {console}\n}}\n'
            f'server {{\n  dev\n    {portal_csp}\n{REFERRER}}}\n' + FRAME_BLOCK + extra)


class AppFrameSettingsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='miy-app-frame-test-'))
        self.state = self.root / 'state'
        self.state.mkdir(mode=0o700)
        settings = {'origin': PORTAL, 'consoleOrigin': CONSOLE, 'devPortalOrigin': DEV, 'bind': '127.0.0.1:18180',
                    'consoleBind': '127.0.0.1:18182', 'devPortalBind': '127.0.0.1:18183'}
        (self.state / 'server.json').write_text(json.dumps(settings, indent=2) + '\n')
        (self.state / 'runtime.env').write_text("DATABASE_URL='private-sentinel'\nMIY_ORIGIN=" + PORTAL + '\n')
        (self.state / 'compose.env').write_text('MIY_PG_ADMIN_PASSWORD=private-sentinel\nMIY_ORIGIN=' + PORTAL + '\n')
        for name in tool.FILES:
            (self.state / name).chmod(0o600)
        self.enabled = self.root / 'sites-enabled'
        self.enabled.mkdir()
        (self.enabled / 'other').write_text('server {\n    server_name other.example.test;\n}\n')
        self.release = self.state / 'release-app-frames'
        self.before = {name: (self.state / name).read_bytes() for name in tool.FILES}

    def tearDown(self):
        shutil.rmtree(self.root)

    def apply(self, template=TEMPLATE, bind=BIND, ports=None, probe=lambda _port: True):
        return tool.apply(self.state, self.release, template, bind, ports, None, probe=probe, enabled=self.enabled)

    def unchanged(self):
        for name, value in self.before.items():
            self.assertEqual((self.state / name).read_bytes(), value, name)

    def test_apply_adds_only_frame_settings_and_keeps_every_other_line_and_secret(self):
        result = self.apply()
        self.assertEqual((result['status'], result['sampleOrigin'], result['frameSources']),
                         ('APPLIED', 'https://v2-app-' + '0' * 32 + '.example.test', SOURCES))
        self.assertNotIn('private-sentinel', json.dumps(result))
        settings = json.loads((self.state / 'server.json').read_text())
        self.assertEqual((settings['appFrameOrigin'], settings['appFrameBind']), (TEMPLATE, BIND))
        self.assertNotIn('appFramePorts', settings)
        runtime = tool.env_values((self.state / 'runtime.env').read_bytes())
        self.assertEqual(runtime['DATABASE_URL'], 'private-sentinel')
        self.assertEqual((runtime['MIY_APP_FRAME_ORIGIN'], runtime['MIY_APP_FRAME_BIND_ADDR'],
                          runtime['MIY_FRAME_RUNTIME_DIR']), (TEMPLATE, BIND, str(self.state / 'frame-runtime')))
        self.assertNotIn('MIY_APP_FRAME_PORTS', runtime)
        self.assertTrue((self.state / 'runtime.env').read_text().startswith(self.before['runtime.env'].decode()))
        compose = tool.env_values((self.state / 'compose.env').read_bytes())
        self.assertEqual((compose['MIY_APP_FRAME_SITES'], compose['MIY_APP_FRAME_SOURCES'],
                          compose['MIY_APP_FRAME_CORE_UPSTREAM']), (SOURCES, SOURCES, BIND))
        for name in tool.FILES:
            self.assertEqual((self.state / name).stat().st_mode & 0o777, 0o600)
            self.assertEqual((self.release / 'settings-backup' / name).read_bytes(), self.before[name])
        self.assertNotIn('private-sentinel', (self.release / 'settings.json').read_text())

    def test_a_port_pool_skips_browser_bad_ports_and_lists_every_caddy_site(self):
        settings = json.loads((self.state / 'server.json').read_text())
        result = self.apply(template='https://v2.example.test:{port}', ports='6665-6670')
        self.assertEqual(result['sampleOrigin'], 'https://v2.example.test:6670')
        compose = tool.env_values((self.state / 'compose.env').read_bytes())
        self.assertEqual(compose['MIY_APP_FRAME_SITES'], 'https://v2.example.test:6670')
        self.assertEqual(compose['MIY_APP_FRAME_SOURCES'], 'https://v2.example.test:*')
        runtime = tool.env_values((self.state / 'runtime.env').read_bytes())
        self.assertEqual(runtime['MIY_APP_FRAME_PORTS'], '6665-6670')
        self.assertEqual(json.loads((self.state / 'server.json').read_text())['appFramePorts'], '6665-6670')
        self.assertEqual(settings['origin'], PORTAL)

    def test_restore_returns_every_file_exactly(self):
        self.apply()
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.unchanged()

    def test_recovery_record_and_all_backups_exist_before_any_live_change(self):
        write = tool.private_write

        def inspect_write(path, data):
            if path.parent == self.state:
                record = json.loads((self.release / 'settings.json').read_text())
                self.assertEqual(record['phase'], 'prepared')
                self.assertEqual(set(record['files']), set(tool.FILES))
                for name in tool.FILES:
                    self.assertEqual((self.release / 'settings-backup' / name).read_bytes(), self.before[name])
                    self.assertEqual(record['files'][name]['before'], tool.digest(self.before[name]))
            write(path, data)

        with patch.object(tool, 'private_write', inspect_write):
            self.apply()
        self.assertEqual(json.loads((self.release / 'settings.json').read_text())['phase'], 'applied')
        self.assertEqual((self.release / 'settings.json').stat().st_mode & 0o777, 0o600)

    def test_backup_or_prepared_manifest_failure_changes_no_live_files(self):
        write = tool.private_write
        for failed in ['settings-backup/runtime.env', 'settings.json']:
            with self.subTest(failed=failed):
                self.release = self.state / ('release-' + failed.replace('/', '-'))

                def fail_write(path, data):
                    if path == self.release / failed:
                        raise OSError('synthetic preparation failure')
                    write(path, data)

                with patch.object(tool, 'private_write', fail_write), self.assertRaises(OSError):
                    self.apply()
                self.unchanged()

    def test_recovery_parent_sync_failure_changes_no_live_files(self):
        self.release = self.state / 'new-releases' / 'app-frames'
        parent = self.state.stat()
        fsync = tool.os.fsync

        def fail_parent_sync(descriptor):
            current = tool.os.fstat(descriptor)
            if (current.st_dev, current.st_ino) == (parent.st_dev, parent.st_ino):
                raise OSError('synthetic recovery directory sync failure')
            fsync(descriptor)

        for attempt in range(2):
            with self.subTest(attempt=attempt):
                with patch.object(tool.os, 'fsync', fail_parent_sync), self.assertRaises(OSError):
                    self.apply()
                self.unchanged()

    def test_interrupted_apply_can_be_restored_and_applied_in_a_new_release(self):
        write = tool.private_write

        def fail_write(path, data):
            if path == self.state / 'runtime.env':
                raise OSError('synthetic live write failure')
            write(path, data)

        with patch.object(tool, 'private_write', fail_write), self.assertRaises(OSError):
            self.apply()
        self.assertNotEqual((self.state / 'server.json').read_bytes(), self.before['server.json'])
        self.assertEqual((self.state / 'runtime.env').read_bytes(), self.before['runtime.env'])
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.unchanged()
        self.release = self.state / 'release-retry'
        self.assertEqual(self.apply()['status'], 'APPLIED')

    def test_applied_manifest_failure_keeps_prepared_record_restorable(self):
        write = tool.private_write

        def fail_write(path, data):
            if path == self.release / 'settings.json' and json.loads(data)['phase'] == 'applied':
                raise OSError('synthetic final record failure')
            write(path, data)

        with patch.object(tool, 'private_write', fail_write), self.assertRaises(OSError):
            self.apply()
        self.assertEqual(json.loads((self.release / 'settings.json').read_text())['phase'], 'prepared')
        self.assertTrue(all((self.state / name).read_bytes() != self.before[name] for name in tool.FILES))
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.unchanged()

    def test_interrupted_restore_can_be_retried_including_its_final_record(self):
        self.apply()
        write = tool.private_write
        for failed in [self.state / 'runtime.env', self.release / 'settings.json']:
            with self.subTest(failed=failed.name):
                def fail_write(path, data):
                    if path == failed:
                        raise OSError('synthetic restore failure')
                    write(path, data)

                with patch.object(tool, 'private_write', fail_write), self.assertRaises(OSError):
                    tool.restore(self.state, self.release)
                self.assertEqual((self.state / 'server.json').read_bytes(), self.before['server.json'])
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.unchanged()
        self.assertEqual(json.loads((self.release / 'settings.json').read_text())['phase'], 'restored')

    def test_restore_retries_sync_after_bytes_were_already_restored(self):
        self.apply()
        fsync = tool.os.fsync

        def fail_restored_file_sync(descriptor):
            current = tool.os.fstat(descriptor)
            restored = (self.state / 'runtime.env').stat()
            if (current.st_dev, current.st_ino) == (restored.st_dev, restored.st_ino):
                raise OSError('synthetic restored file sync failure')
            fsync(descriptor)

        for attempt in range(2):
            with self.subTest(attempt=attempt):
                with patch.object(tool.os, 'fsync', fail_restored_file_sync), self.assertRaises(OSError):
                    tool.restore(self.state, self.release)
        self.assertEqual(tool.restore(self.state, self.release)['status'], 'RESTORED')
        self.unchanged()

    def test_restore_validates_complete_manifest_and_all_backups_before_changing_files(self):
        self.apply()
        after = {name: (self.state / name).read_bytes() for name in tool.FILES}
        manifest = self.release / 'settings.json'
        original = manifest.read_bytes()
        record = json.loads(original)
        del record['files']['compose.env']
        manifest.write_text(json.dumps(record))
        with self.assertRaisesRegex(RuntimeError, 'settings_manifest_incomplete'):
            tool.restore(self.state, self.release)
        manifest.write_bytes(original)
        (self.release / 'settings-backup/compose.env').write_text('changed fixture backup')
        with self.assertRaisesRegex(RuntimeError, 'settings_backup_changed:compose.env'):
            tool.restore(self.state, self.release)
        self.assertEqual({name: (self.state / name).read_bytes() for name in tool.FILES}, after)

    def test_restore_refuses_a_file_changed_after_apply(self):
        self.apply()
        with (self.state / 'runtime.env').open('a') as handle:
            handle.write('MIY_EXTRA=1\n')
        with self.assertRaisesRegex(RuntimeError, 'settings_changed_since_apply:runtime.env'):
            tool.restore(self.state, self.release)

    def test_invalid_or_conflicting_targets_change_nothing(self):
        cases = {
            'app_frame_origin_needs_one_key_or_port': ('https://v2-app-{key}.example.test:{port}', BIND, '1-2'),
            'app_frame_origin_must_use_the_portal_transport': ('http://v2-app-{key}.example.test', BIND, None),
            'app_frame_key_must_be_in_the_first_host_label': ('https://apps.{key}.example.test', BIND, None),
            'app_frame_ports_need_a_port_template': (TEMPLATE, BIND, '20000-20010'),
            'app_frame_port_must_be_the_origin_port': ('https://v2.example.test/{port}', BIND, '1-2'),
            'invalid_app_frame_port_pool': ('https://v2.example.test:{port}', BIND, '20010-20000'),
            'app_frame_port_pool_overlaps_a_platform_port': ('https://v2.example.test:{port}', BIND, '400-500'),
            'app_frame_bind_must_be_loopback': (TEMPLATE, '0.0.0.0:18184', None),
            'app_frame_bind_must_be_distinct': (TEMPLATE, '127.0.0.1:18183', None),
        }
        for code, (template, bind, ports) in cases.items():
            with self.subTest(code), self.assertRaisesRegex(RuntimeError, code):
                self.apply(template, bind, ports)
            self.unchanged()
            self.assertFalse(self.release.exists())

    def test_different_frames_already_configured_are_refused(self):
        settings = json.loads((self.state / 'server.json').read_text())
        settings.update({'appFrameOrigin': 'https://other-{key}.example.test', 'appFrameBind': BIND})
        (self.state / 'server.json').write_text(json.dumps(settings))
        with self.assertRaisesRegex(RuntimeError, 'different_app_frames_already_configured'):
            self.apply()

    def test_busy_addresses_and_claimed_frame_names_are_refused(self):
        with self.assertRaisesRegex(RuntimeError, 'app_frame_address_in_use'):
            self.apply(probe=lambda port: port != 18184)
        with self.assertRaisesRegex(RuntimeError, 'app_frame_address_in_use'):
            self.apply(template='https://v2.example.test:{port}', ports='20000-20003',
                       probe=lambda port: port != 20002)
        for directive in ('server_name v2-app-old.example.test;', 'server_name\n  other.example.test\n  "V2-APP-old.example.test";'):
            (self.enabled / 'claimed').write_text('server {\n    ' + directive + '\n}\n')
            with self.subTest(directive=directive), self.assertRaisesRegex(RuntimeError, 'app_frame_address_in_use'):
                self.apply()
        (self.enabled / 'claimed').write_text('server { server_name "unterminated; }')
        with self.assertRaisesRegex(RuntimeError, 'nginx_site_unparseable'):
            self.apply()
        self.unchanged()

    def test_inspect_reports_public_settings_only(self):
        result = tool.inspect(self.state, TEMPLATE, BIND, probe=lambda _port: True, enabled=self.enabled)
        self.assertEqual((result['status'], result['frameRuntimeIndex']), ('READY', False))
        self.assertNotIn('private-sentinel', json.dumps(result))
        self.unchanged()

    def nginx(self, candidate):
        site = self.root / 'site.conf'
        site.write_text(LIVE_NGINX)
        (self.state / 'nginx-miy.conf').write_text(LIVE_NGINX)
        self.apply()
        calls = []

        def fake_run(args, **_kwargs):
            args = [str(a) for a in args]
            calls.append(args[0] if args[0] != 'install' else 'install')
            if args[1].endswith('render-server-config.py'):
                work = Path(args[args.index('--state-dir') + 1])
                (work / 'nginx-miy.conf').write_text(candidate)
                return ''
            if args[0] == 'cat':
                return Path(args[1]).read_text()
            if args[0] == 'install':
                shutil.copyfile(args[-2], args[-1])
                return ''
            if args[0] == 'nginx' and getattr(self, 'nginx_fails', False):
                raise RuntimeError('command_failed:nginx')
            if args[0] == 'rm':
                shutil.rmtree(args[-1])
            return ''
        return site, calls, fake_run

    def test_nginx_apply_adds_the_frame_block_and_portal_headers_and_restore_reinstalls(self):
        site, calls, fake_run = self.nginx(candidate_nginx())
        with patch.object(tool, 'run', fake_run):
            result = tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
            self.assertEqual((result['status'], result['portalHeaderLocations']), ('NGINX_APPLIED', 2))
            self.assertIn('proxy_pass http://127.0.0.1:18184', site.read_text())
            self.assertIn('systemctl', calls)
            self.assertEqual(tool.nginx_restore(self.state, self.release, site=site, sudo=())['status'],
                             'NGINX_RESTORED')
        self.assertEqual(site.read_text(), LIVE_NGINX)
        self.assertEqual((self.state / 'nginx-miy.conf').read_text(), LIVE_NGINX)

    def test_nginx_apply_refuses_unexpected_changes_without_installing(self):
        for code, candidate in {
            'rendered_nginx_changes_more_than_app_frames': candidate_nginx(console_changed=True),
            'rendered_nginx_changes_more_than_app_frames ': candidate_nginx(extra=FRAME_BLOCK),
            'rendered_frame_block_unexpected': candidate_nginx().replace(
                '    access_log off;\n', '    add_header X-Frame-Options DENY always;\n'),
            'rendered_frame_block_unexpected ': candidate_nginx().replace('    error_log /dev/null;\n', ''),
            'portal_frame_headers_missing': candidate_nginx().replace(REFERRER, '', 1),
        }.items():
            with self.subTest(code):
                shutil.rmtree(self.release, ignore_errors=True)
                for name, value in self.before.items():
                    (self.state / name).write_bytes(value)
                site, calls, fake_run = self.nginx(candidate)
                with patch.object(tool, 'run', fake_run), self.assertRaisesRegex(RuntimeError, code.strip()):
                    tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
                self.assertEqual(site.read_text(), LIVE_NGINX)
                self.assertNotIn('install', calls)

    def test_nginx_test_failure_reinstalls_the_live_file_without_reload(self):
        site, calls, fake_run = self.nginx(candidate_nginx())
        self.nginx_fails = True
        with patch.object(tool, 'run', fake_run), self.assertRaisesRegex(RuntimeError, 'nginx_test_failed_restored'):
            tool.nginx_apply(self.state, self.release, 'cert', 'key', site=site, sudo=())
        self.assertEqual(site.read_text(), LIVE_NGINX)
        self.assertNotIn('systemctl', calls)

    def test_the_frame_diff_check_matches_the_real_renderer(self):
        """Render real configs (loopback HTTP), strip the frame additions, and accept the result."""
        for template, ports, sources in [('http://v2-app-{key}.localhost:18000', None, 'http://*.localhost:18000'),
                                         ('http://localhost:{port}', '29000-29009', 'http://localhost:*')]:
            with self.subTest(template):
                state = Path(tempfile.mkdtemp(dir=self.root))
                settings = {'origin': 'http://localhost:18000', 'consoleOrigin': 'http://localhost:18001',
                            'devPortalOrigin': 'http://localhost:18002', 'bind': '127.0.0.1:18080',
                            'consoleBind': '127.0.0.1:18081', 'devPortalBind': '127.0.0.1:18082',
                            'appFrameBind': '127.0.0.1:18083', 'identityPort': 18181, 'appFrameOrigin': template}
                if ports:
                    settings['appFramePorts'] = ports
                (state / 'server.json').write_text(json.dumps(settings))
                subprocess.run([sys.executable, ROOT / 'scripts/render-server-config.py', '--state-dir', state,
                                '--binary', state / 'miy-server', '--user', getpass.getuser()],
                               check=True, capture_output=True)
                candidate = (state / 'nginx-miy.conf').read_text()
                blocks = candidate.split('\nserver {')
                live = '\n'.join(line.replace(f'; frame-src {sources} blob:', '')
                                 for line in '\nserver {'.join(blocks[:-1]).split('\n')
                                 if 'Referrer-Policy' not in line) + '\n'
                self.assertNotEqual(live, candidate)
                change = tool.frame_only_change(live, candidate, sources)
                self.assertGreaterEqual(change['portalHeaderLocations'], 2)
                self.assertEqual(tool.frame_pattern(template, ports, settings['origin'])['sources'], sources)
                console = [block for block in blocks if 'listen 18001;' in block]
                self.assertTrue(console and all('frame-src' not in block for block in console))


if __name__ == '__main__':
    unittest.main(verbosity=2)
