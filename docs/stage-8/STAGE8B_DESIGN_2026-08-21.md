# Stage 8B-D R2 — bounded engineering effect design freeze

Status: docs/checker-only corrective design candidate; independent acceptance required.

## 1. Lineage, scope and non-authority

Stage 8A is independently closed at `bf58b47fdef8af774a4107455dfcc6204e594283`.
GOV-CI-1B was independently accepted at
`13f659f368cbb36a2d38c2b0b88efa376f0b690c` and merged to `main` by
`7bc9fdab190e011111b15ebdf2f35ff2263a8e34`. Stage 8B-D R1 at
`b3358ba2268da3db4eb8352c097495ebb85575d7` was not frozen, but its bounded
effect architecture was retained by review.

R2 changes documentation and checkers only. It does not add or enable production
Rust, Cargo features, HTTP transport, FINAM POST/DELETE, Redis XADD/XACK or live
consumption, ACK/readiness publication, broker dispatch, runtime-live, real orders,
operator arming or execution authority. Acceptance may open only the separately
reviewed Stage 8B-S implementation specification.

## 2. Mandatory phase order

Every transition is independently reviewed:

1. `8B-D` — this corrective design freeze;
2. `8B-S` — exact API/type-state and implementation specification;
3. `8B-I` — no-send implementation and deterministic crash/replay rehearsal;
4. `8B-P` — fresh GET-only preflight and one exact run authorization;
5. `8B-X` — at most one engineering broker effect and explicit closure.

No phase implicitly opens the next. `8B-X` is transport engineering evidence, not
strategy-live readiness and not Stage 12 authorization.

## 3. Exact max-one run contract

A future immutable run contract selects exactly one command: either `PLACE` or
`CANCEL`, never both. There is no automatic follow-up. A LimitCancel pair is two
broker effects and is outside one run.

It binds one StrategyRequestId, durable ClientOrderId, accepted Stage 6/7 command,
operator arm, process boot, keyed account binding, IMOEXF/IMOEXF@RTSX instrument,
action, side, quantity, order type, TIF, price/notional or exact cancel target,
build/config/policy/endpoint/body identities, trusted time and expiry, kill-switch
generation, ownership lease and durable micro-budget generation.

PLACE is `LIMIT`, `DAY`, maximum one lot. MARKET, Stop, SLTP, bracket, replace,
multi-leg, conditional and native protective effects stay closed. CANCEL targets one
exact currently working order already correlated to the same durable lifecycle.

## 4. BUILD-MANIFEST contract

Execution authority is valid only for an execution-qualified immutable build manifest.
The manifest must contain and hash-bind:

- exact source commit and source archive SHA-256;
- Cargo.lock SHA-256 and every workspace/package Cargo.toml SHA-256;
- exact `rustc -Vv`, rustc release/commit/commit-date/host/LLVM fields;
- exact Cargo version, toolchain channel and target triple;
- cargo metadata dependency graph SHA-256;
- complete package/target/profile/feature set, including default features;
- final executable SHA-256 and executable target name;
- runtime config, policy, instrument contract and API contract snapshot SHA-256;
- endpoint renderer and canonical request-body SHA-256;
- build-manifest schema/version and deterministic aggregate SHA-256.

The production feature set must explicitly record `m3j16-actual-one-shot = false` for
both broker-cli and finam-gateway. Missing, unknown or enabled legacy actual-send
features make the build non-authorizable. A binary hash without its source,
toolchain, dependency graph and features is insufficient. GitHub Actions and the
execution-qualified Rust toolchain must be pinned by immutable revision/version
before protected execution evidence is accepted.

No alternate real transport constructor or reachable send path may exist outside the
Stage 8B capability chain. This is proved in Stage 8B-S/I by compile-fail, feature
matrix and reachable-call-graph evidence; R2 does not implement that path.

## 5. Keyed ACCOUNT-BINDING contract

Raw live account identifiers, credentials and operator HMAC keys never enter Git,
CI output, logs, review packages or handoff archives. Plain unkeyed SHA-256 is not an
account privacy mechanism and is forbidden.

