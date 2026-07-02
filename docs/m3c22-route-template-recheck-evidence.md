# M3c-22 route-template recheck evidence

Status: evidence-only slot closure. This increment does not add or authorize
real FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Goal

M3c-22 closes the second implementation-gate evidence slot:

```text
route_template_recheck: Pending -> EvidenceProvided
```

After M3c-22, the expected slot state is:

```text
release_profile_evidence_or_waiver = EvidenceProvided
positive_get_order_evidence_or_waiver = Pending
route_template_recheck = EvidenceProvided
undocumented_2xx_status_semantics = Pending
cancel_409_410_status_semantics = Pending
```

## Evidence artifact

Use the source-bound route-template recheck helper after creating a clean
handoff archive:

```bash
python3 scripts/m3c_route_template_recheck_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The helper fetches the official FINAM REST documentation page and verifies that
the current design-only route templates are present:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

It writes:

```text
reports/m3c-order-endpoint-gate/route-template-recheck-evidence.json
reports/m3c-order-endpoint-gate/route-template-recheck-evidence.json.sha256
```

The evidence records source commit, source archive SHA-256, official docs URL,
official docs SHA-256, route-template hashes, forbidden-surface scan status/hash,
and the closed trading-boundary booleans.

## Design evidence status

For M3c-22, generate the gate report with:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip \
  --release-profile-status evidence-provided \
  --route-template-recheck-status evidence-provided
```

Expected slot counts:

```text
evidence_slot_count = 5
evidence_provided_or_waiver_count = 2
evidence_pending_count = 3
```

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
