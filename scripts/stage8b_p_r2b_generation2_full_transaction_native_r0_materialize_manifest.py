#!/usr/bin/env python3
"""Rebind a proof-only transaction skeleton to accepted Generation-2 account identity."""

from __future__ import annotations

import hashlib
import hmac
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STATE = Path("/var/lib/moex-trading/stage8b/r2a5/run-manifest.json")
ACCOUNT = Path("/run/credentials/moex-trading/stage8b/r2a5/account-id")
ACCOUNT_KEY = Path("/run/credentials/moex-trading/stage8b/r2a5/account-binding-keys/generation-2.hex")


def digest_parts(domain: str, parts: list[str]) -> str:
    digest = hashlib.sha256(domain.encode())
    for part in parts:
        encoded = part.encode()
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return digest.hexdigest()


def main() -> None:
    fields = json.loads(STATE.read_text())
    account = ACCOUNT.read_text().strip()
    account_key = bytes.fromhex(ACCOUNT_KEY.read_text().strip())
    mac = hmac.new(account_key, digestmod=hashlib.sha256)
    mac.update(b"moex-stage8b-account-binding-v1\0")
    encoded = account.encode()
    mac.update(len(encoded).to_bytes(4, "big"))
    mac.update(encoded)
    binding = mac.hexdigest()
    fields["account_key_generation_id"] = "2"
    fields["keyed_account_binding_hmac_sha256"] = binding
    endpoint = json.loads((ROOT / "docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json").read_text())
    operation = endpoint["operations"][fields["operation"]]
    fields["endpoint_identity_sha256"] = digest_parts(
        "stage8b-i-r2-endpoint-identity-v1",
        [operation["method"], operation["route_template_id"], binding, fields["endpoint_renderer_sha256"]],
    )
    authority = json.loads((ROOT / "docs/stage-8/stage8b-p-r1b-run-identity-authority.json").read_text())["run_identity"]
    ordered = authority["common_fields_in_exact_order_excluding_run_identity"] + authority[
        "place_fields_in_exact_order" if fields["operation"] == "PLACE" else "cancel_fields_in_exact_order"
    ]
    fields["run_identity_sha256"] = digest_parts(authority["domain_utf8"], [fields[name] for name in ordered])
    STATE.write_text(json.dumps(fields, separators=(",", ":")))


if __name__ == "__main__":
    main()
