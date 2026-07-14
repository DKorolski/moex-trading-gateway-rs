# Stage 5D-b2a — versioned persistence envelope and API schema

Stage 5D-b2a extends the accepted Stage 5D additive freeze with a schema-only
versioned persistence envelope. It is intentionally a contract/data-shape patch:
no runtime-private snapshot application, no Stage 5C restore invocation, no Redis,
no FINAM transport, no dispatch, and no runtime-live behavior are added.

## Added public schema surface

- `Stage5dPersistenceEnvelope` with explicit schema version, snapshot identity,
  snapshot revision, previous revision, write generation, persisted-at timestamp,
  timestamp units, canonical config fingerprint, payload checksum, lifecycle
  watermarks, recovery indexes, runtime-private extension DTO and riskgate DTO.
- `Stage5dSnapshotBinding` with stage, strategy kind, strategy/account,
  target instrument, profile binding, broker-protocol/runtime-state versions,
  Stage 5C/5D config fingerprints and source commit/build identity.
- `Stage5dStrategyStatePayload` with a versioned canonical JSON semantic
  `StrategyState` payload. The payload is not a public runtime-private source
  struct and does not expose a raw strategy extractor.
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

- public reexports: 37
- public constants: 5
- public types: 32
- public methods: 6
- opaque capabilities: 6
- externally constructible enums: 10
- normalized signature hash:
  `7a9a4b942d0196d47100a826a0eb4e2b60f48833511b4fc022a78b513dd07ea0`

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
