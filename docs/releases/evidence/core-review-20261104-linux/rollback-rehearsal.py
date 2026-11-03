#!/usr/bin/env python3
"""Restore a private live snapshot into owned PostgreSQL, migrate 17->19 as miy_core,
write new-schema data, then exercise the sibling deploy.restore_database helper.

No live database/service is contacted. The database properties reference is the
sibling deploy.EXPECTED_DB_PROPERTIES, independently checked by the rollout's live
preflight. Compare complete fixture properties, table counts and stable row digests
before/after rollback. Never print rows, credentials, digests or subprocess stderr.

Usage: rollback-rehearsal.py BACKUP_DIR CANDIDATE_BINARY [--out PRIVATE_RESULT_JSON]
The source revision and candidate digest deliberately bind this release rehearsal.
"""
import argparse
import ast
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
ROOT = HERE.parents[3]
SOURCE = '3406d9f5b9c8afbc55a0e3e8752cb178c61db0de'
CANDIDATE = '59407ce6d8853c8176054fb36afb92fa97585502cea1b5b04483d8a82ce91feb'
IMAGE = 'postgres:18.3@sha256:7e32e9833a6fb1c92c32552794cb6ed569d51b445a54907d35fc112ef39684db'
LABEL = 'miy.review-rollback-rehearsal'
PROPERTIES = (
    "SELECT json_build_object('owner',pg_get_userbyid(datdba),"
    "'encoding',pg_encoding_to_char(encoding),'collate',datcollate,'ctype',datctype,"
    "'provider',datlocprovider,'acl',datacl::text,'allowConnections',datallowconn,"
    "'publicRevoked',NOT EXISTS(SELECT 1 FROM aclexplode(coalesce(datacl,acldefault('d',datdba))) WHERE grantee=0)) "
    "FROM pg_database WHERE datname='miy_core'"
)


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with pathlib.Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def require(condition, code):
    if not condition:
        raise RuntimeError(code)


def run(args, *, cwd=None, env=None, stdin=None, stdout=None, input=None, timeout=120):
    result = subprocess.run(args, cwd=cwd, env=env, stdin=stdin, input=input,
                            stdout=subprocess.PIPE if stdout is None else stdout,
                            stderr=subprocess.PIPE, text=stdout is None, timeout=timeout)
    require(result.returncode == 0, 'command_failed')
    return (result.stdout or '').strip() if stdout is None else ''


