#!/usr/bin/env python3
"""Run an acceptance agent inside a fresh mount/PID/network namespace.

The caller supplies the app, a private state directory, a prepared state/miy
configuration, and an already-running, policy-enforcing Unix HTTP proxy. Only
the selected agent's OAuth file is copied from the real home. No model is chosen
here. --check performs the same isolation probes without invoking an agent.
Agent output is private; this process emits only exit/timing/fingerprint data.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import signal
import stat
import subprocess
import sys
import time
import uuid


REPOSITORY = Path("/projects/miy-v2")
CLAUDE_VENDOR = REPOSITORY / ".local/acceptance-tools/node_modules"
PROXY_ADDRESS = "http://127.0.0.1:18099"

# All egress is mediated by the caller's Unix proxy; there is no host netns.
BRIDGE = r"""const net = require('node:net');
const server = net.createServer(client => {
  const upstream = net.connect('/run/acceptance-egress.sock');
  client.on('error', () => upstream.destroy());
  upstream.on('error', () => client.destroy());
  client.on('close', () => upstream.destroy());
  upstream.on('close', () => client.destroy());
  client.pipe(upstream); upstream.pipe(client);
});
server.listen(18099, '127.0.0.1');"""

# Executed via python -c, so the repository containing this launcher is not
# mounted. All its stdout/stderr goes to the parent's private log descriptors.
SUPERVISOR = r"""
import json, os, pathlib, signal, socket, stat, subprocess, sys, tempfile, time, urllib.request
spec = json.loads(sys.argv[1])
bridge = None
child = None
def stop(signum, frame):
    if child is not None and child.poll() is None:
        child.terminate()
    if bridge is not None and bridge.poll() is None:
        bridge.terminate()
    raise SystemExit(128 + signum)
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
try:
    assert os.readlink('/proc/self/ns/net') != spec['hostNet'], 'network_namespace_shared'
    assert os.readlink('/proc/self/ns/pid') != spec['hostPid'], 'pid_namespace_shared'
    assert os.path.samefile(spec['miyConfig'], pathlib.Path.home() / '.config/miy'), 'miy_fallback_scope_mismatch'
    for path in spec['forbidden']:
        assert not os.path.lexists(path), 'forbidden_host_path_visible'
    assert stat.S_ISSOCK(os.stat('/run/acceptance-egress.sock').st_mode), 'proxy_not_socket'
    for address in [('1.1.1.1', 443), ('127.0.0.1', 18180), ('127.0.0.1', 15432)]:
        try:
            direct = socket.create_connection(address, timeout=0.6)
        except OSError:
            continue
        else:
            direct.close()
            raise AssertionError('direct_network_accessible')
    bridge = subprocess.Popen(['/usr/bin/node', '-e', spec['bridge']], stdin=subprocess.DEVNULL)
    for attempt in range(100):
        if bridge.poll() is not None:
            raise AssertionError('proxy_bridge_exited')
        try:
            probe = socket.create_connection(('127.0.0.1', 18099), timeout=0.1)
            probe.close()
            break
        except OSError:
            time.sleep(0.03)
    else:
        raise AssertionError('proxy_bridge_unavailable')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({
        'http': spec['proxy'], 'https': spec['proxy']}))
    with opener.open('http://miy-acceptance.invalid/api/v1/capabilities', timeout=15) as response:
        assert response.status == 200, 'proxy_capabilities_failed'
        capabilities = json.loads(response.read(1048576))
        assert isinstance(capabilities, dict), 'invalid_capabilities'
    if spec['check']:
        with tempfile.TemporaryDirectory(prefix='compiler-probe-') as directory:
            for compiler in ('cc', 'gcc'):
                executable = str(pathlib.Path(directory) / compiler)
                compiled = subprocess.run(
                    ['/usr/bin/' + compiler, '-x', 'c', '-', '-o', executable],
                    input=b'#include <stdio.h>\nint main(void) { return 0; }\n',
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
                assert compiled.returncode == 0, compiler + '_compile_failed'
                executed = subprocess.run([executable], stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5)
                assert executed.returncode == 0, compiler + '_execution_failed'
        mcp_enabled = None
        if spec['agent'] == 'codex':
            configured = subprocess.run(
                ['/usr/local/bin/codex', 'mcp', 'list', '--json'],
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, timeout=30)
            assert configured.returncode == 0, 'codex_mcp_list_failed'
            servers = json.loads(configured.stdout)
            assert isinstance(servers, list), 'codex_mcp_list_invalid'
            assert len(servers) == 1 and isinstance(servers[0], dict), 'codex_mcp_scope_mismatch'
            mcp_enabled = servers[0].get('name') == 'miy_app' and servers[0].get('enabled') is True
            assert mcp_enabled, 'codex_miy_app_not_enabled'
        print(json.dumps({'check': 'passed', 'networkIsolated': True,
              'hostSourceHidden': True, 'hostSocketsHidden': True,
              'proxyCapabilities': 200, 'ccCompileAndRun': True,
              'gccCompileAndRun': True, 'codexMcpEnabled': mcp_enabled}), flush=True)
        raise SystemExit(0)
    child = subprocess.Popen(spec['command'], stdin=sys.stdin.buffer)
    raise SystemExit(child.wait())
except (AssertionError, OSError, ValueError, subprocess.SubprocessError) as error:
    # Do not serialize proxy response bodies, credentials, prompts or argv.
    code = str(error) if isinstance(error, AssertionError) else type(error).__name__
    print(json.dumps({'sandboxError': code}), file=sys.stderr, flush=True)
    raise SystemExit(78)
finally:
    if child is not None and child.poll() is None:
        child.terminate()
    if bridge is not None:
        bridge.terminate()
        try:
            bridge.wait(timeout=3)
        except subprocess.TimeoutExpired:
            bridge.kill()
            bridge.wait()
"""


class SandboxError(Exception):
    pass


def absolute_path(value):
    path = Path(value)
    if not path.is_absolute() or ".." in path.parts:
        raise argparse.ArgumentTypeError("an absolute path without '..' is required")
    return path


def private_dir(path):
    """Refuse symlinks, including ancestor links, before private writes."""
    for item in [*reversed(path.parents), path]:
        if item.is_symlink():
            raise SandboxError("symlink_in_private_path")
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    info = path.stat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid():
        raise SandboxError("private_directory_owner_mismatch")
    path.chmod(0o700)


def read_regular(path, maximum=4 * 1024 * 1024):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
            raise SandboxError("private_file_owner_mismatch")
        data = stream.read(maximum + 1)
        if len(data) > maximum:
            raise SandboxError("private_file_too_large")
        return data


def write_private(path, data):
    private_dir(path.parent)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        os.fchmod(stream.fileno(), 0o600)
        stream.write(data)


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def copy_cache(source, destination):
    if destination.exists() or not source.is_dir():
        return
    if source.is_symlink():
        raise SandboxError("cache_source_symlink")
    private_dir(destination.parent)
    result = subprocess.run(
        ["/usr/bin/cp", "-a", "--reflink=auto", "--", str(source), str(destination)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=300,
        env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"},
    )
    if result.returncode:
        raise SandboxError("cache_copy_failed")


def prepare_profile(args, original_home):
    private_dir(args.state)
    if args.state == original_home:
        raise SandboxError("state_is_real_home")
    private_dir(args.state / "private")
    marker_path = args.state / "private/profile.json"
    expected = {"schemaVersion": 1, "agent": args.agent, "app": str(args.app)}
    if marker_path.exists():
        if json.loads(read_regular(marker_path)) != expected:
            raise SandboxError("state_profile_mismatch")
    else:
        for name in ("home", "cargo", "target"):
            existing = args.state / name
            if existing.exists() and (not existing.is_dir() or any(existing.iterdir())):
                raise SandboxError("new_profile_requires_empty_state")
        private_dir(args.state / "home")
        profile_home = args.state / "home"
        if args.agent == "codex":
            write_private(profile_home / ".codex/auth.json",
                          read_regular(original_home / ".codex/auth.json"))
            write_private(profile_home / ".codex/config.toml",
                          ("[projects." + json.dumps(str(args.app)) + "]\n"
                           'trust_level = "trusted"\n').encode())
        else:
            write_private(profile_home / ".claude/.credentials.json",
                          read_regular(original_home / ".claude/.credentials.json"))
            original = json.loads(read_regular(original_home / ".claude.json"))
            # Account metadata only: no model settings, projects, memory, hooks,
            # plugins, experiment caches, user instructions or conversation data.
            minimal = {key: original[key] for key in ("oauthAccount", "userID") if key in original}
            minimal.update({"hasCompletedOnboarding": True, "projects": {
                str(args.app): {"hasTrustDialogAccepted": True,
                                "enabledMcpjsonServers": ["miy_app"]}}})
            write_private(profile_home / ".claude.json", json_bytes(minimal))
            write_private(profile_home / ".claude/settings.json", json_bytes({
                "permissions": {"defaultMode": "acceptEdits"},
                "enableAllProjectMcpServers": False,
                "enabledMcpjsonServers": ["miy_app"],
            }))
        write_private(profile_home / ".gitconfig", (
            "[user]\n\tname = Miy Acceptance\n\temail = acceptance@example.invalid\n"
            "[commit]\n\tgpgSign = false\n[tag]\n\tgpgSign = false\n").encode())
        write_private(marker_path, json_bytes(expected))
    for name in ("home", "cargo", "target"):
        private_dir(args.state / name)
    copy_cache(original_home / ".cargo/registry", args.state / "cargo/registry")
    copy_cache(original_home / ".cache/node/corepack",
               args.state / "home/.cache/node/corepack")
    for relative in (".cargo/bin", ".rustup", ".local/share/pnpm"):
        private_dir(args.state / "home" / relative)


def agent_command(args):
    if args.agent == "codex":
        # This HOME is a private profile containing only generated project trust
        # and the selected OAuth copy. Read it so project MCP config is loaded;
        # Codex 0.166 does not support projects.* in session config overrides.
        command = ["/usr/local/bin/codex", "exec", "--json",
                   "-s", "danger-full-access", "-c", 'approval_policy="never"',
                   "-c", "features.multi_agent=false", "-c",
                   'mcp_servers.miy_app.env_vars=["MIY_CONFIG_DIR","MIY_CREDENTIAL_BACKEND",'
                   '"HTTP_PROXY","HTTPS_PROXY","http_proxy","https_proxy","NO_PROXY","no_proxy"]']
        if args.resume:
            command += ["resume", str(args.resume), "-"]
        else:
            command += ["-"]
        return command
    command = ["/tools/claude/.bin/claude", "-p", "--output-format", "stream-json",
               "--verbose", "--permission-mode", "acceptEdits", "--allowedTools",
               "Bash Read Edit Write Glob Grep mcp__miy_app__*", "--tools",
               "Bash,Read,Edit,Write,Glob,Grep", "--strict-mcp-config",
               "--mcp-config", str(args.app / ".mcp.json")]
    if args.resume:
        command += ["--resume", str(args.resume)]
    return command


def sandbox_command(args, original_home, private_files):
    command = ["/usr/bin/bwrap", "--unshare-all", "--unshare-user", "--die-with-parent", "--new-session",
               "--cap-drop", "ALL", "--disable-userns", "--clearenv"]
    for path in ("/usr", "/bin", "/lib", "/lib64"):
        if Path(path).exists():
            command += ["--ro-bind", path, path]
    command += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                "--dir", "/run", "--dir", "/etc", "--dir", "/tools"]
    for name in ("passwd", "group", "hosts", "resolv.conf", "nsswitch.conf"):
        command += ["--ro-bind", str(private_files / name), "/etc/" + name]
    command += ["--ro-bind", "/etc/ssl/certs", "/etc/ssl/certs",
                "--ro-bind", "/etc/alternatives", "/etc/alternatives",
                "--bind", str(args.state / "home"), str(original_home),
                "--ro-bind", str(original_home / ".cargo/bin"), str(original_home / ".cargo/bin"),
                "--ro-bind", str(original_home / ".rustup"), str(original_home / ".rustup"),
                "--bind", str(args.app), str(args.app)]
    for name in ("cargo", "target", "miy"):
        command += ["--bind", str(args.state / name), str(args.state / name)]
    # Codex's MCP environment whitelist can omit MIY_CONFIG_DIR. Both the
    # explicit CLI path and the isolated HOME fallback must select the same
    # synthetic context; no original user's MIY config is mounted here.
    command += ["--bind", str(args.state / "miy"), str(original_home / ".config/miy")]
    command += ["--ro-bind", str(args.socket), "/run/acceptance-egress.sock"]
    if args.agent == "claude":
        command += ["--ro-bind", str(CLAUDE_VENDOR), "/tools/claude"]
    environment = {
        # HOME retains its original value. Its mount contains the new profile;
        # CODEX_HOME and all host secrets/config overrides are intentionally absent.
        "HOME": str(original_home), "USER": pwd.getpwuid(os.getuid()).pw_name,
        "LOGNAME": pwd.getpwuid(os.getuid()).pw_name, "SHELL": "/bin/bash",
        "PATH": f"{args.app}/.miy-tools:{original_home}/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
        "LANG": "C.UTF-8", "TERM": "dumb", "TMPDIR": "/tmp",
        "CARGO_HOME": str(args.state / "cargo"), "CARGO_TARGET_DIR": str(args.state / "target"),
        "RUSTUP_HOME": str(original_home / ".rustup"),
        "COREPACK_HOME": str(original_home / ".cache/node/corepack"),
        "COREPACK_ENABLE_DOWNLOAD_PROMPT": "0", "PNPM_HOME": str(original_home / ".local/share/pnpm"),
        "MIY_CONFIG_DIR": str(args.state / "miy"), "MIY_CREDENTIAL_BACKEND": "file",
        "HTTP_PROXY": PROXY_ADDRESS, "HTTPS_PROXY": PROXY_ADDRESS,
        "http_proxy": PROXY_ADDRESS, "https_proxy": PROXY_ADDRESS, "NO_PROXY": "", "no_proxy": "",
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_NAME": "Miy Acceptance",
        "GIT_AUTHOR_EMAIL": "acceptance@example.invalid", "GIT_COMMITTER_NAME": "Miy Acceptance",
        "GIT_COMMITTER_EMAIL": "acceptance@example.invalid", "GIT_TERMINAL_PROMPT": "0",
        "SSL_CERT_FILE": "/etc/ssl/certs/ca-certificates.crt",
    }
    for key, value in environment.items():
        command += ["--setenv", key, value]
    forbidden = [str(REPOSITORY / item) for item in (
        "Cargo.toml", "crates", "docs", ".git", ".local/server.json", ".local/bin/miy-server")]
    forbidden += ["/var/run/docker.sock", "/run/docker.sock", "/run/containerd/containerd.sock",
                  f"/run/user/{os.getuid()}/docker.sock",
                  str(original_home / ".ssh"), str(args.state / "private"),
                  str(args.state.parent / "private"), str(args.socket)]
    spec = {"hostNet": os.readlink("/proc/self/ns/net"),
            "hostPid": os.readlink("/proc/self/ns/pid"), "forbidden": forbidden,
            "bridge": BRIDGE, "proxy": PROXY_ADDRESS, "check": args.check, "agent": args.agent,
            "miyConfig": str(args.state / "miy"),
            "command": agent_command(args)}
    command += ["--chdir", str(args.app), "--", "/usr/bin/python3", "-c", SUPERVISOR, json.dumps(spec)]
    return command


def fingerprint(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(args):
    original_home = Path(pwd.getpwuid(os.getuid()).pw_dir)
    if not Path("/usr/bin/bwrap").is_file():
        raise SandboxError("bubblewrap_required")
    if not args.app.is_dir() or args.app.resolve() != args.app:
        raise SandboxError("app_requires_real_directory")
    if not (args.app / "manifest.json").is_file() or not (args.app / ".miy-tools/miy").is_file():
        raise SandboxError("generated_miy_app_required")
    if args.app == REPOSITORY or args.app in REPOSITORY.parents:
        raise SandboxError("app_would_expose_host_repository")
    if args.app == args.state or args.app in args.state.parents or args.state in args.app.parents:
        raise SandboxError("app_and_state_must_be_disjoint")
    if args.app in original_home.parents or args.app == original_home:
        raise SandboxError("app_would_expose_host_home")
    if not stat.S_ISSOCK(args.socket.lstat().st_mode):
        raise SandboxError("proxy_requires_unix_socket")
    if not (args.state / "miy").is_dir() or (args.state / "miy").is_symlink():
        raise SandboxError("prepared_miy_config_required")
    if args.agent == "claude" and not (CLAUDE_VENDOR / ".bin/claude").exists():
        raise SandboxError("claude_vendor_required")
    if args.timeout <= 0:
        raise SandboxError("positive_timeout_required")
    prompt = read_regular(args.prompt_file, maximum=1024 * 1024)
    if not prompt.strip() and not args.check:
        raise SandboxError("empty_prompt")
    prepare_profile(args, original_home)
    system_files = args.state / "private/system"
    account = pwd.getpwuid(os.getuid())
    files = {
        "passwd": f"{account.pw_name}:x:{os.getuid()}:{os.getgid()}:Acceptance:{original_home}:/bin/bash\n",
        "group": f"{account.pw_name}:x:{os.getgid()}:\n",
        "hosts": "127.0.0.1 localhost\n::1 localhost\n",
        "resolv.conf": "# Network access requires the scoped HTTP proxy.\n",
        "nsswitch.conf": "passwd: files\ngroup: files\nhosts: files dns\n",
    }
    for name, value in files.items():
        write_private(system_files / name, value.encode())
    run_id = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-" + uuid.uuid4().hex[:12]
    log_base = args.state / "private" / run_id
    out_path, err_path = Path(str(log_base) + ".stdout.jsonl"), Path(str(log_base) + ".stderr.log")
    write_private(out_path, b"")
    write_private(err_path, b"")
    started = time.monotonic()
    timed_out = False
    interrupted = False
    with out_path.open("wb") as output, err_path.open("wb") as errors:
        process = subprocess.Popen(sandbox_command(args, original_home, system_files),
                                   stdin=subprocess.PIPE, stdout=output, stderr=errors,
                                   env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"},
                                   start_new_session=True)
        try:
            process.communicate(input=prompt if not args.check else b"", timeout=args.timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            timed_out = isinstance(error, subprocess.TimeoutExpired)
            interrupted = not timed_out
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
    result = {"schemaVersion": 1, "runId": run_id, "check": args.check,
              "childExit": process.returncode, "timedOut": timed_out, "interrupted": interrupted,
              "durationMs": round((time.monotonic() - started) * 1000),
              "stdoutSha256": fingerprint(out_path), "stderrSha256": fingerprint(err_path)}
    write_private(Path(str(log_base) + ".status.json"), json_bytes(result))
    print(json.dumps(result), flush=True)
    if timed_out:
        return 124
    if interrupted:
        return 130
    return process.returncode if process.returncode >= 0 else 128 - process.returncode


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=absolute_path, required=True)
    parser.add_argument("--state", type=absolute_path, required=True)
    parser.add_argument("--socket", type=absolute_path, required=True)
    parser.add_argument("--agent", choices=("codex", "claude"), required=True)
    parser.add_argument("--prompt-file", type=absolute_path, required=True)
    parser.add_argument("--resume", type=uuid.UUID)
    parser.add_argument("--timeout", type=int, default=1200, help="wall-clock limit in seconds (default: 1200)")
    parser.add_argument("--check", action="store_true", help="probe isolation and proxy without an LLM call")
    args = parser.parse_args()
    try:
        return run(args)
    except (SandboxError, OSError, ValueError, subprocess.SubprocessError) as error:
        # Never print exception values from filesystem, JSON, auth or subprocess I/O.
        code = str(error) if isinstance(error, SandboxError) else type(error).__name__
        print(json.dumps({"schemaVersion": 1, "error": code}), file=sys.stderr)
        return 78


if __name__ == "__main__":
    raise SystemExit(main())
