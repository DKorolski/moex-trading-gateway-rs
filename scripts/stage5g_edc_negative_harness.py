#!/usr/bin/env python3
"""Current-head mutation matrix for Stage 5G-e-d-c.

The accepted detached R5 gate contributes 276 predecessor mutations. This
matrix adds independently named e-d-c ownership/application/package mutations,
so the composed gate exceeds the required 320 cases.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_edc_check as checker


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_edc_check.py"
FILES = [
    checker.CONTRACT, checker.DESIGN, checker.PARENT, checker.REDUCER,
    checker.APPLICATION, checker.CLEAN_RESTART, checker.ORDER_POSITION,
    checker.RUNTIME, checker.LIB, checker.STATUS, checker.ONBOARDING,
    checker.GATE, checker.HANDOFF,
]
INHERITED_ACCEPTED_CASES = 276


def replace_once(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"anchor count for {path}: {old!r} => {text.count(old)}")
    target.write_text(text.replace(old, new, 1))


def mutate_contract(root: Path, mutation) -> None:
    path = root / checker.CONTRACT
    value = json.loads(path.read_text())
    mutation(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def cases() -> list[tuple[str, object]]:
    values: list[tuple[str, object]] = []
    scalar_mutations = [
        ("contract-stage", lambda c: c.__setitem__("stage", "5G-e-d-c-mutated")),
        ("contract-predecessor", lambda c: c.__setitem__("accepted_predecessor", "0" * 40)),
        ("contract-branch", lambda c: c.__setitem__("branch", "main")),
        ("contract-entry", lambda c: c.__setitem__("owning_entry_point", "bypass")),
        ("contract-input", lambda c: c.__setitem__("input", "RawBrokerRows")),
        ("contract-replay-policy", lambda c: c.__setitem__("replay_policy", "A_UNAUTHENTICATED")),
        ("contract-failure-count", lambda c: c.__setitem__("failure_injection_point_count", 13)),
        ("contract-negative-floor", lambda c: c.__setitem__("minimum_negative_mutation_count", 319)),
        ("contract-evidence-schema", lambda c: c.__setitem__("application_evidence_schema_version", 2)),
    ]
    for name, mutation in scalar_mutations:
        values.append((name, lambda root, mutation=mutation: mutate_contract(root, mutation)))
    for result in checker.RESULTS:
        values.append((f"remove-result-{result}", lambda root, result=result: mutate_contract(
            root, lambda c: c["results"].remove(result))))
    for flag in [
        "canonical_transition_reused", "authenticated_export_drop_restore",
        "source_post_state_consumed_before_success", "pre_authority_returned_on_failure",
    ]:
        values.append((f"disable-{flag}", lambda root, flag=flag: mutate_contract(
            root, lambda c: c.__setitem__(flag, False))))
    for surface in [
        "strategy_callback", "bar_timer_execution", "redis", "finam",
        "http_post_delete", "broker_dispatch", "runtime_live", "real_orders",
        "protective_execution", "external_journal", "stage5g_f", "stage6",
    ]:
        values.append((f"open-surface-{surface}", lambda root, surface=surface: mutate_contract(
            root, lambda c: c["closed_surfaces"].__setitem__(surface, True))))

    values.extend([
        ("remove-application-module", lambda r: replace_once(r, checker.PARENT, "mod application;", "// removed")),
        ("rename-owning-entry", lambda r: replace_once(
            r, checker.APPLICATION, "pub(crate) fn apply_stage5g_fresh_truth_reduction(",
            "pub(crate) fn bypass_stage5g_fresh_truth_reduction(")),
        ("admit-reduction-by-reference", lambda r: replace_once(
            r, checker.APPLICATION,
            "pub(crate) fn apply_stage5g_fresh_truth_reduction(\n    reduction: Stage5gFreshTruthReduction,",
            "pub(crate) fn apply_stage5g_fresh_truth_reduction(\n    reduction: &mut Stage5gFreshTruthReduction,")),
        ("make-candidate-clone", lambda r: replace_once(
            r, checker.REDUCER, "pub(crate) struct Stage5gOwnedReconciliationCandidate {",
            "#[derive(Clone)]\npub(crate) struct Stage5gOwnedReconciliationCandidate {")),
        ("remove-consuming-parts", lambda r: replace_once(
            r, checker.REDUCER, "into_application_parts(self)", "into_application_parts(&mut self)")),
        ("remove-history-type", lambda r: replace_once(
            r, checker.REDUCER, "#[derive(Clone, Copy, Default)]\nstruct Stage5gHistoryEvidence",
            "struct RemovedStage5gHistoryEvidence")),
        ("bypass-canonical-transition", lambda r: replace_once(
            r, checker.APPLICATION, "match apply_stage5g_restart_canonical_order_position_state(",
            "match removed_canonical_transition(")),
        ("skip-post-export", lambda r: replace_once(
            r, checker.APPLICATION, "stage5g_export_post_application_order_position",
            "removed_post_package_export")),
        ("skip-fresh-restore", lambda r: replace_once(
            r, checker.APPLICATION, "match restore_stage5g_clean_restart(",
            "match removed_fresh_restore(")),
        ("retain-source-restart", lambda r: replace_once(
            r, checker.APPLICATION, "drop(parts.restart);", "let _retained = parts.restart;")),
        ("enable-exact-replay-evidence", lambda r: replace_once(
            r, checker.PARENT, "&& !evidence.exact_replay_enabled", "&& evidence.exact_replay_enabled")),
        ("allow-callback-evidence", lambda r: replace_once(
            r, checker.PARENT, "&& !evidence.callback_invoked", "&& true")),
        ("allow-transport-evidence", lambda r: replace_once(
            r, checker.PARENT, "&& !evidence.transport_opened", "&& true")),
        ("remove-candidate-post-equality", lambda r: replace_once(
            r, checker.PARENT,
            "candidate_fingerprint_sha256\n            == evidence.applied_post_semantic_fingerprint_sha256",
            "candidate_fingerprint_sha256\n            != evidence.applied_post_semantic_fingerprint_sha256")),
        ("remove-restart-mode", lambda r: replace_once(
            r, checker.ORDER_POSITION, "Stage5gCanonicalApplicationMode::RestartFreshTruth,",
            "Stage5gCanonicalApplicationMode::ActiveSession,")),
        ("remove-active-mode", lambda r: replace_once(
            r, checker.ORDER_POSITION,
            "        canonical_evidence,\n        Stage5gCanonicalApplicationMode::ActiveSession,",
            "        canonical_evidence,\n        Stage5gCanonicalApplicationMode::RestartFreshTruth,")),
        ("remove-fresh-runtime-constructor", lambda r: replace_once(
            r, checker.RUNTIME, "stage5g_clean_reconstruction_candidate",
            "removed_reconstruction_candidate")),
        ("remove-package-evidence-binding", lambda r: replace_once(
            r, checker.CLEAN_RESTART,
            "    pub(crate) fresh_truth_application_evidence:\n",
            "    pub(crate) removed_application_evidence:\n")),
        ("open-redis", lambda r: replace_once(
            r, checker.APPLICATION, "//! reachable from this module.",
            "//! reachable from this module.\nfn forbidden() { let _ = redis::Client::open(\"redis://x\"); }")),
        ("open-http-post", lambda r: replace_once(
            r, checker.APPLICATION, "//! reachable from this module.",
            "//! reachable from this module.\nfn forbidden(c: reqwest::Client) { let _ = c.post(\"/orders\"); }")),
        ("open-callback", lambda r: replace_once(
            r, checker.APPLICATION, "//! reachable from this module.",
            "//! reachable from this module.\nfn forbidden() { on_broker_bar(); }")),
        ("rebind-forbidden-authority", lambda r: replace_once(
            r, checker.GATE, checker.FORBIDDEN_AUTHORITY_REF, "0" * 40)),
        ("run-forbidden-scanner-in-current-tree", lambda r: replace_once(
            r, checker.GATE, 'cd "$forbidden_root"', 'cd "$snapshot_root"')),
        ("remove-gate-output-evidence", lambda r: replace_once(
            r, checker.HANDOFF, "stage5g-edc-full-gate.txt", "removed-full-gate.txt")),
        ("remove-source-manifest-evidence", lambda r: replace_once(
            r, checker.HANDOFF, "stage5g-edc-source-manifest.json", "removed-source.json")),
        ("remove-repository-path-redaction", lambda r: replace_once(
            r, checker.HANDOFF, '.replace(str(ROOT), "<REPO>")', "")),
        ("remove-home-path-redaction", lambda r: replace_once(
            r, checker.HANDOFF, '.replace(str(Path.home()), "<HOME>")', "")),
    ])
    for point in checker.FAILURE_POINTS:
        values.append((f"remove-failure-point-{point}", lambda root, point=point: replace_once(
            root, checker.APPLICATION, f"    {point},\n", "")))
    for test in checker.FOCUSED_TESTS:
        values.append((f"remove-witness-{test}", lambda root, test=test: replace_once(
            root, checker.REDUCER, f"fn {test}()", f"fn removed_{test}()")))
    for index, witness in enumerate(checker.COMPILE_FAIL_WITNESSES):
        values.append((f"remove-compile-fail-{witness}", lambda root, witness=witness, index=index: replace_once(
            root, checker.LIB, witness, f"compile_fail_removed_{index}")))
    return values


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-edc-{name}-") as raw:
        root = Path(raw)
        for relative in FILES:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root), "--skip-git"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-edc-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    matrix = cases()
    if len({name for name, _ in matrix}) != len(matrix):
        raise SystemExit("stage5g-edc-negative: FAIL: duplicate case names")
    if INHERITED_ACCEPTED_CASES + len(matrix) < 320:
        raise SystemExit("stage5g-edc-negative: FAIL: composed matrix below 320")
    for name, mutation in matrix:
        run_case(name, mutation)
    total = INHERITED_ACCEPTED_CASES + len(matrix)
    print(
        f"stage5g-edc-negative: PASS current={len(matrix)}/{len(matrix)} "
        f"inherited={INHERITED_ACCEPTED_CASES} aggregate={total}/{total}"
    )


if __name__ == "__main__":
    main()
