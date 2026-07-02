# M3c-21 release-profile evidence

Status: evidence-only slot closure start. This increment does not add or
authorize real FINAM order `POST` / `DELETE`, command consumption, real ACK
lifecycle, runtime/live attachment, `LiveReady`, first live micro, stop/SLTP,
or bracket.

## Goal

M3c-21 closes the first implementation-gate evidence slot:

```text
release_profile_evidence_or_waiver: Pending -> EvidenceProvided
```

The other four slots remain pending:

```text
positive_get_order_evidence_or_waiver = Pending
route_template_recheck = Pending
undocumented_2xx_status_semantics = Pending
cancel_409_410_status_semantics = Pending
```

## Evidence artifact

Use the source-bound release evidence helper after creating a clean handoff
archive:

```bash
python3 scripts/m3c_release_profile_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The helper runs:

```text
cargo build --release -p broker-cli
bash scripts/forbidden_surface_scan.sh
```

and writes:

```text
reports/m3c-order-endpoint-gate/release-profile-evidence.json
reports/m3c-order-endpoint-gate/release-profile-evidence.json.sha256
```

The evidence records source commit, source archive SHA-256, release profile,
built package, cargo/rustc versions, build exit code, binary SHA-256, forbidden
surface scan status/hash, and the closed trading-boundary booleans.

## Design evidence status

For M3c-21, generate the gate report with:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip \
  --release-profile-status evidence-provided
```

Expected slot counts:

```text
evidence_slot_count = 5
evidence_provided_or_waiver_count = 1
evidence_pending_count = 4
```

## Follow-up in M3c-22

M3c-22 closes the route-template recheck slot using source-bound evidence from
the official FINAM REST documentation. The recheck remains design-only: exactly
two route templates, no rendered account/order ids, and no FINAM order endpoint
calls.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
