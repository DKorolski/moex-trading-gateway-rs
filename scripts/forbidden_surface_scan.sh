#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

failures=0

python_with_tomllib=""
for candidate in python3 python3.13 python3.12 python3.11; do
  if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import tomllib' >/dev/null 2>&1; then
    python_with_tomllib="$candidate"
    break
  fi
done
if [[ -z "$python_with_tomllib" ]]; then
  echo "forbidden-surface-scan: Python 3.11+ with stdlib tomllib is required" >&2
  exit 1
fi

report_failure() {
  echo "forbidden-surface-scan: $*" >&2
  failures=$((failures + 1))
}

approved_order_transport="crates/finam-gateway/src/m3d2_real_order_transport.rs"

if rg -n --glob 'crates/**/*.rs' '\.delete\(' crates | grep -v "^${approved_order_transport}:" >/tmp/moex_forbidden_delete.$$; then
  cat /tmp/moex_forbidden_delete.$$ >&2
  report_failure "real HTTP DELETE surface is forbidden outside the reviewed M3d-2c transport"
fi
rm -f /tmp/moex_forbidden_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::DELETE' crates >/tmp/moex_forbidden_method_delete.$$; then
  cat /tmp/moex_forbidden_method_delete.$$ >&2
  report_failure "Method::DELETE surface is forbidden"
fi
rm -f /tmp/moex_forbidden_method_delete.$$

if rg -n --glob 'crates/**/*.rs' 'Method::POST' crates >/tmp/moex_forbidden_method_post.$$; then
  cat /tmp/moex_forbidden_method_post.$$ >&2
  report_failure "Method::POST is not allowed in gateway/order surfaces"
fi
rm -f /tmp/moex_forbidden_method_post.$$

if rg -n '"/v1/accounts/[^"]*/orders' crates/broker-finam/src/lib.rs >/tmp/moex_forbidden_order_route_literal.$$; then
  cat /tmp/moex_forbidden_order_route_literal.$$ >&2
  report_failure "literal FINAM order route bypass is forbidden before explicit endpoint review"
fi
rm -f /tmp/moex_forbidden_order_route_literal.$$

if rg -n --glob 'crates/**/*.rs' 'OrderEndpointHttp(Client|Transport|Adapter|Backend)' crates >/tmp/moex_forbidden_order_http_abstraction.$$; then
  cat /tmp/moex_forbidden_order_http_abstraction.$$ >&2
  report_failure "non-reqwest order endpoint HTTP abstraction is forbidden before explicit endpoint review"
fi
rm -f /tmp/moex_forbidden_order_http_abstraction.$$

if rg -n --glob 'crates/**/*.rs' 'EndpointGateApproved[[:space:]]*\{[[:space:]]*_private:[[:space:]]*\(\)' crates >/tmp/moex_forbidden_endpoint_gate_literal.$$; then
  cat /tmp/moex_forbidden_endpoint_gate_literal.$$ >&2
  report_failure "direct EndpointGateApproved literal construction is forbidden outside reviewed constructors"
fi
rm -f /tmp/moex_forbidden_endpoint_gate_literal.$$

"$python_with_tomllib" - <<'PY'
import hashlib
import json
from pathlib import Path
import sys
import tomllib

failures = 0

for path in Path("crates").glob("**/*.rs"):
    source = path.read_text()
    if ".post(" not in source:
        continue
    if path == Path("crates/finam-gateway/src/m3d2_real_order_transport.rs"):
        if source.count(".post(") != 1:
            print(
                "forbidden-surface-scan: M3d-2c transport must have exactly one .post(",
                file=sys.stderr,
            )
            failures += 1
        continue
    if path != Path("crates/broker-finam/src/lib.rs"):
        for line_no, line in enumerate(source.splitlines(), start=1):
            if ".post(" in line:
                print(
                    f"forbidden-surface-scan: unexpected .post( in {path}:{line_no}",
                    file=sys.stderr,
                )
                failures += 1
        continue

    allowed_functions = {
        "auth": 'self.rest_url(&["v1", "sessions"])',
        "token_details": 'self.rest_url(&["v1", "sessions", "details"])',
        "token_details_typed": 'self.rest_url(&["v1", "sessions", "details"])',
    }
    for function_name, expected_path in allowed_functions.items():
        marker = f"pub async fn {function_name}("
        if marker not in source:
            print(
                f"forbidden-surface-scan: cannot locate allowed POST function {function_name}",
                file=sys.stderr,
            )
            failures += 1
            continue
        block = source.split(marker, 1)[1]
        next_function = block.find("\n    pub async fn ")
        if next_function != -1:
            block = block[:next_function]
        post_count = block.count(".post(")
        if post_count != 1 or expected_path not in block:
            print(
                "forbidden-surface-scan: broker-finam POST allowlist mismatch "
                f"for {function_name}: post_count={post_count}",
                file=sys.stderr,
            )
            failures += 1
    allowed_post_count = len(allowed_functions)
    actual_post_count = source.count(".post(")
    if actual_post_count != allowed_post_count:
        print(
            "forbidden-surface-scan: broker-finam has unexpected .post( count "
            f"actual={actual_post_count} allowed={allowed_post_count}",
            file=sys.stderr,
        )
        failures += 1

