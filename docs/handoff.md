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
  required_gate_names
```

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
