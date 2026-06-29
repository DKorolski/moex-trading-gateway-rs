#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive_dir="$repo_root/reports/handoff"
mkdir -p "$archive_dir"

head_sha="$(git -C "$repo_root" rev-parse --short HEAD)"
archive_path="$archive_dir/moex-trading-project-${head_sha}.zip"

(
  cd "$repo_root"
  zip -qr "$archive_path" . \
    -x '.git/*' \
    -x 'target/*' \
    -x 'tmp/*' \
    -x 'reports/*' \
    -x '.env' \
    -x '.env.*' \
    -x '*.log' \
    -x '*.local.*' \
    -x '.DS_Store'
)

echo "$archive_path"
