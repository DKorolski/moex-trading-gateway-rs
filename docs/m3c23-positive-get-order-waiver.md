# M3c-23 positive GetOrder evidence / waiver package

Status: waiver-package preparation for the positive GetOrder evidence slot.
This increment does not add or authorize real FINAM order `POST` / `DELETE`,
command consumption, real ACK lifecycle, runtime/live attachment, `LiveReady`,
first live micro, stop/SLTP, or bracket.

## Goal

M3c-23 prepares closure for:

```text
positive_get_order_evidence_or_waiver
```

The preferred closure is controlled read-only positive GetOrder evidence for a
known existing broker order id. If such an id is not available in a safe review
window, the safe alternative is reviewer-accepted waiver.

## Why waiver is prepared first

A real positive GetOrder evidence run requires:

```text
readonly FINAM token
allowed account id
known existing broker order id
instrument symbol
```

The repository and handoff archive intentionally do not store live broker order
ids. Fabricating a positive real GetOrder evidence package would be worse than
leaving the slot pending. Therefore M3c-23 prepares a source-bound waiver
package for reviewer decision.

## Waiver package

After creating a clean handoff archive, generate:

```bash
python3 scripts/m3c_positive_get_order_waiver_package.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

It writes:

```text
reports/m3c-order-endpoint-gate/positive-get-order-waiver.json
reports/m3c-order-endpoint-gate/positive-get-order-waiver.json.sha256
```

The package records source commit, source archive SHA-256, the waiver reason,
existing GetOrder 200 exact/mismatch fixture coverage, forbidden-surface scan
status/hash, and the closed trading-boundary booleans.

## Existing fixture coverage

M3b-24 already covers the real-shape GetOrder 200 mapper boundary:

```text
m3b24_get_order_200_real_shape_fixture_covers_exact_and_mismatch_redacted
```

The fixture covers:

```text
GetOrder -> 200 / identity exact / BrokerOrderIdExact
GetOrder -> 200 / identity mismatch / MismatchedOrderIdentity
```

This is not a substitute for controlled real-readonly evidence unless the
reviewer explicitly accepts the waiver.

## If reviewer accepts the waiver

Regenerate the M3c gate report with:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip \
  --release-profile-status evidence-provided \
  --positive-get-order-status waiver-accepted \
  --route-template-recheck-status evidence-provided
```

Expected slot counts after acceptance:

```text
evidence_slot_count = 5
evidence_provided_or_waiver_count = 3
evidence_pending_count = 2
```

Until reviewer acceptance, the slot should remain `Pending`.

## If reviewer rejects the waiver

Run only the controlled read-only probe with reviewer-approved inputs:

```bash
cargo run -p broker-cli -- finam-real-readonly-evidence \
  --account-id <allowed-account-id> \
  --broker-order-id <known-existing-order-id> \
  --symbol <instrument-symbol> \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The run must remain GET-only, redacted, bounded to at most four requests, and
must not enable POST/DELETE order endpoints.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
