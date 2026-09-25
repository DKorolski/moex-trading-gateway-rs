#!/usr/bin/env python3
"""Semantic negative mutations for the Stage 8B-P1-f R3 design."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1f_r3_design_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_p1f_r3_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load P1-f R3 checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def edit_json(root: Path, relative: str, mutate: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {old}")
    path.write_text(text.replace(old, new, 1))


def inv(root: Path, module: Any, mutate: Callable[[dict[str, Any]], None]) -> None:
    edit_json(root, module.INVENTORY, mutate)


def input_digest(root: Path, module: Any) -> str:
    digest = hashlib.sha256()
    for relative in sorted((
        module.DOCUMENT, module.INVENTORY, module.MATRIX, module.MODELS,
        module.TARGET, module.STATUS, module.ROADMAP,
        "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua",
    )):
        raw = (root / relative).read_bytes()
        digest.update(relative.encode())
        digest.update(len(raw).to_bytes(8, "big"))
        digest.update(raw)
    return digest.hexdigest()


Case = tuple[str, Callable[[Path, Any], None], str]
CASES: list[Case] = [
    ("control-root-under-service-state", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(control_root="/var/lib/moex-finam-p1-paper/state/control")), "authority control root drift"),
    ("service-writable-control-parent", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(control_root_custody="moex-p1-paper:moex-p1-paper 0700")), "authority parent custody drift"),
    ("service-can-unlink", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(authority_root_custody="root owned but service may unlink")), "service mutation veto drift"),
    ("authority-in-runtime-state", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(runtime_state_unchanged="state contains authority")), "runtime state/authority separation drift"),
    ("service-is-authority-writer", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(single_writer="moex-p1-paper service")), "authority single-writer drift"),
    ("manifest-sequence-unbound", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(manifest_binding="predecessor_event_sha256 only")), "history predecessor binding drift"),
    ("manifest-predecessor-unbound", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(manifest_binding="authority_sequence only")), "history predecessor binding drift"),
    ("claim-after-effect", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(claim_transaction="commit after first effect with fsync")), "claim transaction drift"),
    ("claim-without-fsync", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(claim_transaction="commit before first effect")), "claim transaction drift"),
    ("manifest-reusable", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(consumed_history="manifest may be reused")), "consumed-manifest history drift"),
    ("missing-history-becomes-unclaimed", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(missing_corrupt_or_rollback="reconstruct Unclaimed and retry")), "history rollback disposition drift"),
    ("rollback-auto-repair", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(missing_corrupt_or_rollback="never reconstruct Unclaimed; silently repair")), "history rollback disposition drift"),
    ("restart-extends-deadline", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(same_active_restart="new deadline")), "restart/new-manifest custody drift"),
    ("new-manifest-reuses-sequence", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(new_manifest="reuse current authority sequence")), "restart/new-manifest custody drift"),
    ("claim-receipt-service-path", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"][0].update(path="/var/lib/moex-finam-p1-paper/state/claim.json")), "Claimed receipt outside protected authority root"),
    ("materialized-receipt-service-path", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"][3].update(path="/var/lib/moex-finam-p1-paper/state/ready.json")), "ReadyForBootstrap receipt outside protected authority root"),
    ("materialized-artifact-no-control-root", lambda r, m: inv(r, m, lambda v: v["artifact_contracts"][2].update(custody="root:moex-p1-paper 0440")), "materialized receipt custody drift"),
    ("custody-field-missing", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].pop("single_writer")), "authority custody key inventory drift"),
    ("custody-field-extra", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(replay_allowed=True)), "authority custody key inventory drift"),
    ("manifest-directory-outside-control", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(manifest_directory_template="/tmp/{manifest_sha256}")), "Claimed receipt outside protected authority root"),
    ("p1d4-script-inventory-no-xinfo-stream", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][5]["nested_commands"].remove("XINFO STREAM exact-command-stream predecessor last-generated-id")), "reserved publication XINFO STREAM capability missing"),
    ("p1d4-source-no-xinfo-stream", lambda r, m: replace(r, "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs", "redis.call('XINFO', 'STREAM', stream)", "redis.call('XINFO', 'GROUPS', stream)"), "production Redis script hashes.p1d4-command-publication-v1 value drift"),
    ("initializer-private-source", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][0].update(source="Stage8bP1RedisBackend::initialize_fresh_namespace")), "public initializer trace drift"),
    ("initializer-no-verify-command", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][0].update(command="EVAL namespace-initialization-v1")), "public initializer trace drift"),
    ("initializer-partial-response-retry", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][0].update(response_loss="retry verifier only")), "public initializer trace drift"),
    ("initializer-trace-removed", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["public_operation_traces"].pop(0)), "public conformance trace inventory drift"),
    ("initializer-step-reordered", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["public_operation_traces"][0]["ordered_steps"].reverse()), "initializer ordered steps drift"),
    ("provisioner-no-initializer", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][0]["allowed"].remove("EVAL pinned-namespace-initialization-script exact-M10-and-command-keys")), "source operation unreachable: fresh-namespace"),
    ("provisioner-no-verifier", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][0]["allowed"].remove("EVAL namespace-verify-v1 exact-keys-argv")), "source operation unreachable: fresh-namespace"),
    ("supervisor-no-reserved-script", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].remove("EVAL one-of-four-command-publication-scripts exact-keys-argv")), "source operation unreachable: command-publication"),
    ("supervisor-no-xlen", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].remove("XLEN canonical-m10")), "source operation unreachable: retention-admission"),
    ("finam-feeder-no-m10-script", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][2]["allowed"].remove("EVAL m10-publication-v1 exact-key-argv")), "source operation unreachable: m10-publish"),
    ("reserved-trace-no-xinfo-stream", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["public_operation_traces"][1]["nested_authority"].remove("XINFO STREAM exact-command-stream predecessor last-generated-id")), "reserved trace predecessor authority drift"),
    ("supervisor-arbitrary-eval", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].append("EVAL")), "arbitrary EVAL capability opened"),
    ("script-wrong-key", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][5].update(keys="any key")), "Redis scripts contract digest drift"),
    ("operation-count-one", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][4].update(command="XREVRANGE COUNT 1")), "Redis source_operations contract digest drift"),
    ("model-case-removed", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"].pop()), "model fixture bytes drift"),
    ("matrix-optional", lambda r, m: replace(r, m.MATRIX, ",REQUIRED\nP1F-R3-032", ",OPTIONAL\nP1F-R3-032"), "acceptance matrix bytes drift"),
    ("document-control-root-removed", lambda r, m: replace(r, m.DOCUMENT, "/var/lib/moex-finam-p1-paper-control", "/tmp/p1-control"), "design document bytes drift"),
    ("status-r2-active", lambda r, m: replace(r, m.STATUS, "active P1-f R3 design/checker correction candidate", "active P1-f R2 design/checker correction candidate"), "current status R3 boundary missing"),
    ("roadmap-auto-open", lambda r, m: replace(r, m.ROADMAP, "P1F-I remains closed until R3\nacceptance", "P1F-I opens automatically"), "roadmap R3 hold boundary missing"),
    ("runtime-live-open", lambda r, m: inv(r, m, lambda v: v["closed_surfaces"].update(runtime_live_authorized=True)), "unchanged closed_surfaces drift"),
    ("correction-parent-drift", lambda r, m: inv(r, m, lambda v: v["correction_parent"].update(commit="00" * 20)), "correction parent.commit value drift"),
    ("phase-opens-on-r2", lambda r, m: inv(r, m, lambda v: v["phases"][0].update(requires_prior_acceptance="P1F-R2-design")), "phases[0].requires_prior_acceptance value drift"),
    ("schema-bool", lambda r, m: inv(r, m, lambda v: v.update(schema_version=True)), "design schema drift"),
    ("namespace-script-hash", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][1].update(sha256="00" * 32)), "Redis script hash drift: namespace-verify-v1"),
    ("flushdb-unforbidden", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["global_forbidden_commands"].remove("FLUSHDB")), "forbidden Redis commands length drift"),
    ("custody-evidence-removed", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["authority_custody"].update(implementation_evidence="unit tests only")), "R3 phase_lifecycle contract drift"),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r3-design-") as directory:
        baseline = Path(directory) / "baseline"
        shutil.copytree(ROOT, baseline, ignore=shutil.ignore_patterns(
            ".git", "target", "reports", "tmp", ".env", "*.log", "__pycache__"))
        module = load(0)
        module.validate(baseline, verify_lineage=False)
        baseline_digest = input_digest(baseline, module)
        for index, (name, mutate, expected_error) in enumerate(CASES, start=1):
            root = Path(directory) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load(index)
            mutate(root, module)
            if input_digest(root, module) == baseline_digest:
                print(f"FAIL {name}: mutation was a no-op")
                return 1
            try:
                module.validate(root, verify_lineage=False)
            except (module.CheckFailure, module.r2.CheckFailure,
                    module.r2.r1.CheckFailure, module.r2.r1.r0.CheckFailure) as error:
                if expected_error not in str(error):
                    print(f"FAIL {name}: wrong rejection: {error}")
                    return 1
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1f-r3-design-negative-harness: PASS {len(CASES)}/{len(CASES)} no_op=0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
