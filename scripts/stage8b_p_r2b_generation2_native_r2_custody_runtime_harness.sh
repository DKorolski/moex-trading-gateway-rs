#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Linux || "$(id -u)" != 0 ]]; then
  echo "stage8b-generation2-native-r2-custody-runtime: FAIL root Linux required" >&2
  exit 1
fi

readonly fixed_source=/run/stage8b-g2-ceremony-source
readonly fixed_container=stage8b-g2-native-proof-r2
readonly synthetic_marker=SYNTHETIC_STAGE8B_CUSTODY_MARKER_NOT_A_PRIVATE_KEY
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner="$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_runner.sh"
review_archive="${1:?review archive required}"
expected_archive_sha256="${2:?review archive SHA-256 required}"
reviewer_acceptance_sha256="${3:?reviewer acceptance SHA-256 required}"

[[ -f "$review_archive" ]]
[[ "$expected_archive_sha256" =~ ^[0-9a-f]{64}$ ]]
[[ "$reviewer_acceptance_sha256" =~ ^[0-9a-f]{64}$ ]]
[[ "$(findmnt -n -o FSTYPE -T /run)" = tmpfs ]]
[[ -z "$(swapon --show --noheadings)" ]]
[[ ! -e "$fixed_source" ]]
[[ -z "$(docker ps -aq --filter "name=^/${fixed_container}$")" ]]

working="$(mktemp -d /run/stage8b-g2-custody-runtime.XXXXXX)"
cleanup_harness() {
  docker rm -f "$fixed_container" >/dev/null 2>&1 || true
  rm -rf --one-file-system -- "$fixed_source" /run/stage8b-g2-reviewed-extraction.synthetic "$working"
}
trap cleanup_harness EXIT INT TERM

seed_synthetic_source() {
  [[ ! -e "$fixed_source" ]]
  install -d -o root -g root -m 0700 "$fixed_source"
  printf '%s\n' "$synthetic_marker" >"$fixed_source/synthetic-only.marker"
  chmod 0600 "$fixed_source/synthetic-only.marker"
}

verify_failed_and_clean() {
  local name="$1" status="$2" output="$3"
  [[ "$status" -ne 0 ]]
  [[ ! -e "$fixed_source" ]]
  [[ -z "$(docker ps -aq --filter "name=^/${fixed_container}$")" ]]
  ! grep -Fq "$synthetic_marker" "$output"
  echo "PASS $name"
}

run_outer_missing_environment() {
  local output="$working/outer-missing-environment.log" status=0
  seed_synthetic_source
  env -i PATH="$PATH" bash "$runner" >"$output" 2>&1 || status=$?
  verify_failed_and_clean outer-missing-environment "$status" "$output"
}

run_nonempty_evidence_root() {
  local evidence="$working/nonempty-evidence" output="$working/nonempty-evidence.log" status=0
  mkdir "$evidence"; touch "$evidence/already-present"
  seed_synthetic_source
  STAGE8B_G2_REVIEW_ARCHIVE="$review_archive" \
  STAGE8B_G2_ACCEPTED_ARCHIVE_SHA256="$expected_archive_sha256" \
  STAGE8B_G2_REVIEWER_ACCEPTANCE_SHA256="$reviewer_acceptance_sha256" \
  STAGE8B_G2_EVIDENCE_ROOT="$evidence" \
  bash "$runner" >"$output" 2>&1 || status=$?
  verify_failed_and_clean nonempty-evidence-root "$status" "$output"
}

run_archive_case() {
  local name="$1" archive="$2" expected="$3"
  local evidence="$working/$name-evidence" output="$working/$name.log" status=0
  mkdir "$evidence"
  seed_synthetic_source
  STAGE8B_G2_REVIEW_ARCHIVE="$archive" \
  STAGE8B_G2_ACCEPTED_ARCHIVE_SHA256="$expected" \
  STAGE8B_G2_REVIEWER_ACCEPTANCE_SHA256="$reviewer_acceptance_sha256" \
  STAGE8B_G2_EVIDENCE_ROOT="$evidence" \
  bash "$runner" >"$output" 2>&1 || status=$?
  verify_failed_and_clean "$name" "$status" "$output"
}

