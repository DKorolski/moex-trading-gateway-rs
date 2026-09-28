#!/usr/bin/env bash
set -euo pipefail

echo "source_ref=$(git rev-parse HEAD)"
echo "source_tree=$(git rev-parse 'HEAD^{tree}')"
python3 --version

run() {
  printf 'COMMAND:'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run git diff --check
run python3 scripts/stage8b_p1e_i1_first_boot_governance_check.py
run python3 scripts/stage8b_p1e_i1_first_boot_governance_negative_harness.py
run python3 scripts/current_tree_authority_check.py
run python3 scripts/current_tree_authority_negative_harness.py

echo "PASS stage8b-p1e-i1-first-boot-governance-gate source=accepted wire=v2 transaction_v5=false receipt_v2=false deployable_i1=false operational_redis=false finam_write=false live=false"
