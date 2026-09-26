#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

selector="stage8b_p1f_fixed_producers::tests::ie_linked_o2_fixed_producers_supervised_runtime_truth_and_readmission"
bash scripts/stage8b_p1f_ie_exact_test.sh finam-gateway "$selector"

echo "PASS stage8b-p1f-ie-linked-composition-witness steps=1 exact_selected=1 operational=false"