def restore_function_digest(path):
    source = pathlib.Path(path).read_text()
    function = next(node for node in ast.parse(source).body
                    if isinstance(node, ast.FunctionDef) and node.name == 'restore_database')
    return hashlib.sha256(ast.get_source_segment(source, function).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('backup', type=pathlib.Path)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--out', type=pathlib.Path)
    args = parser.parse_args()
    backup = args.backup.resolve()
    output = args.out or backup.parent / 'rollback-rehearsal.json'
    # Append a new evidence file for another attempt; never overwrite previous proof.
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    result = {'status': 'FAIL', 'containerRemoved': False, 'liveAccess': False,
              'propertiesReference': 'deploy.EXPECTED_DB_PROPERTIES; live comparison is a separate rollout preflight'}
    token, password, owner_password = (secrets.token_hex(16) for _ in range(3))
    container = None
    core_backup = None
    stage = 'verify_inputs'
    try:
        require(output.parent.resolve().stat().st_mode & 0o077 == 0, 'result_directory_not_private')
        source = run(['git', 'rev-parse', SOURCE + '^{commit}'], cwd=ROOT)
        require(source == SOURCE, 'source_commit_mismatch')
        run(['git', 'diff', '--quiet', SOURCE, '--', 'Cargo.toml', 'Cargo.lock',
             'rust-toolchain.toml', 'crates', 'migrations', 'templates', 'registry'], cwd=ROOT)
        result['sourceCommit'] = source
        result['candidateSha256'] = digest(args.binary)
        require(result['candidateSha256'] == CANDIDATE, 'candidate_digest_mismatch')
        manifest = json.loads((backup / 'manifest.json').read_text())
        require(manifest['database'] == 'miy_core' and manifest['migrationVersion'] == 17,
                'unexpected_backup_database_or_version')
        require(manifest['dump']['file'] == 'miy_core.dump', 'unexpected_dump_file')
        dump = backup / 'miy_core.dump'
        require(backup.stat().st_mode & 0o077 == 0 and dump.stat().st_mode & 0o077 == 0,
                'backup_not_private')
        result['backupSha256'] = digest(dump)
        require(result['backupSha256'] == manifest['dump']['sha256'], 'backup_digest_mismatch')
        deployment = load('review_deployment', HERE / 'deploy.py')
        core_backup = load('review_core_backup', ROOT / 'scripts/core-backup.py')
        result['restoreFunctionSha256'] = restore_function_digest(HERE / 'deploy.py')

        with tempfile.TemporaryDirectory(prefix='miy-review-rollback-') as temporary:
            work = pathlib.Path(temporary)
            work.chmod(0o700)
            binary = work / 'miy'
            shutil.copyfile(args.binary, binary)
            binary.chmod(0o700)
            require(digest(binary) == CANDIDATE, 'frozen_candidate_digest_mismatch')
            require(run([str(binary), '--version'], cwd=work) == 'miy 0.7.0', 'candidate_version_mismatch')
            stage = 'create_disposable_postgres'
            container = run([
                'docker', 'run', '--pull=never', '-d', '--label', f'{LABEL}={token}',
                '--memory', '768m', '--cpus', '1', '--pids-limit', '128',
                '--tmpfs', '/var/lib/postgresql:rw,size=768m', '-e', 'POSTGRES_PASSWORD',
                '-p', '127.0.0.1::5432', IMAGE,
            ], env={**os.environ, 'POSTGRES_PASSWORD': password})
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
                # Importing deploy.py never authorizes its main or service commands.
                require(command[:2] == ['docker', 'exec'], 'non_database_restore_command')
                position = 3 if command[2] == '-i' else 2
                require(command[position] == container and command[position + 1] in ('psql', 'pg_restore'),
                        'non_owned_restore_command')
                return run(command, **kwargs)

            def sql(query, db='postgres', user='postgres'):
                return owned_run(['docker', 'exec', '-i', container, 'psql', '-X', '-qAt',
                                  '-v', 'ON_ERROR_STOP=1', '-U', user, '-d', db], input=query)

            def rows_digest(tables):
                # Hash rows inside PostgreSQL; only private aggregate hashes enter
                # process memory. Counts retain duplicates and row order is ignored.
                values = {}
                for name in sorted(tables):
                    quoted = '"' + name.replace('"', '""') + '"'
                    values[name] = sql(
                        "SET TIME ZONE 'UTC'; SET DateStyle='ISO,YMD'; "
                        "SELECT encode(sha256(convert_to(coalesce(string_agg(h,'' ORDER BY h),''),'UTF8')),'hex') "
                        "FROM (SELECT encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') h "
                        f"FROM public.{quoted} t) rows", 'miy_core')
                return values

            stage = 'restore_original_snapshot'
            sql(f"CREATE ROLE miy_core LOGIN PASSWORD '{owner_password}'")
            sql('CREATE DATABASE miy_core ' + deployment.DB_PROPERTIES)
            sql('REVOKE ALL ON DATABASE miy_core FROM PUBLIC')
            with dump.open('rb') as handle:
                owned_run(['docker', 'exec', '-i', container, 'pg_restore', '-U', 'postgres', '-d', 'miy_core',
                           '--single-transaction', '--exit-on-error'], stdin=handle, stdout=subprocess.DEVNULL)
            original = json.loads(sql(core_backup.COUNTS_SQL, 'miy_core'))
            require(original['tables'] == manifest['tables'] and original['migrationVersion'] == 17,
                    'initial_snapshot_mismatch')
            original_digests = rows_digest(original['tables'])
            original_properties = json.loads(sql(PROPERTIES))
            expected = deployment.EXPECTED_DB_PROPERTIES
            require({key: original_properties[key] for key in expected} == expected,
                    'initial_properties_mismatch')
            require(sql('SELECT current_user', 'miy_core', 'miy_core') == 'miy_core', 'migration_owner_mismatch')
            require(sql("SELECT NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole FROM pg_roles WHERE rolname='miy_core'") == 't',
                    'migration_owner_is_privileged')
            port = int(run(['docker', 'port', container, '5432/tcp']).rsplit(':', 1)[1])
            url = f'postgres://miy_core:{owner_password}@127.0.0.1:{port}/miy_core'
            stage = 'migrate_as_core_owner'
            migration = subprocess.run([str(binary), 'migrate'], cwd=work, env={
                'PATH': '/usr/bin:/bin', 'DATABASE_URL': url,
                'PROVISION_DATABASE_URL': url, 'MIY_DATABASE_CONTAINER': container,
                'MIY_STATE_DIR': str(work / 'state'),
            }, capture_output=True, timeout=120)
            result['migrateExitCode'] = migration.returncode
            result['migratedVersion'] = int(sql('SELECT max(version) FROM _sqlx_migrations', 'miy_core'))
            require(migration.returncode == 0 and result['migratedVersion'] == 19, 'migration_failed')
            stage = 'write_new_schema_probe'
            probe = secrets.token_hex(16)
            sql(f"INSERT INTO groups(id,name,source) VALUES('{probe}','rollback rehearsal','local'); "
                f"INSERT INTO group_members(group_id,user_id) SELECT '{probe}',id FROM users ORDER BY id LIMIT 1;",
                'miy_core', 'miy_core')
            require(sql(f"SELECT count(*) FROM groups WHERE id='{probe}'", 'miy_core') == '1', 'probe_write_failed')
            require(sql(f"SELECT count(*) FROM group_members WHERE group_id='{probe}'", 'miy_core') == '1',
                    'new_foreign_key_probe_missing')
            stage = 'rollback_into_empty_database'
            kept = deployment.restore_database(owned_run, dump, container=container)
            require(re.fullmatch(r'miy_core_failed_[0-9a-f]{16}', kept) is not None, 'unexpected_preserved_database')
            restored = json.loads(sql(core_backup.COUNTS_SQL, 'miy_core'))
            properties = json.loads(sql(PROPERTIES))
            restored_digests = rows_digest(restored['tables'])
            mismatched = sorted(t for t in set(original['tables']) | set(restored['tables'])
                                if original['tables'].get(t) != restored['tables'].get(t))
            changed_rows = sorted(t for t in set(original_digests) | set(restored_digests)
                                  if original_digests.get(t) != restored_digests.get(t))
            closed = sql(f"SELECT NOT datallowconn FROM pg_database WHERE datname='{kept}'") == 't'
            # This copy alone is reopened briefly to inspect the preserved probe.
            # Production restore_database never reopens the preserved database.
            sql(f'ALTER DATABASE {kept} ALLOW_CONNECTIONS true')
            try:
                kept_probe = sql(f"SELECT count(*) FROM groups WHERE id='{probe}'", kept) == '1'
            finally:
                sql(f'ALTER DATABASE {kept} ALLOW_CONNECTIONS false')
            closed_after = sql(f"SELECT NOT datallowconn FROM pg_database WHERE datname='{kept}'") == 't'
            refused = subprocess.run(['docker', 'exec', container, 'psql', '-X', '-qAt', '-U', 'postgres',
                                      '-d', kept, '-c', 'SELECT 1'], capture_output=True, timeout=15).returncode != 0
            result.update({
                'restoredVersion': restored['migrationVersion'], 'tables': len(restored['tables']),
                'rows': sum(restored['tables'].values()), 'mismatchedTables': mismatched,
                'rowDigestMismatchedTables': changed_rows, 'stableRowDigestsMatch': not changed_rows,
                'groupsTableGone': sql("SELECT to_regclass('public.groups') IS NULL AND to_regclass('public.group_members') IS NULL", 'miy_core') == 't',
                'keptDatabaseHasProbe': kept_probe, 'keptDatabaseClosed': closed and closed_after and refused,
                'propertiesMatchOriginalFixture': properties == original_properties,
                'propertiesMatchLive': {key: properties[key] for key in expected} == expected,
                'properties': properties,
                'restoreFunctionUnchanged': restore_function_digest(HERE / 'deploy.py') == result['restoreFunctionSha256'],
            })
            require(digest(dump) == result['backupSha256'], 'backup_changed_during_rehearsal')
            ok = (result['restoredVersion'] == 17 and not mismatched and not changed_rows
                  and result['groupsTableGone'] and result['keptDatabaseHasProbe'] and result['keptDatabaseClosed']
                  and result['propertiesMatchOriginalFixture'] and result['propertiesMatchLive']
                  and result['restoreFunctionUnchanged'])
            result['status'] = 'PASS' if ok else 'FAIL'
    except (Exception, KeyboardInterrupt) as error:
        result.update({'status': 'FAIL', 'failureCategory': 'rollback_rehearsal_failed',
                       'failedStage': stage, 'failureType': type(error).__name__})
    finally:
        if container and core_backup:
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
