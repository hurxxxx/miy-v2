#!/usr/bin/env python3
"""Compare the running live postgres/keycloak containers with what compose would create.
Read-only. Prints only setting names that differ, never values (env values are secrets).
Usage: compose-diff.py [STATE_DIR]   (default /projects/miy-v2/.local)"""
import json, os, subprocess, sys

REPO = '/projects/miy-v2'
STATE = sys.argv[1] if len(sys.argv) > 1 else REPO + '/.local'
CONTAINERS = {'postgres': 'miy-v2-validation-postgres-1', 'keycloak': 'miy-v2-validation-keycloak-1'}


def run(args, env=None):
    return subprocess.run(args, cwd=REPO, env=env, capture_output=True, text=True, check=True).stdout


env = {**os.environ, 'MIY_STATE_DIR': STATE}
config = json.loads(run(['docker', 'compose', '--env-file', STATE + '/compose.env', 'config', '--format', 'json'], env))
project = config['name']
report = {}
for service, container in CONTAINERS.items():
    want = config['services'][service]
    live = json.loads(run(['docker', 'inspect', container]))[0]
    differs = []
    if live['Config']['Labels'].get('com.docker.compose.project') != project:
        differs.append('project')
    if want['image'] != live['Config']['Image']:
        differs.append('image')
    image_id = json.loads(run(['docker', 'image', 'inspect', want['image']]))[0]['Id']
    if image_id != live['Image']:
        differs.append('imageId')
    live_env = dict(item.split('=', 1) for item in live['Config']['Env'])
    for key, value in (want.get('environment') or {}).items():
        if live_env.get(key) != ('' if value is None else str(value)):
            differs.append('env:' + key)
    if want.get('command') is not None and want['command'] != live['Config']['Cmd']:
        differs.append('command')
    want_ports = sorted((p.get('host_ip', ''), str(p['published']), f"{p['target']}/{p.get('protocol', 'tcp')}") for p in want.get('ports', []))
    live_ports = sorted((b['HostIp'], b['HostPort'], port) for port, binds in (live['HostConfig']['PortBindings'] or {}).items() for b in binds)
    if want_ports != live_ports:
        differs.append('ports')
    want_mounts = sorted((m['type'], f"{project}_{m['source']}" if m['type'] == 'volume' else m['source'], m['target'], bool(m.get('read_only'))) for m in want.get('volumes', []))
    live_mounts = sorted((m['Type'], m.get('Name') if m['Type'] == 'volume' else m['Source'], m['Destination'], not m['RW']) for m in live['Mounts'])
    if want_mounts != live_mounts:
        differs.append('mounts')
    want_networks = sorted(config['networks'][n].get('name', f'{project}_{n}') for n in want.get('networks', {'default': None}))
    if want_networks != sorted(live['NetworkSettings']['Networks']):
        differs.append('networks')
    report[service] = {'container': container, 'running': live['State']['Running'], 'differs': differs}
print(json.dumps(report, indent=1))
