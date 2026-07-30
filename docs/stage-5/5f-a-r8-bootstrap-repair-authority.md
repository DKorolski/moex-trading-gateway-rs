# Stage 5F-a-r8 — bootstrap repair authority

## Status

`Review required`. This is a governance-only generation-2 authority rotation;
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

## Exact r9 capability

The resulting generation-2 contract permits one named successor only:
`5F-a-r9-portable-forbidden-scanner`. That candidate must replace the scanner
implementation while keeping its `100755` Git mode. It can alter only the
explicit path set in the authority-rotation protocol: the scanner, its two
negative-harness files, and the necessary Stage 5F governance/documentation
bindings. It cannot alter either GitHub Actions workflow, Cargo/Rust or an
operational surface.

The capability is accepted only from the exact reviewed predecessor: base
`schema_version = 1`, `authority_generation = 2` and
`stage = 5F-a-r8-bootstrap-repair-authority`. Its manifest must advance from
generation 2 to 3 and its candidate state must be the r9 stage. A repeated r9
stage from generation 3 or later, a different generation-2 stage, or a later
state that merely reuses the r8 stage name is rejected before the special
allowlist is considered.

The intended r9 implementation removes the implicit ripgrep dependency by
using the already-required Python 3.11+ path for the six text scans and by
making the worker's infrastructure detection portable. It must retain the
current negative markers and demonstrate the scanner works with no `rg` in
`PATH` before GitHub CI is treated as repaired.

The contract's negative matrix proves the one-shot property with a valid first
r9 transition and rejects: r9 replay from generation 3, invocation from an
arbitrary later generation, a different generation-2 base stage, a rolled-back
r8 stage-name spoof, missing scanner change, mode drift, scope creep and a
scanner change through a generic rotation.

## Required acceptance sequence

1. Independently review and merge r8 through the protected-base authority
   check; canonical `rust` remains expected-red until r9 because r8 does not
   change the scanner.
2. Review and merge r9 only if its protected-base check accepts the exact
   bounded scope and its canonical `rust` and `redis-smoke` checks pass.
3. Configure/verify the no-bypass rule for `main`: PR, independent approval,
   strict up-to-date branch, `base-authority`, `rust` and `redis-smoke`.
4. Run a clean disposable PR and adversarial authority-drift PR. Only then may
   Stage 5F-b begin.
