#!/usr/bin/env python3
"""Check relative Markdown links and heading anchors in tracked documents.

External URLs are not fetched. Generated project guidance (templates/, crates/miy/guidance/)
is skipped because its links resolve inside a generated application, not this repository.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKIP_PREFIXES = ("templates/", "crates/miy/guidance/")
LINK = re.compile(r"(?<!!)\[(?:[^\]\\]|\\.)*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
FENCE = re.compile(r"^(```|~~~)")
INLINE_CODE = re.compile(r"`[^`]*`")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")


def slug(text: str) -> str:
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[^\w\- ]", "", text.strip().lower())
    return text.replace(" ", "-")


def prose_lines(path: Path):
    fenced = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if FENCE.match(line.lstrip()):
            fenced = not fenced
            continue
        if not fenced:
            yield line


def anchors(path: Path, cache: dict) -> set:
    if path not in cache:
        seen: dict = {}
        result = set()
        for line in prose_lines(path):
            match = HEADING.match(line)
            if not match:
                continue
            base = slug(match.group(2))
            count = seen.get(base, 0)
            seen[base] = count + 1
            result.add(base if count == 0 else f"{base}-{count}")
        cache[path] = result
    return cache[path]


def main() -> int:
    tracked = subprocess.run(["git", "ls-files", "*.md"], cwd=ROOT, check=True,
                             capture_output=True, text=True).stdout.split()
    cache: dict = {}
    failures = []
    checked = 0
    for name in tracked:
        if name.startswith(SKIP_PREFIXES):
            continue
        source = ROOT / name
        for number, line in enumerate(prose_lines(source), 1):
            for target in LINK.findall(INLINE_CODE.sub("", line)):
                if re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I):
                    continue
                path_part, _, fragment = target.partition("#")
                resolved = (source.parent / path_part).resolve() if path_part else source
                checked += 1
                if ROOT not in resolved.parents and resolved != ROOT:
                    failures.append(f"{name}: {target} leaves the repository")
                elif not resolved.exists():
                    failures.append(f"{name}: {target} does not exist")
                elif fragment and resolved.suffix == ".md" and fragment not in anchors(resolved, cache):
                    failures.append(f"{name}: {target} has no matching heading")
    for failure in failures:
        print(failure)
    print(f"{'FAIL' if failures else 'PASS'} {checked} relative links in {len(tracked)} tracked Markdown files")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
