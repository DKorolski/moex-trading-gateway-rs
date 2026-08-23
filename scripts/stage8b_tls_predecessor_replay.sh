#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accepted_ref="14e01a9f838080e196ece5945a7796f2bd2600bc"
accepted_adapter_sha256="087856c8e170ddf318a124453987f7e5d85052acde3a260dd53eaed479e4cf87"
replay_root="$repo_root/tmp/stage8b-tls-predecessor-replay"

rm -rf "$replay_root"
git -C "$repo_root" cat-file -e "${accepted_ref}^{commit}"
git clone --quiet --no-hardlinks --shared "$repo_root" "$replay_root"
git -C "$replay_root" checkout --quiet --detach "$accepted_ref"

actual_adapter_sha256="$(shasum -a 256 "$replay_root/crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs" | awk '{print $1}')"
test "$actual_adapter_sha256" = "$accepted_adapter_sha256"

(
  cd "$replay_root"
  python3 scripts/stage8b_it_check.py --no-git
  python3 scripts/stage8b_it_negative_harness.py
  bash scripts/stage8b_it_external_compile_fail.sh
  bash scripts/stage8b_it_internal_compile_fail.sh
)

rm -rf "$replay_root"
echo "stage8b-tls-predecessor-replay: PASS accepted_ref=$accepted_ref adapter_sha256=$accepted_adapter_sha256"
