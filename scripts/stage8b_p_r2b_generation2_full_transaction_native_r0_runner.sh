#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
attestation="${STAGE8B_G2_HOST_ATTESTATION:?host attestation required}"
handoff_commit="${STAGE8B_G2_HANDOFF_COMMIT:?handoff commit marker required}"
archive_sha256="${STAGE8B_G2_ARCHIVE_SHA256:?reviewed archive sha256 required}"
artifact_root="${STAGE8B_G2_ARTIFACT_ROOT:?exact artifact root required}"
proof_tools="${STAGE8B_G2_PROOF_TOOLS_ROOT:?proof tool root required}"
ceremony_root="${STAGE8B_R2B_PHASE6_CEREMONY_DIR:?temporary ceremony root required}"
evidence_root="${STAGE8B_G2_EVIDENCE_ROOT:?empty evidence root required}"
container="stage8b-g2-native-proof-${STAGE8B_G2_RUN_LABEL:-r0}"
image=stage8b-g2-native-proof-systemd:ubuntu24.04-amd64

[[ "$(uname -m)" = x86_64 ]]
[[ "$(docker info --format '{{.Architecture}}')" = x86_64 || "$(docker info --format '{{.Architecture}}')" = amd64 ]]
[[ -d "$evidence_root" && -z "$(find "$evidence_root" -mindepth 1 -print -quit)" ]]
[[ ! -e "/var/lib/stage8b-generation2-active" ]]
[[ -x "$proof_tools/stage8b-r2a5-controlled-layout" ]]
[[ -x "$proof_tools/stage8b-r2b-creator-chain-seeder" ]]

# This cryptographic and host eligibility preflight is intentionally before
# docker create. It emits no private path or value.
python3 "$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py" \
  --root "$repo_root" --attestation "$attestation" --handoff-commit "$handoff_commit" \
  --archive-sha256 "$archive_sha256" --artifact-root "$artifact_root" \
  --proof-tools-root "$proof_tools" \
  --ceremony-root "$ceremony_root" --output "$evidence_root/host-preflight.json"

[[ "$(docker ps -aq --filter "name=^/${container}$" | wc -l | tr -d ' ')" = 0 ]]
docker image inspect "$image" >/dev/null

cleanup() {
  docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker create --privileged --cgroupns=host --name "$container" \
  --network none \
  --tmpfs /run:rw,nosuid,nodev,mode=755 \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  -v "$repo_root:/work:ro" \
  -v "$artifact_root:/artifacts:ro" \
  -v "$proof_tools:/proof-tools:ro" \
  -v "$ceremony_root:/ceremony-source:ro" \
  -v "$evidence_root:/evidence:rw" \
  "$image" >/dev/null
docker start "$container" >/dev/null

for _ in $(seq 1 180); do
  state="$(docker exec "$container" systemctl is-system-running 2>/dev/null || true)"
  [[ "$state" = running || "$state" = degraded ]] && break
  [[ "$(docker inspect -f '{{.State.Status}}' "$container")" = running ]]
  sleep 1
done
docker exec "$container" test "$(docker exec "$container" uname -m)" = x86_64
docker exec "$container" bash -lc '! ip route show default | grep -q .; : > /etc/resolv.conf'

# Private source is copied only into the container tmpfs. The bind-mounted
# source is never mutated and disappears when the container is destroyed.
docker exec "$container" install -d -o root -g root -m 0700 /run/stage8b-g2-ceremony
docker exec "$container" cp -a /ceremony-source/. /run/stage8b-g2-ceremony/
docker exec "$container" chmod -R go-rwx /run/stage8b-g2-ceremony

docker exec "$container" bash /work/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_container_run.sh \
  /work /artifacts /proof-tools /run/stage8b-g2-ceremony /evidence

# The inner runner creates run-1 and run-2, proves reset and uninstalls every
# exact payload. The outer runner independently destroys the only container.
docker exec "$container" test -f /evidence/run-1/run-result.json
docker exec "$container" test -f /evidence/run-2/run-result.json
docker exec "$container" test -f /evidence/uninstall-receipt.json
docker exec "$container" rm -rf /run/stage8b-g2-ceremony
docker rm -f "$container" >/dev/null
trap - EXIT

python3 - "$evidence_root" <<'PY'
import hashlib,json,pathlib,sys
root=pathlib.Path(sys.argv[1])
runs=[json.loads((root/f'run-{n}'/'run-result.json').read_text()) for n in (1,2)]
assert all(r['result']=='PASS_EXPECTED_FAIL_CLOSED' for r in runs)
assert all(r['authorization']=='NOT_ISSUED' and r['generation_2_active'] is False for r in runs)
proofs=[r['request_boundary_proof'] for r in runs]
assert all(p['actual_read_attempts'] is True and p['attempt_count']==1 for p in proofs)
receipt=json.loads((root/'uninstall-receipt.json').read_text())
assert receipt['result']=='PASS' and receipt['authorization']=='NOT_ISSUED'
payload={
 'schema_version':1,'stage':'Stage 8B-P R2B Generation-2 native full transaction proof',
 'result':'PASS','native_execution':True,'qemu_emulation':False,'run_count':2,
 'clean_second_run':True,'reset_between_runs':True,'container_destroyed':True,
 'uninstall_verified':True,'generation':2,'generation_2_active':False,
 'authorization':'NOT_ISSUED','external_finam_network':False,'broker_dispatch':False,
 'http_post_delete':False,'real_orders':False,
 'run_result_sha256':[hashlib.sha256((root/f'run-{n}'/'run-result.json').read_bytes()).hexdigest() for n in (1,2)],
 'uninstall_receipt_sha256':hashlib.sha256((root/'uninstall-receipt.json').read_bytes()).hexdigest(),
}
(root/'aggregate-result.json').write_text(json.dumps(payload,indent=2,sort_keys=True)+'\n')
PY

[[ "$(docker ps -aq --filter "name=^/${container}$" | wc -l | tr -d ' ')" = 0 ]]
echo "stage8b-generation2-full-transaction-native-r0-runner: PASS native=true runs=2 container_destroyed=true authorization=NOT_ISSUED"
