# P1-NRG01 — bounded process-test cleanup correction

Date: 2026-09-30. Status: TEST-ONLY CORRECTION REVIEW CANDIDATE.
No aggregate acceptance, merge readiness, O2 or operational authorization is claimed.

## Accepted boundary and finding

Source `2f46491a4f63249f64676306d5f9c3445818f4f9` remains SOURCE ACCEPTED.
Build fix `7c80d709679ccbf41d7f754364034df4bfafeb78` is independently accepted.
Review target/predecessor `3992fc75cedc56a55c2da420a235e6df9d162ea3` has a correct
authority delta, but its aggregate closure is HOLD for P1-NRG01.
Review: `FINAM_3992fc7_BUILDFIX_AUTHORITY_HANG_REVIEW_2026-09-30.md`, SHA-256
`63e756ebe7df889de5cb77496bf0142cfc604425f41f9d64a2d5c1692155e7b0`.

The prior stack sample locates the hang in test proxy teardown: listener joins
handlers blocked on live sockets. An early assertion can leave a raw Child
alive. The initial triggering assertion is unknown. The isolated witness PASS
does not replace the interrupted full suite, and no production shutdown defect
has been established. The old failed evidence remains unchanged.

## Scope

Rust changes are entirely inside `#[cfg(test)] mod tests` in
`stage8b_p1e_process.rs`. The production prefix, all other Rust source, Cargo,
CI workflows, profiles, source plans, warmup and trading semantics are unchanged
relative to `3992fc7`. There are no exported APIs or production feature changes.
The source delta also contains a local qualification runner, this documentation,
status updates and the normal authority inventory refresh.

- `ProcessTestChild` owns and reaps the production-startup test child on normal
  exit, timeout or unwinding. Child stdout/stderr go to retained, private local
  files, not discarded output or potentially full pipes. Cleanup records retain
  exit/signal, reap and unwind status without printing credentials/environment.
- Proxy cancellation closes registered client/upstream sockets before joining
  the listener and all handlers. A shared registration/stop lock covers the
  late-connect race; connect itself has a one-second test-only bound. There are
  no detached relay threads. Withheld responses are still held during the
  positive witness; the cancellation behavior applies only to teardown.
- Idle-client, blocked-upstream and withheld-response controls keep their peer
  sockets alive across proxy Drop and prove joined handlers within two seconds.
- A subprocess negative control forces an assertion after the real Redis reply
  has been withheld, before SIGTERM. Its exit must be 101 (not success), the
  original panic retained, the owned child reaped, and proxy handlers joined.
  The ignored fixture is explicitly executed by its non-ignored parent test;
  the positive witness remains enabled and is not replaced by this control.

No readiness wait or production grace has been increased. The original positive
witness still requires PaperReady, Draining before response release while the
child is alive, no later PaperReady, exit 0, final Stopped, and unchanged durable
state. The assertion now retains observed readiness phases for diagnosis. A
readiness race remains a hypothesis unless the new trace establishes it.

## Exact-tree qualification

`scripts/stage8b_p1f_nrg01_test_gate.py --phase qualification --output NEW_DIRECTORY`
requires a clean committed tree and refuses to overwrite evidence. It runs:

1. A watchdog negative control with an expected timeout/124 and owned-child reap.
2. fmt and the unchanged workspace/all-targets/all-features strict Clippy.
3. New cleanup controls, then three sequential exact positive witness runs,
   retaining each result without retry-to-green, and the other proxy witness.
4. Affected default process tests.
5. The complete unfiltered `runtime-durable-service --lib --all-features`
   suite with `--test-threads=1`; `--nocapture` only improves failure diagnostics.
6. Current-tree authority, its 45 negative cases and diff check.

The full suite has an external 5400-second deadline (90 minutes), based on the
previous approximately 23-minute default/35-minute interrupted full runs. This
is not a production grace change. Every command uses a fresh process group;
timeout is a retained FAIL, and only its own subprocess group is terminated.
Persistent local Redis/VPS are not targeted. Raw logs and test-child files are
hashed in the result, along with the before/after source inventory. Preliminary
development attempts are not relabeled as final-tree qualification.

## Exit boundary

The immutable review package must include the exact final gate result, complete
source and predecessor tree/diff proof, this review, failed preliminary evidence
where applicable and child diagnostics. A final PASS is only claimed by an
actually completed gate; a remaining failure is handed off with its primary
diagnostics. Fresh canonical GitHub `rust` and `redis-smoke` remain merge gates.
No additional universal CI matrix or production subsystem is introduced.

After independent correction acceptance and ordinary main synchronization:
new no-riskgate O2 artifact, separately authorized history-preserving install,
separately authorized bounded O2, then limited paper windows/ALOR comparison.
Keep the complete terminal history. FINAM writes, real orders and runtime-live
remain closed; VPS/P0 and installed artifacts are unchanged.
