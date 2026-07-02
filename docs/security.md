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

FINAM bar finality golden-check output must stay redacted. It may include
timestamp diagnostics, counts, symbol/account presence flags, and derived
timeframe consistency, but not secret tokens, JWTs, account ids, order ids, raw
broker payloads, or local `.env` values.

Runtime-bridge DLQ records must not store raw Redis payload text. Store only
schema version, timestamp, gateway source, consumer group/name, stream name,
entry id, reason class, payload length, and optional future non-reversible
fingerprints.
Typed expected/actual diagnostics are allowed when represented as enum/type
names, not raw decoded values.
`scripts/runtime_bridge_dry_smoke.sh` includes negative Redis DLQ checks for
invalid JSON and raw `Order.comment`, and verifies that DLQ records do not
contain the raw payload/comment text.
M2k DLQ summary metrics expose only latest reason, timestamp, stream, entry id,
and consecutive count; they must not add raw payload text to stdout or Redis
DLQ records.

M3 outgoing broker comments must follow the same redaction posture. First micro
defaults to no outgoing comment. If sanitized comments are enabled later, Redis
streams and durable mapping exports may store only length/SHA-256 fingerprints,
not the raw comment value.

Broker-neutral place-order commands must not carry raw `PlaceOrder.comment`.
Dry M3 preflight rejects any raw command comment; any future broker-native
comment must be generated inside the gateway through the sanitized outgoing
comment policy and persisted only as a fingerprint.

