# M4-2j live-position pre-authorization gate / no-send

Status: no-send pre-authorization gate. This stage does not place or cancel orders.

M4-2i closed the account-trades/orphan-order blocker for the provided read-only window. M4-2j prepares the next tiny live-position test by producing a fresh canonical package that can evaluate the narrow plain-micro stop-order waiver, while keeping the final decision as no-send.

## Scope

M4-2j may perform only FINAM read-only GET calls through `finam-typed-readonly-check`.

It must not:

- call real POST `/orders`;
- call real DELETE `/orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime-live;
- enable Stop/SLTP/bracket/replace/multi-leg;
- open or close a position.

## Required pre-authorization shape

The typed canonical record must include:

- fresh real read-only `BrokerTruthSnapshot`;
- `account_orphan_orders_count = 0`;
- `pre_waiver_canonical_preflight_blocks = ["Readiness(StopOrderUnsupportedBlocked)"]`;
- explicit `plain_micro_stop_waiver_operator_approval_present`;
- `plain_micro_stop_waiver_source = StopOrderNotRequiredForPlainMicro`;
- `stop_order_waiver_applied`;
- `m4_2j_pre_authorization_gate = true`;
- `pre_authorization_evidence_only = true`;
- `actual_send_allowed = false`;
- `order_post_delete_calls_performed = false`;
- `live_order_calls_performed = false`.

If the operator-approved no-send waiver is supplied, the final canonical preflight may become `allowed = true`, but `no_live_authorization` must remain true and the report decision must remain `NoSendPreAuthorizationReady`, not actual execution.

## Operator approval boundary

The CLI flag is:

```text
--plain-micro-stop-waiver-operator-approved-no-send
```

It is an approval artifact for the no-send pre-authorization report only. It is not authorization to place a live order. A separate actual package and explicit operator approval are required before any POST/DELETE order endpoint can be invoked.

## Canonical command shape

The real report should use an explicit account-trades window that covers the relevant filled order history:

```text
cargo run -p broker-cli -- finam-typed-readonly-check \
  --start-time <RFC3339> \
  --end-time <RFC3339> \
  --limit 1000 \
  --plain-micro-stop-waiver-operator-approved-no-send \
  --output reports/m4/m4-2j-live-position-preauth-no-send-report.json
```

## Acceptance

M4-2j is ready for review when:

- source markers and tests are green;
- a fresh real read-only report is attached, if operator no-send approval was granted;
- orphan-order truth remains clean;
- before waiver, the only canonical block is `StopOrderUnsupportedBlocked`;
- after waiver, no live authorization is produced;
- forbidden surface scanners remain green;
- handoff archive is clean.

Live expansion remains blocked after M4-2j.
