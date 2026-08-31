#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2a6-rehearsal: must run as root" >&2
  exit 1
fi

TOOL_BIN_DIR="${1:-/work/tmp/stage8b-r2a6-tool-build-a/release}"
ADAPTER_BIN="${2:-/work/target/release/stage8b-r2a6-source-adapter}"
ACCEPTED_R2A5_BIN_DIR="${3:-/work/tmp/stage8b-r2a5-build-a/release}"
HELPER="$ACCEPTED_R2A5_BIN_DIR/stage8b-readonly-preflight"
PRODUCER="$TOOL_BIN_DIR/stage8b-r2a5-authority-producer"
ISSUER="$TOOL_BIN_DIR/stage8b-r2a5-authority-issuer"
PACKAGE_ISSUER="$TOOL_BIN_DIR/stage8b-r2a5-package-issuer"
LAYOUT="$TOOL_BIN_DIR/stage8b-r2a5-controlled-layout"
SERVER="$TOOL_BIN_DIR/stage8b-r2a5-controlled-server"
LAUNCHER="$ACCEPTED_R2A5_BIN_DIR/stage8b-r2a5-launcher"

for binary in "$ADAPTER_BIN" "$HELPER" "$PRODUCER" "$ISSUER" "$PACKAGE_ISSUER" "$LAYOUT" "$SERVER" "$LAUNCHER"; do
  test -x "$binary"
done

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
declare -A ISSUER_UID=(
  [trusted_clock]=8201 [stage7b_current_recovery_seal]=8202
  [stage6_exact_dispatch_ready_command]=8203 [stage8a_root_config_policy_control]=8204
  [composite_readiness]=8205 [kill_switch_run_allowed]=8206
  [single_finam_ownership]=8207 [schedule]=8208 [instrument_specification]=8209
  [ambiguity_orphan_unresolved_lifecycle]=8210 [durable_micro_budget]=8211
)

install -d -m 0755 /opt/moex-trading/stage8b-r2a5/bin
install -m 0755 "$HELPER" /opt/moex-trading/stage8b-r2a5/bin/stage8b-readonly-preflight
helper_sha256="$(sha256sum "$HELPER" | awk '{print $1}')"

for operation in PLACE CANCEL; do
  rm -rf /etc/moex-trading/stage8b/r2a5 \
    /var/lib/moex-trading/stage8b/r2a5 \
    /var/lib/moex-trading/stage8b/r2a6 \
    /var/lib/moex-trading/operational-authorities \
    /run/moex-trading/stage8b/r2a5 \
    /run/credentials/moex-trading/stage8b/r2a5
  "$LAYOUT" seed-r2a6 "$operation"

  test "$(stat -c %u /var/lib/moex-trading/operational-authorities)" = "8095"
  test "$(stat -c %a /var/lib/moex-trading/operational-authorities)" = "755"
  test -z "$(find /var/lib/moex-trading/operational-authorities -mindepth 1 -print -quit)"
  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$ADAPTER_BIN" --controlled-rehearsal "$operation" \
    >"/tmp/stage8b-r2a6-adapter-${operation,,}.json"
  grep -Fq '"source_count":10' "/tmp/stage8b-r2a6-adapter-${operation,,}.json"
  grep -Fq '"execution_authority_granted":false' "/tmp/stage8b-r2a6-adapter-${operation,,}.json"
  "$LAYOUT" bind-r2a6
  test "$(find /var/lib/moex-trading/operational-authorities -maxdepth 1 -type f | wc -l | tr -d ' ')" = "10"
  while IFS= read -r source_file; do
    test "$(stat -c %u "$source_file")" = "8095"
    test "$(stat -c %a "$source_file")" = "644"
    test "$(stat -c %h "$source_file")" = "1"
  done < <(find /var/lib/moex-trading/operational-authorities -maxdepth 1 -type f -print)

  pids=()
  for source in "${SOURCES[@]}"; do
    (
      if ! setpriv --reuid "${PRODUCER_UID[$source]}" --regid "${PRODUCER_UID[$source]}" \
        --clear-groups "$PRODUCER" "$source"; then
        echo "stage8b-r2a6-producer-$source: FAIL" >&2
        exit 1
      fi
      if ! setpriv --reuid "${ISSUER_UID[$source]}" --regid "${ISSUER_UID[$source]}" \
        --clear-groups "$ISSUER" "$source"; then
        echo "stage8b-r2a6-issuer-$source: FAIL" >&2
        exit 1
      fi
    ) &
    pids+=("$!")
  done
  for pid in "${pids[@]}"; do wait "$pid"; done

  "$LAYOUT" finalize "$helper_sha256"
  "$PACKAGE_ISSUER"
  "$SERVER" "$operation" &
  server_pid=$!
  for _ in $(seq 1 100); do
    test -s /run/moex-trading/stage8b/r2a5/controlled-endpoint.txt && break
    sleep 0.05
  done
  "$LAUNCHER" --controlled-fixed-layout >"/tmp/stage8b-r2a6-${operation,,}-evidence.json"
  wait "$server_pid"
  grep -Fq "\"operation\":\"$operation\"" "/tmp/stage8b-r2a6-${operation,,}-evidence.json"
  grep -Fq '"authorization_status":"ISSUED"' "/tmp/stage8b-r2a6-${operation,,}-evidence.json"
  echo "stage8b-r2a6-fixed-layout-$operation: PASS adapter_uid=8095"
done

echo "stage8b-r2a6-linux-rehearsal: PASS adapter=true place=true cancel=true real_finam=false"
