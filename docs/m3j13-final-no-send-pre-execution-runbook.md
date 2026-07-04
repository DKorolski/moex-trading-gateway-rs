# M3j-13 final no-send pre-execution runbook

M3j-13 is a final no-send pre-execution package. It prepares the operator
runbook and attach-to-boundary separation evidence before any separately
reviewed real one-shot micro execution package.

The package requires:

- accepted M3j-12 dry attach rehearsal;
- operator runbook with exact stop conditions;
- kill switch coverage for runtime, command-consumer, and endpoint paths;
- authorization, immediate read-only evidence, config digest, session digest,
  and strategy digest binding;
- durable audit-before-boundary record format;
- post-run reconciliation and EOD report templates;
- explicit attach-to-boundary separation;
- scanner coverage for unguarded POST/DELETE/send;
- redacted artifacts only.

M3j-13 keeps all execution boundaries closed:

- `live_micro_go = false`;
- `runtime_live_attachment_allowed = false`;
- `boundary_invocation_performed = false`;
- `real_finam_order_endpoint_used = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `stop_sltp_bracket_replace_multileg_allowed = false`.

If any real FINAM endpoint access, runtime live attach, command-consumer to real
FINAM, non-loopback endpoint, or Stop/SLTP/bracket/replace/multi-leg request is
present, the runbook package is blocked.
