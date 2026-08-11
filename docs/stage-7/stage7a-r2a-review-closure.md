# Stage 7A-R2a — narrow review closure

Stage 7A-R2a is the direct repair of candidate `a50cd7e`. It does not open
Stage 7B or any FINAM/live execution surface.

The patch closes three independent-review findings:

1. A normalized same-authority paper outcome records a process-local,
   command-digest-bound recoverable canonical ACK. Redelivery after either the
   outcome-record boundary or the subsequent `RequestFinalized` boundary
   finishes the durable request and emits the first canonical ACK. Only a
   redelivery after known ACK publication emits the Stage 5G-compatible
   `Duplicate` no-op. Missing recovery evidence remains reconciliation-only.
2. The Stage 7A gate executes the accepted Stage 6E-R1 full gate in a detached
   checkout of `10e3578`, and A-049 requires its saved artifact.
3. F13 injects both source-read and claim-scan outages. The claim witness keeps
   an existing PEL entry unacknowledged, exercises bounded retry/backoff and
   proves that source health cannot heal failed claim health.

The F1-F15 matrix includes the new F6 finalization sub-window. A direct Stage
5G oracle proves that the recovered canonical ACK resolves the pending slot and
the following exact Duplicate is a no-op. A-019 uses the accompanying narrow
acceptance erratum.

External task liveness claims remain limited to normal return, returned error,
panic and JoinError. Cancellation/abort behavior is deferred to Stage 7B and
is not claimed here.
