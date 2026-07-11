# Stage 5C-a — Stage 4-bound paper host admission

Status: review candidate.

Date: 2026-07-11.

## Scope

This is the first Stage 5C slice. It creates a fail-closed admission capability
for the future fixture-backed paper/mock host. It does not invoke the Hybrid
wrapper and does not expose any lifecycle callback.

Admission requires all of the following:

- accepted Stage 4J report with the expected schema and zero blockers;
- accepted Stage 4C/4E/4F/4G/4H status chain;
- exact Stage 4 mock lifecycle event sequence;
- closed redaction, live authorization and execution safety boundaries;
- matching applied Stage 4E snapshot and Stage 4J counts/target state;
- exact full `InstrumentId` equality across report, applied snapshot and host
  configuration;
- exact account scope equality with the applied broker-truth snapshot;
- exact `BrokerInstrumentSpec` identity and positive price step;
- strategy tick size equal to the accepted instrument price step;
- `allow_live_orders = false`.

The resulting `Stage5cPaperHostAdmission` has private fields and implements
neither `Deserialize` nor a public constructor. It records that runtime host and
intent sink attachment are both false.

## Fixture evidence

`stage5c_paper_host_admission.json` covers one accepted case and fail-closed
cases for blocked/inconsistent Stage 4 evidence, open safety boundary, blocked
or snapshot-less Stage 4 application, target/account/spec/tick mismatch and a
live-order request.

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
