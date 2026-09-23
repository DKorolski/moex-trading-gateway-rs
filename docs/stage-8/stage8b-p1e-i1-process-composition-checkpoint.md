# Stage 8B-P1-e I1 process composition checkpoint

Status: internal implementation checkpoint after accepted pre-seal recovery
correction `a655da96ace23eb61d89642f63c49e5275ff98bd`. This is not the I1 review
boundary and does not authorize installation or activation.

## Present in this checkpoint

- the fixed executable identity `stage8b-p1-paper-supervisor`;
- corrected V5 fresh and historical bootstrap export through
  `P1BootstrapReady`, authenticated as the zero-effect `P1SemanticReady`
  lifecycle under adoption predicate version 2;
- a continuous production-path witness from V5 adoption through ordinary
  admission, canonical decision/successor M10, Hybrid callback, Market command,
  paper outcome, replacement truth, XACK and repeated admission without
  rewriting initial provenance or repeating callback/publication/provider/XACK;
- an explicit evidence boundary for Cancel: fresh flat paper V5 produces Market,
  while Cancel enters this process only as an authenticated durable restart
  outcome; injected LIMIT/Cancel construction is retained solely as an isolated
  integration fixture and is not counted as an end-to-end witness;
- exact fixed-path argv parsing for `validate-config`, `bootstrap`,
  `bootstrap-recover`, and composed `run`;
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
- a mandatory post-row latch recheck that destroys continuation authority on
  shutdown and otherwise issues one non-cloneable permit for exactly one next
  recovery row;
- typed schedule-free S06R advancement from ACK to the exact replacement
  truth and from truth to exact source resolution/XACK-last, with a fresh
  latch permit required between every transition;
- the first complete signed-schedule process path for a Ready/Working-LIMIT
  source: latch C performs one bounded newest-only read, latch D binds the
  exact acquired M10 and authenticated working-book transition, latch E
  rereads the committed V4, and latch F issues only the inherited Working
  authority before the existing semantic lifecycle runs;
- an empty latch-C schedule read retains the same non-cloneable Working route
  permit for a later bounded poll, while a stop at C, D, E, or F destroys all
  effect authority and leaves the exact M10 pending for authenticated restart;
- a real-Redis composition test proving the Working C-D-E-F success path
  returns `Ready` and reaches source XACK only as the lifecycle's final step;
- a complete plain-Market `CommandPublished` signed-schedule path: the exact
  decision M10 and first canonical successor are bound into V4, latch E/F
  precede the inherited paper provider, and the continuation stops at the
  typed P1-d2 `S_ack` owner with the decision source still pending;
- a package-preserving generated-Market `CommandPublished` signed-schedule
  path: the V4 record binds the exact P1-d4 reservation/publication seal,
  request/command identity, decision M10 and first canonical successor before
  latch E/F can issue the inherited generated-Market provider authority;
- prepublication routing derived from the authenticated durable candidate:
  generated Market uses only the reservation-bearing publisher and the generic
  publisher retains its fail-closed generated-candidate rejection;
- typed bounded successor waiting for plain Market, generated Market and
  Initial-LIMIT: a not-yet-arrived successor retains the exact published owner,
  while each retry rereads the newest signed schedule before effect authority;
- exact predecessor revalidation on an empty successor range, preserving
  fail-closed handling for predecessor loss, successor gaps and payload or
  operational-identity mismatch instead of converting them to polling;
- authenticated restart from that committed generated-Market V4 route: the
  process reclaims only the exact predecessor PEL entry, revalidates the
  immutable Redis marker/envelope/command and publication binding, verifies
  the exact first successor, restores schedule high-water, and continues
  through combined `S_ack -> S_truth -> XACK-last` without a second schedule
  read or command publication;
- generated-Market ACK/truth restart generation checks recognize the one
  additional V4 generation only when the checkpoint-covered schedule record
  proves the exact retained P1-d4 binding; the legacy no-V4 path keeps its
  original generation contract;
- a complete fresh Initial-LIMIT `CommandPublished` signed-schedule path: the
  exact request/command-bound decision M10 and its first canonical successor
  are sealed into a dedicated Initial-only V4 route, latch E/F issue only the
  Initial-LIMIT authority, and the existing LIMIT lifecycle advances through
  `S_ack`, `S_truth` and source-XACK-last back to exact `Ready`;
