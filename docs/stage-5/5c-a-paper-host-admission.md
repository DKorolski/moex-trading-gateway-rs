# Stage 5C-a — Stage 4-bound paper host admission

Status: review candidate.

Date: 2026-07-11.

## Scope

This is the first Stage 5C slice. It creates a fail-closed admission capability
for the future fixture-backed paper/mock host. It does not invoke the Hybrid
wrapper and does not expose any lifecycle callback.

Admission now accepts one opaque `Stage4AcceptedPaperHostEvidence` produced by
the canonical Stage 4E→4I assembly function. Independent report/application
DTOs are not accepted. The FINAM Stage 4J path exposes a matching canonical
builder.

Admission requires all of the following:

- accepted Stage 4J report with the expected schema and zero blockers;
- accepted Stage 4C/4E/4F/4G/4H status chain;
- exact Stage 4 mock lifecycle event sequence;
- closed redaction, live authorization and execution safety boundaries;
- matching applied Stage 4E snapshot and Stage 4J counts/target state;
- exact full `InstrumentId` equality across report, applied snapshot and host
  configuration;
- exact account scope equality with the applied broker-truth snapshot;
- exact `InstrumentId` from `BrokerInstrumentSpec` and positive price step;
- strategy tick size equal to the accepted instrument price step;
- `allow_live_orders = false`.

The Stage 4 bundle contains the exact applied bootstrap snapshot and the minimum
expiry of all required source sections. At admission time, future-dated or
expired evidence is rejected. The resulting `Stage5cPaperHostAdmission` has
private fields, implements neither `Deserialize` nor a public constructor, and
stores `checked_ts`, `issued_ts`, `expires_at` and the exact admitted snapshot.
It records that runtime host and intent sink attachment are both false.

## Fixture evidence

`stage5c_paper_host_admission.json` covers one accepted case and fail-closed
cases for target/account/spec/tick mismatch, a live-order request, stale
evidence and future-dated evidence. Broker-core tests prove canonical-chain
construction, exact snapshot binding across summary-equivalent runs and source
expiry calculation; broker-finam tests exercise the real Stage 4J builder.

## Safety boundary

- strategy callbacks invoked: false;
- runtime host attached: false;
- paper intent sink attached: false;
- command consumer attached: false;
- live send enabled: false;
- real POST/DELETE added: false;
- Stop/SLTP/bracket broker execution added: false.

## Next review gate

After acceptance, Stage 5C-b may add the bootstrap-notification facade and its
fixtures. Runtime state restore, warmup/history and first semantic bar remain
separate ordered gates. Stage 5D still owns persisted state migration policy.
