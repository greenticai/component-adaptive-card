#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
export PATH="$CARGO_BIN_DIR:$PATH"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 not found; aborting." >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd greentic-integration-tester
require_cmd greentic-component
require_cmd wasm-tools

CHAOS_SEEDS="${CHAOS_SEEDS:-101 202}"
GREENTIC_FAIL_ASSET_TRANSIENT="${GREENTIC_FAIL_ASSET_TRANSIENT:-1/5}"
GREENTIC_FAIL_DELAY_STATE_READ_MS="${GREENTIC_FAIL_DELAY_STATE_READ_MS:-50}"
GREENTIC_FAIL_DUPLICATE_INTERACTION="${GREENTIC_FAIL_DUPLICATE_INTERACTION:-1}"

export CHAOS_SEEDS
export GREENTIC_FAIL_ASSET_TRANSIENT
export GREENTIC_FAIL_DELAY_STATE_READ_MS
export GREENTIC_FAIL_DUPLICATE_INTERACTION

DATE="$(date -u +%Y-%m-%d)"
CORPUS_ROOT="${CORPUS_ROOT:-corpus/${DATE}}"

echo "Prefetching canonical WIT sources..."
./ci/prefetch_canonical_wit.sh

echo "Building wasm32-wasip2 (release)..."
cargo build --target wasm32-wasip2 --release

echo "Generating matrix gtests..."
./tests/tools/gen_matrix --mode full

echo "Checking matrix gtest drift..."
bash ./ci/check_matrix_gtests.sh

mkdir -p "${CORPUS_ROOT}"
echo "Writing nightly chaos artifacts to ${CORPUS_ROOT}"

for SEED in ${CHAOS_SEEDS}; do
  CORPUS_DIR="${CORPUS_ROOT}/${SEED}"
  mkdir -p "${CORPUS_DIR}"
  echo "Replay: greentic-integration-tester run --gtest tests/gtests --artifacts-dir ${CORPUS_DIR} --seed ${SEED}" > "${CORPUS_DIR}/replay.txt"

  echo "Running negative gtests with seed ${SEED}..."
  greentic-integration-tester run --gtest tests/gtests/negative --artifacts-dir "${CORPUS_DIR}/negative" --seed "${SEED}"

  echo "Running pairwise matrix gtests with seed ${SEED}..."
  greentic-integration-tester run --gtest tests/gtests/matrix/pairwise --artifacts-dir "${CORPUS_DIR}/matrix/pairwise" --seed "${SEED}"

  echo "Running full matrix gtests with seed ${SEED}..."
  greentic-integration-tester run --gtest tests/gtests/matrix/full --artifacts-dir "${CORPUS_DIR}/matrix/full" --seed "${SEED}"
done

echo "Nightly chaos run completed."
echo "Artifacts: ${CORPUS_ROOT}"
