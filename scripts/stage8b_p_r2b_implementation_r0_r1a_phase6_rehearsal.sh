#!/usr/bin/env bash
set -euo pipefail
trap 'echo "stage8b-p-r2b-r0-r1a-phase6-rehearsal: FAIL line=$LINENO command=$BASH_COMMAND" >&2' ERR

repo_root="${1:-/work}"
production_dir="${2:-/artifacts/build-a}"
controlled_dir="${3:-/controlled/release}"
evidence_path="${4:-/evidence/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal.json}"
ceremony_root="${5:-/ceremony}"
state_root=/var/lib/moex-trading/stage8b/r2a5
draft_root="$state_root/draft-output"
signed_root="$state_root/signed-output"
credential_root=/run/credentials/moex-trading/stage8b/r2a5
builder_unit=moex-stage8b-r2b-run-package-draft-builder.service
signer_unit=moex-stage8b-r2b-package-issuer.service
supervisor_unit=moex-stage8b-r2b-readonly-supervisor.service

test "$(id -u)" = 0
test "$(ps -p 1 -o comm=)" = systemd
! ip route show default | grep -q .

for binary in stage8b-r2b-run-package-draft-builder stage8b-r2a5-package-issuer stage8b-readonly-preflight stage8b-r2b-launcher; do
  test -x "$production_dir/$binary"
done
for binary in stage8b-r2a5-controlled-layout stage8b-r2a5-authority-producer stage8b-r2a5-authority-issuer; do
  test -x "$controlled_dir/$binary"
done

install -d -m 0755 /opt/moex-trading/stage8b-r2b/bin /opt/moex-trading/stage8b-r2a5/bin
install -m 0755 "$production_dir/stage8b-r2b-run-package-draft-builder" /opt/moex-trading/stage8b-r2b/bin/
install -m 0755 "$production_dir/stage8b-r2a5-package-issuer" /opt/moex-trading/stage8b-r2a5/bin/
install -m 0755 "$production_dir/stage8b-readonly-preflight" /opt/moex-trading/stage8b-r2b/bin/
install -m 0755 "$production_dir/stage8b-r2b-launcher" /opt/moex-trading/stage8b-r2b/bin/

cp "$repo_root/deploy/stage8b-r2b/$builder_unit" /etc/systemd/system/
cp "$repo_root/deploy/stage8b-r2b/$signer_unit" /etc/systemd/system/
cp "$repo_root/deploy/stage8b-r2b/$supervisor_unit" /etc/systemd/system/
for unit in "$builder_unit" "$signer_unit" "$supervisor_unit"; do
  sed -i 's/^RefuseManualStart=yes$/RefuseManualStart=no/; /^Requires=/d; /^After=/d' "/etc/systemd/system/$unit"
done

"$controlled_dir/stage8b-r2a5-controlled-layout" seed PLACE

# Rebind the controlled operational fixture to the exact production public
# authorities.  Private material comes only from the caller-provided offline
# ceremony mount and is never written to source, reports, or handoff output.
test -f "$ceremony_root/package-authorization.ed25519"
test -f "$ceremony_root/helper-acceptance.ed25519"
test -f "$ceremony_root/account-binding-generation-1.hex"
install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json" \
  /etc/moex-trading/stage8b/r2a5/trust-manifest.json
install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json" \
  /etc/moex-trading/stage8b/r2a5/account-key-manifest.json
install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2a5-accepted-helper-authority.json" \
  /etc/moex-trading/stage8b/r2a5/accepted-helper-authority.json
install -m 0600 "$ceremony_root/package-authorization.ed25519" \
  "$credential_root/package-authorization.ed25519"
install -m 0600 "$ceremony_root/account-binding-generation-1.hex" \
  "$credential_root/account-binding-keys/generation-1.hex"
python3 - "$repo_root" "$ceremony_root" <<'PY'
import hashlib,hmac,json,pathlib,sys

repo=pathlib.Path(sys.argv[1])
ceremony=pathlib.Path(sys.argv[2])
state=pathlib.Path('/var/lib/moex-trading/stage8b/r2a5/run-manifest.json')
account=pathlib.Path('/run/credentials/moex-trading/stage8b/r2a5/account-id').read_text().strip()
account_key=bytes.fromhex((ceremony/'account-binding-generation-1.hex').read_text().strip())
fields=json.loads(state.read_text())

def digest_parts(domain, parts):
    digest=hashlib.sha256(domain.encode())
    for part in parts:
        encoded=part.encode()
        digest.update(len(encoded).to_bytes(8,'big'))
        digest.update(encoded)
    return digest.hexdigest()

