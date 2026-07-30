# Handoff packaging

Review handoff archives must not include local runtime artifacts.

Do not include:

- `.env` or other local env files;
- `tmp/` probe outputs/logs;
- `target/`;
- `reports/`;
- raw broker payloads;
- raw secrets, JWTs, account/order/trade payloads, or logs.

Use:

```bash
scripts/stage5d_b2bc_review_gate.sh
scripts/make_handoff_archive.sh
```

The packaging script fails closed when the Git tree is dirty, creates a zip
under `reports/handoff/`, validates source and ZIP safety, and excludes local
artifacts. `reports/` is git-ignored.

Each archive contains exactly generated provenance files:

```text
handoff-commit.txt
  source_commit=<short_sha>
  source_ref=<full_sha>
  archive_name=<archive_filename>

handoff-manifest.json
  schema_version
  review_stage
  source_commit/source_ref/archive_name
  created_at_utc
  Stage 5C/Stage 5D checker and Stage 5D manifest SHA-256 values
  current_review_stage and exactly one active stage-specific provenance set
  (Stage 5E or Stage 5F checker/inventory/plan/gate SHA-256 values)
  source_tree_manifest_sha256
  required_gate_names
```

When `current_review_stage` is a Stage 5E stage, the archive also contains:

```text
handoff-stage5e-gate-result.json
handoff-stage5e-gate-stdout.txt
handoff-stage5e-gate-stderr.txt
handoff-source-tree-manifest.json
```

The archive safety checker validates the Stage 5E checker, inventory, plan and
fresh gate-result hashes, the `current_review_stage`/inventory stage binding,
the Stage 5D baseline ref, closed-surface markers and the gate-result
`source_ref`. The gate-result also records the exact input hashes used by the
successful gate run and a design-scope digest; archive safety binds those values
back to the manifest and actual archive members. The source-tree manifest binds
the exact archive file set to tracked source members plus generated handoff
artifacts, checks each tracked member SHA-256, and reconstructs the Git tree hash
from archive bytes and tracked file modes.

## Stage 5F inherited-closure handoffs

Stage 5F uses a distinct active descriptor. The accepted Stage 5E-B3F descriptor
is deliberately retained as an immutable closure descriptor: moving it would
make its exact accepted review scope disappear. A Stage 5F archive therefore
contains instead:

```text
handoff-stage5f-gate-result.json
handoff-stage5f-gate-stdout.txt
handoff-stage5f-gate-stderr.txt
handoff-stage5f-negative-result.json
handoff-stage5f-negative-stdout.txt
handoff-stage5f-negative-stderr.txt
handoff-stage5f-ci-negative-result.json
handoff-stage5f-ci-negative-stdout.txt
handoff-stage5f-ci-negative-stderr.txt
handoff-source-tree-manifest.json
```

The Stage 5F gate first creates an ephemeral `git archive` snapshot of accepted
ref `e14654f7129aa61011931306140a3bfefe2fcfbc` and runs the immutable B3F
checker plus its production UI harness there. It then checks the Stage 5F
descriptor, IMOEXF canonical-final-M10 paper-only contract, full atomic
BO/MR/riskgate scenario matrix and all closed later-stage surfaces. Archive
safety binds the B3F source pins, both Stage 5F gate logs and the isolated
Stage 5F negative harnesses to the tracked source-tree manifest. The builder
and canonical CI both use `stage5f_b3f_snapshot_provenance_gate.sh` for the
B3F 580-case provenance harness. It creates a detached clone at the accepted
ref, fails closed when that commit is unavailable or the checkout drifts, and
requires exactly 580 `PASS` rows. The frozen Stage 5E gate and the raw B3F
provenance harness must not run against a Stage 5F head.

For Stage 5F-a-r3, the canonical CI executes the verified B3F snapshot runner
before any Stage 5F repository-owned harness. Archive safety additionally
compares the archived canonical CI workflow, shared B3F provenance wrapper,
Stage 5F gate, snapshot verifier and both Stage 5F negative harnesses with the
accepted authority SHA-256 values. Current-tree hashes are retained as evidence
but are not an authority source. The builder repeats this complete authority
check immediately before it creates the source-tree manifest; archive safety
then repeats it from archive bytes. The Stage 5F CI negative result contains
sixteen mutations, including skipped/non-blocking CI steps, Actions-only
wrapper replacement, forged harness output and wrapper mutation before verified
execution.

The script also creates the external sibling
`moex-trading-project-<short>.zip.sha256`. The archive hash is deliberately not
stored inside the archive. Reviewers can run
`python3 scripts/handoff_safety_check.py --archive <zip>` to verify paths,
duplicates, symlinks, exclusions, live-like literals and marker/manifest
binding.

For Stage 5D-b2b-c1 CI/review requires the Stage 5C checker, Stage 5D checker,
normal forbidden scanner, full forbidden negative harness, all 44 isolated
Stage 5D negative mutations, no-Redis smoke, fixture parsing, copied-baseline
completeness, source safety, and workspace fmt/test/clippy.

If a reviewer needs probe evidence, send a manually approved redacted fixture
separately, not the whole `tmp/` directory.
# Stage 5E descriptor selection

For Stage 5E handoffs the builder reads the explicit active-descriptor registry
and selects exactly one registered descriptor. The selected inventory supplies
the plan, checker, baseline ref and exact changed-path set recorded in the gate
result. Unknown, missing or mixed descriptors are rejected by archive safety.

# Stage 5F descriptor selection

For Stage 5F handoffs the builder selects exactly one registered Stage 5F
descriptor. It does not mutate the accepted Stage 5E-B3F descriptor. The Stage
5F inventory supplies the current plan, checker, baseline ref and exact
changed-path set, while its accepted-B3F closure block supplies the immutable
predecessor pins checked by both the gate and archive safety.
