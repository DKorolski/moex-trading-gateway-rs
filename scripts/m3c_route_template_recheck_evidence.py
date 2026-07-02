#!/usr/bin/env python3
"""Generate M3c route-template recheck evidence without calling order endpoints."""

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
ROUTE_TEMPLATES = [
    {
        "purpose": "PlaceOrder",
        "http_method": "POST",
        "route_template": "/v1/accounts/{account_id}/orders",
        "docs_method_marker": "POST /v1/accounts/A12345/orders",
    },
    {
        "purpose": "CancelOrder",
        "http_method": "DELETE",
        "route_template": "/v1/accounts/{account_id}/orders/{order_id}",
        "docs_method_marker": "DELETE /v1/accounts/A12345/orders/ORD789012",
    },
]


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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate source-bound M3c route-template recheck evidence."
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the evidence report.",
    )
    parser.add_argument(
        "--output",
        default=Path("reports/m3c-order-endpoint-gate/route-template-recheck-evidence.json"),
        type=Path,
        help="Route-template recheck evidence JSON output path.",
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
    route_results = []
    for route in ROUTE_TEMPLATES:
        route_template = route["route_template"]
        method_marker = route["docs_method_marker"]
        route_results.append(
            {
                "purpose": route["purpose"],
                "http_method": route["http_method"],
                "route_template": route_template,
                "route_template_sha256": sha256_bytes(route_template.encode("utf-8")),
                "official_docs_method_marker_present": method_marker in docs_text,
                "official_docs_route_template_present": route_template in docs_text,
                "rendered_route_exported": False,
                "raw_account_or_order_id_exported": False,
            }
        )

    official_docs_recheck_passed = (
        docs_http_status == 200
        and docs_fetch_error is None
        and all(
            route["official_docs_method_marker_present"]
            and route["official_docs_route_template_present"]
            for route in route_results
        )
    )

    evidence = {
        "m3c_step": "M3c-22",
        "route_template_recheck_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": sha256_file(source_archive),
        "official_docs_url": args.docs_url,
        "official_docs_fetch_transport": docs_fetch_transport,
        "official_docs_http_status": docs_http_status,
        "official_docs_sha256": sha256_bytes(docs_body) if docs_body else None,
        "official_docs_fetch_error": docs_fetch_error,
        "official_docs_recheck_passed": official_docs_recheck_passed,
        "route_count": len(route_results),
        "exact_two_route_allowlist_required": True,
        "routes": route_results,
        "route_templates_exported_as_design_data_only": True,
        "rendered_routes_exported": False,
        "raw_account_or_order_id_exported": False,
        "order_endpoint_calls_allowed_for_recheck": False,
        "endpoint_calls_allowed": False,
        "marker_constructible": False,
        "real_post_delete_added": False,
        "real_order_endpoint_enabled": False,
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
