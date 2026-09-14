# Stage 8B-P1-e I1 process composition checkpoint

Status: internal implementation checkpoint after accepted pre-seal recovery
correction `a655da96ace23eb61d89642f63c49e5275ff98bd`. This is not the I1 review
boundary and does not authorize installation or activation.

## Present in this checkpoint

- the fixed executable identity `stage8b-p1-paper-supervisor`;
- exact fixed-path argv parsing for `validate-config`, `bootstrap`,
  `bootstrap-recover`, and reserved `run`;
- protected root-owned supervisor-config loading and canonical Linux boot-id
  parsing;
- automatic next bootstrap-attempt generation derived from authenticated
  quarantine history rather than caller input;
- direct use of the accepted V5 administrative pre-seal recovery entry;
- exact historical-source continuation through the accepted supervisor
  recovery entry;
- the four adoption recovery actions without a fresh F00 reconstruction;
- stable nonzero process exit classes and redacted stdout/stderr.

The seven pre-seal actions are routed exactly as follows:

```text
remove-marker-temp                         administrative / no F00
quarantine-root                            administrative / no F00
finalize-quarantine                        administrative / no F00
resume-prepared                            exact historical F00
complete-prepared-to-root-published        exact historical F00
complete-root-published-to-journal-durable exact historical F00
complete-journal-durable-to-seal-committed exact historical F00
```

The four adoption actions use the authenticated durable transaction directly
and do not rebuild fresh admission.

## Deliberate fail-closed boundary

The accepted `run CONFIG` grammar is reserved, but this checkpoint returns
`OwnerLoopUnavailable` before credential loading, durable restart, or Redis
attachment. A verify-only attach followed by dropping the linear restart
owner is not treated as successful startup.

The next implementation must replace this guard with one exhaustive
single-owner composition over all 23 authenticated restart outcomes. It must
retain the existing source-first latch sequence and the signed schedule
binding authorities; it must not add a generic owner escape hatch.

## Still required before review

- route-exhaustive S03/S04 restart-to-acquisition dispatcher;
- S05 verify-only Redis session retained beside the durable owner;
- S06 pending-only recovery and S06R continuation to an authenticated
  quiescent or retained-source boundary;
- signed schedule-source binding and A-F latch composition for Market,
  Working, Cancel, and Day-expiry routes;
- S08 bounded fresh polling and the continuing S09 owner loop;
- fixed-path composition tests for missing administrative F00, stale exact
  continuation F00, and supervisor-hash mismatch;
- process signal/panic/SIGKILL/restart evidence in its separately authorized
  subsequent gate.

Redis DB0/DB15 activation, VPS installation, FINAM POST/DELETE/send, broker
dispatch, runtime-live, and real orders remain closed.
