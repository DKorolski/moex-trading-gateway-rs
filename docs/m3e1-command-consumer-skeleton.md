# M3e-1 command consumer skeleton

M3e-1 starts the Redis command-consumer track after the M3d protected endpoint
stage was closed. This slice is intentionally skeleton-only: it does not call
the FINAM order transport, does not attach runtime strategies, and does not
enable live readiness.

Scope:

- consume a command-stream entry shape in a dry/local harness;
- decode broker-neutral `Envelope<BrokerCommand>`;
- validate schema version, message type, payload, request id carried by typed
  command, and TTL;
- publish a redacted `CommandAck` for valid/expired commands;
- publish a redacted command DLQ record for poison/invalid envelopes;
- model `XACK` only after ACK or DLQ publication.

Current M3e-1 command handling:

- valid non-expired command -> `Rejected / DryRunOnly` ACK;
- expired command -> `Expired / ExpiredCommand` ACK;
- invalid JSON / schema / message type / typed decode failure -> command DLQ;
- ACK and DLQ exports omit raw client id, broker id, raw command comment, raw
  payload, token, body, and account/order identifiers.

This is not yet the real Redis client loop. It is the reviewable contract for
one command-stream entry:

```text
entry -> decode/validate -> ACK or DLQ publish -> XACK marker
```

Still forbidden:

- endpoint transport invocation;
- external FINAM or any non-loopback order endpoint;
- order placement/cancel execution;
- runtime strategy attachment;
- `LiveReady`;
- stop/SLTP/bracket/replace/multi-leg.

Generate evidence:

```bash
python3 scripts/m3e1_command_consumer_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
