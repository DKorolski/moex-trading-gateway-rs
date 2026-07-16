#!/usr/bin/env python3
"""Marker-pinned bounded-parallel forbidden-surface negative harness."""

from __future__ import annotations

import concurrent.futures
import math
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

sys.dont_write_bytecode = True

from copy_review_baseline import copy_review_baseline


ROOT = Path(__file__).resolve().parents[1]
SCANNER = Path("scripts/forbidden_surface_scan.sh")
WORKER = Path("scripts/forbidden_surface_negative_case_worker.sh")


@dataclass(frozen=True)
class Case:
    name: str
    expected_marker: str
    expected_success: bool = False


CASES = [
    Case("same-module-extra-post", "broker-finam has unexpected .post( count"),
    Case("same-module-extra-delete", "real HTTP DELETE surface is forbidden outside"),
    Case("generic-method-post", "Method::POST is not allowed in gateway/order surfaces"),
    Case("generic-method-delete", "Method::DELETE surface is forbidden"),
    Case("route-string-bypass", "literal FINAM order route bypass is forbidden"),
    Case("non-reqwest-client-abstraction", "non-reqwest order endpoint HTTP abstraction is forbidden"),
    Case("wrong-module-post-delete", "real HTTP DELETE surface is forbidden outside"),
    Case("sltp-bracket-endpoint-expansion", "broker-finam has unexpected .post( count"),
    Case("runtime-command-consumer-bypass", "Method::POST is not allowed in gateway/order surfaces"),
    Case("strategy-semantic-kernel-transport-dependency", "strategy semantic kernel contains forbidden transport/runtime token"),
    Case("strategy-semantic-source-correspondence-drift", "correspondence target hash mismatch for crates/strategy-runtime-core/src/hybrid_intraday/types.rs"),
    Case("strategy-integrated-wrapper-oracle-drift", "wrapper oracle hash drifted before Stage 5B-2b"),
    Case("strategy-high180-profile-fixture-drift", "Stage 5 profile artifact drifted for config/imoexf-hybrid-high180-profile.redacted.toml"),
    Case("stage5c-paper-host-source-drift", "crates/strategy-runtime-core/src/stage5c_paper_host.rs: current hash mismatch"),
    Case("stage5c-paper-host-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5c_paper_host_admission.json"),
    Case("stage5cb-bootstrap-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cb_bootstrap_notification.json"),
    Case("stage5cc-restore-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cc_runtime_state_restore.json"),
    Case("stage5cd-warmup-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cd_history_warmup.json"),
    Case("stage5ce-recovery-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5ce_pending_recovery.json"),
    Case("stage5cf-semantic-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cf_semantic_bar.json"),
    Case("stage5cg-settlement-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cg_paper_intent_settlement.json"),
    Case("stage5ch-next-bar-loop-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5ch_controlled_next_bar_loop.json"),
    Case("stage5ci-paper-intent-lifecycle-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5ci_paper_intent_lifecycle.json"),
    Case("stage5cj-paper-broker-lifecycle-fixture-drift", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5cj_paper_broker_lifecycle.json"),
    Case("stage5c-api-freeze-manifest-drift", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-empty-evidence-map-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-remove-evidence-transition-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-remove-source-hash-path-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-alter-baseline-full-commit-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-alter-baseline-handoff-sha-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-remove-accepted-slice-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-remove-public-type-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("stage5c-remove-public-method-valid-json", "Stage 5 profile artifact drifted for docs/stage-5/stage-5c-api-freeze-manifest.json"),
    Case("semantic-formula-drift-with-ledger-rehash", "immutable Stage 5B-1 manifest mismatch for 'strategy-runtime/src/strategies/hybrid_intraday/intraday_breakout.rs'"),
    Case("semantic-source-commit-drift", "strategy semantic correspondence ledger field 'alor_source_commit'"),
    Case("remove-strategy-runtime-core-from-workspace", "strategy-runtime-core must remain an explicit workspace member"),
    Case("comment-out-workspace-member-but-leave-quoted-comment", "strategy-runtime-core must remain an explicit workspace member"),
    Case("redirect-strategy-runtime-core-lib-path", "strategy-runtime-core lib path redirect is forbidden"),
    Case("add-alternate-stage5b2-wrapper-and-export", "hybrid_intraday_runtime_alias.rs; approved path"),
    Case("disable-strategy-runtime-core-tests", "strategy-runtime-core autotests must remain enabled"),
    Case("add-default-build-script", "strategy-runtime-core default build.rs is forbidden"),
    Case("add-untracked-integration-wrapper-target", "crates/strategy-runtime-core/tests/hybrid_intraday_runtime.rs; approved path"),
    Case("add-untracked-bench-wrapper-target", "crates/strategy-runtime-core/benches/hybrid_intraday_runtime.rs; approved path"),
    Case("add-untracked-example-wrapper-target", "crates/strategy-runtime-core/examples/hybrid_intraday_runtime.rs; approved path"),
    Case("copy-wrapper-to-another-workspace-crate-and-export", "crates/broker-core/src/hybrid_intraday_runtime.rs; approved path"),
    Case("add-wrapper-in-new-workspace-member-outside-crates", "stage5-wrapper/src/lib.rs; approved path"),
    Case("workspace-exclude-drift", "workspace.exclude must remain empty before Stage 5B-2b"),
    Case("unapproved-path-dependency-edge", "unapproved local path dependency crates/broker-core:dev-dependencies.stage5-wrapper"),
    Case("excluded-local-path-dependency-wrapper", "workspace.exclude must remain empty before Stage 5B-2b"),
    Case("workspace-member-build-rs", "workspace build.rs is forbidden in the Stage 5B-2 trusted Cargo graph: crates/broker-core/build.rs"),
    Case("repository-local-cargo-config", "repository-local Cargo config is forbidden in the Stage 5B-2 trusted build model"),
    Case("explicit-target-escapes-declaring-member", "explicit Cargo source path escapes its declaring member crates/broker-cli"),
    Case("duplicate-oracle-under-alias-filename", "duplicate wrapper oracle source is forbidden before Stage 5B-2b"),
    Case("macro-meta-path-to-renamed-wrapper-inc", "crates/broker-core/src/stage5_macro_alias_path.rs; approved path"),
    Case("comment-separated-wrapper-definition", "crates/broker-core/src/stage5_comment_wrapper.rs; approved path"),
    Case("macro-generated-wrapper-definition", "crates/broker-core/src/stage5_macro_wrapper.rs; approved path"),
    Case("include-wrapper-oracle", "crates/broker-core/src/stage5_include_wrapper.rs; approved path"),
    Case("split-path-include-wrapper-oracle", "crates/broker-core/src/stage5_split_include.rs; approved path"),
    Case("any-include-macro-before-wrapper-gate", "crates/broker-core/src/stage5_generic_include.rs; approved path"),
    Case("comment-separated-include-macro", "crates/broker-core/src/stage5_comment_include.rs; approved path"),
    Case("nested-comment-separated-include-macro", "crates/broker-core/src/stage5_nested_comment_include.rs; approved path"),
    Case("raw-identifier-include-macro", "crates/broker-core/src/stage5_raw_identifier_include.rs; approved path"),
    Case("macro-indirected-include-activation", "crates/broker-core/src/stage5_indirect_include.rs; approved path"),
    Case("path-attribute-wrapper-oracle", "crates/broker-core/src/stage5_path_wrapper.rs; approved path"),
    Case("comment-separated-path-attribute", "crates/broker-core/src/stage5_comment_path.rs; approved path"),
    Case("cfg-attr-path-wrapper-activation", "crates/broker-core/src/stage5_cfg_attr_path.rs; approved path"),
    Case("macro-meta-path-wrapper-activation", "crates/broker-core/src/stage5_macro_meta_path.rs; approved path"),
    Case("split-path-oracle-include-str-outside-allowlist", "crates/broker-core/src/stage5_split_oracle_read.rs; approved path"),
    Case("escaped-oracle-include-str-outside-allowlist", "crates/broker-core/src/stage5_escaped_oracle_read.rs; approved path"),
    Case("unicode-escaped-oracle-filename", "crates/broker-core/src/stage5_unicode_oracle_read.rs; approved path"),
    Case("stringify-split-oracle-include-str", "crates/broker-core/src/stage5_stringify_oracle_read.rs; approved path"),
    Case("drift-bracket-terminal-reconciliation-fixture", "Stage 5 profile artifact drifted for tests/fixtures/stage5/bracket_terminal_reconciliation.json"),
    Case("close-stage5b2-wrapper-compiled-milestone", "Stage 5B-2 manifest field mismatch wrapper_compiled"),
    Case("open-stage5b2-runtime-host-boundary", "Stage 5 profile artifact drifted for tests/fixtures/stage5/stage5b2_callback_state_mapping.json"),
    Case("forbidden-harness-marker-check-removal", "forbidden negative harness artifact hash mismatch"),
    Case("forbidden-harness-inventory-reduction", "forbidden negative harness artifact hash mismatch"),
    Case("forbidden-worker-contract-drift", "forbidden negative harness artifact hash mismatch"),
    Case("forbidden-ci-timeout-lowered", "forbidden negative harness CI timeout is below"),
    Case("forbidden-baseline-positive-bypass", "forbidden negative harness artifact hash mismatch"),
    Case("forbidden-scanner-contract-drift", "forbidden scanner contract marker mismatch"),
    Case(
        "include-and-path-text-outside-rust-code",
        "forbidden-surface-scan: ok",
        expected_success=True,
    ),
]


@dataclass(frozen=True)
class Run:
    index: int
    name: str
    passed: bool
    duration_seconds: float
    diagnostics: str


def run_process(command: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        return subprocess.CompletedProcess(
            command,
            124,
            stdout,
            stderr + f"\nworker timed out after {timeout}s\n",
        )


def worker_inventory(root: Path) -> list[tuple[str, bool]]:
    result = subprocess.run(
        ["bash", str(root / WORKER), "--list-cases"],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stdout + result.stderr)
    inventory = []
    for line in result.stdout.splitlines():
        name, kind = line.split("|", 1)
        inventory.append((name, kind == "success"))
    return inventory


def run_case(base: Path, clean: Path, index: int, case: Case, timeout: int) -> Run:
    case_root = base / "cases" / f"{index:02d}-{case.name}"
    started = time.monotonic()
    try:
        shutil.copytree(clean, case_root)
        result = run_process(
            [
                "bash",
                str(case_root / WORKER),
                "--run-case",
                case.name,
                case.expected_marker,
                "success" if case.expected_success else "failure",
            ],
            case_root,
            timeout,
        )
        combined = result.stdout + result.stderr
        return Run(
            index,
            case.name,
            result.returncode == 0,
            time.monotonic() - started,
            "" if result.returncode == 0 else combined.strip(),
        )
    except Exception as error:  # noqa: BLE001 - worker diagnostics cross threads
        return Run(index, case.name, False, time.monotonic() - started, repr(error))
    finally:
        shutil.rmtree(case_root, ignore_errors=True)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="forbidden-negative-") as tmp:
        base = Path(tmp)
        clean = base / "clean"
        copy_review_baseline(ROOT, clean)

        declared = [(case.name, case.expected_success) for case in CASES]
        implemented = worker_inventory(clean)
        missing = sorted(set(declared) - set(implemented))
        extra = sorted(set(implemented) - set(declared))
        if declared != implemented or missing or extra or len(set(declared)) != len(declared):
            print(
                "forbidden-surface-negative-harness: inventory mismatch "
                f"missing={missing} extra={extra}",
                file=sys.stderr,
            )
            return 1

        clean_started = time.monotonic()
        clean_result = run_process(["bash", str(clean / SCANNER)], clean, 180)
        clean_duration = time.monotonic() - clean_started
        if clean_result.returncode != 0:
            print(clean_result.stdout + clean_result.stderr, file=sys.stderr)
            print("forbidden-surface-negative-harness: clean baseline failed", file=sys.stderr)
            return 1

        timeout = max(20, min(180, math.ceil(clean_duration * 8)))
        configured_workers = int(os.environ.get("FORBIDDEN_NEGATIVE_WORKERS", "4"))
        workers = max(1, min(configured_workers, 8, len(CASES)))
        (base / "cases").mkdir()
        suite_started = time.monotonic()
        results: list[Run] = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
            futures = [
                executor.submit(run_case, base, clean, index, case, timeout)
                for index, case in enumerate(CASES)
            ]
            for future in concurrent.futures.as_completed(futures):
                results.append(future.result())
        total_duration = time.monotonic() - suite_started
        results.sort(key=lambda result: result.index)
        failures = [result for result in results if not result.passed]

        print("Forbidden surface marker-pinned isolated verification")
        print(f"cases_declared={len(CASES)}")
        print(f"negative_cases={sum(not case.expected_success for case in CASES)}")
        print(f"positive_controls={sum(case.expected_success for case in CASES)}")
        print(f"workers={workers}")
        print(f"case_timeout_seconds={timeout}")
        print(f"passed={len(results) - len(failures)}")
        print(f"missing={missing}")
        print(f"extra={extra}")
        print(
            "worst_case_seconds="
            f"{max((result.duration_seconds for result in results), default=0.0):.3f}"
        )
        print(f"total_duration_seconds={total_duration:.3f}")
        for result in results:
            print(f"{'PASS' if result.passed else 'FAIL'} {result.name}")
            if result.diagnostics:
                print(result.diagnostics, file=sys.stderr)
        if failures:
            return 1
    print("forbidden-surface-negative-harness: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
