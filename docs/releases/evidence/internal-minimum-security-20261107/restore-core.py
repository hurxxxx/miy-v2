"""Narrow, import-only core DB restore helper for the 2026-11-07 boundary rollout.

The caller must stop and prove quiescence of both core services before using this
against production. This module never opens a connection by itself. The database
being replaced is preserved under a random name with connections disabled.
"""
import json
import pathlib
import re
import subprocess
import uuid

DB_PROPERTIES = "OWNER miy_core TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'en_US.utf8' LC_CTYPE 'en_US.utf8'"
EXPECTED_DB_PROPERTIES = {
    'owner': 'miy_core', 'encoding': 'UTF8', 'collate': 'en_US.utf8',
    'ctype': 'en_US.utf8', 'provider': 'c', 'publicRevoked': True,
}
PROPERTIES_SQL = (
    "SELECT json_build_object('owner',pg_get_userbyid(datdba),"
    "'encoding',pg_encoding_to_char(encoding),'collate',datcollate,'ctype',datctype,"
    "'provider',datlocprovider,'acl',datacl::text,'allowConnections',datallowconn,"
    "'publicRevoked',NOT EXISTS(SELECT 1 FROM aclexplode(coalesce(datacl,acldefault('d',datdba))) WHERE grantee=0)) "
    "FROM pg_database WHERE datname='miy_core'"
)


def require(condition, code):
    if not condition:
        raise RuntimeError(code)


def sql(run, query, *, container, database='postgres'):
    return run(['docker', 'exec', '-i', container, 'psql', '-X', '-qAt',
                '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', database], input=query)


def restore_database(run, dump, *, container, database='miy_core'):
    """Preserve the failed DB, restore the old dump/ownership/ACL in one transaction.

    Explicit container and allowlisted DB prevent accidental fallback to production.
    No tables or newer accepted writes are dropped from the preserved database.
    """
    require(database == 'miy_core', 'unsupported_database')
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}', container) is not None,
            'invalid_container')
    dump = pathlib.Path(dump)
    require(dump.is_file() and not dump.is_symlink() and dump.stat().st_mode & 0o077 == 0,
            'dump_missing_or_not_private')
    props = json.loads(sql(run, PROPERTIES_SQL, container=container))
    require({key: props[key] for key in EXPECTED_DB_PROPERTIES} == EXPECTED_DB_PROPERTIES,
            'unexpected_database_properties')
    # This host's only database-level grant is the owner. pg_dump without --create
    # does not carry extra database grants, so refuse an unsupported ACL instead.
    require(props['acl'] == '{miy_core=CTc/miy_core}' and props['allowConnections'],
            'unexpected_database_acl_or_connection_state')
    kept = f'miy_core_failed_{uuid.uuid4().hex[:16]}'
    sql(run, 'ALTER DATABASE miy_core ALLOW_CONNECTIONS false', container=container)
    sql(run, "SELECT count(pg_terminate_backend(pid)) FROM pg_stat_activity WHERE datname='miy_core'",
        container=container)
    sql(run, f'ALTER DATABASE miy_core RENAME TO {kept}', container=container)
    sql(run, 'CREATE DATABASE miy_core ' + DB_PROPERTIES, container=container)
    sql(run, 'REVOKE ALL ON DATABASE miy_core FROM PUBLIC', container=container)
    with dump.open('rb') as handle:
        run(['docker', 'exec', '-i', container, 'pg_restore', '-U', 'postgres', '-d', 'miy_core',
             '--single-transaction', '--exit-on-error'], stdin=handle, stdout=subprocess.DEVNULL)
    return kept
