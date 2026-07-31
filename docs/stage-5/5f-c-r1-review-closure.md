# Stage 5F-c R1 — review closure

Status: review candidate; seven-row characterization remains non-golden
Development predecessor: `11826285d05638b6b0e29c64a3435870091dac38`
B3F immutable source: `e14654f7129aa61011931306140a3bfefe2fcfbc`

## Outcome

R1 closes the findings against the rejected Stage 5F-c candidate without
starting Stage 5F-d. The accepted execution scope remains exactly:

```text
F01 F02 F04 F24 F31 F32 F33
```

The result is still
`candidate_source_characterized_not_golden`. Redis, FINAM transport,
POST/DELETE, broker send, dispatch, runtime-live, feedback lifecycle and
protective execution remain closed.

## Canonical v2 evidence

The immutable v1 catalogs and correction overlay remain historical rejected
evidence. R1 uses only `tests/fixtures/stage5/stage5f/v2/`:

- 34 strict scenario records with event/lifecycle/callback clock ownership;
- 14 typed public/private runtime-state seeds;
- production-equivalent ten-character cycle identities;
- four riskgate authority seeds;
- one target config bound to the accepted IMOEXF High180 profile.

The v2 path has no correction overlay. F02 is source-valid after the BO wait
window; F04 has source-valid stop2 OHLC; the pending-entry seed reflects source
runtime ownership before fill. All 14 seeds materialize, export and rematerialize
exactly with stable public state, private extension and config fingerprint.

## Test-only boundary

Every Stage 5F seam is a direct `#[cfg(test)] pub(crate) mod
stage5f_test_seams` or the exact immutable callback observer statement.
The SHA-bound manifest covers:

- observer module and exact callback call;
- Stage 5C ownership/history/recovery bridge;
- Stage 5D full-restart oracle;
- Stage 5E callback-validation and B3C bridges.

Removing the marked regions restores the accepted production sources. The
checker rejects unguarded items, widened visibility, extra marker items,
macro/cfg_attr activation, source drift and manifest rebinding.

## Full-chain equivalence

Four representative tests execute the actual typed chain:

```text
canonical Stage 5D restart package
  -> strict decode and private apply
  -> Stage 4 broker-truth bootstrap
  -> authoritative riskgate injection
  -> runtime-state-restored
  -> Stage 5C history and pending recovery
  -> Stage 5E B3C authority
  -> exact source callback
  -> canonical settlement
```

The cases are flat F01, non-flat owner/cycle F04, pending-entry F26 and the F24
typed `LedgerEvidenceInvalid` blocker. Equality covers target binding,
canonical config, public/private state, recovery indexes, riskgate
identity/generation/frontier and lifecycle watermarks.

## Seven-row candidate

The versioned candidate is
`docs/stage-5/stage5f-c-r1-candidate-results.json`. It records one callback
and one settlement for accepted source rows, exact ordered request identity,
stable redacted fingerprints, and no accepted post-state/vector for terminal
rows. F24 is now produced by the actual Stage 5D typed authority validator,
not by a manual string branch. A dedicated test runs the seven-row source
characterization twice and requires byte-identical serialized output.

## Acceptance gates

R1 is handed off only after:

```text
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/stage5f_controlled_characterization_check.py
python3 scripts/stage5f_controlled_characterization_negative_harness.py
bash scripts/stage5f_inherited_b1_snapshot_gate.sh
bash scripts/stage5f_b3f_snapshot_provenance_gate.sh
```

The commit-bound package is built only from a clean committed tree:

```text
bash scripts/make_stage5f_c_r1_handoff_archive.sh
python3 scripts/stage5f_c_r1_handoff_safety_check.py \
  --archive reports/handoff/moex-trading-project-<commit>.zip
```

The versioned archive carries the complete tracked source tree, raw Git commit
object, source path/mode/hash manifest, rustc/cargo versions and stdout/stderr
for every R1 gate, including a successful preseal archive-safety run. The
checker reconstructs the Git tree and commit identity from archived bytes and
rejects undeclared members, secrets, symlinks and missing evidence.

Stage 5F-d remains closed until independent R1 acceptance.
