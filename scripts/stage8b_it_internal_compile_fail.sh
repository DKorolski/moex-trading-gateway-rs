#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-it-internal.XXXXXX")"
trap 'rm -rf "$work"' EXIT
candidate="$work/repo"
mkdir -p "$candidate"

while IFS= read -r -d '' relative; do
  [[ -f "$repo_root/$relative" ]] || continue
  mkdir -p "$candidate/$(dirname "$relative")"
  cp "$repo_root/$relative" "$candidate/$relative"
done < <(cd "$repo_root" && git ls-files --cached --others --exclude-standard -z)

lib="$candidate/crates/finam-gateway/src/lib.rs"
original="$work/lib.rs"
cp "$lib" "$original"
adapter="$candidate/crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs"
adapter_original="$work/stage8b_adapter.rs"
cp "$adapter" "$adapter_original"
target="$repo_root/target/stage8b-it-internal-compile-fail"

check_fail() {
  local name="$1"
  local source="$2"
  cp "$original" "$lib"
  cp "$adapter_original" "$adapter"
  printf '\n%s\n' "$source" >>"$lib"
  if CARGO_TARGET_DIR="$target" cargo check --quiet \
      --manifest-path "$candidate/Cargo.toml" -p finam-gateway \
      >"$work/$name.log" 2>&1; then
    echo "stage8b-it-internal-compile-fail: FAIL $name compiled" >&2
    exit 1
  fi
  echo "PASS $name"
}

check_adapter_fail() {
  local name="$1"
  local source="$2"
  cp "$original" "$lib"
  cp "$adapter_original" "$adapter"
  printf '\n%s\n' "$source" >>"$adapter"
  if CARGO_TARGET_DIR="$target" cargo check --quiet \
      --manifest-path "$candidate/Cargo.toml" -p finam-gateway \
      >"$work/$name.log" 2>&1; then
    echo "stage8b-it-internal-compile-fail: FAIL $name compiled" >&2
    exit 1
  fi
  echo "PASS $name"
}

check_fail sibling_request_capsule_access '
mod stage8b_it_sibling_request_capsule_probe {
    fn probe(_: super::stage8b_no_send::Stage8bApprovedRequestParts) {}
}'

check_fail sibling_adapter_access '
mod stage8b_it_sibling_adapter_probe {
    fn probe(_: super::stage8b_no_send::stage8b_adapter::Stage8bItAdapter) {}
}'

check_fail raw_observation_escape '
mod stage8b_it_raw_observation_probe {
    fn probe(_: super::stage8b_no_send::stage8b_adapter::Stage8bItRawObservation) {}
}'

check_fail extraction_without_k4_proof '
mod stage8b_it_no_k4_proof_probe {
    fn probe(
        continuation: super::stage8a1_execution_capability::Stage8a1Stage8bBoundContinuation,
        sink: &mut super::Stage8a2InMemoryNoSendSink,
    ) {
        let _ = continuation.consume_stage8a2_request_capsule(sink);
    }
}'

check_adapter_fail adapter_cannot_construct_permit_proof '
fn forge_stage8b_permit_proof() -> super::stage8b_permit_capsule::Stage8bA2PermitProof {
    super::stage8b_permit_capsule::Stage8bA2PermitProof {
        permit_binding_sha256: String::new(),
        durable_binding_sha256: String::new(),
        continuation_binding_sha256: String::new(),
        exact_attempt_sha256: String::new(),
        covering_seal_sha256: String::new(),
    }
}'

check_adapter_fail adapter_cannot_construct_request_parts '
fn forge_stage8b_request_parts(
    proof: super::stage8b_permit_capsule::Stage8bA2PermitProof,
    diagnostic: crate::Stage8a2BuilderCompositionDiagnostic,
    request: super::stage8b_permit_capsule::Stage8bPrivateRequestSpec,
) -> super::stage8b_permit_capsule::Stage8bApprovedRequestParts {
    super::stage8b_permit_capsule::Stage8bApprovedRequestParts {
        proof,
        diagnostic,
        request,
    }
}'

echo "stage8b-it-internal-compile-fail: PASS negative=6"
