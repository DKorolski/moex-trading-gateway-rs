#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "stage8b-r2b-r4-custody: must run as root" >&2
  exit 1
fi

BIN_DIR="${1:-/work/tools/stage8b-readonly-preflight/target/release}"
CONTROLLED_BIN_DIR="${2:-/work/target/release}"
CONTROLLED_LAUNCHER="${3:-$BIN_DIR/stage8b-r2b-launcher-controlled-custody}"
PRODUCTION_BIN_DIR="${4:-/work/target/release}"
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
CREATOR_CHAIN_SEEDER="$CONTROLLED_BIN_DIR/stage8b-r2b-creator-chain-seeder"
AUTHORITATIVE_CREATOR="$PRODUCTION_BIN_DIR/stage8b-r2a8-authoritative-intake-creator"
INTAKE_STAGER="$PRODUCTION_BIN_DIR/stage8b-r2a8-production-intake-stager"

for binary in "$HELPER" "$LAUNCHER" "$CONTROLLED_LAUNCHER" "$PRODUCER" "$ISSUER" "$PACKAGE_ISSUER" "$LAYOUT" "$SERVER" "$ADAPTER" "$SEEDER" "$MANIFEST_ISSUER" "$CREATOR_CHAIN_SEEDER" "$AUTHORITATIVE_CREATOR" "$INTAKE_STAGER"; do
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

# R4 freezes Yama plus an exclusive helper identity before any nonce is
# admitted. Exercise the kernel boundary directly: a same-UID sibling must
# not duplicate receipt/terminal/helper descriptors, read memory, or attach.
test "$(cat /proc/sys/kernel/yama/ptrace_scope)" -ge 1
python3 <<'PY'
import ctypes
import errno
import fcntl
import os
import signal
import socket
import struct
import time

libc = ctypes.CDLL(None, use_errno=True)
SYS_pidfd_getfd = 438
PTRACE_ATTACH = 16
PR_SET_DUMPABLE = 4
PR_GET_DUMPABLE = 3

class IOVec(ctypes.Structure):
    _fields_ = [("iov_base", ctypes.c_void_p), ("iov_len", ctypes.c_size_t)]

read_fd, write_fd = os.pipe()
target = os.fork()
if target == 0:
    os.close(read_fd)
    root_fd = os.open("/etc/hostname", os.O_RDONLY)
    left, right = socket.socketpair()
    metadata_fd = fcntl.fcntl(write_fd, fcntl.F_DUPFD_CLOEXEC, 8)
    os.dup2(root_fd, 3)
    os.dup2(left.fileno(), 4)
    os.dup2(root_fd, 7)
    os.setgroups([])
    os.setgid(8301)
    os.setuid(8301)
    if libc.prctl(PR_SET_DUMPABLE, 0, 0, 0, 0) != 0:
        os._exit(90)
    secret = ctypes.create_string_buffer(b"r2b-root-fd-secret")
    os.write(
        metadata_fd,
        struct.pack("=iQ", libc.prctl(PR_GET_DUMPABLE, 0, 0, 0, 0), ctypes.addressof(secret)),
    )
    signal.pause()
    os._exit(0)

os.close(write_fd)
wire_size = struct.calcsize("=iQ")
wire = b""
while len(wire) < wire_size:
    chunk = os.read(read_fd, wire_size - len(wire))
    if not chunk:
        raise SystemExit("helper bootstrap metadata channel closed early")
    wire += chunk
dumpable, remote_address = struct.unpack("=iQ", wire)
if dumpable != 0:
    raise SystemExit("helper bootstrap remained dumpable")

attacker = os.fork()
if attacker == 0:
    os.setgroups([])
    os.setgid(8301)
    os.setuid(8301)
    pidfd = os.pidfd_open(target)
    for target_fd in (3, 4, 6, 7):
        ctypes.set_errno(0)
        stolen = libc.syscall(SYS_pidfd_getfd, pidfd, target_fd, 0)
        if stolen >= 0 or ctypes.get_errno() not in (errno.EPERM, errno.EACCES):
            os._exit(91)
    local = ctypes.create_string_buffer(16)
    local_iov = IOVec(ctypes.addressof(local), len(local))
    remote_iov = IOVec(remote_address, len(local))
    ctypes.set_errno(0)
    read = libc.process_vm_readv(
        target, ctypes.byref(local_iov), 1, ctypes.byref(remote_iov), 1, 0
    )
    if read >= 0 or ctypes.get_errno() not in (errno.EPERM, errno.EACCES):
        os._exit(92)
    ctypes.set_errno(0)
    attached = libc.ptrace(PTRACE_ATTACH, target, 0, 0)
    if attached == 0 or ctypes.get_errno() not in (errno.EPERM, errno.EACCES):
        os._exit(93)
    os._exit(0)

_, status = os.waitpid(attacker, 0)
os.kill(target, signal.SIGKILL)
os.waitpid(target, 0)
if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
    raise SystemExit(f"same-UID isolation attack unexpectedly succeeded: {status}")
PY
echo "stage8b-r2b-r4-same-uid-isolation: PASS pidfd_getfd=false process_vm_readv=false ptrace=false dumpable=false"

# Any already-running UID8301 process blocks the launcher before root nonce
# admission. This also prevents a pre-positioned terminal-channel sender.
setpriv --reuid 8301 --regid 8301 --clear-groups sleep 10 &
uid8301_pid="$!"
if "$LAUNCHER" >/tmp/stage8b-r2b-r4-dedicated-uid.log 2>&1; then
  kill "$uid8301_pid" || true
  echo "stage8b-r2b-r4-custody: concurrent UID8301 process was accepted" >&2
  exit 1
