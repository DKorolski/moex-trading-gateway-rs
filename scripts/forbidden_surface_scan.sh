#!/usr/bin/env bash
set -euo pipefail

FORBIDDEN_SURFACE_SCANNER_CONTRACT="stage5d-b2bc1-r4-v1"

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

cargo_control_hashes = {
    Path("Cargo.toml"): "1c3e7dd1b83a6a8942e02cb520d49f33ed3ef77f2970854b9fdcddc7f261bc3e",
    Path("Cargo.lock"): "ff535d0490a848e43631906ee8abd8633630d162714299f7628c0e5fe8a0b36b",
    Path("crates/broker-cli/Cargo.toml"): "8f642b380ae8db32047504e632d1b710cdbc235f5058f57ad0780d72182f2754",
    Path("crates/broker-core/Cargo.toml"): "e807ab613c52d8325d1c46b1f679b319ab72ffeb69196e5a52aacecbd694dc8d",
    Path("crates/broker-finam/Cargo.toml"): "2a4f78beac8390e06e035e1c7ba0c0a71d230165297ad452ff3c4eeb1a2107db",
    Path("crates/finam-gateway/Cargo.toml"): "95b937eb4d166212869d196a1173f40b358c64cf91906ecaef19d7268820f06c",
    Path("crates/strategy-runtime-core/Cargo.toml"): "8246636dfc245e4fd4cdd99c8e09496a6cb11a3f1dbc03605fc50aae58f90e07",
}
for cargo_control_path, expected_sha256 in cargo_control_hashes.items():
    if not cargo_control_path.is_file():
        print(
            f"forbidden-surface-scan: Cargo control file missing {cargo_control_path}",
            file=sys.stderr,
        )
        failures += 1
        continue
    actual_sha256 = hashlib.sha256(cargo_control_path.read_bytes()).hexdigest()
    if actual_sha256 != expected_sha256:
        print(
            "forbidden-surface-scan: Cargo compilation control drifted at "
            f"{cargo_control_path}: actual={actual_sha256} expected={expected_sha256}",
            file=sys.stderr,
        )
        failures += 1


def decode_rust_string_fragment(fragment):
    decoded = []
    index = 0
    escape_map = {
        "0": "\0",
        "n": "\n",
        "r": "\r",
        "t": "\t",
        "\\": "\\",
        "'": "'",
        '"': '"',
    }
    while index < len(fragment):
        if fragment[index] != "\\":
            decoded.append(fragment[index])
            index += 1
            continue
        if index + 1 >= len(fragment):
            decoded.append("\\")
            break

        escaped = fragment[index + 1]
        if escaped in escape_map:
            decoded.append(escape_map[escaped])
            index += 2
            continue
        if escaped == "x" and index + 3 < len(fragment):
            digits = fragment[index + 2 : index + 4]
            try:
                decoded.append(chr(int(digits, 16)))
                index += 4
                continue
            except ValueError:
                pass
        if escaped == "u" and index + 2 < len(fragment) and fragment[index + 2] == "{":
            closing_brace = fragment.find("}", index + 3)
            if closing_brace != -1:
                digits = fragment[index + 3 : closing_brace].replace("_", "")
                try:
                    decoded.append(chr(int(digits, 16)))
                    index = closing_brace + 1
                    continue
                except (ValueError, OverflowError):
                    pass
        if escaped == "\n":
            index += 2
            while index < len(fragment) and fragment[index].isspace():
                index += 1
            continue
        if escaped == "\r":
            index += 2
            if index < len(fragment) and fragment[index] == "\n":
                index += 1
            while index < len(fragment) and fragment[index].isspace():
                index += 1
            continue

        decoded.extend(("\\", escaped))
        index += 2
    return "".join(decoded)


