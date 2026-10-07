#!/usr/bin/env bash
# Disposable Linux qualification only. No operational paths or network access.
set -euo pipefail
test "$(uname -s)" = Linux
test "$(id -u)" = 0
test "$(id -g)" = 987
umask 0077
printf 'QUALIFICATION uid=%s gid=%s umask=%s\n' "$(id -u)" "$(id -g)" "$(umask)"
uname -srm

case "${1:-}" in
  abort)
    cargo test --locked --offline --release -p runtime-durable-service --all-features --lib \
      stage8b_p1f_guardian::tests::abort_tests:: -- --test-threads=1 --nocapture
    echo 'PASS Linux root:987 abort archive/consume/reopen under umask0077'
    ;;
  prevention)
    cargo test --locked --offline --release -p runtime-durable-service --all-features --lib \
      stage8b_p1f_guardian::tests::o2_external_writer_service_umask_matrix \
      -- --exact --test-threads=1 --nocapture
    ;;
  custody)
    scratch="$(mktemp -d /tmp/o2-abort-custody.XXXXXX)"
    trap 'rm -rf -- "$scratch"' EXIT
    chmod 0755 "$scratch"
    root="$scratch/control"
    STAGE8B_P1F_MULTI_UID_EVIDENCE_ROOT="$root" STAGE8B_P1F_MULTI_UID_SERVICE_GID=987 \
      cargo test --locked --offline --release -p runtime-durable-service --all-features --lib \
      stage8b_p1f_guardian::tests::multi_uid_root_transition_source_probe \
      -- --ignored --exact --test-threads=1
    receipt="$(find "$root/authority/manifests" -name claim-receipt.json -type f -print -quit)"
    test -n "$receipt"
    test "$(stat -c '%u:%g:%a' "$root/authority/history-head.json")" = '0:987:440'
    service() { setpriv --reuid=65534 --regid=987 --clear-groups -- "$@"; }
    service test -r "$root/authority/history-head.json"
    service test -r "$receipt"
    denied() {
      local name="$1"
      shift
      if service "$@" >/dev/null 2>&1; then
        echo "FAIL service custody $name" >&2
        exit 1
      fi
      echo "PASS service custody $name"
    }
    denied unlink rm "$root/authority/history-head.json"
    denied rename mv "$receipt" "$receipt.moved"
    denied chmod chmod 0640 "$receipt"
    denied create mkdir "$root/authority/injected"
    denied parent-substitution mv "$root" "$root.moved"
    denied guardian-lock test -r "$root/.guardian.lock"
    echo 'PASS Linux root:987 service-65534:987 custody; no operational state'
    ;;
  *) exit 64 ;;
esac
