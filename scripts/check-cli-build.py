#!/usr/bin/env python3
"""Real CLI remote-build recovery against an isolated loopback fault fixture.

Validates source transfer, provenance, journals and direct-image deploy without
executing Docker or language tools. Actual worker builds are validated separately.
"""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tarfile
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlsplit
import uuid

ROOT = Path(__file__).resolve().parent.parent
APP = 'remote-build-fixture'
USER = 'fixture-developer'
ACCESS = 'miy_access_' + 'a' * 48
REFRESH = 'miy_refresh_' + 'r' * 48
DEVICE = 'miy_device_' + 'd' * 48
SECRET = 'fixture-secret-must-not-upload'
IMAGE = 'sha256:' + 'a' * 64


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def component_fixture(project):
    source = ROOT / 'registry/components/ui/1.0.1'
    metadata = json.loads((source / 'component.json').read_text())
    files = {name: (source / name).read_text() for name in sorted(metadata['files'])}
    package = {key: metadata[key] for key in ['protocol', 'name', 'version', 'description', 'uiContract', 'bundleVersions']}
    package['files'] = files
    digest = hashlib.sha256(json.dumps(package, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    entry = {key: package[key] for key in ['version', 'description', 'uiContract', 'bundleVersions']}
    entry.update(sha256=digest, files={name: hashlib.sha256(text.encode()).hexdigest() for name, text in files.items()})
    for name, text in files.items():
        path = project / 'platform-components/ui' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    (project / 'platform-components/miy.components.lock.json').write_text(json.dumps({
        'protocol': metadata['protocol'], 'registry': 'http://localhost:18000', 'components': {'ui': entry},
    }))


def check(cli):
    passed = []

    def ok(name):
        passed.append(name)
        print('PASS ' + name, flush=True)

    with tempfile.TemporaryDirectory(prefix='miy-cli-build-') as temporary:
        root = Path(temporary).resolve()
        config = root / 'private-config'
        project = root / '한글 committed project'
        tools = root / 'forbidden-tools'
        tools.mkdir()
        tool_log = root / 'forbidden-tool-calls'
        for tool in ['docker', 'cargo', 'rustc', 'pnpm', 'npm', 'node', 'ssh']:
            (tools / tool).write_text('#!/bin/sh\nprintf "%s\\n" "' + tool + '" >> "$MIY_FORBIDDEN_TOOL_LOG"\nexit 97\n')
            (tools / tool).chmod(0o700)
        env = {**os.environ, 'MIY_CONFIG_DIR': str(config), 'MIY_CREDENTIAL_BACKEND': 'file',
               'RUST_LOG': 'off', 'NO_PROXY': '127.0.0.1,localhost', 'no_proxy': '127.0.0.1,localhost',
               'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': os.devnull,
               'PATH': str(tools) + os.pathsep + os.environ.get('PATH', ''), 'MIY_FORBIDDEN_TOOL_LOG': str(tool_log)}
        env.pop('MIY_CONTEXT', None)
        state = {'jobs': {}, 'requests': {}, 'uploads': [], 'runtime_posts': [], 'runtime_job': None,
                 'failures': [], 'lose_build_ack': True, 'hide_lookup': 1, 'lose_runtime_ack': True,
                 'next_status': 'queued', 'override': {}, 'get_paths': [], 'fail_get_once': 0}
        login_id = str(uuid.uuid4())
        session_id = str(uuid.uuid4())

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def reply(self, status, value, cookie=False):
                encoded = json.dumps(value).encode()
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(encoded)))
                if cookie:
                    self.send_header('Set-Cookie', 'fixture=synthetic; Path=/; HttpOnly')
                self.end_headers()
                try:
                    self.wfile.write(encoded)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def authenticated(self):
                require(self.headers.get('Authorization') == 'Bearer ' + ACCESS
                        or self.headers.get('Cookie') == 'fixture=synthetic', 'Missing fixture authentication')

            def drop(self):
                self.close_connection = True
                try:
                    self.connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                self.connection.close()

            def do_GET(self):
                try:
                    self.get()
                except Exception as error:
                    state['failures'].append(str(error))
                    self.reply(500, {'error': {'code': 'fixture_failed'}})

            def get(self):
                parsed = urlsplit(self.path)
                path, query = parsed.path, parse_qs(parsed.query)
                state['get_paths'].append(path)
                if path != '/api/v1/capabilities':
                    self.authenticated()
                if path == '/api/v1/capabilities':
                    self.reply(200, {'schemaVersion': 1, 'cliContractVersion': 1, 'runtimePlatform': 'linux/amd64',
                                    'features': ['auth.device.v1', 'auth.sessions.v1', 'app.builds.v1',
                                                 'runtime.preflight.v2', 'runtime.jobs.v1', 'runtime.events.v1']})
                elif path == '/api/v1/me':
                    self.reply(200, {'id': USER, 'csrf': 'fixture-csrf'})
                elif path.startswith(f'/api/v1/apps/{APP}/build-requests/'):
                    request = path.rsplit('/', 1)[-1]
                    if state['hide_lookup']:
                        state['hide_lookup'] -= 1
                        self.reply(503, {'error': {'code': 'lookup_unavailable'}})
                    elif request not in state['requests']:
                        self.reply(404, {'error': {'code': 'not_found'}})
                    else:
                        self.reply(200, {**state['jobs'][state['requests'][request]], 'requestId': request, **state['override']})
                elif path.startswith('/api/v1/builds/'):
                    parts = path.split('/')
                    job = state['jobs'].get(parts[4])
                    if not job:
                        self.reply(404, {'error': {'code': 'not_found'}})
                    elif path.endswith('/logs'):
                        sequence = int(query.get('after', ['0'])[0])
                        lines = [{'sequence': 1, 'createdAt': '2026-01-01T00:00:00Z', 'text': 'Fixture build accepted.'},
                                 {'sequence': 2, 'createdAt': '2026-01-01T00:00:01Z', 'text': 'Fixture build complete.'}]
                        self.reply(200, {'id': job['id'], 'jobId': job['id'], 'logs': [line for line in lines if line['sequence'] > sequence],
                                         'nextSequence': max(sequence, 2), 'terminal': job['status'] in ['failed', 'succeeded'],
                                         'status': job['status'], 'truncated': False, 'maxBytes': 2097152})
                    elif state['fail_get_once']:
                        state['fail_get_once'] -= 1
                        self.reply(503, {'error': {'code': 'fixture_observation_unavailable'}})
                    else:
                        # The canonical GET identity can differ from a deduplicated request alias.
                        self.reply(200, {**job, 'requestId': job['canonicalRequestId'], **state['override']})
                elif path == '/api/v1/runtime/jobs':
                    job = state['runtime_job']
                    self.reply(200, {'jobs': [job] if job and query.get('requestId') == [job['requestId']] else [], 'nextCursor': None})
                elif state['runtime_job'] and path == '/api/v1/runtime/jobs/' + state['runtime_job']['id']:
                    self.reply(200, state['runtime_job'])
                else:
                    self.reply(404, {'error': {'code': 'not_found'}})

            def do_POST(self):
                try:
                    self.post()
                except Exception as error:
                    state['failures'].append(str(error))
                    self.reply(500, {'error': {'code': 'fixture_failed'}})

            def post(self):
                raw = self.rfile.read(int(self.headers.get('Content-Length', '0')))
                if self.path == f'/api/v1/apps/{APP}/builds':
                    self.authenticated()
                    request = self.headers.get('x-miy-request-id')
                    require(str(uuid.UUID(request)) == request, 'Missing immutable build request UUID')
                    entry = json.loads((config / 'build-journal' / (request + '.json')).read_text())
                    require(entry['submissionStatus'] == 'sending' and entry['requestId'] == request,
                            'Build upload preceded durable journal')
                    require(entry['archiveHash'] == hashlib.sha256(raw).hexdigest() == self.headers.get('x-miy-archive-sha256')
                            and entry['sizeBytes'] == len(raw), 'Source digest/size differs from durable journal')
                    require(entry['sourceCommit'] == self.headers.get('x-miy-source-commit') and entry['platform'] == self.headers.get('x-miy-platform'),
                            'Committed source/platform differs from journal')
                    require(entry['origin'] == origin and entry['userId'] == USER and entry['context'] == 'fixture', 'Journal identity is unbound')
                    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
                        names = archive.getnames()
                        require(all(member.isfile() for member in archive.getmembers()), 'Source contains a link/special file')
                        require('src/main.rs' in names and 'manifest.json' in names, 'Source snapshot is incomplete')
                        require(not any(name.startswith(('.git/', '.miy/', '.miy-tools/', 'dist/', 'target/')) or name in ['.env', '.mcp.json', 'src/.env'] for name in names),
                                'Private/generated files entered source archive')
                        require(SECRET.encode() not in raw, 'Known credential fixture was uploaded')
                    state['uploads'].append((request, hashlib.sha256(raw).hexdigest()))
                    if request in state['requests']:
                        job = state['jobs'][state['requests'][request]]
                        job['status'] = 'queued'
                    else:
                        job_id = str(uuid.uuid4())
                        job = {'id': job_id, 'requestId': request, 'canonicalRequestId': request,
                               'actor': USER, 'userId': USER, 'appId': APP, 'environment': 'dev', 'action': 'build',
                               'status': state['next_status'], 'step': 'queued', 'errorCode': None, 'imageId': None,
                               'sourceCommit': entry['sourceCommit'], 'archiveHash': entry['archiveHash'], 'sizeBytes': entry['sizeBytes'],
                               'platform': entry['platform'], 'version': entry['version'], 'bundleVersion': entry['bundleVersion'], 'retryable': False}
                        state['jobs'][job_id] = job
                        state['requests'][request] = job_id
                    if state['lose_build_ack']:
                        state['lose_build_ack'] = False
                        self.drop()
                    else:
                        self.reply(200, job)
                    return
                body = json.loads(raw or b'{}')
                if self.path == '/auth/cli/device':
                    self.reply(200, {'id': login_id, 'deviceCode': DEVICE, 'userCode': 'ABCDE-FGHJK',
                                     'verificationPath': f'/cli/device/{login_id}', 'expiresIn': 300, 'interval': 1})
                elif self.path == '/auth/development':
                    self.reply(200, {}, cookie=True)
                elif self.path == f'/api/v1/cli/devices/{login_id}/decision':
                    self.authenticated()
                    self.reply(200, {'approved': True})
                elif self.path == '/auth/cli/token':
                    self.reply(200, {'status': 'approved', 'sessionId': session_id, 'userId': USER, 'accessToken': ACCESS,
                                     'refreshToken': REFRESH, 'tokenType': 'Bearer', 'expiresIn': 900,
                                     'refreshExpiresAt': (datetime.now(timezone.utc) + timedelta(days=30)).isoformat()})
                elif self.path == f'/api/v1/apps/{APP}/runtime/preflight':
                    self.authenticated()
                    self.reply(200, {'input': body, 'expectedState': {'target': body['target'], 'installation': None},
                                     'checks': [], 'changes': [], 'canSubmit': True, 'fullyChecked': True,
                                     'desiredState': 'running', 'requiresConfirmation': False, 'preservesDatabase': True})
                elif self.path == f'/api/v1/apps/{APP}/runtime':
                    self.authenticated()
                    entry = json.loads((config / 'journal' / (body['idempotencyKey'] + '.json')).read_text())
                    require(entry['payload'] == body and entry['submissionStatus'] == 'sending', 'Deployment preceded durable payload')
                    require(entry['upload'] is None and entry['serverBuild']['imageId'] == IMAGE, 'Server build attempted image upload')
                    require(body['imageId'] == IMAGE and body['action'] == 'deploy', 'Deployment did not use server image directly')
                    state['runtime_posts'].append(body)
                    if state['lose_runtime_ack']:
                        state['lose_runtime_ack'] = False
                        self.drop()
                    else:
                        state['runtime_job'] = {'id': str(uuid.uuid4()), 'requestId': body['idempotencyKey'], 'appId': APP,
                                                'environment': 'dev', 'action': 'deploy', 'imageId': IMAGE, 'status': 'succeeded'}
                        self.reply(200, state['runtime_job'])
                else:
                    raise AssertionError('Unexpected mutating request: ' + self.path)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        origin = f'http://127.0.0.1:{server.server_port}'
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def call(args, expected=0, cwd=None, output='json'):
            result = subprocess.run([str(cli), '--output', output, '--no-input', *args], cwd=cwd or project,
                                    env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=40)
            require(not state['failures'], '; '.join(state['failures']))
            require(all(secret.encode() not in result.stdout + result.stderr for secret in [ACCESS, REFRESH, DEVICE, SECRET]), 'Credential fixture leaked to output')
            if output == 'jsonl':
                require(result.returncode == expected, 'JSONL observation failed')
                return [json.loads(line) for line in result.stdout.splitlines()]
            value = json.loads(result.stdout)
            require(result.returncode == expected and value['ok'] == (expected == 0),
                    f'CLI result mismatch for {args}: expected {expected}, got {result.returncode}, code={value.get("error", {}).get("code") if value.get("error") else None}')
            return value

        def git(*args):
            result = subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', *args],
                                    cwd=project, env=env, capture_output=True, check=True, timeout=30)
            return result.stdout.decode().strip()

        def change_source(label):
            with (project / 'src/main.rs').open('a') as file:
                file.write('\n// ' + label + '\n')
            git('add', 'src/main.rs')
            git('commit', '--quiet', '-m', label)

        try:
            call(['context', 'add', 'fixture', '--url', origin], cwd=root)
            call(['--context', 'fixture', 'auth', 'login', '--account', 'developer'], cwd=root)
            call(['--context', 'fixture', 'app', 'init', str(project), '--id', APP, '--name', 'Remote build fixture'], cwd=root)
            component_fixture(project)
            (project / '.env').write_text(SECRET)
            (project / 'src/.env').write_text(SECRET)
            git('-c', 'init.templateDir=', 'init', '--quiet')
            git('add', '.')
            git('commit', '--quiet', '-m', 'Committed synthetic app')
            dirty = project / 'src/uncommitted.rs'
            dirty.write_text('// uncommitted source must not reach server')
            invalid = call(['--context', 'fixture', 'app', 'build', '--remote', '--no-wait'], expected=6)
            require(invalid['error']['code'] == 'commit_source_before_building' and not state['uploads'], 'Dirty source was submitted')
            dirty.unlink()
            ok('dirty source is rejected before upload without local tool execution')

            result = call(['--context', 'fixture', 'app', 'build', '--remote', '--no-wait'], expected=8)
            require(result['error']['code'] == 'build_submission_unknown', 'Lost acknowledgement lost recovery semantics')
            request = result['error']['details']['requestId']
            require(len(state['uploads']) == 1 and (config / 'build-journal' / (request + '.tar')).is_file(), 'Lost acknowledgement duplicated upload or discarded source')
            ok('committed filtered source, durable actor-bound journal and lost upload acknowledgement')

            resumed = call(['--context', 'fixture', 'job', 'resume', request, '--no-wait'])['data']
            build_id = resumed['id']
            call(['--context', 'fixture', 'job', 'get', build_id])
            call(['--context', 'fixture', 'app', 'build', '--remote', '--no-wait'])
            require(len(state['uploads']) == 1, 'Resume/repeated build duplicated accepted source transfer')
            state['override'] = {'archiveHash': 'f' * 64}
            call(['--context', 'fixture', 'job', 'resume', request, '--no-wait'], expected=8)
            state['override'] = {}
            ok('accepted build recovery and receipt mismatch rejection without another upload')

            job = state['jobs'][build_id]
            job.update(status='succeeded', imageId=IMAGE, step='completed')
            job['canonicalRequestId'] = str(uuid.uuid4())
            result = call(['--context', 'fixture', 'job', 'resume', build_id])['data']
            require(result['metadataWritten'] and json.loads((project / 'dist/image.json').read_text())['serverBuild']['jobId'] == build_id,
                    'Completed server build did not publish local deployment receipt')
            logs = call(['--context', 'fixture', 'job', 'logs', build_id])['data']
            require(len(logs['logs']) == 2 and logs['nextSequence'] == 2, 'Build logs lost their sequence')
            lines = call(['--context', 'fixture', 'job', 'watch', build_id, '--after', '0'], output='jsonl')
            require(len(lines) == 3 and lines[-1]['data']['status'] == 'succeeded', 'JSONL build observation omitted logs or terminal result')
            ok('canonical build identity, completed receipts, bounded logs and JSONL observation')

            call(['--context', 'fixture', 'app', 'deploy', '--dry-run'])
            require(not state['runtime_posts'], 'Dry-run submitted deployment')
            result = call(['--context', 'fixture', 'app', 'deploy', '--no-wait'], expected=8)
            runtime_request = result['error']['details']['requestId']
            call(['--context', 'fixture', '--app', APP, 'job', 'resume', runtime_request, '--environment', 'dev', '--no-wait'])
            require(len(state['runtime_posts']) == 2 and state['runtime_posts'][0] == state['runtime_posts'][1], 'Deployment recovery changed original request')
            require(not tool_log.exists(), 'Remote build or deployment invoked local Docker/language tools/SSH')
            ok('direct server-image deployment and unknown-result recovery without Docker export')

            image_file = project / 'dist/image.json'
            original = image_file.read_bytes()
            image = json.loads(original)
            image['serverBuild']['origin'] = 'https://wrong.example'
            image_file.write_text(json.dumps(image))
            before = len(state['runtime_posts'])
            call(['--context', 'fixture', 'app', 'deploy', '--no-wait'], expected=6)
            image_file.write_bytes(original)
            require(len(state['runtime_posts']) == before, 'Foreign receipt caused deployment')
            ok('cross-origin receipt refuses deployment before mutation')

            change_source('timeout fixture')
            result = call(['--context', 'fixture', 'app', 'build', '--remote', '--timeout', '1'], expected=8)
            request = result['error']['details']['requestId']
            pending = call(['--context', 'fixture', 'job', 'resume', request, '--no-wait'])['data']
            require(pending['status'] == 'queued', 'Timeout cancelled accepted build')
            count = len(state['uploads'])
            state['jobs'][pending['id']].update(status='failed', errorCode='fixture_build_failed')
            call(['--context', 'fixture', 'job', 'resume', request, '--no-wait'], expected=9)
            require(len(state['uploads']) == count, 'Resuming terminal failure started another build')
            call(['--context', 'fixture', 'app', 'build', '--remote', '--no-wait'])
            require(len(state['uploads']) == count + 1 and state['uploads'][-1][0] != request, 'Explicit new build reused failed request')
            ok('timeout preserves remote work; terminal failure resumes without rebuilding; explicit build starts new attempt')

            change_source('receiving fixture')
            state['next_status'] = 'receiving'
            received = call(['--context', 'fixture', 'app', 'build', '--remote', '--no-wait'])['data']
            call(['--context', 'fixture', 'job', 'resume', received['requestId'], '--no-wait'])
            require(state['uploads'][-1] == state['uploads'][-2], 'Receiving recovery changed the source or request identity')
            ok('interrupted receiving state resumes the same whole source archive and request')

            change_source('network observation fixture')
            state['next_status'] = 'queued'
            state['fail_get_once'] = 1
            result = call(['--context', 'fixture', 'app', 'build', '--remote'], expected=8)
            require(result['error']['details']['requestId'] and result['error']['recovery']['args'], 'Network failure lost build recovery identity')
            count = len(state['uploads'])
            call(['--context', 'fixture', 'job', 'resume', result['error']['details']['requestId'], '--no-wait'])
            require(len(state['uploads']) == count, 'Network observation recovery duplicated build')
            ok('network loss during observation retains original request and usable recovery command')

            if os.name != 'nt':
                change_source('interrupt fixture')
                state['next_status'] = 'queued'
                count = len(state['uploads'])
                process = subprocess.Popen([str(cli), '--output', 'json', '--no-input', '--context', 'fixture', 'app', 'build', '--remote'],
                                           cwd=project, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    deadline = time.monotonic() + 30
                    while len(state['uploads']) == count and process.poll() is None and time.monotonic() < deadline:
                        time.sleep(0.05)
                    require(len(state['uploads']) == count + 1, 'Interrupt fixture never submitted build')
                    time.sleep(0.3)
                    process.send_signal(signal.SIGINT)
                    stdout, stderr = process.communicate(timeout=10)
                    value = json.loads(stdout)
                    require(process.returncode == 130 and value['error']['details']['remoteWorkCancelled'] is False, 'Interrupt cancelled or lost accepted build')
                    call(['--context', 'fixture', 'job', 'resume', value['error']['details']['requestId'], '--no-wait'])
                    require(len(state['uploads']) == count + 1, 'Interrupt recovery duplicated accepted build')
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
                ok('Ctrl-C stops observation and resumes the accepted request without cancellation or duplication')
            require(not state['failures'] and not tool_log.exists(), 'Fixture failed or invoked a forbidden local build tool')
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
    return {'passed': len(passed), 'cases': passed, 'realWorkerBuild': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.cli.resolve()), indent=2))
