#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive_dir="$repo_root/reports/handoff"
mkdir -p "$archive_dir"

head_sha="$(git -C "$repo_root" rev-parse --short HEAD)"
archive_path="$archive_dir/moex-trading-project-${head_sha}.zip"

scan_output="$(mktemp)"
commit_marker="$repo_root/handoff-commit.txt"
trap 'rm -f "$scan_output" "$commit_marker"' EXIT

legacy_portfolio_prefix="75""02"
legacy_account_id="190""9892"
legacy_client_code="63""170"
forbidden_regex="(${legacy_portfolio_prefix}[A-Z0-9]*|${legacy_account_id}|${legacy_client_code}[A-Z0-9/]*|tapi_[sa]k_[A-Za-z0-9_-]+|eyJ[A-Za-z0-9_-]{20,}\\.[A-Za-z0-9_-]{20,}\\.[A-Za-z0-9_-]{10,})"

if (
  cd "$repo_root"
  find . \
    -type f \
    ! -path './.git/*' \
    ! -path './target/*' \
    ! -path './tmp/*' \
    ! -path './reports/*' \
    ! -name '.env' \
    ! -name '.env.*' \
    ! -name '*.log' \
    ! -name '*.local.*' \
    ! -name '.DS_Store' \
    -print0 |
    xargs -0 grep -I -E -n "$forbidden_regex"
) >"$scan_output"; then
  echo "Refusing to build handoff archive: forbidden live-like literal(s) found." >&2
  cat "$scan_output" >&2
  exit 1
fi

cat >"$commit_marker" <<EOF
source_commit=$head_sha
source_ref=$(git -C "$repo_root" rev-parse HEAD)
archive_name=$(basename "$archive_path")
EOF

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
