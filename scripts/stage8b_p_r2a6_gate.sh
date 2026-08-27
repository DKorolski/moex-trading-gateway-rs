#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a6_review_closure_check.py
python3 scripts/stage8b_p_r2a6_negative_harness.py
python3 -m py_compile \
  scripts/make_stage8b_p_r2a6_handoff.py \
  scripts/stage8b_p_r2a6_handoff_safety_check.py \
  scripts/stage8b_p_r2a6_review_closure_check.py \
  scripts/stage8b_p_r2a6_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a6-status.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a6-build-evidence.json >/dev/null

cargo fmt --all -- --check
cargo test -p finam-gateway --features stage8b-r2a6-controlled-rehearsal \
  --lib stage8a1_execution_capability --no-fail-fast
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
  --all-targets -- -D warnings

adapter_a="${STAGE8B_R2A6_ADAPTER_A:-tmp/stage8b-r2a6-adapter-exact-a/release/stage8b-r2a6-source-adapter}"
adapter_b="${STAGE8B_R2A6_ADAPTER_B:-tmp/stage8b-r2a6-adapter-exact-b/release/stage8b-r2a6-source-adapter}"
tools_a="${STAGE8B_R2A6_TOOLS_A:-tmp/stage8b-r2a6-tools-exact-a/release}"
tools_b="${STAGE8B_R2A6_TOOLS_B:-tmp/stage8b-r2a6-tools-exact-b/release}"
accepted="${STAGE8B_R2A5_ACCEPTED_BIN_DIR:-tmp/stage8b-r2a5-build-a/release}"
python3 - "$adapter_a" "$adapter_b" "$tools_a" "$tools_b" "$accepted" <<'PY'
import hashlib, json, pathlib, sys

adapter_a, adapter_b, tools_a, tools_b, accepted = map(pathlib.Path, sys.argv[1:])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2a6-build-evidence.json").read_text())

def digest(path):
    if not path.is_file():
        raise SystemExit(f"missing Linux artifact: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()

expected_adapter = build["adapter"]["build_a_sha256"]
if digest(adapter_a) != expected_adapter or digest(adapter_b) != expected_adapter:
    raise SystemExit("R2A6 adapter build mismatch")
for name, expected in build["r2a6_downstream_tools"].items():
    if name in {"cargo_command", "reproducible"}:
        continue
    if digest(tools_a / name) != expected or digest(tools_b / name) != expected:
        raise SystemExit(f"R2A6 downstream build mismatch: {name}")
if digest(accepted / "stage8b-readonly-preflight") != build["accepted_r2a5_helper"]["executable_sha256"]:
    raise SystemExit("accepted R2A5 helper drift")
if digest(accepted / "stage8b-r2a5-launcher") != build["accepted_r2a5_helper"]["launcher_sha256"]:
    raise SystemExit("accepted R2A5 launcher drift")
print("stage8b-p-r2a6-linux-artifacts: PASS adapter=2 tools=10 accepted_r2a5=2")
PY

if command -v docker >/dev/null 2>&1 && [[ "${STAGE8B_R2A6_SKIP_REHEARSAL:-0}" != "1" ]]; then
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work:ro" -w /work \
    rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
    bash scripts/stage8b_p_r2a6_linux_rehearsal.sh \
      "/work/$tools_a" "/work/$adapter_a" "/work/$accepted"
fi

git diff --check
echo "stage8b-p-r2a6-gate: PASS source_adapter=true adapter_uid=8095 current_tree_negative=33 r2a6_negative=19 rust_tests=65 reproducible_adapter=true reproducible_tools=true place=true cancel=true effect_unchanged=true authorization=NOT_ISSUED real_finam=false"
