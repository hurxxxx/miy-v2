#!/usr/bin/env python3
"""Read-only verification of all CLI/helper downloads from one explicitly selected server."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import ssl
import urllib.error
import urllib.parse
import urllib.request

TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
           "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-pc-windows-msvc")
PLATFORMS = ("linux/amd64", "linux/arm64")
MAX_ARCHIVE = 512 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def valid_hash(value, length=64):
    return isinstance(value, str) and re.fullmatch(r"[a-f0-9]{%d}" % length, value) is not None


def origin_url(value):
    parsed = urllib.parse.urlsplit(value)
    require(parsed.scheme in ("http", "https") and parsed.hostname
            and parsed.username is None and parsed.password is None
            and parsed.path in ("", "/") and not parsed.query and not parsed.fragment,
            "supply an explicit HTTP(S) server origin without credentials or a path")
    host = parsed.hostname.lower()
    if ":" in host:
        host = f"[{host}]"
    port = parsed.port
    authority = host if port is None or (parsed.scheme, port) in (("http", 80), ("https", 443)) else f"{host}:{port}"
    return f"{parsed.scheme}://{authority}"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, url):
        raise ValueError("download endpoint redirected; immutable files must be served by the selected origin")


def catalog(guide, origin, expected_commit):
    version = guide.get("cliVersion", "")
    require(guide.get("schemaVersion") == 1 and guide.get("serverUrl") == origin
            and guide.get("sourceCommit") == expected_commit and valid_hash(expected_commit, 40)
            and re.fullmatch(r"\d+\.\d+\.\d+", version), "installation guide identity differs")
    downloads, helpers = guide.get("downloads"), guide.get("tunnelImages")
    require(isinstance(downloads, list) and len(downloads) == len(TARGETS)
            and isinstance(helpers, list) and len(helpers) == len(PLATFORMS), "installation guide is incomplete")
    seen = set()
    for row in downloads:
        target = row.get("target")
        require(target in TARGETS and target not in seen, "invalid or duplicate CLI target")
        seen.add(target)
        extension = "zip" if target == "x86_64-pc-windows-msvc" else "tar.gz"
        require(row.get("file") == f"miy-{version}-{target}.{extension}"
                and valid_hash(row.get("binarySha256")), "CLI asset provenance differs")
    seen.clear()
    for row in helpers:
        platform = row.get("platform")
        require(platform in PLATFORMS and platform not in seen, "invalid or duplicate helper platform")
        seen.add(platform)
        arch = platform.split("/")[1]
        require(row.get("development") is False
                and row.get("imageTag") == f"miy-db-tunnel:{expected_commit}-{arch}"
                and row.get("file") == f"miy-db-tunnel-{version}-{arch}.oci.tar"
                and valid_hash(row.get("configSha256")), "helper provenance differs")
    for row in downloads + helpers:
        path = f"/cli/downloads/{expected_commit}/{row['file']}"
        require(row.get("schemaVersion") == 1 and row.get("commit") == expected_commit
                and row.get("version") == version and valid_hash(row.get("sha256"))
                and row.get("url") == path and row.get("checksumUrl") == path + ".sha256",
                "download row or immutable URL differs from the candidate")
    return downloads + helpers


def check(origin, expected_commit, ca_file=None):
    origin = origin_url(origin)
    context = ssl.create_default_context()
    if ca_file is not None:
        context.load_verify_locations(cafile=str(ca_file))
    opener = urllib.request.build_opener(NoRedirect(), urllib.request.HTTPSHandler(context=context))

    def request(path):
        return opener.open(urllib.request.Request(origin + path, headers={"Accept-Encoding": "identity"}), timeout=300)

    def read_small(path, limit):
        with request(path) as response:
            data = response.read(limit + 1)
            require(len(data) <= limit, "download metadata is too large")
            return data

    guide = json.loads(read_small("/api/v1/cli/install", 1024 * 1024))
    rows = catalog(guide, origin, expected_commit)
    total = 0
    for row in rows:
        checksum = read_small(row["checksumUrl"], 1024).decode("ascii").strip()
        require(checksum == f"{row['sha256']}  {row['file']}", "served checksum file differs from catalog")
        digest = hashlib.sha256()
        size = 0
        with request(row["url"]) as response:
            while chunk := response.read(128 * 1024):
                size += len(chunk)
                require(size <= MAX_ARCHIVE, "download archive exceeds the size limit")
                digest.update(chunk)
        require(size > 0 and digest.hexdigest() == row["sha256"], "served archive checksum differs from catalog")
        total += size
    return {"verified": True, "serverUrl": origin, "commit": expected_commit,
            "version": guide["cliVersion"], "downloads": len(rows), "bytes": total}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", required=True)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--ca-file", type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.origin, args.expected_commit, args.ca_file)))
