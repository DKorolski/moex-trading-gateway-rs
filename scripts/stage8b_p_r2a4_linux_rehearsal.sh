#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2a4-rehearsal: must run as root" >&2
  exit 1
fi

BIN_DIR="${1:-/work/tools/stage8b-readonly-preflight/target/release}"
HELPER="$BIN_DIR/stage8b-readonly-preflight"
PRODUCER="$BIN_DIR/stage8b-r2a4-authority-producer"
ISSUER="$BIN_DIR/stage8b-r2a4-authority-issuer"
PACKAGE_ISSUER="$BIN_DIR/stage8b-r2a4-package-issuer"
LAYOUT="$BIN_DIR/stage8b-r2a4-controlled-layout"
SERVER="$BIN_DIR/stage8b-r2a4-controlled-server"
LAUNCHER="$BIN_DIR/stage8b-r2a4-launcher"

for binary in "$HELPER" "$PRODUCER" "$ISSUER" "$PACKAGE_ISSUER" "$LAYOUT" "$SERVER" "$LAUNCHER"; do
  test -x "$binary"
done

SOURCES=(
  ambiguity_orphan_unresolved_lifecycle
  composite_readiness
  durable_micro_budget
  instrument_specification
  kill_switch_run_allowed
  schedule
  single_finam_ownership
  stage6_exact_dispatch_ready_command
  stage7b_current_recovery_seal
  stage8a_root_config_policy_control
  trusted_clock
)

declare -A PRODUCER_UID=(
  [trusted_clock]=8101
  [stage7b_current_recovery_seal]=8102
  [stage6_exact_dispatch_ready_command]=8103
  [stage8a_root_config_policy_control]=8104
  [composite_readiness]=8105
  [kill_switch_run_allowed]=8106
  [single_finam_ownership]=8107
  [schedule]=8108
  [instrument_specification]=8109
  [ambiguity_orphan_unresolved_lifecycle]=8110
  [durable_micro_budget]=8111
)

declare -A ISSUER_UID=(
  [trusted_clock]=8201
  [stage7b_current_recovery_seal]=8202
  [stage6_exact_dispatch_ready_command]=8203
  [stage8a_root_config_policy_control]=8204
  [composite_readiness]=8205
  [kill_switch_run_allowed]=8206
  [single_finam_ownership]=8207
  [schedule]=8208
  [instrument_specification]=8209
  [ambiguity_orphan_unresolved_lifecycle]=8210
  [durable_micro_budget]=8211
)

install -d -m 0755 /opt/moex-trading/stage8b-r2a4/bin
install -m 0755 "$HELPER" /opt/moex-trading/stage8b-r2a4/bin/stage8b-readonly-preflight
helper_sha256="$(sha256sum "$HELPER" | awk '{print $1}')"

for operation in PLACE CANCEL; do
  rm -rf /etc/moex-trading/stage8b/r2a4 \
    /var/lib/moex-trading/stage8b/r2a4 \
    /run/moex-trading/stage8b/r2a4 \
    /run/credentials/moex-trading/stage8b/r2a4
  "$LAYOUT" seed "$operation"

  pids=()
  for source in "${SOURCES[@]}"; do
    (
      setpriv --reuid "${PRODUCER_UID[$source]}" --regid "${PRODUCER_UID[$source]}" \
        --clear-groups "$PRODUCER" "$source"
      setpriv --reuid "${ISSUER_UID[$source]}" --regid "${ISSUER_UID[$source]}" \
        --clear-groups "$ISSUER" "$source"
    ) &
    pids+=("$!")
  done
  for pid in "${pids[@]}"; do
    wait "$pid"
  done

  "$LAYOUT" finalize "$helper_sha256"
  "$PACKAGE_ISSUER"
  "$SERVER" "$operation" &
  server_pid=$!
  for _ in $(seq 1 100); do
    test -s /run/moex-trading/stage8b/r2a4/controlled-endpoint.txt && break
    sleep 0.05
  done
  "$LAUNCHER" --controlled-fixed-layout \
    >"/tmp/stage8b-r2a4-${operation,,}-evidence.json"
  wait "$server_pid"
  grep -Fq "\"operation\":\"$operation\"" "/tmp/stage8b-r2a4-${operation,,}-evidence.json"
  grep -Fq '"authorization_status":"ISSUED"' "/tmp/stage8b-r2a4-${operation,,}-evidence.json"
  echo "stage8b-r2a4-fixed-layout-$operation: PASS"
done

echo "stage8b-r2a4-linux-rehearsal: PASS"
