#!/usr/bin/env bash
set -euo pipefail

# Executes one exact Generation-2 transaction inside a disposable native
# systemd container. Production unit files and ELF are copied byte-for-byte;
# this script never patches a production unit or substitutes ExecStart.

repo_root="${1:?repo root required}"
artifact_root="${2:?artifact root required}"
proof_tools="${3:?proof tool root required}"
ceremony_root="${4:?ceremony root required}"
evidence_root="${5:?evidence root required}"

[[ "$(id -u)" = 0 ]]
[[ "$(ps -p 1 -o comm=)" = systemd ]]
[[ "$(uname -m)" = x86_64 ]]
! ip route show default | grep -q .
[[ ! -s /etc/resolv.conf ]]

contract="$repo_root/docs/stage-8/stage8b-p-r2b-generation2-full-transaction-contract.json"
trigger=stage8b-r2b-controlled-proof-trigger.service
state_root=/var/lib/moex-trading/stage8b/r2a5
credential_root=/run/credentials/moex-trading/stage8b/r2a5
terminal_root=/var/lib/moex-trading/stage8b/r2b-evidence
run_id=""
run_root=""
supervisor_invocation_id=""

production_units=(
  moex-stage8b-r2a8-upstream-current-authority-publisher.service
  moex-stage8b-r2a8-authoritative-intake-creator.service
  moex-stage8b-r2a8-production-intake-stager.service
  moex-stage8b-r2a8-production-current-source-writer.service
  stage8b-r2a8-current-manifest-issuer.service
  stage8b-r2a7-source-adapter.service
  stage8b-r2a5-producer@.service
  stage8b-r2a5-issuer@.service
  moex-stage8b-r2b-run-package-draft-builder.service
  moex-stage8b-r2b-package-issuer.service
  moex-stage8b-r2b-readonly-supervisor.service
  moex-stage8b-r2b-phase1-current-source.target
  moex-stage8b-r2b-phase2-manifest-source.target
  moex-stage8b-r2b-phase3-authority-producers.target
  moex-stage8b-r2b-phase4-authority-issuers.target
  moex-stage8b-r2b-phase5-run-package.target
  moex-stage8b-r2b-phase6-readonly-preflight.target
  moex-stage8b-r2b-issuance.target
)

transaction_targets=(
  moex-stage8b-r2b-issuance.target
  moex-stage8b-r2b-phase6-readonly-preflight.target
  moex-stage8b-r2b-phase5-run-package.target
  moex-stage8b-r2b-phase4-authority-issuers.target
  moex-stage8b-r2b-phase3-authority-producers.target
  moex-stage8b-r2b-phase2-manifest-source.target
  moex-stage8b-r2b-phase1-current-source.target
)

sources=(
  trusted_clock stage7b_current_recovery_seal
  stage6_exact_dispatch_ready_command stage8a_root_config_policy_control
  composite_readiness kill_switch_run_allowed single_finam_ownership schedule
  instrument_specification ambiguity_orphan_unresolved_lifecycle durable_micro_budget
)

stop_graph() {
  systemctl stop "$trigger" "${transaction_targets[@]}" >/dev/null 2>&1 || true
  systemctl reset-failed >/dev/null 2>&1 || true
}

reset_transaction_namespace() {
  stop_graph
  rm -rf \
    /etc/moex-trading/stage8b/r2a5 \
    /run/moex-trading/stage8b \
    /run/credentials/moex-trading/stage8b \
    /var/lib/moex-trading/stage7b \
    /var/lib/moex-trading/stage8a1-authority \
    /var/lib/moex-trading/stage8b \
    /var/lib/moex-trading/operational-authorities
  [[ ! -e /run/credentials/moex-trading/stage8b ]]
  [[ ! -e /var/lib/moex-trading/stage8b ]]
}