fi
kill "$uid8301_pid" || true
wait "$uid8301_pid" 2>/dev/null || true
test ! -e /var/lib/moex-trading/stage8b/r2a5/admissions
echo "stage8b-r2b-r4-dedicated-uid-preflight: PASS"

# The production creator and stager are executable, fixed-path components,
# not documentation-only names. A qualification-only setup reconstructs the
# accepted durable owner and publishes the independently owner-signed upstream
# authority, but deliberately does not create the creator's output. The exact
# production creator therefore exercises empty-root generation one, immediate
# N-to-N+1 renewal and staging with Docker networking disabled.
rm -rf \
  /var/lib/moex-trading/stage7b \
  /var/lib/moex-trading/stage8a1-authority \
  /var/lib/moex-trading/stage8b/r2a7/production \
  /var/lib/moex-trading/stage8b/r2a8
install -d -o 8094 -g 8094 -m 0750 \
  /var/lib/moex-trading/stage8b/r2a7/production \
  /var/lib/moex-trading/stage8b/r2a8/intake
printf '%s\n' "$(printf '5a%.0s' {1..32})" \
  | install -o 8096 -g 8095 -m 0640 /dev/stdin \
      /var/lib/moex-trading/stage8b/r2a7/production/stage8b-r2a7-lifecycle-key.hex
"$CREATOR_CHAIN_SEEDER" >/tmp/stage8b-r2b-r4-creator-seed.json
chown -R 8094:8094 /var/lib/moex-trading/stage7b
chown 8094:8094 \
  /var/lib/moex-trading/stage8a1-authority/stage8a4-writer-issuer-signing-key.hex \
  /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-upstream-current-authority.json
test ! -e /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json
test ! -e /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.lock
setpriv --reuid 8094 --regid 8094 --groups 8095 \
  "$AUTHORITATIVE_CREATOR" >/tmp/stage8b-r2b-r4-creator-generation-1.json
python3 - <<'PY'
import json
from pathlib import Path
creator = json.loads(Path('/tmp/stage8b-r2b-r4-creator-generation-1.json').read_text())
intake = json.loads(Path('/var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json').read_text())
assert creator['intake_generation'] == 1
assert creator['bootstrap_mode'] == 'empty_root_generation_one'
assert creator['predecessor_used_as_snapshot_source'] is False
assert intake['intake_generation'] == 1
assert intake['predecessor_intake_commitment_sha256'] is None
PY
generation_one_sha="$(sha256sum /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json | awk '{print $1}')"
setpriv --reuid 8094 --regid 8094 --groups 8095 \
  "$AUTHORITATIVE_CREATOR" >/tmp/stage8b-r2b-r4-creator-generation-2.json
python3 - <<'PY'
import json
from pathlib import Path
creator = json.loads(Path('/tmp/stage8b-r2b-r4-creator-generation-2.json').read_text())
intake = json.loads(Path('/var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json').read_text())
assert creator['intake_generation'] == 2
assert creator['bootstrap_mode'] == 'predecessor_continuity_renewal'
assert creator['predecessor_used_as_snapshot_source'] is False
assert intake['intake_generation'] == 2
assert len(intake['predecessor_intake_commitment_sha256']) == 64
PY
setpriv --reuid 8094 --regid 8094 --groups 8095 \
  "$INTAKE_STAGER" >/tmp/stage8b-r2b-r4-stager.json
after_creator_sha="$(sha256sum /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json | awk '{print $1}')"
staged_sha="$(sha256sum /var/lib/moex-trading/stage8b/r2a8/intake/stage8b-r2a8-production-writer-intake.json | awk '{print $1}')"
test "$after_creator_sha" = "$staged_sha"
test "$generation_one_sha" != "$after_creator_sha"
test ! -e /var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.lock
grep -Fq '"network_accessed":false' /tmp/stage8b-r2b-r4-creator-generation-1.json
grep -Fq '"finam_credential_accessed":false' /tmp/stage8b-r2b-r4-creator-generation-1.json
grep -Fq '"network_accessed":false' /tmp/stage8b-r2b-r4-creator-generation-2.json
grep -Fq '"finam_credential_accessed":false' /tmp/stage8b-r2b-r4-creator-generation-2.json
grep -Fq '"network_accessed":false' /tmp/stage8b-r2b-r4-stager.json
grep -Fq '"finam_credential_accessed":false' /tmp/stage8b-r2b-r4-stager.json
echo "stage8b-r2b-r4-r1-empty-root-renewal-chain: PASS generation1=true generation2=true fixed_paths=true network=false credentials=false"

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

for timeout_fault in \
  NO_START_FRAME NO_TERMINAL_FRAME TERMINAL_THEN_HANG SLOW_DRIP_FRAME \
  PARTIAL_FRAME_HEADER PARTIAL_FRAME_BODY CHILD_IGNORES_CHANNEL; do
  run_supervisor_fault "$timeout_fault"
  nonce="$(tr -d '\r\n' </run/moex-trading/stage8b/r2a5/run-nonce.sha256)"
  terminal="/var/lib/moex-trading/stage8b/r2b-evidence/r2b-terminal-$nonce.json"
  python3 - "$terminal" <<'PY'
import json
import sys

terminal = json.load(open(sys.argv[1], encoding="utf-8"))
assert terminal["root_terminal_outcome"] == "FAILURE", terminal
assert terminal["root_error_category"] == "TIMEOUT", terminal
assert terminal["child_protocol_valid"] is False, terminal
PY
done

echo "stage8b-r2b-r4-linux-custody: PASS root_authenticated=true immutable_terminal=true typed_terminal=true absolute_deadline=true bounded_reap=true yama=true dedicated_uid=true pidfd_getfd=false process_vm_readv=false ptrace=false second_sender=false metadata_fsync=true replay=false direct_helper=false fexecve_failure=true helper_crash=true fsync_failure_marker=true external_network=false"
