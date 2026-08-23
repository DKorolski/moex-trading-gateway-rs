#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-it-external.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"

cat >"$work/Cargo.toml" <<EOF
[package]
name = "stage8b-it-external-boundary"
version = "0.0.0"
edition = "2021"

[dependencies]
finam-gateway = { path = "$repo_root/crates/finam-gateway" }
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
    echo "stage8b-it-external-compile-fail: FAIL $name compiled" >&2
    exit 1
  fi
}

check_pass 'use finam_gateway::{invoke_stage8b_operator_once, Stage8bOperatorInvocationRequest};
fn main() {
    let request = Stage8bOperatorInvocationRequest::new(
        "INVOCATION_COMPILE_0001", "/absolute/package", "/absolute/manifest-root"
    );
    let _ = invoke_stage8b_operator_once;
    let _ = request;
}'

check_fail private_adapter_module 'use finam_gateway::stage8b_adapter;
fn main() { let _ = std::mem::size_of_val(&stage8b_adapter::production_policy_accepts); }'
check_fail private_adapter_type 'use finam_gateway::Stage8bItAdapter;
fn main() { let _ = std::mem::size_of::<Stage8bItAdapter>(); }'
check_fail private_endpoint_type 'use finam_gateway::Stage8bItQualificationEndpoint;
fn main() { let _ = std::mem::size_of::<Stage8bItQualificationEndpoint>(); }'
check_fail private_token_type 'use finam_gateway::Stage8bItQualificationToken;
fn main() { let _ = std::mem::size_of::<Stage8bItQualificationToken>(); }'
check_fail private_observation_type 'use finam_gateway::Stage8bItQualifiedObservation;
fn main() { let _ = std::mem::size_of::<Stage8bItQualifiedObservation>(); }'
check_fail private_request_parts 'use finam_gateway::Stage8bApprovedRequestParts;
fn main() { let _ = std::mem::size_of::<Stage8bApprovedRequestParts>(); }'
check_fail private_request_spec 'use finam_gateway::Stage8bPrivateRequestSpec;
fn main() { let _ = std::mem::size_of::<Stage8bPrivateRequestSpec>(); }'
check_fail private_compose_bridge 'use finam_gateway::compose_stage8b_private_request_parts_from_stage8a2;
fn main() { let _ = compose_stage8b_private_request_parts_from_stage8a2; }'
check_fail private_adapter_constructor 'use finam_gateway::stage8b_adapter::Stage8bItAdapter;
fn main() { let _ = Stage8bItAdapter::qualified(); }'
check_fail private_qualification_call 'use finam_gateway::stage8b_adapter::Stage8bItAdapter;
fn main() { let _ = Stage8bItAdapter::qualify_once; }'
check_fail diagnostic_has_no_raw_request 'use finam_gateway::Stage8bOperatorDiagnostic;
fn consume(value: Stage8bOperatorDiagnostic) { let _ = value.raw_request_parts(); }
fn main() { let _ = consume; }'
check_fail no_production_endpoint_authority 'use finam_gateway::Stage8bItQualificationEndpoint;
fn main() { let _ = Stage8bItQualificationEndpoint::production(); }'

echo "stage8b-it-external-compile-fail: PASS positive=1 negative=12"
