# Stage 8B-P R2B Issuance Package R0

Status: design candidate, `NOT_ISSUED`.

## Purpose

R0 freezes the complete future R2B start transaction after accepted R4-R2A at
`f24f1044ac0b29c2f588853b817e519cfe8d3d8b`. It does not create an activation
target, issue an operator arm, read credentials, contact FINAM or start a unit.

The future transaction contains 30 service invocations in six ordered phases:

1. publisher, creator, stager and current-source writer;
2. current-manifest issuer and source adapter;
3. eleven fixed authority-producer instances;
4. eleven fixed authority-issuer instances;
5. one no-argument package issuer;
6. one root admission supervisor, which may fexecve only the accepted read-only
   helper after durable nonce admission.

The implementation stage may materialize this graph only through one exact
`moex-stage8b-r2b-issuance.target`. Direct manual start of every constituent
unit remains refused. The target itself must remain refused and uninstalled
until a separately reviewed, local, signed activation authority exists.

## Parser and fixed-input boundary

All nine currently shipped unit templates/files in this graph are checked by a
section-aware allowlist and by `systemd-analyze verify`. Unsupported
`ConditionPathIsRegular=` is absent. Supported path conditions are defense in
depth only; every binary retains authoritative no-follow, regular-file,
ownership, mode, link-count, signature, freshness and semantic validation.

## Fresh public contract

On 2026-08-29 the seven public official FINAM documents bound by the accepted
2026-08-24 snapshot were fetched again. All returned HTTP 200 with byte lengths
and SHA-256 values identical to the accepted snapshot. No credentials, account
GET, order POST or DELETE were used.

## Operator-local data

R0 deliberately does not invent or commit:

- operation (`PLACE` or `CANCEL`);
- strategy request/client-order identity;
- account HMAC or key-generation value;
- operator decision, arm nonce or expiry;
- signed run package;
- current kill-switch, schedule, ownership, broker truth or position baseline.

These are fresh local inputs. Their absence keeps the activation authority and
R2B authorization impossible. Historical live-micro values are not reusable.

## Implementation boundary after design acceptance

The next implementation may add only the exact producer/issuer phase targets,
fixed package-issuer unit and aggregate issuance target described by the machine
contract. It must preserve `RefuseManualStart`, zero parser warnings, no
enablement/install link, exact ordering and `NOT_ISSUED`. Operator-specific
material remains a later local package requiring explicit operator approval and
independent review.

## Closed surfaces

R0 keeps closed FINAM credentials and broker account GET, AuthService, order
POST/DELETE, dispatch attempt recording, transport entry, Redis live consumer,
broker dispatch, runtime-live, strategy-live and real orders.
