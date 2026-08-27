#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a7_review_closure_check.py
python3 scripts/stage8b_p_r2a7_negative_harness.py
python3 -m py_compile scripts/stage8b_p_r2a7_review_closure_check.py scripts/stage8b_p_r2a7_negative_harness.py scripts/stage8b_p_r2a7_handoff_safety_check.py scripts/make_stage8b_p_r2a7_handoff.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a7-status.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a7-build-evidence.json >/dev/null

cargo fmt --all -- --check
cargo test -p finam-gateway --features stage8b-r2a7-controlled-qualification stage8b_r2a7_source_adapter --no-fail-fast
cargo clippy -p finam-gateway --features stage8b-r2a7-source-adapter --bin stage8b-r2a7-source-adapter -- -D warnings

adapter_a="${STAGE8B_R2A7_ADAPTER_A:-tmp/stage8b-r2a7-adapter-exact-a/release/stage8b-r2a7-source-adapter}"
adapter_b="${STAGE8B_R2A7_ADAPTER_B:-tmp/stage8b-r2a7-adapter-exact-b/release/stage8b-r2a7-source-adapter}"
seeder="${STAGE8B_R2A7_SEEDER:-tmp/stage8b-r2a7-controlled/release/stage8b-r2a7-controlled-seeder}"
python3 - "$adapter_a" "$adapter_b" <<'PY'
import hashlib, json, pathlib, sys

build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2a7-build-evidence.json").read_text())
for path in map(pathlib.Path, sys.argv[1:]):
    if not path.is_file():
        raise SystemExit(f"missing Linux adapter: {path}")
    if hashlib.sha256(path.read_bytes()).hexdigest() != build["build_a_sha256"]:
        raise SystemExit(f"R2A7 Linux adapter drift: {path}")
print("stage8b-p-r2a7-linux-artifacts: PASS reproducible=2 fixture_graph=false")
PY

if command -v docker >/dev/null 2>&1 && [[ "${STAGE8B_R2A7_SKIP_REHEARSAL:-0}" != "1" ]]; then
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work:ro" -w /work \
    rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
    bash scripts/stage8b_p_r2a7_linux_rehearsal.sh "/work/$adapter_a" "/work/$seeder"
fi

git diff --check
echo "stage8b-p-r2a7-gate: PASS production_reader=true fixture_graph=false current_tree_negative=33 r2a7_negative=18 reproducible_adapter=true place=true cancel=true authorization=NOT_ISSUED real_finam=false"
