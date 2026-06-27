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

Secret-bearing token types must not implement `Serialize` unless the
implementation is explicitly redacted. Use `SecretToken` for the FINAM portal
secret and `AccessToken` for JWT/access tokens.

Broker HTTP error bodies must not be printed by default. Store and print only
redacted metadata such as HTTP status, JSON shape, top-level keys, body length,
and non-reversible hash. Raw response capture requires an explicit local-only
debug/export workflow.

Transport errors must be presented through a redacted formatter before CLI
output or external sharing, because raw HTTP client errors may include URLs.

## Live trading guard

Order-emitting functionality must require:

- explicit config flag;
- explicit account id;
- explicit strategy id;
- readiness = live-ready;
- operator pause not active;
- idempotent client order id.