run_inner_environment_cases() {
  local evidence="$working/inner-evidence" binding="$working/archive-binding.json"
  local output="$working/inner-missing.log" status=0
  mkdir "$evidence" /run/stage8b-g2-reviewed-extraction.synthetic
  printf '{}\n' >"$binding"
  seed_synthetic_source
  env -i PATH="$PATH" bash "$runner" --reviewed-extraction \
    /run/stage8b-g2-reviewed-extraction.synthetic "$binding" >"$output" 2>&1 || status=$?
  verify_failed_and_clean inner-env-validation "$status" "$output"

  output="$working/wrong-path.log"; status=0; seed_synthetic_source
  STAGE8B_G2_HOST_ATTESTATION="$working/missing-attestation.json" \
  STAGE8B_R2B_PHASE6_CEREMONY_DIR=/run/wrong-ceremony-source \
  STAGE8B_G2_EVIDENCE_ROOT="$evidence" STAGE8B_G2_DESTROY_CEREMONY_SOURCE=YES \
  bash "$runner" --reviewed-extraction \
    /run/stage8b-g2-reviewed-extraction.synthetic "$binding" >"$output" 2>&1 || status=$?
  verify_failed_and_clean wrong-ceremony-path-cleans-fixed-source "$status" "$output"
  rmdir /run/stage8b-g2-reviewed-extraction.synthetic
}

fakebin="$working/fakebin"
install -d -m 0755 "$fakebin"
cat >"$fakebin/docker" <<'SH'
#!/usr/bin/env bash
set -eu
behavior="${FAKE_DOCKER_BEHAVIOR:?}"
state="${FAKE_DOCKER_STATE:?}"
command_name="${1:-}"; shift || true
case "$command_name" in
  info)
    [[ "$behavior" != daemon-unavailable ]] || exit 125
    printf 'amd64\n'
    ;;
  ps)
    count=0
    [[ ! -f "$state" ]] || count="$(cat "$state")"
    count=$((count + 1)); printf '%s\n' "$count" >"$state"
    case "$behavior" in
      absent) ;;
      ps-error|daemon-unavailable) exit 125 ;;
      ps-timeout) sleep 20 ;;
      post-ps-error) [[ "$count" -eq 1 ]] && printf 'synthetic-container-id\n' || exit 125 ;;
      rm-success) if [[ "$count" -eq 1 ]]; then printf 'synthetic-container-id\n'; fi ;;
      rm-error|rm-timeout|still-present|rm-block) printf 'synthetic-container-id\n' ;;
      *) exit 126 ;;
    esac
    ;;
  rm)
    case "$behavior" in
      rm-error) exit 125 ;;
      rm-timeout) sleep 20 ;;
      rm-block) touch "${FAKE_DOCKER_RM_ENTERED:?}"; sleep 20 ;;
      rm-success|still-present|post-ps-error) ;;
      *) exit 126 ;;
    esac
    ;;
  *) exit 125 ;;
esac
SH
chmod 0755 "$fakebin/docker"

verify_cleanup_receipt() {
  python3 - "$1" "$2" "$3" "$4" "$5" "$6" <<'PY'
import json,pathlib,sys
receipt=json.loads(pathlib.Path(sys.argv[1]).read_text())
expected={
  "result":sys.argv[2],
  "container_state_known":sys.argv[3] == "true",
  "container_removed":sys.argv[4] == "true",
  "private_material_retained_on_host":sys.argv[5] == "true",
  "vps_destruction_required":sys.argv[6] == "true",
  "host_source_destroyed":True,
  "authorization":"NOT_ISSUED",
}
for key,value in expected.items():
    if receipt.get(key) != value:
        raise SystemExit(f"cleanup receipt mismatch {key}: {receipt.get(key)!r} != {value!r}")
if receipt.get("private_material_retained_on_host") is False and not (
    receipt.get("host_source_destroyed") is True
    and receipt.get("container_state_known") is True
    and receipt.get("container_removed") is True
):
    raise SystemExit("unsafe private-retained=false")
PY
}

run_fake_docker_cleanup_case() {
  local name="$1" behavior="$2" receipt_result="$3" known="$4" removed="$5" retained="$6" destroy_vps="$7"
  local evidence="$working/fake-$name-evidence" output="$working/fake-$name.log" state="$working/fake-$name.state" status=0
  mkdir "$evidence"
  seed_synthetic_source
  PATH="$fakebin:$PATH" FAKE_DOCKER_BEHAVIOR="$behavior" FAKE_DOCKER_STATE="$state" \
  STAGE8B_G2_HOST_ATTESTATION="$working/missing-attestation.json" \
  STAGE8B_R2B_PHASE6_CEREMONY_DIR="$fixed_source" \
  STAGE8B_G2_EVIDENCE_ROOT="$evidence" STAGE8B_G2_DESTROY_CEREMONY_SOURCE=YES \
  bash "$runner" --reviewed-extraction \
    /run/stage8b-g2-reviewed-extraction.synthetic "$working/archive-binding.json" >"$output" 2>&1 || status=$?
  [[ "$status" -ne 0 ]]
  [[ ! -e "$fixed_source" ]]
  ! grep -Fq "$synthetic_marker" "$output"
  verify_cleanup_receipt "$evidence/ceremony-source-destruction-receipt.json" \
    "$receipt_result" "$known" "$removed" "$retained" "$destroy_vps"
  echo "PASS $name"
}

