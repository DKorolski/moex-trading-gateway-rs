# Stage 8B-P1-e R1 deployable paper supervisor design correction

Status: R1 design-only review candidate. Source implementation is not
authorized until independent acceptance of this exact R1 package.

Accepted immutable predecessor: `c2a9e1246dfdd59f3a6297268de907dedcb19903`
(Stage 8B-P1-d4 governance closure R1).

Accepted predecessor review SHA-256:
`93180e3f633c256ab9bb2cdfa43dd63c3fe7a1b7380eac435feae45cf8969142`.

R0 review SHA-256:
`52a4fb8e4dcaada0ba256c15351895744bb5d483111c1ac95505de119450fbf0`.

The R0 direction remains valid, but R0 commit `7a186cd2ac78a57eff1ad8f24aa52b9dab82b68b`
is `HOLD / SUPERSEDED BY R1`. This document is the normative correction. If
R0 and R1 differ, R1 wins.

## 1. Preserved boundary

R1 preserves the accepted R0 decisions:

- dedicated `stage8b-p1-paper-supervisor` binary in
  `runtime-durable-service`, never a `broker-cli` subcommand;
- no `broker-finam`, `finam-gateway`, HTTP client or FINAM token dependency;
- `validate-config`, explicit one-shot `bootstrap` and restart-only `run`;
- fixed `stage8b-p1-lifecycle.key` through systemd `CREDENTIALS_DIRECTORY`;
- authenticated local durable restart before any Redis contact;
- one task owns the mutable lifecycle owner and commitment key;
- paper-only health/readiness with no serializable `LiveReady`;
- systemd is the sole process restart authority;
- existing P0 units remain byte-for-byte unchanged;
- P1-f owns provisioning and isolated operational activation.

This R1 is documentation, frozen data contracts, checkers and evidence only.
It changes no Rust, Cargo, workflow, service unit or operational config. DB0,
VPS activation, FINAM transport/POST/DELETE, broker dispatch, runtime-live,
real orders, partial fills and protective orders remain closed.

## 2. Canonical contract encoding

The five JSON contracts use one canonical projection for their normative
hashes: parse JSON with duplicate-key rejection, recursively sort object keys,
preserve array order, encode compact UTF-8 JSON without insignificant
whitespace, and append no newline. Integers remain integers and semantic
decimal values are strings. Unknown fields are rejected by the future typed
decoders.

| Contract | Canonical SHA-256 |
| --- | --- |
| `Stage8bP1RuntimeProfileV1` | `dd5a211e708db0d40175d19ed1eeb51db26497d344a553b41d7afbfdddde0ef6` |
| `Stage8bP1FirstBootSourceBundleV1` schema | `aed9219ad0c7e79bc860e7e24d18229c5680213ebb183f4a5ed6e3f4774e2e21` |
| `Stage8bP1FirstBootSourcePlanV1` | `e6152e46cd5b49414372f51681cd2194faf17cb2862301aaaba7a5d257ff637c` |
| `Stage8bP1RedisDeploymentManifestV1` template | `080050c53485cf08c86d1055f6ce071d84077baa121440e6e9051d3960ee5d82` |
| telemetry schema/redaction contract | `d2161a02e982a0e5e95b13596d6632a4368b8d74787c3376d5ffa0d11ef120f5` |

The restart matrix file SHA-256 is
`c64a14ad19f40d4ff5964c359b9131b27dc47917404ac283e4301f36dfddf04a`.
The supervisor event matrix file SHA-256 is
`37c4f423f80ebba281964f571511ff23c74f3a5b2355b4e7afb78d36d880bcae`.
The R1 checker recomputes every value; prose alone is not authority.

## 3. Production-reachable runtime profile

`docs/stage-8/stage8b-p1e-runtime-profile-v1.json` is the full semantic input
to the future non-generic `Stage8bP1RuntimeProfileV1` constructor. It freezes
all fields of `HybridIntradayRuntimeConfig`, including values that are inactive
under High180 but still contribute to the accepted Stage 5D config
fingerprint. Decimal strings must be parsed exactly to finite `f64`; no caller
override, environment merge, default substitution or profile inheritance is
allowed.

The implementation may add exactly one narrow production constructor:

```text
Stage8bP1RuntimeProfileV1::build_hybrid_runtime()
  -> (HybridIntradayRuntimeStrategy, RuntimeConfigFingerprint)
```

It embeds the reviewed profile bytes, verifies their canonical hash, maps every
field explicitly, constructs the runtime, obtains the real
`stage5c_config_fingerprint()`, and compares it to the bootstrap config before
durable-root creation or Redis contact. The bootstrap config is extended with
exact `runtime_profile_id`, `runtime_profile_sha256` and
`first_boot_source_bundle_sha256`; all are mandatory.

