#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-i-external.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"

cat >"$work/Cargo.toml" <<EOF
[package]
name = "stage8b-i-external-boundary"
version = "0.0.0"
edition = "2021"

[dependencies]
broker-cli = { path = "$repo_root/crates/broker-cli" }
finam-gateway = { path = "$repo_root/crates/finam-gateway" }
serde_json = "1"
EOF

check_pass() {
  printf '%s\n' "$1" >"$work/src/main.rs"
  cargo check --quiet --manifest-path "$work/Cargo.toml"
}

check_fail() {
  local name="$1"
  local source="$2"
  printf '%s\n' "$source" >"$work/src/main.rs"
  if cargo check --quiet --manifest-path "$work/Cargo.toml" >"$work/$name.log" 2>&1; then
    echo "stage8b-i-external-compile-fail: FAIL $name compiled" >&2
    exit 1
  fi
}

check_pass 'use broker_cli::invoke_stage8b_no_send_from_cli;
use finam_gateway::Stage8bOperatorInvocationRequest;
fn main() {
    let request = Stage8bOperatorInvocationRequest::new(
        "INVOCATION_COMPILE_0001", "/absolute/package", "/absolute/manifest-root"
    );
    let _ = invoke_stage8b_no_send_from_cli;
    let _ = request;
}'

check_fail private_module 'use finam_gateway::stage8b_no_send;
fn main() { let _ = stage8b_no_send::compose_stage8b_effect_authority; }'
check_fail private_root 'use finam_gateway::compose_stage8b_effect_authority;
fn main() { let _ = compose_stage8b_effect_authority; }'
check_fail private_build 'use finam_gateway::Stage8bExecutionQualifiedBuild;
fn main() { let _ = std::mem::size_of::<Stage8bExecutionQualifiedBuild>(); }'
check_fail private_binding 'use finam_gateway::Stage8bKeyedAccountBinding;
fn main() { let _ = std::mem::size_of::<Stage8bKeyedAccountBinding>(); }'
check_fail private_arm 'use finam_gateway::Stage8bAuthenticatedOperatorArm;
fn main() { let _ = std::mem::size_of::<Stage8bAuthenticatedOperatorArm>(); }'
check_fail private_permit 'use finam_gateway::Stage8bExactTransportPermit;
fn main() { let _ = std::mem::size_of::<Stage8bExactTransportPermit>(); }'
check_fail request_literal 'use finam_gateway::Stage8bOperatorInvocationRequest;
fn main() { let _ = Stage8bOperatorInvocationRequest {
    invocation_id: String::new(), accepted_run_package_path: "/x".into(), local_manifest_root: "/y".into()
}; }'
check_fail request_clone 'use finam_gateway::Stage8bOperatorInvocationRequest;
fn consume(value: Stage8bOperatorInvocationRequest) { let _ = value.clone(); }
fn main() { let _ = consume; }'
check_fail raw_path_getter 'use finam_gateway::Stage8bOperatorDiagnostic;
fn consume(value: Stage8bOperatorDiagnostic) { let _ = value.accepted_run_package_path(); }
fn main() { let _ = consume; }'
check_fail authority_conversion 'use finam_gateway::Stage8bOperatorDiagnostic;
fn consume(value: Stage8bOperatorDiagnostic) { let _ = value.into_transport_permit(); }
fn main() { let _ = consume; }'
check_fail arm_issuer 'use finam_gateway::issue_rehearsal_arm;
fn main() { let _ = issue_rehearsal_arm; }'
check_fail classifier_bridge 'use finam_gateway::classify_stage8b_transport_observation_with_stage8a3;
fn main() { let _ = classify_stage8b_transport_observation_with_stage8a3; }'
check_fail private_k2_sources 'use finam_gateway::Stage8bK2FreshSources;
fn main() { let _ = std::mem::size_of::<Stage8bK2FreshSources>(); }'
check_fail durable_attempt_transition 'use finam_gateway::record_stage8b_exact_durable_attempt;
fn main() { let _ = record_stage8b_exact_durable_attempt; }'
check_fail covering_seal_transition 'use finam_gateway::authenticate_stage8b_covering_seal_after_attempt;
fn main() { let _ = authenticate_stage8b_covering_seal_after_attempt; }'
check_fail private_durable_attempt 'use finam_gateway::Stage8bDurableAttemptRecorded;
fn main() { let _ = std::mem::size_of::<Stage8bDurableAttemptRecorded>(); }'
check_fail permit_transition 'use finam_gateway::authorize_stage8b_exact_transport_permit;
fn main() { let _ = authorize_stage8b_exact_transport_permit; }'
check_fail builder_before_permit 'use finam_gateway::compose_stage8b_private_request_parts_from_stage8a2;
fn main() { let _ = compose_stage8b_private_request_parts_from_stage8a2; }'
check_fail raw_request_witness 'use finam_gateway::Stage8bApprovedRequestParts;
fn main() { let _ = std::mem::size_of::<Stage8bApprovedRequestParts>(); }'
check_fail local_boundary 'use finam_gateway::invoke_stage8b_local_no_network_boundary;
fn main() { let _ = invoke_stage8b_local_no_network_boundary; }'

echo "stage8b-i-external-compile-fail: PASS positive=1 negative=20"
