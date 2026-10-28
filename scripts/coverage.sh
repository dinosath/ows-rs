#!/usr/bin/env bash
# Measures line coverage of the runtime library crates and fails if it is below
# the required threshold. The thin CLI wrapper and test-support crates are
# excluded from the gate (they have their own tests).
set -euo pipefail

THRESHOLD="${COVERAGE_THRESHOLD:-90.0}"
PACKAGES=(
  --package ows-core
  --package ows-expressions
  --package ows-dsl
  --package ows-events
  --package ows-scheduler
  --package ows-observability
  --package ows
)

cargo llvm-cov --all-features --no-fail-fast "${PACKAGES[@]}" --summary-only \
  | tee /tmp/ows-coverage.txt

TOTAL=$(grep -E "^TOTAL" /tmp/ows-coverage.txt)
LINE_PCT=$(echo "$TOTAL" | awk '{gsub("%", "", $10); print $10}')

echo ""
echo "Line coverage: ${LINE_PCT}% (threshold: ${THRESHOLD}%)"
if awk "BEGIN{exit !(${LINE_PCT} >= ${THRESHOLD})}"; then
  echo "Coverage check passed."
else
  echo "Coverage is below the required ${THRESHOLD}% threshold." >&2
  exit 1
fi
