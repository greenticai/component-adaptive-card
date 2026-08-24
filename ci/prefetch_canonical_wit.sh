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
    # Bootstrap from the prebuilt release binary: nothing is compiled, so a
    # cargo-binstall dependency raising its MSRV above the pinned toolchain
    # cannot break this step (cargo-binstall 1.22.0 did exactly that).
    binstall_ok=0
    for attempt in 1 2 3; do
      # `curl | bash` would hide a download failure: the pipeline reports
      # bash's status, and bash succeeds on empty input. Download, then run.
      binstall_installer="$(mktemp)"
      if curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh -o "$binstall_installer" && bash "$binstall_installer"; then
        hash -r
        if command -v cargo-binstall >/dev/null 2>&1; then binstall_ok=1; fi
      fi
      rm -f "$binstall_installer"
      if [ "$binstall_ok" -eq 1 ]; then break; fi
      sleep $((attempt * 5))
    done
    if [ "$binstall_ok" -ne 1 ]; then
      # Last release whose bundled lockfile still builds on 1.95.0.
      cargo install cargo-binstall --locked --version 1.21.1
    fi
  fi
  cargo binstall -y wasm-tools
fi