run_blocking_cleanup_order_case() {
  local evidence="$working/fake-blocking-evidence" output="$working/fake-blocking.log"
  local state="$working/fake-blocking.state" entered="$working/fake-blocking.entered" status=0
  mkdir "$evidence"
  seed_synthetic_source
  PATH="$fakebin:$PATH" FAKE_DOCKER_BEHAVIOR=rm-block FAKE_DOCKER_STATE="$state" FAKE_DOCKER_RM_ENTERED="$entered" \
  STAGE8B_G2_HOST_ATTESTATION="$working/missing-attestation.json" \
  STAGE8B_R2B_PHASE6_CEREMONY_DIR="$fixed_source" \
  STAGE8B_G2_EVIDENCE_ROOT="$evidence" STAGE8B_G2_DESTROY_CEREMONY_SOURCE=YES \
  bash "$runner" --reviewed-extraction \
    /run/stage8b-g2-reviewed-extraction.synthetic "$working/archive-binding.json" >"$output" 2>&1 &
  local pid=$!
  for _ in $(seq 1 100); do [[ -e "$entered" ]] && break; sleep 0.1; done
  [[ -e "$entered" ]]
  [[ ! -e "$fixed_source" ]]
  wait "$pid" || status=$?
  [[ "$status" -ne 0 ]]
  verify_cleanup_receipt "$evidence/ceremony-source-destruction-receipt.json" FAIL true false true true
  echo "PASS docker-cleanup-does-not-block-host-source-destruction"
}

run_outer_missing_environment
run_nonempty_evidence_root
run_archive_case archive-sha-failure "$review_archive" "$(printf '0%.0s' {1..64})"

python3 - "$review_archive" "$working/archive-safety.zip" "$working/source-manifest.zip" <<'PY'
import shutil,sys,zipfile
source,safety,manifest=sys.argv[1:]
shutil.copyfile(source,safety)
with zipfile.ZipFile(safety,"a") as archive:
    archive.writestr("../synthetic-traversal",b"public synthetic data\n")
shutil.copyfile(source,manifest)
with zipfile.ZipFile(manifest,"a") as archive:
    archive.writestr("unexpected-public-member.txt",b"public synthetic data\n")
PY
run_archive_case archive-safety-failure "$working/archive-safety.zip" "$(sha256sum "$working/archive-safety.zip" | awk '{print $1}')"
run_archive_case source-manifest-additional-member "$working/source-manifest.zip" "$(sha256sum "$working/source-manifest.zip" | awk '{print $1}')"

# The real archive and digest pass complete outer verification. The recursive
# invocation intentionally lacks host attestation, so no proof container or
# private-key processing can begin.
run_archive_case reviewed-archive-positive-to-inner-fail-closed "$review_archive" "$expected_archive_sha256"
run_inner_environment_cases

printf '{}\n' >"$working/archive-binding.json"
run_fake_docker_cleanup_case docker-absent-proven absent PASS true true false false
run_fake_docker_cleanup_case docker-rm-success rm-success PASS true true false false
run_fake_docker_cleanup_case docker-rm-command-error rm-error FAIL true false true true
run_fake_docker_cleanup_case docker-ps-command-error ps-error FAIL false false true true
run_fake_docker_cleanup_case docker-daemon-unavailable daemon-unavailable FAIL false false true true
run_fake_docker_cleanup_case docker-rm-timeout rm-timeout FAIL true false true true
run_fake_docker_cleanup_case docker-ps-timeout ps-timeout FAIL false false true true
run_fake_docker_cleanup_case container-still-present-after-rm still-present FAIL true false true true
run_fake_docker_cleanup_case post-rm-container-state-unknown post-ps-error FAIL false false true true
run_blocking_cleanup_order_case

echo "stage8b-generation2-native-r2a-custody-runtime: PASS cases=18/18 synthetic_only=true real_proof_container_created=false private_material=false"