def rust_tokens_and_string_fragments(source):
    """Return code tokens and string fragments needed by the Stage 5 guard."""

    tokens = []
    string_fragments = []
    index = 0
    source_len = len(source)

    def skip_quoted(quote_index):
        cursor = quote_index + 1
        while cursor < source_len:
            if source[cursor] == "\\":
                cursor += 2
                continue
            if source[cursor] == '"':
                return cursor + 1
            cursor += 1
        return source_len

    def raw_string_span(start):
        for prefix in ("br", "cr", "r"):
            if not source.startswith(prefix, start):
                continue
            cursor = start + len(prefix)
            hash_count = 0
            while cursor < source_len and source[cursor] == "#":
                hash_count += 1
                cursor += 1
            if cursor >= source_len or source[cursor] != '"':
                continue
            terminator = '"' + ("#" * hash_count)
            end = source.find(terminator, cursor + 1)
            if end == -1:
                return (cursor + 1, source_len, source_len)
            return (cursor + 1, end, end + len(terminator))
        return None

    def char_literal_end(quote_index):
        cursor = quote_index + 1
        if cursor >= source_len or source[cursor] in ("'", "\n", "\r"):
            return None
        if source[cursor] == "\\":
            cursor += 1
            if cursor >= source_len:
                return None
            if source[cursor] == "u" and cursor + 1 < source_len and source[cursor + 1] == "{":
                closing_brace = source.find("}", cursor + 2)
                if closing_brace == -1:
                    return None
                cursor = closing_brace + 1
            elif source[cursor] == "x":
                cursor += 3
            else:
                cursor += 1
        else:
            cursor += 1
        if cursor < source_len and source[cursor] == "'":
            return cursor + 1
        return None

    while index < source_len:
        if source[index].isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = source_len if newline == -1 else newline + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while index < source_len and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue

        raw_span = raw_string_span(index)
        if raw_span is not None:
            content_start, content_end, raw_end = raw_span
            string_fragments.append(source[content_start:content_end])
            index = raw_end
            continue
        if source[index] == '"':
            quoted_end = skip_quoted(index)
            content_end = quoted_end - 1 if quoted_end < source_len else source_len
            string_fragments.append(
                decode_rust_string_fragment(source[index + 1 : content_end])
            )
            index = quoted_end
            continue
        if source[index] in ("b", "c") and index + 1 < source_len and source[index + 1] == '"':
            quoted_end = skip_quoted(index + 1)
            content_end = quoted_end - 1 if quoted_end < source_len else source_len
            string_fragments.append(
                decode_rust_string_fragment(source[index + 2 : content_end])
            )
            index = quoted_end
            continue
        if source[index] == "'":
            literal_end = char_literal_end(index)
            if literal_end is not None:
                index = literal_end
                continue
        if source[index] == "b" and index + 1 < source_len and source[index + 1] == "'":
            literal_end = char_literal_end(index + 1)
            if literal_end is not None:
                index = literal_end
                continue

        if (
            source.startswith("r#", index)
            and index + 2 < source_len
            and (source[index + 2] == "_" or source[index + 2].isalpha())
        ):
            cursor = index + 3
            while cursor < source_len and (
                source[cursor] == "_" or source[cursor].isalnum()
            ):
                cursor += 1
            tokens.append(source[index + 2 : cursor])
            index = cursor
            continue
        if source[index] == "_" or source[index].isalpha():
            cursor = index + 1
            while cursor < source_len and (
                source[cursor] == "_" or source[cursor].isalnum()
            ):
                cursor += 1
            tokens.append(source[index:cursor])
            index = cursor
            continue

        tokens.append(source[index])
        index += 1

    return tokens, string_fragments


def has_path_meta_in_attribute(tokens):
    index = 0
    while index < len(tokens) - 1:
        if tokens[index] != "#":
            index += 1
            continue
        cursor = index + 1
        if cursor < len(tokens) and tokens[cursor] == "!":
            cursor += 1
        if cursor >= len(tokens) or tokens[cursor] != "[":
            index += 1
            continue

        depth = 1
        cursor += 1
        while cursor < len(tokens) and depth:
            if tokens[cursor] == "[":
                depth += 1
            elif tokens[cursor] == "]":
                depth -= 1
            elif (
                tokens[cursor] == "path"
                and cursor + 1 < len(tokens)
                and tokens[cursor + 1] == "="
            ):
                return True
            cursor += 1
        index = cursor
    return False


