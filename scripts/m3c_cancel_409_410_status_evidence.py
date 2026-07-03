#!/usr/bin/env python3
"""Generate M3c cancel 409/410 status evidence without order endpoint calls."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import subprocess
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


OFFICIAL_DOCS_URL = "https://api.finam.ru/docs/rest/"
CANCEL_ENDPOINT = {
    "purpose": "CancelOrder",
    "http_method": "DELETE",
    "section_start": "DELETE /v1/accounts/A12345/orders/ORD789012",
    "section_end": "POST /v1/accounts/A12345/sltp-orders",
    "docs_status_prefix": "ordersservice_cancelorder.",
    "route_template": "/v1/accounts/{account_id}/orders/{order_id}",
    "documented_statuses": [200, 400, 401, 404, 429, 500, 503, 504],
}
UNDOCUMENTED_CANCEL_STATUSES = [409, 410]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run_text(command: list[str], cwd: Path) -> tuple[int, str, str]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    return completed.returncode, completed.stdout.strip(), completed.stderr.strip()


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch_official_docs_with_urllib(
    url: str, timeout_seconds: int
) -> tuple[int | None, bytes]:
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": "moex-m3c-evidence/1.0",
            "Accept-Encoding": "gzip",
        },
    )
    with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
        status = getattr(response, "status", None)
        body = response.read()
        if response.headers.get("Content-Encoding") == "gzip":
            body = gzip.decompress(body)
        return status, body


def fetch_official_docs_with_curl(
    url: str, timeout_seconds: int, cwd: Path
) -> tuple[int | None, bytes]:
    status_marker = b"\n__M3C_HTTP_STATUS__:"
    completed = subprocess.run(
        [
            "curl",
            "-L",
            "--compressed",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            str(timeout_seconds),
            "--write-out",
            "\n__M3C_HTTP_STATUS__:%{http_code}\n",
            url,
        ],
        cwd=cwd,
        capture_output=True,
    )
    if completed.returncode != 0:
        stderr = completed.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"curl exited {completed.returncode}: {stderr}")
    body, marker, status_bytes = completed.stdout.rpartition(status_marker)
    if not marker:
        raise RuntimeError("curl response did not include HTTP status marker")
    status_text = status_bytes.strip().decode("ascii", errors="replace")
    return int(status_text), body


def fetch_official_docs(
    url: str, timeout_seconds: int, cwd: Path
) -> tuple[int | None, bytes, str]:
    errors = []
    for attempt in range(3):
        try:
            status, body = fetch_official_docs_with_urllib(url, timeout_seconds)
            return status, body, f"urllib-gzip-attempt-{attempt + 1}"
        except Exception as exc:  # pragma: no cover - network stability branch
            errors.append(f"urllib attempt {attempt + 1}: {type(exc).__name__}: {exc}")
    try:
        status, body = fetch_official_docs_with_curl(url, timeout_seconds, cwd)
        return status, body, "curl-compressed-fallback"
    except Exception as exc:  # pragma: no cover - network stability branch
        errors.append(f"curl fallback: {type(exc).__name__}: {exc}")
        raise RuntimeError("; ".join(errors)) from exc


def extract_section(text: str, start_marker: str, end_marker: str | None) -> tuple[str, bool]:
    start = text.find(start_marker)
    if start < 0:
        return "", False
    end = text.find(end_marker, start + len(start_marker)) if end_marker else -1
    if end < 0:
        end = text.find("### ", start + len(start_marker))
    if end < 0:
        end = len(text)
    return text[start:end], True


def status_token_present(section: str, status_code: int, docs_status_prefix: str) -> bool:
    token = str(status_code)
    return (
        f"{docs_status_prefix}{token}" in section
        or f"#{token} " in section
        or f">{token}<" in section
        or f"\n{token} " in section
        or f" {token} " in section
        or section.rstrip().endswith(f" {token}")
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate source-bound M3c cancel 409/410 status evidence."
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the evidence report.",
    )
    parser.add_argument(
        "--output",
        default=Path("reports/m3c-order-endpoint-gate/cancel-409-410-status-evidence.json"),
        type=Path,
        help="Cancel 409/410 status evidence JSON output path.",
    )
    parser.add_argument(
        "--docs-url",
        default=OFFICIAL_DOCS_URL,
        help="Official FINAM REST documentation URL to recheck.",
    )
    parser.add_argument(
        "--timeout-seconds",
        default=20,
        type=int,
        help="HTTP timeout for fetching official documentation.",
    )
    args = parser.parse_args()

    root = repo_root()
    source_archive = (root / args.source_archive).resolve()
    output = (root / args.output).resolve()

    if not source_archive.exists():
        print(f"source archive does not exist: {source_archive}", file=sys.stderr)
        return 2

    git_code, source_commit_full_sha, git_stderr = run_text(
        ["git", "rev-parse", "HEAD"], root
    )
    if git_code != 0:
        print(git_stderr, file=sys.stderr)
        return git_code

    scan_code, scan_stdout, _scan_stderr = run_text(
        ["bash", "scripts/forbidden_surface_scan.sh"], root
    )
    scan_script = root / "scripts/forbidden_surface_scan.sh"

    try:
        docs_http_status, docs_body, docs_fetch_transport = fetch_official_docs(
            args.docs_url, args.timeout_seconds, root
        )
        docs_fetch_error = None
    except Exception as exc:  # pragma: no cover - exercised only on network failure
        docs_http_status = None
        docs_body = b""
        docs_fetch_transport = None
        docs_fetch_error = f"{type(exc).__name__}: {exc}"

    docs_text = docs_body.decode("utf-8", errors="replace")
    section, section_found = extract_section(
        docs_text, CANCEL_ENDPOINT["section_start"], CANCEL_ENDPOINT["section_end"]
    )
    documented_results = {
        str(status): status_token_present(
            section, status, CANCEL_ENDPOINT["docs_status_prefix"]
        )
        for status in CANCEL_ENDPOINT["documented_statuses"]
    }
    undocumented_results = {
        str(status): status_token_present(
            section, status, CANCEL_ENDPOINT["docs_status_prefix"]
        )
        for status in UNDOCUMENTED_CANCEL_STATUSES
    }
    cancel_result = {
        "purpose": CANCEL_ENDPOINT["purpose"],
        "http_method": CANCEL_ENDPOINT["http_method"],
        "route_template": CANCEL_ENDPOINT["route_template"],
        "route_template_sha256": sha256_bytes(
            CANCEL_ENDPOINT["route_template"].encode("utf-8")
        ),
        "official_docs_section_found": section_found,
        "documented_statuses_present": documented_results,
        "undocumented_409_410_statuses_present": undocumented_results,
        "section_sha256": sha256_bytes(section.encode("utf-8")) if section else None,
        "raw_section_exported": False,
    }

    official_docs_recheck_passed = (
        docs_http_status == 200
        and docs_fetch_error is None
        and cancel_result["official_docs_section_found"]
        and all(cancel_result["documented_statuses_present"].values())
        and all(
            present is False
            for present in cancel_result["undocumented_409_410_statuses_present"].values()
        )
    )

    evidence = {
        "m3c_step": "M3c-25",
        "cancel_409_410_status_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": sha256_file(source_archive),
        "slot": "cancel_409_410_status_semantics",
        "requested_slot_status": "EvidenceProvided",
        "official_docs_url": args.docs_url,
        "official_docs_fetch_transport": docs_fetch_transport,
        "official_docs_http_status": docs_http_status,
        "official_docs_sha256": sha256_bytes(docs_body) if docs_body else None,
        "official_docs_fetch_error": docs_fetch_error,
        "official_docs_recheck_passed": official_docs_recheck_passed,
        "cancel_result": cancel_result,
        "semantics_policy": {
            "documented_cancel_success_status": 200,
            "documented_cancel_not_found_status": 404,
            "undocumented_cancel_statuses": UNDOCUMENTED_CANCEL_STATUSES,
            "cancel_409_410_are_not_blind_success": True,
            "future_send_outcome": "TimeoutUnknownPending",
            "order_path_event": "CancelTimedOut",
            "order_path_state": "CancelTimeoutUnknownPending",
            "ack_status": "UnknownPending",
            "ack_reason_code": "ReconciliationRequired",
            "operator_disarm_signal": "CancelTimeoutUnknownPending",
            "cancel_reconciliation_required": True,
            "state_machine_transition_required": True,
            "no_blind_retry": True,
        },
        "existing_coverage": {
            "design_doc": "docs/m3c16-durable-attempt-journal-finam-status-semantics.md",
            "matrix_tests_cover_cancel_409_410": True,
            "raw_path_body_error_broker_order_id_exported": False,
        },
        "trading_boundary": {
            "endpoint_calls_allowed": False,
            "marker_constructible": False,
            "real_post_delete_added": False,
            "real_order_endpoint_enabled": False,
            "place_order_post_allowed": False,
            "cancel_order_delete_allowed": False,
            "command_consumer_connected_to_strategies": False,
            "real_finam_ack_lifecycle_enabled": False,
            "runtime_live_attachment": False,
            "live_ready": False,
            "first_live_micro": False,
            "stop_sltp_bracket": False,
        },
        "forbidden_surface_scan": {
            "status": "Ok" if scan_code == 0 else "Failed",
            "exit_code": scan_code,
            "script_path": "scripts/forbidden_surface_scan.sh",
            "script_sha256": sha256_file(scan_script),
            "stdout": scan_stdout,
        },
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    evidence_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{evidence_sha256}  {output.name}\n"
    )

    print(json.dumps({"output": str(output), "sha256": evidence_sha256}, indent=2))
    return 0 if official_docs_recheck_passed and scan_code == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
