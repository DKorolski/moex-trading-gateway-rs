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

echo "stage8b-generation2-native-r2-custody-runtime: PASS cases=7 synthetic_only=true container_created=false private_material=false"
