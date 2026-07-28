#!/usr/bin/env python3
"""Compile-fail probes against the actual private B3F production topology."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs")
CALLBACK_MODULE_MARKER = "    pub(crate) mod callback_settlement {"
CALLBACK_PRIVATE_MARKER = "        struct Stage5eB3fAuditCommitmentSeal(());"


@dataclass(frozen=True)
class Case:
    name: str
    anchor: str
    injected_source: str
    expected_code: str


CASES = (
    Case(
        "actual_consume_seal_clone",
        CALLBACK_PRIVATE_MARKER,
        """

        fn b3f_ui_actual_consume_seal_clone(
            seal: &Stage5ePaperSettlementConsumeSeal,
        ) {
            let _cloned = (*seal).clone();
        }
""",
        "E0599",
    ),
    Case(
        "actual_consume_seal_copy",
        CALLBACK_PRIVATE_MARKER,
        """

        fn b3f_ui_actual_consume_seal_copy(
            seal: Stage5ePaperSettlementConsumeSeal,
        ) {
            let _first = seal;
            let _second = seal;
        }
""",
        "E0382",
    ),
    Case(
        "actual_sibling_seal_reconstruction",
        CALLBACK_MODULE_MARKER,
        """
    fn b3f_ui_actual_sibling_seal_reconstruction(
    ) -> callback_settlement::Stage5ePaperSettlementConsumeSeal {
        callback_settlement::Stage5ePaperSettlementConsumeSeal(())
    }

""",
        "E0603",
    ),
    Case(
        "actual_payload_capability_escape",
        CALLBACK_PRIVATE_MARKER,
        """

        fn b3f_ui_actual_payload_capability_escape(
            payload: Stage5ePaperSettlementPayload,
        ) {
            let _escaped = payload.consume_seal;
        }
""",
        "E0609",
    ),
    Case(
        "actual_escrow_second_consume",
        CALLBACK_MODULE_MARKER,
        """
    fn b3f_ui_actual_escrow_second_consume(
        escrow: Stage5ePaperCallbackResultEscrow,
        seal: &callback_settlement::Stage5ePaperSettlementConsumeSeal,
    ) {
        let _first = escrow.consume_for_settlement(seal);
        let _second = escrow.consume_for_settlement(seal);
    }

""",
        "E0382",
    ),
    Case(
        "actual_preflight_borrow_across_escrow_move",
        CALLBACK_MODULE_MARKER,
        """
    fn b3f_ui_actual_preflight_borrow_across_escrow_move(
        escrow: Stage5ePaperCallbackResultEscrow,
        preflight_seal: &callback_settlement::Stage5ePaperSettlementPreflightSeal,
        consume_seal: &callback_settlement::Stage5ePaperSettlementConsumeSeal,
    ) {
        let borrowed = escrow.borrow_for_settlement_preflight(preflight_seal);
        let _payload = escrow.consume_for_settlement(consume_seal);
        drop(borrowed);
    }

""",
        "E0505",
    ),
)


def copy_source_tree(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(
            ".git",
            "target",
            "reports",
            "tmp",
            ".env",
            "*.log",
            "__pycache__",
        ),
    )


def cargo_check(root: Path) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "true"
    env["CARGO_TARGET_DIR"] = str(ROOT / "target" / "stage5e-b3f-production-ui")
    return subprocess.run(
        [
            "cargo",
            "check",
            "-p",
            "strategy-runtime-core",
            "--lib",
            "--message-format=json",
        ],
        cwd=root,
        env=env,
        text=True,
        capture_output=True,
        timeout=180,
        check=False,
    )


def diagnostic_codes(stdout: str) -> set[str]:
    codes: set[str] = set()
    for line in stdout.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get("reason") != "compiler-message":
            continue
        code = item.get("message", {}).get("code")
        if isinstance(code, dict) and isinstance(code.get("code"), str):
            codes.add(code["code"])
    return codes


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5e-b3f-ui-") as temp:
        checkout = Path(temp) / "repo"
        copy_source_tree(checkout)
        source_path = checkout / SOURCE
        baseline = source_path.read_text()
        if "compile_error!" in baseline:
            raise SystemExit("stage5e-b3f-production-ui: FAIL: unconditional compile_error")

        baseline_result = cargo_check(checkout)
        if baseline_result.returncode != 0:
            raise SystemExit(
                "stage5e-b3f-production-ui: FAIL: baseline cargo check failed\n"
                + baseline_result.stderr[-4000:]
            )

        for case in CASES:
            if baseline.count(case.anchor) != 1:
                raise SystemExit(
                    f"stage5e-b3f-production-ui: FAIL: anchor drift for {case.name}"
                )
            mutated = baseline.replace(
                case.anchor,
                case.injected_source + case.anchor,
                1,
            )
            source_path.write_text(mutated)
            result = cargo_check(checkout)
            codes = diagnostic_codes(result.stdout)
            if result.returncode == 0 or case.expected_code not in codes:
                raise SystemExit(
                    "stage5e-b3f-production-ui: FAIL: "
                    f"{case.name} expected={case.expected_code} observed={sorted(codes)}"
                )
            print(
                "PASS "
                f"{case.name} expected={case.expected_code} observed={sorted(codes)}"
            )
            source_path.write_text(baseline)

    print(f"stage5e-b3f-production-ui: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