Command ACK reasons must remain safe structured codes. Do not publish raw broker
error bodies, account ids, broker order ids, secrets, JWTs, or arbitrary payload
text in `CommandAck.reason`. The M3a-5 dry Redis ACK publisher additionally
clears optional client/broker order ids before publishing `CommandAck` envelopes;
operator correlation must use `StrategyRequestId` plus the local durable mapping
store. M3a-6 keeps this as the future runtime-facing ACK direction; any
internal full-id operator view must be protected and separate from handoff or
runtime-facing exports. M3a-7 applies the same rule to dry cancel ACKs and to
accepted-without-broker-id ambiguity: ACKs expose safe status/reason codes, not
raw broker identifiers. M3a-8 applies the same rule to cancel accepted broker-id
mismatch diagnostics and recovery helpers: public ACKs/state docs may expose
safe reason codes, not raw returned broker ids. M3a-9 applies this to the
SQLite prototype redacted export: raw account, client-order, and broker-order
ids stay in the local protected store only, while public export surfaces use
length/SHA-256 fingerprints.
M3a-10 keeps that posture for read-only diagnostics and transition audit:
diagnostic/audit rows expose state, timestamps, safe reason codes, and
fingerprints only; raw ids remain local to the protected SQLite payload and must
not be included in Redis ACKs or handoff archives.
M3a-11 makes the boundary explicit in API names and gateway decisions:
operator-only SQLite diagnostic methods use the `operator_` prefix, runtime ACK
id policy is `RedactedRuntimeAckOnly`, and real endpoint gates remain blocked
until a later reviewed implementation.
M3b-0 keeps the same posture for response fixtures and future transport shape:
synthetic accepted responses can carry broker-order ids for mapping tests, but
debug/diagnostic output exposes only presence and length, and the future
transport trait requires an endpoint gate marker that is not constructible from
the current blocked decision or from a manually forged allow-looking decision
while the post-review approval constant remains false.
M3b-1 keeps endpoint response integration dry and redacted: rate-limit,
maintenance, and decode-error fixtures become safe enum reason codes,
order-path error kinds, and operator disarm signals. Runtime-facing Redis ACKs
still pass through the dry ACK publisher, which removes raw client and broker
order ids.
M3b-2 keeps local/mock HTTP endpoint characterization redacted by default:
local response `Debug` output records only status, body length, body kind, and
retry-after presence. Unauthorized and decode-error paths use safe enum reason
codes and do not expose raw broker body text in ACKs, diagnostics, Redis, or
handoff archives.
M3b-3 extends that boundary to internal endpoint result types: mapped and
classified endpoint results are not serde export payloads, and their custom
`Debug` implementations expose only safe presence/length/status/kind fields.
`FinamOrderEndpointResponseDiagnostic` remains the review/export surface.
M3b-12 applies the same rule to real-readonly broker-truth: FINAM GET response
bodies are captured privately for typed mapping, while exported route, HTTP, and
SQLite audit diagnostics contain only templates, query-key names, status, body
presence/length/SHA-256, id presence/length metadata, and safe enum reasons.
Rendered paths, query values, raw ids, and raw bodies must not be stored in
handoff artifacts.
M3b-13 makes the read-only enablement marker itself redacted: it stores only the
approved account id length and SHA-256 plus bounded timeout/rate-limit values.
Transport error categories and contract-probe reports are safe enum/shape
diagnostics only.
M3b-14 keeps operator-run evidence redacted as well: output locations are stored
as length + SHA-256 descriptors, audit mode is explicit, and transport error
categories are preserved as operator-action enums.
M3b-15 narrows CI POST allowlisting to exact auth/session functions and binds
real-readonly transport timeout/rate configuration to the approved run marker at
construction time.
M3b-16 extends that binding to the FINAM base URL hash and adds redacted
token/account preflight plus evidence-matrix fingerprints for controlled
read-only probe reports.
M3b-17 requires token readonly scope in the redacted preflight and builds
evidence rows from per-source attempt records with explicit send counters.
M3b-18 keeps that diagnostic redacted but moves approval to a non-serializable
marker, adds probe-run identity/fingerprints for audit correlation, and splits
captured-response counters from actual HTTP send started/completed counters.
M3b-19 binds the approval marker to the current redacted request snapshot,
exports only request/source-order hashes and lengths, and includes those
fingerprints in the probe-run identity.
M3b-20-pre adds explicit preflight freshness metadata and blocks stale markers
before any attempt, while evidence rows expose only safe actual-send booleans
instead of raw transport data.
M3b-21-pre requires an explicit operator-provided probe clock for enabled runs,
exports only computed preflight age, and adds local transport-like coverage for
started/completed send flags without any real FINAM probe.
M3b-22 adds a controlled one-shot real-readonly evidence command. The report is
redacted and GET-only; actual evidence artifacts stay under `reports/` and are
kept separate from source handoff archives.
M3b-23 adds self-contained evidence metadata, runs the forbidden-surface scan
before evidence collection, and adds timing/parsed-count diagnostics without raw
broker identifiers or bodies.
M3b-24 adds GetOrder 200 fixture closeout and pre-order gate policy while
keeping real FINAM order endpoints disabled and absent from the allowed
surface.
M3c-0 adds an explicit `real_order_endpoint_enabled = false` gate flag and a
serializable design report. The marker remains unconstructible and the
forbidden-surface scan still rejects accidental POST/DELETE leakage.
M3c-2 extends that report with self-contained evidence fields, a strict
checklist status vocabulary, future route allowlist data, and negative-test
plan entries. The generated evidence report remains diagnostic only and does
not authorize FINAM order endpoint calls.
M3c-3 binds that report to the supplied source archive contents by reading
`handoff-commit.txt` from the ZIP, adds explicit evidence/waiver slot handling,
and adds a negative forbidden-surface harness. These checks are preconditions
only; they do not enable order endpoints.
M3c-4 records the future implementation transition plan while keeping the
existing real endpoint transport trait as an approved-only compile contract.
Future route rendering and HTTP send must require `EndpointGateApproved`; the
marker remains unconstructible.
M3c-5 resolves the future crate boundary: `broker-finam` remains request-spec
and route-builder only, while future real order HTTP send is planned inside
`finam-gateway`, the crate that owns `EndpointGateApproved`.
M3c-6 adds the gateway-owned API shape module and scanner transition guard, but
the module is design-only and contains no HTTP send surface.

M3 dry order-path durable-store fixtures must remain local/synthetic. They may
persist broker-neutral request ids, derived client order ids, synthetic account
ids, instruments, order state, timestamps, and outgoing-comment fingerprints,
but must not persist FINAM secrets, JWTs, raw broker payloads, or raw outgoing
comment values.

SQLite order-path database, WAL, SHM, and writer-lock files are runtime
artifacts. They must not be included in review handoff archives. Use redacted
exports and the retention/archive policy in
`docs/order-path-retention-archive-policy.md` for review evidence.
M3a-11 hardens DB/WAL/SHM/writer-lock permissions where supported and requires
`umask 077` plus a protected local runtime directory for any future live-capable
deployment.
M3b-0 adds a runtime-directory inspector for future startup/deployment checks;
paths inside the workspace or artifact areas remain unsuitable for live-capable
SQLite runtime state.

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
