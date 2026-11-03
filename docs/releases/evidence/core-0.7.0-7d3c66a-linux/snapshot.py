#!/usr/bin/env python3
"""Read-only snapshot of the live miy host for before/after comparison of a rollout.
Usage: snapshot.py OUT.json  |  snapshot.py --compare BEFORE.json AFTER.json
Records table row counts, installation state, app containers and all host containers
(id, name, image, state). No secrets and no user identifiers are written."""
import importlib.util, json, subprocess, sys

DB = 'miy-v2-validation-postgres-1'
spec = importlib.util.spec_from_file_location('core_backup', '/projects/miy-v2/scripts/core-backup.py')
core_backup = importlib.util.module_from_spec(spec); spec.loader.exec_module(core_backup)


def psql(sql, db='miy_core'):
    return subprocess.run(['docker', 'exec', DB, 'psql', '-X', '-q', '-At', '-U', 'postgres', '-d', db, '-c', sql],
                          check=True, capture_output=True, text=True).stdout.strip()


def snapshot():
    containers = subprocess.run(['docker', 'ps', '-a', '--no-trunc', '--format', '{{json .}}'], check=True,
                                capture_output=True, text=True).stdout.splitlines()
    rows = [json.loads(c) for c in containers]
    return {
        'tables': json.loads(psql(core_backup.COUNTS_SQL))['tables'],
        'migrationVersion': int(psql('SELECT max(version) FROM _sqlx_migrations')),
        'installations': json.loads(psql(
            "SELECT coalesce(json_agg(json_build_object('id', id, 'app', app_id, 'environment', environment, "
            "'release', release_id, 'runtime', runtime_status, 'desired', desired_state, 'container', runtime_container, "
            "'db', db_status) ORDER BY id), '[]') FROM installations")),
        'appContainers': sorted([r['Names'], r['Image'], r['State']] for r in rows if r['Names'].startswith('miy-app-')),
        'hostContainers': sorted([r['ID'], r['Names'], r['Image'], r['State']] for r in rows),
        'maxConnections': psql('SHOW max_connections', 'postgres'),
    }


def compare(before, after):
    # Tables that change with ordinary use during a rollout window, or by the rollout itself.
    volatile = {'audit', 'sessions', 'login_flows', '_sqlx_migrations'}
    tables = sorted(t for t in set(before['tables']) | set(after['tables'])
                    if t not in volatile and before['tables'].get(t) != after['tables'].get(t))
    hb = {c[1]: c for c in before['hostContainers']}; ha = {c[1]: c for c in after['hostContainers']}
    host = sorted(n for n in set(hb) | set(ha) if hb.get(n) != ha.get(n))
    return {'changedTables': {t: [before['tables'].get(t), after['tables'].get(t)] for t in tables},
            'installationsEqual': before['installations'] == after['installations'],
            'appContainersEqual': before['appContainers'] == after['appContainers'],
            'changedHostContainers': host,
            'migrationVersion': [before['migrationVersion'], after['migrationVersion']],
            'maxConnections': [before['maxConnections'], after['maxConnections']]}


if __name__ == '__main__':
    if sys.argv[1] == '--compare':
        load = lambda p: json.load(open(p))
        print(json.dumps(compare(load(sys.argv[2]), load(sys.argv[3])), indent=1))
    else:
        with open(sys.argv[1], 'w') as f:
            json.dump(snapshot(), f, indent=1)
        print(sys.argv[1])
