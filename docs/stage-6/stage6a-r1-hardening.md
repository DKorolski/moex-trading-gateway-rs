# Stage 6A R1 — validation closure

R1 is one direct successor to `94c3fd9d841db5b207ad7fd50090e64c2770365d` on `stage6-durable-chain`.

The patch is validation-only:

- one intrinsic durable-domain validator covers account, instrument, Hybrid attribution and action/role compatibility;
- identity constructors and Deserialize use the same validator;
- snapshot constructors validate both identity and completed snapshot;
- record constructors validate the completed record;
- canonical decode requires exact input/re-encoded byte equality;
- unknown fields fail closed;
- reserved future taxonomy cannot be persisted as empty Marker payloads.

Unchanged: schema version 1, request→client derivation, cancel target correlation, logical record-ID formula, command payloads and both golden byte fixtures.

Still closed: Stage 6B+, journal backend, filesystem, Redis, FINAM, dispatch, runtime attachment, workers, scheduling and live orders.
