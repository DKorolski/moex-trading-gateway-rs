# Stage 8A-4 I1 — additive V2 codec and mixed replay

## Authority

I1 implements only the slice opened by the independently accepted durable
composition implementation specification R2 at
`dd01253596527d6cff1db11cc32ae3c3348c96a0`.

The accepted review SHA-256 is
`acb8364ee2100bf64e50522823b1da21093f96c73f93b20b4cdf9e7ac09b58ec`.

## Implemented boundary

- Stage 6 owns dedicated V2 reconciliation persistence DTOs. It does not
  depend on `finam-gateway` and does not serialize private FINAM types.
- `Stage6JournalRecordV2` uses the immutable V1 request/sequence record-ID
  derivation and the unchanged `S6F1` frame/storage format.
- record dispatch accepts exactly schema 1 or 2 after frame integrity checks;
  malformed, duplicate, unknown, non-canonical and invalid V2 bodies fail
  closed without V1 fallback.
- V2 is read-only and has no public constructor or generic `Deserialize`
  implementation. The only production creation route is canonical decoding.
- mixed replay advances the per-request causal frontier on V2, retains the
  complete typed V2 fact and exposes an incomplete/complete pending suffix
  batch.
- every following V1 suffix record must match event kind, record ID, sequence,
  payload hash and full canonical-record hash before normal V1 semantics run.
- exact duplicate V2 records are idempotent; same stable key with different
  canonical content, unexpected suffix, source/causal drift and V2 after
  finalization fail closed.
- 20 canonical SHA-256 goldens freeze PLACE/CANCEL, fill, optional IDs, both
  holds, all lookup variants, mixed replay states, unknown schema and V1 bytes.

## Deliberately closed

I1 contains no V2 writer, append API, composition owner, apply/CAS, suffix
repair writer, covering-seal writer, ACK/readiness publisher, Redis live
consumer, FINAM transport, broker dispatch, retry/re-arm, runtime-live or real
order path.

I2, I3 and I4 remain separately review-gated. Acceptance of I1 opens only an
I2 private composition-builder slice; it does not authorize execution.

## Inherited tooling limitation

The legacy root `forbidden_surface_scan.sh` is not an I1 acceptance gate: its
compilation-control baseline predates the already accepted
`runtime-command-bridge` and `runtime-durable-service` workspace members and
therefore reports those inherited Cargo/member edges as drift on the exact R2
predecessor as well. I1 does not alter Cargo files, workspace members or those
edges. The I1 semantic checker enforces an exact changed-path allowlist and the
gate separately runs inherited R2 validation, workspace compilation, complete
strategy-runtime-core tests, compile-fail doctests and clippy. Updating the
legacy global baseline is intentionally deferred to a dedicated governance
slice rather than mixed into the V2 persistence schema implementation.