The production binary dependency graph and compiled release strings are
scanned to reject `stage8b_p1_test_first_boot_material`,
`stage5g-artifact-fixtures`, `stage8b-p1-test-fixtures`, `cfg(test)` source
selection and any dependency on `finam-gateway`. Test code may feed a
production-schema bundle to the same production facade, but no fixture helper
may be linked into or selected by the release binary.

## 4. Exact production first boot

`Stage8bP1FirstBootSourcePlanV1` and the accompanying JSON schema define the
only production path to the tuple required by `first_boot_stage8b_p1`.
`bootstrap` accepts no source path argument. It reads exactly:

```text
/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json
```

The regular single-link root-owned source file is SHA-256 bound by config. It
contains observations, not a serialized strategy: one complete read-only
broker-neutral flat snapshot, canonical final History M10 bars, the three raw
fields of each source-produced riskgate session outcome (`session_date`,
`shadow_pnl_points`, `shadow_trade_count`) and one exact Replay candidate M10.
Runtime state JSON, materialized riskgate state, derived ledger fields, an
external riskgate ledger, P0 state, ALOR snapshot and caller-supplied Stage5G
package are forbidden.

`warmup_stage5c_history` / `Strategy::warmup_from_history` is explicitly not a
riskgate-ledger source: the accepted implementation warms price-dependent
strategy state but does not finalize shadow sessions. The future production
`Stage8bP1RiskGateHistoryOracleV1::rebuild` therefore replays the exact same
canonical history through the same source-owned High180 shadow kernel used by
`HybridIntradayRuntimeStrategy`; it is default-feature production code, not a
fixture seam. It emits only the three observational fields above. The facade
recomputes and compares both the canonical history-bars hash and the canonical
observation-list hash, then requires byte-exact equality between oracle output
and the authenticated observations. The candidate must belong to a Moscow
session strictly after the history-tail session, proving that every history
session admitted to this oracle is complete.

The future `build_stage8b_p1_first_boot_source_v1` facade performs this exact
linear workflow with no network or Redis:

```text
F00 authenticate source file metadata, SHA-256, schema and deployment binding
F01 construct Stage8bP1RuntimeProfileV1 and verify actual runtime fingerprint
F02 build and validate the exact Stage 4 broker-neutral flat truth admission
F03 prepare clean state and invoke one bootstrap notification
F04 warm up canonical History M10 through accepted Stage 5C facades
F05 rebuild riskgate session outcomes with Stage8bP1RiskGateHistoryOracleV1
F06 byte-match the rebuilt outcomes and both hashes to the source observations
F07 derive every row/record field through accepted source riskgate algorithms
F08 validate the derived evidence through stage5d_validate_riskgate_ledger_evidence
F09 apply only the rebuilt RiskGateRuntimeState to the warmed runtime
F10 prove empty bounded pending recovery for ack/order/stop-order/position
F11 process exactly the first candidate after history as Replay M10
F12 require exactly one callback, zero intents and zero requests
F13 derive current-shadow/private state and final evidence from that runtime
F14 derive Stage5gCleanRestartExportInput from the source plan
F15 export and restore the authenticated Stage5G package into a second runtime
F16 return source, export input and fresh runtime to first_boot_stage8b_p1
F17 create the identity-derived durable root only inside first_boot_stage8b_p1
```

History requires at least 121 complete Moscow sessions, accepted
`FinamDerivedM1ToM10` provenance, final aligned M10 bars, proven aggregation
and no gaps. History callbacks use `HistorySim` and cannot publish operational
commands. Riskgate is independently rebuilt from that exact history with at
least 120 finalized sessions. For each observed tuple the facade calls
`build_runtime_session_row`, assigns only source `Seed` and status `Complete`,
then calls `build_ledger_records_from_rows`; rolling sums, MR flags, identity,
generation, tail hash and materialized projection never come from the bundle.
The one-shot import uses `BootstrapFromSeed`; the frozen runtime remains
`NormalAppend` after the initial durable package exists. This transition is
derived by `plan_risk_gate_startup`, not by a supervisor-local state builder.
The complete result
must pass `stage5d_validate_riskgate_ledger_evidence` before it can be applied
or exported. The candidate must be the first M10 after history. If an outcome,
hash, derived field, chronology or candidate result differs, or the candidate
emits an intent, bootstrap fails before root creation; an operator must create
a new generation of source observations rather than edit state.

All lifecycle timestamps come from the authenticated source bundle as defined
by the plan; wall time may only reject a future or older-than-300-second broker
truth snapshot. The snapshot ID and watermarks are derived exactly by the
plan. Caller-selected snapshot IDs, revisions, write generations, clocks or
riskgate values are rejected.

Release/default-feature acceptance must execute the real binary twice:

