#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v greentic-integration-tester >/dev/null 2>&1; then
  echo "greentic-integration-tester not found; aborting."
  exit 1
fi

ARTIFACTS_ROOT="${ARTIFACTS_ROOT:-artifacts}"

echo "Running PR gate gtests..."
./tests/tools/gen_matrix --mode pairwise
bash ./ci/check_matrix_gtests.sh
mkdir -p "${ARTIFACTS_ROOT}/readme" "${ARTIFACTS_ROOT}/matrix" "${ARTIFACTS_ROOT}/negative"
greentic-integration-tester run --gtest tests/gtests/README --artifacts-dir "${ARTIFACTS_ROOT}/readme" --errors
greentic-integration-tester run --gtest tests/gtests/matrix/pairwise --artifacts-dir "${ARTIFACTS_ROOT}/matrix" --errors
greentic-integration-tester run --gtest tests/gtests/negative/smoke --artifacts-dir "${ARTIFACTS_ROOT}/negative" --errors

echo "PR gate gtests passed."
