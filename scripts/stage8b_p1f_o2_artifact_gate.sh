#!/usr/bin/env bash
set -euo pipefail

python3 scripts/test_stage8b_p1f_o2_profile_binding.py
python3 scripts/stage8b_p1f_o2_artifact_check.py
python3 scripts/stage8b_p1f_o2_artifact_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1f_o2_artifact_check.py \
  scripts/stage8b_p1f_o2_artifact_negative_harness.py \
  scripts/stage8b_p1f_o2_build_linux.py \
  scripts/make_stage8b_p1f_o2_artifact_handoff.py \
  scripts/test_stage8b_p1f_o2_artifact_archive.py \
  scripts/stage8b_p1f_o2_artifact_handoff_safety_check.py
bash -n scripts/stage8b_p1f_o2_elf_smoke.sh
python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
bash scripts/stage8b_p1f_o2_artifact_witness.sh
cargo fmt --all -- --check
cargo clippy -p broker-finam -p finam-gateway -p runtime-durable-service \
  --all-targets --all-features -- -D warnings
git diff --check
echo "PASS stage8b-p1f-o2-artifact-gate execution=false"
