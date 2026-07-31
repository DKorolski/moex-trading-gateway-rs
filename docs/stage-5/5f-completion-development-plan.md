# Stage 5F completion development plan

Status: active local functional plan  
Base: `0fcab80e4c13822891eeae9bceb0f895b4d453a9`  
Mode: IMOEXF Hybrid, canonical final M10, paper-only, no send

## Inputs

The implementation uses these externally reviewed design inputs:

- `STAGE5F_COMPLETION_TECHNICAL_SPEC_RU_2026-07-31.md`, SHA-256
  `4205f948b795a9d4369283e8565f68cb56b509f2e79bf05549de2f3a8b6dc6c1`;
- `STAGE5F_ACCEPTANCE_MATRIX_DESIGN_V1_2026-07-31.json`, SHA-256
  `979e85d5a64352b1158622688d0322ff17349d8ce4ac5c2c4d5d4373912df59b`;
- completion package, SHA-256
  `4ee13305616cb151bf53d5f1d929666c110446064f84cb754d5c2331e982bb17`.

The governance split and contract corrections are normative in
`docs/adr/adr-stage5f-local-development-governance.md`.

## Delivery order

1. **Technical reproducibility preflight**
   - portable forbidden-surface scan and 87-case no-`rg` matrix;
   - exact accepted-B3F dependency preparation on clean runners;
   - parent-owned handoff evidence cleanup;
   - no functional Rust/runtime behavior change.
2. **5F-b0 source-reachability and observation design**
   - classify all 34 proposed rows against the sole route;
   - pin the two distinct fingerprint algorithms;
   - authorize only the minimum test-only observation region.
3. **5F-b1 fixture/input/fingerprint contract**
   - versioned, fail-closed fixture schema;
   - redacted ordered-intent projection;
   - complete inventory with path/hash/test ownership;
   - no callback invocation.
4. **5F-c controlled invocation**
   - one authorized callback;
   - one observer consume;
   - one B3F settlement attempt;
   - accepted and typed terminal dispositions.
5. **5F-d complete atomic matrix**
   - BO and MR entry/exit behavior;
   - arbitration and owner/cycle invariants;
   - riskgate normal-append and authority blockers;
   - pending/deferred behavior that is source-reachable without broker feedback;
   - terminal and negative matrix.
6. **5F-e aggregate acceptance**
   - deterministic debug/release tests and repeated fingerprint runs;
   - local project gates and clean handoff package;
   - independent aggregate review.

## Development gates

The accepted `stage5f_atomic_hybrid_semantics_gate.sh` is historical authority
for the recovered Stage 5F-a tree. It is not widened to accept later functional
paths. Later heads inherit that proof from the immutable snapshot and use:

```bash
# Fast local development gate
bash scripts/stage5f_functional_development_gate.sh

# Review/handoff gate including the detached B3F 580-case provenance matrix
STAGE5F_FULL_INHERITED_GATE=1 \
  bash scripts/stage5f_functional_development_gate.sh
```

This split keeps routine iteration economical while retaining the full
inherited proof before a review package is declared ready.

## Sole route

```text
Stage5eStage5cAuthorizedCallbackMaterial::invoke_authorized_callback_once
  -> BrokerNeutralHybridStrategy::on_broker_bar
  -> HybridIntradayRuntimeStrategy::on_bar
  -> high180/riskgate update
  -> HybridOrchestrator
  -> ordered broker-neutral semantic intents
  -> validate_and_settle_stage5e_paper_callback_escrow
```

No alternate BO-only/MR-only route, second orchestrator or second callback is
accepted as evidence.

## Closed surfaces

- Redis command consumption;
- FINAM transport and real endpoints;
- dispatch and broker execution;
- runtime-live and deployment;
- ACK/order/trade/position/timer/restart feedback;
- stop/SLTP/bracket implementation;
- strategy parameter or riskgate-enforcement changes.
