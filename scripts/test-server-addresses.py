#!/usr/bin/env python3
"""Address profiles, candidate generation and real isolated frame migration SQL."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

from server_addresses import validate_addresses

ROOT = Path(__file__).resolve().parent.parent
ENV = {key: value for key, value in os.environ.items() if not key.startswith('MIY_')}
spec = importlib.util.spec_from_file_location('frame_move', ROOT / 'scripts/plan-frame-origin-move.py')
move = importlib.util.module_from_spec(spec)
spec.loader.exec_module(move)


def addresses(host='192.0.2.10', scheme='https'):
    return {'origin': f'{scheme}://{host}:8443', 'consoleOrigin': f'{scheme}://{host}:9443',
            'devPortalOrigin': f'{scheme}://{host}:9444', 'appFrameOrigin': f'{scheme}://{host}:{{port}}',
            'appFramePorts': '20000-20039', 'allowHttp': scheme == 'http'}


def run(*command, **kwargs):
    return subprocess.run(list(map(str, command)), capture_output=True, text=True, timeout=60, **kwargs)


class Addresses(unittest.TestCase):
    def test_profiles_and_collisions(self):
        for host in ('192.0.2.10', '[2001:db8::10]', 'localhost', 'platform.example'):
            for scheme in ('http', 'https'):
                self.assertEqual(len(validate_addresses(addresses(host, scheme))['pool']), 40)
        base = addresses()
        cases = [dict(base, appFramePorts='8442-8444'), dict(base, appFramePorts='22-22'),
                 dict(base, appFramePorts='20002-20001'), dict(base, appFramePorts='20000-70000'),
                 dict(base, appFrameOrigin='https://other.example/{port}'),
                 dict(base, consoleOrigin=base['origin']),
                 dict(base, consoleOrigin='http://192.0.2.10:9443'),
                 dict(base, appFrameOrigin='https://app-{key}.example', appFramePorts='20000-20039'),
                 dict(base, appFrameOrigin='https://{key}.apps.example/path', appFramePorts=None),
                 dict(base, appFrameOrigin='https://app.{key}.example', appFramePorts=None),
                 dict(base, appFrameOrigin='https://app-{key}.example', appFramePorts=None,
                      consoleOrigin='https://app-' + '1' * 32 + '.example')]
        for case in cases:
            with self.subTest(case=case), self.assertRaises(ValueError):
                validate_addresses(case)

    def test_bootstrap_candidate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            state, candidate, config = root / 'state', root / 'candidate', root / 'addresses.json'
            config.write_text(json.dumps(addresses('[2001:db8::10]')))
            command = [sys.executable, ROOT / 'scripts/bootstrap.py', '--profile', 'server',
                       '--state-dir', state, '--addresses-file', config]
            checked = run(*command, '--check', env={**ENV, 'MIY_ORIGIN': 'https://wrong.example'})
            self.assertEqual(checked.returncode, 0, checked.stderr)
            self.assertFalse(state.exists())
            self.assertEqual(json.loads(checked.stdout)['mcp'], 'https://[2001:db8::10]:8443/mcp')
            installed = run(*command, env=ENV)
            self.assertEqual(installed.returncode, 0, installed.stderr)
            with (state / 'runtime.env').open('a') as handle:
                handle.write('MIY_DATABASE_CONTAINER=custom-postgres\n')
            original = {path.name: path.read_bytes() for path in state.iterdir() if path.is_file()}
            target = {**addresses(scheme='http'), 'origin': 'http://portal.example', 'consoleOrigin': 'http://console.example',
                      'devPortalOrigin': 'http://dev.example', 'appFrameOrigin': 'http://app-{key}.example',
                      'appFramePorts': None}
            config.write_text(json.dumps(target))
            prepared = run(*command, '--output-dir', candidate, env=ENV)
            self.assertEqual(prepared.returncode, 0, prepared.stderr)
            self.assertEqual(original, {path.name: path.read_bytes() for path in state.iterdir() if path.is_file()})
            self.assertEqual(json.loads((candidate / 'credentials.json').read_text()),
                             json.loads(original['credentials.json']))
            runtime = (candidate / 'runtime.env').read_text()
            self.assertIn('MIY_DATABASE_CONTAINER=custom-postgres\n', runtime)
            self.assertIn('MIY_ARTIFACTS=' + str(state / 'artifacts'), runtime)
            self.assertNotIn('MIY_APP_FRAME_PORTS=', runtime)
            self.assertNotIn(str(candidate), runtime)
            rendered = run(sys.executable, ROOT / 'scripts/render-server-config.py', '--state-dir', state,
                           '--settings-dir', candidate, env=ENV)
            self.assertEqual(rendered.returncode, 0, rendered.stderr)
            unit = (candidate / 'miy-v2-serve.service').read_text()
            self.assertIn('MIY_STATE_DIR=' + str(state), unit)
            self.assertNotIn(str(candidate), unit)
            proxy = (candidate / 'nginx-miy.conf').read_text()
            self.assertIn('proxy_set_header X-Forwarded-Host "portal.example";', proxy)
            self.assertIn('return 302 http://console.example/cli/', proxy)
            self.assertNotIn('2001:db8', proxy)
            self.assertEqual(run(*command, '--output-dir', candidate, env=ENV).returncode, 2)
            config.write_text(json.dumps({**target, 'typo': True}))
            self.assertEqual(run(*command, '--check', env=ENV).returncode, 2)
            # Default no-domain IPv6 frame construction must retain brackets.
            default = run(sys.executable, ROOT / 'scripts/bootstrap.py', '--profile', 'server',
                          '--state-dir', root / 'new', '--origin', 'https://[2001:db8::20]:8443', '--check', env=ENV)
            self.assertEqual(default.returncode, 0, default.stderr)
            self.assertEqual(json.loads(default.stdout)['appFrameOrigin'], 'https://[2001:db8::20]:{port}')

    def test_pool_history(self):
        installation = str(uuid.uuid4())
        rows = [{'origin': 'https://192.0.2.9:20000', 'installation_id': installation, 'port': 20000},
                {'origin': 'https://192.0.2.9:20001', 'installation_id': None, 'port': 20001}]
        _, changes = move.plan(addresses(), rows)
        self.assertEqual(changes[0]['target_port'], 20002)
        with self.assertRaises(ValueError):
            move.plan({**addresses(), 'appFramePorts': '20000-20001'}, rows)
        with self.assertRaises(ValueError):
            move.plan(addresses(), rows + [{'origin': 'https://192.0.2.10:20002',
                                           'installation_id': None, 'port': None}])

    def test_real_database_move(self):
        created = run('docker', 'run', '--rm', '-d', '--network', 'none', '--tmpfs', '/var/lib/postgresql',
                      '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', 'postgres:18.3')
        self.assertEqual(created.returncode, 0, created.stderr)
        container = created.stdout.strip()
        def sql(value):
            return run('docker', 'exec', '-i', container, 'psql', '-X', '-U', 'postgres', '-d', 'postgres',
                       '-v', 'ON_ERROR_STOP=1', '-At', input=value)
        try:
            for _ in range(60):
                logs = run('docker', 'logs', container)
                initialized = 'PostgreSQL init process complete; ready for start up.' in logs.stdout
                if initialized and run('docker', 'exec', container, 'pg_isready', '-U', 'postgres').returncode == 0:
                    break
                time.sleep(0.2)
            else:
                self.fail('Isolated PostgreSQL did not finish initialization')
            scaffold = '''CREATE TABLE installations(id uuid PRIMARY KEY);
CREATE TABLE users(id text PRIMARY KEY);
CREATE TABLE sessions(id_hash text PRIMARY KEY);
CREATE TABLE development_sessions(id_hash text PRIMARY KEY);
CREATE TABLE releases(id uuid PRIMARY KEY);
'''
            schema = sql(scaffold + (ROOT / 'migrations/core/0022_app_frames.sql').read_text())
            self.assertEqual(schema.returncode, 0, schema.stderr)
            installation, release, session = map(str, (uuid.uuid4(), uuid.uuid4(), uuid.uuid4()))
            original = 'https://old-' + uuid.UUID(installation).hex + '.example'
            seed = sql(f"""INSERT INTO installations VALUES('{installation}');
