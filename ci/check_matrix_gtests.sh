#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PATTERN='EXPECT_JSONPATH \$\{OUT\} error(\.code|\.details| not_exists)'

if rg -n "$PATTERN" tests/gtests/matrix/full tests/gtests/matrix/pairwise >/tmp/matrix-gtest-drift.txt 2>/dev/null; then
  echo "Stale generated matrix gtests detected."
  echo "The matrix corpus still contains top-level 'error' jsonpaths, but greentic-component test returns errors under 'result.error'."
  echo "Regenerate with:"
  echo "  ./tests/tools/gen_matrix --mode pairwise"
  echo "  ./tests/tools/gen_matrix --mode full"
  echo
  cat /tmp/matrix-gtest-drift.txt
  exit 1
fi

echo "Matrix gtest jsonpaths are current."
