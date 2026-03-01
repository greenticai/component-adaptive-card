#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "wasm-tools not found; skipping host:state capability check."
  exit 0
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found; skipping host:state capability check."
  exit 0
fi

WASM_PATH="$(jq -r '.artifacts.component_wasm' component.manifest.json)"
if [[ -z "$WASM_PATH" || "$WASM_PATH" == "null" ]]; then
  echo "component.manifest.json is missing artifacts.component_wasm"
  exit 1
fi

if [[ ! -f "$WASM_PATH" ]]; then
  echo "WASM artifact not found at '$WASM_PATH'. Build the component first."
  exit 1
fi

tmp_inspect="$(mktemp)"
trap 'rm -f "$tmp_inspect"' EXIT
if wasm-tools component --help | rg -q '\binspect\b'; then
  wasm-tools component inspect "$WASM_PATH" >"$tmp_inspect"
else
  wasm-tools print "$WASM_PATH" >"$tmp_inspect"
fi

if rg -q 'greentic:state/state-store@1\.0\.0' "$tmp_inspect"; then
  if ! jq -e '
    .capabilities.host.state.read == true and
    .capabilities.host.state.write == true and
    .capabilities.host.state.delete == true
  ' component.manifest.json >/dev/null; then
    echo "state-store import detected in wasm, but host.state read/write/delete is not fully declared in component.manifest.json"
    exit 1
  fi

  if ! rg -q '"host:state"\.to_string\(\)' src/lib.rs; then
    echo "state-store import detected in wasm, but describe() does not declare host:state in src/lib.rs"
    exit 1
  fi
fi

echo "host:state capability check passed."