- authenticated restart from that committed Initial-LIMIT V4 route: V4 now
  retains the exact command-publication seal generation and commitment, and
  restart cross-validates them against the durable predecessor, immutable
  Redis publication marker, exact command bytes, exact predecessor PEL entry
  and exact first-successor identity before inherited P1-d3 execution;
- the restarted Initial-LIMIT route performs no second signed-schedule read,
  no second Hybrid callback and no command republish; it restores the durable
  schedule high-water and follows the same `S_ack -> S_truth -> XACK-last`
  sequence back to exact `Ready`;
- route authority is separated at the effect boundary: Initial, Working and
  Cancel schedule grants cannot be reused across one another, while the
  pre-P1-e legacy test grant remains confined to inherited fixture coverage;
- pre-I/O classification separates plain Market, generated Market and
  Initial-LIMIT published owners, so the plain-Market bridge cannot mutate
  either package-specific route before its own authority bridge exists;
- one linear S08 polling owner shared by startup `Ready/no-pending` and every
  terminal recovered `Ready` route;
- one bounded fresh read per S08 call: an empty read returns the exact same
  owner and verify-only control plane, while an acquired source crosses the
  mandatory post-acquisition latch before parsing or callback authority can
  escape;
- latch checks before acquisition and after an empty bounded wait, so shutdown
  winning either race destroys the in-memory Ready authority and requires
  authenticated restart instead of allowing another poll;
- a process-shared first-wins shutdown latch backed by `OnceLock`, allowing a
  signal/supervision task to stop an owner that is inside the bounded Redis
  wait without replacing the initiating cause, deadline or sequence;
- explicit fail-closed validation when a semantic recovery boundary labelled
  `Ready` does not contain its exact Ready owner;
- a same-invocation schedule-free drain that advances an acquired source
  through at most eight authenticated rows, checks the shutdown latch after
  every row, and returns to S08 only from an exact terminal `Ready` owner;
- the schedule-free S09 owner task: empty S08 reads and terminal Ready results
  remain inside one long-lived linear loop, while shutdown, retry-blocked and
  schedule-dependent results return typed ownership instead of dropping it;
- one combined S08/S09 owner task that retains exact Ready across both the
  schedule-free drain and every completed supported signed-schedule cycle;
  Ready is absent from its terminal API, so successful schedule processing
  immediately resumes bounded fresh polling without releasing ownership;
- one UTC anchor paired with one monotonic process anchor for that long-lived
  owner task: the trusted schedule-verification time is refreshed before every
  acquisition cycle, so a long Ready interval cannot preserve stale
  process-start time and a wall-clock rollback cannot move trusted time back;
- one linear S05/S06-to-S09 entry seam: Ready/no-pending and every composed
  continuation enter the same long-lived task, while LIMIT-dispatch and
  committed-binding startup routes remain explicit typed terminal owners until
  their exact signed-authority bridges are composed;
- one shared bounded lifecycle-to-S09 adapter for completed signed-schedule
  effects: every returned ACK/truth row crosses the same first-wins latch and
  only exact terminal Ready can become a fresh-poll owner again;
- one bounded supported-schedule cycle that pre-classifies plain Market and
  Ready/Working-LIMIT before schedule I/O, performs their existing C-F path,
  and feeds the resulting lifecycle through that adapter; every other route
  is returned as the same opaque unsupported owner;
- exact equality between the trusted schedule-verification clock and the V4
  binding clock, plus in-memory high-water advancement only after the signed
  binding/effect path has returned a committed lifecycle owner;
- restart recovery of the latest signed-schedule high-water only through an
  exact Ready owner whose authenticated checkpoint and committed seal cover
  the complete journal; the V4 signature, operational identity, instrument
  map and exact historical binding are revalidated before progression is
  restored, while journal-ahead/pending routes keep their typed historical
  continuation and cannot promote a global high-water mark;
- the compile-time-pinned I1A acquisition policy around those supported
  routes: at most 12 reads inside 60 seconds, a two-second per-read timeout,
  and deterministic 250/500/1000/2000/4000/5000-ms capped backoff;
- empty, stale, Redis-transport and read-timeout observations retain the exact
  linear owner for bounded retry; authentication, identity, progression and
  binding failures remain immediate fail-closed errors, and exhaustion keeps
  the exact M10 pending without falling through to another fresh read;
- shutdown observed during backoff interrupts the wait and reaches latch C
  before another Redis command; a verified source leaves the acquisition
  deadline and enters the non-cancellable durable lifecycle path;