transport_path = Path("crates/finam-gateway/src/m3d2_real_order_transport.rs")
if transport_path.exists():
    transport_source = transport_path.read_text()
    expected_counts = {
        ".post(": 1,
        ".delete(": 1,
        ".send(": 1,
    }
    for token, expected in expected_counts.items():
        actual = transport_source.count(token)
        if actual != expected:
            print(
                "forbidden-surface-scan: M3d-2c transport allowlist mismatch "
                f"for {token}: actual={actual} expected={expected}",
                file=sys.stderr,
            )
            failures += 1
    required_transport_patterns = [
        "EndpointGateApproved",
        "FinamPlaceOrderRequestSpec",
        "FinamCancelOrderRequestSpec",
        "FinamAuthorizationHeaderMode::BearerJwt",
        "post_send_semantics",
        "raw_token_exported: false",
        "raw_path_exported: false",
        "raw_body_exported: false",
    ]
    for pattern in required_transport_patterns:
        if pattern not in transport_source:
            print(
                "forbidden-surface-scan: M3d-2c transport missing required "
                f"pattern {pattern!r}",
                file=sys.stderr,
            )
            failures += 1

approved_real_transport_test_files = {
    Path("crates/finam-gateway/src/m3d2_real_order_transport.rs"),
    Path("crates/finam-gateway/src/m3d2_real_transport_lifecycle.rs"),
}
m3j16_cli_path = Path("crates/broker-cli/src/main.rs")

for path in Path("crates").glob("**/*.rs"):
    source = path.read_text()
    test_module_idx = source.find("#[cfg(test)]\nmod tests")
    for token in (
        "M3d2RealOrderEndpointTransport::try_new",
        "M3d2RealOrderEndpointTransportConfig::default()",
    ):
        search_from = 0
        while True:
            idx = source.find(token, search_from)
            if idx == -1:
                break
            before = source[:idx]
            line_no = before.count("\n") + 1
            in_test_module = test_module_idx != -1 and idx > test_module_idx
            m3j16_cli_allow = (
                path == m3j16_cli_path
                and token == "M3d2RealOrderEndpointTransport::try_new"
                and '#[command(name = "finam-limit-cancel-one-shot")]' in source
                and 'actual_send_i_understand_risk' in source
                and 'cfg!(feature = "m3j16-actual-one-shot")' in source
                and 'M3d2ExternalOrderEndpointMode::M3j16ActualOneShotExternalFinam' in source
            )
            if (path not in approved_real_transport_test_files or not in_test_module) and not m3j16_cli_allow:
                print(
                    "forbidden-surface-scan: real order transport construction/default "
                    f"token {token!r} outside approved test modules at {path}:{line_no}",
                    file=sys.stderr,
                )
                failures += 1
            search_from = idx + len(token)

source = Path("crates/finam-gateway/src/lib.rs").read_text()

semantic_kernel_root = Path("crates/strategy-runtime-core")
semantic_workspace_member = "crates/strategy-runtime-core"
semantic_kernel_forbidden = [
    "broker-finam",
    "finam-gateway",
    "reqwest",
    "tokio",
    "redis::",
    "std::net",
    "std::process",
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "FINAM_SECRET",
    "real_order_endpoint",
]
if not semantic_kernel_root.exists():
    print(
        "forbidden-surface-scan: strategy-runtime-core semantic kernel missing",
        file=sys.stderr,
    )
    failures += 1
