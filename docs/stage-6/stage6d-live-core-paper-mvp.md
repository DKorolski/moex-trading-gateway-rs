# Stage 6D — live-core integration and paper MVP

Stage 6D joins the accepted Stage 5 restart/runtime authority to the accepted
Stage 6 durable journal and replay chain. It remains a paper-only integration
boundary: no Redis consumer, FINAM transport, network dispatch, runtime-live,
real orders or native protective orders are enabled.

## Boot and restart authority

- `FirstBoot` requires a linear explicit authorization and may create only an
  empty in-memory journal.
- `Restart` requires an existing framed journal. Missing storage fails before
  restart-package decode and cannot fall back to first boot.
- The HMAC package binds the exact Stage 5G restart bytes, Stage 6 checkpoint
  bytes and operational deployment/gateway identity.
- A journal shorter than the authenticated checkpoint or with an equal-length
  hash mismatch fails closed. A longer journal is accepted only when the
  authenticated checkpoint validates as an exact prefix and full Stage 6C
  replay succeeds.

## Durable-before-effect paper path

The process-local adapter receives a capability only after both records have
been appended and replayed:

```text
RequestAccepted
→ DispatchAttemptRecorded
→ Stage6dPaperDispatchReceipt
→ normalized paper broker truth
→ Stage 6C journal records
```

The explicit fixtures cover MARKET fill, LIMIT working/fill, recovered Place,
authoritative no-order, inconclusive, Cancel canceled, execution-observed,
rejected and already-terminal outcomes. Cancel never accepts generic Place
reconciliation evidence.

## Stage 5 runtime application

Restart broker truth is not written into runtime fields by Stage 6D. The only
application path is:

```text
authenticated Stage 6D operational identity
→ Stage 5G fresh-truth validation
→ Stage 5G restart binding
→ Stage 5G owning reducer
→ Stage 5G authenticated application round-trip
→ restored Stage 5G capability
```

Already represented terminal truth is classified as a no-op by the accepted
Stage 5G boundary. A missing working-order fact is applied once without adding
a second dispatch record to the Stage 6 journal.

## Soak evidence

`Stage6dPaperExecutionReport::to_ndjson_line()` emits compact deterministic
evidence containing request/client IDs, account/instrument/attribution,
action, durable sequences, dispatch safety, broker IDs, cancel outcome,
runtime pre/post fingerprints, journal frontier, integration fingerprint and
restart marker.

The integration fingerprint binds the complete authenticated Stage 5 package
authority, Stage 6 replay fingerprint, Stage 6 checkpoint and recovered
request projections.

## Compatibility and closed surfaces

Stage 6A v1 goldens, Stage 6B storage/backend and Stage 6C replay semantics are
unchanged. The only Stage 5 changes are narrow crate-private HMAC, reviewed
operational-authority and test-fixture adapters required to enter the already
accepted Stage 5G reconciliation path.

Primary gate:

```bash
bash scripts/stage6d_gate.sh
```
