# Stage 8B-P1-f O2 — fresh materialization and isolated bootstrap package

Status: **R1 EXECUTION CONTRACT ACCEPTED — ARTIFACT PREPARATION ONLY; DO NOT EXECUTE**.

R1 closes `P1-O2C01` and `P1-O2C02` from the independent R0 review. The
contract was independently accepted at
`a9f8fe30a45752c943f9e399775322d83fcd8a36`; the acceptance document SHA-256
is `3342409133760dbb8ea909460310881eebdd91c797ed97f3879be4cfbb12ce1b`. It
defines unit-level bootstrap supervision and the exact read-only FINAM
method/route allowlist. Acceptance authorizes only construction of one
immutable execution artifact; it does not authorize O2 execution.

Accepted predecessor: O1 operational evidence at
`997e8a1d201048fcdec0e948660f32a0bee3cceb`; governance closure at
`e11744e31f11d567633716f21211c484bb9045eb`. This package changes no
production Rust, installed target byte or remote state. It freezes the exact
O2 actions and evidence required before a separately accepted execution.

## Scope

O2 creates one fresh first-boot input and one durable paper root. It does not
start the long-running P1 service, initialize Redis DB15, invoke a paper order
provider, send a broker command or attach FINAM order-execution/write
endpoints. The accepted read-only `GET /v1/accounts/{account_id}/orders`
snapshot is explicitly permitted as broker-truth input.

The execution is one authority transaction with two effects:

1. `P1F-O2-M` collects fresh read-only FINAM truth and commits the accepted
   source/config/materialized-set chain through
   `Stage8bP1fAuthorityStoreV1::materialize_o2`.
2. `P1F-O2-B` revokes network access and runs exactly
   `moex-finam-p1-paper-bootstrap.service`. The unit is an AF_UNIX-only,
   `PrivateNetwork=yes` one-shot invoking the accepted P1-e bootstrap command.

No second first-boot format, parser, durable-root creator or verification
framework is permitted.

## Package-time immutable inputs

The package binds:

- accepted O1 closure and exact installed binary SHA-256;
- fixed target host and SSH Ed25519 fingerprint;
- accepted P1-f guardian source and P1-e source-plan V2;
- exact O2 materialization policy and immutable supervisor-config template;
- exact installation-manifest SHA-256 observed after O1;
- exact bootstrap unit bytes and fixed paths;
- the public phase-authority key identity, but never its private key;
- scripts/binaries used for offline signing, target guardian operations,
  read-only materialization, one-shot bootstrap and evidence collection.

The reviewed package must be self-contained except for three deliberately
late inputs: the offline private authority key, the read-only FINAM token and
fresh FINAM/history bytes. None may appear in Git, a handoff ZIP, command-line
arguments, stdout or retained unredacted logs.

## Required thin operational facades

The O2 package may add only thin facades over accepted code:

- an offline signer that creates canonical signed genesis, activation and O2
  phase documents using the package-pinned authority identity;
- a root target guardian CLI that calls only the accepted
  `initialize_authority`, `activate_authority`, `claim_phase`,
  `materialize_o2`, `admit_active_phase`, `finish_phase` and `inspect` APIs;
- a read-only source materializer that emits exactly the accepted wire-V2
  first-boot source bundle, can call only the method/route allowlist below and
  cannot call Redis or any FINAM order-execution endpoint;
- an O2-B systemd-supervised runner that holds and polls the linear guardian
  permit while independently observing and controlling the exact bootstrap
  unit; and
- a read-only redacted evidence collector that has no lifecycle authority.

The facades may not reimplement signature verification, first-boot parsing,
runtime bootstrap, riskgate rebuilding or durable-root creation. The target
guardian and bootstrap runner are root-only. The paper supervisor remains the
only process running as `moex-p1-paper` during bootstrap.

## Execution sequence after separate acceptance

### 0. Fresh fail-closed preflight

Repeat the complete O1 post-install read-only probe. Require the accepted six
payload hashes and custody, P1 inactive/static with no process, no operator
material, uninitialized durable state, empty DB15, unchanged P0 unit
identities, at least 10 GiB free on `/`, and the exact target host key. Any
conflict stops before mutation.

### 1. Authority genesis and activation

After R1 design acceptance, but before the execution artifact is finalized,
the operator either creates the phase-authority private key on the designated
offline medium or reuses the explicitly selected offline key. The final
execution artifact contains the resulting exact public identity. Private bytes
remain offline and are absent from Git, handoff, target and logs. This R1
contract commit performs no key operation.

