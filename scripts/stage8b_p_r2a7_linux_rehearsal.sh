#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2a7-rehearsal: must run as root" >&2
  exit 1
fi

ADAPTER_BIN="${1:-/work/target/release/stage8b-r2a7-source-adapter}"
SEEDER_BIN="${2:-/work/target/release/stage8b-r2a7-controlled-seeder}"
test -x "$ADAPTER_BIN"
test -x "$SEEDER_BIN"

for operation in place cancel; do
  base="/var/lib/moex-trading/stage8b/r2a7-controlled/$operation"
  rm -rf /var/lib/moex-trading/stage8b/r2a7-controlled
  install -d -o 0 -g 0 -m 0755 /var/lib/moex-trading/stage8b/r2a7-controlled "$base"
  install -d -o 8095 -g 8095 -m 0700 "$base/input" "$base/input/stage7b"
  install -d -o 8095 -g 8095 -m 0755 "$base/operational-authorities"

  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$SEEDER_BIN" "--seed-controlled-$operation"
  test -z "$(find "$base/operational-authorities" -mindepth 1 -print -quit)"
  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$ADAPTER_BIN" "--one-shot-controlled-$operation" \
    >"/tmp/stage8b-r2a7-$operation.json"
  grep -Fq '"adapter_domain":"controlled_qualification"' "/tmp/stage8b-r2a7-$operation.json"
  grep -Fq '"source_count":10' "/tmp/stage8b-r2a7-$operation.json"
  grep -Fq '"execution_authority_granted":false' "/tmp/stage8b-r2a7-$operation.json"
  grep -Fq '"network_accessed":false' "/tmp/stage8b-r2a7-$operation.json"
  grep -Fq '"finam_credential_accessed":false' "/tmp/stage8b-r2a7-$operation.json"
  test "$(find "$base/operational-authorities" -maxdepth 1 -type f | wc -l | tr -d ' ')" = "10"
  while IFS= read -r source_file; do
    test "$(stat -c %u "$source_file")" = "8095"
    test "$(stat -c %a "$source_file")" = "644"
    test "$(stat -c %h "$source_file")" = "1"
    grep -Fq '"adapter_domain":"controlled_qualification"' "$source_file"
    grep -Fq '"adapter_mode":"one_shot_recovery_reader"' "$source_file"
    if grep -Fq '"adapter_domain":"production"' "$source_file"; then
      echo "stage8b-r2a7-rehearsal: controlled record forged production provenance" >&2
      exit 1
    fi
  done < <(find "$base/operational-authorities" -maxdepth 1 -type f -print)
  echo "stage8b-r2a7-controlled-$operation: PASS"
done

echo "stage8b-r2a7-linux-rehearsal: PASS exact_reader=true place=true cancel=true real_finam=false"
