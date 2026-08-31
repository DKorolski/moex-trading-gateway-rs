#!/usr/bin/env bash
set -euo pipefail
trap 'echo "stage8b-p-r2b-r0-r1-linux-rehearsal: FAIL line=$LINENO command=$BASH_COMMAND" >&2' ERR

if [[ "$(id -u)" != "0" ]] || [[ "$(ps -p 1 -o comm=)" != "systemd" ]]; then
  echo "stage8b-p-r2b-r0-r1-linux-rehearsal: FAIL requires root in a systemd container" >&2
  exit 1
fi

repo_root="${1:-/work}"
production_dir="${2:-/artifacts/build-a}"
controlled_dir="${3:-/controlled/release}"
evidence_path="${4:-/evidence/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal.json}"
builder_unit="moex-stage8b-r2b-run-package-draft-builder.service"
signer_unit="moex-stage8b-r2b-package-issuer.service"
supervisor_unit="moex-stage8b-r2b-readonly-supervisor.service"
credential_root="/run/credentials/moex-trading/stage8b/r2a5"
state_root="/var/lib/moex-trading/stage8b/r2a5"
draft_root="$state_root/draft-output"
signed_root="$state_root/signed-output"

for binary in \
  "$production_dir/stage8b-r2b-run-package-draft-builder" \
  "$production_dir/stage8b-r2a5-package-issuer" \
  "$controlled_dir/stage8b-readonly-preflight" \
  "$controlled_dir/stage8b-r2a5-authority-producer" \
  "$controlled_dir/stage8b-r2a5-authority-issuer" \
  "$controlled_dir/stage8b-r2a5-controlled-layout"; do
  test -x "$binary"
done

if ip route show default | grep -q .; then
  echo "stage8b-p-r2b-r0-r1-linux-rehearsal: FAIL external route present" >&2
  exit 1
fi

install -d -m 0755 /opt/moex-trading/stage8b-r2b/bin /opt/moex-trading/stage8b-r2a5/bin
install -m 0755 "$production_dir/stage8b-r2b-run-package-draft-builder" \
  /opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-run-package-draft-builder
install -m 0755 "$production_dir/stage8b-r2a5-package-issuer" \
  /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-package-issuer
install -m 0755 "$controlled_dir/stage8b-readonly-preflight" \
  /opt/moex-trading/stage8b-r2a5/bin/stage8b-readonly-preflight

