#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

command -v systemd-analyze >/dev/null 2>&1 || {
  echo "stage8b-p-r2b-target-systemd-verify: FAIL systemd-analyze unavailable" >&2
  exit 1
}

bin_root=/opt/moex-trading/stage8b-r2b/bin
if [[ -e "$bin_root" ]]; then
  echo "stage8b-p-r2b-target-systemd-verify: FAIL refusing existing $bin_root" >&2
  exit 1
fi

cleanup() {
  rm -rf /opt/moex-trading/stage8b-r2b
}
trap cleanup EXIT

mkdir -p "$bin_root"
for name in \
  stage8b-r2a8-upstream-current-authority-publisher \
  stage8b-r2a8-authoritative-intake-creator \
  stage8b-r2a8-production-intake-stager \
  stage8b-r2a8-production-current-source-writer
do
  install -m 0755 /bin/true "$bin_root/$name"
done

python3 scripts/stage8b_p_r2b_systemd_unit_check.py --systemd-analyze
version="$(systemd-analyze --version | head -1)"
echo "stage8b-p-r2b-target-systemd-verify: PASS version=${version#systemd } units=4 parser_warnings=0 services_started=0"
