#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON:-python3}"
readonly accepted_r3_ref="e9bcc05deca93e6683abca9b9688b1a814839120"

if ! git -C "$repo_root" cat-file -e "${accepted_r3_ref}^{commit}" 2>/dev/null; then
  echo "stage5f-r3-snapshot-gate: FAIL: accepted R3 commit unavailable" >&2
  exit 1
fi
if ! git -C "$repo_root" merge-base --is-ancestor "$accepted_r3_ref" HEAD; then
  echo "stage5f-r3-snapshot-gate: FAIL: accepted R3 is not an ancestor" >&2
  exit 1
fi

snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-r3-snapshot.XXXXXX")"
cleanup() {
  rm -rf "$snapshot_root"
}
trap cleanup EXIT HUP INT TERM

rm -rf "$snapshot_root"
git clone --quiet --shared --no-checkout "$repo_root" "$snapshot_root"
git -C "$snapshot_root" checkout --quiet --detach "$accepted_r3_ref"

if [[ "$(git -C "$snapshot_root" rev-parse HEAD)" != "$accepted_r3_ref" ]]; then
  echo "stage5f-r3-snapshot-gate: FAIL: accepted R3 snapshot checkout drift" >&2
  exit 1
fi

(
  cd "$snapshot_root"
  "$python_bin" scripts/stage5f_controlled_characterization_check.py
  "$python_bin" scripts/stage5f_controlled_characterization_negative_harness.py
  "$python_bin" scripts/stage5f_source_reachability_check.py
  "$python_bin" scripts/stage5f_source_reachability_negative_harness.py --r2-compat
  "$python_bin" scripts/stage5f_source_reachability_negative_harness.py
)

echo "stage5f-r3-snapshot-gate: ok source_ref=$accepted_r3_ref controlled_negative=51 r2_negative=27 r3_negative=45"
