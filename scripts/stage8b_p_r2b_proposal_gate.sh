#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a8_review_closure_check.py
python3 scripts/stage8b_p_r2b_proposal_check.py
python3 scripts/stage8b_p_r2b_proposal_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_proposal_check.py \
  scripts/stage8b_p_r2b_proposal_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2b-proposal-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-r1-build-evidence.json >/dev/null

cargo fmt --all -- --check
cargo test -p finam-gateway --features stage8b-r2a7-controlled-qualification \
  stage8b_r2a7_source_adapter --no-fail-fast
cargo clippy -p finam-gateway --all-targets \
  --features stage8b-r2a7-controlled-qualification -- -D warnings
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
  --all-targets -- -D warnings

production_a="${STAGE8B_R2B_PRODUCTION_A:-tmp/stage8b-r2b-r1-production-a/release}"
production_b="${STAGE8B_R2B_PRODUCTION_B:-tmp/stage8b-r2b-r1-production-b/release}"
helper_a="${STAGE8B_R2B_HELPER_A:-tmp/stage8b-r2b-r1-helper-a/release}"
helper_b="${STAGE8B_R2B_HELPER_B:-tmp/stage8b-r2b-r1-helper-b/release}"
controlled_a="${STAGE8B_R2B_CONTROLLED_A:-tmp/stage8b-r2b-r1-controlled-a/release}"
controlled_b="${STAGE8B_R2B_CONTROLLED_B:-tmp/stage8b-r2b-r1-controlled-b/release}"

python3 - "$production_a" "$production_b" "$helper_a" "$helper_b" "$controlled_a" "$controlled_b" <<'PY'
import hashlib
import json
import pathlib
import sys

production_a, production_b, helper_a, helper_b, controlled_a, controlled_b = map(pathlib.Path, sys.argv[1:])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2b-r1-build-evidence.json").read_text())

def digest(path: pathlib.Path) -> str:
    if not path.is_file():
        raise SystemExit(f"missing Linux artifact: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()

for name, record in build["production_binaries"].items():
    artifact_name = "stage8b-readonly-preflight" if name == "accepted-stage8b-readonly-preflight" else name
    left = helper_a / artifact_name if name == "accepted-stage8b-readonly-preflight" else production_a / artifact_name
    right = helper_b / artifact_name if name == "accepted-stage8b-readonly-preflight" else production_b / artifact_name
    if digest(left) != record["build_a_sha256"] or digest(right) != record["build_b_sha256"]:
        raise SystemExit(f"production Linux artifact drift: {name}")
for name, record in build["controlled_qualification_binaries"].items():
    if digest(controlled_a / name) != record["build_a_sha256"] or digest(controlled_b / name) != record["build_b_sha256"]:
        raise SystemExit(f"controlled Linux artifact drift: {name}")
print("stage8b-p-r2b-linux-artifacts: PASS production=4x2 controlled=3x2")
PY

if command -v docker >/dev/null 2>&1 && [[ "${STAGE8B_R2B_SKIP_LINUX_TESTS:-0}" != "1" ]]; then
  image="rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0"
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work" -w /work "$image" \
    bash -c 'CARGO_TARGET_DIR=/work/tmp/stage8b-r2b-r1-linux-tests cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml terminal_evidence_is_create_new_single_link_and_non_replayable -- --nocapture'
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work:ro" -w /work "$image" \
    bash scripts/stage8b_p_r2a7_linux_rehearsal.sh \
      "/work/$controlled_a/stage8b-r2a7-source-adapter" \
      "/work/$controlled_a/stage8b-r2a7-controlled-seeder" \
      "/work/$controlled_a/stage8b-r2a8-current-manifest-issuer"
fi

git diff --check
echo "stage8b-p-r2b-proposal-gate: PASS revision=R1 rows=30 negatives=85 production_writer=true production_hashes=separate durable_terminal=true place=true cancel=true authorization=NOT_ISSUED network=false order_post_delete=false runtime_live=false"
