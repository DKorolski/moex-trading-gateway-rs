# Finam read-only fixtures

Read-only fixtures are used only for API characterization before typed DTOs and
gateway lifecycle work. They must never contain raw FINAM payloads, JWTs, secret
tokens, or live order commands.

## Safe capture command

Run this only after the real FINAM secret token is available locally in the
shell environment:

```bash
FINAM_SECRET_TOKEN=... \
FINAM_ACCOUNT_ID=... \
FINAM_SYMBOL='TICKER@MIC' \
cargo run -p broker-cli -- finam-readonly-check \
  --start-time 2026-06-01T00:00:00Z \
  --end-time 2026-06-27T23:59:59Z \
  --limit 1000 \
  --output tmp/finam-readonly-redacted.json
```

`tmp/` is ignored by git. Move a fixture out of `tmp/` only after manually
checking that it contains redacted shape records only.

## Fixture content

The fixture format is `finam-readonly-redacted-v1`. Records contain:

- probe name;
- success/failure flag;
- structured error kind and redacted transport/API error description, if any;
- JSON shape metadata for successful responses.

The JSON shape keeps safe schema-like object field names, array lengths, item
kinds, and a bounded first-item shape. Dynamic/map-like object keys are redacted
to key kind, key length, and SHA-256 fingerprint. The fixture does not keep
scalar values such as account ids, order ids, prices, JWTs, comments, broker
symbols used as map keys, or broker-native error text.

## Current safety gates

Allowed:

- `finam-auth-check`;
- `finam-readonly-check`;
- saving redacted shape fixtures under `tmp/`;
- using redacted fixtures to implement typed DTOs/mappers.

Both CLI checks obtain JWTs through `FinamAuthManager`; this keeps the renewal
boundary in place while M1 remains read-only.

Do not include the whole `tmp/` directory in review handoffs. If fixture
evidence is needed, manually inspect and approve a specific redacted fixture and
send that file separately.

Not allowed as part of fixture/probe work:

- command consumer or ACK lifecycle;
- order placement/cancel;
- runtime adaptation;
- live micro;
- stop/SLTP/bracket work beyond API research.

M2a may use these read-only DTOs and redacted shape assumptions to publish
shadow Redis health/readiness and broker-truth snapshots, but it must not enable
order commands or live trading behavior.