install_payload() {
  systemd-sysusers "$repo_root/deploy/stage8b-r2a5/stage8b-r2a5.sysusers"
  install -d -m 0755 \
    /opt/moex-trading/stage8b-r2b/bin \
    /opt/moex-trading/stage8b-r2a8/bin \
    /opt/moex-trading/stage8b-r2a7/bin \
    /opt/moex-trading/stage8b-r2a5/bin

  install -m 0755 "$artifact_root/stage8b-r2a8-upstream-current-authority-publisher" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/stage8b-r2a8-authoritative-intake-creator" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/stage8b-r2a8-production-intake-stager" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/stage8b-r2a8-production-current-source-writer" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/stage8b-r2a8-current-manifest-issuer" /opt/moex-trading/stage8b-r2a8/bin/
  install -m 0755 "$artifact_root/stage8b-r2a7-source-adapter" /opt/moex-trading/stage8b-r2a7/bin/
  install -m 0755 "$artifact_root/stage8b-r2a5-authority-producer" /opt/moex-trading/stage8b-r2a5/bin/
  install -m 0755 "$artifact_root/stage8b-r2a5-authority-issuer" /opt/moex-trading/stage8b-r2a5/bin/
  install -m 0755 "$artifact_root/stage8b-r2b-run-package-draft-builder" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/stage8b-r2a5-package-issuer" /opt/moex-trading/stage8b-r2a5/bin/
  install -m 0755 "$artifact_root/stage8b-r2b-launcher" /opt/moex-trading/stage8b-r2b/bin/
  install -m 0755 "$artifact_root/accepted-stage8b-readonly-preflight" /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight

  install -m 0644 "$repo_root"/deploy/stage8b-r2b/*.service /etc/systemd/system/
  install -m 0644 "$repo_root"/deploy/stage8b-r2b/*.target /etc/systemd/system/
  install -m 0644 "$repo_root"/deploy/stage8b-r2a5/stage8b-r2a5-{producer,issuer}@.service /etc/systemd/system/
  install -m 0644 "$repo_root/deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service" /etc/systemd/system/
  install -m 0644 "$repo_root/deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service" /etc/systemd/system/
  install -m 0644 "$repo_root/deploy/stage8b-r2b-proof/$trigger" "/etc/systemd/system/$trigger"
  systemctl daemon-reload
}

verify_installed_payload() {
  python3 - "$contract" "$artifact_root" <<'PY'
import hashlib,json,pathlib,sys
contract=json.loads(pathlib.Path(sys.argv[1]).read_text())
artifacts=pathlib.Path(sys.argv[2])
destinations={
 'stage8b-r2a8-upstream-current-authority-publisher':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-upstream-current-authority-publisher',
 'stage8b-r2a8-authoritative-intake-creator':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-authoritative-intake-creator',
 'stage8b-r2a8-production-intake-stager':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-intake-stager',
 'stage8b-r2a8-production-current-source-writer':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-current-source-writer',
 'stage8b-r2a8-current-manifest-issuer':'/opt/moex-trading/stage8b-r2a8/bin/stage8b-r2a8-current-manifest-issuer',
 'stage8b-r2a7-source-adapter':'/opt/moex-trading/stage8b-r2a7/bin/stage8b-r2a7-source-adapter',
 'stage8b-r2a5-authority-producer':'/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-producer',
 'stage8b-r2a5-authority-issuer':'/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-issuer',
 'stage8b-r2b-run-package-draft-builder':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-run-package-draft-builder',
 'stage8b-r2a5-package-issuer':'/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-package-issuer',
 'stage8b-r2b-launcher':'/opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-launcher',
 'accepted-stage8b-readonly-preflight':'/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight',
}
for name,expected in contract['production_linux_amd64_sha256'].items():
    source=artifacts/name
    target=pathlib.Path(destinations[name])
    if hashlib.sha256(source.read_bytes()).hexdigest()!=expected:
        raise RuntimeError(f'source binary drift: {name}')
    if hashlib.sha256(target.read_bytes()).hexdigest()!=expected:
        raise RuntimeError(f'installed binary drift: {name}')
for relative,expected in contract['unit_file_sha256'].items():
    target=pathlib.Path('/etc/systemd/system')/pathlib.Path(relative).name
    if hashlib.sha256(target.read_bytes()).hexdigest()!=expected:
        raise RuntimeError(f'installed unit drift: {relative}')
PY
}

materialize_run() {
  reset_transaction_namespace
  "$proof_tools/stage8b-r2a5-controlled-layout" seed-r2a6 PLACE

  # Prepare only the external C0 source. No R2B-owned Phase 1-6 output is
  # pre-created by the proof tooling.
  install -d -o 8094 -g 8095 -m 0750 /var/lib/moex-trading/stage8b/r2a7/production
  install -d -o 8094 -g 8095 -m 0750 /var/lib/moex-trading/stage8b/r2a8/intake
  printf '%s\n' "$(printf '5a%.0s' {1..32})" | install -o 8096 -g 8095 -m 0640 /dev/stdin \
    /var/lib/moex-trading/stage8b/r2a7/production/stage8b-r2a7-lifecycle-key.hex
  "$proof_tools/stage8b-r2b-creator-chain-seeder" >/dev/null
  chown -R 8095:8095 /var/lib/moex-trading/stage7b
  chown 8095:8094 /var/lib/moex-trading/stage8a1-authority
  chmod 0750 /var/lib/moex-trading/stage8a1-authority
  chown 8095:8095 /var/lib/moex-trading/stage8a1-authority/stage8a4-writer-issuer-signing-key.hex
  chmod 0600 /var/lib/moex-trading/stage8a1-authority/stage8a4-writer-issuer-signing-key.hex
  chown 8096:8095 /var/lib/moex-trading/stage8b/r2a7/production

  # Rebind the controlled transaction skeleton to accepted Generation 2.
  install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json" \
    /etc/moex-trading/stage8b/r2a5/trust-manifest.json
  install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json" \
    /etc/moex-trading/stage8b/r2a5/account-key-manifest.json
  install -m 0644 "$repo_root/docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-authority.json" \
    /etc/moex-trading/stage8b/r2a5/accepted-helper-authority.json
  install -m 0600 "$ceremony_root/package-authorization.ed25519" "$credential_root/package-authorization.ed25519"
  install -m 0600 "$ceremony_root/account-binding-generation-2.hex" "$credential_root/account-binding-keys/generation-2.hex"
  rm -f "$credential_root/account-binding-keys/generation-7.hex"

  python3 - "$repo_root/docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json" <<'PY'
import json,pathlib,sys
trust=json.loads(pathlib.Path(sys.argv[1]).read_text())
root=pathlib.Path('/etc/moex-trading/stage8b/r2a5/authority-public-keys')
root.mkdir(parents=True,exist_ok=True)
for source,key in trust['source_keys'].items():
    (root/f'{source}.ed25519.pub').write_text(key['public_key_ed25519_hex']+'\n')
PY

  for source in "${sources[@]}"; do
    uid="$(case "$source" in trusted_clock) echo 8201;; stage7b_current_recovery_seal) echo 8202;; stage6_exact_dispatch_ready_command) echo 8203;; stage8a_root_config_policy_control) echo 8204;; composite_readiness) echo 8205;; kill_switch_run_allowed) echo 8206;; single_finam_ownership) echo 8207;; schedule) echo 8208;; instrument_specification) echo 8209;; ambiguity_orphan_unresolved_lifecycle) echo 8210;; durable_micro_budget) echo 8211;; esac)"
    install -d -o "$uid" -g "$uid" -m 0700 "$credential_root/issuer-private-keys/$source"
    install -o "$uid" -g "$uid" -m 0600 "$ceremony_root/issuer-private-keys/$source/key.ed25519" \
      "$credential_root/issuer-private-keys/$source/key.ed25519"
  done

  python3 "$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_materialize_manifest.py"
  chmod 0711 "$credential_root"
  chown 8301:8301 "$credential_root/account-id" "$credential_root/finam-readonly-secret"
  chown -R 8301:8301 "$credential_root/account-binding-keys"
  chmod 0600 "$credential_root/account-id" "$credential_root/finam-readonly-secret" \
    "$credential_root/account-binding-keys/generation-2.hex"
  chmod 0700 "$credential_root/account-binding-keys"
  install -d -o root -g root -m 0700 "$terminal_root"
  systemctl daemon-reload
}

collect_run_evidence() {
  local supervisor_exit="$1"
  local terminal_file helper_log derived
  terminal_file="$(find "$terminal_root" -maxdepth 1 -type f -name 'r2b-terminal-*.json' -print -quit)"
  [[ -n "$terminal_file" ]]
  install -o root -g root -m 0400 "$terminal_file" "$run_root/root-terminal.redacted.json"
  helper_log="$run_root/helper-journal.redacted.txt"
  [[ "$supervisor_invocation_id" =~ ^[0-9a-f]{32}$ ]]
  journalctl _SYSTEMD_INVOCATION_ID="$supervisor_invocation_id" --no-pager -o cat \
    | grep -E 'stage8b-r2b-helper: (identity-verified|receipt-verified|authority-verified|credentials-loaded|terminal-sent)' \
    >"$helper_log"
  [[ -s "$helper_log" ]]
  derived="$run_root/request-boundary-proof.json"
  python3 "$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_terminal_oracle.py" \
    "$run_root/root-terminal.redacted.json" "$helper_log" "$derived"
  python3 - "$run_root/run-result.json" "$derived" "$supervisor_exit" "$run_id" <<'PY'
import json,pathlib,sys
proof=json.loads(pathlib.Path(sys.argv[2]).read_text())
payload={
 'schema_version':1,'run_id':sys.argv[4],'result':'PASS_EXPECTED_FAIL_CLOSED',
 'native_execution':True,'qemu_emulation':False,'systemd_pid1':True,
 'container_network_mode':'none','default_route':False,'dns':False,
 'phase_count':6,'service_invocation_count':31,'supervisor_exit_code':int(sys.argv[3]),
 'supervisor_invocation_id_bound':True,
 'request_boundary_proof':proof,'generation':2,'generation_2_active':False,
 'authorization':'NOT_ISSUED','external_finam_network':False,'broker_dispatch':False,
 'http_post_delete':False,'real_orders':False,
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload,indent=2,sort_keys=True)+'\n')
PY
}

run_transaction() {
  run_id="${1:?run id required}"
  run_root="$evidence_root/$run_id"
  [[ ! -e "$run_root" ]]
  install -d -o root -g root -m 0700 "$run_root"
  materialize_run
  set +e
  systemctl start "$trigger"
  supervisor_exit=$?
  set -e
  [[ "$supervisor_exit" -ne 0 ]]
  supervisor_invocation_id="$(systemctl show moex-stage8b-r2b-readonly-supervisor.service -p InvocationID --value)"
  collect_run_evidence "$supervisor_exit"
}

uninstall_payload() {
  reset_transaction_namespace
  rm -f "/etc/systemd/system/$trigger"
  for unit in "${production_units[@]}"; do rm -f "/etc/systemd/system/$unit"; done
  rm -f \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-upstream-current-authority-publisher \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-authoritative-intake-creator \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-intake-stager \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-current-source-writer \
    /opt/moex-trading/stage8b-r2a8/bin/stage8b-r2a8-current-manifest-issuer \
    /opt/moex-trading/stage8b-r2a7/bin/stage8b-r2a7-source-adapter \
    /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-producer \
    /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-issuer \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-run-package-draft-builder \
    /opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-package-issuer \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-launcher \
    /opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight
  systemctl daemon-reload
  systemctl reset-failed >/dev/null 2>&1 || true
  for unit in "${production_units[@]}" "$trigger"; do [[ ! -e "/etc/systemd/system/$unit" ]]; done
  [[ "$(find /opt/moex-trading -type f 2>/dev/null | wc -l)" -eq 0 ]]
  [[ ! -e /var/lib/moex-trading/stage8b ]]
  [[ ! -e /run/credentials/moex-trading/stage8b ]]
  printf '{"schema_version":1,"result":"PASS","units_remaining":0,"binaries_remaining":0,"transaction_state_files":0,"private_material_files":0,"authorization":"NOT_ISSUED"}\n' \
    >"$evidence_root/uninstall-receipt.json"
}

trap uninstall_payload EXIT
install_payload
verify_installed_payload
run_transaction run-1

# A clean second transaction is mandatory; the first materialization and all
# transaction-owned outputs are destroyed before this script is invoked again.
run_transaction run-2
uninstall_payload
trap - EXIT

echo "stage8b-generation2-full-transaction-container-run: PASS runs=2 graph=31 network=none authorization=NOT_ISSUED"
