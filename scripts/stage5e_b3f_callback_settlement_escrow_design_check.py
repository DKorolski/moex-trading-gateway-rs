#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3f settlement-escrow design."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md"
INVENTORY = (
    ROOT
    / "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
STAGE = "5E-b3f-callback-settlement-escrow-design"
BASELINE_REF = "a5ccea08bc64a66e768340f7121e9b94a09ff884"
EXPECTED_PLAN_SHA256 = (
    "6187b009f537ea68f694be92dd1c5cede4d647a1ddef549c9239867f06bbc45f"
)
EXPECTED_INVENTORY_SHA256 = (
    "2b358ccfb730172f07bc281da23a967f6dc1379007a1a6ca124eb3ea132696a6"
)
EXPECTED_PROTECTED_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "d7458cc5acb0004c9a82eb42675ca7a3672f7c584cd686a1ddaa0b72d8035e41"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "75e3e30deff70fd58f740361395bb82c32981bd6107831dfb21ff037591c6b7d"
    ),
}
EXPECTED_IMPLEMENTATION_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/lib.rs": (
        "4a248db1a97799604bcfcb094abd1b22abebc98aec67882c829e1fa5a884e7ae"
    ),
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6"
    ),
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5d_additive_freeze_check.py",
    "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py",
    "scripts/stage5e_b3f_production_ui_harness.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]
EXPECTED_STAGE5C_ERROR_MAPPING = {
    "TooManyIntents": "IntentCapacityExceeded",
    "MissingIntentClass": "Stage5cIntentValidationFailed",
    "InstrumentNamespaceMismatch": "Stage5cIntentValidationFailed",
    "InvalidQuantity": "Stage5cIntentValidationFailed",
    "InvalidPrice": "Stage5cIntentValidationFailed",
    "PriceNotTickAligned": "Stage5cIntentValidationFailed",
    "InvalidStopEnd": "Stage5cIntentValidationFailed",
    "ReplayIntentNotExecutable": "PaperModeMismatch",
    "MissingPendingRequest": "Stage5cPendingRequestMismatch",
    "RequestIdMismatch": "Stage5cPendingRequestMismatch",
    "DuplicateRequestId": "Stage5cIntentValidationFailed",
    "UnsupportedIntentAction": "Stage5cIntentValidationFailed",
}
RUST_RAW_STRING_RE = re.compile(r'(?:br|r)(?P<hashes>#{0,255})"')
RUST_IDENTIFIER_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
RUST_NUMBER_RE = re.compile(r"[0-9][0-9A-Za-z_]*")
EXPECTED_STAGE5C_B3F_FUNCTIONS = {
    "validate_stage5e_b3f_retained_close_chronology",
    "validate_stage5e_b3f_stage5c_preflight_binding",
    "issue_stage5c_b3f_settlement_material_seal",
    "issue_stage5c_b3f_settlement_seal",
    "construct_stage5e_stage5c_settlement_material",
    "borrow_identity_proof",
    "construct_stage5e_success_receipt",
    "test_identity_proof_shape",
    "construct_stage5e_terminal_receipt",
    "settle_stage5e_callback_escrow_material",
}
EXPECTED_METADATA_FUNCTIONS = {
    "validate_stage5e_b3f_retained_close_chronology",
    "validate_stage5e_b3f_stage5c_preflight_binding",
    "construct_stage5e_stage5c_settlement_material",
}
SETTLEMENT_SEALS = {
    "Stage5ePaperSettlementPreflightSeal",
    "Stage5ePaperSettlementConsumeSeal",
    "Stage5ePaperSettlementSuccessSeal",
    "Stage5ePaperSettlementTerminalSeal",
}
EXPECTED_STAGE5E_CALLBACK_PRODUCTION_SIGNATURES = {
    "use:super :: { DateTime , Digest , PrivateStage5ePaperCallbackOutcome , Stage5eAuthorizedCallbackAuditLineage , Stage5ePaperCallbackOutcome , Stage5ePaperCallbackResultEscrow , Utc , }": "8553dc037c811e5990e9bb238d9f894ceac9596843d310a24378eeed56eb53cf",
    "use:sha2 :: Sha256": "ea56b83343ac6ec041cb8bdee6d5d31ffdab2127e859b6ed8e2ef4faae34a268",
    "struct:Stage5ePaperSettlementPreflightSeal": "d8191f157b81f2801c21459cb0da6904a0bf23e74bf679d0653f562dfa9834f5",
    "struct:Stage5ePaperSettlementConsumeSeal": "f0fe9e5be02987a12d5086e0f64b01f5f65355ff73e39e6557f6e8200309079a",
    "struct:Stage5ePaperSettlementSuccessSeal": "fa426c1944af0d666a3d859eff4af1d6f9c5bf2d3f04ff634ff4eadd2caebad8",
    "struct:Stage5ePaperSettlementTerminalSeal": "b6c901fa529f7c249b9674d49737b1f339b4809a370bfc0cbe1f042d3935d303",
    "struct:Stage5eB3fAuditCommitmentSeal": "268347067c10271517849bd4b899b205c8ab2f111133edac22cc7cce718cd325",
    "struct:Stage5ePaperSettlementPreflight": "6440c3caa4ba29821dcc79cf161eb50aba1625a22f7865b21f49d7592afa3e6d",
    "impl:impl < ' a > Stage5ePaperSettlementPreflight < ' a >": "9d5e6a5320057730b50f809a19717d60e3fac4346e10ea0ca12d52a153a0e329",
    "struct:Stage5ePaperSettlementPayload": "c2533383b09c8a1065563796aacbe3ae9fca50ff98bf6e59e51aea23fc71ae15",
    "impl:impl Stage5ePaperSettlementPayload": "245f47e91f0c7827cbbc0c252ec57b02e57ed9726ff45582747a818a46176401",
    "enum:Stage5ePaperSettlementPreflightDecision": "c3e73cc1a35f94a7eeda8412a59dbe142d18337e53d728bfe796ed3a94b3f07d",
    "enum:Stage5ePaperSettlementTerminalReason": "fa88cabe46f353200d4f6ec0bdd98e3d460b4d135975e057cbc2d56a42f68415",
    "enum:Stage5ePaperSettlementTerminalOwnership": "0eac4d90bea5f86f252785545d9aa6f3c4058702fa292bc99d866b0350c9dccb",
    "struct:Stage5ePaperSettlementTerminalReceipt": "65f39364e0835be80c351dd6d9480d6fa7a2c3cbfdc3bbfeccaff78e9c7664cd",
    "struct:Stage5eValidatedPaperSettlementReceipt": "24cb76153f18905d68b1bfa9dbbe0700faf04cab5ce6da4d14cce6e69ee65df7",
    "fn:validate_and_settle_stage5e_paper_callback_escrow": "6dce9b0a6d0016a2c8689e96ae53d88d967e3985a456a2044abe0d5cadafb4ba",
    "fn:validate_preflight": "2a6fcd86a0ad57afdfa5c593291f39fe47250dd8e685877f65c56c2b0463f666",
    "fn:construct_stage5c_expected_preflight_binding": "56c00cba0c272a5bbf2dbbc1795f07859e46e30ffcb2cf5646999640f518c041",
    "fn:stage5e_authority_identity_matches": "a70ab32080b3b80dbd3c8a2b7eab936c7c13d3d97bce11472c189c9954bab1b2",
    "fn:stage5e_audit_chronology_matches": "449012f962f95f544051237a198a00e9053da95cb027e922e721f00b62b4280a",
    "fn:map_stage5c_preflight_mismatch_exact": "4288bfe456544836eb19dcc479f75385d33fd56048983d04c7c3459d24debab1",
    "fn:construct_preflight_terminal_receipt": "4593ce9f90576414b60453d64a44f05d7ded55ce0b400c6408bd3dfc532228fd",
    "fn:construct_stage5e_paper_settlement_terminal_receipt": "97d899dd701c0b3bbc2fddbf04e78fcc86574d71a2651c6f08c5b844826670da",
    "fn:construct_stage5e_validated_paper_settlement_receipt": "66938653086e0802eb093b1d3a385c2dfce8082448ad19e8875560e4432b72dd",
    "fn:map_stage5c_settlement_error_exact": "1bedc413ab695d896dc522f0fcdba6e12c19b2c310ea86652c27166d5dadfe25",
    "fn:construct_stage5e_b3f_audit_commitment": "16288ab5fc17c2d4a93d4a850f7c8ee05e10a013b852a98de6665c05808e1ca0",
    "fn:construct_stage5e_b3f_settlement_identity": "133c4504e772f9953bd913d90f53a3bb7382492dc4e44260d664a3c935396999",
    "struct:Stage5eB3fCanonicalEncoder": "c31b03874a6ee0d84dc8c683baf0b387e21ee3d705ba6136c7621e5f13e6b198",
    "impl:impl Stage5eB3fCanonicalEncoder": "79901ec7f133d560c4b87eb10cd692ca928ac61f4509076f01dfdf6c7836275a",
}
EXPECTED_STAGE5C_REGION_PRODUCTION_SIGNATURES = {
    "struct:Stage5eB3fStage5cExpectedPreflightBinding": "a5656ce39a3cebb43c969903954849b972d257939c4a7bd22121274c66879929",
    "fn:construct_stage5e_b3f_stage5c_expected_preflight_binding": "66af6b506a207f51cc39a9843c51011efb60cb6013ce0a3d56fdf7c4dd6c01e5",
    "enum:Stage5eStage5cPreflightMismatch": "be0bfad9e479d762431ec951e2c080d9aa76a7ca082f5d1a21e7569d0decb5e7",
    "struct:Stage5eStage5cPreflightValidatedProof": "f6f6c004ce6e255204958730a88dd3e06dd1845162f3028b5807f8528b5ba712",
    "struct:Stage5eStage5cRetainedCloseChronologyProof": "e7d53ba56f0a99a5c5372d8372e61ab0f41401b6fbaf2b6d2eeed485fc2aa940",
    "struct:Stage5eStage5cRetainedCloseChronologyMismatch": "35704f907f07ea9d86eb06c79815044fefd97d50b79db98b0d07a80dc0375b21",
    "fn:validate_stage5e_b3f_retained_close_chronology": "fe0b98144cf1ef5c20fa40bda3d481d79323e6849eec9a5da5a5c81654fdcb24",
    "fn:validate_stage5e_b3f_stage5c_preflight_binding": "9098eaa56cad17627b07d141bff050422410e842ca0e87ed6ca333931882696f",
    "struct:Stage5cB3fSettlementMaterialSeal": "6cade0e51c91b18732060128184a2687fed8b263211426450d367c2c46f0dd12",
    "struct:Stage5cB3fSettlementSeal": "3bde00039766d09227cea3254064d09eab3a6da7aad91024a93e90a1f62215ba",
    "struct:Stage5cB3fSuccessProofSeal": "06d6686fa8e2bc8b7be110478e2394d55eb4cce4063ade5f41123deb2de566d3",
    "fn:issue_stage5c_b3f_settlement_material_seal": "bfcaaa1a68aa0e59691025101d4837ad43b5b91673bab0ae3e10651a94f4ed05",
    "fn:issue_stage5c_b3f_settlement_seal": "d4020ac603259d1cb8403dc915dc68c0f572a7b65ce938c2cf3ac826f90b84fe",
    "struct:Stage5eStage5cSettlementMaterial": "c59f448aac71a486977924e0414d96beebfc26eff087400fb696ef77efa857cf",
    "fn:construct_stage5e_stage5c_settlement_material": "3a432b587c02f93a7b215c8b43d42d7d278d6439a98896f995bd05e95e7d28df",
    "struct:Stage5eStage5cSettlementSuccess": "5d0e9f84c1adc70c989f5de2489c423138525d8a3ae84984b89854704be0d883",
    "struct:Stage5eStage5cSettlementSuccessProof": "2db0ce6687181d21d3e27171334f6d41952c7aad4e00a4c4c2707b90f8fb6693",
    "struct:Stage5eStage5cSettlementTerminalMaterial": "a146589866fb90472fb527425b35440a848d569c0295f444e5f5c882fdf6b106",
    "impl:impl Stage5eStage5cSettlementSuccess": "7b80b92aa3693dcdd3d3d1f8edc1c060f2de9a6deafe8c2a9a474bb34420ca2a",
    "impl:impl Stage5eStage5cSettlementTerminalMaterial": "3657a97a30b119568c5f9abda29848e7b8367a44f7198e2fb2ce8b2b8cd6f96c",
    "fn:settle_stage5e_callback_escrow_material": "c0e56272729a58c4b8c0b534a1474f00d340ce7a7fd9c023e4a5ef934594f8a2",
}
EXPECTED_SENSITIVE_TOKEN_WINDOWS_SHA256 = {
    "stage5c:Stage5ePaperSettlementPreflightSeal": (
        "27500e736fe869d9b65a50aea40321dafa462f28f3d9fb4f261be0921fa0fad3"
    ),
    "stage5c:Stage5ePaperSettlementConsumeSeal": (
        "cb10b2fde66ece99aae75326c8270b806de81c3792032595aaf07b31004a45f7"
    ),
    "stage5c:Stage5ePaperSettlementSuccessSeal": (
        "92ae5636cb55d1538b31f299fd30ae66e2ca2f31cbab94f229d97545108218a8"
    ),
    "stage5c:Stage5ePaperSettlementTerminalSeal": (
        "89afe8634b68a0473ce88edb61ef4220d41538b1bb7944f16194fc604189df0b"
    ),
    "stage5c:Stage5eAcceptedBarSettlementMetadata": (
        "e5377384c28fc814a4340b155fd4a16562e30a3c5ff55d8a4bf790f18e51ddc0"
    ),
    "stage5c:accepted_bar_close_ts": (
        "db5f178876b512f3a9f5c832600895f479f2ba03d90b3d5c80f688284e5a402d"
    ),
    "stage5e:Stage5eAcceptedBarSettlementMetadata": (
        "e15d104026cbb730c820d6b83ee40ea65029746a4f98e241b901ff098b27a452"
    ),
    "stage5e:Stage5ePaperSettlementConsumeSeal": (
        "41f6364e15c95a8a1b28ab2f143833e843c13672abb36bb715729bde162f4c85"
    ),
    "stage5e:Stage5ePaperSettlementPreflightSeal": (
        "a61f7ee9d715744296e6c351cd6d7543c9a9b8ccf0bec4730e60b26601a998bd"
    ),
    "stage5e:Stage5ePaperSettlementSuccessSeal": (
        "ee275db01e422f9d3e4138cb6a8f3c696810df44725a6489c4338bef9602d8aa"
    ),
    "stage5e:Stage5ePaperSettlementTerminalSeal": (
        "41cd495ba86609b8f2db51cf19f49445dc2d6804a683aa81d27d0f5a78401055"
    ),
    "stage5e:Stage5ePaperSettlementPayload": (
        "2ba8b8877f0821da839ee8bae80677c17dca0498b3770d55327d85121ba3d8de"
    ),
}


