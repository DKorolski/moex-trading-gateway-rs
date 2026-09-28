#!/usr/bin/env bash
# Run only inside the disposable --network=none Linux build-image container.
# This probes real ELF admission/exit/custody, NOT a running systemd manager.
set -euo pipefail
test "$(uname -s)" = Linux
test "$(id -u)" = 0
test -f /payload/stage8b-p1f-o2-materializer
test -f /payload/stage8b-p1f-o2-operator
test -f /payload/stage8b-p1-paper-supervisor

groupadd --gid 65000 moex-p1-paper
useradd --uid 65000 --gid 65000 --no-create-home moex-p1-paper
install -d -m 0755 /usr/local/libexec/moex
install -m 0555 /payload/stage8b-p1f-o2-materializer /usr/local/libexec/moex/
install -m 0555 /payload/stage8b-p1f-o2-operator /usr/local/libexec/moex/
install -m 0555 /payload/stage8b-p1-paper-supervisor /usr/local/libexec/moex/
supervisor=/usr/local/libexec/moex/stage8b-p1-paper-supervisor
operator=/usr/local/libexec/moex/stage8b-p1f-o2-operator
materializer=/usr/local/libexec/moex/stage8b-p1f-o2-materializer
control=/var/lib/moex-finam-p1-paper-control
selector=/etc/moex-finam-p1-paper/o2/active-manifest.sha256
install -d -o root -g moex-p1-paper -m 0750 "$control"

grep -Fx 'User=root' /units/moex-finam-p1-paper-o2-bootstrap-runner.service
grep -Fx 'Group=moex-p1-paper' /units/moex-finam-p1-paper-o2-bootstrap-runner.service
grep -Fx 'UMask=0027' /units/moex-finam-p1-paper-o2-bootstrap-runner.service
grep -Fx 'CapabilityBoundingSet=' /units/moex-finam-p1-paper-o2-bootstrap-runner.service
grep -Fx "ExecStart=$operator runner-fixed" /units/moex-finam-p1-paper-o2-bootstrap-runner.service
grep -Fx "ExecStart=$materializer" /units/moex-finam-p1f-o2-materializer.service
sha256sum "$operator" "$materializer" "$supervisor" /units/*.service

as_runner() {
  (umask 0027; setpriv --regid 65000 --clear-groups --bounding-set=-all \
    --inh-caps=-all --ambient-caps=-all --no-new-privs "$@")
}
expect_70() {
  local name="$1" result
  shift
  set +e
  timeout 10s "$@" >/tmp/negative.stdout 2>/tmp/negative.stderr
  result=$?
  set -e
  test "$result" -eq 70
  test ! -s /tmp/negative.stdout
  printf 'PASS exact-elf %s exit=%s\n' "$name" "$result"
}

# The accepted provisioning operation is exercised only on a disposable root.
as_runner "$operator" prepare-control-root
test "$(stat -c '%u:%g:%a' "$control")" = '0:65000:750'
test -z "$(ls -A "$control")"
echo 'PASS exact-elf precreated-control-root-no-capabilities'
expect_70 materializer-no-policy "$materializer"
expect_70 materializer-nonroot runuser -u moex-p1-paper -- "$materializer"
expect_70 runner-no-selector "$operator" runner-fixed
expect_70 cleanup-no-selector "$operator" cleanup-fixed

install -d -m 0750 -o root -g moex-p1-paper "$(dirname "$selector")"
printf 'invalid-selector\n' > "$selector"
chown root:moex-p1-paper "$selector"
chmod 0440 "$selector"
expect_70 runner-malformed-selector "$operator" runner-fixed
expect_70 cleanup-malformed-selector "$operator" cleanup-fixed

# Config validation reads no FINAM credentials, source bundle or Redis.
# Provision the required empty state parent; no durable root may be created.
# This verifies the bootstrap executable (not just its O2 runner) knows baseline07.
config=/etc/moex-finam-p1-paper/supervisor.json
install -d -m 0700 -o moex-p1-paper -g moex-p1-paper /var/lib/moex-finam-p1-paper/state
install -m 0440 -o root -g moex-p1-paper /template.json "$config"
runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/config.stdout
grep -Fx 'stage8b-p1-paper-supervisor: config-valid' /tmp/config.stdout
echo 'PASS exact-elf bootstrap-supervisor-baseline07-config'
sed -i 's/imoexf-baseline07-bo-only-paper-v1/imoexf-hybrid-high180-paper-v1/' "$config"
set +e
runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/config.stdout 2>/tmp/config.stderr
result=$?
set -e
test "$result" -eq 64
test ! -s /tmp/config.stdout
echo 'PASS exact-elf bootstrap-supervisor-stale-profile exit=64'
if runuser -u moex-p1-paper -- sh -c 'printf tamper >> "$1"' sh "$selector" 2>/dev/null; then
  exit 1
fi
if runuser -u moex-p1-paper -- mv "$control" "$control.moved" 2>/dev/null; then
  exit 1
fi
test -z "$(ls -A "$control")"
test -z "$(ls -A /var/lib/moex-finam-p1-paper/state)"
echo 'PASS exact-elf service-cannot-write-selector-or-replace-control-root'
echo 'PASS stage8b-p1f-o2-elf-smoke network=none authority=absent systemd_runtime_tested=false execution=false'
