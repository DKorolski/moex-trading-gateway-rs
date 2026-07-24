# Stage 5E-b3a — sealed ScheduleWindowEvidence

Baseline: `04431096e269daaf9715e253b2354b1ac8fcc3e8`.

This slice creates only the broker-neutral, no-I/O contract for a sealed,
instrument-scoped schedule window. It must not manufacture a trusted receipt
from caller-supplied scalar fields. Production mapping, if added, consumes
accepted Stage 4 schedule evidence and a sealed trusted schedule definition.

The window policy is inclusive: `open_from <= bar_close <= open_until`; mapper
validation requires `open_from < open_until`. The output is non-copyable,
private, carries lifecycle observation/expiry and a deterministic SHA-256
fingerprint over identity, window, source/version/epoch and Stage 4 identity.

Callback, strategy mutation, intents, Redis, FINAM, transport, dispatch,
runtime-live and execution remain closed. Binding to an observed bar is b3b,
not this slice.
