#!/usr/bin/env python3
"""Install the verified 0.7.0 server + CLI/installer index set on v2.dwdcc.kr.
Backs up binary, indexes and the miy_core database first; applies core migrations
16-17 with the new binary; restores everything (including the database) on failure."""
import hashlib, json, os, pathlib, shutil, subprocess, sys, tempfile, time, urllib.request
REL = pathlib.Path('/projects/miy-v2/.local/release-0.7.0')
LIVE = pathlib.Path('/projects/miy-v2')
SRC = REL / 'src'
BACKUP = REL / 'rollback'
COMMIT, BINARY, BINARY_SHA = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3]
DB = 'miy-v2-validation-postgres-1'
INDEXES = ['.local/cli-downloads/release-index.json', '.local/cli-installers/installer-index.json']
REPORT = {'startedAt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'commit': COMMIT, 'commands': [],
          'rollbackUsed': False, 'databaseRestored': False}
migrated = False

def save(): (REL / 'deployment.json').write_text(json.dumps(REPORT, indent=2) + '\n')
def call(args, cwd=LIVE, env=None, stdin=None, stdout=None):
    p = subprocess.run(args, cwd=cwd, env=env, stdin=stdin, stdout=stdout or subprocess.PIPE, stderr=subprocess.STDOUT if stdout is None else subprocess.PIPE, text=stdout is None)
    REPORT['commands'].append({'command': [str(a) for a in args], 'exitCode': p.returncode}); save()
    if stdout is None: (REL / 'deployment-commands.log').open('a').write(p.stdout or '')
    if p.returncode: raise RuntimeError('command_failed')
    return (p.stdout or '').strip() if stdout is None else ''
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def atomic_copy(source, destination):
    fd, name = tempfile.mkstemp(prefix=destination.name + '.070-', dir=destination.parent); os.close(fd)
    tmp = pathlib.Path(name)
    try:
        shutil.copy2(source, tmp)
        with tmp.open('rb') as f: os.fsync(f.fileno())
        os.replace(tmp, destination)
    finally: tmp.unlink(missing_ok=True)
def migrations():
    return call(['docker', 'exec', DB, 'psql', '-U', 'postgres', '-d', 'miy_core', '-At', '-c', 'SELECT max(version) FROM _sqlx_migrations'])
def restore():
    REPORT['rollbackUsed'] = True; save()
    subprocess.run(['sudo', '-n', 'systemctl', 'stop', 'miy-v2-serve', 'miy-v2-worker'])
    for rel in ['.local/bin/miy-server', *INDEXES]: atomic_copy(BACKUP / rel, LIVE / rel)
    if migrated:
        with (BACKUP / 'miy_core.dump').open('rb') as dump:
            call(['docker', 'exec', '-i', DB, 'pg_restore', '-U', 'postgres', '-d', 'miy_core', '--clean', '--if-exists', '--single-transaction'], stdin=dump, stdout=subprocess.DEVNULL)
        REPORT['databaseRestored'] = True
    call(['sudo', '-n', 'systemctl', 'start', 'miy-v2-serve', 'miy-v2-worker'])
    REPORT['status'] = 'ROLLED_BACK'; save()

# Preconditions: exact binary, staged indexes for the same commit/version.
assert digest(BINARY) == BINARY_SHA and call([str(BINARY), '--version']) == 'miy 0.7.0'
cli = REL / 'staging/cli-downloads'; inst = REL / 'staging/cli-installers'
ci = json.loads((cli / 'release-index.json').read_text()); wi = json.loads((inst / 'installer-index.json').read_text())
win = next(a for a in ci['assets'] if a['target'] == 'x86_64-pc-windows-msvc')
assert ci['version'] == wi['version'] == '0.7.0' and ci['commit'] == wi['cliCommit'] == wi['installerCommit'] == COMMIT
assert win['sha256'] == wi['archiveSha256'] and win['binarySha256'] == wi['cliBinarySha256']
assert migrations() == '15'
REPORT['serverSha256'] = BINARY_SHA; save()

# Backup (verified readable) before any change.
BACKUP.mkdir(parents=True, exist_ok=True)
for rel in ['.local/bin/miy-server', *INDEXES]:
    (BACKUP / rel).parent.mkdir(parents=True, exist_ok=True); shutil.copy2(LIVE / rel, BACKUP / rel)
REPORT['backupHashes'] = {rel: digest(BACKUP / rel) for rel in ['.local/bin/miy-server', *INDEXES]}
try:
    call(['sudo', '-n', 'systemctl', 'stop', 'miy-v2-serve', 'miy-v2-worker'])
    with (BACKUP / 'miy_core.dump').open('wb') as dump:
        call(['docker', 'exec', DB, 'pg_dump', '-U', 'postgres', '-Fc', 'miy_core'], stdout=dump)
    os.chmod(BACKUP / 'miy_core.dump', 0o600)
    with (BACKUP / 'miy_core.dump').open('rb') as dump:
        listing = subprocess.run(['docker', 'exec', '-i', DB, 'pg_restore', '--list'], stdin=dump, capture_output=True, text=True)
    assert listing.returncode == 0 and 'TABLE DATA public installations' in listing.stdout, 'backup_unreadable'
    REPORT['databaseBackup'] = {'bytes': (BACKUP / 'miy_core.dump').stat().st_size, 'sha256': digest(BACKUP / 'miy_core.dump'), 'migrationVersion': 15}
    save()
    env = {**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local'), 'MIY_BINARY': str(BINARY)}
    migrated = True
    call(['bash', 'scripts/run-service.sh', 'migrate'], env=env)
    assert migrations() == '17', 'migration_version'
    call(['python3', 'scripts/stage-cli-release.py', '--source', str(cli / COMMIT), '--destination', str(LIVE / '.local/cli-downloads'), '--commit', COMMIT], cwd=SRC)
    call(['python3', 'scripts/stage-windows-installer.py', '--source', str(inst / COMMIT), '--destination', str(LIVE / '.local/cli-installers'), '--installer-commit', COMMIT, '--cli-commit', COMMIT], cwd=SRC)
    atomic_copy(BINARY, LIVE / '.local/bin/miy-server')
    call(['sudo', '-n', 'systemctl', 'start', 'miy-v2-serve', 'miy-v2-worker'])
    for _ in range(60):
        try:
            with urllib.request.urlopen('https://v2.dwdcc.kr/api/health', timeout=3) as r: health = json.load(r)
            with urllib.request.urlopen('https://v2.dwdcc.kr/api/v1/cli/install', timeout=3) as r: guide = json.load(r)
            assert health == {'status': 'ok'} and guide['cliVersion'] == '0.7.0' and guide['sourceCommit'] == COMMIT
            assert guide['windowsInstaller']['sha256'] == wi['sha256']
            REPORT['health'] = health; REPORT['installGuide'] = {k: guide[k] for k in ['cliVersion', 'sourceCommit']}; break
        except Exception: time.sleep(1)
    else: raise RuntimeError('post_deployment_health_failed')
    call(['systemctl', 'is-active', 'miy-v2-serve', 'miy-v2-worker'])
    REPORT['status'] = 'PASS'
except Exception as e:
    REPORT['failureCategory'] = str(e) or type(e).__name__; save(); restore(); raise
finally:
    REPORT['finishedAt'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()); save()
print(json.dumps({'status': REPORT['status'], 'serverSha256': BINARY_SHA, 'rollbackUsed': REPORT['rollbackUsed']}))
