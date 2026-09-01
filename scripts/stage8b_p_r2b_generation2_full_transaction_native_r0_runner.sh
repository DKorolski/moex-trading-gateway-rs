#!/usr/bin/env bash
set -euo pipefail

readonly fixed_ceremony_root=/run/stage8b-g2-ceremony-source
readonly proof_container=stage8b-g2-native-proof-r2
extraction_root=""
trusted_evidence_root=""
container_creation_attempted=false
container_removed=false
ceremony_source_destroyed=false

destroy_fixed_ceremony_source() {
  local result=0
  if [[ -e "$fixed_ceremony_root" || -L "$fixed_ceremony_root" ]]; then
    rm -rf --one-file-system -- "$fixed_ceremony_root" || result=1
  fi
  if [[ -e "$fixed_ceremony_root" || -L "$fixed_ceremony_root" ]]; then
    result=1
  else
    ceremony_source_destroyed=true
  fi
  if [[ "$ceremony_source_destroyed" = true && -n "$trusted_evidence_root" && -d "$trusted_evidence_root" ]]; then
    python3 - "$trusted_evidence_root/ceremony-source-destruction-receipt.json" <<'PY' || result=1
import json,pathlib,sys
payload={
  "schema_version":2,"result":"PASS","fixed_source_path":True,
  "source_destroyed":True,"private_material_retained_on_host":False,
  "private_path_exported":False,"generation_2_active":False,
  "authorization":"NOT_ISSUED",
}
path=pathlib.Path(sys.argv[1])
path.write_text(json.dumps(payload,indent=2,sort_keys=True)+"\n",encoding="utf-8")
PY
  fi
  return "$result"
}

remove_proof_container() {
  local result=0
  if command -v docker >/dev/null 2>&1; then
    docker rm -f "$proof_container" >/dev/null 2>&1 || true
    if [[ -n "$(docker ps -aq --filter "name=^/${proof_container}$" 2>/dev/null || true)" ]]; then
      result=1
    else
      container_removed=true
    fi
  elif [[ "$container_creation_attempted" = true ]]; then
    result=1
  fi
  return "$result"
}

global_custody_cleanup() {
  local status=$?
  trap - EXIT INT TERM
  remove_proof_container || status=1
  destroy_fixed_ceremony_source || status=1
  if [[ -n "$extraction_root" && -e "$extraction_root" ]]; then
    rm -rf --one-file-system -- "$extraction_root" || status=1
  fi
  exit "$status"
}

trap global_custody_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

host_swap_entries="$(swapon --show --noheadings 2>/dev/null)"
[[ -z "$host_swap_entries" ]]

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" != "--reviewed-extraction" ]]; then
  review_archive="${STAGE8B_G2_REVIEW_ARCHIVE:?actual reviewed ZIP required}"
  accepted_archive_sha256="${STAGE8B_G2_ACCEPTED_ARCHIVE_SHA256:?accepted archive SHA-256 required}"
  reviewer_acceptance_sha256="${STAGE8B_G2_REVIEWER_ACCEPTANCE_SHA256:?review acceptance SHA-256 required}"
  evidence_root="${STAGE8B_G2_EVIDENCE_ROOT:?empty evidence root required}"
  [[ -d "$evidence_root" && -z "$(find "$evidence_root" -mindepth 1 -print -quit)" ]]
  trusted_evidence_root="$evidence_root"
  [[ "$(findmnt -n -o FSTYPE -T /run)" = tmpfs ]]
  extraction_root="$(mktemp -d /run/stage8b-g2-reviewed-extraction.XXXXXX)"
  python3 "$script_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r1_review_archive.py" \
    --archive "$review_archive" --expected-sha256 "$accepted_archive_sha256" \
    --reviewer-acceptance-sha256 "$reviewer_acceptance_sha256" \
    --extraction-root "$extraction_root" --output "$evidence_root/archive-binding.json"
  bash "$extraction_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_runner.sh" \
    --reviewed-extraction "$extraction_root" "$evidence_root/archive-binding.json"
  exit $?
fi

repo_root="${2:?fresh reviewed extraction required}"
archive_binding="${3:?archive binding receipt required}"
attestation="${STAGE8B_G2_HOST_ATTESTATION:?host attestation required}"
ceremony_root="${STAGE8B_R2B_PHASE6_CEREMONY_DIR:?temporary ceremony root required}"
evidence_root="${STAGE8B_G2_EVIDENCE_ROOT:?evidence root required}"
[[ "$repo_root" = /run/stage8b-g2-reviewed-extraction.* ]]
[[ "$ceremony_root" = "$fixed_ceremony_root" ]]
[[ "${STAGE8B_G2_DESTROY_CEREMONY_SOURCE:?ceremony destruction authorization required}" = YES ]]
[[ -d "$evidence_root" ]]
trusted_evidence_root="$evidence_root"

