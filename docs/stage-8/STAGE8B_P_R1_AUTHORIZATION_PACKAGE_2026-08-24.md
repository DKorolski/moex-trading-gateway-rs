# Stage 8B-P R1 — exact-build authorization package

Status: design-only authorization candidate. `NOT_ISSUED`; Stage 8B-P and
Stage 8B-XE remain closed.

## 1. Purpose and non-authority

R1 converts the accepted Stage 8B prerequisites into one reviewable
authorization contract. It does not select an operation, read a broker account,
issue an operator arm, record a dispatch attempt, enter the transport boundary
or call FINAM order endpoints.

This split is intentional. An exact run cannot be authorized safely until the
operator supplies a local account HMAC/key generation and selects exactly one
PLACE or CANCEL lifecycle. Those values are secrets or time-sensitive run
inputs and must not be invented, stored in Git, or inferred from a prior live
micro. Acceptance of R1 may open only a separate R2 GET-only preflight package.

## 2. Accepted lineage

The predecessor is `main` at
`16a59bca74f94881c70d9fa39bbdf1c357e65f95`, tree
`cc613dbf15858671eb6a0e5ee1435a2bc2b9f172`. GOV-P1 is closed in the
operator-authorized solo mode: PR and strict `rust`/`redis-smoke` checks remain
mandatory, review threads must be resolved, bypass is empty, and force-push and
branch deletion remain blocked.

The only build eligible for later R2/XE consideration is the independently
qualified TLS source archive:

```text
source ref        6cb179509fad97e8be56e31bb930b2a86caefc6a
source tree       4900fd38d741ab24f643acf211e7d1f807d23792
archive SHA-256   1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6
executable SHA    677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06
target            aarch64-apple-darwin
Rust              1.95.0 / 59807616e1fa2540724bfbac14d7976d7e4a3860
```

The legacy `m3j16-actual-one-shot` feature remains disabled for both
`broker-cli` and `finam-gateway`. R1 does not rebuild or execute the binary.
Any production Rust, Cargo/lock, resolved graph, feature, toolchain, config,
policy, instrument, API, renderer, request-body or executable drift discards
the package and requires requalification before a new P package.

## 3. Fresh official FINAM contract

On 2026-08-24 all seven public official FINAM documents were fetched again
without credentials. Every response was HTTP 200 and its byte count and SHA-256
matched the accepted 2026-08-14 baseline and the 2026-08-23 refresh.

The documentation server repeatedly truncated one chunked Python 3.9 response.
The verifier therefore uses system `curl` with HTTP/1.1, connection/request
timeouts and at most three attempts for public documentation GET only. A retry
never targets a broker account or order endpoint and cannot grant authority.
The final complete body still must match the exact accepted bytes and digest.

No FINAM credentials were used. No account GET and no POST/DELETE were sent.

## 4. Future exact run manifest

R2 must receive one complete manifest. Missing, additional, inferred or cached
authority fields fail closed. It binds:

- exact `StrategyRequestId` and durable `ClientOrderId`;
- exactly one operation, PLACE or CANCEL;
- keyed account HMAC and non-secret key-generation ID;
- IMOEXF identity and exact side/quantity/order type/TIF/price or cancel target;
- accepted source archive, executable, config, policy, instrument and API
  identities;
- endpoint renderer and canonical request-body identities;
- current Stage 7B seal, Stage 6 checkpoint and durable budget generation;
- current kill-switch and ownership lease identities;
- opaque one-use arm nonce and bounded issue/expiry timestamps;
- approved target-instrument pre-run position baseline.

Raw account identifiers, credentials and operator HMAC keys remain local and
must not enter Git, logs, CI or handoff archives.

## 5. Frozen operation policy

One run contains one effect only. PLACE is limited to
`IMOEXF@RTSX`, LIMIT, DAY, quantity one. CANCEL requires one exact currently
working order correlated to the same durable lifecycle. A LimitCancel pair is
two effects and is forbidden in one run.

MARKET, Stop, SLTP, bracket, replace, multi-leg, conditional fields, automatic
retry, same-request resend and re-arm remain forbidden.

## 6. R2 GET-only preflight

Acceptance of R1 may open preparation of R2 only after the operator supplies
the operation-specific local inputs. R2 must freshly read and bind:

- the current Stage 7B seal and dispatch-ready Stage 6 command;
- current kill switch, schedule and instrument specification;
- current account and target orders, positions and trades;
- current ownership, ambiguity, orphan and unresolved-lifecycle state;
- the target-instrument pre-run position baseline.

Caller-built or cached broker truth is forbidden. R2 is GET-only and may not
issue the operator arm, record `DispatchAttemptRecorded`, enter transport or
send POST/DELETE. Read-only broker GET itself still requires a separate explicit
operator decision and local read-only credential; R1 grants neither.

## 7. Operator arm

The future arm is opaque, request-keyed, build/account-bound, expiring and
one-use. It is non-Clone, non-Copy, non-serializable, not reconstructible after
restart and cannot be issued twice for the same durable request. This R1 package
does not construct or issue it.

## 8. Promotion and invalidation

R1 acceptance does not authorize R2 automatically. R2 requires a fresh
operator-selected manifest and separate review. Any contract, build, account,
command, schedule, broker-truth, seal, budget, kill-switch, ownership or expiry
drift blocks and discards the candidate; no authority carries forward.

Only after an accepted R2 exact preflight and a fresh explicit operator go may
a separate Stage 8B-XE package be discussed. Even then, XE is at most one
engineering effect and is not runtime-live or strategy-live authorization.

## 9. Closed surfaces

R1 keeps closed:

- broker account GET and all credentials;
- operator-arm issuance and exact run authorization;
- dispatch-attempt recording and transport entry;
- FINAM POST/DELETE and broker effect;
- Redis execution consumer, broker dispatch and runtime-live;
- real strategy orders, Stage 8B-XE, Stage 11 execution promotion and Stage 12.