def has_path_assignment_in_macro_invocation(tokens):
    matching_delimiter = {"(": ")", "[": "]", "{": "}"}
    for index, token in enumerate(tokens[:-1]):
        if token != "!" or tokens[index + 1] not in matching_delimiter:
            continue
        stack = [matching_delimiter[tokens[index + 1]]]
        cursor = index + 2
        while cursor < len(tokens) and stack:
            current = tokens[cursor]
            if current in matching_delimiter:
                stack.append(matching_delimiter[current])
            elif current == stack[-1]:
                stack.pop()
            elif (
                current == "path"
                and cursor + 1 < len(tokens)
                and tokens[cursor + 1] == "="
            ):
                return True
            cursor += 1
    return False


def has_macro_generated_attribute(tokens):
    for index in range(len(tokens) - 2):
        if tokens[index : index + 2] != ["#", "["]:
            continue
        depth = 1
        cursor = index + 2
        while cursor < len(tokens) and depth:
            if tokens[cursor] == "[":
                depth += 1
            elif tokens[cursor] == "]":
                depth -= 1
            elif tokens[cursor] == "$":
                return True
            cursor += 1
    return False

root_manifest_path = Path("Cargo.toml")
root_manifest = {}
expected_workspace_members = {
    "crates/broker-core",
    "crates/broker-finam",
    "crates/finam-gateway",
    "crates/broker-cli",
    "crates/strategy-runtime-core",
}
if not root_manifest_path.is_file():
    print("forbidden-surface-scan: root Cargo.toml missing", file=sys.stderr)
    failures += 1
    workspace_members = set()
    workspace_excludes = set()
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

if workspace_members != expected_workspace_members:
    print(
        "forbidden-surface-scan: workspace member set drifted: "
        f"actual={sorted(workspace_members)} expected={sorted(expected_workspace_members)}",
        file=sys.stderr,
    )
    failures += 1

if workspace_excludes:
    print(
        "forbidden-surface-scan: workspace.exclude must remain empty before "
        f"Stage 5B-2b; actual={sorted(workspace_excludes)}",
        file=sys.stderr,
    )
    failures += 1

workspace_source_candidate_files = set()
workspace_member_paths = {
    member: Path(member).resolve() for member in workspace_members
}
explicit_cargo_source_paths = set()
expected_local_path_edges = {
    ("crates/broker-finam", "crates/broker-core"),
    ("crates/finam-gateway", "crates/broker-core"),
    ("crates/finam-gateway", "crates/broker-finam"),
    ("crates/broker-cli", "crates/broker-core"),
    ("crates/broker-cli", "crates/broker-finam"),
    ("crates/broker-cli", "crates/finam-gateway"),
    ("crates/strategy-runtime-core", "crates/broker-core"),
}
observed_local_path_edges = set()

dependency_section_names = (
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
)


def dependency_tables(manifest):
    for section_name in dependency_section_names:
        section = manifest.get(section_name, {})
        if isinstance(section, dict):
            yield section_name, section
    targets = manifest.get("target", {})
    if isinstance(targets, dict):
        for target_name, target_config in targets.items():
            if not isinstance(target_config, dict):
                continue
            for section_name in dependency_section_names:
                section = target_config.get(section_name, {})
                if isinstance(section, dict):
                    yield f"target.{target_name}.{section_name}", section


