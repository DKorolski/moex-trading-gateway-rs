# M3j-2 fresh read-only FINAM evidence package

M3j-2 closes the fresh read-only evidence slot for first-live-micro preparation.

This stage is still read-only and pre-live. It does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg;
- first live micro.

## Required read-only observations

The M3j-2 package requires fresh, redacted evidence for:

- account / account details;
- orders snapshot;
- GetOrder if applicable;
- trades;
- positions;
- schedule / session;
- instrument params.

## Required safety conclusions

The M3j-2 stage report can pass only when all of these are true:

- account allowlist has exactly one entry;
- symbol allowlist has exactly one entry;
- no unknown active orders;
- no orphan active orders;
- position is flat or explicitly expected;
- schedule/session is loaded;
- instrument params are validated;
- broker-truth snapshots are fresh;
- no raw token is exported;
- no raw account id is exported;
- no raw client order id is exported;
- no raw broker order id is exported;
- no raw position payload is exported.

## Runtime evidence source

The controlled runtime artifact should be generated with the existing GET-only
operator command:

```bash
set -a
source .env
set +a
cargo run -p broker-cli -- finam-real-readonly-evidence \
  --output reports/finam-real-readonly-evidence/m3j2-redacted-evidence.json
```

The generated artifact stays under `reports/` and is not included in clean
source handoff archives.

## Boundary interpretation

`m3j2_fresh_readonly_evidence_ok = true` means the read-only pre-live evidence
slot is satisfied.

It still does not mean live readiness. The M3j-2 report emits:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

## Next steps

M3j-3: one-symbol dry shadow session report.

M3j-4: explicit pre-live NO-GO/GO decision package.
