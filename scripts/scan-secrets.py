#!/usr/bin/env python3
"""Scan reviewable files, excluding ignored runtime secrets and build output."""
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
files = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
).split(b"\0")
with tempfile.TemporaryDirectory(prefix="tomi-secret-scan-") as temporary:
    target = Path(temporary)
    for entry in files:
        if not entry:
            continue
        relative = Path(entry.decode())
        if relative.is_absolute() or ".." in relative.parts:
            raise SystemExit("Unsafe repository path")
        source = root / relative
        if source.is_file() and not source.is_symlink():
            destination = target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
    subprocess.run(["gitleaks", "dir", str(target), "--redact", "--no-banner"], check=True)
