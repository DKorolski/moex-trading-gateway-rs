#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2b-r3-custody: must run as root" >&2
  exit 1
fi

BIN_DIR="${1:-/work/tools/stage8b-readonly-preflight/target/release}"
CONTROLLED_BIN_DIR="${2:-/work/target/release}"
CONTROLLED_LAUNCHER="${3:-$BIN_DIR/stage8b-r2b-launcher-controlled-custody}"
HELPER="$BIN_DIR/stage8b-readonly-preflight"
LAUNCHER="$BIN_DIR/stage8b-r2b-launcher"
PRODUCER="$BIN_DIR/stage8b-r2a5-authority-producer"
ISSUER="$BIN_DIR/stage8b-r2a5-authority-issuer"
PACKAGE_ISSUER="$BIN_DIR/stage8b-r2a5-package-issuer"
LAYOUT="$BIN_DIR/stage8b-r2a5-controlled-layout"
SERVER="$BIN_DIR/stage8b-r2a5-controlled-server"
ADAPTER="$CONTROLLED_BIN_DIR/stage8b-r2a7-source-adapter"
SEEDER="$CONTROLLED_BIN_DIR/stage8b-r2a7-controlled-seeder"
MANIFEST_ISSUER="$CONTROLLED_BIN_DIR/stage8b-r2a8-current-manifest-issuer"

for binary in "$HELPER" "$LAUNCHER" "$CONTROLLED_LAUNCHER" "$PRODUCER" "$ISSUER" "$PACKAGE_ISSUER" "$LAYOUT" "$SERVER" "$ADAPTER" "$SEEDER" "$MANIFEST_ISSUER"; do
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

install -d -o 0 -g 0 -m 0755 /opt/moex-trading/stage8b-r2b/bin
install -o 0 -g 0 -m 0755 "$HELPER" \
  /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
helper_sha256="$(sha256sum "$HELPER" | awk '{print $1}')"
accepted_sha256="$(tr -d '\r\n' </work/docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt)"
test "$helper_sha256" = "$accepted_sha256"
rm -rf /var/lib/moex-trading/stage8b/r2a5

# A wrong helper is rejected before package validation or irreversible nonce
# admission. This is the executable proof of the open-once hash preflight.
cp /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight \
  /tmp/stage8b-r2b-accepted-helper
printf '\0' >>/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
if "$LAUNCHER" >/tmp/stage8b-r2b-wrong-helper.log 2>&1; then
  echo "stage8b-r2b-r3-custody: wrong helper was accepted" >&2
  exit 1
fi
test ! -e /var/lib/moex-trading/stage8b/r2a5/admissions
install -o 0 -g 0 -m 0755 /tmp/stage8b-r2b-accepted-helper \
  /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight

# Privileged helper metadata is rejected before nonce admission.
for privileged_mode in 4755 2755; do
  chmod "$privileged_mode" /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
  if "$LAUNCHER" >/tmp/stage8b-r2b-privileged-helper.log 2>&1; then
    echo "stage8b-r2b-r3-custody: privileged helper mode accepted" >&2
    exit 1
  fi
  test ! -e /var/lib/moex-trading/stage8b/r2a5/admissions
  chmod 0755 /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
done
if command -v setcap >/dev/null 2>&1; then
  setcap cap_net_raw+ep /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
  if "$LAUNCHER" >/tmp/stage8b-r2b-capability-helper.log 2>&1; then
    echo "stage8b-r2b-r3-custody: file-capability helper accepted" >&2
    exit 1
  fi
  test ! -e /var/lib/moex-trading/stage8b/r2a5/admissions
  setcap -r /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
fi