for member in sorted(workspace_members):
    member_path = Path(member)
    if not member_path.is_dir():
        print(
            f"forbidden-surface-scan: workspace member path missing {member}",
            file=sys.stderr,
        )
        failures += 1
        continue
    member_build_scripts = [
        path
        for path in member_path.glob("**/build.rs")
        if path.is_file() and "target" not in path.parts
    ]
    for build_script in member_build_scripts:
        print(
            "forbidden-surface-scan: workspace build.rs is forbidden in the "
            f"Stage 5B-2 trusted Cargo graph: {build_script}",
            file=sys.stderr,
        )
        failures += 1
    workspace_source_candidate_files.update(
        path
        for path in member_path.glob("**/*")
        if path.is_file()
        and (path.suffix in {".rs", ".inc", ".in"} or not path.suffix)
        and "target" not in path.parts
        and ".git" not in path.parts
    )

    member_manifest_path = member_path / "Cargo.toml"
    if not member_manifest_path.is_file():
        print(
            f"forbidden-surface-scan: member manifest missing {member_manifest_path}",
            file=sys.stderr,
        )
        failures += 1
        continue
    try:
        with member_manifest_path.open("rb") as manifest_file:
            member_manifest = tomllib.load(manifest_file)
    except (OSError, tomllib.TOMLDecodeError, TypeError) as error:
        print(
            f"forbidden-surface-scan: cannot parse {member_manifest_path}: {error}",
            file=sys.stderr,
        )
        failures += 1
        continue

    package = member_manifest.get("package", {})
    if isinstance(package, dict) and isinstance(package.get("build"), str):
        explicit_cargo_source_paths.add((member, member_path / package["build"]))
    for target_kind in ("lib", "bin", "test", "example", "bench"):
        target_configs = member_manifest.get(target_kind, [])
        if isinstance(target_configs, dict):
            target_configs = [target_configs]
        if not isinstance(target_configs, list):
            continue
        for target_config in target_configs:
            if isinstance(target_config, dict) and isinstance(target_config.get("path"), str):
                explicit_cargo_source_paths.add(
                    (member, member_path / target_config["path"])
                )

    for section_name, dependencies in dependency_tables(member_manifest):
        for dependency_name, dependency_spec in dependencies.items():
            if not isinstance(dependency_spec, dict) or "path" not in dependency_spec:
                continue
            dependency_path_value = dependency_spec["path"]
            if not isinstance(dependency_path_value, str):
                print(
                    "forbidden-surface-scan: local dependency path must be a string "
                    f"at {member_manifest_path}:{section_name}.{dependency_name}",
                    file=sys.stderr,
                )
                failures += 1
                continue
            dependency_path = (member_path / dependency_path_value).resolve()
            target_member = next(
                (
                    candidate_member
                    for candidate_member, candidate_path in workspace_member_paths.items()
                    if candidate_path == dependency_path
                ),
                None,
            )
            edge = (member, target_member) if target_member is not None else None
            if edge is not None:
                observed_local_path_edges.add(edge)
            if edge not in expected_local_path_edges:
                print(
                    "forbidden-surface-scan: unapproved local path dependency "
                    f"{member}:{section_name}.{dependency_name} -> {dependency_path}",
                    file=sys.stderr,
                )
                failures += 1

if observed_local_path_edges != expected_local_path_edges:
    print(
        "forbidden-surface-scan: local path dependency edge set drifted: "
        f"actual={sorted(observed_local_path_edges)} "
        f"expected={sorted(expected_local_path_edges)}",
        file=sys.stderr,
    )
    failures += 1

workspace_dependency_table = root_manifest.get("workspace", {}).get("dependencies", {})
if isinstance(workspace_dependency_table, dict):
    for dependency_name, dependency_spec in workspace_dependency_table.items():
        if isinstance(dependency_spec, dict) and "path" in dependency_spec:
            print(
                "forbidden-surface-scan: root workspace local path dependency is "
                f"not allowed before Stage 5B-2b: {dependency_name}",
                file=sys.stderr,
            )
            failures += 1

for override_section in ("patch", "replace"):
    if root_manifest.get(override_section):
        print(
            "forbidden-surface-scan: Cargo dependency override section is frozen "
            f"before Stage 5B-2b: {override_section}",
            file=sys.stderr,
        )
        failures += 1

