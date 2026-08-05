# Stage 5G-e-d-b — deterministic fresh BrokerTruth reducer

Accepted predecessor: `4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`.

This slice adds classification and opaque candidate construction only. The
single reducer entry point consumes one authenticated clean-restart capability
and one e-d-a validated fresh BrokerTruth package. Its linear result retains
both inputs; no competing continuation is returned.

The clean-restart owner exposes a narrow immutable projection containing the
accepted lifecycle kind, strategy/account/instrument binding, Stage 5C SHA-256 config
fingerprint, committed summary/checkpoint, request/client/broker identities,
local order/trade/position evidence, callback/request counts and semantic
fingerprints. The fresh package remains the existing canonical Broker Core
order/trade/position model. No raw broker DTO or second domain model enters the
reducer.

Cross-binding compares every fact available on both accepted authorities:
account, strategy definition, canonical config fingerprint, target instrument
and reconstructed runtime-state fingerprint. Deployment/gateway/feed identity
continues to be constructor-validated inside the e-d-a package; the accepted
Stage 5G clean-restart schema does not duplicate those fields, so e-d-b neither
weakens nor invents a second source for them.

The reducer performs no observable mutation. Blocked, contradiction and exact
replay outcomes retain checkpoint, generated-intent escrow, counters and runtime
state. `ApplyOwnedCandidate` contains only an in-memory order/trade/position
projection with explicit request/client/broker identity, side and target
quantity binding; it is not Clone, Serialize, transport input or persistence output.
Applying or exporting it belongs to Stage 5G-e-d-c after separate acceptance.

GRST01–GRST12 execute in their frozen order. Incomplete sections map to waiting,
never broker absence. Complete contradictions map to reconciliation, manual
intervention or terminal inconsistency without a candidate. Exact replay is a
semantic no-op. Debug, release, sequential and parallel runs use no wall clock
or shared mutable state. Focused executable negatives cover cross-boundary
identity, request/client/broker/trade linkage, independent completeness,
quantity/position convergence and distinct canceled/rejected/expired outcomes.

Still closed: strategy callbacks, runtime and Stage 5D mutation, persistence
export, Redis, FINAM, HTTP POST/DELETE, broker dispatch, runtime-live, real
orders, Stage 5G-f and Stage 6.
