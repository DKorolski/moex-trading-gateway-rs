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

## Handoff content scan

`scripts/make_handoff_archive.sh` refuses to build an external handoff archive
if tracked or local included files contain known live-like portfolio/account
literals, FINAM token prefixes, or JWT-like strings. Synthetic examples should
use names such as `ACC_TEST_0001`, `ACC_DYNAMIC_TEST_001`,
`ORDER_DYNAMIC_TEST_001`, and `SYNTH@TEST`.

Public instrument symbols are not secrets by themselves, but tests, config
templates, and handoff examples should still prefer synthetic values such as
`TICKER@MIC`, `TESTFUT@TEST`, and `INTERNAL_TEST_FUT`. Real instrument symbols
belong only in domain documentation where they are the explicit subject of API
characterization or migration planning.

Broker-native order comments can contain operator/broker context. Broker-neutral
Redis snapshots must redact raw comments by default and may expose only a
non-reversible `comment_fingerprint`. Raw comment export requires a separate
local-only broker-truth/debug workflow.

Runtime-bridge DLQ records must not store raw Redis payload text. Store only
stream name, entry id, reason class, payload length, and optional future
non-reversible fingerprints.
Typed expected/actual diagnostics are allowed when represented as enum/type
names, not raw decoded values.

CLI command argument containers should not derive auto `Debug`, because account
ids and venue symbols can be supplied as args or environment-derived values.

## Live trading guard

Order-emitting functionality must require:

- explicit config flag;
- explicit account id;
- explicit strategy id;
- readiness = live-ready;
- operator pause not active;
- idempotent client order id.
