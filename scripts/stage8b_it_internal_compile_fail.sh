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
target="$repo_root/target/stage8b-it-internal-compile-fail"

check_fail() {
  local name="$1"
  local source="$2"
  cp "$original" "$lib"
  printf '\n%s\n' "$source" >>"$lib"
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

check_fail second_consuming_extraction '
mod stage8b_it_second_extraction_probe {
    fn probe(
        continuation: super::stage8a1_execution_capability::Stage8a1Stage8bBoundContinuation,
        sink: &mut super::Stage8a2InMemoryNoSendSink,
    ) {
        let _first = continuation.consume_stage8a2_request_capsule(sink);
        let _second = continuation.consume_stage8a2_request_capsule(sink);
    }
}'

echo "stage8b-it-internal-compile-fail: PASS negative=4"
