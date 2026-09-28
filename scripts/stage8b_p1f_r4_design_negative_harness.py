#!/usr/bin/env python3
"""Semantic negative mutations for the Stage 8B-P1-f R4 design."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1f_r4_design_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_p1f_r4_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load P1-f R4 checker")
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


def custody(value: dict[str, Any]) -> dict[str, Any]:
    return value["phase_lifecycle"]["authority_custody"]


def input_digest(root: Path, module: Any) -> str:
    result = hashlib.sha256()
    for relative in sorted((
        module.DOCUMENT, module.INVENTORY, module.MATRIX, module.MODELS,
        module.TARGET, module.STATUS, module.ROADMAP,
        "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua",
    )):
        raw = (root / relative).read_bytes()
        result.update(relative.encode())
        result.update(len(raw).to_bytes(8, "big"))
        result.update(raw)
    return result.hexdigest()


Case = tuple[str, Callable[[Path, Any], None], str]
CASES: list[Case] = [
    ("control-root-under-service-state", lambda r, m: inv(r, m, lambda v: custody(v).update(control_root="/var/lib/moex-finam-p1-paper/state/control")), "authority control root drift"),
    ("service-writable-parent", lambda r, m: inv(r, m, lambda v: custody(v).update(control_root_custody="moex-p1-paper:moex-p1-paper 0700")), "authority parent custody drift"),
    ("service-can-unlink", lambda r, m: inv(r, m, lambda v: custody(v).update(authority_root_custody="service may unlink")), "service mutation veto drift"),
    ("partial-loss-reconstructs-unclaimed", lambda r, m: inv(r, m, lambda v: custody(v).update(partial_history_failure="reconstruct Unclaimed")), "partial history disposition drift"),
    ("partial-loss-without-trusted-root", lambda r, m: inv(r, m, lambda v: custody(v).update(partial_history_failure="never reconstruct Unclaimed")), "partial history disposition drift"),
    ("restore-may-touch-control", lambda r, m: inv(r, m, lambda v: custody(v).update(permitted_restore_operations="restore all /var/lib")), "restore exclusion drift"),
    ("restore-overlap-after-mutation", lambda r, m: inv(r, m, lambda v: custody(v).update(permitted_restore_operations="exclude /var/lib/moex-finam-p1-paper-control after restore")), "restore exclusion drift"),
    ("hash-chain-detects-full-rollback", lambda r, m: inv(r, m, lambda v: custody(v).update(coherent_full_rollback="hash chain automatically detects every coherent rollback and quarantines normal guardian")), "coherent rollback limit drift"),
    ("full-rollback-allows-claim", lambda r, m: inv(r, m, lambda v: custody(v).update(coherent_full_rollback="not claimed to be locally detectable; allow normal guardian claim")), "coherent rollback limit drift"),
    ("trust-boundary-no-operator", lambda r, m: inv(r, m, lambda v: custody(v).update(rollback_trust_boundary="control root outside every permitted restore")), "rollback trust boundary drift"),
    ("genesis-field-missing", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].pop("offline_registry")), "genesis protocol key inventory drift"),
    ("ordinary-claim-creates-genesis", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(command="ordinary phase claim creates genesis")), "ordinary claim gained genesis authority"),
    ("genesis-manifest-unbinds-host", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"]["manifest_binds"].remove("target_host_key")), "genesis manifest binding drift"),
    ("registry-on-vps", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(offline_registry="registry inside VPS never reissues")), "offline genesis registry drift"),
    ("registry-reissues-nonce", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(offline_registry="offline registry outside VPS may reissue nonce")), "offline genesis registry drift"),
    ("genesis-enables-claim-before-cert", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(local_steps="fsync then claims start")), "genesis local transaction drift"),
    ("activation-unbinds-host", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(activation_certificate="marks generation Activated binding generation genesis head installation")), "genesis activation certificate drift"),
    ("claim-without-activated-cert", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(claim_admission="local genesis admits sequence 1")), "pre-activation claim admission drift"),
    ("genesis-crash-starts-over", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(crash_recovery="start new genesis transaction; permanently rejected later")), "genesis crash/replay drift"),
    ("lost-root-treated-fresh", lambda r, m: inv(r, m, lambda v: custody(v)["genesis_protocol"].update(repeat_attempt="absence after prior activation is fresh installation")), "lost authority treated as fresh genesis"),
    ("rebind-trigger-removed", lambda r, m: inv(r, m, lambda v: custody(v)["control_loss_rebind"]["triggers"].pop()), "control-loss trigger inventory drift"),
    ("control-loss-normal-start", lambda r, m: inv(r, m, lambda v: custody(v)["control_loss_rebind"].update(normal_start="allow ordinary claim")), "control-loss quarantine drift"),
    ("rebind-without-review", lambda r, m: inv(r, m, lambda v: custody(v)["control_loss_rebind"].update(required_authority="automatic local repair")), "rebind review boundary drift"),
    ("rebind-same-generation", lambda r, m: inv(r, m, lambda v: custody(v)["control_loss_rebind"].update(generation_rule="reuse old generation")), "rebind generation rule drift"),
    ("rebind-self-authorized", lambda r, m: inv(r, m, lambda v: custody(v)["control_loss_rebind"].update(current_stage_authorized=True)), "authority rebind self-authorized"),
    ("operator-claim-genesis", lambda r, m: inv(r, m, lambda v: v["operator_authority"].update(ordinary_claim_cannot_create_genesis=False)), "operator genesis/rebind boundary drift"),
    ("operator-no-rebind", lambda r, m: inv(r, m, lambda v: v["operator_authority"].update(control_state_loss_requires_new_generation_rebind=False)), "operator genesis/rebind boundary drift"),
    ("implementation-no-restore-test", lambda r, m: inv(r, m, lambda v: custody(v).update(implementation_evidence="multi-UID and repeated genesis only")), "implementation evidence boundary drift"),
    ("model-coherent-auto-detect", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"][42].update(expected="hash chain automatically detects rollback")), "model fixture bytes drift"),
    ("model-genesis-by-claim", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"][43].update(expected="ordinary phase claim creates genesis")), "model fixture bytes drift"),
    ("model-case-removed", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"].pop()), "model fixture bytes drift"),
    ("matrix-optional", lambda r, m: replace(r, m.MATRIX, ",REQUIRED\nP1F-R4-074", ",OPTIONAL\nP1F-R4-074"), "acceptance matrix bytes drift"),
    ("document-full-rollback-claim", lambda r, m: replace(r, m.DOCUMENT, "not** claimed", "now** claimed"), "design document bytes drift"),
    ("status-r3-active", lambda r, m: replace(r, m.STATUS, "active P1-f R4 design/checker correction candidate", "active P1-f R3 design/checker correction candidate"), "current status R4 boundary missing"),
    ("roadmap-auto-open", lambda r, m: replace(r, m.ROADMAP, "P1F-I remains closed until R4\nacceptance", "P1F-I opens automatically"), "roadmap R4 hold boundary missing"),
    ("phase-opens-on-r3", lambda r, m: inv(r, m, lambda v: v["phases"][0].update(requires_prior_acceptance="P1F-R3-design")), "phases[0].requires_prior_acceptance value drift"),
    ("provisioner-no-verifier-regression", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][0]["allowed"].remove("EVAL namespace-verify-v1 exact-keys-argv")), "source operation unreachable: fresh-namespace"),
    ("reserved-xinfo-stream-regression", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][5]["nested_commands"].remove("XINFO STREAM exact-command-stream predecessor last-generated-id")), "reserved publication XINFO STREAM capability missing"),
    ("receipt-service-path-regression", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"][0].update(path="/var/lib/moex-finam-p1-paper/state/claim.json")), "R4 freshness_contract contract drift"),
    ("terminal-deadline-extension-regression", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(restart_extends_deadline=True)), "R4 phase_lifecycle contract drift"),
    ("v5-v4-conflation-regression", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(historical_v5_recovery="schedule V4 recovery")), "R4 freshness_contract contract drift"),
    ("runtime-live-open", lambda r, m: inv(r, m, lambda v: v["closed_surfaces"].update(runtime_live_authorized=True)), "unchanged closed_surfaces drift"),
    ("correction-parent-drift", lambda r, m: inv(r, m, lambda v: v["correction_parent"].update(commit="00" * 20)), "correction parent.commit value drift"),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r4-design-") as directory:
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
            except (module.CheckFailure, module.r3.CheckFailure, module.r3.r2.CheckFailure,
                    module.r3.r2.r1.CheckFailure, module.r3.r2.r1.r0.CheckFailure) as error:
                if expected_error not in str(error):
                    print(f"FAIL {name}: wrong rejection: {error}")
                    return 1
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1f-r4-design-negative-harness: PASS {len(CASES)}/{len(CASES)} no_op=0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