repository_cargo_configs = [
    path
    for config_name in ("config", "config.toml")
    for path in Path(".").glob(f"**/.cargo/{config_name}")
    if path.is_file()
    and not any(part in {".git", "target", "tmp", "reports"} for part in path.parts)
]
for cargo_config_path in repository_cargo_configs:
    print(
        "forbidden-surface-scan: repository-local Cargo config is forbidden in "
        f"the Stage 5B-2 trusted build model: {cargo_config_path}",
        file=sys.stderr,
    )
    failures += 1

for declaring_member, explicit_source_path in explicit_cargo_source_paths:
    resolved_source_path = explicit_source_path.resolve()
    declaring_member_path = workspace_member_paths[declaring_member]
    if not resolved_source_path.is_relative_to(declaring_member_path):
        print(
            "forbidden-surface-scan: explicit Cargo source path escapes its "
            f"declaring member {declaring_member}: {explicit_source_path}",
            file=sys.stderr,
        )
        failures += 1
    elif not explicit_source_path.is_file():
        print(
            f"forbidden-surface-scan: explicit Cargo source missing {explicit_source_path}",
            file=sys.stderr,
        )
        failures += 1
    else:
        workspace_source_candidate_files.add(explicit_source_path)

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
wrapper_future_target_path = Path(
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
)

allowed_wrapper_oracle_include_str_paths = {
    Path("crates/strategy-runtime-core/tests/high180_profile_binding.rs"),
    Path("crates/strategy-runtime-core/tests/stage5b2_boundary_manifest.rs"),
    Path("crates/strategy-runtime-core/tests/wrapper_bracket_terminal_inventory.rs"),
}
allowed_wrapper_identifier_paths = {
    wrapper_future_target_path,
    Path("crates/strategy-runtime-core/src/lib.rs"),
    Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs"),
    Path("crates/strategy-runtime-core/src/stage5d_persistence.rs"),
    Path("crates/strategy-runtime-core/tests/stage5b2_boundary_manifest.rs"),
}
wrapper_oracle_filename = "hybrid_intraday_runtime.rs"
wrapper_oracle_reference_markers = (
    "source-oracles/alor-stage5",
    wrapper_oracle_filename,
)
wrapper_oracle_path = Path(
    "source-oracles/alor-stage5/hybrid_intraday_runtime.rs"
)
wrapper_oracle_sha256 = (
    "6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa"
)
compiled_wrapper_sha256 = (
    "767815903b8bc07ee48ac96a9d4dac74553b6c32ae63326e141743b12d98b65c"
)
compiled_wrapper_line_count = 6936

if not wrapper_oracle_path.is_file():
    print(
        f"forbidden-surface-scan: wrapper oracle missing {wrapper_oracle_path}",
        file=sys.stderr,
    )
    failures += 1
elif hashlib.sha256(wrapper_oracle_path.read_bytes()).hexdigest() != wrapper_oracle_sha256:
    print(
        "forbidden-surface-scan: wrapper oracle hash drifted before Stage 5B-2b",
        file=sys.stderr,
    )
    failures += 1

if not wrapper_future_target_path.is_file():
    print(
        f"forbidden-surface-scan: compiled wrapper target missing {wrapper_future_target_path}",
        file=sys.stderr,
    )
    failures += 1
else:
    # Stage 5D-b1 moves the current compiled wrapper from a whole-file hash to
    # dual-baseline region enforcement. The immutable Stage 5C closure hash is
    # still checked by scripts/stage5d_additive_freeze_check.py after removing
    # approved Stage 5D additive bridge regions.
    pass

duplicate_scan_excluded_parts = {
    ".git",
    "target",
    "tmp",
    "reports",
    "__pycache__",
}
for candidate in Path(".").glob("**/*"):
    if not candidate.is_file() or any(
        part in duplicate_scan_excluded_parts for part in candidate.parts
    ):
        continue
    try:
        candidate_sha256 = hashlib.sha256(candidate.read_bytes()).hexdigest()
    except OSError as error:
        print(
            f"forbidden-surface-scan: cannot hash source candidate {candidate}: {error}",
            file=sys.stderr,
        )
        failures += 1
        continue
    if candidate_sha256 == wrapper_oracle_sha256 and candidate != wrapper_oracle_path:
        print(
            "forbidden-surface-scan: duplicate wrapper oracle source is forbidden "
            f"before Stage 5B-2b: {candidate}",
            file=sys.stderr,
        )
        failures += 1