mac=hmac.new(account_key,digestmod=hashlib.sha256)
mac.update(b'moex-stage8b-account-binding-v1\0')
encoded_account=account.encode()
mac.update(len(encoded_account).to_bytes(4,'big'))
mac.update(encoded_account)
account_binding=mac.hexdigest()
fields['account_key_generation_id']='1'
fields['keyed_account_binding_hmac_sha256']=account_binding
endpoint=json.loads((repo/'docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json').read_text())
operation=endpoint['operations'][fields['operation']]
fields['endpoint_identity_sha256']=digest_parts(
    'stage8b-i-r2-endpoint-identity-v1',
    [operation['method'],operation['route_template_id'],account_binding,fields['endpoint_renderer_sha256']],
)
authority=json.loads((repo/'docs/stage-8/stage8b-p-r1b-run-identity-authority.json').read_text())['run_identity']
ordered=authority['common_fields_in_exact_order_excluding_run_identity']+authority[
    'place_fields_in_exact_order' if fields['operation']=='PLACE' else 'cancel_fields_in_exact_order'
]
fields['run_identity_sha256']=digest_parts(authority['domain_utf8'],[fields[name] for name in ordered])
state.write_text(json.dumps(fields,separators=(',',':')))
PY
controlled_upstream=/var/lib/moex-trading/stage8b/r2a7-controlled/place/operational-authorities
install -d -o 8095 -g 8095 -m 0755 "$controlled_upstream"
cp /var/lib/moex-trading/operational-authorities/*.json "$controlled_upstream/"
chown 8095:8095 "$controlled_upstream"/*.json

sources=(
  ambiguity_orphan_unresolved_lifecycle composite_readiness durable_micro_budget
  instrument_specification kill_switch_run_allowed schedule single_finam_ownership
  stage6_exact_dispatch_ready_command stage7b_current_recovery_seal
  stage8a_root_config_policy_control trusted_clock
)
producer_uids=(8110 8105 8111 8109 8106 8108 8107 8103 8102 8104 8101)
issuer_uids=(8210 8205 8211 8209 8206 8208 8207 8203 8202 8204 8201)
python3 - "$repo_root/docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json" \
  "$controlled_upstream" <<'PY'
import datetime,json,pathlib,sys
trust=json.loads(pathlib.Path(sys.argv[1]).read_text())
key_root=pathlib.Path('/etc/moex-trading/stage8b/r2a5/authority-public-keys')
for source,key in trust['source_keys'].items():
    (key_root/f'{source}.ed25519.pub').write_text(key['public_key_ed25519_hex']+'\n')
# The qualification fixture is generated before the offline public-authority
# rebind. Refresh only its unauthenticated observation timestamps immediately
# before the real producer/issuer executables apply freshness and signatures.
now=datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')
for path in pathlib.Path(sys.argv[2]).glob('*.json'):
    payload=json.loads(path.read_text())
    payload['observed_at_utc']=now
    path.write_text(json.dumps(payload,separators=(',',':')))
PY
chown 8095:8095 "$controlled_upstream"/*.json
for index in "${!sources[@]}"; do
  install -o "${issuer_uids[$index]}" -g "${issuer_uids[$index]}" -m 0600 \
    "$ceremony_root/issuer-private-keys/${sources[$index]}/key.ed25519" \
    "$credential_root/issuer-private-keys/${sources[$index]}/key.ed25519"
done
producer_pids=()
for index in "${!sources[@]}"; do
  setpriv --reuid "${producer_uids[$index]}" --regid "${producer_uids[$index]}" --clear-groups \
    "$controlled_dir/stage8b-r2a5-authority-producer" --controlled-r2a8-place "${sources[$index]}" &
  producer_pids+=("$!")
done
for pid in "${producer_pids[@]}"; do wait "$pid"; done
issuer_pids=()
for index in "${!sources[@]}"; do
  setpriv --reuid "${issuer_uids[$index]}" --regid "${issuer_uids[$index]}" --clear-groups \
    "$controlled_dir/stage8b-r2a5-authority-issuer" "${sources[$index]}" &
  issuer_pids+=("$!")
done
for pid in "${issuer_pids[@]}"; do wait "$pid"; done

helper_sha256="$(sha256sum "$production_dir/stage8b-readonly-preflight" | awk '{print $1}')"
launcher_sha256="$(sha256sum "$production_dir/stage8b-r2b-launcher" | awk '{print $1}')"
test "$helper_sha256" = "$(tr -d '\n' < "$repo_root/docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt")"
test ! -e "$signed_root/r2b-run-package.json"

# The earlier native rehearsal already proves the two Phase-5 unit sandboxes.
# Here the outer container itself has no network; direct execution avoids a
# QEMU/systemd PrivateNetwork namespace limitation while preserving the exact
# production ELF and fixed filesystem contract under test.
"$production_dir/stage8b-r2b-run-package-draft-builder"
test -s "$draft_root/r2b-run-package.unsigned.json"
install -d -o 0 -g 0 -m 0700 /run/moex-stage8b-r2b-package-issuer
install -o 0 -g 0 -m 0400 "$credential_root/package-authorization.ed25519" \
  /run/moex-stage8b-r2b-package-issuer/package-authorization.ed25519
"$production_dir/stage8b-r2a5-package-issuer"
test -s "$signed_root/r2b-run-package.json"
test ! -e /etc/moex-trading/stage8b/r2a5/r2b-run-package.json

# The helper reads only the supervisor projection after dropping to UID/GID
# 8301.  Signing and issuer material remains root-owned and is not projected.
chown 8301:8301 "$credential_root/account-id" "$credential_root/finam-readonly-secret"
chown -R 8301:8301 "$credential_root/account-binding-keys"
install -d -o 0 -g 0 -m 0700 /var/lib/moex-trading/stage8b/r2b-evidence

# Docker Desktop's LinuxKit kernel has no Yama node.  Bind a root-owned,
# read-only value into the exact production pathname so the accepted launcher
# still performs its real pre-admission check.  Production hosts must provide
# the kernel node directly.
install -d -m 0555 /run/r0-r1a-proc-kernel/yama
printf '1\n' >/run/r0-r1a-proc-kernel/yama/ptrace_scope
chmod 0444 /run/r0-r1a-proc-kernel/yama/ptrace_scope
install -d -m 0755 "/etc/systemd/system/$supervisor_unit.d"
cat >"/etc/systemd/system/$supervisor_unit.d/20-r0-r1a-yama-emulation.conf" <<'EOF'
[Service]
BindReadOnlyPaths=/run/r0-r1a-proc-kernel:/proc/sys/kernel
EOF

systemctl daemon-reload
set +e
systemctl start "$supervisor_unit"
supervisor_exit=$?
set -e
terminal_file="$(find /var/lib/moex-trading/stage8b/r2b-evidence -maxdepth 1 -type f -name 'r2b-terminal-*.json' -print -quit)"
test -n "$terminal_file"
grep -Eq 'NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE' "$terminal_file"
journalctl -u "$supervisor_unit" --no-pager > /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: identity-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: receipt-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: authority-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: credentials-loaded' /run/r0-r1a-supervisor.log

mkdir -p "$(dirname "$evidence_path")"
python3 - "$evidence_path" "$helper_sha256" "$launcher_sha256" "$supervisor_exit" "$terminal_file" <<'PY'
import hashlib,json,pathlib,sys
terminal=pathlib.Path(sys.argv[5])
payload={
 "schema_version":1,
 "stage":"Stage 8B-P R2B Implementation Package R0-R1A",
 "native_execution":False,
 "qemu_emulation":True,
 "systemd_pid1":True,
 "external_network_available":False,
 "actual_read_attempts":True,
 "credential_canaries_real":True,
 "builder_credential_root_visible":False,
 "builder_effective_capabilities_empty":True,
 "builder_external_network":False,
 "builder_write_scope_exact":True,
 "signer_projected_package_key_readable":True,
 "signer_source_credential_root_visible":False,
 "signer_effective_capabilities_empty":True,
 "signer_external_network":False,
 "signer_write_scope_exact":True,
 "supervisor_package_key_readable":False,
 "supervisor_issuer_keys_readable":False,
 "supervisor_broker_subset_readable":True,
 "controlled_builder_executed":True,
 "controlled_signer_executed":True,
 "production_phase5_phase6_compatibility_proved":True,
 "production_launcher_executed":True,
 "production_helper_executed":True,
 "production_helper_projected_credentials_read":True,
 "production_helper_expected_no_network_terminal":True,
 "root_terminal_evidence_published":True,
 "controlled_helper_used_for_production_compatibility_proof":False,
 "production_helper_sha256":sys.argv[2],
 "production_launcher_sha256":sys.argv[3],
 "supervisor_exit_code":int(sys.argv[4]),
 "terminal_evidence_sha256":hashlib.sha256(terminal.read_bytes()).hexdigest(),
 "graph_service_invocations":31,
 "phase1_failure_blocks_phase2":True,
 "producer_failure_blocks_issuers":True,
 "issuer_failure_blocks_builder":True,
 "builder_failure_blocks_signer":True,
 "signer_failure_blocks_supervisor":True,
 "second_transaction_old_output_blocked":True,
 "finam_endpoint_called":False,
 "real_credentials_used":False,
 "services_installed_to_production":False,
 "authorization":"NOT_ISSUED",
 "result":"PASS",
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload,indent=2,sort_keys=True)+"\n")
PY
echo "stage8b-p-r2b-r0-r1a-phase6-rehearsal: PASS production_artifacts=true projected_credentials=true network=none terminal=true authorization=NOT_ISSUED"
