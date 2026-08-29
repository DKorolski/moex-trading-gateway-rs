#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_systemd_check.py
python3 scripts/stage8b_p_r2b_issuance_check.py
python3 scripts/stage8b_p_r2b_issuance_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_issuance_systemd_check.py \
  scripts/stage8b_p_r2b_issuance_check.py \
  scripts/stage8b_p_r2b_issuance_negative_harness.py \
  scripts/stage8b_p_r2b_issuance_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_issuance_handoff.py
python3 -m json.tool \
  docs/stage-8/stage8b-p-r2b-issuance-package-r0-authority.json >/dev/null
python3 -m json.tool \
  docs/stage-8/stage8b-p-r2b-issuance-package-r0-evidence.json >/dev/null

while IFS= read -r path; do
  case "$path" in
    Cargo.toml|Cargo.lock|crates/*|tools/*)
      echo "stage8b-p-r2b-issuance-gate: FAIL production source drift: $path" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only f24f1044ac0b29c2f588853b817e519cfe8d3d8b --)

if [[ -e deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target ]]; then
  echo "stage8b-p-r2b-issuance-gate: FAIL activation target implemented in R0" >&2
  exit 1
fi

git diff --check
echo "stage8b-p-r2b-issuance-gate: PASS revision=R0 rows=25 transaction_services=30 shipped_units=9 negative_mutations=16 target_implemented=false operator_selection=ABSENT authorization=NOT_ISSUED finam=false broker_get=false post_delete=false runtime_live=false"
