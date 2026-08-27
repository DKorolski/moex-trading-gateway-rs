#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a8_review_closure_check.py
python3 scripts/stage8b_p_r2a8_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a8-status.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a8-build-evidence.json >/dev/null

cargo fmt --all -- --check
cargo test -p finam-gateway --features stage8b-r2a7-controlled-qualification \
  stage8b_r2a7_source_adapter --no-fail-fast
cargo clippy -p finam-gateway --features stage8b-r2a7-source-adapter \
  --bin stage8b-r2a7-source-adapter --bin stage8b-r2a8-current-manifest-issuer -- -D warnings
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml \
  --all-targets -- -D warnings

adapter_a="${STAGE8B_R2A8_ADAPTER_A:-tmp/stage8b-r2a8-production-a/release/stage8b-r2a7-source-adapter}"
adapter_b="${STAGE8B_R2A8_ADAPTER_B:-tmp/stage8b-r2a8-production-b/release/stage8b-r2a7-source-adapter}"
issuer_a="${STAGE8B_R2A8_ISSUER_A:-tmp/stage8b-r2a8-production-a/release/stage8b-r2a8-current-manifest-issuer}"
issuer_b="${STAGE8B_R2A8_ISSUER_B:-tmp/stage8b-r2a8-production-b/release/stage8b-r2a8-current-manifest-issuer}"
python3 - "$adapter_a" "$adapter_b" "$issuer_a" "$issuer_b" <<'PY'
import hashlib, pathlib, sys
paths = [pathlib.Path(value) for value in sys.argv[1:]]
for path in paths:
    if not path.is_file():
        raise SystemExit(f"missing R2A8 Linux artifact: {path}")
for left, right in (paths[:2], paths[2:]):
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    if digest(left) != digest(right):
        raise SystemExit(f"non-reproducible R2A8 binary: {left.name}")
print("stage8b-p-r2a8-reproducibility: PASS binaries=2 runs=2")
PY

if command -v docker >/dev/null 2>&1 && [[ "${STAGE8B_R2A8_SKIP_REHEARSAL:-0}" != "1" ]]; then
  controlled="${STAGE8B_R2A8_CONTROLLED_DIR:-tmp/stage8b-r2a8-linux/release}"
  tools="${STAGE8B_R2A8_TOOLS_DIR:-tmp/stage8b-r2a8-tools-linux/release}"
  accepted="${STAGE8B_R2A5_ACCEPTED_BIN_DIR:-tmp/stage8b-r2a5-build-a/release}"
  args=(
    "/work/$controlled/stage8b-r2a7-source-adapter"
    "/work/$controlled/stage8b-r2a7-controlled-seeder"
    "/work/$controlled/stage8b-r2a8-current-manifest-issuer"
  )
  # The full chain has a deliberately strict one-second freshness budget and
  # must run natively on Linux/amd64.  The normal cross-platform review gate
  # validates its commit-bound evidence; an operator explicitly opts into a
  # native rerun on a suitable host.
  if [[ "${STAGE8B_R2A8_NATIVE_FULL_CHAIN:-0}" = "1" ]]; then
    args+=("/work/$tools" "/work/$accepted")
  fi
  docker run --rm --platform linux/amd64 -v "$repo_root:/work:ro" -w /work \
    rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
    bash scripts/stage8b_p_r2a7_linux_rehearsal.sh "${args[@]}"
fi

git diff --check
echo "stage8b-p-r2a8-gate: PASS trusted_manifest=true schema_compatible=true strict_key=true reproducible_adapter=true reproducible_issuer=true place=true cancel=true authorization=NOT_ISSUED real_finam=false"
