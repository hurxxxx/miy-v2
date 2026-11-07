#!/usr/bin/env python3
"""Real core19 snapshot -> owned DB -> candidate20 -> exact restore helper -> core19.

Only the private snapshot is read. No production service or DB is contacted.
Aggregate row hashes remain in memory; output contains counts, bools and file hashes.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
HERE = pathlib.Path(__file__).resolve().parent
ROOT = pathlib.Path('/projects/miy-v2')
SOURCE = 'a4ad37006c58bc80de61892517cd3e1a153208ba'
CANDIDATE = '450a1709dea67803ce120bcc8c4fc55d9823f28006cf420378f312c93c26e5a2'
IMAGE = 'postgres:18.3@sha256:7e32e9833a6fb1c92c32552794cb6ed569d51b445a54907d35fc112ef39684db'
LABEL = 'miy.boundary-rollback-rehearsal'
DELETED = {'sessions', 'development_sessions', 'login_flows', 'cli_authorizations'}


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def require(condition, code):
    if not condition:
        raise RuntimeError(code)


def digest(path):
    with pathlib.Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def run(command, *, cwd=None, env=None, stdin=None, stdout=None, input=None, timeout=120):
    process = subprocess.run(command, cwd=cwd, env=env, stdin=stdin, input=input,
                             stdout=subprocess.PIPE if stdout is None else stdout,
                             stderr=subprocess.PIPE, text=stdout is None, timeout=timeout)
    require(process.returncode == 0, 'command_failed')
    return (process.stdout or '').strip() if stdout is None else ''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('backup', type=pathlib.Path)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    result = {'status': 'FAIL', 'sourceCommit': SOURCE, 'productionAccess': False,
              'containerRemoved': False, 'rehearsal': 'real core19 dump -> core20 -> exact helper -> core19'}
    container, stage = None, 'verify_inputs'
    token, password, owner_password = (secrets.token_hex(16) for _ in range(3))
    core_backup = load('boundary_core_backup', ROOT / 'scripts/core-backup.py')
    restore = load('boundary_restore', HERE / 'restore_core.py')
    try:
        require(args.out.parent.stat().st_mode & 0o077 == 0, 'result_directory_not_private')
        require(run(['git', 'rev-parse', SOURCE + '^{commit}'], cwd=ROOT) == SOURCE, 'source_missing')
        run(['git', 'diff', '--quiet', SOURCE, '--', 'Cargo.toml', 'Cargo.lock',
             'rust-toolchain.toml', 'crates', 'migrations', 'templates', 'registry'], cwd=ROOT)
        require(digest(args.binary) == CANDIDATE, 'candidate_digest_mismatch')
        result['candidateSha256'] = CANDIDATE
        result['restoreHelperSha256'] = digest(HERE / 'restore_core.py')
        backup = args.backup.resolve()
        manifest = json.loads((backup / 'manifest.json').read_text())
        require(manifest['database'] == 'miy_core' and manifest['migrationVersion'] == 19,
                'unexpected_backup_database_or_version')
        require(manifest['dump']['file'] == 'miy_core.dump', 'unexpected_dump_file')
        dump = backup / 'miy_core.dump'
        require(backup.stat().st_mode & 0o077 == 0 and dump.stat().st_mode & 0o077 == 0,
                'backup_not_private')
        result['backupSha256'] = digest(dump)
        require(result['backupSha256'] == manifest['dump']['sha256'], 'backup_digest_mismatch')
        with tempfile.TemporaryDirectory(prefix='miy-boundary-rollback-') as temporary:
            work = pathlib.Path(temporary)
            work.chmod(0o700)
            binary = work / 'miy'
            shutil.copyfile(args.binary, binary)
            binary.chmod(0o700)
            require(digest(binary) == CANDIDATE, 'frozen_candidate_digest_mismatch')
            require(run([str(binary), '--version'], cwd=work) == 'miy 0.7.0', 'candidate_version_mismatch')
            stage = 'create_disposable_postgres'
            container = run(['docker', 'run', '--pull=never', '-d', '--label', f'{LABEL}={token}',
                '--memory', '768m', '--cpus', '1', '--pids-limit', '128',
                '--tmpfs', '/var/lib/postgresql:rw,size=768m', '-e', 'POSTGRES_PASSWORD',
                '-p', '127.0.0.1::5432', IMAGE], env={**os.environ, 'POSTGRES_PASSWORD': password})
            require(re.fullmatch(r'[0-9a-f]{64}', container) is not None, 'invalid_container_id')
            result['container'] = container[:12]
            for _ in range(60):
                ready = subprocess.run(['docker', 'exec', container, 'pg_isready', '-U', 'postgres', '-h', '127.0.0.1'],
                                       capture_output=True, timeout=10)
                if ready.returncode == 0:
                    break
                time.sleep(1)
            else:
                raise RuntimeError('disposable_database_not_ready')

            def owned_run(command, **kwargs):
                require(command[:2] == ['docker', 'exec'], 'non_database_restore_command')
                position = 3 if command[2] == '-i' else 2
                require(command[position] == container and command[position + 1] in ('psql', 'pg_restore'),
                        'non_owned_restore_command')
                return run(command, **kwargs)

            def sql(query, db='postgres', user='postgres'):
                return owned_run(['docker', 'exec', '-i', container, 'psql', '-X', '-qAt',
                                  '-v', 'ON_ERROR_STOP=1', '-U', user, '-d', db], input=query)

            def quote(name):
                require(re.fullmatch(r'[a-z_][a-z_0-9]*', name) is not None, 'unexpected_identifier')
                return '"' + name + '"'

            def row_digests(columns, filter_devices=False, old_migrations=False):
                values = {}
                for table, names in sorted(columns.items()):
                    where = " WHERE status NOT IN ('pending','approved')" if table == 'cli_device_authorizations' and filter_devices else ''
                    if table == '_sqlx_migrations' and old_migrations:
                        where = ' WHERE version <= 19'
                    projection = ','.join(quote(name) for name in names)
                    values[table] = sql("SET TIME ZONE 'UTC'; SET DateStyle='ISO,YMD'; "
                        "SELECT encode(sha256(convert_to(coalesce(string_agg(h,'' ORDER BY h),''),'UTF8')),'hex') "
                        "FROM (SELECT encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') h "
                        f"FROM (SELECT {projection} FROM public.{quote(table)}{where}) t) rows", 'miy_core')
                return values

            def object_metadata():
                return sql("SELECT json_build_object('relations',(SELECT json_agg(json_build_array(c.relname,c.relkind,pg_get_userbyid(c.relowner),c.relacl::text) ORDER BY c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),'functions',(SELECT json_agg(json_build_array(p.proname,pg_get_userbyid(p.proowner),p.proacl::text) ORDER BY p.proname) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public'),'schema',(SELECT json_build_array(pg_get_userbyid(nspowner),nspacl::text) FROM pg_namespace WHERE nspname='public'),'defaults',(SELECT json_agg(json_build_array(pg_get_userbyid(defaclrole),defaclobjtype,defaclacl::text) ORDER BY defaclrole,defaclobjtype) FROM pg_default_acl))", 'miy_core')

            stage = 'restore_original_snapshot'
            sql(f"CREATE ROLE miy_core LOGIN PASSWORD '{owner_password}'")
            sql('CREATE DATABASE miy_core ' + restore.DB_PROPERTIES)
            sql('REVOKE ALL ON DATABASE miy_core FROM PUBLIC')
            with dump.open('rb') as handle:
                owned_run(['docker', 'exec', '-i', container, 'pg_restore', '-U', 'postgres', '-d', 'miy_core',
                           '--single-transaction', '--exit-on-error'], stdin=handle, stdout=subprocess.DEVNULL)
            original = json.loads(sql(core_backup.COUNTS_SQL, 'miy_core'))
            require(original['tables'] == manifest['tables'] and original['migrationVersion'] == 19,
                    'initial_snapshot_mismatch')
            columns = json.loads(sql("SELECT json_object_agg(table_name,cols) FROM (SELECT table_name,json_agg(column_name ORDER BY ordinal_position) cols FROM information_schema.columns WHERE table_schema='public' GROUP BY table_name) t", 'miy_core'))
            original_digests = row_digests(columns)
            original_retained = row_digests({t:c for t,c in columns.items() if t not in DELETED}, filter_devices=True)
            original_properties = json.loads(sql(restore.PROPERTIES_SQL))
            original_objects = object_metadata()
            require({key: original_properties[key] for key in restore.EXPECTED_DB_PROPERTIES} == restore.EXPECTED_DB_PROPERTIES,
                    'initial_properties_mismatch')
            require(original_properties['acl'] == '{miy_core=CTc/miy_core}', 'initial_acl_mismatch')
            require(sql('SELECT current_user', 'miy_core', 'miy_core') == 'miy_core', 'migration_owner_mismatch')
            require(sql("SELECT NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole FROM pg_roles WHERE rolname='miy_core'") == 't',
                    'migration_owner_is_privileged')
            port = int(run(['docker', 'port', container, '5432/tcp']).rsplit(':', 1)[1])
            url = f'postgres://miy_core:{owner_password}@127.0.0.1:{port}/miy_core'
            stage = 'migrate_as_core_owner'
            migration = subprocess.run([str(binary), 'migrate'], cwd=work, env={
                'PATH': '/usr/bin:/bin', 'DATABASE_URL': url, 'PROVISION_DATABASE_URL': url,
                'MIY_DATABASE_CONTAINER': container, 'MIY_STATE_DIR': str(work / 'state')}, capture_output=True, timeout=120)
            result['migrateExitCode'] = migration.returncode
            result['migratedVersion'] = int(sql('SELECT max(version) FROM _sqlx_migrations', 'miy_core'))
            require(migration.returncode == 0 and result['migratedVersion'] == 20, 'migration_failed')
            stage = 'verify_migration_preservation'
            migrated = json.loads(sql(core_backup.COUNTS_SQL, 'miy_core'))
            retained = row_digests({t:c for t,c in columns.items() if t not in DELETED}, filter_devices=False, old_migrations=True)
            mismatched_retained = sorted(t for t in original_retained if original_retained[t] != retained[t])
            require(not mismatched_retained, 'preserved_rows_changed')
            require(all(migrated['tables'][t] == 0 for t in DELETED), 'old_browser_flow_remains')
            require(sql("SELECT count(*)=0 FROM cli_device_authorizations WHERE status IN ('pending','approved')", 'miy_core') == 't',
                    'old_pending_device_approval_remains')
            require(sql('SELECT count(*)=count(id) AND count(*)=count(DISTINCT id) FROM cli_tokens', 'miy_core') == 't',
                    'legacy_public_ids_invalid')
            result.update({'retainedOldColumnTables': len(retained), 'retainedRowsMatch': True,
                'cliCredentialAndDelegationRowsMatch': True, 'invalidatedBrowserTables': sorted(DELETED),
                'oldPendingDeviceApprovalsCleared': True, 'legacyPublicIdsUnique': True,
                'independentCredentialRowsBefore': {t: original['tables'][t] for t in ('cli_tokens','cli_sessions','cli_access_tokens','cli_refresh_tokens','delegations')},
                'invalidatedBrowserRowsBefore': {t:original['tables'][t] for t in sorted(DELETED)}})
            stage = 'write_new_schema_probe'
            probe = secrets.token_hex(16)
            sql(f"INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at,browser_purpose) SELECT '{probe}',id,'fixture','fixture','fixture',now()+interval '1 hour','console' FROM users ORDER BY id LIMIT 1", 'miy_core', 'miy_core')
            require(sql(f"SELECT count(*)=1 FROM sessions WHERE id_hash='{probe}' AND browser_purpose='console'", 'miy_core') == 't', 'new_schema_probe_failed')
            invalid = subprocess.run(['docker', 'exec', '-i', container, 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-U', 'miy_core', '-d', 'miy_core'], input=f"UPDATE sessions SET browser_purpose='invalid' WHERE id_hash='{probe}'", capture_output=True, text=True)
            require(invalid.returncode != 0, 'purpose_check_not_enforced')
            stage = 'rollback_into_empty_database'
            kept = restore.restore_database(owned_run, dump, container=container)
            require(re.fullmatch(r'miy_core_failed_[0-9a-f]{16}', kept) is not None, 'unexpected_preserved_database')
            restored = json.loads(sql(core_backup.COUNTS_SQL, 'miy_core'))
            properties = json.loads(sql(restore.PROPERTIES_SQL))
            restored_digests = row_digests(columns)
            mismatched = sorted(t for t in set(original['tables']) | set(restored['tables']) if original['tables'].get(t) != restored['tables'].get(t))
            changed_rows = sorted(t for t in original_digests if original_digests[t] != restored_digests[t])
            closed = sql(f"SELECT NOT datallowconn FROM pg_database WHERE datname='{kept}'") == 't'
            sql(f'ALTER DATABASE {kept} ALLOW_CONNECTIONS true')
            try:
                kept_probe = sql(f"SELECT count(*)=1 FROM sessions WHERE id_hash='{probe}' AND browser_purpose='console'", kept) == 't'
            finally:
                sql(f'ALTER DATABASE {kept} ALLOW_CONNECTIONS false')
            refused = subprocess.run(['docker','exec',container,'psql','-X','-qAt','-U','postgres','-d',kept,'-c','SELECT 1'], capture_output=True, timeout=15).returncode != 0
            result.update({'restoredVersion': restored['migrationVersion'], 'tables': len(restored['tables']),
                'rows': sum(restored['tables'].values()), 'mismatchedTables': mismatched,
                'rowDigestMismatchedTables': changed_rows, 'stableRowDigestsMatch': not changed_rows,
                'newPurposeColumnGone': sql("SELECT count(*)=0 FROM information_schema.columns WHERE table_schema='public' AND table_name='sessions' AND column_name='browser_purpose'", 'miy_core') == 't',
                'keptDatabaseHasProbe': kept_probe, 'keptDatabaseClosed': closed and refused,
                'propertiesMatchOriginalFixture': properties == original_properties,
                'ownershipAndObjectACLMatch': object_metadata() == original_objects,
                'restoreHelperUnchanged': digest(HERE / 'restore_core.py') == result['restoreHelperSha256'],
                'nonPrivilegedMigrationOwner': True})
            require(digest(dump) == result['backupSha256'], 'backup_changed_during_rehearsal')
            require(result['restoredVersion'] == 19 and not mismatched and not changed_rows
                and result['newPurposeColumnGone'] and result['keptDatabaseHasProbe'] and result['keptDatabaseClosed']
                and result['propertiesMatchOriginalFixture'] and result['ownershipAndObjectACLMatch']
                and result['restoreHelperUnchanged'], 'rollback_verification_failed')
            result['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        result.update({'status':'FAIL','failedStage':stage,'failureType':type(error).__name__})
        if isinstance(error, RuntimeError) and re.fullmatch('[a-z_]+', str(error)):
            result['failureCategory'] = str(error)
    finally:
        if container:
            result.update(core_backup.cleanup_container(container, LABEL, token))
        if not result['containerRemoved']:
            result['status'] = 'FAIL'
        with os.fdopen(fd, 'w') as handle:
            json.dump(result, handle, indent=2)
            handle.write('\n')
        print(json.dumps(result))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
