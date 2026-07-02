# M3b-15 real-readonly pre-run hardening

Status: pre-run hardening for a future controlled FINAM real-readonly contract
probe. M3b-15 still does not authorize FINAM order placement/cancel, real
command consumption, real CommandAck lifecycle, runtime attachment, `LiveReady`,
live micro, stop/SLTP, or bracket behavior.

M3b-16 follow-up binds the approved FINAM base URL into `RunApproved`, adds the
token/account preflight diagnostic, and emits a redacted evidence matrix. See
`docs/m3b16-real-readonly-contract-probe-evidence-gate.md`.

## Exact POST allowlist

`scripts/forbidden_surface_scan.sh` now treats `.post(` as an exact allowlist,
not a file-level allowlist.

Allowed POST usage remains only in `broker-finam` auth/session code:

```text
auth                 -> /v1/sessions
token_details        -> /v1/sessions/details
token_details_typed  -> /v1/sessions/details
```

Any additional `.post(` occurrence, including one added to the same
`broker-finam/src/lib.rs` file, fails the scan unless the allowlist is
explicitly reviewed and updated.

## Transport config bound to RunApproved

`ReqwestFinamRealReadonlyBrokerTruthTransport::try_new(...)` now rejects
configuration that does not match `RealReadonlyBrokerTruthRunApproved`:

```text
config.request_timeout_ms == RunApproved.request_timeout_ms
config.min_request_interval_ms == RunApproved.min_request_interval_ms
```

This moves the timeout/rate-limit invariant from operator-run-only validation
into transport construction.

## Operator entrypoint only

The lower-level contract probe loop is now an internal helper. The public
operator-facing entrypoint remains:

```text
run_finam_real_readonly_operator_contract_probe(...)
```

The forbidden-surface scan asserts that the lower-level helper is not public and
that the operator entrypoint remains present.

## Persistent audit mode policy

`PersistentAuditStore` remains modeled but is not the default real-readonly
pre-run mode. Until a separate persistent-audit hardening ticket is accepted,
controlled real-readonly probes should use:

```text
EphemeralEvidenceStore
```

Persistent mode requires a separate operational policy for:

- WAL/synchronous settings;
- file permissions and umask;
- retention;
- backup/export;
- corruption handling;
- redacted export contract.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