```text
empty canonical parent
  -> bootstrap CONFIG CREATE_NEW_STAGE8B_P1_DURABLE_ROOT
  -> source-produced initial TimerReady and one durable root
  -> run CONFIG with the same credential/profile/source identity
  -> authenticated restart succeeds
```

Wrong profile bytes, source hash, account, instrument, chronology, warmup,
riskgate observation/hash/oracle result, nonzero intent, state override or test
feature fails before the root exists.

### Administrative bootstrap unit

Implementation owns a separate exact one-shot unit
`moex-finam-p1-paper-bootstrap.service`:

```text
[Service]
Type=oneshot
User=moex-p1-paper
Group=moex-p1-paper
UMask=0077
LoadCredential=stage8b-p1-lifecycle.key:/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key
ExecStart=/usr/local/libexec/moex/stage8b-p1-paper-supervisor bootstrap /etc/moex-finam-p1-paper/supervisor.json CREATE_NEW_STAGE8B_P1_DURABLE_ROOT
RestrictAddressFamilies=AF_UNIX
PrivateNetwork=yes
ReadOnlyPaths=/etc/moex-finam-p1-paper
ReadWritePaths=/var/lib/moex-finam-p1-paper
RemainAfterExit=no
```

The ordinary service cannot invoke bootstrap. The bootstrap unit cannot attach
Redis and is disabled after the one accepted successful run. P1-f owns its
installation/invocation and the credential/source-file ceremony.

## 5. Restart and recovery-drain matrix

The 22-row `Stage8bP1eRestartContinuationMatrixV1` is exhaustive against every
current `Stage7bRestartOutcome` variant. The implementation must use an
exhaustive Rust match with no wildcard arm and a checker comparing enum variant
names to matrix keys.

Startup is corrected to:

```text
S00 parse argv and exact config
S01 load and validate systemd credential
S02 construct exact profile and compare actual runtime fingerprint
S03 authenticated restart of the identity-derived local durable root
S04 classify one exact Stage7bRestartOutcome
S05 verify the pre-provisioned Redis deployment manifest and inventory
S06 perform bounded stale-PEL claim scan; no fresh read
S06R execute the matrix continuation to quiescent or retained-source boundary
S07 publish Degraded/Starting through pre-existing telemetry stream
S08 perform one successful bounded fresh poll only when S06R permits it
S09 publish PaperReady and enter the one-owner loop
```

Pre-evidence/dispatch-only phases may reacquire only the exact deterministic
authority named by the matrix. In particular:

- P1-d3 LIMIT and CANCEL dispatch reissue only equivalent schedule-step
  authority;
- P1-d3 initial expiry reissues only equivalent day-expiry authority;
- generated-Market prepublication reissues only its exact schedule step.

Every post-evidence phase forbids provider, schedule or callback authority
reacquisition and uses only its named accepted resume function. `Blocked` and
`Stage8a4I3Pending` have no generic fallback, never attach Redis and exit
nonzero. `PaperReady` and fresh polling are impossible before successful S06R.

## 6. Redis deployment identity and verify-only attach

P1-f must provision a concrete instance of
`Stage8bP1RedisDeploymentManifestV1` at the exact string key:

```text
finam_imoexf_paper:{finam-imoexf-p1}:deployment-manifest
```

The template freezes DB15, all stream names/types/groups, the settlement key
prefix, instrument-map fingerprint and namespace digest
`18efd270fb03fa68f92b8288968f6cf16e8f529330c4335cb86cdaaba0b826ed`.
P1-f replaces only the explicitly config-bound digest/generation placeholders,
canonicalizes the concrete manifest, writes it before activation and records
its SHA-256 in supervisor config. P1-e `run` performs `GET`, canonical hash and
field comparison before any claim, read or telemetry write.

The only accepted URLs are literal:

```text
redis://127.0.0.1:6379/15
redis://[::1]:6379/15
```

Scheme case changes, host aliases, omitted/alternate ports, DB0, leading-zero
database components, paths beyond `/15`, userinfo, password, query, fragment,
percent encoding and post-parse normalization are rejected before connection.

Run verifies the manifest key type, every exact stream type and exact expected
group set. Missing/wrong-type keys, extra groups, identity/fingerprint/
generation drift or manifest hash drift are deployment failure. Run never
calls `XGROUP CREATE`, `MKSTREAM`, stream initialization, manifest write,
repair or rename.

Health and readiness streams are pre-provisioned by P1-f. The only telemetry
mutation is exact:

```text
XADD <fixed-key> NOMKSTREAM MAXLEN = 4096 * payload <canonical-json>
```

Missing telemetry streams therefore cannot be created implicitly.

## 7. PEL and consumer hygiene

The future ordinary unit exact policy is:

```text
Restart=on-failure
RestartSec=5s
StartLimitIntervalSec=300
StartLimitBurst=5
KillSignal=SIGTERM
KillMode=control-group
TimeoutStopSec=100s
FinalKillSignal=SIGKILL
SendSIGKILL=yes
```

