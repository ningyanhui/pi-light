#!/usr/bin/env bash
# Download official cloudflared for linux-amd64, verify SHA-256, copy to DestDir.
# Exit non-zero on failure. Binary is not committed to git.
set -euo pipefail

DEST_DIR="${1:?usage: fetch-cloudflared.sh <dest-dir> [lock-file]}"
LOCK_FILE="${2:-}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "$LOCK_FILE" ]]; then
  LOCK_FILE="${SCRIPT_DIR}/cloudflared.lock.json"
fi

if [[ ! -f "$LOCK_FILE" ]]; then
  echo "lock file missing: $LOCK_FILE" >&2
  exit 1
fi

python3 - "$LOCK_FILE" "$DEST_DIR" <<'PY'
import hashlib, json, os, sys, urllib.request

lock_path, dest_dir = sys.argv[1], sys.argv[2]
with open(lock_path, encoding="utf-8") as f:
    lock = json.load(f)
art = lock["artifacts"]["linux-amd64"]
url = art["url"]
expected = art["sha256"].lower()
name = art.get("install_name") or "cloudflared"
os.makedirs(dest_dir, exist_ok=True)
tmp = os.path.join(dest_dir, name + ".download")
dest = os.path.join(dest_dir, name)
print(f"Downloading cloudflared {lock['version']} from {url} ...")
urllib.request.urlretrieve(url, tmp)
h = hashlib.sha256()
with open(tmp, "rb") as f:
    for chunk in iter(lambda: f.read(1024 * 1024), b""):
        h.update(chunk)
got = h.hexdigest()
if got != expected:
    os.remove(tmp)
    raise SystemExit(f"SHA-256 mismatch: expected {expected} got {got}")
os.replace(tmp, dest)
os.chmod(dest, 0o755)
print(f"cloudflared ready: {dest}")
PY
