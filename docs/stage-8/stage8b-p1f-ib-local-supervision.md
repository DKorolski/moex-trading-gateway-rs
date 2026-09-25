# Stage 8B-P1-f Ib — fixed local guardian/I1 supervision

Status: `REVIEW_CANDIDATE_LOCAL_SUPERVISION_ONLY`.

Accepted predecessor: P1F-Ia guardian foundation at
`9be356b04a38e627337ed148ccc9fbdaebae8d4a`. This slice composes that linear
guardian permit with the already accepted P1-e/I1 process. It performs no
installation, target start, Redis activation or FINAM operation.

## Fixed production boundary

The new `stage8b-p1f-local-supervisor` accepts only:

```text
stage8b-p1f-local-supervisor run MANIFEST_SHA256
```

It opens the fixed root-owned P1-f authority, resolves the exact
`moex-p1-paper` identity and can launch only:

```text
/usr/local/libexec/moex/stage8b-p1-paper-supervisor \
  run /etc/moex-finam-p1-paper/supervisor.json
```

There is no public arbitrary executable, argv, UID/GID, Redis, FINAM or broker
dispatch input. O1 and O2 permits cannot enter this I1 run composition. A
child is created only after an exact O3/O4 Active permit has acquired the sole
execution flock.

## Process ownership and stop ordering

SIGTERM and SIGINT handlers are installed before authority admission. A signal
retained before spawn commits Stopping and creates no child. Each admitted
child becomes leader of a new Unix process group. The production launch drops
supplementary groups and changes to the exact service GID/UID before exec. On
Linux it also sets `PR_SET_PDEATHSIG=SIGKILL` and checks the parent identity,
so abrupt guardian death cannot leave the I1 child running outside its owner.

The original `Stage8bP1fRunPermitV1` remains alive across child restarts.
Restart therefore cannot reacquire phase admission, replace the original
deadline or repeat a guardian claim. The retained I1 policy is bounded to five
starts in 600 seconds with a five-second delay. A clean child exit without a
guardian stop decision and restart-budget exhaustion fail the phase closed.

For an operator signal or phase deadline the permit commits the exact durable
Stopping transition. The guardian forwards TERM/INT to the whole process
group, polls the same process-local monotonic witness and sends SIGKILL to the
whole group at `ForceKill`. The 30-second authority grace is unchanged. A
graceful operator stop records Completed; deadline completion records Expired;
clock distrust, recovered Stopping, signal-task failure and forced
non-deadline termination record Failed. Failure paths return nonzero even when
terminal evidence was successfully retained.

## Recovery behavior

An Active execution-owner record with no live permit is not readmitted. The Ia
recovery changes it to Stopping and Ib resumes that exact transaction. Because
the monotonic witness was lost, the first poll is `ForceKill`; Ib does not
spawn a replacement child and terminates the phase conservatively. A retained
Stopping phase follows the same no-spawn path. Pending authority writes remain
owned by the accepted Ia exact-recovery API.

Every in-process error keeps a kill-on-drop process-group guard. Guardian or
signal supervision failure first requests bounded termination and then kills
the group if it remains alive. No error path intentionally detaches a child.

## Executable evidence

The real-process tests use isolated `/bin/sh` children only as the private test
replacement for the fixed I1 command. They prove:

- retained pre-spawn SIGTERM creates no child;
- cooperative SIGTERM and SIGINT stop the real child and commit Completed;
- one nonzero child exit restarts under the same permit;
- the five-start restart budget fails closed without readmission;
- a TERM-ignoring leader and descendant are killed as one process group;
- signal-supervision loss returns nonzero, commits Failed and leaves no child;
- lost guardian/monotonic ownership resumes Stopping and never spawns;
- on Linux, killing the guardian with SIGKILL triggers the child's parent-death
  SIGKILL.

The Ib gate also reruns all inherited runtime-durable-service tests, doctests,
strict Clippy and the accepted Ia multi-UID evidence when a root Linux runner
is available.

## Remaining boundary

P1F-Ic producer/high-water composition, P1F-Id fixed Redis roles/resource
polling/command audit and P1F-Ie aggregate source closure remain open. P1F-O0
through O4 and P1F-A remain closed. This slice does not install either binary,
modify a systemd unit, connect to DB15/DB0, execute a paper provider, attach to
FINAM, send/cancel an order, dispatch to a broker or start runtime-live.
