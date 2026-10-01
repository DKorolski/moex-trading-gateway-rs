#!/usr/bin/env bash
# Only in disposable network-none Docker root, never on a host.
set -euo pipefail
bash /legacy-smoke.sh
install -m 0440 -o root -g moex-p1-paper /package/payload/supervisor.template.json /etc/moex-finam-p1-paper/supervisor.json
runuser -u moex-p1-paper -- /usr/local/libexec/moex/stage8b-p1-paper-supervisor validate-config /etc/moex-finam-p1-paper/supervisor.json
install -d -m 0750 /etc/moex-finam-p1-paper/o2
install -m 0440 -o root -g moex-p1-paper /package/payload/materialization-policy.json /etc/moex-finam-p1-paper/o2/materialization-policy.json
test ! -e /run/credentials/moex-finam-p1f-o2-materializer.service/finam-account.id
set +e
/usr/local/libexec/moex/stage8b-p1f-o2-materializer >/tmp/policy.stdout 2>/tmp/policy.stderr
code=$?
set -e
test "$code" -eq 70
test ! -s /tmp/policy.stdout
grep -Fx 'No such file or directory (os error 2)' /tmp/policy.stderr
echo 'PASS exact-ELF policy-v2 admission then missing-account rejection exit=70; no credentials supplied'
test -z "$(ls -A /var/lib/moex-finam-p1-paper/state)"
test -z "$(ls -A /var/lib/moex-finam-p1-paper-control)"
echo 'PASS no-riskgate-install ELF inputs network=none activation=false'