S06 uses 12 attempts over at most 60 seconds with exponential delays
`250ms, 500ms, 1s, 2s, 4s, 5s` capped at 5 seconds. A pending entry below the
claim-idle threshold keeps readiness Degraded and forbids `XREADGROUP >`.
Expiry of the 60-second startup budget exits 67; it never reads fresh work.

Each process gets one consumer name
`p1e-<deployment-generation>-<boot-id>`. At most 16 stale consumers are
examined per boot. `XGROUP DELCONSUMER` is permitted only when the exact
consumer has Redis-reported pending count zero and idle time at least 24 hours.
A nonzero-pending consumer is never deleted; pending ownership is transferred
only by accepted `XAUTOCLAIM`. The gate uses repeated short restarts, pending
below/above claim-idle and more-than-16 stale consumers; no high-load benchmark
is required.

## 8. Finite owner/event state machine

The 24-row `Stage8bP1eSupervisorEventMatrixV1` is normative. Every coordinator
event maps owner availability and phase to exactly one next effect, exit code,
readiness phase, PEL disposition and restart authority.

Closed process exit classes are:

```text
0  clean operator shutdown at an authenticated boundary
64 config or credential rejection before effects
66 Redis deployment identity/type/group rejection
67 Redis claim/read lifecycle failure
70 owner panic or invalid owner return
71 telemetry task/write failure
72 shutdown grace exhausted
73 signal task failure
```

An owner panic or return without owner destroys linear drain authority. The
coordinator must not fabricate, reconstruct or continue an owner in the same
process: it removes readiness, exits 70 and leaves P1-d4 restart as the only
continuation. A typed return carrying an owner may perform only the matrix's
bounded phase-authorized drain and still exits nonzero if the return was
unexpected. Telemetry or signal-task failure cannot acquire the owner; the
live owner task may drain to its next covering seal, then the process exits its
closed failure code.

Redis acquisition is linearized twice:

```text
check shutdown latch
issue bounded XREADGROUP
receive response
check shutdown latch again before parse/callback/provider/schedule
```

If the latch changes while the read is in flight, the returned entry remains
pending exactly once. The stopping process performs no semantic processing,
provider call, schedule issue or callback; P1-d4 restart reclaims it. A second
signal records only a diagnostic and never changes durability or XACK rules.

## 9. Exact telemetry, redaction and reproducibility

`stage8b-p1e-telemetry-contract-v1.json` fixes field order, canonical bytes,
health/readiness enums, failure/reason enums, redaction and retention. Raw
account IDs, Redis URLs, paths, credentials, tokens, command payloads and raw
error strings cannot enter telemetry. Account/request hashes use the exact
domain-separated transforms in that contract.

Two-run comparison may remove only:

```text
observation_ts_utc
boot_id
consumer_name
```

PID, port, temporary path and wall duration belong only to external harness
metadata and are removed there. Durable identity, generations, seal/commitment,
ordering, phases, failure classes, PEL counts and blocked hashes are never
normalized. After the three allowed snapshot fields and external harness
metadata are removed, two clean runs must have byte-identical canonical
semantic evidence.

## 10. R1 implementation acceptance gate

Acceptance of this design may open one P1-e source slice. That slice must prove:

1. release/default-feature first boot and restart through the production
   profile/source facade, including wrong-input pre-root negatives;
2. release dependency/string scanning for forbidden test helpers/features;
3. exhaustive enum-to-restart-matrix correspondence and all 22 continuations;
4. exact manifest/URL/key/type/group verification and no-create telemetry;
5. all 24 supervisor event rows, including real child panic/lost-owner and the
   signal/XREADGROUP race;
6. exact telemetry canonicalization/redaction/two-run normalization;
7. repeated restart, unclaimable PEL and bounded stale-consumer hygiene;
8. exact one-shot bootstrap and ordinary systemd unit hardening;
9. inherited P1-d4, workspace debug/release, doctest, strict clippy, no-Redis
   and isolated-Redis gates.

Targeted negative mutations must independently detect fixture-linked bootstrap,
runtime-profile defaulting, root-before-validation, post-evidence authority
reissue, omitted restart variant, Redis alias/DB drift, manifest mismatch,
implicit telemetry creation, lost-owner drain, missing post-read latch check,
overbroad normalization, early XACK, fresh read with unclaimable PEL and unsafe
consumer deletion.

The previously deferred bound `0 < child_pid <= u32::MAX` remains a separate
nonblocking P2. R1 does not silently edit accepted P1-d4 production code to
close it.

Independent acceptance of R1 authorizes only P1-e source implementation.
Only a later independent source acceptance may open P1-f isolated operational
acceptance. No P1-e result authorizes installation, startup or DB0/VPS use.
