#!/usr/bin/env bash
set -euo pipefail
# Compatibility entrypoint: current Stage 6A authority is the R2 candidate.
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/stage6a_r2_gate.sh" "$@"