artifact_root="$repo_root/handoff-evidence/linux-amd64/exact-binaries"
proof_tools="$repo_root/handoff-evidence/linux-amd64/proof-tools"
container="$proof_container"
image_tag="stage8b-r2b-r0-r1a-systemd:ubuntu24.04-amd64"
image_id="sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526"
ceremony_container_parent=/var/lib/stage8b-g2-proof-ceremony
ceremony_container_root="$ceremony_container_parent/ceremony"
[[ "$(uname -m)" = x86_64 ]]
[[ "$(docker info --format '{{.Architecture}}')" = x86_64 || "$(docker info --format '{{.Architecture}}')" = amd64 ]]
[[ ! -e /var/lib/stage8b-generation2-active ]]
[[ -x "$proof_tools/stage8b-r2a5-controlled-layout" ]]
[[ -x "$proof_tools/stage8b-r2b-creator-chain-seeder" ]]
[[ -x "$proof_tools/stage8b-r2b-trust-rebind-key-ceremony-verify" ]]
actual_image_id="$(docker image inspect --format '{{.Id}}' "$image_tag")"
[[ "$actual_image_id" = "$image_id" ]]
[[ "$(docker image inspect --format '{{.Id}}' "$image_id")" = "$image_id" ]]

python3 "$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py" \
  --root "$repo_root" --attestation "$attestation" --archive-binding "$archive_binding" \
  --artifact-root "$artifact_root" --proof-tools-root "$proof_tools" \
  --ceremony-root "$ceremony_root" --output "$evidence_root/host-preflight.json"

[[ "$(docker ps -aq | wc -l | tr -d ' ')" = 0 ]]
container_creation_attempted=true
docker create --privileged --cgroupns=host --name "$container" \
  --network none \
  --tmpfs /run:rw,nosuid,nodev,mode=755 \
  --tmpfs "$ceremony_container_parent:rw,nosuid,nodev,noexec,mode=0700" \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  -v "$repo_root:/work:ro" \
  -v "$artifact_root:/artifacts:ro" \
  -v "$proof_tools:/proof-tools:ro" \
  -v "$ceremony_root:/ceremony-source:ro" \
  -v "$evidence_root:/evidence:rw" \
  "$image_id" >/dev/null
docker start "$container" >/dev/null

for _ in $(seq 1 180); do
  state="$(docker exec "$container" systemctl is-system-running 2>/dev/null || true)"
  [[ "$state" = running || "$state" = degraded ]] && break
  [[ "$(docker inspect -f '{{.State.Status}}' "$container")" = running ]]
  sleep 1
done
[[ "$(docker exec "$container" uname -m)" = x86_64 ]]
docker exec "$container" bash -lc '! ip route show default | grep -q .; : > /etc/resolv.conf'
container_swap_entries="$(docker exec "$container" awk 'NR > 1 && NF > 0 { count += 1 } END { print count + 0 }' /proc/swaps)"
[[ "$container_swap_entries" = 0 ]]
python3 - "$evidence_root/swap-custody-preflight.json" <<'PY'
import json,pathlib,sys
payload={
  "schema_version":1,"result":"PASS",
  "host_swap_enabled":False,"host_swap_entries":0,
  "container_visible_swap_enabled":False,"container_visible_swap_entries":0,
  "ceremony_copied_to_container":False,"authorization":"NOT_ISSUED",
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload,indent=2,sort_keys=True)+"\n",encoding="utf-8")
PY

docker exec "$container" install -d -o root -g root -m 0700 "$ceremony_container_root"
docker exec "$container" cp -a /ceremony-source/. "$ceremony_container_root/"
docker exec "$container" chown -R root:root "$ceremony_container_root"
docker exec "$container" find "$ceremony_container_root" -type d -exec chmod 0700 '{}' +
docker exec "$container" find "$ceremony_container_root" -type f -exec chmod 0600 '{}' +
docker exec "$container" chmod 0644 "$ceremony_container_root/trust-manifest.json" "$ceremony_container_root/account-key-manifest.json"

source_ref="$(python3 - "$archive_binding" <<'PY'
import json,pathlib,sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text())
if not isinstance(value.get("source_ref"),str) or len(value["source_ref"]) != 40:
    raise SystemExit("invalid source ref")
print(value["source_ref"])
PY
)"
verified_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
docker exec \
  -e STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR="$ceremony_container_root" \
  -e STAGE8B_R2B_TRUST_REBIND_SOURCE_REF="$source_ref" \
  -e STAGE8B_R2B_TRUST_REBIND_VERIFIED_AT_UTC="$verified_at" \
  -e STAGE8B_R2B_TRUST_REBIND_VERIFIER_SOURCE_SHA256=d8b6173c65d87ad1ff0c6b202645335c2cf9fcad76a8b44b2a551a3f494af8f5 \
  "$container" /proof-tools/stage8b-r2b-trust-rebind-key-ceremony-verify \
  >"$evidence_root/ceremony-verification-receipt.json"