cp "$repo_root"/deploy/stage8b-r2b/*.service /etc/systemd/system/
cp "$repo_root"/deploy/stage8b-r2b/*.target /etc/systemd/system/
cp "$repo_root"/deploy/stage8b-r2a5/*.service /etc/systemd/system/
# The production files remain non-manually-startable.  Only the disposable
# rehearsal copies relax activation so their unchanged [Service] sandboxes and
# dependency graph can be exercised by the controlled trigger.
sed -i 's/^RefuseManualStart=yes$/RefuseManualStart=no/' \
  /etc/systemd/system/moex-stage8b-*.service \
  /etc/systemd/system/moex-stage8b-*.target \
  /etc/systemd/system/stage8b-r2a5-*.service
sed -i '/^Requires=/d; /^After=/d' \
  "/etc/systemd/system/$builder_unit" \
  "/etc/systemd/system/$signer_unit" \
  "/etc/systemd/system/$supervisor_unit"

install -d -m 0755 "$state_root" "$draft_root" "$signed_root" \
  /var/lib/moex-trading/stage8b/r2b-evidence \
  /etc/moex-trading/stage8b/r2a5 /run/moex-trading/stage8b/r2a5
install -d -m 0700 "$credential_root" "$credential_root/account-binding-keys" \
  "$credential_root/issuer-private-keys"
printf 'CANARY-PACKAGE\n' >"$credential_root/package-authorization.ed25519"
printf 'CANARY-FINAM\n' >"$credential_root/finam-readonly-secret"
printf 'CANARY-ACCOUNT\n' >"$credential_root/account-id"
printf 'CANARY-ACCOUNT-KEY\n' >"$credential_root/account-binding-keys/canary.key"
printf 'CANARY-ISSUER\n' >"$credential_root/issuer-private-keys/canary.key"
printf 'CANARY-HELPER\n' >"$credential_root/helper-acceptance.ed25519"
chmod 0400 "$credential_root"/*.ed25519 "$credential_root"/finam-readonly-secret \
  "$credential_root"/account-id "$credential_root"/account-binding-keys/canary.key \
  "$credential_root"/issuer-private-keys/canary.key

cat >/usr/local/bin/r0r1-sandbox-probe <<'SH'
#!/usr/bin/env bash
set -euo pipefail
exec >>/run/r0r1-probe-debug 2>&1
role="$1"
credential_root=/run/credentials/moex-trading/stage8b/r2a5
state_root=/var/lib/moex-trading/stage8b/r2a5
network_must_fail() {
  ! timeout 1 bash -c 'exec 3<>/dev/tcp/192.0.2.1/9' 2>/dev/null
}
case "$role" in
  builder)
    test ! -e "$credential_root"
    test "$(awk '/^CapEff:/ {print $2}' /proc/self/status)" = 0000000000000000
    network_must_fail
    printf ok >"$state_root/draft-output/probe"
    ! sh -c "printf bad >'$state_root/forbidden-builder-write'" 2>/dev/null
    ;;
  signer)
    test "$(cat /run/moex-stage8b-r2b-package-issuer/package-authorization.ed25519)" = CANARY-PACKAGE
    test ! -e "$credential_root"
    test "$(awk '/^CapEff:/ {print $2}' /proc/self/status)" = 0000000000000000
    network_must_fail
    printf ok >"$state_root/signed-output/probe"
    ! sh -c "printf bad >/etc/moex-trading/stage8b/r2a5/forbidden-signer-write" 2>/dev/null
    ;;
  supervisor)
    test ! -e "$credential_root"
    test ! -e /run/moex-stage8b-r2b-supervisor/package-authorization.ed25519
    test ! -e /run/moex-stage8b-r2b-supervisor/helper-acceptance.ed25519
    test ! -e /run/moex-stage8b-r2b-supervisor/issuer-private-keys
    test "$(cat /run/moex-stage8b-r2b-supervisor/account-id)" = CANARY-ACCOUNT
    test "$(cat /run/moex-stage8b-r2b-supervisor/finam-readonly-secret)" = CANARY-FINAM
    test "$(cat /run/moex-stage8b-r2b-supervisor/account-binding-keys/canary.key)" = CANARY-ACCOUNT-KEY
    network_must_fail
    ;;
  *) exit 64 ;;
esac
printf '%s PASS\n' "$role" >>/run/r0r1-sandbox-results
SH
chmod 0755 /usr/local/bin/r0r1-sandbox-probe

make_isolated_dropin() {
  unit="$1"
  role="$2"
  directory="/etc/systemd/system/$unit.d"
  install -d -m 0755 "$directory"
  cat >"$directory/10-r0r1-isolated.conf" <<EOF
[Unit]
Requires=
After=
RefuseManualStart=no

[Service]
ExecStart=
ExecStart=/usr/local/bin/r0r1-sandbox-probe $role
EOF
  cat >"/etc/systemd/system/r0r1-$role-trigger.service" <<EOF
[Unit]
Requires=$unit
After=$unit

[Service]
Type=oneshot
ExecStart=/bin/true
EOF
}

: >/run/r0r1-sandbox-results
make_isolated_dropin "$builder_unit" builder
make_isolated_dropin "$signer_unit" signer
make_isolated_dropin "$supervisor_unit" supervisor
systemctl daemon-reload
for role in builder signer supervisor; do
  systemctl start "r0r1-$role-trigger.service"
done
grep -Fxq 'builder PASS' /run/r0r1-sandbox-results
grep -Fxq 'signer PASS' /run/r0r1-sandbox-results
grep -Fxq 'supervisor PASS' /run/r0r1-sandbox-results

rm -f "$draft_root/probe" "$signed_root/probe"
rm -rf /etc/systemd/system/"$builder_unit".d \
  /etc/systemd/system/"$signer_unit".d \
  /etc/systemd/system/"$supervisor_unit".d
rm -f /etc/systemd/system/r0r1-*-trigger.service
systemctl daemon-reload
systemctl reset-failed

rm -rf /etc/moex-trading/stage8b/r2a5 "$state_root" \
  /var/lib/moex-trading/operational-authorities \
  /run/moex-trading/stage8b/r2a5 "$credential_root"
"$controlled_dir/stage8b-r2a5-controlled-layout" seed PLACE
controlled_upstream=/var/lib/moex-trading/stage8b/r2a7-controlled/place/operational-authorities
install -d -o 8095 -g 8095 -m 0755 "$controlled_upstream"
cp /var/lib/moex-trading/operational-authorities/*.json "$controlled_upstream/"
chown 8095:8095 "$controlled_upstream"/*.json
chmod 0644 "$controlled_upstream"/*.json

sources=(
  ambiguity_orphan_unresolved_lifecycle composite_readiness durable_micro_budget
  instrument_specification kill_switch_run_allowed schedule single_finam_ownership
  stage6_exact_dispatch_ready_command stage7b_current_recovery_seal
  stage8a_root_config_policy_control trusted_clock
)
producer_uids=(8110 8105 8111 8109 8106 8108 8107 8103 8102 8104 8101)
issuer_uids=(8210 8205 8211 8209 8206 8208 8207 8203 8202 8204 8201)
for index in "${!sources[@]}"; do
  setpriv --reuid "${producer_uids[$index]}" --regid "${producer_uids[$index]}" \
    --clear-groups "$controlled_dir/stage8b-r2a5-authority-producer" \
    --controlled-r2a8-place "${sources[$index]}"
done
for index in "${!sources[@]}"; do
  setpriv --reuid "${issuer_uids[$index]}" --regid "${issuer_uids[$index]}" \
    --clear-groups "$controlled_dir/stage8b-r2a5-authority-issuer" "${sources[$index]}"
done
helper_sha256="$(sha256sum "$controlled_dir/stage8b-readonly-preflight" | awk '{print $1}')"
"$controlled_dir/stage8b-r2a5-controlled-layout" finalize "$helper_sha256"
rm -f "$draft_root/r2b-run-package.unsigned.json"

make_actual_trigger() {
  unit="$1"
  role="$2"
  install -d -m 0755 "/etc/systemd/system/$unit.d"
  cat >"/etc/systemd/system/$unit.d/10-r0r1-isolated.conf" <<EOF
[Unit]
Requires=
After=
RefuseManualStart=no
EOF
  cat >"/etc/systemd/system/r0r1-$role-trigger.service" <<EOF
[Unit]
Requires=$unit
After=$unit

[Service]
Type=oneshot
ExecStart=/bin/true
EOF
}
make_actual_trigger "$builder_unit" actual-builder
make_actual_trigger "$signer_unit" actual-signer
systemctl daemon-reload
systemctl start r0r1-actual-builder-trigger.service
test -s "$draft_root/r2b-run-package.unsigned.json"
systemctl start r0r1-actual-signer-trigger.service
test -s "$signed_root/r2b-run-package.json"
test ! -e /etc/moex-trading/stage8b/r2a5/r2b-run-package.json
unsigned_sha256="$(sha256sum "$draft_root/r2b-run-package.unsigned.json" | awk '{print $1}')"
signed_sha256="$(sha256sum "$signed_root/r2b-run-package.json" | awk '{print $1}')"

rm -rf /etc/systemd/system/"$builder_unit".d /etc/systemd/system/"$signer_unit".d
rm -f /etc/systemd/system/r0r1-actual-*-trigger.service
rm -f "$draft_root/r2b-run-package.unsigned.json" "$signed_root/r2b-run-package.json"

# Restore exact production dependency declarations before fault-injecting the
# complete six-phase graph; only manual-start refusal remains relaxed in this
# disposable namespace.
cp "$repo_root"/deploy/stage8b-r2b/*.service /etc/systemd/system/
cp "$repo_root"/deploy/stage8b-r2b/*.target /etc/systemd/system/
cp "$repo_root"/deploy/stage8b-r2a5/*.service /etc/systemd/system/
sed -i 's/^RefuseManualStart=yes$/RefuseManualStart=no/' \
  /etc/systemd/system/moex-stage8b-*.service \
  /etc/systemd/system/moex-stage8b-*.target \
  /etc/systemd/system/stage8b-r2a5-*.service

# Every production namespace prerequisite exists in the disposable graph
# rehearsal. ExecStart is replaced below, but the original mount policy stays
# active and must be constructible for every one of the 31 service instances.
install -d -m 0755 \
  /run/r0r1-graph \
  /var/lib/moex-trading/stage7b \
  /var/lib/moex-trading/stage8a1-authority \
  /var/lib/moex-trading/stage8b/r2a6/adapter-work \
  /var/lib/moex-trading/stage8b/r2a7/production \
  /var/lib/moex-trading/stage8b/r2a8/intake \
  /var/lib/moex-trading/stage8b/r2a8/current-source

cat >/usr/local/bin/r0r1-graph-probe <<'SH'
#!/usr/bin/env bash
set -euo pipefail
unit="$1"
printf '%s\n' "$unit" >>/run/r0r1-graph/invocations.log
if [[ -s /run/r0r1-graph/fail-unit ]] && [[ "$(cat /run/r0r1-graph/fail-unit)" = "$unit" ]]; then
  exit 75
fi
case "$unit" in
  moex-stage8b-r2b-run-package-draft-builder.service)
    test ! -e /var/lib/moex-trading/stage8b/r2a5/draft-output/graph-package
    printf draft >/var/lib/moex-trading/stage8b/r2a5/draft-output/graph-package
    ;;
  moex-stage8b-r2b-package-issuer.service)
    test ! -e /var/lib/moex-trading/stage8b/r2a5/signed-output/graph-package
    printf signed >/var/lib/moex-trading/stage8b/r2a5/signed-output/graph-package
    ;;
esac
SH
chmod 0755 /usr/local/bin/r0r1-graph-probe

for service in /etc/systemd/system/moex-stage8b-*.service /etc/systemd/system/stage8b-r2a*.service; do
  [[ -f "$service" ]] || continue
  unit="$(basename "$service")"
  install -d -m 0755 "/etc/systemd/system/$unit.d"
  cat >"/etc/systemd/system/$unit.d/90-r0r1-graph.conf" <<EOF
[Unit]
RefuseManualStart=no

[Service]
User=root
Group=root
SupplementaryGroups=
ReadWritePaths=/run/r0r1-graph
ExecStart=
ExecStart=/usr/local/bin/r0r1-graph-probe %n
EOF
done
for target in /etc/systemd/system/moex-stage8b-r2b-*.target; do
  [[ -f "$target" ]] || continue
  unit="$(basename "$target")"
  install -d -m 0755 "/etc/systemd/system/$unit.d"
  cat >"/etc/systemd/system/$unit.d/90-r0r1-graph.conf" <<EOF
[Unit]
RefuseManualStart=no
EOF
done
cat >/etc/systemd/system/r0r1-graph-trigger.service <<EOF
[Unit]
Requires=moex-stage8b-r2b-issuance.target
After=moex-stage8b-r2b-issuance.target

[Service]
Type=oneshot
ExecStart=/bin/true
EOF
systemctl daemon-reload

graph_reset() {
  systemctl stop r0r1-graph-trigger.service moex-stage8b-r2b-issuance.target \
    moex-stage8b-r2b-phase6-readonly-preflight.target \
    moex-stage8b-r2b-phase5-run-package.target \
    moex-stage8b-r2b-phase4-authority-issuers.target \
    moex-stage8b-r2b-phase3-authority-producers.target \
    moex-stage8b-r2b-phase2-manifest-source.target \
    moex-stage8b-r2b-phase1-current-source.target >/dev/null 2>&1 || true
  systemctl reset-failed >/dev/null 2>&1 || true
  rm -f /run/r0r1-graph/invocations.log /run/r0r1-graph/fail-unit \
    "$draft_root/graph-package" "$signed_root/graph-package"
}

graph_reset
systemctl start r0r1-graph-trigger.service
test "$(wc -l </run/r0r1-graph/invocations.log)" -eq 31
for _ in $(seq 1 50); do
  [[ "$(systemctl is-active moex-stage8b-r2b-issuance.target || true)" = inactive ]] && break
  sleep 0.1
done
test "$(systemctl is-active moex-stage8b-r2b-issuance.target || true)" = inactive
if systemctl start r0r1-graph-trigger.service; then
  echo "stage8b-p-r2b-r0-r1-linux-rehearsal: stale package replay succeeded" >&2
  exit 1
fi

declare -A downstream=(
  [moex-stage8b-r2a8-upstream-current-authority-publisher.service]=stage8b-r2a8-current-manifest-issuer.service
  [stage8b-r2a5-producer@m8p8105.service]=stage8b-r2a5-issuer@m8i8201.service
  [stage8b-r2a5-issuer@m8i8205.service]=moex-stage8b-r2b-run-package-draft-builder.service
  [moex-stage8b-r2b-run-package-draft-builder.service]=moex-stage8b-r2b-package-issuer.service
  [moex-stage8b-r2b-package-issuer.service]=moex-stage8b-r2b-readonly-supervisor.service
)
for failed in "${!downstream[@]}"; do
  graph_reset
  printf '%s\n' "$failed" >/run/r0r1-graph/fail-unit
  if systemctl start r0r1-graph-trigger.service; then
    echo "stage8b-p-r2b-r0-r1-linux-rehearsal: failure propagated as success: $failed" >&2
    exit 1
  fi
  test -s /run/r0r1-graph/invocations.log
  ! grep -Fxq "${downstream[$failed]}" /run/r0r1-graph/invocations.log
done

mkdir -p "$(dirname "$evidence_path")"
python3 - "$evidence_path" "$unsigned_sha256" "$signed_sha256" "$(uname -m)" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
architecture = {"x86_64": "amd64", "aarch64": "arm64"}.get(sys.argv[4], sys.argv[4])
payload = {
    "schema_version": 1,
    "stage": "Stage 8B-P R2B Implementation Package R0-R1",
    "platform": f"linux/{architecture}",
    "native_execution": True,
    "qemu_emulation": False,
    "systemd_pid1": True,
    "external_network_available": False,
    "actual_read_attempts": True,
    "credential_canaries_real": True,
    "builder_credential_root_visible": False,
    "builder_effective_capabilities_empty": True,
    "builder_external_network": False,
    "builder_write_scope_exact": True,
    "signer_projected_package_key_readable": True,
    "signer_source_credential_root_visible": False,
    "signer_effective_capabilities_empty": True,
    "signer_external_network": False,
    "signer_write_scope_exact": True,
    "supervisor_package_key_readable": False,
    "supervisor_issuer_keys_readable": False,
    "supervisor_broker_subset_readable": True,
    "controlled_builder_executed": True,
    "controlled_signer_executed": True,
    "unsigned_package_sha256": sys.argv[2],
    "signed_package_sha256": sys.argv[3],
    "graph_service_invocations": 31,
    "phase1_failure_blocks_phase2": True,
    "producer_failure_blocks_issuers": True,
    "issuer_failure_blocks_builder": True,
    "builder_failure_blocks_signer": True,
    "signer_failure_blocks_supervisor": True,
    "second_transaction_old_output_blocked": True,
    "finam_endpoint_called": False,
    "real_credentials_used": False,
    "services_installed_to_production": False,
    "authorization": "NOT_ISSUED",
    "result": "PASS",
}
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print("stage8b-p-r2b-r0-r1-linux-rehearsal: PASS credentials=true actual_read_attempts=true graph=31 failures=5 replay=blocked network=none authorization=NOT_ISSUED")
PY
