#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output="${1:-$repo_root/tmp/m4-3x-evidence-no-redis.json}"
mkdir -p "$(dirname "$output")"

python3 "$repo_root/scripts/m4_3x_runtime_state_parity_evidence.py" \
  --finam-redis-cli-prefix "missing-redis-cli-for-stage-1b-test" \
  --alor-redis-cli-prefix "missing-redis-cli-for-stage-1b-test" \
  --source-commit "STAGE1B_NO_REDIS_SMOKE" \
  --output "$output" >/dev/null

python3 - "$output" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text())
assert report["comparison"]["status"] == "EvidenceIncomplete", report["comparison"]["status"]
assert report["raw_payload_exported"] is False
assert report["finam"]["read_error"] is not None
assert report["alor"]["read_error"] is not None
PY
