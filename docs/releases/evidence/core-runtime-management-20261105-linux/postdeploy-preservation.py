"""Read-only post-deployment preservation check against the apply record. Prints public facts only."""
import hashlib, json, subprocess, sys, urllib.request
rel = sys.argv[1]
deployment = json.load(open(rel + '/core-1/deployment.json'))
DB = 'miy-v2-validation-postgres-1'
def run(args):
    r = subprocess.run(args, capture_output=True, text=True); assert r.returncode == 0, 'command_failed'; return r.stdout.strip()
def sql(q): return run(['docker', 'exec', DB, 'psql', '-X', '-q', '-At', '-U', 'postgres', '-d', 'miy_core', '-c', q])
checks, ok = {}, True
services = []
for name, expected in deployment['runningExecutables'].items():
    props = dict(line.split('=', 1) for line in run(['systemctl', 'show', '-p', 'ActiveState', '-p', 'SubState', '-p', 'MainPID', '-p', 'NRestarts', name]).splitlines())
    pid = props['MainPID']
    try: actual = hashlib.file_digest(open(f'/proc/{pid}/exe', 'rb'), 'sha256').hexdigest()
    except PermissionError: actual = run(['sudo', '-n', 'sha256sum', f'/proc/{pid}/exe']).split()[0]
    same = props['ActiveState'] == 'active' and props['SubState'] == 'running' and int(pid) == expected['pid'] and props['NRestarts'] == '0' and actual == expected['sha256'] == deployment['serverSha256']
    ok &= same
    services.append({'service': name, **props, 'samePidAndBinaryAsRollout': same})
checks['services'] = services
installed = hashlib.file_digest(open('/projects/miy-v2/.local/bin/miy-server', 'rb'), 'sha256').hexdigest()
checks['installedBinaryIsCandidate'] = installed == deployment['serverSha256']; ok &= checks['installedBinaryIsCandidate']
db = json.loads(sql("SELECT json_build_object('migrationVersion',(SELECT max(version) FROM _sqlx_migrations),"
    "'installations',(SELECT count(*) FROM installations),"
    "'running',(SELECT count(*) FROM installations WHERE runtime_status='running'),"
    "'healthyRunning',(SELECT count(*) FROM installations WHERE runtime_status='running' AND health_status='healthy'),"
    "'activeWebSessions',(SELECT count(*) FROM sessions WHERE expires_at>now()),"
    "'activeWork',(SELECT (SELECT count(*) FROM runtime_jobs WHERE status IN ('queued','running'))+(SELECT count(*) FROM app_builds WHERE status IN ('receiving','queued','running'))+(SELECT count(*) FROM jobs WHERE status IN ('queued','running'))))"))
checks['database'] = db
ok &= db['migrationVersion'] == deployment['schema']
baseline_rows = {t: v['rows'] for t, v in deployment['stableRecordsAfter'].items()}
current_rows = {t: int(sql(f'SELECT count(*) FROM "{t}"')) for t in baseline_rows}
checks['stableRowCountsSinceRollout'] = 'unchanged' if current_rows == baseline_rows else {t: [baseline_rows[t], current_rows[t]] for t in baseline_rows if baseline_rows[t] != current_rows[t]}
ok &= current_rows == baseline_rows
before = deployment['appContainersAfter']
now = {}
for name in before:
    m = json.loads(run(['docker', 'inspect', name]))[0]
    now[name] = {'id': m['Id'], 'image': m['Image'], 'status': m['State']['Status'], 'startedAt': m['State']['StartedAt'], 'restartCount': m['RestartCount']}
checks['appContainersPreserved'] = sum(now[n] == before[n] for n in before); checks['appContainers'] = len(before)
checks['runningAppContainers'] = sum(c['status'] == 'running' for c in now.values())
ok &= checks['appContainersPreserved'] == len(before)
checks['health'] = json.load(urllib.request.urlopen('https://v2.dwdcc.kr/api/health', timeout=10)); ok &= checks['health'] == {'status': 'ok'}
budget = subprocess.run(['python3', '/projects/miy-v2/.local/release-7b43789/source/scripts/check-db-connections.py'], capture_output=True, text=True)
report = json.loads(budget.stdout)
checks['connectionBudget'] = {k: report.get(k) for k in ('status', 'worstCase', 'usableConnections')}; ok &= report.get('status') == 'PASS'
print(json.dumps({'status': 'PASS' if ok else 'FAIL', **checks}, indent=2))