- schedule-dependent continuations leave that drain as an opaque retained
  owner, while pending-not-claimable, blocked and shutdown outcomes remain
  structurally unable to poll fresh data;
- normalized quiescent, pending-not-claimable, multi-intent-blocked and
  schedule-deferred outputs that retain the verify-only control plane and do
  not expose raw lifecycle owners;
- explicit deferral of Cancel, Day-expiry and their recovered dispatch process
  routes until each can retain or reissue its exact signed V4 authority; no
  guessed or reconstructed schedule authority is issued;
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

## Composed process boundary

The accepted `run CONFIG` grammar now starts signal supervision first, loads
the fixed systemd credential, performs authenticated V5 ordinary-run admission,
attaches only the verified DB15 Redis surface and transfers the exact startup
owner into the continuing S06R/S08/S09 loop. Admission binds the immutable
adopted marker/receipt/provenance to the current authenticated restart package
without requiring the current seal to equal the initial receipt. A returned
committed-Cancel completion is consumed explicitly back into Ready polling; no
second owner or fresh admission of the completed V4 is created.

The schedule registry version and identity hash are explicit protected-config
bindings. They are never learned from the schedule envelope being verified,
which prevents a self-asserted trust root on the first signed read.

## Still required before review

- signed schedule-source process composition for Cancel, Day-expiry and their
  recovered dispatch routes; Ready/Working-LIMIT, plain Market, generated
  Market and fresh Initial-LIMIT now cover C-F and remain the reference
  ownership shapes;
- authenticated restart composition for later retained Initial-LIMIT
  frontiers after `S_ack`; the `ScheduleBindingCommitted` frontier is complete
  and remains the reference rule that no restart may reconstruct or guess
  consumed schedule authority;
- fixed-path composition tests for missing administrative F00, stale exact
  continuation F00, and supervisor-hash mismatch;
- fixed-path installation, systemd and aggregate I1 evidence after the
  separately reviewed process-supervision matrix.

The separately authorized OS-process matrix is implemented in
`stage8b-p1e-i1-process-supervision-matrix.md`. It covers idle SIGTERM,
idle SIGKILL/restart, panic exit class, committed-Cancel owner handoff and a
kernel SIGKILL after replacement Cancel truth but before source XACK.
It also delays exact server-processed Redis replies without changing the
production startup future and isolates the noncooperative exit-72 proof in the
common supervisor.

The continuous fresh-V5 Market witness now also preserves the production
signed-schedule boundary. After the real callback and command XADD it reads one
fixture-signed schedule through the production reader, commits the exact V4,
restarts before effect, and continues only from the V4-retained authority.
Recovery reclaims the exact PEL entry and revalidates the immutable publication
marker and first successor before the provider. The same lineage then reaches
S_ack, S_truth and XACK-last; a post-truth restart proves
`AlreadyAcknowledged`. Neither recovery frontier rereads schedule input,
replays callback/publication/provider effects nor recreates Redis.

The checkpoint review finding `P2-CP01` is closed locally by opening the fixed
config with `O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK` and exercising the exact
regular-file-to-FIFO replacement window through the production loader.

The generated-Market review findings on `66591fd` are addressed by the narrow
source correction documented in
`stage8b-p1e-i1-generated-market-process-correction.md` and independently
accepted at `ff6639e`. Freshly published CANCEL composition through exact
active-order/predecessor validation, signed V4 binding, all three inherited
P1-d3 race outcomes and source XACK-last is independently SOURCE ACCEPTED at
`cc1f02c`; its boundary and retained recovery-evidence requirements are
documented in `stage8b-p1e-i1-cancel-signed-schedule-composition.md`.

The held `9e6217a` Day-expiry source checkpoint exposed P1-DEX01: its separate
entry could start schedule verification with empty external high-water despite
an authenticated durable Open V4. The local correction makes recovery and
exact matching mandatory at admission, carries the recovered high-water in the
private deferred route, rechecks it before Redis read, and removes the public
low-level signed-snapshot bridge. The positive history is Open `1/1` to later
Closed `2/2`; conflict and rollback cases fail before V4/seal/effect, including
when Redis retention removed the prior row. The corrected boundary is
documented in `stage8b-p1e-i1-day-expiry-signed-schedule-composition.md`;
committed Cancel/Day-expiry restart remains the next slice.

Redis DB0/DB15 activation, VPS installation, FINAM POST/DELETE/send, broker
dispatch, runtime-live, and real orders remain closed.
