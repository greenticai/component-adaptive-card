#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "Prefetching canonical WIT sources..."
./ci/prefetch_canonical_wit.sh

echo "Running cargo fmt..."
cargo fmt --all -- --check

echo "Running cargo clippy..."
cargo clippy --workspace --all-targets -- -D warnings

echo "Running cargo clippy (wasm32-wasip2)..."
cargo clippy --workspace --all-targets --target wasm32-wasip2 -- -D warnings

echo "Running cargo test..."
cargo test --workspace --all-targets

echo "Building wasm32-wasip2 (release)..."
cargo build --target wasm32-wasip2 --release

echo "Syncing dist wasm..."
./ci/sync_dist_wasm.sh

echo "Checking host:state capability wiring..."
./ci/check_host_state_capability.sh

if command -v greentic-integration-tester >/dev/null 2>&1; then
  echo "Running README gtests..."
  greentic-integration-tester run --gtest tests/gtests/README --artifacts-dir artifacts/readme-gtests --errors
else
  echo "greentic-integration-tester not found; skipping README gtests."
fi

echo "All checks passed."
