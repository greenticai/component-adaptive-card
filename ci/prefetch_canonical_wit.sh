#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_FILE="$ROOT/Cargo.lock"
CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"

export PATH="$CARGO_BIN_DIR:$PATH"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$CARGO_BIN_DIR" >>"$GITHUB_PATH"
fi

if [[ ! -f "$LOCK_FILE" ]]; then
  echo "Cargo.lock not found at $LOCK_FILE" >&2
  exit 1
fi

INTERFACES_GUEST_VERSION="$(
  awk '
    $0 == "name = \"greentic-interfaces-guest\"" {
      getline
      if ($1 == "version") {
        gsub(/"/, "", $3)
        print $3
        exit
      }
    }
  ' "$LOCK_FILE"
)"

if [[ -z "${INTERFACES_GUEST_VERSION:-}" ]]; then
  echo "Unable to resolve greentic-interfaces-guest version from Cargo.lock" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$TMP_DIR/src"
cat >"$TMP_DIR/src/lib.rs" <<'EOF'
pub fn _prefetch_marker() {}
EOF

cat >"$TMP_DIR/Cargo.toml" <<EOF
[package]
name = "prefetch-canonical-wit"
version = "0.0.0"
edition = "2021"

[dependencies]
greentic-interfaces = { version = "=${INTERFACES_GUEST_VERSION}", default-features = false }
EOF

echo "Prefetching greentic-interfaces =${INTERFACES_GUEST_VERSION} source package..."
cargo fetch --manifest-path "$TMP_DIR/Cargo.toml"

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Installing wasm-tools for CI validation..."
  if ! command -v cargo-binstall >/dev/null 2>&1; then
    cargo install cargo-binstall --locked
  fi
  cargo binstall -y wasm-tools
fi