else:
    semantic_paths = list(semantic_kernel_root.glob("**/*.rs")) + [
        semantic_kernel_root / "Cargo.toml"
    ]
    for semantic_path in semantic_paths:
        if not semantic_path.exists():
            continue
        semantic_source = semantic_path.read_text()
        for pattern in semantic_kernel_forbidden:
            if pattern in semantic_source:
                print(
                    "forbidden-surface-scan: strategy semantic kernel contains "
                    f"forbidden transport/runtime token {pattern!r} in {semantic_path}",
                    file=sys.stderr,
                )
                failures += 1

root_manifest_path = Path("Cargo.toml")
if not root_manifest_path.is_file():
    print("forbidden-surface-scan: root Cargo.toml missing", file=sys.stderr)
    failures += 1
else:
    try:
        with root_manifest_path.open("rb") as manifest:
            root_manifest = tomllib.load(manifest)
        workspace = root_manifest.get("workspace", {})
        workspace_members = set(workspace.get("members", []))
        workspace_excludes = set(workspace.get("exclude", []))
    except (OSError, tomllib.TOMLDecodeError, TypeError) as error:
        print(
            f"forbidden-surface-scan: root Cargo.toml cannot be parsed: {error}",
            file=sys.stderr,
        )
        failures += 1
        workspace_members = set()
        workspace_excludes = set()
    if semantic_workspace_member not in workspace_members:
        print(
            "forbidden-surface-scan: strategy-runtime-core must remain an explicit "
            "workspace member",
            file=sys.stderr,
        )
        failures += 1
    if semantic_workspace_member in workspace_excludes:
        print(
            "forbidden-surface-scan: strategy-runtime-core must not be workspace-excluded",
            file=sys.stderr,
        )
        failures += 1

semantic_crate_manifest_path = semantic_kernel_root / "Cargo.toml"
expected_semantic_crate_manifest_sha256 = (
    "00f18c0d3ddc6f7fb4196edc2a51f18da034070555aad980c35098cbd4ed5fd0"
)
if not semantic_crate_manifest_path.is_file():
    print(
        "forbidden-surface-scan: strategy-runtime-core Cargo.toml missing",
        file=sys.stderr,
    )
    failures += 1
else:
    semantic_crate_manifest_bytes = semantic_crate_manifest_path.read_bytes()
    actual_semantic_manifest_sha256 = hashlib.sha256(
        semantic_crate_manifest_bytes
    ).hexdigest()
    if actual_semantic_manifest_sha256 != expected_semantic_crate_manifest_sha256:
        print(
            "forbidden-surface-scan: strategy-runtime-core Cargo.toml drifted: "
            f"actual={actual_semantic_manifest_sha256} "
            f"expected={expected_semantic_crate_manifest_sha256}",
            file=sys.stderr,
        )
        failures += 1
    try:
        semantic_crate_manifest = tomllib.loads(
            semantic_crate_manifest_bytes.decode("utf-8")
        )
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        print(
            "forbidden-surface-scan: strategy-runtime-core Cargo.toml cannot be "
            f"parsed: {error}",
            file=sys.stderr,
        )
        failures += 1
        semantic_crate_manifest = {}
    package = semantic_crate_manifest.get("package", {})
    if package.get("autotests") is False:
        print(
            "forbidden-surface-scan: strategy-runtime-core autotests must remain enabled",
            file=sys.stderr,
        )
        failures += 1
    if "build" in package:
        print(
            "forbidden-surface-scan: strategy-runtime-core package build field is forbidden",
            file=sys.stderr,
        )
        failures += 1
    lib = semantic_crate_manifest.get("lib", {})
    if lib.get("path", "src/lib.rs") != "src/lib.rs":
        print(
            "forbidden-surface-scan: strategy-runtime-core lib path redirect is forbidden",
            file=sys.stderr,
        )
        failures += 1
    if lib.get("test") is False:
        print(
            "forbidden-surface-scan: strategy-runtime-core lib tests must remain enabled",
            file=sys.stderr,
        )
        failures += 1

semantic_build_script = semantic_kernel_root / "build.rs"
if semantic_build_script.exists():
    print(
        "forbidden-surface-scan: strategy-runtime-core default build.rs is forbidden",
        file=sys.stderr,
    )
    failures += 1

semantic_lib_path = semantic_kernel_root / "src/lib.rs"
expected_semantic_lib_sha256 = (
    "eba13a333fc0c003d9afa96f379cfb833b3148d549b97425406f4386bc3cea4a"
)
if not semantic_lib_path.is_file():
    print(
        "forbidden-surface-scan: strategy-runtime-core src/lib.rs missing",
        file=sys.stderr,
    )
    failures += 1
