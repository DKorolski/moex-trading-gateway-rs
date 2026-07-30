#!/usr/bin/env python3
"""Negative coverage for the Stage 5F CI/snapshot inheritance closure."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5f_ci_snapshot_inheritance_check.py"
ENTRY_CHECKER = "scripts/stage5f_atomic_hybrid_semantics_entry_check.py"
WRAPPER = "scripts/stage5f_b3f_snapshot_provenance_gate.sh"
CI = ".github/workflows/ci.yml"
IGNORED = shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__", ".DS_Store")


@dataclass(frozen=True)
class Case:
    name: str
    expected_marker: str
    mutate: Callable[[Path], None]


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if source.count(old) != 1:
        raise AssertionError(f"{label}: expected one replacement anchor")
    return source.replace(old, new, 1)


def run_checker_case(case: Case) -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-negative-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        case.mutate(candidate)
        checker = (
            ENTRY_CHECKER
            if case.expected_marker.startswith("Stage 5F CI execution authority drift")
            else CHECKER
        )
        completed = subprocess.run(
            [sys.executable, str(candidate / checker)],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print(f"expected failure missing for {case.name}", file=sys.stderr)
            return False
        if case.expected_marker not in output:
            print(
                f"expected marker {case.expected_marker!r} missing for {case.name}:\n{output}",
                file=sys.stderr,
            )
            return False
    print(f"PASS {case.name}")
    return True


def run_entry_checker_case(case: Case) -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-authority-negative-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        case.mutate(candidate)
        completed = subprocess.run(
            [sys.executable, str(candidate / ENTRY_CHECKER)],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print(f"expected failure missing for {case.name}", file=sys.stderr)
            return False
        if case.expected_marker not in output:
            print(
                f"expected marker {case.expected_marker!r} missing for {case.name}:\n{output}",
                file=sys.stderr,
            )
            return False
    print(f"PASS {case.name}")
    return True


def run_verified_execution_wrapper_mutation() -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-execution-negative-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        (candidate / WRAPPER).write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "for i in $(seq 1 580); do echo \"PASS forged-$i\"; done\n"
            "echo \"stage5f-b3f-snapshot-provenance-gate: ok tested_source_ref=e14654f7129aa61011931306140a3bfefe2fcfbc cases=580\"\n"
        )
        completed = subprocess.run(
            [
                sys.executable,
                str(candidate / CHECKER),
                "--execute-verified-provenance",
            ],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print("expected failure missing for wrapper-mutated-before-verified-execution", file=sys.stderr)
            return False
        if "Stage 5F wrapper authority digest drift" not in output or "PASS forged-" in output:
            print(
                "verified execution did not reject the mutated wrapper before use:\n" + output,
                file=sys.stderr,
            )
            return False
    print("PASS wrapper-mutated-before-verified-execution")
    return True


def run_missing_snapshot_case() -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-missing-snapshot-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        completed = subprocess.run(
            ["bash", str(candidate / WRAPPER)],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print("expected failure missing for accepted-snapshot-unavailable", file=sys.stderr)
            return False
        if "accepted B3F snapshot commit unavailable" not in output:
            print(
                "expected missing-snapshot marker absent for accepted-snapshot-unavailable:\n"
                + output,
                file=sys.stderr,
            )
            return False
    print("PASS accepted-snapshot-unavailable")
    return True


def main() -> int:
    cases = [
        Case(
            "accepted-snapshot-ref-rebound",
            "Stage 5F wrapper authority digest drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    'accepted_b3f_ref="e14654f7129aa61011931306140a3bfefe2fcfbc"',
                    'accepted_b3f_ref="0000000000000000000000000000000000000000"',
                    "accepted snapshot ref",
                )
            ),
        ),
        Case(
            "legacy-stage5e-gate-restored",
            "Stage 5F CI execution authority drift: .github/workflows/ci.yml",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "- name: Stage 5F atomic Hybrid semantics gate\n"
                    "        run: bash scripts/stage5f_atomic_hybrid_semantics_gate.sh",
                    "- name: Stage 5E lifecycle event-time gate\n"
                    "        run: bash scripts/stage5e_lifecycle_event_time_gate.sh",
                    "legacy Stage 5E gate",
                )
            ),
        ),
        Case(
            "provenance-redirected-to-head",
            "Stage 5F wrapper authority digest drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    'git -C "$snapshot_root" checkout --quiet --detach "$accepted_b3f_ref"',
                    'git -C "$snapshot_root" checkout --quiet --detach HEAD',
                    "snapshot checkout",
                )
            ),
        ),
        Case(
            "stage5f-negative-harness-omitted",
            "Stage 5F CI execution authority drift: .github/workflows/ci.yml",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "      - name: Stage 5F atomic Hybrid negative harness\n"
                    "        run: python3 scripts/stage5f_atomic_hybrid_semantics_negative_harness.py\n"
                    "        timeout-minutes: 5\n\n",
                    "",
                    "Stage 5F negative CI step",
                )
            ),
        ),
        Case(
            "snapshot-gate-continue-on-error",
            "Stage 5F CI execution authority drift: .github/workflows/ci.yml",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "          python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance\n"
                    "        timeout-minutes: 20",
                    "          python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance\n"
                    "        continue-on-error: true\n"
                    "        timeout-minutes: 20",
                    "snapshot gate continue-on-error",
                )
            ),
        ),
        Case(
            "snapshot-gate-if-false",
            "Stage 5F CI execution authority drift: .github/workflows/ci.yml",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "          python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance\n"
                    "        timeout-minutes: 20",
                    "          python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance\n"
                    "        if: ${{ false }}\n"
                    "        timeout-minutes: 20",
                    "snapshot gate if false",
                )
            ),
        ),
        Case(
            "wrapper-forged-pass-rows",
            "Stage 5F wrapper authority digest drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    "(\n"
                    "  cd \"$snapshot_root\"\n"
                    "  python3 scripts/handoff_provenance_negative_harness.py\n"
                    ") | tee \"$output_log\"",
                    "# python3 scripts/handoff_provenance_negative_harness.py\n"
                    "for i in $(seq 1 580); do echo \"PASS forged-$i\"; done | tee \"$output_log\"",
                    "forged provenance pass rows",
                )
            ),
        ),
        Case(
            "wrapper-second-checkout-before-provenance",
            "Stage 5F wrapper authority digest drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    "fi\n\n(\n  cd \"$snapshot_root\"",
                    "fi\n\ngit -C \"$snapshot_root\" checkout --quiet --detach HEAD\n\n(\n  cd \"$snapshot_root\"",
                    "second checkout before provenance",
                )
            ),
        ),
        Case(
            "ci-direct-raw-provenance-invocation",
            "Stage 5F CI execution authority drift: .github/workflows/ci.yml",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "      - name: No-Redis evidence smoke\n",
                    "      - name: Ad hoc raw provenance\n"
                    "        run: python3 scripts/handoff_provenance_negative_harness.py\n\n"
                    "      - name: No-Redis evidence smoke\n",
                    "direct raw provenance invocation",
                )
            ),
        ),
        Case(
            "wrapper-harness-failure-suppressed",
            "Stage 5F wrapper authority digest drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    "  python3 scripts/handoff_provenance_negative_harness.py\n",
                    "  python3 scripts/handoff_provenance_negative_harness.py || true\n",
                    "suppressed provenance harness failure",
                )
            ),
        ),
    ]
    failures = [case.name for case in cases if not run_checker_case(case)]
    authority_cases = [
        Case(
            "ci-negative-actions-wrapper-replacement",
            "Stage 5F CI execution authority drift: scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
            lambda root: (root / "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py").write_text(
                replace_once(
                    (root / "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py").read_text(),
                    "    if failures:\n",
                    "    import os\n"
                    "    if os.environ.get(\"GITHUB_ACTIONS\") == \"true\":\n"
                    "        (ROOT / WRAPPER).write_text(\"#!/usr/bin/env bash\\nfor i in $(seq 1 580); do echo PASS forged-$i; done\\n\")\n"
                    "    if failures:\n",
                    "CI-negative Actions-only wrapper replacement",
                )
            ),
        ),
        Case(
            "stage5f-negative-actions-wrapper-replacement",
            "Stage 5F CI execution authority drift: scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
            lambda root: (root / "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py").write_text(
                replace_once(
                    (root / "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py").read_text(),
                    "def main() -> int:\n",
                    "def main() -> int:\n"
                    "    import os\n"
                    "    if os.environ.get(\"GITHUB_ACTIONS\") == \"true\":\n"
                    "        (ROOT / \"scripts/stage5f_b3f_snapshot_provenance_gate.sh\").write_text(\"#!/usr/bin/env bash\\nfor i in $(seq 1 580); do echo PASS forged-$i; done\\n\")\n",
                    "Stage5F-negative Actions-only wrapper replacement",
                )
            ),
        ),
        Case(
            "ci-negative-forged-case-count",
            "Stage 5F CI execution authority drift: scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
            lambda root: (root / "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py").write_text(
                replace_once(
                    (root / "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py").read_text(),
                    "    failures = [case.name for case in cases if not run_checker_case(case)]\n",
                    "    print(\"stage5f-ci-snapshot-inheritance-negative-harness: ok cases=16\")\n"
                    "    return 0\n"
                    "    failures = [case.name for case in cases if not run_checker_case(case)]\n",
                    "forged CI-negative case count",
                )
            ),
        ),
        Case(
            "verifier-wrapper-replaced-between-check-and-execution",
            "Stage 5F CI execution authority drift: scripts/stage5f_ci_snapshot_inheritance_check.py",
            lambda root: (root / "scripts/stage5f_ci_snapshot_inheritance_check.py").write_text(
                replace_once(
                    (root / "scripts/stage5f_ci_snapshot_inheritance_check.py").read_text(),
                    "        if args.execute_verified_provenance:\n"
                    "            subprocess.run([\"bash\", str(WRAPPER)], cwd=ROOT, check=True)\n",
                    "        if args.execute_verified_provenance:\n"
                    "            WRAPPER.write_text(\"#!/usr/bin/env bash\\nfor i in $(seq 1 580); do echo PASS forged-$i; done\\n\")\n"
                    "            subprocess.run([\"bash\", str(WRAPPER)], cwd=ROOT, check=True)\n",
                    "verifier check/use wrapper replacement",
                )
            ),
        ),
    ]
    failures.extend(
        case.name for case in authority_cases if not run_entry_checker_case(case)
    )
    if not run_verified_execution_wrapper_mutation():
        failures.append("wrapper-mutated-before-verified-execution")
    if not run_missing_snapshot_case():
        failures.append("accepted-snapshot-unavailable")
    if failures:
        print("FAIL " + ", ".join(failures), file=sys.stderr)
        return 1
    print("stage5f-ci-snapshot-inheritance-negative-harness: ok cases=16")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
