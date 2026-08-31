# Stage 8B-P R2B Implementation Package R0

Status: implementation complete, independent review required, authorization `NOT_ISSUED`.

This package materializes the accepted design at commit
`ebec9a100c92872134f3de91644cec50e2ed073a`. It adds the production
unsigned-package builder, strengthens the existing package signer, and defines
the six fail-closed systemd phase barriers plus the aggregate target.

## Fixed transaction

The transaction contains exactly 31 service invocations:

1. four current-source services;
2. current-manifest issuer and source adapter;
3. eleven authority producers;
4. eleven authority issuers;
5. draft builder followed by package signer;
6. root read-only supervisor.

Every downstream phase has both `Requires=` and `After=` on the preceding
phase. Every phase target has both directives for every service in that phase.
The signer independently requires and follows the builder. The supervisor
independently requires and follows phase 5. Conditions are not used as success
barriers.

All targets use `StopWhenUnneeded=yes`; all relevant units are non-retained
oneshots. Draft and signed outputs are create-new/no-replace. Therefore an old
active target, successful oneshot, unsigned package, or signed package cannot
satisfy a new transaction.

## Builder and signer separation

The root-only builder accepts no arguments and reads only the seven frozen
input classes, including exactly eleven receipt paths. It validates common
nonce, source-generation commitment, freshness, signatures, operator/account
binding, helper authority, and contract snapshot. It has no signing key,
broker credential, or network access and writes only the fixed unsigned path.

The signer reads that fixed draft, revalidates all source bindings and exact
30-second validity, and only then signs it using the sole transient package
authorization credential. It cannot repair or synthesize a different draft.

## Deliberately closed

The source units and targets are not installed, enabled, started, or connected
to another installed target. There is no `[Install]`, preset, timer, path, or
socket activation. No operator is selected, no run nonce or credentials are
materialized, and no run package exists in the repository.

R2B remains `NOT_ISSUED`. FINAM/AuthService/broker GET, POST/DELETE, Redis live
consumer, dispatch, runtime-live, and real orders remain closed.
