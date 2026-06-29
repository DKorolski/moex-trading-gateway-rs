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
scripts/make_handoff_archive.sh
```

The script creates a zip under `reports/handoff/` and excludes local artifacts.
`reports/` is git-ignored.

If a reviewer needs probe evidence, send a manually approved redacted fixture
separately, not the whole `tmp/` directory.
