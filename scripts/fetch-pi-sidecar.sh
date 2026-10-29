#!/usr/bin/env bash
# Download official Pi v1.4.0 Windows zip, verify SHA-256, extract to DEST_DIR.
# Exit non-zero on download, checksum, or layout failure.
set -euo pipefail

PI_URL="${PI_LIGHT_PI_ZIP_URL:-https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip}"
PI_SHA256="${PI_LIGHT_PI_ZIP_SHA256:-0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b}"

usage() {
  echo "usage: $(basename "$0") <dest-dir>" >&2
  echo "  Extracts pi.exe and bundled Pi assets into <dest-dir> (e.g. sidecar/pi)." >&2
  exit 1
}

[[ $# -eq 1 ]] || usage
DEST_DIR="$1"

if [[ ! -d "${DEST_DIR}" ]]; then
  mkdir -p "${DEST_DIR}"
fi

TMP_ZIP="$(mktemp)"
trap 'rm -f "${TMP_ZIP}"' EXIT

echo "Downloading Pi sidecar from ${PI_URL} ..."
curl -fsSL -o "${TMP_ZIP}" "${PI_URL}"

if command -v sha256sum >/dev/null 2>&1; then
  echo "${PI_SHA256}  ${TMP_ZIP}" | sha256sum -c -
elif command -v shasum >/dev/null 2>&1; then
  echo "${PI_SHA256}  ${TMP_ZIP}" | shasum -a 256 -c -
else
  echo "error: need sha256sum or shasum on PATH" >&2
  exit 1
fi

echo "Extracting into ${DEST_DIR} ..."
unzip -o -q "${TMP_ZIP}" -d "${DEST_DIR}"

if [[ ! -f "${DEST_DIR}/pi.exe" ]]; then
  echo "error: pi.exe not found under ${DEST_DIR} after extract" >&2
  exit 1
fi

echo "Pi sidecar ready: ${DEST_DIR}/pi.exe"
