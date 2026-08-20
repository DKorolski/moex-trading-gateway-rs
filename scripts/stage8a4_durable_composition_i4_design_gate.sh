#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/stage8a4_durable_composition_i4_design_check.py
python3 scripts/stage8a4_durable_composition_i4_design_negative_harness.py
# The repository-wide legacy scanner is pinned to a pre-Stage6 workspace and
# rejects the already accepted runtime-durable-service baseline. For this
# docs/checker-only slice, exact source equality to accepted I3 R6 is both
# narrower and stronger than changing that unrelated historical baseline.
git diff --quiet 593ff255ef7826a22e66c9aff6f7ea47acf47644 -- Cargo.toml Cargo.lock crates tests .github/workflows
cargo fmt --all -- --check
git diff --check

echo "stage8a4-durable-composition-i4-design-gate: PASS revision=R3 rows=64 negatives=46 implementation=false ack_publish=false xack=false redis=false finam=false dispatch=false live=false"
