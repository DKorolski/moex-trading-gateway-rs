# Stage 5D-b2a — versioned persistence envelope and API schema

Stage 5D-b2a extends the accepted Stage 5D additive freeze with a schema-only
versioned persistence envelope. It is intentionally a contract/data-shape patch:
no runtime-private snapshot application, no Stage 5C restore invocation, no Redis,
no FINAM transport, no dispatch, and no runtime-live behavior are added.

## Added public schema surface

- `Stage5dPersistenceEnvelope` with explicit schema version, snapshot identity,
  snapshot revision, previous revision, write generation, persisted-at timestamp,
  per-family timestamp policy, canonical config fingerprint, payload checksum, lifecycle
  watermarks, recovery indexes, runtime-private extension DTO and riskgate DTO.
- `Stage5dTimestampPolicy`, which fixes source semantic lifecycle timestamps as
  epoch seconds, runtime wall-clock timers as epoch milliseconds, and structured
  timestamps as RFC3339 UTC. A single global timestamp unit is intentionally not
  used because it cannot represent the source runtime contract.
- `Stage5dSnapshotBinding` with stage, strategy kind, strategy/account,
  target instrument, profile binding, broker-protocol/runtime-state versions,
  Stage 5C/5D config fingerprints and source commit/build identity.
- `Stage5dStrategyStatePayload` with a versioned canonical JSON semantic
  `StrategyState` payload plus strict `Stage5dSemanticStrategyStateV1` /
  `Stage5dHybridIntradayStrategyStateV1` schema. The payload is not a public
  runtime-private source struct and does not expose a raw strategy extractor.
- `Stage5dInstrumentBinding`, a strict Stage 5D-owned target instrument DTO
  with conversion to broker-core `InstrumentId`.
- Typed recovery indexes for `BrokerOrderId`, `BrokerStopOrderId`,
  `BrokerTradeId`, `ClientOrderId`, and `StrategyRequestId`. `ClientOrderId`
  remains distinct from `StrategyRequestId`.
- `Stage5dRuntimePrivateExtension` DTO for pending entry/exit, partial-entry
  timer, bracket reconciliation timer, cleanup retry state, expected working
  sets, processed-bar watermark and riskgate finalization outbox.
- `Stage5dRiskGatePersistence` DTO for identity, materialized state, ledger tail
  hash and finalization outbox.
- `Stage5dEnvelopeValidationError` and checksum/schema validation helpers.
- `Stage5dPersistenceEnvelope::from_json_str_strict(...)`, a single public
  strict decode helper that rejects unknown Stage 5D DTO fields before checksum
  validation.
- `Stage5dPersistenceEnvelope::validated_from_json_str_strict(...)` and
  `validate_restore_contract_schema_only(...)`, which produce opaque
  `Stage5dValidatedPersistenceEnvelope` only after checksum, semantic payload,
  binding, timestamp chronology, source-roundtrip tuple, recovery-index and
  pending-state consistency checks.

The restorable enums intentionally match the accepted source semantics:

- `Stage5dEntryStyle`: `Market`, `Bracket`.
- `Stage5dLifecycleReason`: all source `ReasonCode` variants.

Execution/config order styles such as marketable-limit belong to profile/config
binding, not the runtime-private pending-entry semantic field.

## Freeze and validation

The Stage 5D additive manifest now freezes the complete public Stage5d API
surface: reexports, constants, public types, public fields, enum variants,
public methods, opaque capabilities and a normalized signature hash.

Current surface counts:

- public reexports: 43
- public constants: 5
- public types: 38
- public methods: 11
- opaque capabilities: 7
- externally constructible enums: 12
- normalized signature hash:
  `1a8c63e2fe58e4a39920b06637d45e51ea21ef21781945308a7b5049409e0be3`

The negative harness now includes `stage5d_api_surface_drift`, which mutates a
public Stage5d DTO field and proves that the checker rejects the changed surface
even when the file hash is updated.

## Fixtures

- `tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json`
- `tests/fixtures/stage5/stage5d_b2a_persistence_envelope_corrupt_checksum.json`
- `tests/fixtures/stage5/stage5d_b2a_persistence_envelope_bad_version.json`
- `tests/fixtures/stage5/stage5d_b2a_persistence_envelope_empty_config.json`

The valid fixture deserializes, reserializes and validates its payload checksum.
The corrupt fixture deserializes but is rejected with
`PayloadChecksumMismatch`. Version and config-negative fixtures are rejected with
`RuntimePrivateSchemaMismatch` and `RequiredFieldEmpty`.

Additional unit tests prove strict decode rejection for unknown fields at:

- envelope root;
- runtime-private extension;
- riskgate section;
- nested outbox record.
- nested instrument binding.

Additional schema-only restore-contract tests reject:

- scalar/array/wrong-variant semantic state payloads;
- unknown, misspelled or invalidly typed semantic state fields;
- semantic timestamps encoded as milliseconds;
- runtime wall-clock timers encoded as seconds;
- timers after the persisted snapshot timestamp;
- pending-entry creation after the last broker-event watermark;
- source-local date/datetime strings that would parse to `None` in source;
- partial deferred-entry/deferred-exit/shadow-position tuples;
- missing pending request coverage for pending/deferred TP/SL/entry/exit;
- broker object references or expected working sets missing from typed indexes;
- duplicate recovery-index values;
- inconsistent bidirectional pending-entry/private/timer state.

The valid fixture is a single consistent partial MR entry restart scenario:

- 10-character hex cycle id;
- partial fill quantity below target quantity;
- semantic pending-entry owner/side/cycle/request/timestamp populated with source
  epoch seconds;
- matching runtime-private pending-entry DTO;
- matching partial-entry timer populated with runtime epoch milliseconds;
- matching `pending_requests` coverage;
- no contradictory pending exit;
- parseable source-local date/datetime strings;
- top-level and binding Stage 5D config fingerprints equal.

Riskgate finalization collections are named by role:

- `runtime_pending_finalizations`: runtime-observed pending finalizations;
- `durable_finalization_outbox`: durable riskgate outbox state.

Stage 5D-b2a defines the schema roles only; restore-contract validation for
monotonic durable/runtime relations remains a later pre-mutation gate.

## Still forbidden

- Runtime-private mutation/application.
- Direct or indirect calls to legacy Stage 5C restore functions from new Stage 5D
  code.
- Redis, FINAM, broker transport, command dispatch, runtime-live, autonomous loop
  or real order execution.