prepare_operation() {
  local operation="$1"
  operation_lower="${operation,,}"
  rm -rf /etc/moex-trading/stage8b/r2a5 \
    /var/lib/moex-trading/stage8b/r2a5 \
    /var/lib/moex-trading/stage8b/r2a6 \
    /var/lib/moex-trading/stage8b/r2a7-controlled \
    /var/lib/moex-trading/stage8b/r2b-evidence \
    /var/lib/moex-trading/operational-authorities \
    /run/moex-trading/stage8b/r2a5 \
    /run/credentials/moex-trading/stage8b/r2a5
  "$LAYOUT" seed-r2a6 "$operation"
  base="/var/lib/moex-trading/stage8b/r2a7-controlled/$operation_lower"
  install -d -o 0 -g 0 -m 0755 /var/lib/moex-trading/stage8b/r2a7-controlled "$base"
  install -d -o 8095 -g 8095 -m 0700 "$base/stage7b" "$base/stage8a1-authority"
  install -d -o 8095 -g 8095 -m 0755 "$base/current-source" "$base/operational-authorities"
  install -d -o 8096 -g 8096 -m 0755 "$base/manifest"
  printf '%s\n' "$(printf '5a%.0s' {1..32})" \
    | install -o 8096 -g 8095 -m 0640 /dev/stdin "$base/manifest/stage8b-r2a7-lifecycle-key.hex"
  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$SEEDER" "--seed-controlled-$operation_lower"
  setpriv --reuid 8096 --regid 8096 --clear-groups \
    "$MANIFEST_ISSUER" "--one-shot-controlled-$operation_lower"
  setpriv --reuid 8095 --regid 8095 --clear-groups \
    "$ADAPTER" "--one-shot-controlled-$operation_lower" \
    >"/tmp/stage8b-r2b-r3-adapter-$operation_lower.json"
  grep -Fq '"adapter_domain":"controlled_qualification"' \
    "/tmp/stage8b-r2b-r3-adapter-$operation_lower.json"
  "$LAYOUT" bind-r2a8 "$operation"
  pids=()
  for source in "${SOURCES[@]}"; do
    (
      setpriv --reuid "${PRODUCER_UID[$source]}" --regid "${PRODUCER_UID[$source]}" \
        --clear-groups "$PRODUCER" "--controlled-r2a8-$operation_lower" "$source"
      setpriv --reuid "${ISSUER_UID[$source]}" --regid "${ISSUER_UID[$source]}" \
        --clear-groups "$ISSUER" "$source"
    ) &
    pids+=("$!")
  done
  for pid in "${pids[@]}"; do wait "$pid"; done
  "$LAYOUT" finalize "$helper_sha256"
  "$PACKAGE_ISSUER"

  # Issuance is complete. Transfer only the three runtime credential objects
  # to the fixed helper identity; package/source signing keys remain root or
  # issuer owned and unreadable by UID 8301. The top-level execute-only mode
  # permits exact-path opens without exposing a directory listing.
  credential_root=/run/credentials/moex-trading/stage8b/r2a5
  chmod 0711 "$credential_root"
  chown 8301:8301 \
    "$credential_root/account-id" \
    "$credential_root/finam-readonly-secret" \
    "$credential_root/account-binding-keys" \
    "$credential_root/account-binding-keys/generation-7.hex"
  chmod 0600 \
    "$credential_root/account-id" \
    "$credential_root/finam-readonly-secret" \
    "$credential_root/account-binding-keys/generation-7.hex"
  chmod 0700 "$credential_root/account-binding-keys"
}

for operation in PLACE CANCEL; do
  operation_lower="${operation,,}"
  prepare_operation "$operation"

  install -d -o 0 -g 0 -m 0700 /var/lib/moex-trading/stage8b/r2b-evidence
  "$SERVER" "$operation" >"/tmp/stage8b-r2b-${operation,,}-server.out" 2>&1 &
  server_pid="$!"
  for _ in $(seq 1 100); do
    [[ -s /run/moex-trading/stage8b/r2a5/controlled-endpoint.txt ]] && break
    sleep 0.05
  done
  test -s /run/moex-trading/stage8b/r2a5/controlled-endpoint.txt
  "$CONTROLLED_LAUNCHER" --controlled-custody >"/tmp/stage8b-r2b-${operation,,}.out" 2>&1
  wait "$server_pid"

  nonce="$(tr -d '\r\n' </run/moex-trading/stage8b/r2a5/run-nonce.sha256)"
  admission=/var/lib/moex-trading/stage8b/r2a5/admissions
  if [[ ! -f "$admission/$nonce.requested" ]]; then
    sed -n '1,40p' "/tmp/stage8b-r2b-${operation,,}.out" >&2
    echo "stage8b-r2b-r3-custody: launcher failed before durable admission" >&2
    exit 1
  fi
  test -f "$admission/$nonce.requested"
  test -f "$admission/$nonce.marker-created"
  test -f "$admission/$nonce.durable"
  test -f "$admission/$nonce.helper-exec-attempted"
  test -f "$admission/$nonce.helper-process-started"
  test -f "$admission/$nonce.helper-terminal-received"
  test -f "$admission/$nonce.helper-exited-success"
  test -f "$admission/$nonce.terminal-evidence-durable"
  test -f "/var/lib/moex-trading/stage8b/r2a5/used-run-nonces/$nonce"
  terminal="/var/lib/moex-trading/stage8b/r2b-evidence/r2b-terminal-$nonce.json"
  test -f "$terminal"
  grep -Fq '"terminal_outcome":"SUCCESS"' "$terminal"
  grep -Fq '"order_post_sent":false' "$terminal"
  grep -Fq '"order_delete_sent":false' "$terminal"
  grep -Fq '"raw_body_exported":false' "$terminal"
  grep -Fq '"admission_commitment_sha256"' "$terminal"
  grep -Fq '"child_pid":' "$terminal"
  grep -Fq '"child_exit_code":0' "$terminal"
  test "$(stat -c %u /var/lib/moex-trading/stage8b/r2b-evidence)" = "0"
  test "$(stat -c %g /var/lib/moex-trading/stage8b/r2b-evidence)" = "0"
  test "$(stat -c %a /var/lib/moex-trading/stage8b/r2b-evidence)" = "700"
  test "$(stat -c %u "$terminal")" = "0"
  test "$(stat -c %g "$terminal")" = "0"
  test "$(stat -c %a "$terminal")" = "400"
  if setpriv --reuid 8301 --regid 8301 --clear-groups \
    rm "/var/lib/moex-trading/stage8b/r2a5/used-run-nonces/$nonce" 2>/dev/null; then
    echo "stage8b-r2b-r3-custody: helper identity deleted nonce" >&2
    exit 1
  fi

  # Direct startup and an unprivileged self-sealed memfd cannot substitute the
  # root-owned receipt plus admission/nonce inode descriptors.
  if setpriv --reuid 8301 --regid 8301 --clear-groups \
    "$HELPER" --r2b-controlled-custody-one-shot >/tmp/stage8b-r2b-direct-helper.log 2>&1; then
    echo "stage8b-r2b-r3-custody: direct helper invocation accepted" >&2
    exit 1
  fi
  if setpriv --reuid 8301 --regid 8301 --clear-groups python3 - "$HELPER" <<'PY'