python3 - "$evidence_root/ceremony-verification-receipt.json" "$source_ref" <<'PY'
import json,pathlib,sys
receipt=json.loads(pathlib.Path(sys.argv[1]).read_text())
required={
  "verification_status":"PASS","generation":2,"source_ref":sys.argv[2],
  "exact_inventory_verified":True,"owner_verified":True,"directory_modes_verified":True,
  "file_modes_verified":True,"single_link_verified":True,"symlink_rejection_verified":True,
  "private_public_bindings_verified":13,"account_key_binding_verified":True,
  "private_path_recorded":False,"private_values_exported":False,
}
for key,expected in required.items():
    if receipt.get(key) != expected:
        raise SystemExit(f"ceremony receipt mismatch: {key}")
PY

docker exec "$container" bash /work/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_container_run.sh \
  /work /artifacts /proof-tools "$ceremony_container_root" /evidence

docker exec "$container" test -f /evidence/run-1/run-result.json
docker exec "$container" test -f /evidence/run-2/run-result.json
docker exec "$container" test -f /evidence/uninstall-receipt.json
docker exec "$container" rm -rf --one-file-system "$ceremony_container_root"
docker rm -f "$container" >/dev/null
container_removed=true
destroy_fixed_ceremony_source

python3 - "$evidence_root" <<'PY'
import hashlib,json,pathlib,sys
root=pathlib.Path(sys.argv[1])
runs=[json.loads((root/f"run-{n}"/"run-result.json").read_text()) for n in (1,2)]
for result in runs:
    if result.get("result") != "PASS_EXPECTED_FAIL_CLOSED":
        raise SystemExit("run result drift")
    if result.get("authorization") != "NOT_ISSUED" or result.get("generation_2_active") is not False:
        raise SystemExit("run authorization boundary opened")
    proof=result.get("request_boundary_proof",{})
    if proof.get("actual_read_attempts") is not True or proof.get("attempt_count") != 1:
        raise SystemExit("request proof drift")
receipt=json.loads((root/"uninstall-receipt.json").read_text())
if receipt.get("result") != "PASS" or receipt.get("authorization") != "NOT_ISSUED":
    raise SystemExit("uninstall receipt drift")
destruction=json.loads((root/"ceremony-source-destruction-receipt.json").read_text())
if destruction.get("source_destroyed") is not True:
    raise SystemExit("ceremony destruction missing")
swap=json.loads((root/"swap-custody-preflight.json").read_text())
if swap.get("host_swap_enabled") is not False or swap.get("container_visible_swap_enabled") is not False:
    raise SystemExit("swap custody preflight missing")
payload={
 "schema_version":3,"stage":"Stage 8B-P R2B Generation-2 native full transaction proof R2",
 "result":"PASS","native_execution":True,"qemu_emulation":False,"run_count":2,
 "clean_second_run":True,"reset_between_runs":True,"container_destroyed":True,
 "uninstall_verified":True,"ceremony_source_destroyed":True,"generation":2,
 "generation_2_active":False,"authorization":"NOT_ISSUED","external_finam_network":False,
 "broker_dispatch":False,"http_post_delete":False,"real_orders":False,
 "failure_replay_proof":"INHERITED_ACCEPTED_IMPLEMENTATION_R0_R1A",
 "run_result_sha256":[hashlib.sha256((root/f"run-{n}"/"run-result.json").read_bytes()).hexdigest() for n in (1,2)],
 "uninstall_receipt_sha256":hashlib.sha256((root/"uninstall-receipt.json").read_bytes()).hexdigest(),
 "ceremony_verification_receipt_sha256":hashlib.sha256((root/"ceremony-verification-receipt.json").read_bytes()).hexdigest(),
 "ceremony_destruction_receipt_sha256":hashlib.sha256((root/"ceremony-source-destruction-receipt.json").read_bytes()).hexdigest(),
 "swap_custody_preflight_sha256":hashlib.sha256((root/"swap-custody-preflight.json").read_bytes()).hexdigest(),
}
(root/"aggregate-result.json").write_text(json.dumps(payload,indent=2,sort_keys=True)+"\n")
PY

[[ "$container_removed" = true ]]
[[ "$(docker ps -aq | wc -l | tr -d ' ')" = 0 ]]
echo "stage8b-generation2-full-transaction-native-r2-runner: PASS native=true runs=2 no_swap=true container_destroyed=true authorization=NOT_ISSUED"
