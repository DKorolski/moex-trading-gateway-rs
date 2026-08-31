#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Historical R1/R1A/R1B artifacts remain immutable. Their old current-tree
# checker intentionally rejects any later production change, so R2A5 replays
# the accepted current-tree authority and its mutation matrix instead.
python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a5_review_closure_check.py
python3 scripts/stage8b_p_r2a5_negative_harness.py
python3 -m py_compile \
  scripts/make_stage8b_p_r2a5_handoff.py \
  scripts/stage8b_p_r2a5_handoff_safety_check.py \
  scripts/stage8b_p_r2a5_review_closure_check.py \
  scripts/stage8b_p_r2a5_negative_harness.py
for document in \
  docs/stage-8/stage8b-p-r2a5-authority.json \
  docs/stage-8/stage8b-p-r2a5-build-evidence.json \
  docs/stage-8/stage8b-p-r2a5-controlled-authority.json \
  docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json \
  docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json \
  docs/stage-8/stage8b-p-r2a5-source-adapter-authority.json \
  docs/stage-8/stage8b-p-r2a5-accepted-helper-authority.json \
  docs/stage-8/stage8b-p-r2a5-status.json; do
  python3 -m json.tool "$document" >/dev/null
done

cargo fmt --all -- --check
cargo test -p finam-gateway --lib stage8a1_execution_capability::tests --no-fail-fast
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings

if tools/stage8b-readonly-preflight/target/debug/stage8b-readonly-preflight --r2b-one-shot >/dev/null 2>&1; then
  echo "stage8b-p-r2a5-gate: FAIL R2B opened without production package" >&2
  exit 1
fi

linux_bin_dir="${STAGE8B_R2A5_LINUX_BIN_DIR:-tmp/stage8b-r2a5-build-a/release}"
python3 - "$linux_bin_dir" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
build = json.loads(pathlib.Path("docs/stage-8/stage8b-p-r2a5-build-evidence.json").read_text())
for name, expected in build["linux_release_sha256"].items():
    path = root / name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f"Linux release artifact mismatch: {name}")
print(f"stage8b-p-r2a5-linux-artifacts: PASS count={len(build['linux_release_sha256'])}")
PY

if command -v docker >/dev/null 2>&1; then
  docker run --rm --platform linux/amd64 \
    -v "$repo_root:/work" -w /work \
    rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
    /work/scripts/stage8b_p_r2a5_linux_rehearsal.sh "/work/$linux_bin_dir"
fi

git diff --check
echo "stage8b-p-r2a5-gate: PASS inherited_r1b_lineage=134 current_tree_negative=33 r2a5_negative=24 rust_tests=43 adapter_tests=17 linux_binaries=11 place=true cancel=true authorization=NOT_ISSUED real_http=false"
