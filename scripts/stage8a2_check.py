#!/usr/bin/env python3
"""Fail-closed Stage 8A-2 R1 builder/no-send composition scanner."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "8e7c4426ba075e66441dc998a82a786b34fdd0b5"
ACCEPTED_STAGE8A1 = "1ff04154ba4b7a5ee060a73b853ce89bd7442f44"
BRANCH = "stage8a2-builder-composition"
MODULE = Path(
    "crates/finam-gateway/src/stage8a1_execution_capability/"
    "stage8a2_builder_composition.rs"
)
PARENT = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
DESCRIPTOR = Path("docs/stage-8/stage8a2-r1-implementation-descriptor.json")
AUTHORITY = Path("docs/stage-8/stage8a2-r1-contract-authority.json")
MATRIX = Path("docs/stage-8/STAGE8A_2_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_2_R1_NEGATIVE_INVENTORY_2026-08-15.md")
ENTRY = Path("docs/stage-8/stage8a2-entry-contract.md")

ALLOWED_CHANGED_PATHS = {
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    str(MODULE),
    "docs/current-status.md",
    "docs/roadmap.md",
    str(AUTHORITY),
    str(DESCRIPTOR),
    str(ENTRY),
    str(MATRIX),
    str(INVENTORY),
    "scripts/make_stage8a2_handoff_archive.py",
    "scripts/stage8a2_check.py",
    "scripts/stage8a2_gate.sh",
    "scripts/stage8a2_handoff_safety_check.py",
    "scripts/stage8a2_negative_harness.py",
    "scripts/stage8a2_proof_map.py",
}

# Filled only for the final candidate. Semantic checks remain independently
# mutation-tested when pin_hashes=False.
PINNED_FINAL_SHA256: dict[Path, str] = {
    MODULE: "5558fcfa18882bc7bc548a79fe3ac248db76c9b2e3365410616aacf53444c892",
    PARENT: "e80948e1a7643daa3b033879c53227f6a40a63416ef24e38a0e2280f49e1b769",
    LIB: "5695b230af3de1a46e43a88db9587ccb3ae872209629a44365cac7873040b2df",
    DESCRIPTOR: "3f736d695b748664a150108ce3fc26c71932201fa16c3b6e9d6e5dbc23a006f9",
    AUTHORITY: "a516dab995b8ee05dab649375c216dd8e3c87c09f1400e010942a7788ef94f50",
    MATRIX: "98d1f9c123401f77f36d423a0fca94471e82d94c85915aa4a58b25afef227ee3",
    INVENTORY: "daf223a6cf212c2cfa36ba73fe6b04a4aabb7a4b010c99534a61f8684152bbe5",
    ENTRY: "b5dcca978eeac7caff8d16f5f771c1a11998f6746636ac8330346d0fd5548f1c",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def code_without_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def accepted_file(path: Path) -> bytes:
    return subprocess.check_output(
        ["git", "show", f"{ACCEPTED_STAGE8A1}:{path.as_posix()}"], cwd=ROOT
    )


def check_successor_parent_delta(root: Path) -> None:
    accepted_parent = accepted_file(PARENT).decode()
    expected_parent = accepted_parent.replace(
        "\nuse broker_core::{\n",
        "\nmod stage8a2_builder_composition;\n\n"
        "pub use stage8a2_builder_composition::{\n"
        "    Stage8a2BuilderCompositionDiagnostic, Stage8a2BuilderCompositionError,\n"
        "    Stage8a2InMemoryNoSendSink, Stage8a2RequestShapeKind,\n"
        "};\n\nuse broker_core::{\n",
        1,
    )
    require((root / PARENT).read_text() == expected_parent, "Stage8A1 successor delta drift")

    accepted_lib = accepted_file(LIB).decode()
    expected_lib = accepted_lib.replace(
        "    Stage8a1ZeroAmbiguityAuthority,\n",
        "    Stage8a1ZeroAmbiguityAuthority, Stage8a2BuilderCompositionDiagnostic,\n"
        "    Stage8a2BuilderCompositionError, Stage8a2InMemoryNoSendSink, "
        "Stage8a2RequestShapeKind,\n",
        1,
    )
    require((root / LIB).read_text() == expected_lib, "crate export delta drift")


def check(
    root: Path = ROOT,
    *,
    git_scope: bool = True,
    pin_hashes: bool = True,
    exact_parent_delta: bool = True,
) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["stage"] == "8A-2-R1", "stage drift")
    require(
        descriptor["status"] == "builder_composition_no_send_candidate",
        "self acceptance or status drift",
    )
    require(descriptor["transition_baseline"] == BASE, "transition baseline drift")
    require(descriptor["accepted_stage8a1_ref"] == ACCEPTED_STAGE8A1, "predecessor drift")
    require(descriptor["acceptance_rows"] == 50, "acceptance count drift")
    require(descriptor["negative_cases"] == 37, "negative count drift")
    require(all(descriptor["required"].values()), "required proof disabled")
    require(all(descriptor["closed"].values()), "closed surface opened")
    require(descriptor["next_after_acceptance"] == "Stage 8A-3 only", "next scope drift")

    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["transition_baseline"] == BASE, "R1 authority baseline drift")
    require(authority["accepted_stage8a1_ref"] == ACCEPTED_STAGE8A1, "R1 authority predecessor drift")
    require(
        authority["transition_review_sha256"]
        == "9319b2dea111359489050a52384edbc1fc7aa2a394ab0ef41557e8af0e5fd316",
        "transition review authority drift",
    )
    require(
        authority["r1_tz_sha256"]
        == "c12d4a206988e1ec2048a6ec2de9defb517fc271e373f87ee24ff2f337a751da",
        "R1 TZ authority drift",
    )

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 50, "acceptance matrix must contain 50 rows")
    require(
        [row["id"] for row in rows] == [f"S8A2R1-{index:03d}" for index in range(1, 51)],
        "acceptance matrix IDs drift",
    )
    require(all(row["mandatory"] == "YES" for row in rows), "optional acceptance row introduced")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 37, "negative inventory drift")

    source = (root / MODULE).read_text()
    code = code_without_comments(source)
    production = code.split("#[cfg(test)]\nmod tests", 1)[0]

    require(
        re.search(
            r"pub fn compose_stage8a2_no_send\s*\(\s*self\s*,\s*"
            r"sink:\s*&mut Stage8a2InMemoryNoSendSink",
            production,
            re.S,
        )
        is not None,
        "single by-value continuation seam missing",
    )
    require(
        production.count("build_place_order_request(&approved, None)") == 1,
        "PLACE must call the existing builder exactly once with None",
    )
    require(
        production.count("build_cancel_order_request(&approved)") == 1,
        "CANCEL must call the existing builder exactly once",
    )
    require(
        "Stage8a1CurrentlyAuthorizedCapability {" in production
        and "capability.approved" in production,
        "fresh continuation is not consumed in the privacy domain",
    )
    require(
        "struct Stage8a2OpaqueRequestShapeWitness" in production
        and "pub struct Stage8a2OpaqueRequestShapeWitness" not in production,
        "opaque witness visibility drift",
    )
    witness_prefix = production.split("struct Stage8a2OpaqueRequestShapeWitness", 1)[0]
    require(
        not re.search(r"#\[derive\([^\]]*(?:Debug|Clone|Serialize)", witness_prefix[-180:]),
        "opaque witness gained Debug/Clone/Serialize",
    )
    require(
        "fn consume(\n        &mut self,\n        witness: Stage8a2OpaqueRequestShapeWitness"
        in production,
        "private no-send sink consumer missing",
    )

    for forbidden in (
        "M3d2RealOrderEndpointTransport",
        "EndpointGateApproved",
        "m3j16-actual-one-shot",
        "reqwest",
        ".post(",
        ".delete(",
        ".send(",
        "place_order_execution",
        "cancel_order_execution",
        "ProvenNoMatch",
        "reconcile_",
        "http_status",
        "automatic_retry",
        "redis::",
        "runtime_live",
        "stop_sltp",
        "bracket",
        "multi_leg",
        "base_url",
        "reconstruct_capability",
        "reconstruct_approved_command",
        "hand_build_place",
        "hand_build_cancel",
        "serialize_place_order",
        "raw_body",
        "raw_path",
        "raw_identifier",
        "transport_ready",
        "real_strategy_order",
    ):
        require(forbidden not in production, f"forbidden Stage8A2 surface: {forbidden}")

    for forbidden_api in (
        "approved(",
        "into_approved(",
        "raw_request(",
        "raw_body(",
        "raw_path(",
        "rest_path_segments(",
    ):
        require(
            not re.search(rf"pub(?:\(crate\))?\s+fn\s+{re.escape(forbidden_api)}", production),
            f"raw/extraction API introduced: {forbidden_api}",
        )

    require(
        re.search(
            r"pub(?:\(crate\))?\s+fn[^{{;]*(?:FinamPlaceOrderRequestSpec|"
            r"FinamCancelOrderRequestSpec|PreflightApprovedPlaceOrder|"
            r"PreflightApprovedCancelOrder)",
            production,
            re.S,
        )
        is None,
        "raw approved/request type escaped through public function",
    )
    diagnostic_block = production.split("pub struct Stage8a2BuilderCompositionDiagnostic", 1)[1].split("}", 1)[0]
    for raw_field in (
        "account_id: String",
        "order_id: String",
        "client_order_id: String",
        "raw_",
        "url:",
        "body:",
    ):
        require(raw_field not in diagnostic_block, f"raw diagnostic field introduced: {raw_field}")

    require("comment_present: body.comment_present" in production, "comment absence evidence missing")
    require("day_time_in_force_present" in production, "DAY mapping evidence missing")
    require("cancel_request_shape_sha256" in production, "CANCEL exact identity fingerprint missing")
    require("Stage8a2InMemoryNoSendSink" in production, "no-send sink missing")

    if exact_parent_delta:
        check_successor_parent_delta(root)

    if git_scope:
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root, check=True)
        changed = set(
            subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=root, text=True).splitlines()
        )
        changed.update(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
            ).splitlines()
        )
        require(changed == ALLOWED_CHANGED_PATHS, f"changed-path drift: {sorted(changed)}")
        require(not any(path.startswith(".github/") for path in changed), "CI workflow drift")
        require(not any(path.endswith("Cargo.toml") or path == "Cargo.lock" for path in changed), "Cargo drift")

    if pin_hashes and PINNED_FINAL_SHA256:
        for path, expected in PINNED_FINAL_SHA256.items():
            require(sha256(root / path) == expected, f"final candidate hash drift: {path}")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8a2-r1-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a2-r1-check: PASS rows=50 builder-only=true comment-none=true no-send=true next=8A-3-pending")


if __name__ == "__main__":
    main()
