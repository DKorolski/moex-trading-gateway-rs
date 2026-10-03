#!/usr/bin/env bash
# Disposable Linux container with network=none, no host operational mounts.
set -euo pipefail
bash /legacy-smoke.sh
supervisor=/usr/local/libexec/moex/stage8b-p1-paper-supervisor
materializer=/usr/local/libexec/moex/stage8b-p1f-o2-materializer
config=/etc/moex-finam-p1-paper/supervisor.json
policy=/etc/moex-finam-p1-paper/o2/materialization-policy.json
install -m 0440 -o root -g moex-p1-paper /input/supervisor-fixture.json "$config"
runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/sparse.stdout
grep -Fx 'stage8b-p1-paper-supervisor: config-valid' /tmp/sparse.stdout
echo 'PASS sparse exact-elf bootstrap-schema2 config-valid'
for name in schema1 missing-policy foreign-policy legacy-fingerprint; do
  install -m 0440 -o root -g moex-p1-paper "/input/mixed-$name.json" "$config"
  result=0
  runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/sparse.stdout 2>/tmp/sparse.stderr || result=$?
  test "$result" -eq 64
  test ! -s /tmp/sparse.stdout
  printf 'PASS sparse exact-elf mixed-%s exit=64\n' "$name"
done
# Validate policy selection in the actual materializer. Deliberately wrong
# synthetic credential stops before source, manifest, token or network access.
install -d -m 0700 /run/credentials/moex-finam-p1f-o2-materializer.service
printf 'WRONG_FIXTURE_ACCOUNT\n' >/run/credentials/moex-finam-p1f-o2-materializer.service/finam-account.id
chmod 0400 /run/credentials/moex-finam-p1f-o2-materializer.service/finam-account.id
install -m 0440 /input/materialization-policy-fixture.json "$policy"
result=0
"$materializer" >/tmp/sparse.stdout 2>/tmp/sparse.stderr || result=$?
test "$result" -eq 70
test ! -s /tmp/sparse.stdout
grep -Fx 'FINAM account credential does not match policy' /tmp/sparse.stderr
echo 'PASS sparse exact-elf policy-v3 reaches-account-boundary'
install -m 0440 /input/mixed-materialization-policy.json "$policy"
result=0
"$materializer" >/tmp/sparse.stdout 2>/tmp/sparse.stderr || result=$?
test "$result" -eq 70
test ! -s /tmp/sparse.stdout
grep -Fx 'observed policy requires exact source-policy and operational identity' /tmp/sparse.stderr
echo 'PASS sparse exact-elf policy-mismatch rejected'
test -z "$(ls -A /var/lib/moex-finam-p1-paper/state)"
test -z "$(ls -A /var/lib/moex-finam-p1-paper-control)"
echo 'PASS sparse-elf-smoke network=none state=empty activation=false'