for candidate in sorted(workspace_source_candidate_files):
    candidate_source = candidate.read_text(errors="replace")
    candidate_tokens, candidate_string_fragments = rust_tokens_and_string_fragments(
        candidate_source
    )
    has_wrapper_identifier = "HybridIntradayRuntimeStrategy" in candidate_source
    has_forbidden_include = "include" in candidate_tokens
    has_forbidden_path_attribute = has_path_meta_in_attribute(candidate_tokens)
    has_macro_path_assignment = has_path_assignment_in_macro_invocation(
        candidate_tokens
    )
    has_generated_attribute = has_macro_generated_attribute(candidate_tokens)
    decoded_string_surface = "".join(candidate_string_fragments)
    has_oracle_reference = any(
        marker in decoded_string_surface
        for marker in wrapper_oracle_reference_markers
    )
    unapproved_oracle_reference = (
        has_oracle_reference
        and candidate not in allowed_wrapper_oracle_include_str_paths
    )
    is_approved_wrapper_target = candidate == wrapper_future_target_path
    wrapper_markers = [
        candidate.name == wrapper_oracle_filename and not is_approved_wrapper_target,
        has_wrapper_identifier and candidate not in allowed_wrapper_identifier_paths,
        has_forbidden_include,
        has_forbidden_path_attribute,
        has_macro_path_assignment,
        has_generated_attribute,
        unapproved_oracle_reference,
    ]
    if any(wrapper_markers):
        print(
            "forbidden-surface-scan: Stage 5B-2 wrapper surface is outside the "
            f"single approved target: {candidate}; approved path is "
            f"{wrapper_future_target_path}",
            file=sys.stderr,
        )
        failures += 1

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
    "8246636dfc245e4fd4cdd99c8e09496a6cb11a3f1dbc03605fc50aae58f90e07"
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
    "dc6625f571f07954c85e397e0e9835ed64cc73b843c9ad3f6b89565d10295e25"
)
if not semantic_lib_path.is_file():
    print(
        "forbidden-surface-scan: strategy-runtime-core src/lib.rs missing",
        file=sys.stderr,
    )
    failures += 1
