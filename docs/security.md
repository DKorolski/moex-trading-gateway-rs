# Security policy

## Secrets

Never commit:

- Finam secret token;
- JWT/access tokens;
- account ids in public examples if they identify live accounts;
- broker reports containing personal data;
- private keys or `.env` files.

Use local `.env` files only for development and keep them ignored by git.

`.env.example` may include empty variable names such as `FINAM_SECRET_TOKEN`,
`FINAM_ACCOUNT_ID`, and `FINAM_SYMBOL`; it must never include real values.

## Logging

Logs may include:

- token fingerprints;
- account aliases;
- normalized symbols;
- client order ids;
- broker order ids.

Logs must not include:

- raw secret token;
- raw JWT;
- full personal account identifiers unless explicitly required for local-only diagnostics.

Operator CLI probes should prefer redacted response shapes and key lists over
full broker payloads until an explicit export workflow is added.

Rust structs containing secret or JWT values must not derive raw `Debug`.
If a debug implementation is required, it must expose only presence/length or a
non-reversible fingerprint.

## Live trading guard

Order-emitting functionality must require:

- explicit config flag;
- explicit account id;
- explicit strategy id;
- readiness = live-ready;
- operator pause not active;
- idempotent client order id.
