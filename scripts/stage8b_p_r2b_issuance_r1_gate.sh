#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_read_contract_refresh.py
python3 scripts/stage8b_p_r2b_issuance_r1_check.py
python3 scripts/stage8b_p_r2b_issuance_r1_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_r1a_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_r1a1_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_read_contract_refresh.py \
  scripts/stage8b_p_r2b_issuance_r1_check.py \
  scripts/stage8b_p_r2b_issuance_r1_negative_harness.py \
  scripts/stage8b_p_r2b_issuance_r1a_negative_harness.py \
  scripts/stage8b_p_r2b_issuance_r1a1_negative_harness.py \
  scripts/stage8b_p_r2b_issuance_r1_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_issuance_r1_handoff.py
python3 -m json.tool \
  docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json >/dev/null
python3 -m json.tool \
  docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json >/dev/null
python3 -m json.tool \
  docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json >/dev/null

while IFS= read -r path; do
  case "$path" in
    Cargo.toml|Cargo.lock|crates/*|tools/*|deploy/*)
      echo "stage8b-p-r2b-issuance-r1-gate: FAIL design closure changed production: $path" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only 928168ed47e5b9dd873cd73815fbccecde7a8981 --)

for forbidden in \
  deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target \
  deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service \
  deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service \
  tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs
do
  if [[ -e "$forbidden" ]]; then
    echo "stage8b-p-r2b-issuance-r1-gate: FAIL implementation present: $forbidden" >&2
    exit 1
  fi
done

git diff --check
echo "stage8b-p-r2b-issuance-r1-gate: PASS revision=R0-R1A1 rows=66 read_documents=6 snapshot_sha256=7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5 services=31 phases=6 negative_mutations=66 strict_schema=true exact_freeze=true builder=SEPARATE target_implemented=false operator_selection=ABSENT authorization=NOT_ISSUED finam=false authservice=false broker_get=false post_delete=false runtime_live=false"
