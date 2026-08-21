# Stage 5F terminal authority retirement

This generation-5 rotation retires the Stage 5F protected-base workflow from
current-tree enforcement. It is a governance-only transition required before the
canonical CI can move to the modern cumulative authority.

The historical workflow remains available through `workflow_dispatch` for explicit
immutable-ref evidence. It has no `pull_request_target`, `pull_request`, or `push`
trigger and therefore cannot block or authorize later current-tree changes.

The rotation deliberately leaves `.github/workflows/ci.yml` byte-identical to the
accepted base. It changes no Cargo file, Rust source, FINAM transport, Redis consumer,
broker dispatch, runtime-live path, or real-order authority.

After this retirement is independently accepted and merged, a separate GOV-CI-1B
package may replace canonical CI. Stage 8B-D R2 and Stage 8B-S remain closed.
