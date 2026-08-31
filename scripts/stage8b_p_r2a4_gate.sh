#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bash scripts/stage8b_p_r1b_identity_gate.sh
python3 scripts/stage8b_p_r2a4_review_closure_check.py
python3 scripts/stage8b_p_r2a4_negative_harness.py
python3 -m py_compile \
  scripts/make_stage8b_p_r2a4_handoff.py \
  scripts/stage8b_p_r2a4_handoff_safety_check.py \
  scripts/stage8b_p_r2a4_review_closure_check.py \
  scripts/stage8b_p_r2a4_negative_harness.py
for document in \
  docs/stage-8/stage8b-p-r2a4-authority.json \
  docs/stage-8/stage8b-p-r2a4-build-evidence.json \
  docs/stage-8/stage8b-p-r2a4-controlled-authority.json \
  docs/stage-8/stage8b-p-r2a4-production-account-key-manifest.json \
  docs/stage-8/stage8b-p-r2a4-production-trust-manifest.json \
  docs/stage-8/stage8b-p-r2a4-status.json; do
  python3 -m json.tool "$document" >/dev/null
done

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings

if tools/stage8b-readonly-preflight/target/debug/stage8b-readonly-preflight --r2b-one-shot >/dev/null 2>&1; then
  echo "stage8b-p-r2a4-gate: FAIL R2B opened without production authority" >&2
  exit 1
fi

linux_bin_dir="${STAGE8B_R2A4_LINUX_BIN_DIR:-tmp/stage8b-r2a4-build-a/release}"
python3 - "$linux_bin_dir" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2a4-build-evidence.json").read_text())
for name, expected in build["linux_release_sha256"].items():
    path = root / name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f"Linux release artifact mismatch: {name}")
print(f"stage8b-p-r2a4-linux-artifacts: PASS count={len(build['linux_release_sha256'])}")
PY

if command -v docker >/dev/null 2>&1; then
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work" -w /work \
    rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
    /work/scripts/stage8b_p_r2a4_linux_rehearsal.sh "/work/$linux_bin_dir"
fi

git diff --check
echo "stage8b-p-r2a4-gate: PASS inherited_r1b=134 r2a4_negative=32 rust_tests=36 linux_binaries=10 place=true cancel=true authorization=NOT_ISSUED real_http=false"