import fcntl
import os
import subprocess
import sys

fd = os.memfd_create("forged-r2b-receipt", os.MFD_ALLOW_SEALING)
os.write(fd, b"{}")
os.fchmod(fd, 0o400)
fcntl.fcntl(fd, fcntl.F_ADD_SEALS, fcntl.F_SEAL_SEAL | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_GROW | fcntl.F_SEAL_WRITE)
if fd != 3:
    os.dup2(fd, 3)
result = subprocess.run([sys.argv[1], "--r2b-controlled-custody-one-shot"], pass_fds=(3,), check=False)
raise SystemExit(0 if result.returncode == 0 else 1)
PY
  then
    echo "stage8b-r2b-r3-custody: forged UID8301 receipt accepted" >&2
    exit 1
  fi

  # The final root record is neither mutable nor replaceable by the helper.
  for attack in \
    "sh -c ': > \"$terminal\"'" \
    "chmod 0600 \"$terminal\"" \
    "rm \"$terminal\"" \
    "mv \"$terminal\" \"$terminal.moved\"" \
    "touch \"$terminal.recreated\""; do
    if setpriv --reuid 8301 --regid 8301 --clear-groups sh -c "$attack" 2>/dev/null; then
      echo "stage8b-r2b-r3-custody: UID8301 mutated root terminal evidence" >&2
      exit 1
    fi
  done
  test -f "$terminal"
  test "$(stat -c %a "$terminal")" = "400"
  if "$CONTROLLED_LAUNCHER" --controlled-custody >/tmp/stage8b-r2b-replay.log 2>&1; then
    echo "stage8b-r2b-r3-custody: replay was accepted" >&2
    exit 1
  fi
  test "$(find /var/lib/moex-trading/stage8b/r2b-evidence -maxdepth 1 -type f -name 'r2b-terminal-*.json' | wc -l | tr -d ' ')" = "1"
  echo "stage8b-r2b-r3-production-custody-$operation: PASS"
done

run_supervisor_fault() {
  local fault="$1"
  local operation="PLACE"
  local operation_lower="place"
  prepare_operation "$operation"
  install -d -o 0 -g 0 -m 0700 /var/lib/moex-trading/stage8b/r2b-evidence
  if STAGE8B_R2B_CONTROLLED_FAULT="$fault" \
    "$CONTROLLED_LAUNCHER" --controlled-custody \
    >"/tmp/stage8b-r2b-r3-fault-$fault.log" 2>&1; then
    echo "stage8b-r2b-r3-custody: controlled fault unexpectedly succeeded: $fault" >&2
    exit 1
  fi
  local nonce
  nonce="$(tr -d '\r\n' </run/moex-trading/stage8b/r2a5/run-nonce.sha256)"
  local admission=/var/lib/moex-trading/stage8b/r2a5/admissions
  test -f "$admission/$nonce.helper-exec-attempted"
  if [[ "$fault" = "FINALIZER_FSYNC_FAILURE" ]]; then
    test -f "$admission/$nonce.terminal-persistence-failure"
    test ! -f "/var/lib/moex-trading/stage8b/r2b-evidence/r2b-terminal-$nonce.json"
  else
    test -f "$admission/$nonce.helper-exited-failure"
    test -f "$admission/$nonce.terminal-evidence-durable"
    test -f "/var/lib/moex-trading/stage8b/r2b-evidence/r2b-terminal-$nonce.json"
  fi
  if [[ "$fault" = "HELPER_CRASH_AFTER_STARTED" ]]; then
    test -f "$admission/$nonce.helper-process-started"
  fi
  echo "stage8b-r2b-r3-supervisor-fault-$fault: PASS"
}

run_supervisor_fault FEXECVE_FAILURE
run_supervisor_fault HELPER_CRASH_AFTER_STARTED
run_supervisor_fault FINALIZER_FSYNC_FAILURE

echo "stage8b-r2b-r3-linux-custody: PASS root_authenticated=true immutable_terminal=true helper_fd_bound=true root_nonce=true uid8301=true replay=false direct_helper=false fexecve_failure=true helper_crash=true fsync_failure_marker=true external_network=false"
