# Stage 5G-e-d-c — fresh broker truth application contract

Accepted predecessor: `2b2bcc671c68722b3b84b914b785ffcb83f6802d`.

This slice consumes one accepted Stage 5G-e-d-b reduction and either applies
its opaque owned candidate once, continues without mutation, or returns the
original authenticated restart authority as a typed block.

## Boundary

The only owning entry is crate-private:

```text
Stage5gFreshTruthReduction
  -> apply_stage5g_fresh_truth_reduction
  -> Applied | Continued | Blocked
```

`Applied` uses the existing canonical Stage 5G order/trade/position state
transition. It then proves candidate/post-state semantic equality, exports an
authenticated Stage 5D/5G package, consumes the source post-state, restores in
a fresh runtime instance, and returns only that restored capability.

`Continued` and `Blocked` never expose a candidate or post-state authority.
Injected failures at all fourteen internal boundaries return the exact
pre-application restart package fingerprint.

The ordinary active-session canonical path still rejects broker evidence after
a terminal ACK. Restart-only application may admit a same-status monotonic
Canceled/Expired late fill only after the accepted e-d-b reducer has formed an
owned GRST11 candidate; the evidence is then independently revalidated by the
same canonical order/position core.

## Replay policy

Policy B is selected. `ExactReplay` remains disabled. This stage adds no
durable applied-tuple ledger and makes no crash-durability claim beyond the
authenticated, persistable package bytes. External CAS/fsync/anti-rollback
ownership remains Stage 6 work.

## Closed surfaces

- strategy callbacks and bar/timer execution;
- Redis and command consumption;
- FINAM, HTTP and broker dispatch;
- runtime-live and real orders;
- protective execution and Stage 5G-f;
- external persistence/journal implementation.

## Evidence

`Stage5gFreshTruthApplicationEvidenceV1` is included in the authenticated
restart projection. Candidate and applied semantic fingerprints must be equal;
`runtime_transition_applied` is true, while `callback_invoked`,
`transport_opened` and `exact_replay_enabled` are false.

Focused source-produced fixtures cover Working application, terminal
Filled/Rejected/Canceled/Expired continuation, Canceled/Expired late fill,
timer continuation, blocked pre-ACK/generated escrow, package tamper, row-order
and parallel determinism, and the fourteen-point failure matrix.
