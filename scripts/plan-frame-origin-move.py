#!/usr/bin/env python3
"""Generate reviewed SQL for existing frame origins; never connect to a database.

Use a JSON snapshot of SELECT origin,installation_id,port FROM app_frame_origins
and the candidate server.json. Apply the resulting SQL only after backup/restore
verification and stopping Core/worker. Old origins and ports stay reserved.
"""
import argparse
import json
from pathlib import Path
import uuid

from server_addresses import validate_addresses


def plan(settings, rows):
    frame = validate_addresses(settings)
    template = settings['appFrameOrigin']
    origins, installations, ports = set(), set(), set()
    normalized = []
    for row in rows:
        origin, installation, port = row['origin'], row['installation_id'], row['port']
        if not isinstance(origin, str) or not origin or origin in origins or '\0' in origin:
            raise ValueError('Invalid or duplicate snapshot origin')
        origins.add(origin)
        if installation is not None:
            installation = str(uuid.UUID(installation))
            if installation in installations:
                raise ValueError('Duplicate snapshot installation')
            installations.add(installation)
        if port is not None:
            if type(port) is not int or not 1 <= port <= 65535 or port in ports:
                raise ValueError('Invalid or duplicate snapshot port')
            ports.add(port)
        normalized.append({'origin': origin, 'installation_id': installation, 'port': port})
    free = iter(number for number in frame['pool'] if number not in ports)
    changes = []
    for row in sorted(normalized, key=lambda row: row['origin']):
        installation = row['installation_id']
        if installation is None:
            continue
        if '{key}' in template:
            target, port = template.replace('{key}', uuid.UUID(installation).hex), None
        else:
            # Keep an already valid assignment. Never recycle a tombstone's
            # port, including when moving to another hostname or IP address.
            if row['port'] in frame['pool'] and row['origin'] == template.replace('{port}', str(row['port'])):
                continue
            port = next(free, None)
            if port is None:
                raise ValueError('Frame port pool exhausted; extend it without reusing reserved ports')
            target = template.replace('{port}', str(port))
        if target == row['origin']:
            continue
        if target in origins:
            raise ValueError('Target frame origin is already reserved: ' + target)
        origins.add(target)
        changes.append({**row, 'target': target, 'target_port': port})
    return normalized, changes


def literal(value):
    if value is None:
        return 'NULL'
    return "'" + str(value).replace("'", "''") + "'"


def sql_plan(settings, rows):
    snapshot, changes = plan(settings, rows)
    # Lock and compare the entire allocation snapshot before touching data. A
    # concurrent allocation, deletion or second application rolls back in full.
    lines = ["\\set ON_ERROR_STOP on", "BEGIN;", "SET LOCAL standard_conforming_strings = on;",
             "SET LOCAL lock_timeout = '10s';", "SET LOCAL statement_timeout = '60s';",
             "SELECT pg_advisory_xact_lock(hashtextextended('app-frame-origins',0));",
             "LOCK TABLE app_frame_origins, app_frame_sessions, app_frame_codes IN ACCESS EXCLUSIVE MODE;",
             "CREATE TEMP TABLE expected_frame_origins (origin text, installation_id uuid, port integer) ON COMMIT DROP;"]
    if snapshot:
        values = ',\n'.join('(' + ','.join(literal(row[key]) for key in ('origin', 'installation_id', 'port')) + ')'
                            for row in snapshot)
        lines.append('INSERT INTO expected_frame_origins VALUES\n' + values + ';')
    lines.append("""DO $$ BEGIN
IF EXISTS (
    (SELECT origin,installation_id,port FROM app_frame_origins EXCEPT SELECT * FROM expected_frame_origins)
    UNION ALL
    (SELECT * FROM expected_frame_origins EXCEPT SELECT origin,installation_id,port FROM app_frame_origins)
) THEN RAISE EXCEPTION 'frame_origin_snapshot_changed'; END IF;
END $$;""")
    for row in changes:
        installation = literal(row['installation_id'])
        lines.extend([
            f'DELETE FROM app_frame_codes WHERE installation_id={installation};',
            f'UPDATE app_frame_sessions SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE installation_id={installation};',
            f'UPDATE app_frame_origins SET installation_id=NULL WHERE origin={literal(row["origin"])};',
            'INSERT INTO app_frame_origins(origin,installation_id,port) VALUES (' +
            ','.join(literal(value) for value in (row['target'], row['installation_id'], row['target_port'])) + ');',
        ])
    lines.extend(['COMMIT;', f'-- Moved {len(changes)} installation origins; old origins/ports remain reserved.'])
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--settings', type=Path, required=True, help='Candidate server.json')
    parser.add_argument('--snapshot', type=Path, required=True, help='Public allocation rows as a JSON array')
    parser.add_argument('--output', type=Path, required=True, help='New SQL file; existing files are refused')
    args = parser.parse_args()
    try:
        sql = sql_plan(json.loads(args.settings.read_text()), json.loads(args.snapshot.read_text()))
        with args.output.open('x') as handle:
            handle.write(sql)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, 'Frame origin plan failed: ' + str(error) + '\n')
    print('Frame origin SQL prepared; no database or running service was changed.')


if __name__ == '__main__':
    main()
