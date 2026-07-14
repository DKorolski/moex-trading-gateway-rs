# Stage 5D-a3 — additive freeze enforcement and exact type-state bridge

Status: review candidate. Scope: design-only, no production source changes.

Stage 5D-a2 accepted the controlled additive-extension principle. This slice
closes the remaining design gap: how freeze enforcement migrates from the
historical Stage 5C source baseline to a dual-baseline model, and which exact
Stage 5C/Stage 5D type-state transitions implement the persistence-enabled
restore path.

## 1. Dual-baseline enforcement decision

Stage 5D will not delete or rewrite the Stage 5C closure baseline. Instead it
adds a second baseline.

| Baseline | Purpose | Enforcement |
| --- | --- | --- |
| Stage 5C closure baseline | Immutable historical proof of the accepted Stage 5C public API and production source at closure. | Retained as archived manifest/report/checker input. Used to prove what changed. |
| Stage 5D additive baseline | Current accepted source after the reviewed persistence bridge is added. | Pins the approved bridge files, region hashes, Stage5d* public API and unchanged Stage 5C public API shape. |

The old rule "current production file hash must equal the Stage 5C closure
hash" is replaced only for the approved bridge files. For every other pinned
file, whole-file hash enforcement remains unchanged.

## 2. Control artifacts that must change or be added

The additive extension will require changes beyond production source. The
allowed control-artifact scope is:

| Artifact | Required change |
| --- | --- |
| `scripts/stage5c_api_freeze_check.py` | Add dual-baseline mode: validate the 95 Stage 5C public symbols/signatures against the closure manifest while allowing approved additive bridge file hashes to move to the Stage 5D baseline. |
| `scripts/forbidden_surface_scan.sh` | Run Stage 5C public API freeze checks plus Stage 5D additive baseline checks; stop pinning approved bridge files to historical whole-file hashes as if they were byte-identical. |
| Stage 5C closure manifest/report | Remain immutable archived evidence; never silently overwritten. |
| New Stage 5D additive manifest | Pin approved bridge files, approved bridge regions, Stage5d* public API surface, and the immutable Stage 5C closure baseline reference. |
| New Stage 5D checker | Validate additive manifest, bridge region hashes, public namespace policy, and negative-test fixtures. |
| Negative harness | Add tests for public API drift, trading-region drift, bridge-region drift, namespace leakage, and missing baseline reference. |
| Handoff evidence | Record both baseline IDs, both checker results, and the approved source diff scope. |

Stage 5D-b must introduce this enforcement migration before adding persistence
DTO implementation logic. A DTO without dual-baseline enforcement is not
accepted.

## 3. Bridge-region hash policy

For `hybrid_intraday_runtime.rs`, whole-file hash replacement is not precise
enough. The wrapper must be split logically into:

| Region | Policy |
| --- | --- |
| Trading-semantic region | Byte-for-byte frozen against the Stage 5C closure baseline. BO/MR/high180/orchestrator/riskgate formulas remain unchanged. |
| Stage 5D bridge region | Explicitly delimited additive region containing only crate-private export/apply snapshot glue and conversion calls. |

The scanner must pin both region hashes. A change in the trading-semantic
region fails even if the whole file is listed as an approved Stage 5D bridge
file.

`stage5c_paper_host.rs` and `lib.rs` also require additive-region enforcement:

| File | Frozen region | Additive region |
| --- | --- | --- |
| `lib.rs` | Existing Stage 5C exports and doctest compile-fail boundary. | Stage5d* module/export list only. |
| `stage5c_paper_host.rs` | Existing Stage 5C public types/functions and callback logic. | Crate-private Stage 5D bridge transitions only. |

## 4. Required negative tests

The enforcement migration is accepted only if these negative tests exist:

| Mutation | Expected result |
| --- | --- |
| Change one of the 95 Stage 5C public symbols/signatures | Failure. |
| Change Stage 5C trading-semantic wrapper region | Failure. |
| Change approved Stage 5D bridge region and update Stage 5D manifest | Success. |
| Change bridge code outside an approved additive region | Failure. |
| Add public non-Stage5d symbol | Failure. |
| Add public raw strategy extractor or public Stage 5C capability constructor | Failure. |
| Remove historical Stage 5C closure baseline reference | Failure. |
| Change Stage5d* public symbol without updating Stage 5D manifest | Failure. |

## 5. Exact Stage 5D restore type-state chain

Stage 5D uses a single chosen model: explicit Stage5d wrapper capabilities.
It does not use ambiguous "mapping functions or maybe transition" alternatives.

The persistence-enabled restore path is:

```text
Stage5cRuntimeStateLoadedPaperStrategy
+ validated Stage5dRuntimePrivateExtension
→ Stage5dPrivateStateAppliedPaperStrategy

Stage5dPrivateStateAppliedPaperStrategy
→ Stage5d notify/bootstrap wrapper
→ Stage5dBootstrappedPaperStrategy

Stage5dBootstrappedPaperStrategy
+ authoritative RiskGateRuntimeState
→ Stage5dRiskGateInjectedPaperStrategy

Stage5dRiskGateInjectedPaperStrategy
→ Stage5d runtime-state-restored wrapper
→ Stage5cRuntimeStateRestoredPaperStrategy
```

The final output intentionally returns to the accepted Stage 5C chain only
after private extension and riskgate injection are complete.

## 6. Transition table

