#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${ROOT}/target/wasm32-wasip2/release/component_adaptive_card.wasm"

if [[ ! -f "${SRC}" ]]; then
  echo "Missing built wasm at ${SRC}" >&2
  exit 1
fi

mapfile -t DIST_MATCHES < <(find "${ROOT}/dist" -maxdepth 1 -type f -name 'component_adaptive_card__*.wasm' | sort)

if [[ "${#DIST_MATCHES[@]}" -ne 1 ]]; then
  echo "Expected exactly one dist wasm target, found ${#DIST_MATCHES[@]}." >&2
  printf '%s\n' "${DIST_MATCHES[@]}" >&2
  exit 1
fi

DEST="${DIST_MATCHES[0]}"
install -m 0644 "${SRC}" "${DEST}"
echo "Updated dist wasm: ${DEST}"
