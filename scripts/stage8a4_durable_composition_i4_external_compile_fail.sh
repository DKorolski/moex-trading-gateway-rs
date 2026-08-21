#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8a4-i4-external.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"

cat >"$work/Cargo.toml" <<EOF
[package]
name = "stage8a4-i4-external-boundary"
version = "0.0.0"
edition = "2021"

[dependencies]
runtime-durable-service = { path = "$repo_root/crates/runtime-durable-service" }
finam-gateway = { path = "$repo_root/crates/finam-gateway" }
serde_json = "1"
EOF

check_pass() {
  local source="$1"
  printf '%s\n' "$source" >"$work/src/main.rs"
  cargo check --quiet --manifest-path "$work/Cargo.toml"
}

check_fail() {
  local name="$1"
  local source="$2"
  printf '%s\n' "$source" >"$work/src/main.rs"
  if cargo check --quiet --manifest-path "$work/Cargo.toml" >"$work/$name.log" 2>&1; then
    echo "stage8a4-i4-external-compile-fail: FAIL $name compiled" >&2
    exit 1
  fi
}

check_pass 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(_: Stage7bStage8a4TerminalAuthority) {}
fn receive_owner_issued(value: Stage7bStage8a4TerminalAuthority) { consume(value); }
fn main() { let _ = receive_owner_issued; }'

check_fail literal 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn main() { let _ = Stage7bStage8a4TerminalAuthority {}; }'
check_fail constructor 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn main() { let _ = Stage7bStage8a4TerminalAuthority::new; }'
check_fail clone 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = value.clone(); }
fn main() { let _ = consume; }'
check_fail debug 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = format!("{value:?}"); }
fn main() { let _ = consume; }'
check_fail serialize 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = serde_json::to_vec(&value); }
fn main() { let _ = consume; }'
check_fail receipt_mint 'use runtime_durable_service::Stage7bStage8a4DurableBatchReceipt;
fn consume(value: Stage7bStage8a4DurableBatchReceipt) { let _ = value.into_terminal_authority(); }
fn main() { let _ = consume; }'
check_fail raw_mint 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn main() { let _ = Stage7bStage8a4TerminalAuthority::from_raw_journal; }'
check_fail checkpoint_getter 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = value.stage6_checkpoint_sha256(); }
fn main() { let _ = consume; }'
check_fail seal_generation_getter 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = value.seal_generation(); }
fn main() { let _ = consume; }'
check_fail seal_commitment_getter 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = value.seal_commitment_sha256(); }
fn main() { let _ = consume; }'
check_fail settlement_fingerprint_getter 'use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
fn consume(value: Stage7bStage8a4TerminalAuthority) { let _ = value.settlement_authority_fingerprint_sha256(); }
fn main() { let _ = consume; }'
check_fail trusted_sources_literal 'use finam_gateway::Stage8a1TrustedCurrentSources;
fn main() { let _ = Stage8a1TrustedCurrentSources {}; }'

echo "stage8a4-i4-external-compile-fail: PASS positive=1 negative=12"
