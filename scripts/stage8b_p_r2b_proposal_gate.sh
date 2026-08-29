#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a8_review_closure_check.py
python3 scripts/stage8b_p_r2a8_negative_harness.py
python3 scripts/stage8b_p_r2a8_r1_readiness_negative_harness.py
python3 scripts/stage8b_p_r2b_proposal_check.py
python3 scripts/stage8b_p_r2b_proposal_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_proposal_check.py \
  scripts/stage8b_p_r2b_proposal_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2b-proposal-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-r4-build-evidence.json >/dev/null

cargo fmt --all -- --check
cargo test -p finam-gateway --features stage8b-r2a7-controlled-qualification \
  stage8b_r2a7_source_adapter --no-fail-fast
cargo clippy -p finam-gateway --all-targets \
  --features stage8b-r2a7-controlled-qualification -- -D warnings
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
  --all-targets -- -D warnings

production_a="${STAGE8B_R2B_PRODUCTION_A:-tmp/stage8b-r2b-r4r2-production-a/release}"
production_b="${STAGE8B_R2B_PRODUCTION_B:-tmp/stage8b-r2b-r4r2-production-b/release}"
tool_a="${STAGE8B_R2B_TOOL_A:-tmp/stage8b-r2b-r4-tool-a/release}"
tool_b="${STAGE8B_R2B_TOOL_B:-tmp/stage8b-r2b-r4-tool-b/release}"
controlled_a="${STAGE8B_R2B_CONTROLLED_A:-tmp/stage8b-r2b-r4r2-controlled-a/release}"
controlled_b="${STAGE8B_R2B_CONTROLLED_B:-tmp/stage8b-r2b-r4r2-controlled-b/release}"
controlled_launcher_a="${STAGE8B_R2B_CONTROLLED_LAUNCHER_A:-tmp/stage8b-r2b-r4-controlled-launcher-a/release}"
controlled_launcher_b="${STAGE8B_R2B_CONTROLLED_LAUNCHER_B:-tmp/stage8b-r2b-r4-controlled-launcher-b/release}"

python3 - "$production_a" "$production_b" "$tool_a" "$tool_b" "$controlled_a" "$controlled_b" "$controlled_launcher_a" "$controlled_launcher_b" <<'PY'
import hashlib
import json
import pathlib
import sys

production_a, production_b, tool_a, tool_b, controlled_a, controlled_b, controlled_launcher_a, controlled_launcher_b = map(pathlib.Path, sys.argv[1:])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2b-r4-build-evidence.json").read_text())

def digest(path: pathlib.Path) -> str:
    if not path.is_file():
        raise SystemExit(f"missing Linux artifact: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()

for name, record in build["production_binaries"].items():
    artifact = "stage8b-readonly-preflight" if name == "accepted-stage8b-readonly-preflight" else name
    tool_names = {
        "accepted-stage8b-readonly-preflight", "stage8b-r2b-launcher",
        "stage8b-r2a5-authority-producer", "stage8b-r2a5-authority-issuer",
        "stage8b-r2a5-package-issuer",
    }
    root_a = tool_a if name in tool_names else production_a
    root_b = tool_b if name in tool_names else production_b
    if digest(root_a / artifact) != record["build_a_sha256"] or digest(root_b / artifact) != record["build_b_sha256"]:
        raise SystemExit(f"production Linux artifact drift: {name}")
for name, record in build["controlled_qualification_binaries"].items():
    artifact = "stage8b-r2b-launcher" if name == "stage8b-r2b-controlled-custody-launcher" else name
    if name == "stage8b-r2b-controlled-custody-launcher":
        root_a, root_b = controlled_launcher_a, controlled_launcher_b
    elif name == "stage8b-r2a5-controlled-server":
        root_a, root_b = tool_a, tool_b
    elif name == "stage8b-r2a5-controlled-layout":
        root_a, root_b = tool_a, tool_b
    else:
        root_a, root_b = controlled_a, controlled_b
    if digest(root_a / artifact) != record["build_a_sha256"] or digest(root_b / artifact) != record["build_b_sha256"]:
        raise SystemExit(f"controlled Linux artifact drift: {name}")
print(f"stage8b-p-r2b-linux-artifacts: PASS production={len(build['production_binaries'])}x2 controlled={len(build['controlled_qualification_binaries'])}x2")
PY

if command -v docker >/dev/null 2>&1 && [[ "${STAGE8B_R2B_SKIP_LINUX_TESTS:-0}" != "1" ]]; then
  image="rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0"
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work" -w /work "$image" \
    bash -c 'CARGO_TARGET_DIR=/work/tmp/stage8b-r2b-r4-linux-tests cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- --test-threads=1'
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work:ro" -w /work "$image" \
    bash scripts/stage8b_p_r2a7_linux_rehearsal.sh \
      "/work/$controlled_a/stage8b-r2a7-source-adapter" \
      "/work/$controlled_a/stage8b-r2a7-controlled-seeder" \
      "/work/$controlled_a/stage8b-r2a8-current-manifest-issuer"
  docker run --rm --platform linux/amd64 --network none \
    -v "$repo_root:/work:ro" -w /work "$image" \
    bash scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh \
      "/work/$tool_a" "/work/$controlled_a" \
      "/work/$controlled_launcher_a/stage8b-r2b-launcher" \
      "/work/$production_a"
fi

git diff --check
echo "stage8b-p-r2b-proposal-gate: PASS revision=R4-R2 closure=R4-R2A rows=85 negative_mutations=284 external_c0=true refreshed_c1=true causal_cycle=false writer_unit=true upstream_publisher=true production_reachable=true empty_root_generation_one=true renewal=true source_chain=true predecessor_snapshot_source=false creator=true isolation=true typed_terminal=true absolute_deadline=true metadata_fsync=true stager=true root_authenticated=true immutable_terminal=true supervisor=true hardening=true place=true cancel=true authorization=NOT_ISSUED external_network=false order_post_delete=false runtime_live=false"
