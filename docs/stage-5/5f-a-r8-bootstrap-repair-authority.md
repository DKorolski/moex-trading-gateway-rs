# Stage 5F-a-r8/r8a/r9 — bootstrap repair authority

## Status

R8 is merged as the generation-2 authority state. R8a is merged as the
governance-only generation-3 amendment. This generation-4 candidate consumes
their one named successor capability to repair the portable scanner boundary;
it does not claim Stage 5F external activation or open Stage 5F-b.

## Observed activation failure

The disposable clean PR executed on GitHub Actions job
[`CI / rust`](https://github.com/DKorolski/moex-trading-gateway-rs/actions/runs/30544769619/job/90878032039).
The runner did not provide `rg`/ripgrep. The first scan therefore printed
`rg: command not found` six times but reported success because those calls
were shell conditional commands. The negative harness then correctly returned
non-zero: 80 of 87 cases passed and these seven scanner-dependent mutations
were not detected:

```text
same-module-extra-delete
generic-method-post
generic-method-delete
route-string-bypass
non-reqwest-client-abstraction
wrong-module-post-delete
runtime-command-consumer-bypass
```

This failure is fail-safe at the harness boundary, but it proves the scanner
must not rely on an undeclared runner binary.

## r8 scope

r8 advances the Stage 5F authority state from generation 1 to generation 2.
It changes the protected-base contract, its Git-tree negative matrix and the
corresponding governance evidence only. It deliberately leaves all of the
following untouched:

```text
.github/workflows/ci.yml
scripts/forbidden_surface_scan.sh
all crates/** and Cargo files
Redis, FINAM, transport, dispatch, persistence, runtime-live and broker execution
```

## R8a freeze-rebind authority and exact r9 capability

The first R9 preparation exposed a contract collision: the R9 allowlist
permitted a portable worker/harness repair, while the immutable Stage 5D
freeze still bound their exact SHA-256 values. R8a resolves that collision
without changing either freeze artifact. It advances the authority state from
generation 2 to generation 3 and retargets the one named R9 successor to that
exact R8a state.

The resulting generation-3 contract permits one named successor only:
`5F-a-r9-portable-forbidden-scanner`. That candidate must replace the scanner
implementation while keeping its `100755` Git mode. It can alter only the
explicit path set in the authority-rotation protocol: the scanner, its two
negative-harness files, the paired Stage 5D freeze manifest/checker, and the
necessary Stage 5F governance/documentation bindings. It cannot alter either
GitHub Actions workflow, Cargo/Rust or an operational surface.

The capability is accepted only from the exact reviewed predecessor: base
`schema_version = 1`, `authority_generation = 3` and
`stage = 5F-a-r8a-stage5d-freeze-rebind-authority`. Its manifest must advance
from generation 3 to 4 and its candidate state must be the r9 stage. A
repeated r9 stage from generation 4 or later, a different generation-3 stage,
or a later state that merely reuses the R8a stage name is rejected before the
special allowlist is considered.

Before any of those comparisons, the contract requires exact JSON integer
types for the base, candidate and manifest schema/generation fields. In
particular, `true`, `1.0` and `4.0` are rejected rather than treated as Python
numeric aliases for `1` or `4`. A malformed float generation is also rejected
as a future rotation base, so it cannot create a governance deadlock.

The R9 implementation removes the implicit ripgrep dependency by using the
already-required Python 3.11+ path for the six text scans and by making the
worker's infrastructure detection portable. It retains the current negative
markers. The complete 87-case negative harness passed with `rg` absent from
`PATH`; that demonstrates the local hosted-runner equivalence without opening
CI, Rust or an operational surface.

The contract's negative matrix proves the one-shot property with a valid first
R9 transition, accepts only the two paired Stage 5D freeze-rebind paths, and
rejects: R9 replay from generation 4, invocation from an arbitrary later
generation, a different generation-3 base stage, a rolled-back R8a stage-name
spoof, numeric type smuggling in the candidate or manifest, a float state used
as a later base, missing scanner change, mode drift, scope creep, another Stage
5D freeze path, and a scanner change through a generic rotation. After this
transition the state is generation 4, so the exception is no longer callable.

## Required acceptance sequence

1. R8a was independently reviewed and merged through the protected-base
   authority check; canonical `rust` remained expected-red because R8a did not
   change the scanner.
2. R9 must be accepted by its protected-base check and pass canonical `rust`
   and `redis-smoke`; the local 87-case no-`rg` result is evidence, not a
   substitute for that hosted result.
3. Configure/verify the no-bypass rule for `main`: PR, independent approval,
   strict up-to-date branch, `base-authority`, `rust` and `redis-smoke`.
4. Run a clean disposable PR and adversarial authority-drift PR. Only then may
   Stage 5F-b begin.