else:
    actual_semantic_lib_sha256 = hashlib.sha256(semantic_lib_path.read_bytes()).hexdigest()
    if actual_semantic_lib_sha256 != expected_semantic_lib_sha256:
        print(
            "forbidden-surface-scan: strategy-runtime-core src/lib.rs drifted: "
            f"actual={actual_semantic_lib_sha256} expected={expected_semantic_lib_sha256}",
            file=sys.stderr,
        )
        failures += 1

semantic_ledger_path = semantic_kernel_root / "source-correspondence.toml"
if not semantic_ledger_path.exists():
    print(
        "forbidden-surface-scan: strategy semantic source correspondence ledger missing",
        file=sys.stderr,
    )
    failures += 1
else:
    try:
        semantic_ledger = {}
        semantic_file_records = []
        current_record = None
        for raw_line in semantic_ledger_path.read_text().splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            if line == "[[files]]":
                current_record = {}
                semantic_file_records.append(current_record)
                continue
            key, raw_value = (part.strip() for part in line.split("=", 1))
            if raw_value in {"true", "false"}:
                value = raw_value == "true"
            elif raw_value.startswith('"'):
                value = json.loads(raw_value)
            else:
                value = int(raw_value)
            target = current_record if current_record is not None else semantic_ledger
            target[key] = value
        semantic_ledger["files"] = semantic_file_records
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(
            "forbidden-surface-scan: cannot parse strategy semantic source "
            f"correspondence ledger: {error}",
            file=sys.stderr,
        )
        failures += 1
        semantic_ledger = {}

    expected_manifest_header = {
        "schema_version": 1,
        "stage": "Stage5B1",
        "alor_source_commit": "43242c89944d335d9cb0729b38bdd7d658378d5e",
        "production_semantics_changed": False,
        "finam_transport_dependency_added": False,
        "redis_client_dependency_added": False,
        "real_order_endpoint_added": False,
    }
    for field, expected in expected_manifest_header.items():
        if semantic_ledger.get(field) != expected:
            print(
                "forbidden-surface-scan: strategy semantic correspondence "
                f"ledger field {field!r} must be {expected}",
                file=sys.stderr,
            )
            failures += 1

    expected_semantic_manifest = {
        "strategy-runtime/src/strategies/hybrid_intraday/mod.rs": {
            "source_sha256": "c70e3847f1a99e00c5d078d19b7b5f103d9b4d26853886b0b47d4805818ac84c",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/mod.rs",
            "target_sha256": "c70e3847f1a99e00c5d078d19b7b5f103d9b4d26853886b0b47d4805818ac84c",
            "change_class": "CopiedUnchanged",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/types.rs": {
            "source_sha256": "8b515e252bc493890483793887248a6a12bedcf072ab87c574d4d3efd3b7eedc",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/types.rs",
            "target_sha256": "8b515e252bc493890483793887248a6a12bedcf072ab87c574d4d3efd3b7eedc",
            "change_class": "CopiedUnchanged",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/intraday_breakout.rs": {
            "source_sha256": "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs",
            "target_sha256": "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5",
            "change_class": "CopiedUnchanged",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/mean_reversion.rs": {
            "source_sha256": "4aecdeeb0bee8bcbae10cd2596c13d4450885b4ad7a8899346b14d743d4039ab",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/mean_reversion.rs",
            "target_sha256": "4aecdeeb0bee8bcbae10cd2596c13d4450885b4ad7a8899346b14d743d4039ab",
            "change_class": "CopiedUnchanged",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/high180.rs": {
            "source_sha256": "e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/high180.rs",
            "target_sha256": "e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456",
            "change_class": "CopiedUnchanged",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/orchestrator.rs": {
            "source_sha256": "db4dfdb014592d99567db9239c84b02c7f61b7eb768ee97a9203bead1c8ed1c0",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs",
            "target_sha256": "1e784411d348fcf090887f7f50062b0cbd34494912288100c1ca1d851d8d5bd9",
            "change_class": "NamespaceOnly",
        },
        "strategy-runtime/src/strategies/hybrid_intraday/risk_gate.rs": {
            "source_sha256": "c85779ec5023e602cb6088e116fb58ed0bc80c31828499a0bd4557e2034dee34",
            "target_path": "crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs",
            "target_sha256": "c85779ec5023e602cb6088e116fb58ed0bc80c31828499a0bd4557e2034dee34",
            "change_class": "CopiedUnchanged",
        },
    }

    semantic_files = semantic_ledger.get("files", [])
    if len(semantic_files) != len(expected_semantic_manifest):
        print(
            "forbidden-surface-scan: strategy semantic correspondence ledger "
            "must contain exactly the immutable Stage 5B-1 file set; "
            f"found {len(semantic_files)}",
            file=sys.stderr,
        )
        failures += 1
    seen_source_paths = set()
    seen_target_paths = set()
    for record in semantic_files:
        source_path_raw = record.get("source_path")
        target_path_raw = record.get("target_path")
        source_sha256 = record.get("source_sha256")
        expected_sha256 = record.get("target_sha256")
        change_class = record.get("change_class")
        expected_record = expected_semantic_manifest.get(source_path_raw)
        if expected_record is None:
            print(
                "forbidden-surface-scan: unapproved Stage 5B-1 source path "
                f"{source_path_raw!r}",
                file=sys.stderr,
            )
            failures += 1
        else:
            for field, expected in expected_record.items():
                if record.get(field) != expected:
                    print(
                        "forbidden-surface-scan: immutable Stage 5B-1 manifest "
                        f"mismatch for {source_path_raw!r} field {field!r}: "
                        f"actual={record.get(field)!r} expected={expected!r}",
                        file=sys.stderr,
                    )
                    failures += 1
        if source_path_raw in seen_source_paths:
            print(
                "forbidden-surface-scan: duplicate correspondence source path "
                f"{source_path_raw!r}",
                file=sys.stderr,
            )
            failures += 1
        seen_source_paths.add(source_path_raw)
        if not isinstance(target_path_raw, str):
            print(
                "forbidden-surface-scan: correspondence target path missing",
                file=sys.stderr,
            )
            failures += 1
            continue
        if target_path_raw in seen_target_paths:
            print(
                "forbidden-surface-scan: duplicate correspondence target path "
                f"{target_path_raw!r}",
                file=sys.stderr,
            )
            failures += 1
        seen_target_paths.add(target_path_raw)
        target_path = Path(target_path_raw)
        if not target_path.is_file():
            print(
                "forbidden-surface-scan: correspondence target file missing "
                f"{target_path}",
                file=sys.stderr,
            )
            failures += 1
            continue
        actual_sha256 = hashlib.sha256(target_path.read_bytes()).hexdigest()
        if actual_sha256 != expected_sha256:
            print(
                "forbidden-surface-scan: correspondence target hash mismatch "
                f"for {target_path}: actual={actual_sha256} "
                f"expected={expected_sha256}",
                file=sys.stderr,
            )
            failures += 1
        if change_class == "CopiedUnchanged" and not (
            actual_sha256 == expected_sha256 == source_sha256
        ):
            print(
                "forbidden-surface-scan: CopiedUnchanged file is not identical "
                f"to frozen source for {target_path}",
                file=sys.stderr,
            )
            failures += 1
        if change_class == "NamespaceOnly":
            production_region = target_path.read_bytes().split(b"#[cfg(test)]", 1)[0]
            production_sha256 = hashlib.sha256(production_region).hexdigest()
            expected_production_sha256 = (
                "ca836ded92cc7b9872482103f48dccac87b7b79d9ad9433979ee2069195dfb53"
            )
            if production_sha256 != expected_production_sha256:
                print(
                    "forbidden-surface-scan: NamespaceOnly production region "
                    f"changed for {target_path}: actual={production_sha256} "
                    f"expected={expected_production_sha256}",
                    file=sys.stderr,
                )
                failures += 1

    expected_target_paths = {
        record["target_path"] for record in expected_semantic_manifest.values()
    }
    if seen_source_paths != set(expected_semantic_manifest):
        print(
            "forbidden-surface-scan: Stage 5B-1 ledger source file set drifted: "
            f"actual={sorted(seen_source_paths)} "
            f"expected={sorted(expected_semantic_manifest)}",
            file=sys.stderr,
        )
        failures += 1
    if seen_target_paths != expected_target_paths:
        print(
            "forbidden-surface-scan: Stage 5B-1 ledger target file set drifted: "
            f"actual={sorted(seen_target_paths)} expected={sorted(expected_target_paths)}",
            file=sys.stderr,
        )
        failures += 1
    actual_target_paths = {
        str(path) for path in (semantic_kernel_root / "src/hybrid_intraday").glob("*.rs")
    }
    if actual_target_paths != expected_target_paths:
        print(
            "forbidden-surface-scan: hybrid semantic target file set drifted: "
            f"actual={sorted(actual_target_paths)} expected={sorted(expected_target_paths)}",
            file=sys.stderr,
        )
        failures += 1

    expected_semantic_production_paths = expected_target_paths | {str(semantic_lib_path)}
    actual_semantic_production_paths = {
        str(path) for path in (semantic_kernel_root / "src").glob("**/*.rs")
    }
    if actual_semantic_production_paths != expected_semantic_production_paths:
        print(
            "forbidden-surface-scan: strategy-runtime-core production source set drifted: "
            f"actual={sorted(actual_semantic_production_paths)} "
            f"expected={sorted(expected_semantic_production_paths)}",
            file=sys.stderr,
        )
        failures += 1

    expected_semantic_rust_paths = expected_semantic_production_paths | {
        str(semantic_kernel_root / "tests/high180_profile_binding.rs"),
        str(semantic_kernel_root / "tests/stage5b2_boundary_manifest.rs"),
        str(semantic_kernel_root / "tests/wrapper_bracket_terminal_inventory.rs"),
    }
    actual_semantic_rust_paths = {
        str(path) for path in semantic_kernel_root.glob("**/*.rs")
    }
    if actual_semantic_rust_paths != expected_semantic_rust_paths:
        print(
            "forbidden-surface-scan: strategy-runtime-core complete Rust/Cargo target "
            f"set drifted: actual={sorted(actual_semantic_rust_paths)} "
            f"expected={sorted(expected_semantic_rust_paths)}",
            file=sys.stderr,
        )
        failures += 1

    expected_semantic_test_sha256 = {
        semantic_kernel_root / "tests/high180_profile_binding.rs": (
            "98e7bbbdc8a0eb852bcdfc2f46cbfc9635c5cc0dc03caefc69a4b50c377a5951"
        ),
        semantic_kernel_root / "tests/stage5b2_boundary_manifest.rs": (
            "e039796971207d763b7ba06e6c85e45c834687866bd12e84780cb51ecfa1cc23"
        ),
        semantic_kernel_root / "tests/wrapper_bracket_terminal_inventory.rs": (
            "b276b376d33073454fd0df243b6d87a351724794d95d52126a8258e9324aeafe"
        ),
    }
    for test_path, expected_test_sha256 in expected_semantic_test_sha256.items():
        if not test_path.is_file():
            print(
                f"forbidden-surface-scan: locked semantic test missing {test_path}",
                file=sys.stderr,
            )
            failures += 1
            continue
        actual_test_sha256 = hashlib.sha256(test_path.read_bytes()).hexdigest()
        if actual_test_sha256 != expected_test_sha256:
            print(
                "forbidden-surface-scan: locked semantic test drifted "
                f"for {test_path}: actual={actual_test_sha256} "
                f"expected={expected_test_sha256}",
                file=sys.stderr,
            )
            failures += 1