Before `open_production`, create the otherwise empty fixed control-root
skeleton `/var/lib/moex-finam-p1-paper-control` if absent, with exact custody
`root:moex-p1-paper 0750`, after validating its parent chain. If it already
exists, require the same custody and an empty permitted-entry inventory. Any
unexpected content, link, owner or mode fails before open/genesis. Skeleton
creation is not genesis and grants no phase authority; repeated genesis or
authority replacement remains forbidden.

Install only the package-pinned public identity on the target. The offline
registry records one Prepared generation and unique ceremony nonce.

The root guardian commits the signed genesis and emits a receipt. That receipt
is returned offline, byte-hashed and signed into the activation certificate.
Only the exact certificate may activate local generation 1. Repeated genesis,
an existing authority, a nonempty or invalid control root, an unexpected
history head or a different host / installation identity fails closed. A
valid empty control-root skeleton is permitted as specified above. No service
starts in this step.

### 2. O2 claim

The offline signer uses the exact activated head to issue one
`O2_MATERIALIZE_BOOTSTRAP` manifest. It binds source tree, host, generation,
next sequence, predecessor event, policy/template/install hashes, controller,
not-before and the original deadline. The guardian durably claims it before
FINAM collection. A second controller or different manifest fails before any
effect.

### 3. O2-M fresh materialization

The materializer receives the FINAM token through a transient root-owned
credential file. The facade validates the HTTP method, normalized route,
query-key inventory, exact configured account and exact `IMOEXF@RTSX` symbol
before a dedicated GET-only transport can send the request. The base URL is
exactly `https://api.finam.ru`; redirects and system proxies are disabled.
Host/IP filtering alone is not accepted as method enforcement.

The complete allowlist is:

| Method | Route | Permitted query keys | Purpose |
| --- | --- | --- | --- |
| GET | `/v1/accounts/{account_id}` | none | complete exact-account positions |
| GET | `/v1/accounts/{account_id}/orders` | none | complete account-wide active-order snapshot |
| GET | `/v1/assets/{venue_symbol}/params` | `account_id` | exact-account instrument parameters / price step |
| GET | `/v1/assets/{venue_symbol}/schedule` | none | explicit session windows |
| GET | `/v1/instruments/{venue_symbol}/bars` | `timeframe`, `interval.start_time`, `interval.end_time` | bounded final M1 history |

`account_id` must byte-match the accepted configured account and
`venue_symbol` must be `IMOEXF@RTSX`; bars use `TIME_FRAME_M1`. All unlisted
routes and query keys are denied before transport. Every method except GET is
denied, including POST, PUT, PATCH and DELETE. The orders-snapshot GET is
broker-truth observation, not order execution. Placement, replacement,
cancellation, exact-order mutation and every other order route remain closed.
Redis connections remain forbidden. In particular,
`active_orders_complete=true` and both zero order counts must come from the
complete GET snapshot; they may never be constants inferred from a flat
position.

It creates the wire-V2 source bundle from:

- complete flat broker truth for the exact configured account and
  `IMOEXF@RTSX`;
- at least 121 exact Moscow sessions of final M1 history aggregated through
  the accepted M1-to-M10 path and explicit session windows;
- at least 120 source-compatible high180 riskgate observations rebuilt and
  cross-validated by the accepted oracle;
- one later canonical final M10 candidate with exact ten-M1 provenance and
  zero callback intents.

The root guardian verifies the source through the accepted P1-e parser,
finalizes only `first_boot_source_bundle_sha256` in the signed template, and
commits source, final config and `ReadyForBootstrap` receipt with exact fixed
custody. The lifecycle commitment key is independently generated once,
installed root-owned mode `0400`, and retained outside logs and handoffs.

After materialization, the FINAM credential is removed and network access is
revoked. The source must still be no more than 300 seconds old at O2-B
admission. At 301 seconds the phase fails with zero bootstrap mutation and a
new manifest is required; no freshness exception is permitted.

### 4. O2-B isolated supervised one-shot

The root runner is the foreground process of the fixed
`moex-finam-p1-paper-o2-bootstrap-runner.service`; its exact unit bytes and
hash are part of the later execution artifact. It rereads and cross-validates
the Active claim, `ReadyForBootstrap` receipt, source/config bytes, custody,
installation and freshness, then holds the linear guardian permit. One reviewed
`systemctl daemon-reload` is allowed only to load the already installed exact
unit bytes. Enabling any unit remains forbidden.

The runner spawns the following exact command as a child; the runner itself
does not block its supervision loop:

```text
/usr/bin/systemctl start --wait moex-finam-p1-paper-bootstrap.service
```