else:
    # Stage 5D-b1 allows additive Stage5d* exports in lib.rs. The historical
    # Stage 5C lib.rs hash is still enforced by the Stage 5D checker after
    # stripping approved additive regions.
    pass

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
            "target_sha256": "67224b1523d3eeeae924f10c77cb74582671dc24a5badef554843fe57d079fd1",
            "change_class": "ControlledStage5dSourceOwnedCodec",
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
            "target_sha256": "e5b80db163b0d97cfd50b8ad064c076850dbd2c15a95833895f5beb7a66d71a6",
            "change_class": "ControlledStage5dSourceOwnedCodec",
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

    expected_semantic_production_paths = expected_target_paths | {
        str(semantic_lib_path),
        str(semantic_kernel_root / "src/hybrid_intraday_runtime.rs"),
        str(semantic_kernel_root / "src/runtime_compat.rs"),
        str(semantic_kernel_root / "src/stage5c_paper_host.rs"),
        str(semantic_kernel_root / "src/stage5d_persistence.rs"),
    }
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
        str(semantic_kernel_root / "tests/stage5c_paper_host_admission.rs"),
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
            "2226fc838e69d00027778f3824dfe4d40c84b1b0cb888106d18df2339f20affb"
        ),
        semantic_kernel_root / "tests/stage5c_paper_host_admission.rs": (
            "3a2e28bd8f5f9448ad5d01d0032c7c4393b6881795dc9574b066c7688c4305b4"
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
    "5a53d02d31468cf44f35b623fbddaf7938ce780988a85ed239615a3ba09d3397"
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
        "schema_version": 3,
        "stage": "Stage5B2b",
        "status": "BoundaryHardenedBrokerNeutralPaperNoSend",
        "oracle_sha256": wrapper_oracle_sha256,
        "oracle_line_count": 6203,
        "accepted_stage5b1_manifest_unchanged": True,
        "target_sha256": "7cff5ee4646720c996e066ed9c7aafeae80dd80cfd8f89b3e09725e26dc8fb1b",
        "target_line_count": 6886,
        "wrapper_copied": True,
        "wrapper_compiled": True,
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
        "target_path": str(wrapper_future_target_path),
        "library_export": "private module plus approved root facade re-exports",
        "activation_gate": "Stage5B2bAcceptedTargetOnly",
        "currently_allowed_in_rust_target_set": True,
    }
    if approved_future_target != expected_future_target:
        print(
            "forbidden-surface-scan: Stage 5B-2 approved future target drifted",
            file=sys.stderr,
        )
        failures += 1

    expected_broker_neutral_compatibility = {
        "path": "crates/strategy-runtime-core/src/runtime_compat.rs",
        "request_id": "broker_core::StrategyRequestId",
        "order_id": "broker_core::BrokerOrderId",
        "stop_order_id": "broker_core::BrokerStopOrderId",
        "callback_adapter": "BrokerNeutralHybridStrategy",
        "source_compatible_host_api_public": False,
        "exclusive_public_callback_facade": True,
        "callback_result": "Result<Vec<BrokerNeutralHybridIntent>, HybridRuntimeCallbackValidationError>",
        "context_payload_instrument_validation": True,
        "configured_target_symbol_validation": True,
        "validation_before_state_mutation": True,
        "runtime_host_attached": False,
        "command_consumer_attached": False,
        "live_send_enabled": False,
    }
    if stage5b2_manifest.get("broker_neutral_compatibility") != expected_broker_neutral_compatibility:
        print(
            "forbidden-surface-scan: Stage 5B-2 broker-neutral compatibility boundary drifted",
            file=sys.stderr,
        )
        failures += 1

    expected_regions = {
        "imports": (1, 22, 22, "472f0d6ac4ef8fd240a24d1c544d564ab2826626d2f9064c1e4a20ea45878506"),
        "config_state_types": (23, 208, 186, "f84c0183858747ffe6988ed6278cd4cc5361a97df4d036373f5bab55626155f9"),
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
        expected_region_status = {
            "imports": "MechanicallyMigrated",
            "config_state_types": "MechanicallyMigrated",
            "wrapper_implementation": "MechanicallyMigrated",
            "oracle_unit_tests": "MechanicallyMigratedAndPassing",
            "strategy_callback_impl": "MechanicallyMigratedWithBrokerNeutralAdapter",
        }[name]
        expected_region_fields = {
            "line_start": line_start,
            "line_end": line_end,
            "line_count": line_count,
            "sha256": expected_region_sha256,
            "implementation_status": expected_region_status,
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
        "df802340f462ce4074eb9dda291b4165123d2bb17cc77bf135906fa7622e124d"
    ),
    Path("tests/fixtures/stage5/stage5c_paper_host_admission.json"): (
        "821e241970df245f7aaaeb78312537c29512173108c59f40f7f449eb44cb8aa4"
    ),
    Path("docs/stage-5/stage-5c-acceptance-api-freeze-report.md"): (
        "1d15c992ce1658fea6d7ec8a25094b094400ba00b764ac23d32c525207d19b48"
    ),
    Path("docs/stage-5/stage-5c-api-freeze-manifest.json"): (
        "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323"
    ),
    Path("docs/stage-5/stage-5d-additive-freeze-manifest.json"): (
        "00d602cf0317235ad0a325491dc47099eef9d7178bfaf46a1d593c477d01cf5c"
    ),
    Path("docs/stage-5/5d-b2a-versioned-persistence-envelope-api-schema.md"): (
        "9f6cc0f7a07c08f5fc67e6ef7904ced2c20b7f6a995204e288d6952792e034a6"
    ),
    Path("scripts/stage5c_api_freeze_check.py"): (
        "2ed629e4e7a157f03b25e55f7b294713855d84a5a9cef3b284d58baa60bc257d"
    ),
    Path("scripts/stage5d_additive_freeze_check.py"): (
        "0abbc6523516814c22e66faac5e2384a48477f664e3873b88841fafe17402917"
    ),
    Path("scripts/stage5d_additive_freeze_negative_harness.py"): (
        "0b6746fb61871d8cc047b1ae96d32f6f6ea498da9f4820f3a32b62c413930daa"
    ),
    Path("tests/fixtures/stage5/stage5c_api_freeze_check.closure.py"): (
        "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209"
    ),
    Path("tests/fixtures/stage5/stage5ch_controlled_next_bar_loop.json"): (
        "687a94ea97c437715039dc8f44c53539094c89d2c5e9c34d83162e24515f2699"
    ),
    Path("tests/fixtures/stage5/stage5ci_paper_intent_lifecycle.json"): (
        "9c23a730d142f47882ba08cd9d86b8f354c235e37048aae6c49e628422bc86de"
    ),
    Path("tests/fixtures/stage5/stage5cj_paper_broker_lifecycle.json"): (
        "14cdd518383b86d5a223d65ef46969450ea7a8573df21c6c6c520b817571621d"
    ),
    Path("tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"): (
        "40243c150960af80fe2c914555a8527674e60c87fd31cc6efdbac44cdec95cf9"
    ),
    Path("tests/fixtures/stage5/stage5d_b2a_persistence_envelope_corrupt_checksum.json"): (
        "a6f9e5f7fe3b3c74370b2c75051176454bd932322826d46bcc4713684a566c13"
    ),
    Path("tests/fixtures/stage5/stage5d_b2a_persistence_envelope_bad_version.json"): (
        "aee3d07d30520622cd81f7709f51c0093c64d28d444bfe4ddd92331c4917d104"
    ),
    Path("tests/fixtures/stage5/stage5d_b2a_persistence_envelope_empty_config.json"): (
        "3e66147ec3625a68a7d5ae7552e94aeb4ba9cebf00e471d58e8bb83bd6bf83f0"
    ),
    Path("tests/fixtures/stage5/stage5cb_bootstrap_notification.json"): (
        "9db7888d27374eeaa8ac046c1df1727a3e3cd085c938a76945d22b9bd68f00de"
    ),
    Path("tests/fixtures/stage5/stage5cc_runtime_state_restore.json"): (
        "ca28733fb7321450d56cd017f410ac5bf1214d81a63435c5d4a2302a64165bc8"
    ),
    Path("tests/fixtures/stage5/stage5cd_history_warmup.json"): (
        "b71ffc65017a5bf172ffef2c5da26d28aabe45eb7833535481de200df86bda4b"
    ),
    Path("tests/fixtures/stage5/stage5ce_pending_recovery.json"): (
        "4bea0ec0cf2a066c29024d909f846426f12b39fe2c4546eb633cf7f906cdc28c"
    ),
    Path("tests/fixtures/stage5/stage5cf_semantic_bar.json"): (
        "e33b96f5eda32274bf7e771f2d365f726fc1929d7d8f62ba237b522566ffee96"
    ),
    Path("tests/fixtures/stage5/stage5cg_paper_intent_settlement.json"): (
        "810d7d22f3e9f0bd5fe1d162e36dd2b2981e8553d9419fbce501e43cb3449d3e"
    ),
    Path("crates/broker-core/src/hybrid_strategy_boundary.rs"): (
        "c154754d3be57bc5566ee8cfde5d2ec552dea31afc7e56a7277d4592f219157d"
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

"$python_with_tomllib" scripts/stage5c_api_freeze_check.py
"$python_with_tomllib" scripts/stage5d_additive_freeze_check.py

if (( failures > 0 )); then
  exit 1
fi

echo "forbidden-surface-scan: ok"
