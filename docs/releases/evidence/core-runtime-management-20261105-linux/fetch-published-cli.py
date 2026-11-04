"""Download the published Linux CLI into a new private directory and verify its provenance."""
import hashlib, json, os, pathlib, sys, tarfile, urllib.request
origin = 'https://v2.dwdcc.kr'
out = pathlib.Path(sys.argv[1])
out.mkdir(mode=0o700)  # must be new
guide = json.load(urllib.request.urlopen(origin + '/api/v1/cli/install', timeout=20))
target = 'x86_64-unknown-linux-gnu'
item = next(d for d in guide['downloads'] if d['file'] == f"miy-{guide['cliVersion']}-{target}.tar.gz")
url = f"{origin}/cli/downloads/{item['commit']}/{item['file']}"
archive_dir = out / 'archive'; archive_dir.mkdir(mode=0o700)
archive = archive_dir / item['file']
data = urllib.request.urlopen(url, timeout=60).read()
archive.write_bytes(data); os.chmod(archive, 0o600)
checksum = urllib.request.urlopen(origin + item['checksumUrl'], timeout=20).read().decode().split()[0]
archive_sha = hashlib.sha256(data).hexdigest()
assert archive_sha == item['sha256'] == checksum, 'archive_checksum_mismatch'
extract = out / 'extract'; extract.mkdir(mode=0o700)
with tarfile.open(archive) as tar:
    members = tar.getmembers()
    for m in members:
        p = pathlib.PurePosixPath(m.name)
        assert not p.is_absolute() and '..' not in p.parts and (m.isfile() or m.isdir()), 'unsafe_archive_member'
    tar.extractall(extract, filter='data')
binaries = [p for p in extract.rglob('miy') if p.is_file()]
assert len(binaries) == 1, 'cli_binary_not_unique'
binary = binaries[0]
binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
assert binary_sha == item['binarySha256'], 'binary_checksum_mismatch'
os.chmod(binary, 0o700)
record = {'status': 'PASS', 'sourceCommit': item['commit'], 'cliVersion': guide['cliVersion'], 'target': target,
          'archiveSha256': archive_sha, 'binarySha256': binary_sha, 'downloadBytes': len(data), 'binaryPath': str(binary)}
(out / 'published-cli-provenance.json').write_text(json.dumps(record, indent=2) + '\n')
os.chmod(out / 'published-cli-provenance.json', 0o600)
print(json.dumps(record))