wrapper_oracle_path = Path(
    "source-oracles/alor-stage5/hybrid_intraday_runtime.rs"
)
wrapper_oracle_sha256 = (
    "6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa"
)
if not wrapper_oracle_path.is_file():
    print(
        "forbidden-surface-scan: Stage 5 integrated wrapper source oracle missing",
        file=sys.stderr,
    )
    failures += 1
else:
    wrapper_oracle_bytes = wrapper_oracle_path.read_bytes()
    actual_wrapper_sha256 = hashlib.sha256(wrapper_oracle_bytes).hexdigest()
    actual_wrapper_lines = len(wrapper_oracle_bytes.splitlines())
    if actual_wrapper_sha256 != wrapper_oracle_sha256 or actual_wrapper_lines != 6203:
        print(
            "forbidden-surface-scan: Stage 5 integrated wrapper oracle drifted: "
            f"sha256={actual_wrapper_sha256} lines={actual_wrapper_lines}",
            file=sys.stderr,
        )
        failures += 1
    wrapper_oracle_source = wrapper_oracle_bytes.decode("utf-8")
    required_wrapper_binding_markers = [
        "let high180_mr = High180MrEngine::new(High180MrConfig::default());",
        "MeanReversionVariant::High180",
        ".on_bar_with_mr_override(",
    ]
    for marker in required_wrapper_binding_markers:
        if marker not in wrapper_oracle_source:
            print(
                "forbidden-surface-scan: Stage 5 wrapper oracle missing "
                f"high180 binding marker {marker!r}",
                file=sys.stderr,
            )
            failures += 1

