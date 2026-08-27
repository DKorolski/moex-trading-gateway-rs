#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2a7-rehearsal: must run as root" >&2
  exit 1
fi

ADAPTER_BIN="${1:-/work/target/release/stage8b-r2a7-source-adapter}"
SEEDER_BIN="${2:-/work/target/release/stage8b-r2a7-controlled-seeder}"
ISSUER_BIN="${3:-/work/target/release/stage8b-r2a8-current-manifest-issuer}"
TOOL_BIN_DIR="${4:-}"
ACCEPTED_BIN_DIR="${5:-}"
test -x "$ADAPTER_BIN"
test -x "$SEEDER_BIN"
test -x "$ISSUER_BIN"

if [[ -n "$TOOL_BIN_DIR" || -n "$ACCEPTED_BIN_DIR" ]]; then
  test -n "$TOOL_BIN_DIR"
  test -n "$ACCEPTED_BIN_DIR"
  PRODUCER="$TOOL_BIN_DIR/stage8b-r2a5-authority-producer"
  SOURCE_ISSUER="$TOOL_BIN_DIR/stage8b-r2a5-authority-issuer"
  PACKAGE_ISSUER="$TOOL_BIN_DIR/stage8b-r2a5-package-issuer"
  LAYOUT="$TOOL_BIN_DIR/stage8b-r2a5-controlled-layout"
  SERVER="$TOOL_BIN_DIR/stage8b-r2a5-controlled-server"
  HELPER="$ACCEPTED_BIN_DIR/stage8b-readonly-preflight"
  LAUNCHER="$ACCEPTED_BIN_DIR/stage8b-r2a5-launcher"
  for binary in "$PRODUCER" "$SOURCE_ISSUER" "$PACKAGE_ISSUER" "$LAYOUT" "$SERVER" "$HELPER" "$LAUNCHER"; do
    test -x "$binary"
  done
fi

SOURCES=(
  ambiguity_orphan_unresolved_lifecycle composite_readiness durable_micro_budget
  instrument_specification kill_switch_run_allowed schedule single_finam_ownership
  stage6_exact_dispatch_ready_command stage7b_current_recovery_seal
  stage8a_root_config_policy_control trusted_clock
)
declare -A PRODUCER_UID=(
  [trusted_clock]=8101 [stage7b_current_recovery_seal]=8102
  [stage6_exact_dispatch_ready_command]=8103 [stage8a_root_config_policy_control]=8104
  [composite_readiness]=8105 [kill_switch_run_allowed]=8106
  [single_finam_ownership]=8107 [schedule]=8108 [instrument_specification]=8109
  [ambiguity_orphan_unresolved_lifecycle]=8110 [durable_micro_budget]=8111
)
declare -A SOURCE_ISSUER_UID=(
  [trusted_clock]=8201 [stage7b_current_recovery_seal]=8202
  [stage6_exact_dispatch_ready_command]=8203 [stage8a_root_config_policy_control]=8204
  [composite_readiness]=8205 [kill_switch_run_allowed]=8206
  [single_finam_ownership]=8207 [schedule]=8208 [instrument_specification]=8209
  [ambiguity_orphan_unresolved_lifecycle]=8210 [durable_micro_budget]=8211
)

for operation in place cancel; do
  operation_upper="${operation^^}"
  if [[ -n "$TOOL_BIN_DIR" ]]; then
    rm -rf /etc/moex-trading/stage8b/r2a5 \
      /var/lib/moex-trading/stage8b/r2a5 \
      /var/lib/moex-trading/stage8b/r2a6 \
      /var/lib/moex-trading/operational-authorities \
      /run/moex-trading/stage8b/r2a5 \
      /run/credentials/moex-trading/stage8b/r2a5
    "$LAYOUT" seed-r2a6 "$operation_upper"
  fi
  base="/var/lib/moex-trading/stage8b/r2a7-controlled/$operation"
  rm -rf /var/lib/moex-trading/stage8b/r2a7-controlled
  install -d -o 0 -g 0 -m 0755 /var/lib/moex-trading/stage8b/r2a7-controlled
  install -d -o 0 -g 0 -m 0755 "$base"
  install -d -o 8095 -g 8095 -m 0700 "$base/stage7b"
  install -d -o 8095 -g 8095 -m 0700 "$base/stage8a1-authority"
  install -d -o 8095 -g 8095 -m 0755 "$base/current-source"
  install -d -o 8096 -g 8096 -m 0755 "$base/manifest"
  printf '%s\n' "$(printf '5a%.0s' {1..32})" \
    | install -o 8096 -g 8095 -m 0640 /dev/stdin "$base/manifest/stage8b-r2a7-lifecycle-key.hex"
  install -d -o 8095 -g 8095 -m 0755 "$base/operational-authorities"

  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$SEEDER_BIN" "--seed-controlled-$operation"
  test -z "$(find "$base/operational-authorities" -mindepth 1 -print -quit)"
  test -s "$base/current-source/stage8b-r2a8-trusted-current-source.json"
  test ! -e "$base/manifest/stage8b-r2a7-reader-manifest.json"
  setpriv --reuid 8096 --regid 8096 --clear-groups \
    "$ISSUER_BIN" "--one-shot-controlled-$operation"
  test -s "$base/manifest/stage8b-r2a7-reader-manifest.json"
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

  if [[ -n "$TOOL_BIN_DIR" ]]; then
    "$LAYOUT" bind-r2a8 "$operation_upper"
    pids=()
    for source in "${SOURCES[@]}"; do
      (
        if ! setpriv --reuid "${PRODUCER_UID[$source]}" --regid "${PRODUCER_UID[$source]}" \
          --clear-groups "$PRODUCER" "--controlled-r2a8-$operation" "$source"; then
          echo "stage8b-r2a8-producer-$source: FAIL" >&2
          exit 1
        fi
        if ! setpriv --reuid "${SOURCE_ISSUER_UID[$source]}" --regid "${SOURCE_ISSUER_UID[$source]}" \
          --clear-groups "$SOURCE_ISSUER" "$source"; then
          echo "stage8b-r2a8-issuer-$source: FAIL" >&2
          exit 1
        fi
      ) &
      pids+=("$!")
    done
    for pid in "${pids[@]}"; do wait "$pid"; done
    helper_sha256="$(sha256sum "$HELPER" | awk '{print $1}')"
    install -d -m 0755 /opt/moex-trading/stage8b-r2a5/bin
    install -m 0755 "$HELPER" /opt/moex-trading/stage8b-r2a5/bin/stage8b-readonly-preflight
    "$LAYOUT" finalize "$helper_sha256"
    "$PACKAGE_ISSUER"
    "$SERVER" "$operation_upper" &
    server_pid=$!
    for _ in $(seq 1 100); do
      test -s /run/moex-trading/stage8b/r2a5/controlled-endpoint.txt && break
      sleep 0.05
    done
    "$LAUNCHER" --controlled-fixed-layout >"/tmp/stage8b-r2a8-$operation-evidence.json"
    wait "$server_pid"
    grep -Fq "\"operation\":\"$operation_upper\"" "/tmp/stage8b-r2a8-$operation-evidence.json"
    grep -Fq '"authorization_status":"ISSUED"' "/tmp/stage8b-r2a8-$operation-evidence.json"
    echo "stage8b-r2a8-full-chain-$operation: PASS"
  fi
  echo "stage8b-r2a7-controlled-$operation: PASS"
done

echo "stage8b-r2a7-linux-rehearsal: PASS exact_reader=true place=true cancel=true real_finam=false"
