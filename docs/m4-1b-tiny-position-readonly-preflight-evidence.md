# M4-1b tiny position lifecycle no-send preflight evidence

Date: 2026-07-04

## Goal

M4-1b collects real FINAM read-only evidence for a future tiny position
lifecycle test. It does not send broker orders and does not open a position.

M4-1b is the dry operational proof that these future M4-1 prerequisites can be
checked against broker truth:

- account/token binding;
- symbol availability;
- tradability / asset params;
- fresh quote;
- active/unknown/orphan orders = `0`;
- positions = `0`;
- no unexpected blocking broker state;
- command-consumer-to-real-FINAM disabled;
- continuous runtime-live disabled;
- Stop/SLTP/bracket/replace/multi-leg blocked.

## Inputs

M4-1b should include:

- `finam-auth-check` single JSON report;
- `finam-typed-readonly-check` typed DTO/mapper report;
- guarded `finam-limit-cancel-one-shot --pre-actual-gate-only` report used as a
  no-send broker-truth/risk preflight;
- M4-1a accepted no-send runbook evidence.

## No-send boundary

M4-1b must not:

- call FINAM order POST/DELETE;
- place an entry order;
- place an exit order;
- open a position;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime-live;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize market orders.

## Readiness checks

The evidence is ready for review only if:

- auth HTTP = `200`;
- token details HTTP = `200`;
- typed readonly fixture exists and has no failed probe;
- account typed probe maps positions count;
- account orders typed probe maps active orders count;
- asset params typed probe is present;
- last quote typed probe has bid/ask/last source evidence;
- no-send preflight reports `actual_send_allowed = true` but
  `boundary_invocation_performed = false`;
- no-send broker truth is flat and clean.

## Next stage after review

If accepted, the next possible stage is:

`M4-1c tiny position lifecycle actual pre-authorization`

That stage still must not infer permission from M4-1b. It requires fresh explicit
operator approval with entry/exit type, prices, maximum position lifetime and
abort rules.
