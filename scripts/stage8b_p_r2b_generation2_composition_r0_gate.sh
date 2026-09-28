#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_check.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_generation2_composition_rebuild_r0_issue_helper.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_materialize_phase6.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_check.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_negative_harness.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_handoff_safety_check.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_handoff_negative_harness.py \
  scripts/make_stage8b_p_r2b_generation2_composition_r0_handoff.py
bash -n \
  scripts/stage8b_p_r2b_generation2_composition_rebuild_r0_build_linux.sh \
  scripts/stage8b_p_r2b_generation2_composition_r0_phase6_runner.sh

for document in \
  docs/stage-8/stage8b-p-r2b-generation2-production-authority.json \
  docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-authority.json \
  docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-build-evidence.json \
  docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-rehearsal-evidence.json \
  docs/stage-8/stage8b-p-r2b-generation2-composition-r0-authority.json; do
  python3 -m json.tool "$document" >/dev/null
done

materialized_root="$(mktemp -d "$repo_root/tmp/stage8b-g2-gate-materialized.XXXXXX")"
python3 scripts/stage8b_p_r2b_generation2_composition_r0_materialize_phase6.py \
  "$materialized_root/rehearsal.sh"
bash -n "$materialized_root/rehearsal.sh"
if rg -q 'generation-1\.hex|stage8b-p-r2a5-production-trust-manifest|stage8b-p-r2a5-production-account-key-manifest' \
  "$materialized_root/rehearsal.sh"; then
  echo "stage8b-generation2-composition-r0-gate: FAIL Generation-1 residue" >&2
  exit 1
fi

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
git diff --check

echo "stage8b-generation2-composition-r0-gate: PASS generation=2 production_binaries=7 builds=2 negative=41 phase6=PASS network=none active=false authorization=NOT_ISSUED finam=false"
