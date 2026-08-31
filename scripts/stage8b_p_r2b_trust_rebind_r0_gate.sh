#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -z "${STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR:-}" ]]; then
  echo "stage8b-p-r2b-trust-rebind-r0-gate: FAIL missing local ceremony environment" >&2
  exit 1
fi

receipt_temp="$(mktemp -d)"
trap 'rm -rf "$receipt_temp"' EXIT
receipt_out="${STAGE8B_R2B_TRUST_REBIND_RECEIPT_OUT:-$receipt_temp/primary-ceremony-verification-receipt.json}"
if [[ -e "$receipt_out" ]]; then
  echo "stage8b-p-r2b-trust-rebind-r0-gate: FAIL receipt output already exists" >&2
  exit 1
fi
export STAGE8B_R2B_TRUST_REBIND_RECEIPT_OUT="$receipt_out"

python3 scripts/stage8b_p_r2b_trust_rebind_r0_actual_ceremony_verify.py
source_ref="$(git rev-parse HEAD)"
python3 scripts/stage8b_p_r2b_trust_rebind_r0_receipt.py \
  "$receipt_out" \
  --source-ref "$source_ref"
receipt_sha256="$(shasum -a 256 "$receipt_out" | awk '{print $1}')"
echo "actual_ceremony_verifier=PASS receipt_sha256=$receipt_sha256 private_path_recorded=false"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_check.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_trust_rebind_r0_check.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_handoff_safety_check.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_receipt.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_actual_ceremony_verify.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_handoff_negative_harness.py \
  scripts/make_stage8b_p_r2b_trust_rebind_r0_handoff.py
for document in \
  docs/stage-8/stage8b-p-r2b-trust-rebind-r0-authority.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-r0-supersession.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json; do
  python3 -m json.tool "$document" >/dev/null
done

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
git diff --check

echo "stage8b-p-r2b-trust-rebind-r0-gate: PASS stage=R0-R1 generation=2 rust_tests=52 source_negative=46 receipt_negative=10 actual_ceremony_verifier=PASS receipt_signed=true historical_immutable=true backup=REQUIRED_NOT_VERIFIED active=false authorization=NOT_ISSUED finam=false"
