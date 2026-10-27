#!/usr/bin/env python3
"""Check offline server bootstrap against a native CLI and the Unix installer.

Every configuration and installed file lives in a private temporary directory.
No deployed server, network request, real login or user credential store is used.
Windows installer transactions are checked separately by check-windows-installer.ps1.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid


ROOT = Path(__file__).resolve().parents[1]
FIRST = "https://first.example"
SECOND = "https://second.example:8443"
SECRET = b"private-bootstrap-fixture-do-not-print"


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def check(cli):
    passed = []
    env = dict(os.environ)
    env.pop("MIY_CONTEXT", None)
    env["MIY_CREDENTIAL_BACKEND"] = "file"
    env["RUST_LOG"] = "off"

    def ok(name):
        passed.append(name)
        print(f"PASS {name}", flush=True)

    with tempfile.TemporaryDirectory(prefix="miy-cli-install-") as temporary:
        # Canonicalize macOS's system /var alias while keeping config symlink
        # rejection enabled for user-owned paths.
        root = Path(temporary).resolve()

        def configuration(name):
            directory = root / name
            env["MIY_CONFIG_DIR"] = str(directory)
            return directory / "config.json"

        def invoke(args, *, expected=0, cwd=root, binary=cli):
            result = subprocess.run(
                [str(binary), "--output", "json", *args],
                cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=30, check=False,
            )
            require(SECRET not in result.stdout + result.stderr, "CLI printed a fixture credential")
            require(result.returncode == expected,
                    f"Unexpected CLI exit {result.returncode}, expected {expected}: {args[:2]}")
            require(not result.stderr.strip(), "Offline CLI command produced unexpected diagnostics")
            response = json.loads(result.stdout.decode("utf-8"))
            require(response["schemaVersion"] == 1, "Unexpected output schema")
            require(response["ok"] is (expected == 0), "Result and exit code disagree")
            return response

        def init(url, **kwargs):
            response = invoke(["context", "init", "--url", url], **kwargs)
            require(response["command"] == "context init", "Wrong initialization command identity")
            return response

        def read(path):
            return json.loads(path.read_bytes())

        def write_config(path, config):
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            # Keep a noncanonical but valid layout: preservation means bytes,
            # not merely equivalent parsed JSON or a regenerated client ID.
            path.write_bytes((json.dumps(config, indent=3) + "\n\n").encode("utf-8"))

        unconfigured = configuration("missing-address")
        response = invoke(["--no-input", "context", "init"], expected=2)
        require(response["error"]["code"] == "server_url_required", "Missing address did not require setup")
        require(not unconfigured.exists(), "Missing address invented a server configuration")
        response = invoke(["--no-input", "auth", "login"], expected=7)
        require(response["error"]["code"] == "context_required", "Login guessed an unconfigured server")
        require(not unconfigured.exists(), "Unconfigured login wrote a server configuration")
        ok("noninteractive setup and login require an explicit server without guessing")

        path = configuration("fresh")
        response = init("https://FIRST.EXAMPLE:443/")
        require(response["data"]["status"] == "initialized", "Fresh install was not initialized")
        config = read(path)
        require(config["active"] == "default", "Fresh context is not active")
        require(config["contexts"] == {
            "default": {"origin": FIRST, "caFile": None, "userId": None},
        }, "Fresh context does not match the normalized supplied server")
        uuid.UUID(config["clientId"])
        selected = invoke(["context", "show"])["data"]
        require(selected["name"] == "default" and selected["context"]["origin"] == FIRST,
                "A fresh CLI does not select the installed server")
        if os.name != "nt":
            require(path.parent.stat().st_mode & 0o077 == 0, "Config directory is not private")
            require(path.stat().st_mode & 0o077 == 0, "Config file is not private")
        ok("fresh install selects the supplied normalized server")

        before = path.read_bytes()
        for url in [FIRST, SECOND]:
            preserved = init(url)["data"]
            require(preserved["status"] == "preserved", "Reinstall did not preserve configuration")
            require(preserved["activeOrigin"] == FIRST and preserved["requestedOrigin"] == url,
                    "Reinstall did not show configured and requested targets")
            require(preserved["serverMismatch"] is (url != FIRST), "Different server was silently ignored")
            require(path.read_bytes() == before, "Reinstall rewrote existing configuration")
        require(invoke(["--no-input", "context", "init"])["data"]["activeOrigin"] == FIRST,
                "Reinstall without an address did not show the saved target")
        ok("repeated or different-server initialization preserves existing bytes")

        custom = {
            "version": 1, "clientId": str(uuid.uuid4()), "active": "work",
            "contexts": {
                "default": {"origin": FIRST, "caFile": None, "userId": None},
                "work": {"origin": SECOND, "caFile": str(root / "private-root.crt"),
                         "userId": "fixture-user"},
            },
        }
        write_config(path, custom)
        credential = path.parent / "credential-sentinel.bin"
        credential.write_bytes(SECRET)
        before = path.read_bytes()
        response = init("https://third.example")
        require(response["data"]["status"] == "preserved" and response["data"]["active"] == "work",
                "Existing selected context was replaced")
        require(path.read_bytes() == before and credential.read_bytes() == SECRET,
                "Initialization altered existing context, identity or credentials")
        require(response["data"]["next"][0]["args"] == ["context", "add", "work-2", "--url", "https://third.example"],
                "Different server recovery reused an existing context name or omitted its URL")
        ok("custom contexts, active selection, CA, user identity and credentials survive")

        empty = configuration("intentionally-empty")
        write_config(empty, {"version": 1, "clientId": str(uuid.uuid4()), "active": None, "contexts": {}})
        before = empty.read_bytes()
        empty_result = init(FIRST)["data"]
        require(empty_result["status"] == "preserved" and empty_result["setupRequired"],
                "Empty saved config was treated as a new user or a configured server")
        require(empty_result["next"][0]["args"] == ["context", "add", "work", "--url", FIRST],
                "Empty saved config did not provide explicit setup instructions")
        require(empty.read_bytes() == before, "An intentionally empty config was repopulated")
        ok("an intentionally empty existing configuration stays empty")

        project = root / "project"
        (project / ".miy").mkdir(parents=True)
        (project / ".miy/context.json").write_text('{"context":"project-work"}', encoding="utf-8")
        isolated = configuration("explicit-selection")
        env["MIY_CONTEXT"] = "environment-work"
        try:
            invoke(["--context", "flag-work", "context", "init", "--url", SECOND], cwd=project)
            require(read(isolated)["active"] == "default", "Bootstrap used a caller's context override")
            require(read(isolated)["contexts"]["default"]["origin"] == SECOND,
                    "Bootstrap used the project or environment server")
            require((project / ".miy/context.json").read_text(encoding="utf-8") == '{"context":"project-work"}',
                    "Bootstrap changed the project binding")
        finally:
            env.pop("MIY_CONTEXT", None)
        ok("bootstrap is independent of context flags, environment and project bindings")

        invalid = configuration("invalid-url")
        for url in ["https://name:password@example.invalid",
                    "https://first.example/path", "https://first.example?x=1", "not-a-url"]:
            init(url, expected=2)
            require(not invalid.exists(), "Invalid server URL created configuration")
        ok("invalid or credential-bearing server URLs are rejected without configuration")

        http = configuration("internal-http")
        init("http://192.168.5.10:8080")
        require(read(http)["contexts"]["default"]["origin"] == "http://192.168.5.10:8080",
                "Explicit HTTP IP was not registered")
        ok("explicit certificate-free HTTP IP is retained without URL inference")

        damaged = configuration("damaged")
        damaged.parent.mkdir(mode=0o700)
        for data in [b"{not-json", json.dumps({**custom, "version": 999}).encode("utf-8")]:
            damaged.write_bytes(data)
            init(FIRST, expected=7)
            require(damaged.read_bytes() == data, "Invalid existing configuration was overwritten")
        ok("malformed and unsupported existing configurations fail without overwrite")

        # Exercise the actual installer, including argument handling and its
        # propagation of bootstrap failure after the binary has been installed.
        if os.name != "nt":
            prefix = root / "installed cli 한글"
            installer = ROOT / "scripts/install-cli.sh"

            def install(*arguments, expected=0):
                result = subprocess.run(
                    ["bash", str(installer), "--from", str(cli), "--prefix", str(prefix), *arguments],
                    cwd=root, env=env, stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    timeout=60, check=False,
                )
                require(result.returncode == expected,
                        f"Unexpected installer exit {result.returncode}, expected {expected}")
                require(SECRET not in result.stdout + result.stderr, "Installer printed a fixture credential")
                return result

            unconfigured = configuration("generic-install")
            missing = install("--no-input")
            installed = prefix / "miy"
            expected_hash = digest(cli)
            require(digest(installed) == expected_hash, "Installer changed the executable")
            require(not unconfigured.exists(), "Generic installation invented a default server")
            require(b'"status":"required"' in missing.stdout and b"miy context init" in missing.stdout,
                    "Generic installation omitted pending server registration")
            ok("Unix installation without --server leaves configuration uninitialized")

            installed_config = configuration("server-install")
            install("--server", FIRST)
            require(read(installed_config)["contexts"]["default"]["origin"] == FIRST,
                    "Unix installer did not register its --server value")
            invoke(["context", "show"], binary=installed)
            ok("Unix installer registers a default connection usable by the installed CLI")

            write_config(installed_config, custom)
            secret_path = installed_config.parent / "credential-sentinel.bin"
            secret_path.write_bytes(SECRET)
            before = installed_config.read_bytes()
            install("--server", "https://third.example")
            require(installed_config.read_bytes() == before and secret_path.read_bytes() == SECRET,
                    "Unix update changed existing configuration or credentials")
            require(digest(installed) == expected_hash, "Unix update changed the executable bytes")
            ok("Unix update preserves existing selected server, identity and credentials")

            installed_config.write_bytes(b"invalid-existing-config")
            failed = install("--server", FIRST, expected=7)
            require(installed_config.read_bytes() == b"invalid-existing-config",
                    "Failed Unix bootstrap overwrote the invalid config")
            require(digest(installed) == expected_hash, "Bootstrap failure lost the installed binary")
            require(b"installed" in failed.stderr.lower(), "Partial install failure does not report binary state")
            ok("Unix bootstrap failure is nonzero and reports the installed binary state")

            # Exercise real TTY input. Pipes are deliberately not treated as
            # interactive, and no additional Python package is required.
            import pty
            import select

            def terminal(arguments, replies, expected):
                master, slave = pty.openpty()
                process = subprocess.Popen(arguments, cwd=root, env=env, stdin=slave,
                                           stdout=slave, stderr=slave, start_new_session=True)
                os.close(slave)
                transcript = bytearray()
                sent = 0
                deadline = time.monotonic() + 30
                try:
                    while True:
                        require(time.monotonic() < deadline, "Terminal setup did not finish")
                        if select.select([master], [], [], 0.1)[0]:
                            try:
                                chunk = os.read(master, 65536)
                            except OSError:
                                break
                            if not chunk:
                                break
                            transcript.extend(chunk)
                            while sent < len(replies) and transcript.count(b"Target server URL") > sent:
                                os.write(master, replies[sent].encode("utf-8") + b"\n")
                                sent += 1
                        if process.poll() is not None and not select.select([master], [], [], 0)[0]:
                            break
                    require(process.wait(timeout=5) == expected, "Unexpected terminal setup exit")
                    require(sent == len(replies), "Expected server prompt was not shown")
                    require(SECRET not in transcript, "Terminal setup printed a credential")
                    return bytes(transcript)
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.wait()
                    os.close(master)

            prompted = configuration("terminal-init")
            terminal([str(cli), "context", "init"], ["not-a-url", "http://192.168.6.10:8080"], 0)
            require(read(prompted)["contexts"]["default"]["origin"] == "http://192.168.6.10:8080",
                    "Interactive registration did not retain the entered URL")
            ok("terminal registration requests a URL and retries invalid input")

            cancelled = configuration("terminal-cancel")
            terminal([str(cli), "context", "init"], [""], 130)
            require(not cancelled.exists(), "Cancelled registration invented a target")
            terminal([str(cli), "auth", "login"], [""], 130)
            require(not cancelled.exists(), "Cancelled first-login setup invented a target")
            terminal([str(cli), "--no-input", "context", "init"], [], 2)
            require(not cancelled.exists(), "Explicit no-input registered a target")
            ok("cancelled setup and first login preserve no-server state; no-input never prompts")

            interactive_install = configuration("terminal-installer")
            terminal(["bash", str(installer), "--from", str(cli), "--prefix", str(prefix)], [FIRST], 0)
            require(read(interactive_install)["contexts"]["default"]["origin"] == FIRST,
                    "Interactive installer did not register the entered target")
            ok("Unix installer requests the server when omitted in a terminal")

            deferred = configuration("terminal-installer-cancel")
            cancelled_output = terminal(["bash", str(installer), "--from", str(cli), "--prefix", str(prefix)], [""], 0)
            require(not deferred.exists() and digest(installed) == expected_hash,
                    "Deferred setup changed configuration or lost the installed CLI")
            require(b'"status":"required"' in cancelled_output, "Deferred installer omitted setup status")
            ok("cancelled installer registration retains the installed CLI and requires later setup")

    print(json.dumps({"status": "pass", "checks": len(passed), "platform": os.name}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    args = parser.parse_args()
    binary = args.cli.resolve(strict=True)
    check(binary)