INSERT INTO releases VALUES('{release}'); INSERT INTO users VALUES('u'); INSERT INTO sessions VALUES('parent');
INSERT INTO app_frame_origins(origin,installation_id) VALUES('{original}','{installation}');
INSERT INTO app_frame_sessions(id,user_id,parent_session_hash,browser_purpose,installation_id,release_id,
 environment,origin,token_hash,token_expires_at,asset_hash,expires_at) VALUES
 ('{session}','u','parent','portal','{installation}','{release}','prod','{original}',
 'token',now()+interval '5 minutes','asset',now()+interval '1 hour');
INSERT INTO app_frame_codes(code_hash,purpose,frame_id,user_id,parent_session_hash,browser_purpose,
 installation_id,release_id,environment,portal_origin,expires_at) VALUES
 ('code','launch','{session}','u','parent','portal','{installation}','{release}','prod',
 'https://old.example',now()+interval '30 seconds');""")
            self.assertEqual(seed.returncode, 0, seed.stderr)
            rows = [{'origin': original, 'installation_id': installation, 'port': None}]
            script = move.sql_plan(addresses(), rows)
            rejected = sql("""CREATE FUNCTION reject_frame() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'isolated_insert_failure'; END $$;
CREATE TRIGGER reject_frame BEFORE INSERT ON app_frame_origins FOR EACH ROW EXECUTE FUNCTION reject_frame();""")
            self.assertEqual(rejected.returncode, 0, rejected.stderr)
            self.assertNotEqual(sql(script).returncode, 0)
            self.assertEqual(sql('SELECT count(*) FROM app_frame_codes;').stdout.strip(), '1')
            self.assertEqual(sql('SELECT bool_and(revoked_at IS NULL) FROM app_frame_sessions;').stdout.strip(), 't')
            self.assertEqual(sql('SELECT count(*) FROM app_frame_origins WHERE installation_id IS NOT NULL;').stdout.strip(), '1')
            self.assertEqual(sql('DROP TRIGGER reject_frame ON app_frame_origins;').returncode, 0)
            applied = sql(script)
            self.assertEqual(applied.returncode, 0, applied.stderr)
            self.assertEqual(sql('SELECT count(*) FROM app_frame_origins;').stdout.strip(), '2')
            self.assertEqual(sql('SELECT origin FROM app_frame_origins WHERE installation_id IS NOT NULL;').stdout.strip(),
                             'https://192.0.2.10:20000')
            self.assertEqual(sql('SELECT bool_and(revoked_at IS NOT NULL) FROM app_frame_sessions;').stdout.strip(), 't')
            self.assertEqual(sql('SELECT count(*) FROM app_frame_codes;').stdout.strip(), '0')
            stale = sql(script)
            self.assertNotEqual(stale.returncode, 0)
            self.assertIn('frame_origin_snapshot_changed', stale.stderr)
            self.assertEqual(sql('SELECT count(*) FROM app_frame_origins;').stdout.strip(), '2')
            fresh = json.loads(sql("SELECT json_agg(t) FROM (SELECT origin,installation_id,port FROM app_frame_origins) t;").stdout)
            self.assertEqual(move.plan(addresses(), fresh)[1], [])
            domains = {**addresses(), 'appFrameOrigin': 'https://app-{key}.example', 'appFramePorts': None}
            second = sql(move.sql_plan(domains, fresh))
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(sql('SELECT port FROM app_frame_origins WHERE installation_id IS NULL AND port IS NOT NULL;').stdout.strip(), '20000')
            self.assertEqual(sql('SELECT count(*) FROM app_frame_origins;').stdout.strip(), '3')
        finally:
            removed = run('docker', 'rm', '-f', container)
            self.assertEqual(removed.returncode, 0, removed.stderr)


if __name__ == '__main__':
    unittest.main()