The binding is:

```text
HMAC-SHA256(
  operator_secret_key,
  "moex-stage8b-account-binding-v1\0" ||
  u32be(len(canonical_account_id_utf8)) ||
  canonical_account_id_utf8
)
```

The operator key is random, at least 256 bits, stored outside the repository and
identified only by an opaque non-secret key generation ID. Canonical account bytes
are exact UTF-8 supplied by the trusted local operator manifest; whitespace trimming,
case folding and lossy normalization are forbidden. Verification is local and
constant-time. The run contract binds the HMAC value and key generation ID. Missing
key, wrong generation, malformed binding, account mismatch or attempted fallback to
plain digest blocks before attempt recording.

## 6. Operator arm and durable max-one authority

The arm is opaque, request-keyed, build-bound, account-binding-bound, expiring and
one-use. It cannot be cloned, serialized, default-constructed, reconstructed after
restart or minted twice for the same durable request. It is issued only from an
independently accepted 8B-P package.

The persistent budget is consumed before possible send and covered by the Stage 7B
seal. Restart reconstructs observation/reconciliation authority only. Changed body,
endpoint, build manifest, feature set, account binding, command or config blocks.

## 7. Fresh GET-only preflight

Trusted production issuers must reread current Stage 7B seal and dispatch-ready Stage
6 command, Stage 8A root/config/policy/current control, composite readiness,
single-broker ownership, kill switch, schedule, instrument specification, trusted
clock, account, target/account orders, positions, trades, ambiguity, unresolved
lifecycle and consumed budget.

Caller-built snapshots, cached readiness, stale broker truth, missing schedule,
unreadable control, non-RunAllowed kill switch, ownership conflict, unknown/orphan,
unresolved lifecycle, account-binding mismatch, build/config/API drift or enabled
legacy feature block before attempt recording. Preflight is GET/read-only and the
transport cannot silently refresh authority inputs.

## 8. Durable attempt-before-send ordering

The only legal order is:

```text
fresh preflight
  -> consume arm and durable budget
  -> append exact DispatchAttemptRecorded
  -> fsync journal
  -> authenticate covering Stage 7B seal
  -> reread kill switch, expiry and immutable run contract
  -> enter exact transport boundary at most once
```

Redis state is never execution authority. Append/fsync/seal/revalidation failure is
definitely no send but closes the consumed arm; it never grants automatic retry.

## 9. Crash, ambiguity and reconciliation

Before durable attempt there is no send authority. A committed attempt before
transport consumes authority without retry. Once transport may have been entered,
timeout, disconnect, partial write, response loss, malformed/truncated 2xx, 2xx
without broker order ID, 429 or 5xx is `OutcomeUnknown` unless the transport proves
zero bytes/effect could have crossed the boundary. Ambiguous requests are never
automatically resent with the same or a new identity.

Fresh exact order, scoped orders/trades and instrument-scoped positions are normalized
through accepted reconciliation semantics. Empty, missing, stale or account-wide row
counts do not prove absence or flat. Broker truth cannot rewrite durable identity.
Multiple matches, lifecycle regression or cross-scope evidence are conflicts.

## 10. SAFE-CLOSURE state machine

Every 8B-X run reaches exactly one durable classification:

- `Stage8BClosedSafe`;
- `ResidualWorkingOrder`;
- `ResidualPosition`;
- `OutcomeUnknown`;
- `BrokerTruthConflict`.

`Stage8BClosedSafe` is legal only when all predicates are proven from fresh truth:

```text
ambiguity_count == 0
unknown_orphan_count == 0
active_target_orders == 0
target_lifecycle_is_exact_terminal == true
target_position == approved_pre_run_baseline
account_safety_guard == clean
journal_seal_outcome_consistent == true
operator_signoff == present
```

No other state is accepted closure. A working order requires broker-native manual
intervention or a new separately reviewed CANCEL run; automatic second command is
forbidden. A residual position requires the pre-approved manual emergency disposition.
Manual action does not synthesize safe closure: a new fresh GET-only reconciliation,
durable evidence and signoff must prove the predicate. While any residual/unknown/
conflict exists, Stage 8B stays blocked and no new arm can be issued.

