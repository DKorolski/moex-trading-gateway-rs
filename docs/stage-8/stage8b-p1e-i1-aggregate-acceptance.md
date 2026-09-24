# Stage 8B-P1-e I1 aggregate acceptance

Status: **REVIEW CANDIDATE — I1 NOT CLOSED**.

This governance/evidence-only boundary follows independent acceptance of the
process source (`1086b8d`), production telemetry (`b6f6d5b`) and fixed-path
installation/systemd material (`7f2e876`). It changes no Rust, Cargo,
deployment unit, configuration or workflow file and grants no operational
authority.

## Accepted component chain

The machine-readable inventory binds all fourteen milestones already frozen by
the aggregate-readiness package and appends the two independently accepted
completion slices:

- telemetry source/material at `b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac`;
- fixed-path installation/systemd source/material at
  `7f2e876c4cad7a3a4a0fa10a1eb5202e58202d2f`.

Held intermediate commits are historical development points and are not
promoted. The accepted release binary used by the target-Linux rehearsal is
built from `b6f6d5b`; the installation correction only hardens the installer,
runner and evidence boundary.

## Aggregate evidence

The aggregate gate runs from one clean immutable commit and requires:

1. exact lineage, document and closed-surface validation;
2. aggregate negative mutations;
3. I1A and transaction V5 negative harnesses plus committed-restart, P1-d4,
   process, telemetry and fixed-install source/negative gates;
4. the retained 21-file Ubuntu 24.04/systemd 255 installation evidence and
   the 14-case behavioral filesystem matrix;
5. full `strategy-runtime-core` and `runtime-durable-service` library tests,
   doctests, workspace formatting and strict all-target/all-feature Clippy.

P1-c real-Redis behavior is covered by the full durable-service suite and uses
only an isolated subprocess Redis instance created by the tests. It does not
contact operational DB0/DB15. Target-Linux evidence is retained evidence from
the accepted non-activating rehearsal; this aggregate slice does not repeat an
installation or start a service.

## Decision boundary

This commit is not self-accepting. Only independent review of the immutable
handoff may change I1 to `CLOSED / ACCEPTED`. Until then all of the following
remain closed:

- operational Redis DB0 or DB15;
- VPS installation, service-manager reload, enable or service start;
- paper-provider operational activation;
- FINAM POST/DELETE/send and broker dispatch;
- runtime-live, real orders and P1-f.

After independent aggregate acceptance, the next separately designed and
reviewed slice is P1-f isolated operational acceptance. Aggregate acceptance
does not itself authorize that activation.
