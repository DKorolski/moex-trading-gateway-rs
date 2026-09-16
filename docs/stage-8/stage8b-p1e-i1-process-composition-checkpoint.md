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
- nonblocking protected config open across the regular-file-to-FIFO replacement
  window, followed by descriptor metadata and identity validation;
- automatic next bootstrap-attempt generation derived from authenticated
  quarantine history rather than caller input;
- direct use of the accepted V5 administrative pre-seal recovery entry;
- exact historical-source continuation through the accepted supervisor
  recovery entry;
- the four adoption recovery actions without a fresh F00 reconstruction;
- an exhaustive, wildcard-free S04 ownership router for all 23 authenticated
  restart outcomes: 21 attachable owners and two typed pre-Redis terminal
  owners;
- a linear S05/S06 startup owner that retains the verify-only Redis control
  plane beside exactly one Ready/recovered/deferred durable route;
- an observation-only S06 `XPENDING` pass before any non-Ready accepted resume
  wrapper becomes the sole exact lookup/reclaim owner;
- a Ready-only pending scan with no fresh read, plus retained
  `PendingNotClaimable` ownership;
- a mandatory post-acquisition shutdown-latch conversion that either destroys
  effect authority and returns a retained-source receipt or issues one opaque
  route-bound continuation;
- a closed 23-variant post-latch continuation inventory, including separate
  generated-Market journal-ahead phases and separate LIMIT/expiry/CANCEL
  dispatch permits;
- a one-step S06R dispatcher that calls only the route's accepted resume
  function while retaining the verify-only Redis control connection beside
  the resulting lifecycle owner;
- explicit deferral of Ready/working-LIMIT and the three P1-d3 dispatch routes
  until exact signed schedule authority exists; no guessed or reconstructed
  schedule authority is issued;
- deferred typed ownership for the LIMIT-versus-expiry dispatch decision and
  for an already committed signed schedule binding;
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

The next implementation must connect the new owner-retaining S03-S06 seam to
the continuing S06R/S08/S09 loop before replacing this guard. It must retain
the existing source-first latch sequence and signed schedule binding
authorities; it must not attach and then return after dropping an owner.

## Still required before review

- production `run` wiring for credential load, S03 durable restart and the
  completed owner-retaining S04-S06 seam;
- completion of multi-step S06R settlement from the new exact one-step result
  through replacement truth and XACK-last to an authenticated quiescent,
  schedule-deferred, blocked, or retained-source boundary;
- signed schedule-source binding and A-F latch composition for Market,
  Working, Cancel, and Day-expiry routes;
- S08 bounded fresh polling and the continuing S09 owner loop;
- fixed-path composition tests for missing administrative F00, stale exact
  continuation F00, and supervisor-hash mismatch;
- process signal/panic/SIGKILL/restart evidence in its separately authorized
  subsequent gate.

The checkpoint review finding `P2-CP01` is closed locally by opening the fixed
config with `O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK` and exercising the exact
regular-file-to-FIFO replacement window through the production loader.

Redis DB0/DB15 activation, VPS installation, FINAM POST/DELETE/send, broker
dispatch, runtime-live, and real orders remain closed.
