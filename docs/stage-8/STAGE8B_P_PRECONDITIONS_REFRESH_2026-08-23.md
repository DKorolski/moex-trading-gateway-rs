# Stage 8B-P preconditions R2 — GOV-P1 corrective review package

Status: candidate for independent review. Stage 8B-P remains closed.

## Scope and accepted predecessor

The exact Stage 8B-IT-TLS R1 candidate
`6cb179509fad97e8be56e31bb930b2a86caefc6a` was independently accepted and
history-preserving fast-forwarded to `main`. The merged tree is exactly
`4900fd38d741ab24f643acf211e7d1f807d23792`; the accepted source handoff is
`moex-trading-project-6cb1795.zip`, SHA-256
`1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6`.

This corrective package retains the independently accepted CONTRACT-P1 and
BUILD-P1 evidence and changes only governance/workflow control-plane files. It
does not change Rust, Cargo manifests, config, endpoint code or any runtime
path. It does not issue an operator arm and cannot call FINAM.

## Preconditions result

| Gate | Current result | Meaning |
|---|---|---|
| CONTRACT-P1 | Accepted in R1 | Seven official FINAM pages were freshly fetched; status, byte count and SHA-256 are identical to the accepted 2026-08-14 snapshot. It must be refreshed again immediately before actual Stage 8B-P. |
| BUILD-P1 | Accepted in R1 | The accepted TLS archive was built twice from clean roots in locked/offline release mode with deterministic path remapping; both executable hashes are identical. No execution-source input changed in R2. |
| GOV-P1 | Ready for independent acceptance | Ruleset 20111805 is active on the default `main` branch, PR review/status checks are enforced, bypass is empty, force-push/deletion are blocked, and canonical CI identities are immutable. |

GOV-P1 is ready for independent acceptance, not self-accepted. Therefore
`all_prerequisites_accepted=false` and Stage 8B-P remains closed. Only an
independent R2 verdict followed by an exact reviewed merge and post-merge
verification can close GOV-P1.

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

## Enforced GOV-P1 controls

The public GitHub API is checked by `stage8b_p_governance_refresh.py`. The
required normalized state is:

```text
default branch         main
main protected         true
ruleset 20111805       active
ruleset target         ~DEFAULT_BRANCH
bypass actors          none
required checks        rust, redis-smoke
```

The ruleset requires one approval, dismisses stale reviews, requires approval
of the last push, requires review-thread resolution and permits only merge
commits. Branch deletion and non-fast-forward updates are blocked. The exact
policy key set is checker-frozen; missing or additional authority keys fail.

Canonical CI is pinned in both jobs to:

- `actions/checkout@11d5960a326750d5838078e36cf38b85af677262`;
- `dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c`;
- Rust `1.95.0`.

Mutable tags and `stable` are rejected by both the current-tree authority and
the R2 negative harness. Any break-glass/bypass use invalidates this candidate
and requires a new governance observation and independent review.

The handoff gate also runs the inherited current-tree replay, current workspace
debug and release tests, doctests, all-feature clippy, no-Redis evidence smoke,
Redis shadow smoke and runtime-bridge dry smoke. Its terminal regression marker
is emitted only after every command succeeds on the exact corrective commit.
The canonical `rust` job runs on the explicit `macos-15` Arm64 image matching
the accepted BUILD-P1 `aarch64-apple-darwin` target and explicitly installs the
`redis-server` test toolchain required by the immutable Stage 7B replay. The
job also fixes `CARGO_TERM_COLOR=never` because the immutable acceptance report
binds exact plain-text Cargo completion markers and cannot parse ANSI-decorated
variants. The
separate Linux Redis smoke remains mandatory. These are test-infrastructure
choices only and do not enable the closed Redis execution consumer.
The inherited current-tree replay has a 120-minute step limit: its nested
debug/release/doctest/clippy qualification exceeded 75 minutes on the shared
Arm64 runner, while the exact same gate completed locally. This changes only
the execution budget; no command, acceptance marker or failure behavior is
relaxed.
If an immutable historical replay fails, the current-tree wrapper emits bounded
fail-only nested diagnostics; this changes observability only and does not add
an allowance, retry or bypass.

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

The only next action authorized by this candidate is independent review of
GOV-P1 R2. It cannot authorize a GET, POST, DELETE or any live run.
