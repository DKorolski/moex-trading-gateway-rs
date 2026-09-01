#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_check.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_r1_check.py
python3 scripts/stage8b_p_r2b_generation2_composition_r0_r1_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_r1_materialize_phase6.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_r1_check.py \
  scripts/stage8b_p_r2b_generation2_composition_r0_r1_negative_harness.py
bash -n scripts/stage8b_p_r2b_generation2_composition_r0_r1_phase6_runner.sh

for document in \
  docs/stage-8/stage8b-p-r2b-generation2-composition-r0-r1-linux-rehearsal-evidence.json \
  docs/stage-8/stage8b-p-r2b-generation2-composition-r0-r1-authority.json; do
  python3 -m json.tool "$document" >/dev/null
done

materialized_root="$(mktemp -d "$repo_root/tmp/stage8b-g2-r0-r1-gate.XXXXXX")"
trap 'rm -rf "$materialized_root"' EXIT
python3 scripts/stage8b_p_r2b_generation2_composition_r0_r1_materialize_phase6.py \
  "$materialized_root/rehearsal.sh"
bash -n "$materialized_root/rehearsal.sh"
if rg -q \
  'generation-1\.hex|AUTH_SESSION_FAILURE|"actual_read_attempts":True|stage8b-p-r2a5-production-trust-manifest|stage8b-p-r2a5-production-account-key-manifest' \
  "$materialized_root/rehearsal.sh"; then
  echo "stage8b-generation2-composition-r0-r1-gate: FAIL legacy/category-only proof residue" >&2
  exit 1
fi
rg -q 'stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle\.py' \
  "$materialized_root/rehearsal.sh"
rg -q 'request_boundary_proof\["actual_read_attempts"\]' \
  "$materialized_root/rehearsal.sh"

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
git diff --check

echo "stage8b-generation2-composition-r0-r1-gate: PASS request=POST:/v1/sessions:1 outcomes=NETWORK_CONNECT_FAILURE|TIMEOUT category_only=false negative=12 binaries_rebuilt=false network=none active=false authorization=NOT_ISSUED finam=false"
