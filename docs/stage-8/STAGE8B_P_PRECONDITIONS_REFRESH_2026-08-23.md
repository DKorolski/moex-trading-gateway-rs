# Stage 8B-P preconditions refresh — design-only review package

Status: candidate for independent review. Stage 8B-P remains closed.

## Scope and accepted predecessor

The exact Stage 8B-IT-TLS R1 candidate
`6cb179509fad97e8be56e31bb930b2a86caefc6a` was independently accepted and
history-preserving fast-forwarded to `main`. The merged tree is exactly
`4900fd38d741ab24f643acf211e7d1f807d23792`; the accepted source handoff is
`moex-trading-project-6cb1795.zip`, SHA-256
`1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6`.

This additive package performs only the allowed preconditions refresh. It does
not change Rust, Cargo manifests, workflows, config, endpoint code or any
runtime path. It does not issue an operator arm and cannot call FINAM.

## Preconditions result

| Gate | Current result | Meaning |
|---|---|---|
| CONTRACT-P1 | Ready for independent acceptance | Seven official FINAM pages were freshly fetched; status, byte count and SHA-256 are identical to the accepted 2026-08-14 snapshot. |
| BUILD-P1 | Ready for independent acceptance | The accepted TLS archive was built twice from clean roots in locked/offline release mode with deterministic path remapping; both executable hashes are identical. |
| GOV-P1 | Pending | `main.protected=false`, ruleset 20111805 is disabled and canonical CI still contains mutable action/toolchain references. An exact-review/handoff/tree-identical-merge equivalent is proposed but is not self-accepted here. |

Because GOV-P1 is pending, `all_prerequisites_accepted=false` and Stage 8B-P is
not open. Independent review may accept the documented equivalent mechanism or
require the repository ruleset and immutable CI pins to be enabled in a later,
separately reviewed governance slice.

## Fresh FINAM contract

The public documentation was fetched without credentials on
2026-08-23. The normalized snapshot binds exact HTTP status, byte count and
SHA-256 for REST PLACE/CANCEL, gRPC PlaceOrder/GetOrder and the asset,
asset-params and schedule pages. Every one of the seven responses matches the
accepted Stage 8A-0 baseline byte-for-byte.

The exact production contract remains:

- host `api.finam.ru` over TLS;
- PLACE: `POST /v1/accounts/{account_id}/orders`;
- CANCEL: `DELETE /v1/accounts/{account_id}/orders/{order_id}`;
- initial effect shape: LIMIT, DAY, quantity one, `IMOEXF@RTSX`;
- explicit nonempty `client_order_id`, at most 20 characters;
- no stop, SLTP, legs, replace, multi-leg or automatic retry;
- CANCEL success is an observation, never terminal broker truth;
- ambiguous/transient/error outcomes require reconciliation, never blind resend.

No documentation response body is copied into the repository. Only normalized
facts, public URLs, byte counts and hashes are retained.

## Exact TLS-qualified build identity

The accepted source ZIP was extracted into two different new empty roots under
the explicitly bound macOS `/tmp` parent. Each
build used:

```text
CARGO_NET_OFFLINE=true
CARGO_INCREMENTAL=0
SOURCE_DATE_EPOCH=1787497046
RUSTFLAGS=--remap-path-prefix=<canonical-extracted-root>=/stage8b-source --remap-path-prefix=<extracted-root>=/stage8b-source
cargo build --release --locked -p broker-cli
```

Both builds produced a 11,202,608-byte `aarch64-apple-darwin` executable with
SHA-256:

```text
677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06
```

The source member manifest remained unchanged after each build. Cargo manifests,
Cargo.lock, production/all-feature graphs, exact rustc commit and the accepted
config/policy/instrument/API/renderer/body-schema projections are separately
bound in the build identity. The executable was not invoked.

This evidence proves a deterministic local artifact for review. It does not
make the artifact deployable or execution-authorized.

## Governance observation and proposed equivalent

The public GitHub API reported:

```text
main head              6cb179509fad97e8be56e31bb930b2a86caefc6a
main protected         false
ruleset 20111805       disabled
```

Canonical CI currently refers to `actions/checkout@v4` and
`dtolnay/rust-toolchain@stable`. The public upstream revisions observed for
those refs are recorded, but this package does not silently edit the frozen
workflow and does not claim those mutable references are acceptable.

The proposed equivalent reviewed-change mechanism requires all of:

1. independent review of an exact full commit;
2. immutable source ZIP plus SHA-256 and source manifest;
3. a history-preserving tree-identical merge only after acceptance;
4. post-merge exact head/tree verification;
5. the current-tree and negative gates;
6. no administrator self-acceptance for Stage 8B-P.

Independent acceptance of that mechanism, or activation of reviewed branch
protection with immutable action/toolchain pins, is required to close GOV-P1.

## Explicitly closed surfaces

This package keeps all of the following closed:

- Stage 8B-P and Stage 8B-XE;
- production operator-arm issuance;
- FINAM POST/DELETE and any broker effect;
- Redis execution/live consumer;
- broker dispatch and runtime-live;
- real strategy orders;
- market/protective/replace/multi-leg expansion;
- Stage 12.

The only next action authorized by this candidate is independent review of this
preconditions package. It cannot authorize a GET, POST, DELETE or any live run.
