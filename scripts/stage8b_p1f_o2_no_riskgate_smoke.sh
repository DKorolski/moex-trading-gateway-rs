#!/usr/bin/env bash
# Disposable network-none container only; /input is a local fixture, not a deployment.
set -euo pipefail
bash /legacy-smoke.sh
supervisor=/usr/local/libexec/moex/stage8b-p1-paper-supervisor
config=/etc/moex-finam-p1-paper/supervisor.json
install -m 0440 -o root -g moex-p1-paper /input/supervisor-fixture.json "$config"
runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/nrg.stdout
grep -Fx 'stage8b-p1-paper-supervisor: config-valid' /tmp/nrg.stdout
echo 'PASS no-riskgate exact-elf profile-v2 config-valid'
for field in runtime_profile_sha256 runtime_config_fingerprint_sha256; do
  install -m 0440 -o root -g moex-p1-paper "/input/mixed-$field.json" "$config"
  set +e
  runuser -u moex-p1-paper -- "$supervisor" validate-config "$config" >/tmp/nrg.stdout 2>/tmp/nrg.stderr
  result=$?
  set -e
  test "$result" -eq 64
  test ! -s /tmp/nrg.stdout
  printf 'PASS no-riskgate exact-elf mixed-%s exit=64\n' "$field"
done
test -z "$(ls -A /var/lib/moex-finam-p1-paper/state)"
test -z "$(ls -A /var/lib/moex-finam-p1-paper-control)"
echo 'PASS no-riskgate-elf-smoke network=none state=empty activation=false'