| Function | Input capability | Additional input | Mutation/callback point | Output on success | Recoverable block | Terminal failure |
| --- | --- | --- | --- | --- | --- | --- |
| `stage5d_apply_runtime_private_extension` | `Stage5cRuntimeStateLoadedPaperStrategy` | `Stage5dValidatedRuntimePrivateExtension` | Apply validated private extension before bootstrap callback. No Stage 5C callback yet. | `Stage5dPrivateStateAppliedPaperStrategy` | `Stage5dRestoreBlocked::PrivateExtension` carrying original loaded capability if no mutation occurred. | Invalid/corrupt extension; active lifecycle missing required extension; broker-object contradiction. |
| `stage5d_notify_bootstrap` | `Stage5dPrivateStateAppliedPaperStrategy` | clock/evidence context | Delegates to existing Stage 5C bootstrap notification order. | `Stage5dBootstrappedPaperStrategy` | Admission expired / bootstrap guard block preserving pre-callback state when possible. | Any Stage 5C bootstrap terminal failure. |
| `stage5d_inject_riskgate_state` | `Stage5dBootstrappedPaperStrategy` | authoritative `RiskGateRuntimeState` plus ledger/materialized evidence | Apply riskgate through crate-private bridge before runtime-state-restored callback. | `Stage5dRiskGateInjectedPaperStrategy` | `Stage5dRestoreBlocked::RiskGate` carrying bootstrapped capability if no mutation occurred. | Ledger identity/tail/generation mismatch; stale finalization identity; non-reproducible row count. |
| `stage5d_notify_runtime_state_restored` | `Stage5dRiskGateInjectedPaperStrategy` | restored callback context | Delegates to existing Stage 5C runtime-state-restored callback order. | `Stage5cRuntimeStateRestoredPaperStrategy` | Broker-truth validation block preserving riskgate-injected capability when callback has not run. | Any terminal Stage 5C restore failure after mutation/callback. |

Duplicate application is prevented because each function consumes its input
capability and returns a distinct next capability. The same private extension
or riskgate injection cannot be applied twice without reconstructing a consumed
capability, which remains impossible outside the crate.

## 7. Clean/no-persistence path vs persistence-enabled path

The existing Stage 5C clean/no-persistence path remains available for tests and
for explicitly non-persistent paper startup:

```text
prepare_stage5c_without_runtime_state
→ notify_stage5c_bootstrap
→ notify_stage5c_runtime_state_restored
```

The Stage 5D restore path must not use the public Stage 5C callbacks directly
after a persistence envelope is loaded. Operational Stage 5D admission returns
only Stage5d capabilities until private extension and riskgate state are
applied.

Bypass prevention is enforced by API shape:

- Stage 5D persistence loader returns `Stage5dPrivateStateAppliedPaperStrategy`,
  not `Stage5cRuntimeStateLoadedPaperStrategy`, after extension application;
- Stage 5D bootstrapping returns `Stage5dBootstrappedPaperStrategy`, not the raw
  Stage 5C bootstrapped capability;
- only `stage5d_notify_runtime_state_restored` can convert the
  riskgate-injected capability back to `Stage5cRuntimeStateRestoredPaperStrategy`;
- no public constructor exists for Stage5d capabilities.

## 8. Blocked and terminal preservation semantics

Recoverable block is allowed only before a mutation/callback has occurred or
when the exact input capability can be returned unchanged.

| Block point | Preservation rule |
| --- | --- |
| Private extension validation fails before mutation | Return original `Stage5cRuntimeStateLoadedPaperStrategy` inside a blocked result. |
| Private extension validation fails after partial mutation would be required | Do not partially mutate; validation must be complete before apply. |
| Bootstrap guard fails before callback | Return `Stage5dPrivateStateAppliedPaperStrategy` when unchanged. |
| Riskgate validation fails before injection | Return `Stage5dBootstrappedPaperStrategy` when unchanged. |
| Runtime-state-restored validation fails before callback | Return `Stage5dRiskGateInjectedPaperStrategy` when unchanged. |
| Any failure after callback mutation | Terminal failure; no retry with the same capability unless a later reviewed recovery type is added. |

The implementation must validate first, mutate second, and emit evidence for
the exact stage where restoration stopped.

## 9. Broker working-set ownership

`working_orders` and `working_stop_orders` are not persisted as authoritative
runtime state.

Stage 5D may persist only non-authoritative expected ownership hints:

```text
expected_working_order_ids: Vec<BrokerOrderId>
expected_working_stop_order_ids: Vec<BrokerStopOrderId>
```

Startup must rebuild actual working sets from broker truth. The runtime-private
snapshot may not directly rehydrate stale working maps before broker-truth
reconciliation. If broker truth cannot classify a target active object, restore
blocks before callback application.

## 10. Recovery ID collections

Stage 5D chooses separate typed collections, not a mixed untyped string list:

```text
known_order_ids: Vec<BrokerOrderId>
known_stop_order_ids: Vec<BrokerStopOrderId>
known_trade_ids: Vec<BrokerTradeId>
known_client_order_ids: Vec<ClientOrderId>
pending_requests: Vec<StrategyRequestId>
```

Each namespace has independent validation and broker-truth reconciliation.
`ClientOrderId` never substitutes for `StrategyRequestId`.

## 11. Stage 5D DTO namespace

Public persistence DTOs must not directly expose private source structs or
private source enums. Stage 5D defines stable schema enums and explicit
conversions:

```text
runtime-private representation
↔ Stage5d persistence representation
```

The conversion layer is crate-private unless a DTO is explicitly part of the
Stage5d public manifest. Redacted diagnostics may expose only schema names,
versions, hashes and blocker categories.

## 12. Stage 5D-b entry criteria

Stage 5D-b can start only after this design is accepted and must begin with:

1. dual-baseline checker/scanner migration;
2. Stage 5D additive manifest;
3. negative tests from section 4;
4. compile-fail tests proving no public Stage 5C extractor/constructor appears;
5. no Redis, FINAM, transport, dispatch, runtime-live or broker execution.
