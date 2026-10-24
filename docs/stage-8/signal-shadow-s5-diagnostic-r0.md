# Signal-shadow S5 diagnostic — source candidate

Baseline: a8facf27617153fda818d42873e0830892e686e9. Scope is diagnostic only.
No deployment, FINAM access, observation activation or trading authorization.

The runner retains allowlisted transport_event receipts:

- BARS generation 1: handshake_and_subscription_sent, first_validated_data.
- QUOTES generation 1: handshake_and_subscription_sent, first_validated_data.
- After the existing one-shot controlled BARS reconnect, BARS generation 2
  emits its own handshake/send and first-data receipts.

The socket returns Handshake only after subscription send succeeds. This is
not a provider acknowledgement of a specific subscription. Every new receipt
therefore states provider_subscription_ack=false and finality_proven=false.
Validated channel data is the observed-delivery witness; socket-open, heartbeat,
empty data and successful send alone do not establish delivery.

New receipts contain only fixed schema/event/channel/instrument identifiers,
connection generation, local receive-completion time and item count.
No token, subscription key, raw payload, error text or provider-supplied string
is copied into these receipts. Existing accepted quote evidence is unchanged.
Each connection/channel emits at most two receipts. Failed persistence follows
the existing terminal transport-receipt failure path.

No callback, profile, deduplication, finality, seed or feedback algorithm changes.
The existing reconnect plan remains one-shot BARS-only. QUOTES reconnect is not
implemented or claimed. Late/missing generation-2 data leaves S5 incomplete;
the existing bars_reconnect_completion_observed field still means socket-open,
not complete S5. No new readiness authorization follows from diagnostics.

Operational acceptance must inspect persisted ordered receipts, existing
controlled_reconnect_socket_open and actual session coverage. It must also
retain the active-worker stop witness in the separately authorized bounded
observation. Tests are not live FINAM evidence.
