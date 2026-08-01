#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly accepted_b1_ref="86b43c448fb65a3c54b6118d04d3f40e08e74ad7"
python_bin=""
portable_bin=""
snapshot_root=""

cleanup() {
  local status=$?
  if [[ -n "$portable_bin" ]]; then
    rm -rf "$portable_bin"
  fi
  if [[ -n "$snapshot_root" ]]; then
    rm -rf "$snapshot_root"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

for candidate in python3.13 python3.12 python3.11 python3; do
  if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import tomllib' >/dev/null 2>&1; then
    python_bin="$(command -v "$candidate")"
    break
  fi
done
if [[ -z "$python_bin" ]]; then
  echo "stage5f-forbidden-no-rg-gate: FAIL: Python 3.11+ with tomllib is required" >&2
  exit 1
fi

portable_bin="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-no-rg-path.XXXXXX")"
ln -s "$python_bin" "$portable_bin/python3"
snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-no-rg-b1.XXXXXX")"

git -C "$repo_root" cat-file -e "${accepted_b1_ref}^{commit}"
git -C "$repo_root" archive "$accepted_b1_ref" | tar -x -C "$snapshot_root"

restricted_path="$portable_bin:/usr/bin:/bin:/usr/sbin:/sbin"
if PATH="$restricted_path" command -v rg >/dev/null 2>&1; then
  echo "stage5f-forbidden-no-rg-gate: FAIL: rg remains visible" >&2
  exit 1
fi

(
  cd "$snapshot_root"
  PATH="$restricted_path" bash scripts/forbidden_surface_negative_harness.sh
)

echo "stage5f-forbidden-no-rg-gate: ok source_ref=$accepted_b1_ref rg_absent=true cases=87"
