#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-f Ib source checker."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p1f_ib_check as check


ROOT = Path(__file__).resolve().parents[1]
REQUIRED_PATHS = {
    check.GUARDIAN,
    check.SUPERVISION,
    check.BINARY,
    check.LIB,
    check.DOCUMENT,
    check.INVENTORY,
    check.MATRIX,
    check.STATUS,
    check.ROADMAP,
}


def replace(root: Path, path: str, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if old not in text:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(text.replace(old, new, 1))


def mutate_json(root: Path, callback: Callable[[dict], None]) -> None:
    path = root / check.INVENTORY
    value = json.loads(path.read_text())
    callback(value)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def copy_inputs(root: Path) -> None:
    for relative in REQUIRED_PATHS:
        source = ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def cases() -> list[tuple[str, str, Callable[[Path], None]]]:
    return [
        (
            "fixed-child-binary",
            "child",
            lambda root: replace(
                root,
                check.SUPERVISION,
                '"/usr/local/libexec/moex/stage8b-p1-paper-supervisor"',
                '"/tmp/arbitrary-child"',
            ),
        ),
        (
            "fixed-child-config",
            "child",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "OsString::from(STAGE8B_P1E_SUPERVISOR_CONFIG_PATH)",
                'OsString::from("/tmp/arbitrary.json")',
            ),
        ),
        (
            "dedicated-process-group",
            "ownership",
            lambda root: replace(root, check.SUPERVISION, ".process_group(0);", ";"),
        ),
        (
            "supplementary-groups",
            "identity",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "libc::setgroups(0, std::ptr::null())",
                "libc::getgroups(0, std::ptr::null_mut())",
            ),
        ),
        (
            "parent-death-signal",
            "ownership",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "libc::PR_SET_PDEATHSIG",
                "libc::PR_SET_NAME",
            ),
        ),
        (
            "whole-group-kill",
            "stop",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "libc::kill(-self.process_group, signal)",
                "libc::kill(self.process_group, signal)",
            ),
        ),
        (
            "restart-budget",
            "restart",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "const MAX_STARTS_PER_WINDOW: usize = 5;",
                "const MAX_STARTS_PER_WINDOW: usize = 6;",
            ),
        ),
        (
            "restart-window",
            "restart",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "const RESTART_WINDOW: StdDuration = StdDuration::from_secs(600);",
                "const RESTART_WINDOW: StdDuration = StdDuration::from_secs(601);",
            ),
        ),
        (
            "restart-delay",
            "restart",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "const RESTART_DELAY: StdDuration = StdDuration::from_secs(5);",
                "const RESTART_DELAY: StdDuration = StdDuration::from_secs(4);",
            ),
        ),
        (
            "admission-bypass",
            "admission",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "store.admit_active_phase(manifest_sha256, trusted_now)",
                "store.admit_active_phase_unchecked(manifest_sha256, trusted_now)",
            ),
        ),
        (
            "recovery-bypass",
            "recovery",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "store.resume_stopping_phase(manifest_sha256, trusted_now)?",
                "store.admit_active_phase(manifest_sha256, trusted_now)?",
            ),
        ),
        (
            "signal-registration-order",
            "signal",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "register_unix_signal_supervision().await?",
                "{ let _ = resolve_service_identity()?; register_unix_signal_supervision().await? }",
            ),
        ),
        (
            "synchronous-term-registration",
            "signal",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "let terminate = unix_signal(SignalKind::terminate())",
                "let terminate = unix_signal(SignalKind::interrupt())",
            ),
        ),
        (
            "pre-spawn-signal-barrier-priority",
            "signal",
            lambda root: replace(root, check.SUPERVISION, "        biased;", "        // unbiased"),
        ),
        (
            "pending-stopping-recovery-route",
            "recovery",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired)",
                "Err(Stage8bP1fAuthorityErrorV1::InvalidDocument)",
            ),
        ),
        (
            "fatal-child-exit-preservation",
            "exit-status",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "Self::ChildExit(code) => *code",
                "Self::ChildExit(_) => 70",
            ),
        ),
        (
            "recovered-stop-nonzero",
            "exit-status",
            lambda root: replace(
                root,
                check.SUPERVISION,
                "if !forced && decision == Stage8bP1fDeadlineDecisionV1::ForceKill",
                "if !forced && decision == Stage8bP1fDeadlineDecisionV1::BeginStopping",
            ),
        ),
        (
            "real-process-case-removal",
            "evidence",
            lambda root: replace(
                root,
                check.GUARDIAN,
                "fn linux_guardian_death_signal_kills_child()",
                "fn removed_linux_guardian_death_signal_kills_child()",
            ),
        ),
        (
            "opened-runtime-live",
            "closed-surface",
            lambda root: mutate_json(
                root, lambda value: value["closed_surfaces"].__setitem__("runtime_live", True)
            ),
        ),
        (
            "matrix-removal",
            "evidence",
            lambda root: replace(
                root,
                check.MATRIX,
                "P1FIB-020,closed,Installation VPS Redis FINAM provider dispatch runtime-live and real orders remain closed,REQUIRED\n",
                "",
            ),
        ),
        (
            "operational-roadmap-open",
            "closed-surface",
            lambda root: replace(root, check.ROADMAP, "P1F-O0 stays closed", "P1F-O0 is open"),
        ),
    ]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-ib-positive-") as directory:
        baseline = Path(directory)
        copy_inputs(baseline)
        try:
            check.validate(baseline, verify_lineage=False)
        except Exception as error:
            print(f"FAIL positive-control category=infrastructure error={error}")
            return 1
        document = baseline / check.DOCUMENT
        document.write_text(document.read_text() + "\n<!-- nonsemantic control -->\n")
        try:
            check.validate(baseline, verify_lineage=False)
        except Exception as error:
            print(f"FAIL nonsemantic-control category=false-positive error={error}")
            return 1
        print("PASS positive-control")
        print("PASS nonsemantic-control")

    passed = 0
    for name, category, mutate in cases():
        with tempfile.TemporaryDirectory(prefix="stage8b-p1f-ib-negative-") as directory:
            root = Path(directory)
            copy_inputs(root)
            before = {path: (root / path).read_bytes() for path in REQUIRED_PATHS}
            mutate(root)
            after = {path: (root / path).read_bytes() for path in REQUIRED_PATHS}
            if before == after:
                print(f"FAIL {name}: mutation was a no-op")
                return 1
            try:
                check.validate(root, verify_lineage=False)
            except check.CheckFailure as error:
                print(f"PASS {name} category={category} rejection={error}")
                passed += 1
                continue
            except (OSError, UnicodeDecodeError) as error:
                print(f"FAIL {name}: infrastructure failure was not a semantic rejection: {error}")
                return 1
            print(f"FAIL {name}: checker accepted mutation")
            return 1
    print(f"PASS stage8b-p1f-ib-negative-harness {passed}/{len(cases())}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
