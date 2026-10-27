#!/usr/bin/env python3
"""Check immutable staging and public download verification with isolated release fixtures."""
import contextlib
import copy
import functools
import hashlib
import http.server
import importlib.util
import io
import json
from pathlib import Path
import struct
import tarfile
import tempfile
import threading
import unittest
import zipfile


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


release = module("release", "package-cli-release.py")
staging = module("staging", "stage-cli-release.py")
downloads = module("downloads", "check-cli-downloads.py")
tunnel = module("tunnel", "db_tunnel_package.py")
VERSION = "0.6.0"
COMMIT = "a" * 40


def tar(path, contents, mode="w"):
    with tarfile.open(path, mode) as writer:
        for name, data in contents.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            entry.mode = 0o755 if name in ("miy", "install-cli.sh") else 0o644
            writer.addfile(entry, io.BytesIO(data))


def fixture(root, commit=COMMIT):
    root.mkdir()
    for target, (binary_name, extension, kind, machine) in release.TARGETS.items():
        binary = bytearray(80)
        if kind == "elf":
            binary[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<H", binary, 18, machine)
        elif kind == "mach":
            binary[:4] = b"\xcf\xfa\xed\xfe"
            struct.pack_into("<I", binary, 4, machine)
        else:
            binary[:2] = b"MZ"
            struct.pack_into("<I", binary, 60, 64)
            binary[64:68] = b"PE\0\0"
            struct.pack_into("<H", binary, 68, machine)
        metadata = {"schemaVersion": 1, "version": VERSION, "commit": commit,
                    "target": target, "binarySha256": hashlib.sha256(binary).hexdigest()}
        files = {binary_name: bytes(binary), "release.json": release.metadata_bytes(metadata)}
        files.update({name: b"fixture installer" for name in
                      (release.WINDOWS_SCRIPTS if extension == "zip" else ("install-cli.sh",))})
        filename = f"miy-{VERSION}-{target}.{extension}"
        path = root / filename
        if extension == "zip":
            with zipfile.ZipFile(path, "w") as writer:
                for name, data in files.items():
                    writer.writestr(name, data)
        else:
            tar(path, files, "w:gz")
        (root / (filename + ".sha256")).write_text(f"{staging.digest(path)}  {filename}\n")
        (root / f"provenance-{target}.json").write_bytes(release.metadata_bytes(metadata))
    for arch in ("amd64", "arm64"):
        tag = f"miy-db-tunnel:{commit}-{arch}"
        config = json.dumps({"os": "linux", "architecture": arch, "config": {"Labels": {
            "org.miy.component": "db-tunnel", "org.opencontainers.image.revision": commit}}}).encode()
        config_sha256 = hashlib.sha256(config).hexdigest()
        name = f"blobs/sha256/{config_sha256}" if arch == "amd64" else f"{config_sha256}.json"
        filename = f"miy-db-tunnel-{VERSION}-{arch}.oci.tar"
        path = root / filename
        tar(path, {"manifest.json": json.dumps([{"Config": name, "RepoTags": [tag], "Layers": []}]).encode(), name: config})
        row = {"schemaVersion": 1, "platform": f"linux/{arch}", "version": VERSION, "commit": commit,
               "imageTag": tag, "imageId": "sha256:" + "d" * 64, "configSha256": config_sha256,
               "file": filename, "sha256": staging.digest(path), "development": False}
        (root / f"tunnel-{arch}.json").write_bytes(release.metadata_bytes(row))
        (root / (filename + ".sha256")).write_text(f"{row['sha256']}  {filename}\n")
    return release.verify(root)


@contextlib.contextmanager
def serve(root, index):
    state = {"guide": None, "redirect": False}

    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            if self.path == "/api/v1/cli/install":
                data = json.dumps(state["guide"]).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            elif state["redirect"]:
                self.send_response(302)
                self.send_header("Location", "http://untrusted.invalid/download")
                self.end_headers()
            else:
                super().do_GET()

    handler = functools.partial(Handler, directory=root)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    origin = f"http://127.0.0.1:{server.server_port}"
    guide = {"schemaVersion": 1, "serverUrl": origin, "cliVersion": index["version"],
             "sourceCommit": index["commit"], "downloads": copy.deepcopy(index["assets"]),
             "tunnelImages": copy.deepcopy(index["tunnelImages"])}
    for row in guide["downloads"] + guide["tunnelImages"]:
        row["url"] = f"/cli/downloads/{index['commit']}/{row['file']}"
        row["checksumUrl"] = row["url"] + ".sha256"
    state["guide"] = guide
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield origin, state
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


class Downloads(unittest.TestCase):
    def test_staging_and_all_public_downloads_are_verified(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            index = fixture(root / "aggregate")
            destination = root / "public/cli/downloads"
            first = staging.stage(root / "aggregate", destination, COMMIT)
            self.assertEqual(first["targets"], 5)
            self.assertEqual(first["helpers"], 2)
            self.assertEqual(staging.stage(root / "aggregate", destination, COMMIT), first)
            self.assertEqual(json.loads((destination / "release-index.json").read_bytes()), index)
            with serve(root / "public", index) as (origin, state):
                self.assertEqual(downloads.check(origin, COMMIT)["downloads"], 7)
                state["redirect"] = True
                with self.assertRaisesRegex(ValueError, "redirected"):
                    downloads.check(origin, COMMIT)
                state["redirect"] = False
                (destination / COMMIT / index["assets"][0]["file"]).write_bytes(b"corrupted")
                with self.assertRaisesRegex(ValueError, "checksum differs"):
                    downloads.check(origin, COMMIT)
            with self.assertRaisesRegex(ValueError, "cannot be overwritten"):
                staging.stage(root / "aggregate", destination, COMMIT)

    def test_failed_candidate_preserves_the_published_pointer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture(root / "first")
            destination = root / "downloads"
            staging.stage(root / "first", destination, COMMIT)
            before = (destination / "release-index.json").read_bytes()
            commit = "b" * 40
            fixture(root / "second", commit)
            row = root / "second/tunnel-arm64.json"
            data = json.loads(row.read_bytes())
            data["development"] = True
            row.write_bytes(release.metadata_bytes(data))
            with self.assertRaisesRegex(ValueError, "helper provenance differs"):
                staging.stage(root / "second", destination, commit)
            self.assertEqual((destination / "release-index.json").read_bytes(), before)
            self.assertFalse((destination / commit).exists())
            self.assertFalse(list(root.glob(".miy-release-*")))

    def test_catalog_and_helper_archive_reject_foreign_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            index = fixture(root / "aggregate")
            destination = root / "public/cli/downloads"
            staging.stage(root / "aggregate", destination, COMMIT)
            with serve(root / "public", index) as (origin, state):
                valid = state["guide"]
                for field, value in (("commit", "b" * 40), ("schemaVersion", 2),
                                     ("url", "https://untrusted.invalid/archive")):
                    guide = copy.deepcopy(valid)
                    guide["downloads"][0][field] = value
                    with self.assertRaises(ValueError):
                        downloads.catalog(guide, origin, COMMIT)
                guide = copy.deepcopy(valid)
                guide["tunnelImages"][0]["development"] = True
                with self.assertRaises(ValueError):
                    downloads.catalog(guide, origin, COMMIT)
                with self.assertRaises(ValueError):
                    downloads.catalog(valid, "https://untrusted.invalid", COMMIT)
            for arch in ("amd64", "arm64"):
                row = index["tunnelImages"][0 if arch == "amd64" else 1]
                path = root / "aggregate" / row["file"]
                self.assertEqual(tunnel.archive_identity(path, row["platform"], COMMIT, row["imageTag"]), row["configSha256"])
                with self.assertRaisesRegex(ValueError, "provenance differs"):
                    tunnel.archive_identity(path, row["platform"], "b" * 40, row["imageTag"])
                with self.assertRaisesRegex(ValueError, "tag differs"):
                    tunnel.archive_identity(path, row["platform"], COMMIT, "miy-db-tunnel:latest")


if __name__ == "__main__":
    unittest.main()
