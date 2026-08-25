#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bash scripts/stage8b_p_r1b_identity_gate.sh
python3 scripts/stage8b_p_r2a3_contract_refresh.py --verify
python3 scripts/stage8b_p_r2a3_review_closure_check.py
python3 scripts/stage8b_p_r2a3_negative_harness.py
python3 -m py_compile \
  scripts/make_stage8b_p_r2a3_handoff.py \
  scripts/stage8b_p_r2a3_contract_refresh.py \
  scripts/stage8b_p_r2a3_handoff_safety_check.py \
  scripts/stage8b_p_r2a3_review_closure_check.py \
  scripts/stage8b_p_r2a3_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a3-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a3-build-evidence.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json >/dev/null

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
cargo build --release --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --bins
tools/stage8b-readonly-preflight/target/release/stage8b-readonly-preflight --qualify-controlled
if tools/stage8b-readonly-preflight/target/release/stage8b-readonly-preflight --r2b-one-shot >/dev/null 2>&1; then
  echo "stage8b-p-r2a3-gate: FAIL R2B opened without an ISSUED package" >&2
  exit 1
fi

linux_bin_dir="${STAGE8B_R2A3_LINUX_BIN_DIR:-tmp/stage8b-r2a3-linux-final}"
python3 - "$linux_bin_dir" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2a3-build-evidence.json").read_text())
for name, expected in build["linux_release_sha256"].items():
    path = root / name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f"Linux release artifact mismatch: {name}")
print("stage8b-p-r2a3-linux-artifacts: PASS count=3")
PY

if command -v docker >/dev/null 2>&1; then
  docker run --rm --platform linux/amd64 \
    -v "$repo_root/$linux_bin_dir:/bin-r2a3:ro" \
    ubuntu:24.04 /bin-r2a3/stage8b-readonly-preflight --qualify-controlled
  if docker run --rm --platform linux/amd64 \
    -v "$repo_root/$linux_bin_dir:/opt/moex-trading/stage8b-r2a3/bin:ro" \
    ubuntu:24.04 /opt/moex-trading/stage8b-r2a3/bin/stage8b-r2a3-launcher >/dev/null 2>&1; then
    echo "stage8b-p-r2a3-gate: FAIL Linux launcher opened without run package" >&2
    exit 1
  fi
fi

git diff --check
echo "stage8b-p-r2a3-gate: PASS inherited=134 r2a3_negative=47 rust_tests=32 contracts=6 fixtures=6 linux_fd_launch=true authorization=NOT_ISSUED real_http=false"