def fail(message: str) -> None:
    print(
        f"stage5e-b3f-callback-settlement-escrow-design-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def require_exact(actual: object, expected: object, message: str) -> None:
    if actual != expected:
        fail(message)


def require_source_fragment(source: str, fragment: str, message: str) -> None:
    if fragment not in source:
        fail(message)


def rust_tokens(source: str) -> list[str]:
    """Return identifiers and punctuation after removing Rust comments/literals."""
    tokens: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = length if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while index < length and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue
        raw_match = RUST_RAW_STRING_RE.match(source, index)
        if raw_match:
            terminator = '"' + raw_match.group("hashes")
            index = raw_match.end()
            finish = source.find(terminator, index)
            index = length if finish < 0 else finish + len(terminator)
            continue
        if source[index] == '"':
            index += 1
            while index < length:
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            continue
        identifier = RUST_IDENTIFIER_RE.match(source, index)
        if identifier:
            tokens.append(identifier.group(0))
            index = identifier.end()
            continue
        number = RUST_NUMBER_RE.match(source, index)
        if number:
            tokens.append(number.group(0))
            index = number.end()
            continue
        pair = source[index : index + 2]
        if pair in {"::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||"}:
            tokens.append(pair)
            index += 2
            continue
        if not source[index].isspace():
            tokens.append(source[index])
        index += 1
    return tokens


def token_windows_sha256(tokens: list[str], needle: str, radius: int = 8) -> str:
    windows = [
        tokens[max(0, index - radius) : index + radius + 1]
        for index, token in enumerate(tokens)
        if token == needle
    ]
    return canonical_sha256(windows)


def balanced_item_body(tokens: list[str], item_prefix: list[str]) -> list[str]:
    for index in range(len(tokens) - len(item_prefix)):
        if tokens[index : index + len(item_prefix)] != item_prefix:
            continue
        brace = index + len(item_prefix)
        if brace >= len(tokens) or tokens[brace] != "{":
            continue
        depth = 1
        cursor = brace + 1
        while cursor < len(tokens) and depth:
            depth += tokens[cursor] == "{"
            depth -= tokens[cursor] == "}"
            cursor += 1
        if depth != 0:
            fail(f"unbalanced tokenized item: {' '.join(item_prefix)}")
        return tokens[brace + 1 : cursor - 1]
    fail(f"tokenized item missing: {' '.join(item_prefix)}")
    raise AssertionError("unreachable")


def impl_headers(tokens: list[str], protected: set[str]) -> list[list[str]]:
    headers: list[list[str]] = []
    for index, token in enumerate(tokens):
        if token != "impl":
            continue
        cursor = index + 1
        angle_depth = 0
        header = ["impl"]
        while cursor < len(tokens):
            current = tokens[cursor]
            if current == "{" and angle_depth == 0:
                break
            angle_depth += current == "<"
            angle_depth -= current == ">"
            header.append(current)
            cursor += 1
        if protected.intersection(header):
            headers.append(header)
    return headers


def bounded_statement(tokens: list[str], start: int) -> list[str]:
    cursor = start
    depths = {"(": 0, "[": 0, "{": 0}
    closing = {")": "(", "]": "[", "}": "{"}
    while cursor < len(tokens):
        token = tokens[cursor]
        if token in depths:
            depths[token] += 1
        elif token in closing:
            depths[closing[token]] -= 1
        elif token == ";" and all(depth == 0 for depth in depths.values()):
            return tokens[start:cursor]
        cursor += 1
    return tokens[start:]


def matching_delimiter(tokens: list[str], start: int) -> int:
    pairs = {"(": ")", "[": "]", "{": "}"}
    opening = tokens[start]
    if opening not in pairs:
        fail(f"delimiter expected, got: {opening}")
    stack = [pairs[opening]]
    for cursor in range(start + 1, len(tokens)):
        token = tokens[cursor]
        if token in pairs:
            stack.append(pairs[token])
        elif token in {")", "]", "}"}:
            if not stack or token != stack.pop():
                fail("unbalanced tokenized delimiter")
            if not stack:
                return cursor
    fail("unterminated tokenized delimiter")
    raise AssertionError("unreachable")


def item_attributes(tokens: list[str], start: int) -> tuple[list[list[str]], int]:
    attributes: list[list[str]] = []
    cursor = start
    while (
        cursor + 1 < len(tokens)
        and tokens[cursor] == "#"
        and tokens[cursor + 1] == "["
    ):
        finish = matching_delimiter(tokens, cursor + 1)
        attributes.append(tokens[cursor : finish + 1])
        cursor = finish + 1
    return attributes, cursor


def item_is_test_only(attributes: list[list[str]]) -> bool:
    return any(
        "cfg" in attribute
        and ("test" in attribute or "doctest" in attribute)
        for attribute in attributes
    )


def top_level_items(tokens: list[str]) -> list[dict[str, object]]:
    """Parse a bounded Rust module/region into structural top-level items."""
    items: list[dict[str, object]] = []
    cursor = 0
    brace_items = {"fn", "impl", "enum", "mod"}
    semicolon_items = {"use", "type", "const", "static"}
    while cursor < len(tokens):
        attributes, item_start = item_attributes(tokens, cursor)
        if item_start >= len(tokens):
            break
        scan = item_start
        if tokens[scan] == "pub":
            scan += 1
            if scan < len(tokens) and tokens[scan] == "(":
                scan = matching_delimiter(tokens, scan) + 1
        if scan >= len(tokens):
            fail("truncated tokenized top-level item")

        keyword = tokens[scan]
        if keyword == "const" and scan + 1 < len(tokens) and tokens[scan + 1] == "fn":
            keyword = "fn"
            keyword_index = scan + 1
        else:
            keyword_index = scan

        if keyword == "macro_rules":
            if (
                keyword_index + 3 >= len(tokens)
                or tokens[keyword_index + 1] != "!"
                or tokens[keyword_index + 3] != "{"
            ):
                fail("malformed macro_rules item")
            name = tokens[keyword_index + 2]
            body_start = keyword_index + 3
            item_end = matching_delimiter(tokens, body_start) + 1
            kind = "macro_rules"
            signature = tokens[item_start:body_start]
        elif (
            keyword_index + 2 < len(tokens)
            and tokens[keyword_index + 1] == "!"
            and tokens[keyword_index + 2] in {"(", "[", "{"}
        ):
            name = keyword
            body_start = keyword_index + 2
            item_end = matching_delimiter(tokens, body_start) + 1
            if item_end < len(tokens) and tokens[item_end] == ";":
                item_end += 1
            kind = "macro_invocation"
            signature = tokens[item_start:body_start]
        elif keyword == "struct":
            if keyword_index + 1 >= len(tokens):
                fail("truncated struct item")
            name = tokens[keyword_index + 1]
            probe = keyword_index + 2
            while probe < len(tokens) and tokens[probe] not in {"{", ";"}:
                if tokens[probe] in {"(", "["}:
                    probe = matching_delimiter(tokens, probe) + 1
                else:
                    probe += 1
            if probe >= len(tokens):
                fail(f"unterminated struct item: {name}")
            if tokens[probe] == "{":
                item_end = matching_delimiter(tokens, probe) + 1
                signature = tokens[item_start:probe]
            else:
                item_end = probe + 1
                signature = tokens[item_start:item_end]
            kind = "struct"
        elif keyword in brace_items:
            name = (
                tokens[keyword_index + 1]
                if keyword != "impl"
                else " ".join(tokens[keyword_index:])
            )
            probe = keyword_index + 1
            paren_depth = 0
            bracket_depth = 0
            while probe < len(tokens):
                token = tokens[probe]
                paren_depth += token == "("
                paren_depth -= token == ")"
                bracket_depth += token == "["
                bracket_depth -= token == "]"
                if token == "{" and paren_depth == 0 and bracket_depth == 0:
                    break
                probe += 1
            if probe >= len(tokens):
                fail(f"unterminated braced item: {keyword}")
            item_end = matching_delimiter(tokens, probe) + 1
            signature = tokens[item_start:probe]
            if keyword == "impl":
                name = " ".join(tokens[keyword_index:probe])
            kind = keyword
        elif keyword in semicolon_items:
            statement = bounded_statement(tokens, item_start)
            item_end = item_start + len(statement)
            if item_end >= len(tokens) or tokens[item_end] != ";":
                fail(f"unterminated semicolon item: {keyword}")
            item_end += 1
            name = (
                tokens[keyword_index + 1]
                if keyword != "use"
                else " ".join(statement[keyword_index - item_start + 1 :])
            )
            kind = keyword
            signature = statement
        else:
            fail(f"unrecognized tokenized top-level item: {keyword}")

        items.append(
            {
                "kind": kind,
                "name": name,
                "attributes": attributes,
                "test_only": item_is_test_only(attributes),
                "signature": signature,
                "tokens": tokens[cursor:item_end],
            }
        )
        cursor = item_end
    return items


def resolve_protected_aliases(
    tokens: list[str], protected: set[str], label: str
) -> None:
    aliases: dict[str, set[str]] = {}
    for index, token in enumerate(tokens):
        if token != "type":
            continue
        statement = bounded_statement(tokens, index)
        if len(statement) < 4 or "=" not in statement:
            continue
        alias_name = statement[1]
        aliases[alias_name] = set(statement[statement.index("=") + 1 :])

    resolved = set(protected)
    changed = True
    while changed:
        changed = False
        for alias_name, targets in aliases.items():
            if alias_name not in resolved and resolved.intersection(targets):
                resolved.add(alias_name)
                changed = True
    opened = sorted(set(aliases).intersection(resolved))
    if opened:
        fail(f"{label} protected-type alias opened: {opened[0]}")


def production_signature_contract(
    items: list[dict[str, object]], label: str
) -> dict[str, str]:
    contract: dict[str, str] = {}
    for item in items:
        if item["test_only"]:
            continue
        identity = f"{item['kind']}:{item['name']}"
        if identity in contract:
            fail(f"{label} duplicate production item identity: {identity}")
        contract[identity] = canonical_sha256(item["signature"])
    return contract


def validate_production_macro_surface(
    items: list[dict[str, object]], label: str, allowed_invocations: set[str]
) -> None:
    for item in items:
        if item["test_only"]:
            continue
        tokens = item["tokens"]
        if item["kind"] in {"macro_rules", "macro_invocation"}:
            fail(f"{label} production item macro surface opened")
        for index in range(len(tokens) - 2):
            if tokens[index] == "macro_rules" and tokens[index + 1] == "!":
                fail(f"{label} production macro definition opened")
            if (
                tokens[index + 1] == "!"
                and tokens[index + 2] in {"(", "[", "{"}
                and tokens[index] not in allowed_invocations
            ):
                fail(
                    f"{label} unlisted production macro invocation opened: "
                    f"{tokens[index]}"
                )


def validate_settlement_constructor_owners(
    items: list[dict[str, object]]
) -> None:
    actual: dict[str, list[str]] = {seal: [] for seal in SETTLEMENT_SEALS}
    constructor_suffix = ["(", "(", ")", ")"]
    for item in items:
        if item["test_only"]:
            continue
        identity = f"{item['kind']}:{item['name']}"
        tokens = item["tokens"]
        for seal in SETTLEMENT_SEALS:
            for index, token in enumerate(tokens):
                if (
                    token == seal
                    and tokens[index + 1 : index + 5] == constructor_suffix
                    and (index == 0 or tokens[index - 1] != "struct")
                ):
                    actual[seal].append(identity)
        if (
            any(token in {"const", "static"} for token in tokens)
            and SETTLEMENT_SEALS.intersection(tokens)
        ):
            fail(f"Stage 5E const/static settlement capability opened: {identity}")
        if (
            item["kind"] in {"struct", "enum"}
            and item["name"] not in SETTLEMENT_SEALS
            and SETTLEMENT_SEALS.intersection(tokens)
        ):
            fail(f"Stage 5E settlement capability storage opened: {identity}")

    expected_owner = ["fn:validate_and_settle_stage5e_paper_callback_escrow"]
    require_exact(
        actual,
        {seal: expected_owner for seal in SETTLEMENT_SEALS},
        "Stage 5E exact settlement constructor owner vector drift",
    )


def validate_protected_production_items(
    stage5c_region: str, stage5e_region: str
) -> None:
    stage5c_items = top_level_items(rust_tokens(stage5c_region))
    stage5e_region_tokens = rust_tokens(stage5e_region)
    callback_tokens = balanced_item_body(
        stage5e_region_tokens,
        ["pub", "(", "crate", ")", "mod", "callback_settlement"],
    )
    stage5e_items = top_level_items(callback_tokens)
    require_exact(
        production_signature_contract(stage5c_items, "Stage 5C B3F region"),
        EXPECTED_STAGE5C_REGION_PRODUCTION_SIGNATURES,
        "Stage 5C B3F exact production item/signature vector drift",
    )
    require_exact(
        production_signature_contract(stage5e_items, "Stage 5E callback_settlement"),
        EXPECTED_STAGE5E_CALLBACK_PRODUCTION_SIGNATURES,
        "Stage 5E callback_settlement exact production item/signature vector drift",
    )
    validate_production_macro_surface(
        stage5c_items, "Stage 5C B3F region", {"debug_assert_eq"}
    )
    validate_production_macro_surface(
        stage5e_items, "Stage 5E callback_settlement", set()
    )
    validate_settlement_constructor_owners(stage5e_items)


def validate_protected_use_aliases(tokens: list[str], label: str) -> None:
    protected = SETTLEMENT_SEALS | {
        "Stage5eAcceptedBarSettlementMetadata",
        "Stage5ePaperSettlementPayload",
    }
    for index, token in enumerate(tokens):
        if token != "use":
            continue
        statement = bounded_statement(tokens, index)
        if "as" in statement and protected.intersection(statement):
            fail(f"{label} protected use alias opened")


def validate_protected_macro_surfaces(tokens: list[str], label: str) -> None:
    sensitive = SETTLEMENT_SEALS | {
        "Stage5eAcceptedBarSettlementMetadata",
        "Stage5ePaperSettlementPayload",
        "accepted_bar_close_ts",
    }
    pairs = {"(": ")", "[": "]", "{": "}"}
    for index in range(len(tokens) - 2):
        if tokens[index + 1] != "!":
            continue
        if tokens[index] == "macro_rules":
            if index + 3 >= len(tokens) or tokens[index + 3] not in pairs:
                fail(f"{label} malformed macro definition")
            opening_index = index + 3
        elif tokens[index + 2] in pairs:
            opening_index = index + 2
        else:
            continue
        opening = tokens[opening_index]
        closing = pairs[opening]
        depth = 1
        cursor = opening_index + 1
        while cursor < len(tokens) and depth:
            depth += tokens[cursor] == opening
            depth -= tokens[cursor] == closing
            cursor += 1
        body = tokens[opening_index + 1 : cursor - 1]
        if sensitive.intersection(body):
            fail(f"{label} protected macro expansion surface opened")


def validate_seal_declarations(tokens: list[str], label: str) -> None:
    expected = ["pub", "(", "crate", ")", "struct"]
    suffix = ["(", "(", ")", ")", ";"]
    for seal in SETTLEMENT_SEALS:
        starts = [
            index
            for index in range(len(tokens) - len(expected) - len(suffix) - 1)
            if tokens[index : index + len(expected)] == expected
            and tokens[index + len(expected)] == seal
            and tokens[
                index + len(expected) + 1 :
                index + len(expected) + 1 + len(suffix)
            ]
            == suffix
        ]
        require_exact(
            len(starts),
            1 if label == "Stage 5E" else 0,
            f"{label} exact settlement seal declaration drift: {seal}",
        )
        if starts and starts[0] > 0 and tokens[starts[0] - 1] == "]":
            fail(f"{label} settlement seal attribute surface opened: {seal}")


def validate_tokenized_production_structure(stage5c: str, stage5e: str) -> None:
    stage5c_tokens = rust_tokens(stage5c)
    stage5e_tokens = rust_tokens(stage5e)
    protected_alias_targets = SETTLEMENT_SEALS | {
        "Stage5eAcceptedBarSettlementMetadata",
        "Stage5ePaperSettlementPayload",
    }
    for label, tokens in (("Stage 5C", stage5c_tokens), ("Stage 5E", stage5e_tokens)):
        validate_protected_use_aliases(tokens, label)
        validate_protected_macro_surfaces(tokens, label)
        validate_seal_declarations(tokens, label)
        resolve_protected_aliases(tokens, protected_alias_targets, label)

    seal_impls = impl_headers(stage5c_tokens + stage5e_tokens, SETTLEMENT_SEALS)
    require_exact(seal_impls, [], "settlement seal impl surface opened")
    payload_impls = impl_headers(stage5e_tokens, {"Stage5ePaperSettlementPayload"})
    require_exact(
        payload_impls,
        [["impl", "Stage5ePaperSettlementPayload"]],
        "settlement payload impl surface drift",
    )
    metadata_impls = impl_headers(
        stage5c_tokens, {"Stage5eAcceptedBarSettlementMetadata"}
    )
    require_exact(
        metadata_impls,
        [["impl", "Stage5eAcceptedBarSettlementMetadata"]],
        "retained metadata impl surface drift",
    )

    payload_body = balanced_item_body(
        stage5e_tokens,
        ["struct", "Stage5ePaperSettlementPayload"],
    )
    expected_payload_body = [
        "mutated_strategy", ":", "crate", "::", "hybrid_intraday_runtime", "::",
        "HybridIntradayRuntimeStrategy", ",",
        "recovery_receipt", ":", "crate", "::", "stage5c_paper_host", "::",
        "Stage5cPendingRecoveryReceipt", ",",
        "audit_lineage", ":", "Stage5eAuthorizedCallbackAuditLineage", ",",
        "pre_callback_attribution_snapshot", ":", "crate", "::",
        "stage5c_paper_host", "::", "Stage5ePreCallbackAttributionSnapshot", ",",
        "retained_bar_metadata", ":", "crate", "::", "stage5c_paper_host", "::",
        "Stage5eAcceptedBarSettlementMetadata", ",",
        "callback_invoked_at", ":", "DateTime", "<", "Utc", ">", ",",
        "callback_authority_id", ":", "[", "u8", ";", "32", "]", ",",
        "callback_outcome", ":", "Stage5ePaperCallbackOutcome", ",",
    ]
    require_exact(
        payload_body,
        expected_payload_body,
        "settlement payload exact field-type vector drift",
    )

    actual_fingerprints = {
        **{
            f"stage5c:{seal}": token_windows_sha256(stage5c_tokens, seal)
            for seal in SETTLEMENT_SEALS
        },
        **{
            f"stage5e:{seal}": token_windows_sha256(stage5e_tokens, seal)
            for seal in SETTLEMENT_SEALS
        },
        "stage5c:Stage5eAcceptedBarSettlementMetadata": token_windows_sha256(
            stage5c_tokens, "Stage5eAcceptedBarSettlementMetadata"
        ),
        "stage5c:accepted_bar_close_ts": token_windows_sha256(
            stage5c_tokens, "accepted_bar_close_ts"
        ),
        "stage5e:Stage5eAcceptedBarSettlementMetadata": token_windows_sha256(
            stage5e_tokens, "Stage5eAcceptedBarSettlementMetadata"
        ),
        "stage5e:Stage5ePaperSettlementPayload": token_windows_sha256(
            stage5e_tokens, "Stage5ePaperSettlementPayload"
        ),
    }
    require_exact(
        actual_fingerprints,
        EXPECTED_SENSITIVE_TOKEN_WINDOWS_SHA256,
        "B3F sensitive token occurrence allowlist drift",
    )


def validate_production_ui_harness() -> None:
    source = (ROOT / "scripts/stage5e_b3f_production_ui_harness.py").read_text()
    expected_cases = {
        "actual_consume_seal_clone": "E0599",
        "actual_consume_seal_copy": "E0382",
        "actual_sibling_seal_reconstruction": "E0603",
        "actual_payload_capability_escape": "E0609",
        "actual_escrow_second_consume": "E0382",
        "actual_preflight_borrow_across_escrow_move": "E0505",
        "actual_consume_seal_from_unit": "E0277",
        "actual_consume_seal_default": "E0277",
    }
    observed_cases = dict(
        re.findall(
            r'Case\(\s*"([^"]+)",.*?\n\s*"(E[0-9]{4})",\s*\)',
            source,
            re.S,
        )
    )
    require_exact(
        observed_cases,
        expected_cases,
        "production UI case matrix drift",
    )
    for fragment in (
        'message.get("level") != "error"',
        'span.get("is_primary") is True',
        'Path(span.get("file_name", "")).name == source_path.name',
        "first_line <= int(span.get(\"line_start\", -1)) <= last_line",
        "codes != {case.expected_code}",
        "or unexpected",
        "primary_span=bound",
    ):
        require_source_fragment(
            source,
            fragment,
            f"production UI diagnostic provenance drift: {fragment}",
        )


def git_changed_paths() -> list[str]:
    # Negative-harness archive copies intentionally contain no .git metadata;
    # source/hash checks remain authoritative in that environment.
    if not (ROOT / ".git").exists():
        return EXPECTED_ALLOWED_CHANGED_PATHS
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASELINE_REF, "--"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return sorted(line for line in tracked.stdout.splitlines() if line)


def accepted_git_blob_sha256(relative: str) -> str:
    completed = subprocess.run(
        ["git", "show", f"{BASELINE_REF}:{relative}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    )
    return hashlib.sha256(completed.stdout).hexdigest()


def marked_region(source: str, label: str, marker_id: str) -> str:
    begin = f"// {label}-BEGIN: {marker_id}"
    end = f"// {label}-END: {marker_id}"
    require_exact(source.count(begin), 1, f"{label} begin marker drift")
    require_exact(source.count(end), 1, f"{label} end marker drift")
    start = source.index(begin) + len(begin)
    finish = source.index(end, start)
    region = source[start:finish]
    if not region.strip():
        fail(f"{label} implementation region is empty")
    return region


def validate_implementation_source() -> None:
    lib = (ROOT / "crates/strategy-runtime-core/src/lib.rs").read_text()
    stage5c = (
        ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    ).read_text()
    stage5e = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    validate_tokenized_production_structure(stage5c, stage5e)
    stage5c_region = marked_region(
        stage5c,
        "STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION",
        "stage5c-private-bridge-v1",
    )
    stage5e_region = marked_region(
        stage5e,
        "STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION",
        "private-process-local-v1",
    )
    validate_protected_production_items(stage5c_region, stage5e_region)
    combined = stage5c_region + stage5e_region
    for forbidden in (
        "redis::",
        "reqwest",
        "finam",
        ".send(",
        "tokio::spawn",
        "std::fs",
        "runtime_live",
        "broker_execution",
    ):
        if forbidden.lower() in combined.lower():
            fail(f"forbidden implementation surface opened: {forbidden}")
    for symbol in (
        "validate_and_settle_stage5e_paper_callback_escrow",
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "construct_stage5e_b3f_stage5c_expected_preflight_binding",
        "settle_stage5e_callback_escrow_material",
        "settle_stage5c_semantic_result_owning_core",
        "map_stage5c_preflight_mismatch_exact",
        "map_stage5c_settlement_error_exact",
        "construct_stage5e_b3f_settlement_identity",
    ):
        if symbol not in combined:
            fail(f"required B3F implementation symbol missing: {symbol}")
    mismatch_match = re.search(
        r"pub\(crate\) enum Stage5eStage5cPreflightMismatch\s*\{(?P<body>.*?)\n\}",
        stage5c_region,
        re.S,
    )
    if mismatch_match is None:
        fail("Stage 5C mismatch enum missing")
    mismatch_prefix = stage5c_region[: mismatch_match.start()]
    if mismatch_prefix.rstrip().endswith("]"):
        fail("Stage 5C mismatch enum gained an attribute or derive")
    for forbidden_trait in (
        "Clone",
        "Copy",
        "Serialize",
        "Deserialize",
        "Display",
        "From",
        "Into",
    ):
        if re.search(
            rf"impl(?:<[^>]+>)?\s+{forbidden_trait}\s+for\s+"
            r"Stage5eStage5cPreflightMismatch",
            combined,
        ):
            fail(f"Stage 5C mismatch gained forbidden trait: {forbidden_trait}")
    if "drop(exact_intent_vector);" not in stage5c_region:
        fail("early-attribution intent vector is no longer irreversibly disposed")
    stage5c_region_functions = set(
        re.findall(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", stage5c_region)
    )
    require_exact(
        stage5c_region_functions,
        EXPECTED_STAGE5C_B3F_FUNCTIONS,
        "Stage 5C B3F structural function allowlist drift",
    )
    metadata_functions = set(
        re.findall(
            r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*"
            r"\([^{};]*Stage5eAcceptedBarSettlementMetadata[^{};]*\)",
            stage5c,
            re.S,
        )
    )
    require_exact(
        metadata_functions,
        EXPECTED_METADATA_FUNCTIONS,
        "opaque retained-close metadata function allowlist drift",
    )
    metadata_impls = re.findall(
        r"(?m)^[ \t]*(?:impl(?:<[^{}\n]+>)?[^{}\n]*"
        r"Stage5eAcceptedBarSettlementMetadata[^{}\n]*\{)",
        stage5c,
    )
    require_exact(
        metadata_impls,
        ["impl Stage5eAcceptedBarSettlementMetadata {"],
        "opaque retained-close metadata impl allowlist drift",
    )
    require_exact(
        stage5c.count(
            "#[cfg(test)]\nimpl Stage5eAcceptedBarSettlementMetadata {"
        ),
        1,
        "opaque retained-close metadata test-only impl drift",
    )
    if "fn accepted_bar_close_ts(" in stage5c:
        fail("production raw retained-close getter opened")
    if ".accepted_bar_close_ts()" in stage5e:
        fail("settlement chronology regressed to raw scalar extraction")
    seal_definition = "pub(crate) struct Stage5ePaperSettlementConsumeSeal(());"
    require_exact(
        stage5e.count("Stage5ePaperSettlementConsumeSeal(())"),
        2,
        "settlement consume seal constructor count drift",
    )
    seal_offset = stage5e.index(seal_definition)
    if "derive(Clone" in stage5e[max(0, seal_offset - 80) : seal_offset] or re.search(
        r"derive\([^)]*\bCopy\b[^)]*\)\s*"
        r"pub\(crate\) struct Stage5ePaperSettlementConsumeSeal",
        stage5e[max(0, seal_offset - 160) : seal_offset + len(seal_definition)],
    ):
        fail("settlement consume seal gained Clone/Copy")
    if re.search(
        r"impl(?:<[^>]+>)?\s+(?:Clone|Copy)\s+for\s+"
        r"Stage5ePaperSettlementConsumeSeal",
        stage5e,
    ):
        fail("settlement consume seal gained Clone/Copy impl")
    if re.search(r"impl\s+Stage5ePaperSettlementConsumeSeal\s*\{", stage5e):
        fail("settlement consume seal gained a second construction surface")
    payload_match = re.search(
        r"pub\(super\) struct Stage5ePaperSettlementPayload\s*\{"
        r"(?P<body>.*?)\n        \}",
        stage5e_region,
        re.S,
    )
    if payload_match is None:
        fail("settlement payload structural definition missing")
    if "Stage5ePaperSettlementConsumeSeal" in payload_match.group("body"):
        fail("settlement payload stores consume capability")
    payload_impl_start = stage5e_region.index(
        "impl Stage5ePaperSettlementPayload {"
    )
    payload_impl_end = stage5e_region.index(
        "enum Stage5ePaperSettlementPreflightDecision", payload_impl_start
    )
    payload_impl_functions = set(
        re.findall(
            r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
            stage5e_region[payload_impl_start:payload_impl_end],
        )
    )
    require_exact(
        payload_impl_functions,
        {"from_escrow"},
        "settlement payload method allowlist drift",
    )
    for retained_close_fragment in (
        "pub(crate) fn validate_stage5e_b3f_retained_close_chronology(",
        "DateTime::from_timestamp(retained_bar_metadata.accepted_bar_close_ts, 0)",
        "accepted_bar_close > authority_issued_at",
        "accepted_bar_close > callback_invoked_at",
        "Stage5eStage5cRetainedCloseChronologyProof(())",
        "Stage5eStage5cRetainedCloseChronologyMismatch(())",
    ):
        require_source_fragment(
            stage5c_region,
            retained_close_fragment,
            f"Stage 5C retained-close authority missing: {retained_close_fragment}",
        )
    require_exact(
        (stage5c + stage5e).count("validate_stage5e_b3f_retained_close_chronology("),
        2,
        "retained-close chronology authority must have one definition and one call site",
    )
    for chronology_fragment in (
        "audit._b3c_effective_observed_at == audit._effective_observed_at",
        "audit._b3c_effective_expires_at == audit._authority_expires_at",
    ):
        require_source_fragment(
            stage5e_region,
            chronology_fragment,
            f"settlement chronology relation missing: {chronology_fragment}",
        )
    for authority_fragment in (
        "let recomputed = super::callback_authority_id(",
        "recomputed.0 == escrow.callback_authority_id",
        "recomputed.0 == audit._callback_authority_id",
    ):
        require_source_fragment(
            stage5e_region,
            authority_fragment,
            f"canonical callback-authority recomputation missing: {authority_fragment}",
        )
    if combined.count("super::callback_authority_id(") != 1:
        fail("callback-authority encoder must have exactly one B3F call site")
    compile_fail_fixtures = (
        "b3f_compile_fail_consume_seal_clone_or_copy",
        "b3f_compile_fail_consume_seal_reconstruction",
        "b3f_compile_fail_capability_escape",
        "b3f_compile_fail_second_escrow_consume",
        "b3f_compile_fail_borrow_survives_consume",
    )
    for compile_fail_fixture in compile_fail_fixtures:
        if lib.count(compile_fail_fixture) != 1:
            fail(f"required B3F compile-fail fixture missing: {compile_fail_fixture}")
    for diagnostic_fence in (
        "```compile_fail,E0599\n//! // b3f_compile_fail_consume_seal_clone_or_copy",
        "```compile_fail,E0423\n//! // b3f_compile_fail_consume_seal_reconstruction",
        "```compile_fail,E0599\n//! // b3f_compile_fail_capability_escape",
        "```compile_fail,E0382\n//! // b3f_compile_fail_second_escrow_consume",
        "```compile_fail,E0505\n//! // b3f_compile_fail_borrow_survives_consume",
    ):
        require_source_fragment(
            lib,
            diagnostic_fence,
            f"compile-fail diagnostic class drift: {diagnostic_fence}",
        )
    if "compile_error!" in lib:
        fail("B3F compile-fail evidence may not use unconditional compile_error")
    for facade_fragment in (
        "#[cfg(doctest)]\n#[doc(hidden)]\npub mod stage5e_b3f_compile_fail_facade",
        "type ProductionEscrow = callback_authority::Stage5ePaperCallbackResultEscrow;",
        "Stage5ePaperSettlementPreflightSeal;",
        "Stage5ePaperSettlementConsumeSeal;",
        "b3f_doctest_borrow_preflight(&self.0, &seal.0);",
        "b3f_doctest_consume_escrow(self.0, &seal.0);",
        "let _first = escrow.consume(&seal);",
        "let _second = escrow.consume(&seal);",
        "let borrowed = escrow.preflight(&preflight_seal);",
        "let _payload = escrow.consume(&consume_seal);",
    ):
        require_source_fragment(
            lib,
            facade_fragment,
            f"production-backed compile-fail facade drift: {facade_fragment}",
        )
    for helper in (
        "b3f_doctest_borrow_preflight",
        "b3f_doctest_consume_escrow",
    ):
        if not re.search(rf"#\[cfg\(doctest\)\]\s+pub\(crate\) fn {helper}", stage5e_region):
            fail(f"compile-fail production delegate is not doctest-only: {helper}")
    for test_name in (
        "b3f_owning_core_matches_legacy_public_zero_intent_settlement",
        "b3f_canonical_zero_intent_escrow_settles_once_with_one_entry_history",
        "b3f_source_produced_intent_preserves_ordered_request_ids_and_exact_count",
        "b3f_callback_validation_error_consumes_escrow_into_terminal_receipt",
        "b3f_intent_capacity_boundary_is_exact_at_255_and_256",
        "b3f_preflight_mismatch_mapper_is_exact_for_all_nine_variants",
        "b3f_stage5c_error_mapper_is_exact_for_all_twelve_variants",
        "b3f_event_key_validator_rejects_every_frozen_source_drift",
        "b3f_settlement_identity_preserves_request_order_and_chronology",
        "b3f_stage5c_preflight_validator_produces_all_nine_exact_mismatches",
        "b3f_callback_before_retained_close_is_terminal_chronology_mismatch",
        "b3f_retained_close_after_authority_issue_is_terminal_chronology_mismatch",
        "b3f_b3c_outer_chronology_drift_is_terminal_chronology_mismatch",
        "b3f_same_wrong_stored_authority_ids_fail_canonical_recomputation",
        "b3f_canonical_authority_input_drift_without_new_id_is_identity_mismatch",
        "b3f_early_attribution_error_disposes_exact_intent_vector",
        "b3f_owning_core_matches_legacy_public_nonempty_settlement",
        "b3f_owning_core_matches_legacy_public_representative_error",
    ):
        if test_name not in stage5c + stage5e:
            fail(f"required B3F acceptance test missing: {test_name}")


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid inventory: {exc}")

    require_exact(
        canonical_sha256(inventory),
        EXPECTED_INVENTORY_SHA256,
        "design inventory drift",
    )
    require_exact(sha256(PLAN), EXPECTED_PLAN_SHA256, "design plan drift")
    require_exact(
        json.loads(ACTIVE.read_text()),
        {"schema_version": 1, "stage": STAGE},
        "active descriptor drift",
    )
    require_exact(inventory.get("schema_version"), 1, "schema drift")
    require_exact(inventory.get("stage"), STAGE, "stage identity drift")
    require_exact(
        inventory.get("status"),
        "private_process_local_implementation_pending_review",
        "implementation status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "baseline drift")
    require_exact(
        inventory.get("expected_provenance_case_count"),
        555,
        "provenance case count drift",
    )
    ui_contract = inventory.get("production_ui_harness_contract")
    require_exact(
        ui_contract,
        {
            "path": "scripts/stage5e_b3f_production_ui_harness.py",
            "sha256": sha256(
                ROOT / "scripts/stage5e_b3f_production_ui_harness.py"
            ),
            "normal_runtime_surface": False,
            "baseline_must_compile": True,
            "temporary_source_tree": True,
            "unconditional_compile_error_allowed": False,
            "cases": {
                "actual_consume_seal_clone": "E0599",
                "actual_consume_seal_copy": "E0382",
                "actual_sibling_seal_reconstruction": "E0603",
                "actual_payload_capability_escape": "E0609",
                "actual_escrow_second_consume": "E0382",
                "actual_preflight_borrow_across_escrow_move": "E0505",
                "actual_consume_seal_from_unit": "E0277",
                "actual_consume_seal_default": "E0277",
            },
        },
        "production UI harness contract drift",
    )
    require_exact(
        inventory.get("opaque_metadata_structural_allowlist_contract"),
        {
            "stage5c_b3f_functions": sorted(EXPECTED_STAGE5C_B3F_FUNCTIONS),
            "metadata_functions": sorted(EXPECTED_METADATA_FUNCTIONS),
            "production_inherent_impl_count": 0,
            "production_trait_impl_count": 0,
            "test_only_inherent_impl_count": 1,
            "raw_scalar_getters_allowed": False,
            "free_scalar_or_tuple_bridges_allowed": False,
            "payload_capability_storage_or_access_allowed": False,
        },
        "opaque metadata structural allowlist contract drift",
    )
    require_exact(
        inventory.get("tokenized_structural_enforcement_contract"),
        {
            "comments_and_literals_excluded": True,
            "balanced_transitive_type_alias_resolution_required": True,
            "protected_type_aliases_allowed": False,
            "protected_use_aliases_allowed": False,
            "protected_macro_definitions_or_invocations_allowed": False,
            "settlement_seal_impl_count": 0,
            "exact_attribute_vector_all_settlement_seals": True,
            "stage5e_callback_production_item_signature_count": 30,
            "stage5c_region_production_item_signature_count": 21,
            "exact_constructor_owner_vector_all_settlement_seals": True,
            "production_macro_definitions_allowed": False,
            "unlisted_item_producing_macro_invocations_allowed": False,
            "payload_impl_headers": ["impl Stage5ePaperSettlementPayload"],
            "metadata_impl_headers": [
                "cfg_test impl Stage5eAcceptedBarSettlementMetadata"
            ],
            "payload_field_type_vector_exact": True,
            "sensitive_occurrence_windows_pinned": True,
            "full_rebind_sensitive_fingerprints_required": True,
            "macro_body_sensitive_tokens_in_scope": True,
            "ui_primary_span_binding_required": True,
            "ui_unexpected_primary_errors_allowed": False,
        },
        "tokenized structural enforcement contract drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "allowed changed paths drift",
    )
    validate_production_ui_harness()
    require_exact(
        git_changed_paths(),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "design changed-path set drift",
    )

    if (ROOT / ".git").exists():
        for relative, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
            require_exact(
                accepted_git_blob_sha256(relative),
                expected,
                f"accepted B3E predecessor source drift: {relative}",
            )
    require_exact(
        inventory.get("protected_b3e_source_sha256"),
        EXPECTED_PROTECTED_SOURCE_SHA256,
        "protected B3E source inventory drift",
    )
    for relative, expected in EXPECTED_IMPLEMENTATION_SOURCE_SHA256.items():
        require_exact(
            sha256(ROOT / relative),
            expected,
            f"protected B3F implementation source changed: {relative}",
        )
    require_exact(
        inventory.get("implementation_source_sha256"),
        EXPECTED_IMPLEMENTATION_SOURCE_SHA256,
        "protected B3F implementation source inventory drift",
    )
    validate_implementation_source()
    require_exact(
        inventory["module_visibility_contract"],
        {
            "callback_settlement_module": "pub_crate_child_module_named_opaque_surface_only",
            "schedule_window_evidence_module": "pub_crate_existing_module",
            "public_outside_crate_surface_allowed": False,
            "unlisted_cross_module_reexports_allowed": False,
        },
        "cross-module path visibility drift",
    )

    transition = inventory["transition_contract"]
    require_exact(
        transition["only_input"],
        "Stage5ePaperCallbackResultEscrow",
        "settlement sole-input drift",
    )
    require_exact(
        transition["implementation_status"],
        "implemented_private_process_local_pending_review",
        "settlement implementation status drift",
    )
    require_exact(
        transition["borrowed_preflight_before_consume"],
        True,
        "borrowed preflight ordering drift",
    )
    require_exact(transition["consume_count"], 1, "escrow consume-count drift")
    require_exact(
        transition["preflight_decisions"],
        ["ProceedOk", "Terminal"],
        "preflight decision taxonomy drift",
    )
    require_exact(
        transition["consume_after_every_decision"],
        True,
        "terminal preflight ownership conflict reintroduced",
    )
    require_exact(
        transition["settlement_implementation_allowed_in_this_stage"],
        True,
        "accepted private settlement implementation was closed",
    )
    escrow_bridge = inventory["escrow_bridge_contract"]
    require_exact(
        escrow_bridge["consume_signature"],
        "self,&Stage5ePaperSettlementConsumeSeal->Stage5ePaperSettlementPayload",
        "consume capability must remain borrowable after escrow consume",
    )
    require_exact(
        escrow_bridge["consume_capability_stored_in_payload"],
        False,
        "consume capability escaped into payload",
    )
    require_exact(
        escrow_bridge["consume_capability_stored_in_receipt"],
        False,
        "consume capability escaped into receipt",
    )
    liveness = inventory["consume_capability_liveness_contract"]
    require_exact(liveness["issuance_count"], 1, "consume capability issuance drift")
    require_exact(liveness["storage"], "stack_local_only", "consume capability storage drift")
    require_exact(liveness["consume_borrow_mode"], "shared_borrow", "consume borrow mode drift")
    require_exact(
        liveness["ordered_uses"],
        [
            "borrow_for_sole_escrow_consume",
            "borrow_for_material_seal_after_ProceedOk_classification",
            "borrow_for_settlement_seal_immediately_before_stage5c_bridge",
            "drop_before_success_or_terminal_return",
        ],
        "consume capability liveness order drift",
    )
    require_exact(liveness["material_seal_issuance_count"], 1, "material seal issuance drift")
    require_exact(liveness["settlement_seal_issuance_count"], 1, "settlement seal issuance drift")
    for key in (
        "issuance_before_ProceedOk_classification_allowed",
        "settlement_seal_early_issuance_allowed",
        "capability_return_allowed",
        "capability_reconstruction_allowed",
        "capability_conversion_allowed",
        "second_escrow_consume_possible",
    ):
        require_exact(liveness[key], False, f"consume capability invariant opened: {key}")
    require_exact(
        liveness["compile_fail_cases"],
        [
            "second_consume_after_self_move",
            "consume_capability_clone_or_reconstruction",
            "consume_capability_escape_in_payload_or_receipt",
        ],
        "consume capability compile-fail contract drift",
    )
    require_exact(
        inventory["seal_contract"]["cross_module_visibility"],
        {
            "Stage5ePaperSettlementPreflightSeal": "pub_crate_opaque_private_fields",
            "Stage5ePaperSettlementConsumeSeal": "pub_crate_opaque_private_fields",
        },
        "cross-module seal visibility drift",
    )
    require_exact(
        inventory["retained_close_chronology_authority_contract"],
        {
            "function": "validate_stage5e_b3f_retained_close_chronology",
            "owner": "strategy_runtime_core::stage5c_paper_host",
            "definition_count": 1,
            "call_site_count": 1,
            "ownership": "immutable_borrows_only",
            "inputs": [
                "&Stage5eAcceptedBarSettlementMetadata",
                "authority_issued_at",
                "callback_invoked_at",
                "&Stage5ePaperSettlementPreflightSeal",
            ],
            "success": "Stage5eStage5cRetainedCloseChronologyProof",
            "failure": "Stage5eStage5cRetainedCloseChronologyMismatch",
            "success_and_failure": "pub_crate_opaque_payload_free",
            "raw_timestamp_return_allowed": False,
            "production_raw_getters_allowed": False,
            "proof_reusable_authority": False,
            "mismatch_maps_to": "ChronologyMismatch",
            "borrows_end_before_escrow_consume": True,
        },
        "retained-close chronology authority contract drift",
    )
    require_exact(
        inventory["compile_fail_evidence_contract"],
        {
            "configuration": "cfg_doctest_only",
            "normal_runtime_public_surface_added": False,
            "facade_wraps_actual_production_escrow_and_seals": True,
            "facade_operations_delegate_to_actual_production_borrow_and_consume": True,
            "unconditional_compile_error_allowed": False,
            "expected_diagnostic_codes": {
                "consume_seal_clone_or_copy": "E0599",
                "consume_seal_reconstruction": "E0423",
                "capability_escape": "E0599",
                "second_escrow_consume": "E0382",
                "preflight_borrow_survives_escrow_consume": "E0505",
            },
            "cases": [
                "consume_seal_clone_or_copy",
                "consume_seal_reconstruction",
                "capability_escape",
                "second_escrow_consume",
                "preflight_borrow_survives_escrow_consume",
            ],
        },
        "production-backed compile-fail evidence contract drift",
    )

    preflight = inventory["preflight_contract"]
    require_exact(
        preflight["ownership"],
        "borrowed_non_decomposable",
        "preflight ownership drift",
    )
    require_exact(
        preflight["raw_intent_export_allowed"],
        False,
        "raw intent export opened",
    )
    require_exact(
        preflight["terminal_decision_still_consumes_escrow"],
        True,
        "terminal preflight consume drift",
    )
    require_exact(
        set(preflight["checks"]),
        {
            "callback_outcome_discriminant",
            "intent_count_lte_u8_max",
            "accepted_bar_origin_live",
            "execution_eligible_true",
            "paper_mode_and_live_orders_disabled",
            "strategy_id_exact_equality",
            "account_id_exact_equality",
            "full_instrument_id_exact_equality",
            "semantic_bar_identity_exact_equality",
            "bar_close_ts_exact_equality",
            "callback_chronology",
            "authority_and_fingerprint_nonzero_equality",
            "no_prior_intent_extraction",
        },
        "preflight check vector drift",
    )
    stage5c_preflight = inventory["stage5c_preflight_bridge_contract"]
    require_exact(
        stage5c_preflight["function"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "Stage 5C preflight bridge drift",
    )
    require_exact(stage5c_preflight["definition_count"], 1, "preflight bridge definition drift")
    require_exact(stage5c_preflight["call_site_count"], 1, "preflight bridge call-site drift")
    require_exact(
        stage5c_preflight["ownership"],
        "immutable_borrows_only",
        "preflight bridge ownership drift",
    )
    require_exact(
        stage5c_preflight["inputs"],
        [
            "&Stage5cPendingRecoveryReceipt",
            "&Stage5ePreCallbackAttributionSnapshot",
            "&Stage5eAcceptedBarSettlementMetadata",
            "&Stage5eB3fStage5cExpectedPreflightBinding<'_>",
            "&Stage5ePaperSettlementPreflightSeal",
        ],
        "Stage 5C preflight input topology drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor"],
        "construct_stage5e_b3f_stage5c_expected_preflight_binding",
        "expected-binding constructor drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "expected-binding reverse-sibling privacy drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "expected-binding constructor owner drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_signature"],
        "eight_lifetime_bound_immutable_audit_field_borrows,&Stage5ePaperSettlementPreflightSeal->Stage5eB3fStage5cExpectedPreflightBinding<'a>",
        "expected-binding constructor signature drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_visibility"],
        "pub_crate_opaque_stage5c_private_fields",
        "expected-binding field visibility drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_lifetime_bound"],
        True,
        "expected-binding lifetime drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_input_mode"],
        "immutable_borrow_field_for_field_from_stage5e_call_site",
        "expected-binding constructor direction drift",
    )
    for key in (
        "intent_extraction_allowed",
        "ownership_mutation_allowed",
        "production_raw_getters_allowed",
        "production_tuple_exports_allowed",
        "test_helper_reuse_allowed",
    ):
        require_exact(stage5c_preflight[key], False, f"preflight privacy opened: {key}")

    relation_matrix = inventory["identity_contract"]["field_source_relation_matrix"]
    require_exact(
        relation_matrix,
        [
            {
                "relation": "strategy_id",
                "sources": ["recovery_admission", "pre_callback_attribution_snapshot"],
                "check": "exact_equality",
            },
            {
                "relation": "account_id",
                "sources": ["recovery_admission", "pre_callback_attribution_snapshot"],
                "check": "exact_equality",
            },
            {
                "relation": "full_instrument_id",
                "sources": [
                    "recovery_admission",
                    "pre_callback_attribution_snapshot",
                    "audit_full_instrument_id",
                    "audit_owned_instrument",
                ],
                "check": "exact_full_InstrumentId_equality",
            },
            {
                "relation": "accepted_semantic_bar_identity",
                "sources": [
                    "pre_callback_attribution_snapshot",
                    "retained_bar_metadata",
                    "audit_accepted_semantic_bar_identity",
                    "audit_owned_bar_identity",
                ],
                "check": "exact_32_byte_equality",
            },
            {
                "relation": "accepted_bar_close_ts",
                "sources": ["pre_callback_attribution_snapshot", "retained_bar_metadata"],
                "check": "exact_i64_equality",
            },
            {
                "relation": "bar_close_to_audit_event_key",
                "sources": [
                    "retained_bar_metadata.accepted_bar_close_ts",
                    "audit_schedule_identity_fingerprint",
                    "audit_full_instrument_id",
                    "audit_sequence_identity_fingerprint",
                    "audit_event_key_fingerprint",
                    "audit_b3b_event_key_fingerprint",
                ],
                "check": "recompute_b3b_event_key_fingerprint_and_exact_compare_to_both_audit_fields",
            },
            {
                "relation": "callback_chronology",
                "sources": [
                    "callback_invoked_at",
                    "retained_bar_metadata.accepted_bar_close_ts",
                    "accepted_b3e_audit_chronology",
                ],
                "check": "callback_not_before_bar_close_and_accepted_b3e_ordering",
            },
            {
                "relation": "callback_authority_and_fingerprints",
                "sources": [
                    "payload_callback_authority_id",
                    "audit_callback_authority_id",
                    "audit_schedule_identity_fingerprint",
                    "audit_sequence_identity_fingerprint",
                    "audit_event_key_fingerprint",
                    "audit_b3b_event_key_fingerprint",
                    "audit_continuation_binding_id",
                    "audit_b3c_continuation_binding_id",
                ],
                "check": "exact_equality_nonzero_and_internal_recomputation",
            },
            {
                "relation": "paper_live_closure",
                "sources": [
                    "recovery_admission",
                    "retained_bar_metadata",
                    "intent_sink_authority",
                ],
                "check": "paper_only_live_orders_absent_origin_Live_execution_eligible",
            },
        ],
        "field/source relation matrix drift",
    )
    require_exact(
        inventory["identity_contract"]["cartesian_source_claim_allowed"],
        False,
        "unsatisfiable Cartesian identity claim reopened",
    )
    require_exact(
        stage5c_preflight["expected_binding_fields"],
        [
            "audit_schedule_identity_fingerprint",
            "audit_sequence_identity_fingerprint",
            "audit_event_key_fingerprint",
            "audit_b3b_event_key_fingerprint",
            "audit_full_instrument_id",
            "audit_owned_instrument",
            "audit_accepted_semantic_bar_identity",
            "audit_owned_bar_identity",
        ],
        "expected-binding field/source schema drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_field_source"],
        "field_for_field_from_Stage5eAuthorizedCallbackAuditLineage",
        "expected-binding field source drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_inputs"],
        [
            "audit_schedule_identity_fingerprint",
            "audit_sequence_identity_fingerprint",
            "audit_event_key_fingerprint",
            "audit_b3b_event_key_fingerprint",
            "audit_full_instrument_id",
            "audit_owned_instrument",
            "audit_accepted_semantic_bar_identity",
            "audit_owned_bar_identity",
            "Stage5ePaperSettlementPreflightSeal",
        ],
        "expected-binding constructor input vector drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_fields_inspected_only_by_stage5c_owner"],
        True,
        "expected-binding private field inspector drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_borrows_end_before_escrow_consume"],
        True,
        "expected-binding borrow lifetime escaped consume boundary",
    )

    event_key = inventory["b3b_event_key_validation_authority_contract"]
    require_exact(
        event_key["function"],
        "validate_stage5e_b3f_b3b_event_key_binding",
        "B3B validation authority drift",
    )
    require_exact(
        event_key["owner"],
        "strategy_runtime_core::stage5e_no_io_lifecycle::schedule_window_evidence",
        "B3B validation authority owner drift",
    )
    require_exact(event_key["definition_count"], 1, "B3B authority definition drift")
    require_exact(event_key["call_site_count"], 1, "B3B authority call-site drift")
    require_exact(
        event_key["sole_caller"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "B3B authority caller drift",
    )
    require_exact(
        event_key["inputs"],
        [
            "&audit_schedule_identity_fingerprint",
            "&audit_full_instrument_id",
            "retained_bar_close_i64",
            "&audit_sequence_identity_fingerprint",
            "&audit_event_key_fingerprint",
            "&audit_b3b_event_key_fingerprint",
            "&Stage5ePaperSettlementPreflightSeal",
        ],
        "B3B authority exact input vector drift",
    )
    require_exact(
        event_key["visibility"],
        "pub_crate_capability_gated",
        "B3B authority visibility drift",
    )
    require_exact(
        event_key["success"],
        "Stage5eB3fEventKeyValidatedProof",
        "B3B authority success type drift",
    )
    require_exact(
        event_key["failure"],
        "Stage5eB3fEventKeyMismatch",
        "B3B authority failure type drift",
    )
    require_exact(
        event_key["canonical_delegate"],
        "schedule_window_evidence::b3b_event_key_fingerprint",
        "canonical B3B encoder delegation drift",
    )
    require_exact(event_key["canonical_delegate_call_count"], 1, "canonical B3B delegate count drift")
    for key in (
        "second_encoder_allowed",
        "second_domain_string_allowed",
        "raw_audit_lineage_getters_allowed",
        "mutable_inputs_allowed",
        "proof_reusable_authority",
    ):
        require_exact(event_key[key], False, f"B3B authority surface opened: {key}")
    require_exact(event_key["proof_fields"], [], "B3B proof gained reusable material")
    require_exact(
        event_key["success_visibility"],
        "pub_crate_opaque_payload_free",
        "B3B proof visibility drift",
    )
    require_exact(
        event_key["failure_visibility"],
        "pub_crate_opaque_payload_free",
        "B3B mismatch visibility drift",
    )
    require_exact(
        event_key["mismatch_maps_to"],
        "Stage5eStage5cPreflightMismatch::AuditEventKey",
        "B3B mismatch mapping drift",
    )

    mismatch = inventory["stage5c_preflight_mismatch_contract"]
    require_exact(
        mismatch["owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "preflight mismatch owner drift",
    )
    require_exact(
        mismatch["representation"],
        "pub_crate_closed_payload_free_enum",
        "preflight mismatch representation drift",
    )
    require_exact(
        mismatch["variants"],
        [
            "StrategyId",
            "AccountId",
            "FullInstrumentId",
            "SemanticBarIdentity",
            "AcceptedBarClose",
            "AuditEventKey",
            "PaperMode",
            "AcceptedBarOrigin",
            "ExecutionEligibility",
        ],
        "preflight mismatch variant drift",
    )
    require_exact(
        mismatch["forbidden_traits"],
        ["Clone", "Copy", "Serialize", "Deserialize", "Display", "From", "Into"],
        "preflight mismatch forbidden-trait contract drift",
    )
    require_exact(
        mismatch["sole_inspector"],
        "map_stage5c_preflight_mismatch_exact",
        "preflight mismatch sole-inspector drift",
    )
    require_exact(mismatch["mapper_definition_count"], 1, "preflight mismatch mapper definition drift")
    require_exact(mismatch["mapper_call_site_count"], 1, "preflight mismatch mapper call-site drift")
    require_exact(
        mismatch["mapper_signature"],
        "Stage5eStage5cPreflightMismatch,&Stage5ePaperSettlementPreflightSeal->Stage5ePaperSettlementTerminalReason",
        "preflight mismatch mapper signature drift",
    )
    require_exact(mismatch["mapper_implementation"], "exhaustive_9_arm_match", "preflight mismatch mapper drift")
    require_exact(mismatch["wildcard_arm_allowed"], False, "preflight mismatch wildcard opened")
    require_exact(mismatch["generic_conversion_allowed"], False, "preflight mismatch conversion opened")
    require_exact(
        mismatch["mapping"],
        {
            "StrategyId": "IdentityMismatch",
            "AccountId": "IdentityMismatch",
            "FullInstrumentId": "IdentityMismatch",
            "SemanticBarIdentity": "IdentityMismatch",
            "AcceptedBarClose": "IdentityMismatch",
            "AuditEventKey": "IdentityMismatch",
            "PaperMode": "PaperModeMismatch",
            "AcceptedBarOrigin": "PaperModeMismatch",
            "ExecutionEligibility": "PaperModeMismatch",
        },
        "preflight mismatch mapping drift",
    )

    require_exact(
        inventory["capacity_contract"]["maximum_intents"],
        255,
        "Stage 5C intent limit drift",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["canonical_batch_builder"],
        "stage5c_build_paper_intent_batch",
        "canonical Stage 5C builder drift",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["canonical_attribution_builder"],
        "stage5cj_expected_generated_attribution_by_request_from_ledger",
        "canonical Stage 5C attribution builder drift",
    )
    stage5c_bridge = inventory["stage5c_bridge_contract"]
    require_exact(
        stage5c_bridge["seal_issuer"],
        "issue_stage5c_b3f_settlement_seal",
        "Stage 5C settlement seal issuer drift",
    )
    require_exact(stage5c_bridge["seal_issuer_definition_count"], 1, "settlement seal issuer definition drift")
    require_exact(stage5c_bridge["seal_issuer_call_site_count"], 1, "settlement seal issuer call-site drift")
    require_exact(
        stage5c_bridge["canonical_public_entrypoint"],
        "settle_stage5c_semantic_result",
        "canonical public Stage 5C entrypoint drift",
    )
    require_exact(
        stage5c_bridge["canonical_owning_core"],
        "settle_stage5c_semantic_result_owning_core",
        "canonical owning core drift",
    )
    require_exact(stage5c_bridge["canonical_owning_core_definition_count"], 1, "owning core definition drift")
    require_exact(stage5c_bridge["canonical_owning_core_call_site_count"], 2, "owning core call-site drift")
    require_exact(
        stage5c_bridge["existing_public_entrypoint_delegates_to_owning_core"],
        True,
        "legacy canonical entrypoint delegation drift",
    )
    require_exact(
        stage5c_bridge["b3f_bridge_invokes_owning_core_once"],
        True,
        "B3F canonical owning-core call drift",
    )
    require_exact(
        stage5c_bridge["parallel_settlement_algorithm_allowed"],
        False,
        "parallel Stage 5C settlement algorithm opened",
    )
    require_exact(stage5c_bridge["canonical_history_builder"], "stage5ch_batch_summary", "canonical history builder drift")
    require_exact(stage5c_bridge["canonical_initial_history_length"], 1, "canonical initial history length drift")
    require_exact(
        stage5c_bridge["canonical_initial_history_rule"],
        "settled_batch_history_equals_single_stage5ch_batch_summary_of_canonical_batch",
        "canonical settled history rule drift",
    )
    require_exact(
        stage5c_bridge["intent_vector_after_attribution_error"],
        "explicitly_irreversibly_dropped_before_terminal_return",
        "early attribution failure recovered intent vector",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["stage5e_reimplementation_allowed"],
        False,
        "parallel Stage 5E intent oracle opened",
    )
    material = inventory["stage5c_material_construction_contract"]
    require_exact(
        material["seal_issuer"],
        "issue_stage5c_b3f_settlement_material_seal",
        "Stage 5C material seal issuer drift",
    )
    require_exact(
        material["constructor"],
        "construct_stage5e_stage5c_settlement_material",
        "Stage 5C material constructor drift",
    )
    require_exact(material["constructor_definition_count"], 1, "material constructor count drift")
    require_exact(material["constructor_call_site_count"], 1, "material call-site drift")
    require_exact(
        material["fields"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "pre_callback_attribution_snapshot",
            "retained_bar_metadata",
            "exact_intent_vector",
            "derived_original_intent_count",
        ],
        "Stage 5C material field schema drift",
    )
    require_exact(material["caller_supplied_intent_count_allowed"], False, "caller-supplied intent count reopened")
    require_exact(material["derived_intent_count_type"], "usize", "derived intent count type drift")
    require_exact(
        material["derived_intent_count_source"],
        "exact_intent_vector.len_before_move",
        "intent count/vector binding drift",
    )
    success_return = inventory["stage5c_success_return_contract"]
    require_exact(
        success_return["proof_fields"],
        [
            "strategy_id",
            "account_id",
            "full_instrument_id",
            "accepted_bar_close_timestamp",
            "batch_state_fingerprint",
            "ordered_strategy_request_ids",
            "intent_count_u8",
            "settled_batch_history_length",
            "canonical_first_batch_summary",
        ],
        "Stage 5C success proof schema drift",
    )
    require_exact(
        success_return["history_proof_rule"],
        "length_one_and_first_summary_equals_stage5ch_batch_summary_of_canonical_batch",
        "success history proof drift",
    )
    require_exact(
        success_return["proof_borrow_before_settled_move"],
        True,
        "success proof ordering drift",
    )
    require_exact(success_return["settled_move_count"], 1, "settled strategy move drift")
    terminal_return = inventory["stage5c_terminal_return_contract"]
    require_exact(
        terminal_return["fields"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "pre_callback_attribution_snapshot",
            "retained_bar_metadata",
            "exact_stage5c_intent_settlement_error",
            "derived_original_intent_count",
        ],
        "Stage 5C terminal return schema drift",
    )
    require_exact(terminal_return["mapper_call_count"], 1, "terminal mapper call-count drift")
    require_exact(
        escrow_bridge["payload_consumer_count"],
        1,
        "escrow payload consumer-count drift",
    )
    require_exact(
        escrow_bridge["raw_getters_allowed"],
        False,
        "escrow raw getter opened",
    )
    require_exact(
        inventory["stage5c_error_mapping"],
        EXPECTED_STAGE5C_ERROR_MAPPING,
        "Stage 5C error mapping drift",
    )
    require_exact(
        inventory["stage5c_error_mapping_policy"]["mapping_count"],
        12,
        "Stage 5C mapping cardinality drift",
    )
    require_exact(
        inventory["stage5c_error_mapping_policy"]["wildcard_mapping_allowed"],
        False,
        "Stage 5C wildcard mapping opened",
    )
    require_exact(
        inventory["callback_validation_error_policy"],
        {
            "preflight_decision": "Terminal",
            "disposition": "consume_then_terminal_receipt",
            "reason": "CallbackValidationError",
            "empty_success_batch_allowed": False,
            "callback_retry_allowed": False,
            "escrow_retry_allowed": False,
            "mutated_strategy_retained": True,
            "recovery_ownership_retained": True,
            "audit_lineage_retained": True,
        },
        "callback ValidationError policy drift",
    )
    require_exact(
        inventory["settlement_identity_contract"]["ordered_fields"],
        [
            "callback_authority_id",
            "callback_invocation_timestamp",
            "accepted_semantic_bar_identity",
            "strategy_id",
            "account_id",
            "full_instrument_id",
            "accepted_bar_close_timestamp",
            "stage5c_batch_state_fingerprint",
            "ordered_strategy_request_ids",
            "intent_count_u8",
            "audit_commitment",
        ],
        "settlement identity field vector drift",
    )
    require_exact(
        inventory["canonical_encoding_contract"]["hash"],
        "SHA-256",
        "canonical identity hash drift",
    )
    named = inventory["named_authority_functions"]
    require_exact(
        named["stage5c_settlement_seal_issuer"]["name"],
        "issue_stage5c_b3f_settlement_seal",
        "named settlement seal issuer drift",
    )
    require_exact(
        named["stage5c_settlement_seal_issuer"]["ordering"],
        "immediately_before_settle_stage5e_callback_escrow_material",
        "settlement seal issuer ordering drift",
    )
    require_exact(
        named["stage5c_preflight_bridge"]["name"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "named Stage 5C preflight bridge drift",
    )
    require_exact(
        named["stage5c_expected_binding_builder"]["owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "named expected-binding builder owner drift",
    )
    require_exact(
        named["b3b_event_key_validation_authority"]["name"],
        "validate_stage5e_b3f_b3b_event_key_binding",
        "named B3B event-key authority drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["name"],
        "map_stage5c_preflight_mismatch_exact",
        "named preflight mismatch mapper drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["implementation"],
        "exhaustive_9_arm_match",
        "named preflight mismatch mapper implementation drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["wildcard_arm_allowed"],
        False,
        "named preflight mismatch wildcard opened",
    )
    require_exact(
        named["stage5c_error_mapper"]["implementation"],
        "exhaustive_12_arm_match",
        "named error mapper implementation drift",
    )
    require_exact(
        named["stage5c_error_mapper"]["wildcard_arm_allowed"],
        False,
        "named error mapper wildcard opened",
    )
    for authority in named.values():
        require_exact(authority["definition_count"], 1, "named authority definition-count drift")
        require_exact(authority["call_site_count"], 1, "named authority call-site drift")
    terminal_matrix = inventory["terminal_ownership_matrix"]
    required_common = {
        "mutated_strategy",
        "recovery_receipt",
        "audit_lineage",
        "pre_callback_attribution_snapshot",
        "retained_bar_metadata",
        "callback_invoked_at",
        "callback_authority_id",
        "audit_commitment",
    }
    for path_name in (
        "preflight_ok_terminal",
        "callback_validation_error_terminal",
        "stage5c_error_terminal",
    ):
        path = terminal_matrix[path_name]
        if not required_common.issubset(set(path["fields"])):
            fail(f"{path_name} retained ownership drift")
        require_exact(path["retryable"], False, f"{path_name} retry opened")
    if "opaque_exact_ok_callback_outcome_with_intent_vector" not in terminal_matrix[
        "preflight_ok_terminal"
    ]["fields"]:
        fail("preflight Ok callback outcome dropped")
    if "opaque_exact_callback_validation_error" not in terminal_matrix[
        "callback_validation_error_terminal"
    ]["fields"]:
        fail("callback ValidationError ownership dropped")
    if "exact_stage5c_error_some" not in terminal_matrix["stage5c_error_terminal"]["fields"]:
        fail("exact Stage 5C terminal error dropped")
    require_exact(
        inventory["terminal_receipt_contract"]["reasons"],
        [
            "CallbackValidationError",
            "IntentCapacityExceeded",
            "IdentityMismatch",
            "ChronologyMismatch",
            "PaperModeMismatch",
            "Stage5cIntentValidationFailed",
            "Stage5cPendingRequestMismatch",
        ],
        "terminal reason taxonomy drift",
    )
    require_exact(
        set(inventory["terminal_reason_producers"]),
        set(inventory["terminal_receipt_contract"]["reasons"]),
        "terminal reason without exact producer",
    )
    if any(not producers for producers in inventory["terminal_reason_producers"].values()):
        fail("empty terminal reason producer set")
    require_exact(
        inventory["terminal_receipt_contract"]["records_original_intent_count_as_usize"],
        True,
        "terminal original intent count type drift",
    )
    if "true_preflight_intent_count_usize" not in terminal_matrix[
        "preflight_ok_terminal"
    ]["fields"]:
        fail("preflight true intent count dropped")
    if "derived_original_intent_count_usize" not in terminal_matrix[
        "stage5c_error_terminal"
    ]["fields"]:
        fail("Stage 5C derived intent count dropped")
    for contract_name in ("success_receipt_contract", "terminal_receipt_contract"):
        contract = inventory[contract_name]
        require_exact(contract["constructor_count"], 1, f"{contract_name} constructor drift")
        require_exact(
            contract["constructor_call_site_count"],
            1,
            f"{contract_name} call-site drift",
        )
        forbidden = set(contract["forbidden_surfaces"])
        if not {"Debug", "Clone", "From", "Into", "Serialize", "Deserialize"}.issubset(
            forbidden
        ):
            fail(f"{contract_name} forbidden-surface drift")
    if not {
        "settled",
        "into_settled",
        "batch",
        "intent",
        "request_ids",
        "generic_parts",
    }.issubset(set(inventory["success_receipt_contract"]["forbidden_surfaces"])):
        fail("public Stage5c settled inspection escaped success receipt")
    require_exact(
        inventory["exactly_once_contract"]["scope"],
        "process_local_only",
        "exactly-once scope drift",
    )
    require_exact(
        inventory["exactly_once_contract"]["crash_restart_policy_deferred"],
        True,
        "crash/restart policy opened",
    )

    closed = inventory["closed_surfaces"]
    opened_private = {
        "actual_callback_invocation",
        "strategy_state_mutation",
        "in_memory_intent_construction",
        "escrow_validation_or_settlement",
    }
    if any(closed[name] is not True for name in opened_private):
        fail("accepted B3E private surface regressed")
    if any(
        value is not False
        for name, value in closed.items()
        if name not in opened_private
    ):
        fail("forbidden B3F surface opened")

    print("stage5e-b3f-callback-settlement-escrow-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
