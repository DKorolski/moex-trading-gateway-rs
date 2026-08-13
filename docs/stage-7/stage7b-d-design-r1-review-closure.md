# Stage 7B-d Design R1 — accepted freeze

Status: independently accepted/frozen at
`00cead2989493b44e0d86ead29b95d57a7fbcbe2`.

Predecessors:

- accepted Stage 7B-c: `c57ae8d5f98bbb11df0a81f78262d3916b276d81`;
- rejected-as-frozen initial design: `09a22765ae6ee37b304bfed6492bd103da44360d`.

This design-only closure addresses the four blocking findings from the
independent review:

1. terminal ACK authority is an exact-bound, linear and non-serializable
   `DurableAckAuthorized`, combined with one exact Redis entry into a linear
   settlement plan;
2. permanent pre-Stage6 poison uses a separate `PoisonDlqAuthorized` and
   requires proof of zero Stage6 admission/mutation;
3. Redis settlement uses a stable entry key independent of payload fingerprint,
   a conflict-checking marker value, a request-level canonical marker, expected
   PEL membership for new settlement and validation before first mutation;
4. B-052/B-053 stay pending in d-a and may close only in d-c with their required
   real-Redis restart witnesses.

The design also freezes fail-stop reconciliation after an ambiguous recovery
seal commit and limits B-059 to client/process response loss within one Redis
durability/failover domain.

No production source, Cargo dependency or workflow changed in this accepted
design freeze from accepted
Stage 7B-c. Redis settlement, XACK, FINAM transport, runtime-live and real orders
remain disabled. Its acceptance opened only Stage 7B-d-a; d-b and d-c remain
closed.
