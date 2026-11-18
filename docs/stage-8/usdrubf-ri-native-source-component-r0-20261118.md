# USD/RI native source component R0 — source review boundary

Baseline: accepted `cb73c833b9e6d56c08cf4232a08fc0dbd08a7b23`, numerical-context preparation only.

## Delivered in this slice

Two explicitly selected market-data identities (`USDRUBF@RTSX`, asset 3447194,
tick 0.01; `RIZ6@RTSX`, asset 5436517, tick 10) feed two independent native M10
processors and one serialized numerical SQLite writer. Each input reaches exactly
its two policy arms. IMOEXF receives no new data. Profiles, numerical kernels,
fees, Cargo manifests and lockfiles remain byte-identical to the baseline.

The additional GET-only client permits metadata, params, schedule and M1 only.
It rejects account/orders routes. Legacy client/config/entry remain IMOEXF-only.
The shared WS driver has an explicit instrument selector, with the same auth
grant, lease, heartbeat, bounded ingress and physical-owner checks. Numeric JSON
normalization does not grant physical subscription authority.

`primary::additional_models::Prepared::admit` checks the exact USD/RI manifest,
four READY contexts, fresh full metadata/params responses, selected account hash,
asset ID, currency, scaled price step, reference lot, trade lot 1 and exact RI
expiry. Reference lot 1000 for USD is NOT an order quantity. No automatic rollover.
Full params/asset bodies are consumed in memory, not included in public logs;
public projections retain original response hashes and request/receive clocks.
This is not a replacement for auth admission in the future managed entry.

Each independent retained raw schedule must cover its ordinary 101-M10 model
day and identify the previous actual raw session. Unknown/short/conflicting days
fail admission. Historical data keeps its original acquisition clocks. No
timestamp retiming, invented M1 or silent position/context reset.

## Publication and failure boundary

1. Native physical-owner and pending-capability validation.
2. Durable source-control publication intent, including exact canonical M1.
3. One numerical SQL transaction (input, two outcomes, checkpoints).
4. Checked readback of exact input/revision/result IDs through the existing
   journal API (which cross-validates the committed numerical state).
5. Durable checked-readback witness; recheck STOP/deadline.
6. Native source advancement LAST. No required consumer/IO follows it.

Mandatory timely admitted REST for sparse/startup/terminal exceptions retains
`REST_EXCEPTION` provenance. It is not relabelled as WS. Existing V8 admission
and U=100 / 2000-ms observed budget are not relaxed. Numeric availability clocks
remain explicitly DECLARED, not an upstream fsync or measured latency claim.

Any error latches the shared STOP and holds the component. Already durable SQL
is retained even if source advancement is refused after STOP/expiry. Creation
requires a new directory. No automatic live resume/adoption of an ahead-of-source
prefix, checkpoint migration or destructive reset is implemented.

The read-only reader uses existing `Journal::inspect`, validates the exact SQL
prefix, two outcomes per input, canonical M1 aggregation/provenance, control hash
chain, independent source frontiers and terminal component snapshot. It does not
reconstruct an `AdmittedCanonicalM10` from JSON. A held/incomplete prefix is not
reported as successful operational execution. Inspection does not change SQL.

Explicit file-only CLI (models-bridge feature required):

```
finam-signal-shadow verify-usd-ri-source-component OUTPUT_DIRECTORY
```

Exit 0 means COMPONENT_PREFIX_VERIFIED, not a clean trading session; exit 2 is
held component evidence. Invalid/incomplete evidence is refused. The old
`observe-ws-primary-models` command still cannot start USD/RI.

## Qualification and non-claims

Synthetic tests cover two physical source owners, foreign identity/owner,
duplicate DATA, independent frontiers/interleaving, four model arms, timely
mandatory REST, post-SQL STOP/late completion with no source advance, exact
read-only SQL/control readback and tampered/truncated evidence. A 26-input /
52-outcome fixture demonstrates actual virtual USD MR / RI model actions, not
only ready flags. Fixture clocks and data are synthetic and labelled as such.
Post-SQL fault injection is NOT an actual SIGKILL/process-restart qualification.

This slice is a SOURCE COMPONENT, not the completed managed USD/RI runner.
There are no network-worker join/exit-witness or clean-session claims. No VPS,
FINAM API, credential, timer, installation or GitHub action is used by these
checks. Source review is requested before further operational composition.

## Remaining, necessary next slice

Compose this component with the existing managed auth lifecycle and two bounded
WS owners: fresh account/readonly/permission admission, historical seed/raw
bootstrap preparation, independent REST scheduling, bounded reconnect/resync,
STOP relay, worker join and actual exit receipt. Qualify aggregate reader against
those real worker facts and source/SQL crash frontiers. Do not copy the old
single-instrument Hybrid runtime as a dummy USD/RI consumer.

Then: exact-source Linux release + isolated native qualification → reviewed
bounded execution package → separately authorized installation and run. USDRUBF
BO wait4 needs observation beyond 11:20 MSK; no earlier entry is not a failure.
The accepted IMOEXF 19-Nov timers/config/ELF are unchanged. No 20-session campaign,
live micro, broker dispatch, Redis DB0 or orders is authorized here.

## Reproduction

```
cargo test --offline --locked -p broker-finam --all-targets
cargo test --offline --locked -p finam-signal-shadow --features models-bridge
cargo clippy --offline --locked -p finam-signal-shadow --features models-bridge --all-targets -- -D warnings
cargo clippy --offline --locked -p finam-signal-shadow --no-default-features --all-targets -- -D warnings
cargo test --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml
```

Exact logs, retained synthetic source cohort and byte-identity checks accompany
the source package. No governance closure, reviewer acceptance or operational
readiness is inferred from successful local tests.
