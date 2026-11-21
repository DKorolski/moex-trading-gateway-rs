# O2 first-boot Replay correction — 2026-10-08

Status: source review candidate, not O2 acceptance or deployment permission.
Baseline: `d1a4004959add2bc660b0cbfff1e8b9502dee18a` (installed ELF source
`817985c66c321accbe32d712428cf6f9c6ac39d9`).

## Observed failure and reproduction

The single authorized 15:15 MSK attempt reached `ReadyForBootstrap` and then
failed at bootstrap exit 66 (`first-boot source is invalid`). Its final state
is **FAILED/1/23**. No durable root was created; P1 stopped, P0 unchanged,
DB15 empty. This correction makes no remote calls and does not repeat O2.
The consumed phase and terminal history must not be reused or reset.

Exact retained public source SHA-256:
`22fbcbe9b883ffd9a801b188f32763c8596d3e6134cd70b642f7268006c440ce`.
Exact config SHA-256:
`100965eb86c259ab33c94679c55d36bbed1f387def40054c73301c90cb6e8d4b`.

Offline source admission succeeds at the historical trusted clock
`2026-10-08T12:15:04Z`. The old composition then returns `Candidate`:
the Replay callback captured one BO `Market Sell` entry while execution
eligibility was false. The candidate is the M10 ending at 12:10 UTC
(model start label 12:00 UTC), OHLCV 2317.5/2318/2316/2316/2741.
This is a deterministic callback-policy mismatch, not a credential failure,
missing M1, reason to relax source validation, or evidence of a broker order.

`Paper + LiveOnly` deliberately permits hypothetical intents in the ordinary
strategy callback. The first-boot contract instead requires one Replay
observation with zero intents. Earlier flat-price fixtures did not trigger an
entry and therefore missed this mismatch.

## Narrow correction

Only the existing crate-private authenticated first-boot Replay facade selects
the new observation path. Shared bar preprocessing still updates day aggregates,
last processed/close values, configured shadow state and indicator warmup.
The BO engine receives `warm_bar`; entry/exit evaluation, deferred actions,
repair intents, pending orders and request-ID allocation are not reached.
Intents are **not** generated and discarded after mutation.

The observation callback rejects non-Replay/nonfinal/non-M10, wrong instrument,
non-paper/non-LiveOnly, live authorization, exposure and existing pending/owner
state before mutation. The enclosing admission/freshness/chronology/tick checks
and the post-callback zero-intent/Replay/non-executable requirements remain.

Ordinary `Strategy::on_bar` and broker callbacks still select the original
path. Paper simulation, Live, backtest, model parameters, sparse M10 admission,
timestamps, riskgate policy and all persisted wire formats are unchanged.
The production inventory is refreshed to the changed source/test bytes only;
no workflow, ruleset, checker policy or historical accepted gate is changed.

## Evidence and reproduction

Run from the extracted source tree (Cargo cache/toolchain required):

```sh
python3 scripts/stage8b_p1_first_boot_replay_gate.py \
  handoff-evidence/first-boot-replay/public-failure-inputs.stdout \
  /absolute/new/qualification-directory
```

The retained-input test pins both original byte hashes. Only `durable_parent`
is relocated to a fresh local temporary directory, outside the operational
identity. The key is a public synthetic fixture; no systemd credential or
ceremony is read. Preparation, authenticated export/restore, durable first
boot and ordinary restart must all pass. Both initial and restarted roots
must satisfy the accepted one-callback/zero-request/zero-journal predicate.

Always-on model tests exercise BO Sell and Buy triggers, no phantom pending
or consumed direction, same indicator projection, a subsequent normal Live
signal, and eight invalid-input cases rejected before state mutation.
The full runtime-core library suite, targeted durable first-boot/source/recovery
tests and strict all-target/all-feature Clippy accompany this source review.
The broader durable suite was deliberately interrupted while executing the
unchanged P1-d4 SIGKILL matrices (over 13 minutes); it is **not** claimed PASS.
Its partial log and interrupted result are retained as `local-gates-3`.
`local-gates-1/2` retain test-construction compile errors, not operational
attempts. Final qualification is recorded separately in `local-gates-4`.
An isolated `linux/amd64`, network-disabled container also links a standalone
probe to the normal **release/default-feature** libraries, not fixture/test
builds. It uses the unchanged public source and config at the production fixed
path, with the required synthetic file custody, and completes first boot and
restart with one callback, zero requests and zero journal frames. This is a
library-path qualification, not a rebuilt/accepted installation ELF package.
Inherited tests can use disposable local Redis; no operational Redis/VPS
activation is part of this gate. A deliberately reverted first-boot route is
also replayed against the retained input as a red control.
That control reverts the callback route only (patch retained in evidence); it
is not presented as a rebuild of the entire historical baseline ELF.

## Next boundary

Source review first, then the full Linux O2 artifact/build qualification and
non-activating installation with FAILED/1/23 preserved. A new bounded phase
requires separate authorization; no timer is armed here. O2 remains HOLD.
Continuous paper, WS subscription/reconnect/freshness/EOD checks, multi-session
ALOR operational parity and live micro remain later roadmap work.