Every 250 ms the runner calls `Stage8bP1fRunPermitV1::poll_deadline`, checks
the child non-blockingly and reads the bootstrap unit's `ActiveState`,
`SubState`, `Result`, `ExecMainStatus`, `MainPID`, `ControlPID`, `Job` and
`ControlGroup`. Exiting or killing the `systemctl` client is never treated as
proof that the bootstrap service stopped.

On `BeginStopping`, SIGTERM, SIGINT or supervision failure, the runner issues
`/usr/bin/systemctl stop --no-block moex-finam-p1-paper-bootstrap.service` and
continues permit/unit polling. At `ForceKill` it issues
`/usr/bin/systemctl kill --kill-who=all --signal=SIGKILL
moex-finam-p1-paper-bootstrap.service`. The accepted 30-second permit grace is
not extended. Stopped proof requires no pending Job, `MainPID=0`,
`ControlPID=0`, inactive-or-failed unit state and an empty `cgroup.procs` for
the reported ControlGroup.

The runner service's pinned `ExecStopPost` invokes the same runner binary in a
cleanup mode, so unexpected runner death causes stop, bounded kill and stopped
proof independently of SSH. It is not a sixth facade. After host restart both
units remain disabled; recovery first proves there is no bootstrap process or
job. If cleanup cannot prove absence, authority stays Active or Stopping,
diagnostics are retained, and neither a terminal receipt nor a new admission
is allowed.

The bootstrap unit has `PrivateNetwork=yes` and
`RestrictAddressFamilies=AF_UNIX`; it cannot contact Redis or FINAM. Terminal
state is selected according to the accepted guardian API:

| Observed outcome at terminal write | Guardian terminal state |
| --- | --- |
| systemd success, accepted receipt and exact durable-root reread before deadline | `Completed` |
| verified failure, operator stop or rejected durable root before deadline | `Failed` |
| deadline reached, or recovery attempts terminal write at/after deadline | `Expired` |
| unit/process absence cannot be proved | no terminal write; retain Active/Stopping diagnostics |

`Completed` and `Failed` are forbidden at or after the deadline. A terminal
write failure leaves the exact pending-terminal transaction and permits only
byte-identical state/reason continuation after stopped proof. It never creates
a new admission. Ordinary P1 service is not started.

### 5. Retained evidence

The evidence collector is read-only and records:

- exact package/target/install/policy/template/public-key identities;
- genesis, activation, claim, materialized and terminal receipt hashes;
- source/config/credential metadata and redacted hashes, never secret bytes;
- broker-truth checked time and measured age at O2-B;
- bootstrap unit static identity, one-shot result and journal digest;
- durable-root journal/seal/receipt identities and custody;
- P1 main unit inactive/not-enabled, no P1 process after the one-shot;
- DB15 still empty and P0 identities unchanged;
- zero Redis contact by O2 and zero FINAM write/order-execution calls; the
  accepted read-only account orders snapshot remains part of broker truth.

The exact raw and normalized evidence, command exit codes and package safety
report are retained for independent acceptance. O3 remains closed until that
acceptance.

## Crash and retry policy

- Genesis/activation/claim/materialization recover only the exact retained
  pending transaction under the original generation, sequence and deadline.
- `SourceCommitted` or `ConfigCommitted` resumes deterministic completion only;
  neither state admits bootstrap.
- Lost O2-M response rereads exact retained bytes and receipts; it never
  recollects under the same claim with a different source.
- Lost O2-B response first proves the bootstrap unit/job/cgroup absent, then
  classifies the exact durable first-boot marker/receipt through the accepted
  V5 recovery contract. Before the deadline, an exact committed result may
  become `Completed` and a verified failure becomes `Failed`; at or after the
  deadline the only API-valid terminal state is `Expired`, even when a durable
  root was created. The root and evidence are retained and bootstrap is never
  blindly repeated.
- Runner loss is handled by the fixed systemd runner unit's `ExecStopPost` and
  the same bounded cleanup mode. Recovery does not infer service termination
  from runner or SSH termination.
- A completed, failed, expired or spent manifest cannot be claimed again.
- Rollback never deletes the authority root or a created durable root. Any
  post-effect correction requires a separately reviewed recovery package.

## Closed boundary

This commit does not generate keys, contact the VPS or FINAM, create config or
credentials, mutate Redis, reload systemd or start a unit. O2 execution remains
closed until a later immutable package contains the reviewed facade binaries,
exact public authority identity, policy/template, commands and safety result
and receives independent execution acceptance. O3/O4, paper-provider order
execution, FINAM POST/DELETE, broker dispatch, runtime-live and real orders
remain closed.
