#!/usr/bin/env bash
set -euo pipefail

# Candidate digest becomes launch authority only after independent R2A2 review.
readonly helper="/opt/moex-trading/stage8b-r2a2/bin/stage8b-readonly-preflight"
readonly accepted_sha256="0c6dcde920de131863fe12632b0e3092f30fedc796e4627873cea89b6aace363"

[[ -f "$helper" && ! -L "$helper" ]] || {
  echo "stage8b-r2a2-launch: helper is not a regular non-symlink file" >&2
  exit 1
}
actual_sha256="$(sha256sum "$helper" | awk '{print $1}')"
[[ "$actual_sha256" == "$accepted_sha256" ]] || {
  echo "stage8b-r2a2-launch: exact accepted helper digest mismatch" >&2
  exit 1
}
exec "$helper"
