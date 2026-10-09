# O3 P1-O301 — retained early shutdown correction

2026-10-09. **SOURCE CORRECTION REVIEW CANDIDATE; not SOURCE ACCEPT or O3 operation.**
Review baseline: `6f0a6bf8f49a57272ce6a20024f57743b6b11e1d` (SOURCE HOLD).
Accepted preparation ancestor remains `bc9813434b35238e9e6e861476cab2aaf3fc72a2`.
The [independent review](reviews/REVIEW_6f0a6bf_O3_SOURCE_HOLD_20261009_RU.txt)
identifies only P1-O301; accepted history need not be re-audited.

## Narrow source change

Composition creates the existing coordinator/latch before its lazy runtime
future. Both SIGTERM and SIGINT branches retain the external shutdown on that
same latch, using the already validated shutdown grace, then await the SAME
runtime future. Publisher failure retains OwnerFailure instead of re-raising a
signal before late subscription. Its fatal Producer result remains fatal.

The new observed entry accepts that coordinator, checks retained stop before
loading runtime config, and checks it again before owner startup. The owner and
ordinary supervisor receive the same latch. No extra authority, owner, timer,
signal framework, clock/config override or CLI capability was introduced.
OnceLock retains the first cause/sequence/deadline; a pending durable future is
not dropped. Live-owner bounded drain and fatal precedence remain ordinary paths.

Stop before admission returns existing conservative DurableRestart (runtime
exit class 66), NOT invented authenticated success. In early-stop tests the
test wrapper verifies this error and exits 0; its OS exit is not misreported as
the runtime's exit class. Normal live-owner stop still exits 0.

## Retained qualification

Evidence directories below are included in the immutable ZIP, not just console
summaries. Source snapshots are byte-bound to this correction, with only
explicit documentation/packaging changes allowed after qualification.

- `evidence/components/`: **28 exact Rust selectors**, each selected=1/passed=1,
  plus fmt, production build, strict clippy and diff check = **32 PASS records**.
  Includes first-wins, pre-admission rejection, retained telemetry deadline,
  live supervisor and owner-panic fatal-precedence regressions.
- `evidence/production/o301/`: **four Linux parent selectors**, two real
  SIGTERM/SIGINT cases and two disconnected-latch negative controls.
- `evidence/production/`: subsequent separately built production ELF proof,
  ordinary no-signal startup, one paper Market, successor M10, completed-frontier
  SIGKILL/restart, and bounded graceful stop. Both source rows resolved, six
  journal frames, ACK/truth 1/2 and Hybrid position +1. Child PID 592 → 715;
  parent exit 0; stop about 0.558 seconds, no forced stop; signer exit 70 after
  terminal revocation remains recorded as 70. Temporary containers removed.

Pinned result SHA-256:

```text
components/result.json
e9819ec36f630008aa7d6a480b768c88bde4fe55adc3f6a8e3147a944065d04e
production/result.json
9730d8fac191c65e49559bf1dff2f59398728522ea2dadbd03b44f0a29bc6b84
```

### Exact process selectors and ordering

Prefix: `stage8b_p1f_o3_composition::shutdown_tests::process::`.

| Selector suffix | Real signal | Parent / child OS exit | Meaning |
| --- | --- | --- | --- |
| o3_sigterm_before_runtime_start_is_retained | SIGTERM | 0 / 0 | retained stop; runtime error class 66 |
| o3_sigint_before_runtime_start_is_retained | SIGINT | 0 / 0 | retained stop; runtime error class 66 |
| o3_sigterm_disconnected_latch_mutation_is_detected | SIGTERM | 0 / 101 | expected missing-intent assertion |
| o3_sigint_disconnected_latch_mutation_is_detected | SIGINT | 0 / 101 | expected missing-intent assertion |

The child is nonroot and calls the real `run_fixed` path with a real typed
Published decision/successor pair. A **cfg(test)-only** Unix datagram barrier
holds the entry before runtime startup. Parent receives PUBLISHED_WINDOW_READY,
sends the OS signal, receives STOP_ACCEPTED from the selected composition branch,
then sends RELEASE. Raw logs prove STOP_ACCEPTED precedes RUNTIME_RELEASED.
The deadline is only a test-failure bound; elapsed sleeps are not the ordering
oracle. No test barrier or mutation environment switch exists in production ELF.

Each positive case compares durable root/journal/seal and bootstrap transaction/
receipt bytes, command/M10 rows and group states, and Redis command counters:
XREADGROUP, XAUTOCLAIM and XACK stay at zero. No command, callback/provider effect
or fresh runtime admission occurs. Already published data is kept; the schedule
checkpoint is byte-identical across stop. These are local disposable objects,
not reads or replacement of accepted VPS/O2 state.

Negative controls omit ONLY the composition latch write, corresponding to the
reviewed defect. The same assertion rejects missing retained intent before
release; child exit 101 and exact diagnostic are retained. They do not claim to
have deliberately executed unsafe paper effects. The subsequent no-signal
control is the ordinary production ELF, with no test barrier, sharing the
unchanged bootstrapped root and published data left by the early-stop cases.

Unit/process fixture builds and production builds are separate Cargo invocations.
Production artifact feature lists exclude fixture features. Real clock and
accepted generation-2 public schedule trust are used; local history is synthetic.
The selected private schedule key and local HMAC credential are not in the ZIP.
This remains Linux **debug** qualification, not a release/installation artifact.

### Reproduction and intermediate failures

Use `scripts/stage8b_p1f_o3_local_components.py NEW_OUTPUT` for local checks.
For the explicitly authorized disposable Linux witness, use the existing
`stage8b_p1f_o3_signer_linux_witness.py` with `--linked-runtime` and
`--early-shutdown-regressions`, its selected schedule-key argument and explicit
local-key-use flag. It waits for ordinary real M10 boundaries; no VPS or broker
network is used. Do not use it as operational O3 authorization.

Final retained runs: `o3-shutdown-correction-20261009/components-run4` and
`production-run4`. Earlier local attempts are not promoted to acceptance:
run1 caught a missing Linux-only map type annotation; run2 caught test-launcher
setgroups ordering after UID drop; run3 passed SIGTERM but failed Docker cp
export from container /run. The launcher now clears groups before dropping
UID, and public diagnostics are exported via fixed-path docker exec/cat.
Run4 repeats the complete matrix and positive production witness on one unchanged
source snapshot. No operational permissions were broadened to fix test plumbing.

## Next gate

One focused source re-review closes P1-O301 if accepted. Then one exact release/
authority/installation package, a separately allowed stopped installation
preserving accepted O2 root/profile/key/marker/receipt/history/P0, and separately
authorized bounded O3. All 13 operational O3 rows remain NOT_RUN.
O4 retains WS subscriptions, reconnect, freshness, REST overlap, EOD and ALOR
paper comparisons. No VPS, DB0, FINAM request, broker write, runtime-live,
push/merge or GitHub CI was performed for this correction.
