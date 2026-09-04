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

# Last cargo-binstall release whose bundled lockfile still builds on the
# canonical 1.95.0 toolchain. 1.22.0 pins vergen 10.0.2, which raised its MSRV
# to 1.96.0, so building cargo-binstall from source is no longer possible here.
# Only used if the prebuilt-binary bootstrap below is unreachable.
BINSTALL_SOURCE_FALLBACK_VERSION="1.21.1"
BINSTALL_RELEASE_INSTALLER="https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh"

bootstrap_cargo_binstall() {
  if command -v cargo-binstall >/dev/null 2>&1; then
    return 0
  fi

  # Prefer the prebuilt release binary: it needs no compilation and therefore
  # cannot be broken by an MSRV bump in cargo-binstall's dependency tree.
  if command -v curl >/dev/null 2>&1; then
    for attempt in 1 2 3; do
      echo "Bootstrapping cargo-binstall from prebuilt release (attempt ${attempt}/3)..."
      if curl -L --proto '=https' --tlsv1.2 -sSf "$BINSTALL_RELEASE_INSTALLER" | bash; then
        hash -r
        if command -v cargo-binstall >/dev/null 2>&1; then
          return 0
        fi
      fi
      if [[ "$attempt" -lt 3 ]]; then
        sleep "$((attempt * 5))"
      fi
    done
    echo "Prebuilt cargo-binstall bootstrap failed after 3 attempts; building from source." >&2
  else
    echo "curl not available; building cargo-binstall from source." >&2
  fi

  echo "Installing cargo-binstall ${BINSTALL_SOURCE_FALLBACK_VERSION} from source..."
  cargo install cargo-binstall --locked --version "$BINSTALL_SOURCE_FALLBACK_VERSION"
}

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Installing wasm-tools for CI validation..."
  bootstrap_cargo_binstall
  cargo binstall -y wasm-tools
fi