## 11. Kill switch, ownership and network boundary

The persistent kill switch is fresh, readable and exactly RunAllowed before arm,
before attempt, after seal, immediately before transport and before continuation.
After possible send, stop means reconcile, not assume absence. FINAM is the sole
execution owner; ALOR may be a read-only oracle only.

Future transport accepts only non-serializable approved request parts. It binds exact
TLS host `api.finam.ru`, method and rendered route for PLACE or CANCEL. Redirects,
proxies, alternate hosts, arbitrary URLs/methods/headers/routes and automatic HTTP
retry are forbidden. Secrets and raw responses follow redaction policy.

## 12. Stage 11 hard paper-parity promotion gate

Stage 8B-X does not authorize Stage 12. Before Stage 12, an independently accepted
Stage 11 package must prove at least three complete active IMOEXF MOEX sessions on
consecutive trading days after the last blocking semantic/runtime/config fix, plus one
separate controlled restart/reconnect/gap-recovery qualification.

ALOR remains the sole execution owner/oracle. FINAM runs the same source-oracle Hybrid
strategy in paper/shadow with POST/DELETE and broker dispatch disabled. The acceptance
config explicitly enables strategy invocation, explicitly disables real send and binds
strategy/profile/quantity/order-style/input policy by hash; semantic CLI overrides are
forbidden.

Each normal session requires the same final M10 decision boundaries, zero unexplained
blocking decision divergences, no missing blocking bars, no unknown/orphan or unresolved
lifecycle at close, converged paper order/position/runtime/riskgate state and complete
hash-bound evidence. A blocking fix resets the clean-session counter. Recovery
qualification does not replace a normal session. Characterization thresholds are
frozen before the three-session acceptance series and cannot be widened post hoc.

## 13. Reachable-action coverage before strategy live

Before Stage 12, a frozen accepted IMOEXF live configuration and exact source oracle
must produce a machine-readable reachable-action inventory. Promotion requires:

```text
reachable_actions(frozen_imoexf_config)
  subset_of
independently_accepted_finam_execution_capabilities
```

Each reachable action includes order type, side, TIF, cancel/replace behavior,
protective role, quantity policy and lifecycle transitions. MARKET, CANCEL and the
required protective subset must be independently qualified if reachable. The Stage 8B
LIMIT engineering effect alone does not qualify MARKET or protective semantics.

Silent MARKET-to-LIMIT conversion, quantity rewriting or dropping protective actions
at the execution boundary is forbidden. If the final live config differs from the
three-session paper config, Stage 11 qualification repeats for the exact final config.
Until inclusion is complete, Stage 12 and full strategy live remain blocked.

## 14. Governance and evidence boundaries

Before Stage 8B-P, `main` must have branch protection or an equivalent external rule
requiring the accepted authoritative CI and reviewed change path. Force-push and
unreviewed direct execution-authority promotion are forbidden. This is not authority to
change repository settings in R2; it is a future promotion prerequisite.

Every later package binds source/archive/build/features/toolchain/config/policy/
instrument/API/endpoint/body, keyed account binding, session/run identity, command and
lifecycle counts, closure state and signoff. Raw secrets and raw account IDs are absent.

## 15. Closed surfaces and next transition

R2 keeps closed: Stage 8B-S implementation; Stage 8B execution; FINAM POST/DELETE;
Redis XADD/XACK/live consumer; ACK/readiness publication; broker dispatch; retry,
resend or re-arm; runtime-live; real orders; autonomous strategy attachment;
MARKET/Stop/SLTP/bracket/replace/multi-leg; unattended or repeated send; Stage 12.

Independent R2 acceptance may authorize only Stage 8B-S specification work. A real
request remains forbidden until a later exact 8B-X package, explicit operator arm and
all preceding independent gates. Strategy-driven live remains forbidden until Stage 11
and reachable-action coverage are independently accepted.
