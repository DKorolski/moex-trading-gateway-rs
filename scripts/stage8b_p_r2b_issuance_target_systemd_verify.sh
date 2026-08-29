#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

command -v systemd-analyze >/dev/null 2>&1 || {
  echo "stage8b-p-r2b-issuance-target-systemd-verify: FAIL systemd-analyze unavailable" >&2
  exit 1
}

roots=(
  /opt/moex-trading/stage8b-r2b/bin
  /opt/moex-trading/stage8b-r2a8/bin
  /opt/moex-trading/stage8b-r2a7/bin
  /opt/moex-trading/stage8b-r2a5/bin
)
for root in "${roots[@]}"; do
  if [[ -e "$root" ]]; then
    echo "stage8b-p-r2b-issuance-target-systemd-verify: FAIL refusing existing $root" >&2
    exit 1
  fi
done

cleanup() {
  rm -rf \
    /opt/moex-trading/stage8b-r2b \
    /opt/moex-trading/stage8b-r2a8 \
    /opt/moex-trading/stage8b-r2a7 \
    /opt/moex-trading/stage8b-r2a5
}
trap cleanup EXIT

for root in "${roots[@]}"; do mkdir -p "$root"; done
for name in \
  stage8b-r2a8-upstream-current-authority-publisher \
  stage8b-r2a8-authoritative-intake-creator \
  stage8b-r2a8-production-intake-stager \
  stage8b-r2a8-production-current-source-writer \
  stage8b-r2b-launcher
do
  install -m 0755 /bin/true "/opt/moex-trading/stage8b-r2b/bin/$name"
done
install -m 0755 /bin/true /opt/moex-trading/stage8b-r2a8/bin/stage8b-r2a8-current-manifest-issuer
install -m 0755 /bin/true /opt/moex-trading/stage8b-r2a7/bin/stage8b-r2a7-source-adapter
install -m 0755 /bin/true /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-producer
install -m 0755 /bin/true /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-issuer

python3 scripts/stage8b_p_r2b_issuance_systemd_check.py --systemd-analyze
version="$(systemd-analyze --version | head -1)"
echo "stage8b-p-r2b-issuance-target-systemd-verify: PASS version=${version#systemd } units=9 parser_warnings=0 services_started=0"