wrapper_oracle_rs_files = {
    str(path) for path in wrapper_oracle_path.parent.glob("*.rs")
}
if wrapper_oracle_rs_files != {str(wrapper_oracle_path)}:
    print(
        "forbidden-surface-scan: unexpected Stage 5 wrapper source oracle file set "
        f"{sorted(wrapper_oracle_rs_files)}",
        file=sys.stderr,
    )
    failures += 1

stage5b2_manifest_path = semantic_kernel_root / "stage5b2-source-correspondence.toml"
expected_stage5b2_manifest_sha256 = (
    "727e870aa5ab6da4498c2602d4f5cf3c0df2a933bc53010241d81684a4959360"
)
if not stage5b2_manifest_path.is_file():
    print(
        "forbidden-surface-scan: Stage 5B-2 correspondence manifest missing",
        file=sys.stderr,
    )
    failures += 1
else:
    stage5b2_manifest_bytes = stage5b2_manifest_path.read_bytes()
    actual_stage5b2_manifest_sha256 = hashlib.sha256(stage5b2_manifest_bytes).hexdigest()
    if actual_stage5b2_manifest_sha256 != expected_stage5b2_manifest_sha256:
        print(
            "forbidden-surface-scan: Stage 5B-2 correspondence manifest drifted: "
            f"actual={actual_stage5b2_manifest_sha256} "
            f"expected={expected_stage5b2_manifest_sha256}",
            file=sys.stderr,
        )
        failures += 1
    try:
        stage5b2_manifest = tomllib.loads(stage5b2_manifest_bytes.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        print(
            f"forbidden-surface-scan: Stage 5B-2 manifest cannot be parsed: {error}",
            file=sys.stderr,
        )
        failures += 1
        stage5b2_manifest = {}

    expected_stage5b2_top_level = {
        "schema_version": 1,
        "stage": "Stage5B2a",
        "status": "BoundaryManifestOnly",
        "oracle_sha256": wrapper_oracle_sha256,
        "oracle_line_count": 6203,
        "accepted_stage5b1_manifest_unchanged": True,
        "wrapper_copied": False,
        "wrapper_compiled": False,
        "runtime_host_attached": False,
        "paper_boundary": True,
    }
    for field, expected_value in expected_stage5b2_top_level.items():
        if stage5b2_manifest.get(field) != expected_value:
            print(
                "forbidden-surface-scan: Stage 5B-2 manifest field mismatch "
                f"{field}: actual={stage5b2_manifest.get(field)!r} "
                f"expected={expected_value!r}",
                file=sys.stderr,
            )
            failures += 1

    approved_future_target = stage5b2_manifest.get("approved_future_target", {})
    expected_future_target = {
        "crate_name": "strategy-runtime-core",
        "target_kind": "library_module",
        "module_name": "hybrid_intraday_runtime",
        "target_path": "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
        "library_export": "pub mod hybrid_intraday_runtime;",
        "activation_gate": "Stage5B2bSeparateReview",
        "currently_allowed_in_rust_target_set": False,
    }
    if approved_future_target != expected_future_target:
        print(
            "forbidden-surface-scan: Stage 5B-2 approved future target drifted",
            file=sys.stderr,
        )
        failures += 1

    expected_regions = {
        "imports": (1, 21, 21, "62185934244137f77fbb9cb1e8951d7639eebc114b01e4e29d62696f09addd73"),
        "config_state_types": (23, 207, 185, "2e3a9a9eb2af38119d318f561c0d1defccb36fff34bc30fa3cffd0c63ae054bb"),
        "wrapper_implementation": (209, 2313, 2105, "6cf35346fd2759efbb6ac6b40e4f5748c2f6361349d0fa833de2229c621f0417"),
        "oracle_unit_tests": (2314, 5067, 2754, "c4f5d92bb307e66baf5ab2425512557a3d5715fdde109a4da5f9de21cb678e9e"),
        "strategy_callback_impl": (5068, 6203, 1136, "7749e6ade0bfeff4e6e67fc4fa915759ff064c2a23439c12ecafe026fd84cc39"),
    }
    manifest_regions = stage5b2_manifest.get("regions", [])
    if {region.get("name") for region in manifest_regions} != set(expected_regions):
        print(
            "forbidden-surface-scan: Stage 5B-2 source region set drifted",
            file=sys.stderr,
        )
        failures += 1
    wrapper_lines = wrapper_oracle_path.read_bytes().splitlines(keepends=True)
    for region in manifest_regions:
        name = region.get("name")
        if name not in expected_regions:
            continue
        line_start, line_end, line_count, expected_region_sha256 = expected_regions[name]
        actual_region_bytes = b"".join(wrapper_lines[line_start - 1 : line_end])
        actual_region_sha256 = hashlib.sha256(actual_region_bytes).hexdigest()
        expected_region_fields = {
            "line_start": line_start,
            "line_end": line_end,
            "line_count": line_count,
            "sha256": expected_region_sha256,
            "implementation_status": "Planned",
        }
        for field, expected_value in expected_region_fields.items():
            if region.get(field) != expected_value:
                print(
                    "forbidden-surface-scan: Stage 5B-2 region manifest mismatch "
                    f"{name}.{field}",
                    file=sys.stderr,
                )
                failures += 1
        if actual_region_sha256 != expected_region_sha256:
            print(
                "forbidden-surface-scan: Stage 5B-2 oracle region drifted "
                f"{name}: actual={actual_region_sha256} "
                f"expected={expected_region_sha256}",
                file=sys.stderr,
            )
            failures += 1

expected_stage5_profile_artifacts = {
    Path("config/imoexf-hybrid-high180-profile.redacted.toml"): (
        "15e31d7a285f1c8c80e9168a9098e37e56bbd60ab3ab3264592d23605708dfe4"
    ),
    Path("tests/fixtures/stage5/imoexf_high180_profile_binding.json"): (
        "ec6daea39f19f3162da5e8d77abb0f03a3f4f5ea2e2876c1d1e189401580ec5d"
    ),
    Path("tests/fixtures/stage5/bracket_terminal_reconciliation.json"): (
        "a869ff79d35c7c0f75e1417b998c388256cfd87794d3cd1cf78d33b0f4dc563c"
    ),
    Path("tests/fixtures/stage5/stage5b2_callback_state_mapping.json"): (
        "01585a01941dcc530e7769fa2fd85ac7b2bdec409f2b87f64005e7fe54ec6f5e"
    ),
}
for artifact_path, expected_artifact_sha256 in expected_stage5_profile_artifacts.items():
    if not artifact_path.is_file():
        print(
            f"forbidden-surface-scan: Stage 5 profile artifact missing {artifact_path}",
            file=sys.stderr,
        )
        failures += 1
        continue
    actual_artifact_sha256 = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
    if actual_artifact_sha256 != expected_artifact_sha256:
        print(
            "forbidden-surface-scan: Stage 5 profile artifact drifted "
            f"for {artifact_path}: actual={actual_artifact_sha256} "
            f"expected={expected_artifact_sha256}",
            file=sys.stderr,
        )
        failures += 1

scopes = {
    "real-readonly transport": (
        "pub struct ReqwestFinamRealReadonlyBrokerTruthTransport",
        "#[derive(Clone)]\npub struct LocalMockFinamRealReadonlyBrokerTruthTransport",
    ),
    "real-readonly operator probe": (
        "pub struct FinamRealReadonlyContractProbeOperatorRunConfig",
        "#[derive(Clone)]\npub struct LocalMockCancelBrokerTruthReadonlyHttpClient",
    ),
}

forbidden = [
    ".post(",
    ".delete(",
    "Method::POST",
    "Method::DELETE",
    "FinamPlaceOrderRequestSpec",
    "FinamCancelOrderRequestSpec",
    "FinamRealOrderEndpointTransport",
    "place_order_endpoint",
    "cancel_order_endpoint",
]

for scope_name, (start, end) in scopes.items():
    try:
        scoped = source.split(start, 1)[1].split(end, 1)[0]
    except IndexError:
        print(f"forbidden-surface-scan: cannot locate {scope_name} scope", file=sys.stderr)
        failures += 1
        continue
    for pattern in forbidden:
        if pattern in scoped:
            print(
                f"forbidden-surface-scan: {pattern!r} found in {scope_name}",
                file=sys.stderr,
            )
            failures += 1

if "\npub async fn run_finam_real_readonly_contract_probe" in source:
    print(
        "forbidden-surface-scan: lower-level real-readonly contract probe must not be public",
        file=sys.stderr,
    )
    failures += 1
if "\npub async fn run_finam_real_readonly_operator_contract_probe" not in source:
    print(
        "forbidden-surface-scan: operator real-readonly contract probe entrypoint missing",
        file=sys.stderr,
    )
    failures += 1

sys.exit(1 if failures else 0)
PY

if (( failures > 0 )); then
  exit 1
fi

echo "forbidden-surface-scan: ok"
