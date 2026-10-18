#!/bin/sh
# Usage: scripts/scan-container.sh [source|history]
# Runs Gitleaks 8.30.1 in an immutable official container; never mounts .env files.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
scan_mode=${1:-source}
case "$scan_mode" in source|history) ;; *) printf '%s\n' 'Expected source or history' >&2; exit 2 ;; esac
scanner_image=ghcr.io/gitleaks/gitleaks@sha256:c00b6bd0aeb3071cbcb79009cb16a60dd9e0a7c60e2be9ab65d25e6bc8abbb7f
engine=${TOMI_CONTAINER_ENGINE:-}
if [ -z "$engine" ]; then
    if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then engine=docker
    elif command -v container >/dev/null 2>&1; then engine=container
    else printf '%s\n' 'Secret scan blocked: Docker or Apple Container is required.' >&2; exit 1
    fi
fi
case "$engine" in docker|container) ;; *) printf '%s\n' 'Unsupported container engine' >&2; exit 2 ;; esac
scratch=$(mktemp -d "${TMPDIR:-/tmp}/tomi-secret-scan.XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
if [ "$scan_mode" = history ]; then
    git -C "$repo_root" rev-parse --verify HEAD >/dev/null 2>&1 || { printf '%s\n' 'No committed history exists yet; source scan is still required.'; exit 0; }
    git clone --quiet --mirror --no-hardlinks "$repo_root" "$scratch/history"
    set -- git /scan/history --log-opts=--all
else
    python3 - "$repo_root" "$scratch/source" <<'PY_SOURCE'
from pathlib import Path
import shutil, subprocess, sys
root, target = map(Path, sys.argv[1:])
target.mkdir()
for name in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'], cwd=root).split(b'\0'):
    if not name: continue
    relative = Path(name.decode())
    if relative.is_absolute() or '..' in relative.parts: raise SystemExit('Unsafe repository path')
    source = root / relative
    if source.is_file() and not source.is_symlink():
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
PY_SOURCE
    set -- dir /scan/source
fi
if [ "$engine" = docker ]; then
    docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
        --volume "$scratch:/scan:ro" "$scanner_image" "$@" --redact --no-banner --ignore-gitleaks-allow
else
    container run --rm --read-only --cap-drop ALL --cpus 1 --memory 256M \
        --mount "type=bind,source=$scratch,target=/scan,readonly" "$scanner_image" "$@" --redact --no-banner --ignore-gitleaks-allow
fi
