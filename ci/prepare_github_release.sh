#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-${ROOT_DIR}/release}"

cd "${ROOT_DIR}"

VERSION="$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name=="component-adaptive-card") | .version')"
COMPONENT_NAME="$(jq -r '.name' component.manifest.json)"
WASM_PATH="$(jq -r '.artifacts.component_wasm' component.manifest.json)"
WASM_BASENAME="$(basename "${WASM_PATH}")"
PACKAGE_DIR="${OUT_DIR}/${COMPONENT_NAME}-${VERSION}"
RELEASE_WASM_PATH="${OUT_DIR}/${WASM_BASENAME}"
RELEASE_MANIFEST_PATH="${OUT_DIR}/component.manifest.json"
RELEASE_ARCHIVE_PATH="${OUT_DIR}/${COMPONENT_NAME}-${VERSION}.tar.gz"

rm -rf "${OUT_DIR}"
mkdir -p "${PACKAGE_DIR}/e2e" "${OUT_DIR}/e2e"

cp "${WASM_PATH}" "${RELEASE_WASM_PATH}"
cp component.manifest.json "${RELEASE_MANIFEST_PATH}"
cp e2e/answers.json "${OUT_DIR}/e2e/answers.json"
cp e2e/adaptive-card.json "${OUT_DIR}/e2e/adaptive-card.json"

jq --arg wasm "${WASM_BASENAME}" '.artifacts.component_wasm = $wasm' \
  "${RELEASE_MANIFEST_PATH}" > "${OUT_DIR}/component.manifest.tmp"
mv "${OUT_DIR}/component.manifest.tmp" "${RELEASE_MANIFEST_PATH}"

WASM_BASENAME="${WASM_BASENAME}" RELEASE_MANIFEST_PATH="${RELEASE_MANIFEST_PATH}" python - <<'PY'
import json
import os
from pathlib import Path

import blake3

manifest_path = Path(os.environ["RELEASE_MANIFEST_PATH"])
wasm_path = manifest_path.parent / os.environ["WASM_BASENAME"]
data = json.loads(manifest_path.read_text())
data.setdefault("hashes", {})
data["hashes"]["component_wasm"] = f"blake3:{blake3.blake3(wasm_path.read_bytes()).hexdigest()}"
manifest_path.write_text(json.dumps(data, indent=2) + "\n")
PY

cp "${RELEASE_WASM_PATH}" "${PACKAGE_DIR}/${WASM_BASENAME}"
cp "${RELEASE_MANIFEST_PATH}" "${PACKAGE_DIR}/component.manifest.json"
cp "${OUT_DIR}/e2e/answers.json" "${PACKAGE_DIR}/e2e/answers.json"
cp "${OUT_DIR}/e2e/adaptive-card.json" "${PACKAGE_DIR}/e2e/adaptive-card.json"

tar -C "${OUT_DIR}" -czf "${RELEASE_ARCHIVE_PATH}" "$(basename "${PACKAGE_DIR}")"

cat > "${OUT_DIR}/notes.md" <<EOF
Release bundle for ${COMPONENT_NAME} ${VERSION}.

Included assets:
- ${WASM_BASENAME}
- component.manifest.json
- e2e/answers.json
- e2e/adaptive-card.json
EOF

cat > "${OUT_DIR}/release.env" <<EOF
VERSION=${VERSION}
VERSION_TAG=${VERSION}
LATEST_TAG=latest
COMPONENT_NAME=${COMPONENT_NAME}
RELEASE_WASM_PATH=${RELEASE_WASM_PATH}
RELEASE_MANIFEST_PATH=${RELEASE_MANIFEST_PATH}
RELEASE_ARCHIVE_PATH=${RELEASE_ARCHIVE_PATH}
RELEASE_NOTES_PATH=${OUT_DIR}/notes.md
RELEASE_ANSWERS_PATH=${OUT_DIR}/e2e/answers.json
RELEASE_CARD_PATH=${OUT_DIR}/e2e/adaptive-card.json
EOF
