# Current status — FINAM migration / ALOR parity

Status date: 2026-09-02.

This document is the operator/developer status source of truth. It intentionally
separates what already exists from what is still forbidden for continuous
runtime-live.

The stable macro-roadmap is fixed in [roadmap.md](roadmap.md). Review may split
an accepted macro-stage into smaller patch gates, but it does not renumber or
replace the Stage 0–13 roadmap without a separate roadmap ADR.

## Current accepted boundary

- An isolated paper-shadow P0 stand is active on the retained native
  Linux/amd64 VPS at source ref
  `9bdc2719ae30e7e59d6db35e8a48246bb15ddd6f`. Redis is loopback-only with
  protected mode and AOF/everysec enabled. A separate FINAM token passed the
  in-process fail-closed preflight with `readonly=true`, 2 visible accounts and
  68 market-data permissions; its root-owned mode-0600 secret file is not part
  of Git or evidence. The WS and paper runtime services are enabled and active.
  Continuous real final M1 bars produced two complete M10 runtime-state batches;
  the consumer has `pending=0` and `lag=0`. Health is `ReadOnly`, readiness is
  `Reconciliation / OperatorLiveArmMissing`, and every order/cancel/real-runtime
  flag remains false. One fail-closed DLQ row records a non-contiguous source
  bar from pre-stable-generation diagnostics; no incomplete bucket entered
  runtime state. This is unseeded live transport/projection evidence, not ALOR
  parity and not the Stage 7B durable paper order/ACK lifecycle. See the
  [initial deployment evidence](stage-8/stage8b-paper-shadow-vps-deployment-evidence.json),
  [live activation evidence](stage-8/stage8b-paper-shadow-readonly-live-activation-evidence.json)
  and [runbook](stage-8/stage8b-paper-shadow-vps-runbook.md).

- The R0-R2B verifier-workdir repair at `50a2d26774ec71342a5acf281704dff09e5c7f0d`
  is independently accepted. A fresh controlled attempt on 2026-09-02 passed
  archive binding, native host preflight and the pinned Generation-2 ceremony
  verifier (`13 + 1` bindings), then failed closed before run 1 when the
  systemd aggregate trigger reported a dependency-job failure. Uninstall and
  both custody layers passed: no unit, binary, transaction state, private
  material or proof container remains. Generation 2 is inactive and R2B is
  `NOT_ISSUED`. The native installation proof is deferred until a reviewed
  failed-unit diagnostic exists; it does not block isolated FINAM paper-shadow
  parity work. See
  [the evidence summary](stage-8/stage8b-p-r2b-generation2-native-r2b-deferred-systemd-attempt.json),
  [the ADR](adr/adr-stage8b-native-proof-defer-for-paper-shadow.md) and
  [the paper resumption plan](stage-8/stage8b-paper-shadow-resumption-plan.md).

- Stage 8B-P R2B Generation-2 Backup/Restore R0-R1 is independently accepted at
  `3029bab714f8b75daaba3946ed858426515b4165`; its immutable review archive has
  SHA-256 `ee7deefa31dcf6b126408452f4772081ba20999c90ef58cf52df7b873869759f`.
  Stage 8B-P R2B Generation-2 Composition Rebuild R0-R1 Exact Phase-6 Evidence Closure is
  independently accepted at
  `c74382a7e3a63d3673dec220ff4e9caaba6b48ee`; its immutable review archive has
  SHA-256 `2185e1af518bbfadb7e9f426cacab00d444dcdd8ca37957c1e4f9d3901e09a62`.
  Stage 8B-P R2B Generation-2 Composition Rebuild R0 at
  `1a1933f90075591a88d4631c7c72599a1262115d` remains the accepted substantive
  predecessor; production binaries rebuilt in R0-R1: false. Generation 2 remains inactive.
  The accepted typed evidence proves exactly one failed attempt: ordinal `1`,
  `POST /v1/sessions`, `NETWORK_CONNECT_FAILURE`, no HTTP status/body, and all
  dispatch/effect/order flags false. Production binaries were not rebuilt.
  The active candidate is **Stage 8B-P R2B Generation-2 Full Transaction
  Rebind / Native Controlled Installation Proof R0-R2B verifier-workdir repair**. The prior
  review-only R0 package at `2a3ae5cc3f7ecc32962dd69c8af781b51d647bba`
  was not accepted and was never executed. Its static contract
  binds the unchanged six-phase, 31-service, 18-unit graph to six inherited
  Phase-1/2 ELF and six accepted Generation-2 Phase-3/6 ELF. The native runner
  now starts from the actual reviewed ZIP, pins the privileged image ID,
  installs 18/18 exact units, preserves root terminal mode `0400`, verifies
  the temporary ceremony in memory and destroys its tmpfs source. The R0-R1
  review accepted those substantive corrections but denied execution because
  cleanup began after early archive/argument failures and tmpfs was not protected
  by a no-swap precondition. R0-R2 installs the fixed-path cleanup guard before
  the first failure-prone operation, makes cleanup failure fatal, requires zero
  host and container-visible swap entries, and limits host-attestation age to
  15 minutes. The R0-R2 review accepted those closures but reproduced one remaining
  Docker cleanup issue: daemon/query errors could be mistaken for an absent container.
  R0-R2A destroys the host source first, bounds Docker queries/removal, treats unknown
  state as failure and requires VPS destruction when container absence is not proven.
  R0-R2A was independently accepted and invoked once on the dedicated disposable
  VPS. Archive, host, image, ceremony metadata and both no-swap checks passed. The
  container was created with `network none`, but the pinned ceremony verifier failed
  before unit installation with `Error: Input`: Docker's empty image working directory
  started `docker exec` at `/`, while the verifier correctly rejects a ceremony below
  its current working directory. Cleanup passed, the host ceremony and container are
  absent, private material retained is false, and no transaction phase started. R0-R2B
  adds only `docker exec --workdir /work` for the verifier and is independently accepted.
  Its first controlled execution passed archive/host/ceremony verification and
  stopped before run 1 at the systemd transaction graph; the exact failed unit
  was not retained by the accepted cleanup flow. A repeat execution is deferred
  until redacted failed-unit diagnostics are reviewed.
  Failure and stale-replay properties are hash-bound to accepted Implementation R0-R1A;
  the native runner proves the exact success-to-expected-fail-closed path and
  clean reset. Static checks cover 79 contract mutations and 18 attestation
  mutations plus live-swap and Docker-cleanup runtime cases. The full two-run
  native proof has not completed. The developer Docker daemon is ARM. A separate
  clean Ubuntu 24.04 native Linux/amd64 VPS was used for the accepted
  verifier-workdir attempt and is now retained for isolated paper-shadow work
  under an explicit reviewer/operator decision; it contains no Generation-2
  private material or production broker credentials. Generation-2 private
  material remains offline as the accepted encrypted backup; no plaintext
  ceremony is retained in the repository or handoff. Generation 2 remains
  inactive, production credentials are not installed, controlled installation
  is not performed and R2B authorization remains `NOT_ISSUED`. FINAM,
  AuthService external network, broker GET/POST/DELETE, dispatch, Redis live,
  runtime-live and real orders remain closed.
  The active x86_64 VPS has been used only for a native static engineering
  rehearsal in a separate directory: the checker and 40/40 contract mutations
  passed against the public G2 ELF evidence. Because sensitive trading
  cotenants are active, no proof or privileged container was created, no
  ceremony or credential was transferred and this result is explicitly not
  formal native acceptance evidence.

- Stage 8B-P R2B Implementation R0-R1A is independently accepted at
  `6672819e357a3c2a2c1e73e5408c393da01913a1`. Controlled Installation / Full
  Transaction Proof R0 is independently accepted as a design-only boundary at
  `1e4db79288b0809fd5975edfdd0fc14740bcc8c6`; its immutable archive
  `moex-trading-project-1e4db79.zip` has SHA-256
  `5d55ccd8a585d6da780531aa237c9fba215328bce502b1099a8dc5aa3c22faea`.
  Initial Controlled Installation Implementation R0 preflight at `b9f0c43` was
  not accepted because it incorrectly claimed aggregate success under no
  network, mixed a fresh canary domain with production authority, omitted the
  graph trigger and had an incomplete uninstall inventory. The current
  R1 at `9fd9fa9` closed those findings but was not accepted because its
  production oracle could accept an auth/client failure before any request.
  The current candidate is **Implementation R0 Preflight R1A**. It additionally
  requires exact typed evidence for failed attempt #1, `POST /v1/sessions`,
  with only request-level network failure or timeout and every effect flag
  closed. It retains the proof-only aggregate trigger, all 19 unit removals,
  all 12 binary removals, final systemd image and fresh-handoff source mount.
  The preflight has not
  created a container, installed a unit, materialized a key or executed the
  proof. R2B remains
  `NOT_ISSUED`; FINAM network, AuthService and broker GET, POST/DELETE, broker
  dispatch, Redis live, runtime-live and real orders remain closed.

- Stage 8B-P R2A8-R1 is independently accepted at
  `5b2079d7d524d2fa6f084f44f961c4b5958c042a`; its immutable handoff is
  `moex-trading-project-5b2079d.zip`, SHA-256
  `903df69b800477706f4b2e95097fe84174f42e89b0a85a4b5fa94430619acb6a`.
- The Stage 8B-P R2B proposals through R4 at `293dbad` were not accepted.
  The active candidate is **Stage 8B-P R2B Proposal R4-R2A Acceptance
  Closure**, a design-only
  runtime-isolation, typed-terminal and reachable-creator correction. It keeps
  the owner-mediated creator/stager chain and adds a built creator entrypoint,
  fail-closed Yama plus exclusive UID8301 admission, pre-runtime nondumpability,
  an exact typed terminal protocol, canonical root verdict, one monotonic
  pidfd-backed child deadline, bounded reap and crash-durable chmod metadata.
  R4-R2 adds the missing no-argument production publisher ahead of the creator
  and retains R4-R1's removal of the creator's cyclic dependency: an
  independently owner-signed upstream authority drives empty-root generation
  1, while an optional predecessor binds only N-to-N+1 continuity.
  Independent review withdrew the previously alleged production dependency
  cycle: the accepted R2A8-R1 current source is external input C0 and the R2B
  writer creates later output C1. The narrow R4-R2A closure freezes this
  distinction, corrects controlled-fixture wording, adds the missing exact
  writer oneshot unit and exact R4-R2 evidence naming. Independent review
  accepted the closure at `f24f1044ac0b29c2f588853b817e519cfe8d3d8b`
  after the deployment-only systemd syntax microfix. The exact
  publisher/creator/stager/writer units place
  `RefuseManualStart=yes` under `[Unit]`, omit unsupported
  `ConditionPathIsRegular=`, and are covered by a section-aware checker plus a
  target-version parser gate. R4-R2A is formally closed.
  Issuance Package R0 at `928168ed47e5b9dd873cd73815fbccecde7a8981`
  retained the correct design direction but was not accepted: its refresh used
  the seven-document effect inventory rather than the six-document read/auth
  snapshot embedded by the R2B helper, and its 30-service graph had no
  production producer for the package issuer's unsigned draft. The narrow
  **Stage 8B-P R2B Issuance Package R0-R1A1 Strict Schema Freeze** was
  independently accepted at
  `ebec9a100c92872134f3de91644cec50e2ed073a`. It binds Auth,
  TokenDetails, GetAccount, Trades, GetOrders and
  GetOrder to embedded snapshot SHA-256 `7c8e6bcd...03c5`, records a fresh 6/6
  public-doc match, and selects a separate no-signing-key draft builder. The
  fail-closed graph contains 31 service invocations in six exact phase targets
  with failure, skip, partial-fanout, mixed-nonce and stale-output propagation
  frozen. R0-R1A freezes seven ordered builder inputs, eleven source-specific
  producer/issuer/UID/receipt mappings, all six complete phase objects, the
  sole signer identity and read-refresh provenance. R0-R1A1 additionally
  enforces exact dictionary
  keysets, the unique ordered six-document refresh inventory and byte-pinned
  acceptance rows. Its 66 acceptance rows and 66 targeted negatives comprise
  the retained 25/25 and 29/29 matrices plus 12/12 strict-schema mutations.
  R0-R1A1 is the accepted design predecessor.
  The active candidate is **Stage 8B-P R2B Implementation Package R0**. It
  materializes the no-argument production draft builder, independent package
  signer revalidation, six phase targets and aggregate target as source only.
  The graph is statically frozen at 31 service invocations; every barrier uses
  exact `Requires=` plus `After=`, targets stop when unneeded, oneshots retain
  no active success state, and unsigned/signed package creation is no-replace.
  The implementation checker passes 52 acceptance rows and rejects 70/70
  targeted mutations. Isolated Ubuntu 24.04 systemd 255 verification passes
  all 18 units without loading or starting them. Implementation R0 requires
  independent review before any activation work. The units are not installed,
  enabled or started. R2B authorization remains `NOT_ISSUED`; operator
  selection, run nonce, credentials and unsigned/signed run packages are
  absent.
- FINAM network access, account-order POST/DELETE, Redis live consumption,
  broker dispatch, runtime-live, strategy-live and real orders remain closed.
  Proposal R4-R2 does not authorize a credential read or any broker request.

- Stage 8B-D R1 at `b3358ba2268da3db4eb8352c097495ebb85575d7` retained the
  proposed bounded-effect architecture but was not accepted as a frozen transition
  package. Review found an inherited P0 governance defect: default CI still invoked
  the historical Stage 5 forbidden scanner, which rejects the accepted current tree.
- GOV-CI-1A terminal Stage-5 authority retirement was completed by the explicitly
  authorized history-preserving administrative merge
  `1dea519cbf2affc3d99866fdae66bbddbafefa24`. Canonical `ci.yml` was unchanged in
  that merge; the historical Stage-5 workflow is now manual-only.
- GOV-CI-1B was independently accepted at
  `13f659f368cbb36a2d38c2b0b88efa376f0b690c` and merged to `main` by
  `7bc9fdab190e011111b15ebdf2f35ff2263a8e34`. GOV-CI-1 is closed.
- Stage 8B-D R2 was independently accepted at
  `f296d0be782b8aa550a20e27600ba16826214349` and merged tree-identically to
  `main` by `50ed5382fdbe2d62ed253d65a312f951e2a267ff`.
- Stage 8B-S R1 at `a675a772e02fa6da1a33973127542696019eb2f7`
  retained the architecture but was not frozen. R2 at `831eec8` closed the R1
  findings but was not frozen because IT followed exact P authorization.
  Corrective Stage 8B-S R3 was independently accepted at
  `afecc2584593570b62cbe7f00ee81f64d4b9b26b` and merged tree-identically by
  `d1581962666aa82b993854d0642e67bd66624032`. Stage 8B-I at `a52fbca` remained
  fail-closed but was not accepted: builder/permit ordering, durable K2 binding,
  exact five-state closure recovery, build/endpoint provenance, arm binding and
  full regression evidence required correction. I-R2 at `21426ee` closed those
  findings but was not accepted because K1 depended on future K2 evidence, K3
  preceded exact attempt durability, and capability/durable cross-binding was
  incomplete. Stage 8B-I R3 was independently accepted and merged exactly at
  `0af222f252cdc2b4c763c9e04935a5cb5f0c6d65`. Stage 8B-IT candidate
  `e44053917a928aeb4bc8e3330a58a693edc31fd3` and corrective R2 at
  `74d07c842f0ef3a02c4c30a919542a108304b52e` were rejected. Corrective IT R3
  was independently accepted and history-preserving fast-forwarded to `main`
  at `14e01a9f838080e196ece5945a7796f2bd2600bc`: reqwest protocol retries
  are explicitly disabled; K4 mints an opaque proof consumed by the Stage 8A-2
  extraction seam itself; request parts live in a sibling capsule whose fields
  the nested adapter cannot forge; raw observations still cannot escape the
  mandatory Stage 8A-3 classifier. The active candidate is now the separate
  Stage 8B-IT-TLS R1 controlled rustls qualification: test-only local CA,
  exact-host validation, HTTP/2 ALPN, wrong-CA/SAN/validity rejection and
  post-handshake timeout/loss classification through the same adapter.
  Production endpoint authority,
  operator arming, FINAM execution, Redis live consumption, broker dispatch,
  runtime-live, real orders, Stage 8B-P/XE and Stage 12 remain closed. A
  separately accepted controlled TLS qualification remains a blocking Stage
  8B-P precondition.

- Stage 8A-0 is independently accepted and closed at
  `c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d`.
- Stage 8A-1 R3 is independently accepted and closed at
  `1ff04154ba4b7a5ee060a73b853ce89bd7442f44`.
- Its independent acceptance review SHA-256 is
  `ac11ffae08cc11c6e11f031deec9ca4e6d55b76eae6bcc5887f85b0fb913c74c`;
  the accepted immutable handoff is `moex-trading-project-1ff0415.zip`,
  SHA-256 `02a332af7ccf1815a79de0f9a025be9d31094160f7977cd3e34de270497138a0`.
- Stage 8A-2 R1 is independently accepted and closed at
  `16180ac4f8eab761b3b055c1f5515f62cd94bfb9`.
- Stage 8A-3 R2 is independently accepted and closed at
  `012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`.
- Stage 8A-4 Design R2 is independently accepted and closed at
  `cc58c10d22db312cd83640f1c1e7fd86861a4594`.
- Stage 8A-4 Implementation R1 was rejected at
  `245fea18f3f22bd4233eed4f9207445efd0a6d46`; Implementation R2 was rejected
  at `3c445aef6dce3f38a81ee477eaa73e56ffdc0a80`; Implementation R3 was rejected
  at `5a846f9852c7fe58b9a24891c22e9d9dafeefd27`.
- Stage 8A-4 Implementation R4 is independently accepted and closed at
  `4caf07c16ddad021add7cffe6e887165e49e1bf0`; its final review SHA-256 is
  `0f8de37819ccc005bbc609bc21f029f5783ccdd43c0a634b4c09614f507c2a0a`.
- Stage 8A-4 durable-composition Design R1 at
  `80fe35ef67e335540e0984781f63a99af794bfe1` was not accepted; its architecture
  was retained. Durable-composition Design R2 at
  `6ddf54ef9d7f740dc59cd2450e78301be3d068cb` is independently ACCEPTED and CLOSED;
  its final review SHA-256 is
  `160b674d661982b6dbaa6248c2c4acaf883543cb8be99318ef04b0787492f4ba`.
- Stage 8A-4 durable-composition implementation Specification R1 was not accepted
  at `e3d0ac39dcff25439a7e78f51142b852d8347a2f`. Specification R2 is independently
  ACCEPTED and CLOSED at `dd01253596527d6cff1db11cc32ae3c3348c96a0`; its
  accepted review SHA-256 is
  `acb8364ee2100bf64e50522823b1da21093f96c73f93b20b4cdf9e7ac09b58ec`.
- I1 R1 at `0678354185c52aff1d6e194b16f80b0a2c84a1f0` was not accepted. I1 R2 is
  independently ACCEPTED and CLOSED at
  `113d2827ef255e8d2c2597a3acb38fe52dd7e52d`; its review SHA-256 is
  `5ef7d0fcc645874a8d9bce7e2d2bb3004f06b038c81b0bf5496582464cb1b9e7`.
- I2 R1 at `65276199b42b3dac5f7b48346dfe11e61f42e41d` was not accepted. I2 R2
  at `e04edea480b049a569385ea342a8802fedd5a307` closed all four R1 findings but
  was not accepted because exact Stage 6/7 action/payload cross-binding was
  incomplete. I2 R3 is independently ACCEPTED and CLOSED at
  `90f46052cc31cea012437eddb59fb7c3ca5c2320`; its review SHA-256 is
  `196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0`.
  It provides one private `RequestAccepted` command authority, exact
  endpoint/attribution/PLACE-shape/CANCEL-target validation and binds the
  accepted-command payload digest into the private outcome.
- I3 R1 at `a490bbe700c51f0e9c6debd2a007cb9b5061c3d8` was not accepted because
  its raw Stage7 batch entry was caller-forgeable, truth/control evidence was
  not exact-writer-bound and post-write I/O uncertainty was not sticky. I3 R2
  at `62e5e0509adb9cceb1d9947b5b3f92120e2f19ea` retained the corrected durable
  mechanics but was not accepted: a doc-hidden raw core append remained public,
  Stage8A1 current-owner authority regressed, runtime depended on FINAM, and
  restart required the lost I2 candidate. I3 R3 at `3aa2670` was also not
  accepted because its public sealer was forgeable and incomplete restart
  became ordinary Ready too early. I3 R4 at `4403068` closed those findings but
  was not accepted because fresh-process recovery retained the old capability
  and issuer. I3 R5 at `0d1b14f` closed that gap but was not accepted because
  recovery issuer construction still required readable current control. I3 R6
  is independently ACCEPTED and CLOSED at
  `593ff255ef7826a22e66c9aff6f7ea47acf47644`; its review SHA-256 is
  `1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257`.
- I4 Design R1 at `06bb09fa13431d0ae34039f37497d4f37914f022` was not accepted
  with zero P0 and three P1 design ambiguities. Design R2 at
  `d1a050a53d95a3d53874bf0866e3598b948dde68` closed those semantics but was
  not accepted because its all-crate-private declaration made the accepted
  cross-crate handoff impossible. I4 Design R3 is active as a docs/checker-only
  slice. It preserves every R2 semantic rule and changes only the Stage7B
  terminal capability to public-opaque/external-nonconstructible for the
  existing `finam-gateway -> runtime-durable-service` direction. Design R3 is
  independently ACCEPTED and CLOSED at
  `81727aae1f648f17961177fc9541e2483cbf07f2`. I4 Implementation R1 at
  `1da0a65ae0246edd837a7e4c060c702ff4cc097b` was not accepted with zero P0,
  two P1 and one P2: it accepted raw current snapshot DTOs/caller time, omitted
  inherited micro-budget/strategy-instance barriers, exposed settlement-only
  getters and traced only 40/12 implementation checks. R2 at
  `6a7f07cfd5b3ea97e64fa6c12f2d493514e1a804` closed those findings but was not
  accepted because its only I4 facade still required the process-local
  pre-effect Stage8A1 issuer, which cannot be reconstructed for a finalized
  request after restart. I4 Implementation R3 is independently ACCEPTED and
  CLOSED at `4a11688c941ee240e377b384042c4bca837b040f`; its final review
  SHA-256 is
  `0377879b5b10c38ef0740af54e3d2b341d980b21824490664d828e8a6d4e0046`.
  It reconstructs the crate-private I4-only read-only issuer from terminal/root
  authority, resamples opaque current sources in process B and preserves the
  exact historical ACK when current readiness is absent or blocked.
- Stage 8A-5 is independently ACCEPTED and CLOSED at
  `bf58b47fdef8af774a4107455dfcc6204e594283`; its final review SHA-256 is
  `72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc`.
  The accepted handoff is `moex-trading-project-bf58b47.zip`, SHA-256
  `f3de068809f19e44daae5ccd98cf7c8ce131cb4c756d278e8f9dadd01c7d1a9b`.
  Stage 8A is formally CLOSED. Only a separate Stage 8B design package is now
  authorized. ACK/readiness publication, Redis XADD/XACK and live consumption,
  FINAM POST/DELETE, broker dispatch, retry/resend/re-arm, runtime-live, real
  orders and Stage 8B execution remain CLOSED.
- Stage 8B-D R2 is independently accepted and merged. Stage 8B-S R1 was not
  frozen; R2 at `831eec8` was also not frozen due to phase ordering. Corrective
  Stage 8B-S R3 is accepted and merged. The first Stage 8B-I candidate was not
  accepted; I-R2 was also not accepted. Corrective Stage 8B-I R3 is accepted
  and merged exactly at `0af222f`. The rejected IT candidate `e440539` is
  retained in lineage, together with rejected R2 at `74d07c8`. Corrective IT
  R3 uses K4-proof-bound Stage 8A-2 extraction, sibling-private request parts,
  explicit reqwest never-retry policy and a mandatory in-adapter classifier.
  Independent IT R3 acceptance and controlled TLS qualification
  are required before Stage 8B-P; real requests remain unauthorized.

The reviewer transition package is documented in
[stage8a2-reviewer-transition-handoff.md](stage-8/stage8a2-reviewer-transition-handoff.md).

## Detailed transition history

- Stage 6E-R1 is independently accepted at
  `10e357825a701193d964975bb5769bd0745d4986`; Stage 6 is CLOSED.
- Stage 7A-R2c is independently accepted and Stage 7A is CLOSED at
  `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.
- Stage 7B-a-R1 is independently accepted and CLOSED at
  `a947c24bb413a91c5eb0ad97f4ac0b402bfd0641`.
- Stage 7B-b at `cf65ff6183b3a9be5c4df9e1a1adb2510da42b43` was not accepted
  because pathname re-resolution could detach the held lock inode from the
  journal selected after a durable-root replacement race.
- Stage 7B-b-R1 at `f3298fe23f5ab18bc78c98dfbceb1c53baab19fc`
  closed the filesystem TOCTOU but was held because writable constructors
  could accept a second operational identity after root validation.
- Stage 7B-b-R2 is independently accepted and Stage 7B-b is CLOSED at
  `ff3fa2e8908440863b40b838991d4716b33caad4`. It retains a
  linear directory FD plus exact root `dev/ino`, resolves lock/journal children
  through `openat`, locks an identity-scoped trusted-parent namespace file, anchored root directory and sidecar inode,
  and fails closed on root or lock namespace drift before StorageReady and
  every later journal operation. Real subprocess tests inject root replacement
  between lock and journal open and replace the live lock pathname.
  Every writable constructor recomputes the complete operational-identity
  digest and compares it with the root-bound digest before authorization,
  locking or any journal effect.
- Stage 7B-c-R1 is independently accepted and Stage 7B-c is CLOSED at
  `c57ae8d5f98bbb11df0a81f78262d3916b276d81`. It composes the
  accepted Stage 5G clean-restart seed and Stage 6 file journal into one linear
  recovery owner, commits a canonical HMAC-authenticated recovery seal before
  readiness, and fails closed into an explicit zero-effect RecoveryBlocked
  state for missing, corrupt or cross-binding-invalid authority. Namespace
  validation is required before StorageReady and every later writable or
  authority-sensitive boundary. R1 adds direct file-backed B-034/B-039/B-040/
  B-041 witnesses and a real SIGKILL B-032 pre-rename crash witness.
- Stage 7B-d design at `09a22765ae6ee37b304bfed6492bd103da44360d`
  was not accepted as frozen because settlement authority, ACK-versus-poison
  provenance, Redis stable-key semantics and B-052/B-053 slice ownership were
  underspecified. Stage 7B-d-design-R1 was independently accepted/frozen at
  `00cead2989493b44e0d86ead29b95d57a7fbcbe2`. It resolves those design
  blockers and divides durable Redis
  paper-service composition into: (a) lifecycle/seal-before-settlement,
  (b) atomic idempotent ACK/DLQ plus XACK, and (c) composite readiness,
  supervision and restart PEL reclaim.
- Stage 7B-d-a-R1 is independently accepted and closed at
  `8418cfb63ecee6702bf8a2873592b7cad1e711ee` after the original
  `f71eeb926464f6634d485d5720b25c5e026b40d5` candidate was not accepted. It adds an owner-held,
  Redis-free lifecycle facade and exact-bound non-serializable terminal ACK
  authority. R1 authenticates and exactly compares the current on-disk seal
  before frontier refresh and again before a no-advance ACK mint; deleted,
  corrupt or valid-but-different seals fail stop. Real SIGKILL witnesses cover
  accepted, dispatch, unknown-effect, durable-outcome, finalized and sealed
  crash states; B-046 now fsyncs a test-only provider-effect witness before its
  during-effect barrier and proves redelivery does not invoke it twice;
  correlated sequential CANCEL is reconstructed from source-bound Stage 5G and
  Stage 6 history. B-043..B-051 and B-054..B-056 are candidate-implemented.
  B-052/B-053 remain pending for d-c real-Redis restart evidence. Acceptance
  opens only Stage 7B-d-b atomic Redis ACK/DLQ + XACK; d-c remains closed.
- Stage 7B-d-b-R1 is independently accepted and closed at
  `e0bf9b7d9eb209e19b875f199511a493ddcd0da9`. It adds one private
  owner-mediated, Lua-atomic Redis primitive for ACK or redacted permanent
  pre-Stage6 poison DLQ publication plus source `XACK`. Stable entry markers,
  request-level canonical ACK markers, exact PEL preconditions, response-loss
  retry and entry-scoped unresolved settlement health are exercised against
  isolated real Redis. R1 separates stable terminal-request identity from the
  dynamic seal authority and requires opaque canonical Stage 7A poison
  evidence; a valid command cannot be caller-labelled as poison.
  Rows B-057..B-063 are accepted/closed.
- Stage 7B-d-c-R2 is independently accepted and closed at
  `2b6371adb905654e0ddd8b6714159bcef737b577`; R1 at
  `9b98c360e1153e79971b5935d03fd0a0bdd1f4f4` was not accepted. It composes the
  recovery owner and atomic settlement backend into an isolated paper Redis
  consumer with per-boot identity, bounded old-PEL reclaim, durable PEL
  reconstruction, independent source/claim freshness, external task
  supervision and storage/seal-aware composite readiness. Real-Redis restart
  evidence closes rows B-052/B-053 and B-064..B-070. R1 restores deterministic Stage 7A
  pre-Stage6 ACK semantics with zero Stage 6 mutation, adds a real-service
  positive `PaperReady` witness and one child-process Redis old-PEL reclaim
  witness. R2 adds a read-only pre-admission request-marker guard: changed
  marker-only history remains pending before Stage 6/provider, while exact
  history emits only an atomic duplicate ACK. Stable command hash and canonical
  ACK stream/output are validated; incomplete legacy markers fail closed.
  Every FINAM/live surface stays closed.
- Stage 7B-e R1 at `422bd1a8b45bfd3397aa588f914494cc11f5c401`, R2 at
  `8cc72f148032bedda6a0ef86f6edda2c1394abc7` and R3 at
  `d501d62543cde890bfbb8d8ea0dc878e28a711b2` were not accepted. R3 closed the
  intended production-prefix B-079 mutation but retained a suffix-after-tests
  scanner gap. R4 required the exact changed-path set and exact full-file
  SHA-256 for all six allowed crate deltas, plus before-test and after-test
  hidden-Stage8 mutations. R4 was independently accepted at
  `a1044e0dbe324c722b637498ca80ffafd9f0cbee`; Stage 7B is formally CLOSED.
  The final accepted evidence contains
  no execution behavior: exact normative X01-X20, the mandatory inherited Stage 7A
  full gate, complete file/Redis/subprocess evidence and the 80/80 proof map
  with 59 aggregate negative cases.
- The Transition Gate 7→8 R3 specification is the active design-only candidate
  on branch `gate7-to-8-spec`. Candidate `4d1106e` was rejected as an
  acceptance package while its architecture was retained. R1 `f7afc1c` closed
  the original findings but was not accepted because CANCEL 401 and
  same-durable-request re-execution after `DefinitelyNotSent` were not pinned.
  R2 closed both gaps but was not accepted because its post-acceptance
  transition authority simultaneously opened broad Stage 8A implementation
  and described Stage 8 implementation as closed. R3 closes only that
  governance contradiction and retains the current
  official FINAM contract snapshot, sole-builder rule, Day-only TIF,
  endpoint-specific outcome tables, closed `ProvenNoMatch`, exact kill-switch
  semantics, 69 mandatory rows and 36 negative mutations. It freezes the
  protected capability, operator arm,
  MARKET/LIMIT/CANCEL mapping, ambiguous-outcome reconciliation, max-one micro
  budget, allowlists, limits, kill switch, single-broker ownership and required
  evidence. Independent R3 acceptance opens only Stage 8A-0 current FINAM
  contract refresh/freeze for docs/evidence/checkers. Stage 8 production Rust
  and 8A-1 through 8A-5 remain CLOSED; Stage 8B real execution requires another
  gate.
- FINAM runtime POST/DELETE, broker network dispatch, runtime-live, real
  strategy orders and native Stop/SLTP/bracket remain CLOSED.
- Stage 7A permits at most one non-final command lifecycle per strategy
  instance without exceptions. A source-correlated CANCEL is admitted only
  after the target PLACE request has an explicit final disposition while its
  broker-order identity remains available. Stage 7A constructs no fresh broker
  truth, so fresh-truth temporal rows are explicitly closed-surface N/A here.

### Historical accepted transition record

- Stage 5G-h and Stage 5G were independently accepted and closed at
  `013e63bbee57c4f2d00a0587e9343ab623efba0d`. Transition Gate 5→6 is the
  current planning/authority target. It defines the broker-neutral durable
  identity chain, single persistence ownership model and crash-window policy;
  it contains no Stage 6 production implementation.
- Stage 6 remains closed until independent Transition Gate acceptance. Stage
  7+, Redis live consumers, FINAM execution, broker dispatch, runtime-live and
  real orders remain closed.
- Stage 5G-g was independently accepted and closed at
  `ee0505dfee71f043f3185c16cbdd563e3b36a6c1`. Its exact 54-row artifact SHA-256
  is `0f6698a7256537596071eef762f7d623050d1a1ec3023ecafc9b3799e9ba8bf0`.
- Stage 5G-h added no lifecycle
  behavior: it pins the accepted artifact byte-for-byte, rechecks every row
  hash, produces ACK/order-position/protective evidence from independent
  concurrent source adapters, freezes timer/restart witness tuples and records
  the authority digest inventory. Stage 6 and all live surfaces remain closed.
- Stage 5G-f was independently accepted and closed at
  `12af52d23218c67bc15b7b79835790e40834dfbb`.
- Stage 5G-g freezes the exact
  54-case lifecycle matrix in one deterministic artifact: 10 ACK, 16
  order/position, 8 timer, 12 restart and 8 protective rows. ACK and
  order/position use narrow source-produced artifact adapters; protective rows
  reuse the accepted Stage 5G-f runtime artifact; timer/restart remain bound to
  and executed through their exact accepted witnesses without fabricating
  unavailable runtime fingerprints.
- The Stage 5G-g gate proves debug/release artifact identity, exact scenario
  ordering and closed live surfaces. Stage 5G-h strengthens the aggregate
  evidence boundary; only its independent acceptance may close Stage 5G and
  open the separate Transition Gate 5→6. Stage 6 is not opened automatically.

- Stage 5G-e-d-c R3 was independently accepted at
  `c38d2e44e083e39552ea716823e43ebae775b881` and pushed to `origin/main`.
  It closes the exact source-proof field-map gap for fresh-truth application:
  constructor assignments are machine-described, mutation-sealed and
  independently source-oracle checked. Stage 5G-f is now the active allowed
  functional slice.
- Stage 5G-f is a paper/mock protective-completion implementation candidate on
  direct base `c38d2e44e083e39552ea716823e43ebae775b881`. It owns only Mean
  Reversion target/stop completion from exact protective broker/runtime
  feedback plus complete flat position truth for the eight frozen GPRT
  scenarios. It does not open FINAM native stops, SLTP/brackets, HTTP
  POST/DELETE, Redis live consumers, broker dispatch, runtime-live, real orders,
  Stage 5G-g/h or Stage 6.
- Stage 5F-e aggregate acceptance was independently accepted at
  `fb8245e2f91cfc1678548a1228e8558d9adc2181`; Stage 5F is formally closed.
- The immutable closure facts are recorded in
  [stage5f-closure-descriptor.json](stage-5/stage5f-closure-descriptor.json).
- Stage 5G is the next allowed functional slice, limited to paper/mock
  ACK, order, position, timer, restart and lifecycle-reconciliation work.
- Stage 5G-a is independently accepted at
  `011fd4b7baaa41fffdad7d3c28e463b7977f5989` on `stage5g-lifecycle`.
  Its accepted plan inventories the existing Stage 5C/5D and Broker Core
  authorities and freezes a 54-case lifecycle matrix.
- Stage 5G-b is an implementation review candidate. It adds one linear,
  paper-only mock ACK session, delegates pending-state policy to Broker Core,
  and invokes only the existing Stage 5C-i ACK facade after a complete
  callback-safe vector. Timeout, unknown-pending, missing broker ID, unproved
  expiry, unsupported duplicate and error outcomes remain retained without a
  runtime callback. Stage 5G-c remains blocked pending independent acceptance
  of this implementation.
- Stage 5G-b R1 is an implementation review candidate on immutable base
  `b6f4194769ce0f6c00a82361eba57dc3ed07e55c`. It binds the exact redacted ACK
  projection into fingerprint schema v2, makes contradictory no-send evidence
  fail closed, requires exact duplicate identity, admits only source-authenticated
  Market intents, and uses one deterministic state machine in debug/release
  evidence and in the linear Stage 5C production wrapper. Public Limit/Cancel
  admission remains `NotYetSourceAuthenticated`; no Stage 5G-c surface opens.
- Stage 5G-b R2 is an implementation review candidate on immutable predecessor
  `00d158978904c177828ff2a330b1f3c1bfb4bb10`. It makes exact no-send proof
  dependent on prior lifecycle provenance, preserves observed broker-order ID
  continuity, adds a non-decreasing ACK-time watermark to fingerprint schema
  v3, and exercises the public Stage 5G wrappers against a real linear Stage 5C
  settled capability. Stage 5G-a and R1 remain pinned by detached snapshot
  gates. Stage 5G-c remains blocked pending independent R2 acceptance.
- Stage 5G-b R3 was independently accepted at
  `92f57c7831d8a15fb2e37668d3b07f1ccea03af7` on immutable predecessor
  `d03f6e5e88fb853290457d6d6dac08f21c2cf28b`. It binds the full current
  lifecycle fingerprint into transition schema v4, so post-resolution
  duplicate ACK sequence/count/time watermark cannot collide. Pure and public
  production-wrapper witnesses distinguish `T+20` and `T+30` duplicate
  histories and their `T+25` continuation behavior without repeating Stage 5C.
  The accepted handoff required `origin/stage5g-lifecycle == HEAD`; P0/P1 were
  empty and Stage 5G-b is formally closed.
- Stage 5G-c is an implementation review candidate. It adds canonical Broker Core
  order/trade/position convergence around the ACK-resolved capability, keeps
  active/partial evidence callback-free, and delegates only terminal-complete
  vectors through the existing Stage 5C-j callback boundary. Stage 5G-d remains
  blocked pending independent acceptance of 5G-c.
- Independent review rejected Stage 5G-c R1 at
  `16591e819c571aa2ccb8e4b0d087d28c84090415`. The only authorized successor is
  Stage 5G-c R2-a authority recovery. R2-a restores the exact frozen Stage 5F
  semantic route, confines the required Stage 5C source projection to
  digest-pinned crate-private read-only regions, and keeps the accepted Stage
  5G-a inventory immutable. The rejected R1 convergence semantics are not
  accepted and their five public witnesses are explicitly deferred until R2-b.
  R2-b requires a separate independent ACCEPTED verdict for R2-a. Stage 5G-d,
  Redis, FINAM transport, runtime-live and real orders remain closed.
- Stage 5G-c R2-a was independently accepted at
  `c6ae2bdaea2575dd41e6da00acad5c231f3c7572`. R2-b is now an implementation
  review candidate pinned to that exact detached authority snapshot. It adds
  monotonic Market entry/exit position progression, explicit target Market
  order-state policy, transactional pre-terminal source authentication,
  exact-correlation watermarks, vector-order-independent broker evidence and
  five non-ignored source-reachable production witnesses. Exact source limit
  price and Replace fields are not present in the accepted Stage 5C projection;
  extending that projection remains blocked pending a separate authority
  review. Stage 5G-d and all live surfaces remain closed.
- Stage 5G-c R2-c-a at
  `581f4f6021dd781e7a5db9177be05feb7d94b12a` was rejected as submitted: its
  exact terminal MARKET evidence validator was retained as useful work, but
  the no-callback completion left stale pending/position state while returning
  a false timer-ready type-state. The only active successor is R2-c-a R1.
- Independent review rejected Stage 5G-c R2-c-a R1 at
  `d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654` as submitted while retaining its
  validation and state-coherence direction. The only authorized successor is
  R2-c-a R2.
- Stage 5G-c R2-c-a R2 is a review candidate on that exact predecessor. It
  rejects Canceled/Expired full fills before mutation, binds terminal evidence
  to the real source-owned owner/cycle payload, applies ACK/position/escrow work
  to an isolated transaction candidate, and uses canonical broker-evidence
  time for bracket grace. Inside grace it preserves the timer and returns
  `ReadyForTimer`; the deterministic timer later escrows the residual Exit.
  Ten focused witnesses run through accepted Stage 5F semantics and Stage 5G-b
  Accepted or Submitted→Recovered ACK lifecycle. A separately pinned local
  test clock also removes inherited weekend dependence from three Stage 5E
  parity fixtures without changing production time authority. R2-c-b remains
  closed.
- Independent review rejected R2-c-a R2 at
  `3d995af48e88588909e11505fdefc826ff8f66ce` as submitted because its grace
  decision mixed a local millisecond timer with broker source seconds. The only
  active successor is R2-c-a R3. R3 preserves the exact R2 transaction boundary
  but captures `BrokerTruth.received_ts.timestamp_millis()` before validation,
  so timer origin and decision watermark share the local receipt-clock domain
  and exact precision. Six source-reachable witnesses cover same-second
  post-start receipt, true stale receipt, delayed post-grace receipt, fresh
  snapshot retry, deterministic replay and inherited full-fill/rollback.
  R2-c-b and all live surfaces remain closed.
- Stage 5G-c and its exact replay-package identity are independently accepted
  and closed at `d7561e6f36d01aea3d0dd67892800fbb6ac0a716`.
- Stage 5G-e-d-b R5 was independently accepted at
  `2b2bcc671c68722b3b84b914b785ffcb83f6802d`. Stage 5G-e-d-c is the current
  implementation target. It consumes one accepted fresh-truth reduction,
  applies an owned candidate through the canonical order/position core, and
  returns only an authenticated clean-process restored capability. Replay
  Policy B is fixed: ExactReplay stays disabled and no external ledger is
  introduced. Redis, FINAM, dispatch, callbacks, runtime-live and real orders
  remain closed.
- Stage 5G-e-d-c at `18240b26a5bea77ea71c851f72a644706a7e0b57`
  was held for R1 authority hardening. The R1 implementation is one direct
  successor on `stage5g-lifecycle`: it makes the validated post-application
  token linear and opaque, independently derives candidate/post/restored
  semantic projections, cross-binds application evidence, and exercises all
  14 real failure boundaries plus GRST01–GRST12. Its current mutation evidence
  is 60/60 (424/424 including the predecessor). This remains an in-memory
  paper boundary: there is no external durable journal, fsync/CAS, callback,
  Redis, FINAM, broker dispatch, runtime-live or real order authority. Policy B
  continues to disable ExactReplay.
- Stage 5G-e-d-c R2 is the current closure patch on top of
  `67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf`. R2 keeps the accepted R1
  application architecture but adds a private linear application-source proof,
  field-by-field source/evidence validation, source-bound `captured_at`, a
  two-phase finalized post-application token, final authority over
  `post_restart_package_fingerprint_sha256`, restore-side post-package
  fingerprint recomputation, and real serializer/reconstruction/Policy-B
  failure phases. All live, Redis, FINAM, callback, broker dispatch, real order
  and Stage 6 surfaces remain closed.
- Independent review of R2 at
  `95901eb9bf19e103e9acb82fb9726708f356b4cd` confirmed the production mapping
  is semantically correct but held acceptance because the checker did not
  mutation-seal every source-proof field assignment. Stage 5G-e-d-c R3 is the
  current narrow review candidate on top of `95901eb...`. R3 adds the
  machine-readable source-proof field-map descriptor, exact constructor
  assignment checking, direct production source-map negative mutations,
  independent source-oracle GRST02/03/04/11 witnesses, nonzero history-count
  oracle coverage, parent revision/package-instance cross-binding, and R3
  handoff/gate scripts. It does not open Redis, FINAM, callbacks, broker
  dispatch, runtime-live, real orders, Stage 5G-f or Stage 6.
- Stage 5G-e-d-a R6 was independently accepted and closed at
  `4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`. R1 at
  `9a3221602a902bc6207418f0131665a039d62768` closed the substantive freshness,
  semantic position, identity construction, order-shape and replay-lineage
  defects but was rejected because its current-HEAD checker was not a strict
  superset of the inherited contract. R2 binds exact ordered GRST/disposition/
  operational-field arrays in both JSON and Rust, mutation-protects every row
  chronology class, and applies one no-whitespace/no-control canonical token
  grammar to all Stage 5G identities. R2 at
  `8384a13bc8b7babcb11f6f5bb0f717f1a6c70388` was substantively correct but
  rejected because its current-HEAD checker omitted inherited guards, its
  reducer mutation changed only JSON, and it did not freeze ordered `ALL`.
  R3 added the explicit inherited-invariant inventory, missing focused witnesses,
  a real Rust reducer mutation and exact ordered `ALL` enforcement, but its gate
  remained bypassable through aliases and omitted inherited contract/compound
  guards. R4 froze the accepted production prefix, bound the exact contract and
  closed-surface maps, added alias-aware diagnostics and completed test-only
  witnesses, but its checker did not seal suffix or sibling Rust authority.
  R5 changed no Rust source and froze the complete 19-file runtime-core `src`
  set, but did not cover alternate Cargo target roots or repository-local Cargo
  wrappers. R6 changes no Rust or Cargo semantics: it freezes the full accepted
  R5 project tree outside an exact ten-file allowlist, Cargo manifests/lockfile,
  workspace topology and all 23 runtime Rust targets. The first Stage 5G-e-d-b
  implementation at `8a02f2a6b6e27587539d1e4e4717301bf010e6a1` was rejected:
  it did not bind the complete operational identity or replay lineage to the
  authenticated restart, accepted a trade when one ID matched but the other
  conflicted, calculated post-position from zero, and had incomplete terminal
  status guards. Stage 5G-e-d-b R2 at
  `c5f84bbcf7c1b44c1eac9c2e99857834d333a4c4` was rejected on source LIMIT
  price authority, cancel identity, historical-row partitioning and semantic
  refresh. R3 at `f9bc372f7ad5a56514ce1d6ad7ffd4f54097bb28` was rejected on
  global no-slot history, canonical flat semantics, cancel target authority
  provenance and immutable target-order payload monotonicity. R4 at
  `66c5fbd2518ec2e7398c88bb59cc7e4dae3ce1bd` closed those findings but was
  rejected because exact Rejected/Canceled/Expired truth still created a new
  candidate, safe same-status Canceled/Expired late fills were blocked, and
  missing-owned outcomes lost classified history counts. R5 is the accepted
  reducer predecessor for e-d-c. It retains the R2 restart-owned
  authority for all twelve operational identity fields, conservative replay
  hints without `ExactReplay`, account-wide order safety, source-action and
  monotonicity checks, exact GRST06/GRST11 separation, target identity conflict
  handling and temporary integral-lot-only source admission. It also adds
  restart-bound operational/replay commitments, exact target identity,
  typed intent plus canonical integral Decimal projection, shared exact trade linkage and
  a fail-closed status × fill × completeness matrix. R4 retains the R3 policy that fails all
  source LIMIT recovery closed pending canonical Decimal/tick price authority,
  separates cancel command from target-order identity, ignores and counts only
  unrelated terminal/history rows, uses semantic GRST06 equality without
  receipt timestamps or volatile PnL, and requires complete exact pre-position
  truth for Working/New. R4 globally partitions harmless terminal/trade history
  before GRST01/GRST07, carries history counts without a candidate, canonicalizes
  complete empty and explicit-zero positions as flat, derives cancel target
  client authority only from accepted target-order evidence, and rejects drift
  in every immutable target-order field before lifecycle progress. R5 makes
  exact Filled/Rejected/Canceled/Expired re-observation one generic GRST06
  no-candidate path. It permits GRST11 only for same-status Canceled/Expired
  added immutable trades with exact fill sum, source intent and canonical
  position convergence; Rejected fills, cross-status transitions and all
  regressions remain blocking. Missing-owned and terminal-conflict outcomes now
  retain already classified history counts. Authenticated owning
  fixtures exercise export/restore/validate/bind/reduce and GRST01–GRST12;
  multi-trade row order and parallel evidence are deterministic. It still
  cannot invoke a callback, publish Redis data, use FINAM/HTTP or dispatch an
  order. Its accepted output is now the sole input authority for the active
  Stage 5G-e-d-c review candidate described above.
- Stage 5G-e-b at `cbe4044bbca8303a7852d225364ec5cf89f02386`
  was rejected as submitted because ExactReplay advanced the committed
  checkpoint without synchronizing the continuing Stage 5G-c session. R1 at
  `1621307a6012fa1f9dcbc89a59651c801f6cc26f` fixed the current-package path
  but was rejected because historical exact replay still entered NewPackage
  chronology and current-slot preflight. R2 at
  `6995f8dd2ac226eff33b781f575927361fdc2c45` is independently accepted and
  closes Stage 5G-e-b. Stage 5G-e-c R1 at
  `e269b709d2c3e1a2d3892a88099585bce12d0778` was rejected as submitted because
  its package tests did not reseal the nested lifecycle authority and its
  TimerReady summary/checkpoint lacked source-settlement authority.
  Stage 5G-e-c R2 at `f2f5b1508171632d2e4b211eae79ee6bf3b18178` was rejected
  because all lifecycle commitments still lived inside one rehashable Stage 5G
  extension. Stage 5G-e-c R3 at
  `2394dbcd15d953e1799e07f7c903fdb3b072fc3f` was rejected because its Stage 5D
  anchor remained inside the same coherently rehashable package trust domain.
  Stage 5G-e-c R4 at `3a9fd1106a064ed6c29b1a378cbc02da90b2efc1` was
  rejected because its keyed commitment did not include the complete Stage 5D
  package semantics. Stage 5G-e-c R5 is the only current implementation review
  candidate. It authenticates one versioned canonical semantic projection of
  the complete Stage 5D + Stage 5G restart package with HMAC-SHA256 under an
  opaque operator-managed key that is never stored in the package and is
  zeroized on drop. Package schema/checkpoint state, revision/generation,
  timestamps, source build, semantic/private runtime state, watermarks,
  recovery indexes and riskgate persistence/evidence are covered; only
  circular transport checksums and the HMAC field itself are excluded. Twelve
  coherent-reseal attacks recompute every ordinary checksum and unkeyed hash
  while retaining the old tag, and all fail at the keyed boundary before
  runtime mutation. R5 retains the R4 removal of duplicate TimerReady
  summary/checkpoint copies, persists the versioned recovery receipt projection
  and recomputes its identity. Its acceptance witness coherently reseals every
  unkeyed source/lifecycle hash, Stage 5D anchor, extension, envelope and package
  checksum after mutating summary, history, recovery and checkpoint; restore
  still fails at the keyed commitment boundary. The accepted
  owning exact-replay proof carries the old
  and new checkpoints plus exact canonical evidence, updates the live session
  through the same canonical Stage 5G-c metadata authority, and proves both
  `A→B→exact A→C` and inherited-old-request replay chains. Replay identity and
  fingerprint classification now precedes NewPackage-only continuation,
  current-slot and broker-state checks. Only local sequence and duplicate count
  may change during exact synchronization; slots, identity ledger, BrokerTruth
  receipt watermarks and callback state remain fixed. All public commit
  authority now performs hard checkpoint validation in release builds. The
  accepted Stage 5C callback authority remains byte-pinned to
  `d0494537d7c1739a16350b2d28f71b304165c812`. The new linear clean-process
  boundary consumes one of four supported Stage 5G capabilities, emits only a
  checksummed Stage 5D package, drops the source, and reconstructs semantic,
  private and riskgate state in a freshly configured runtime. Fresh
  BrokerTruth reconciliation and GRST01–GRST12 remain pending; Stage 5G-f
  remains closed.
- Stable macro-roadmap Stage 5 remains active while Stage 5G is incomplete.
  Stage 6 durable command-chain work is not opened by the Stage 5F acceptance.
- Real FINAM `POST`/`DELETE`, Redis live command consumption, runtime-live,
  unattended execution and real orders remain forbidden.

## What exists

- Broker-neutral core contracts for orders, trades, positions, market data,
  readiness, broker truth, runtime host lifecycle, and paper runtime state.
- Stage 3 market-data parity contract is accepted/closed to strategy-input
  level.
- Stage 4A broker-truth bootstrap plan/evidence schema and Stage 4A-1
  plan/schema alignment are accepted as foundation.
- Stage 4B existing broker-truth type inventory and v2 alignment decision is
  accepted.
- Stage 4C validated broker-truth bootstrap wrapper and validation is accepted.
- Stage 4D FINAM read-only broker-truth mapper/source normalization is
  accepted.
- Stage 4E broker-truth to runtime bootstrap application evidence is
  accepted.
- Stage 4F dirty-start / explicit adoption / manual-intervention policy is
  accepted.
- Stage 4G runtime lifecycle ordering evidence is accepted.
- Stage 4H paper/mock runtime-host bootstrap integration tests are accepted.
- Stage 4I redacted operator-facing bootstrap evidence report is accepted.
- Stage 4J FINAM Stage 4 report assembly bridge is accepted.
- Stage 4 macro-stage is accepted/closed as the broker-truth bootstrap
  foundation. The next active macro-stage is Stage 5 — real strategy semantics
  attachment.
- Stage 5A semantic inventory, source provenance, callback/state/configuration
  ledger and evidence schema are a review candidate. No strategy code or live
  execution surface is added by Stage 5A.
- Stage 5B-1 pure Hybrid semantic-kernel import is accepted. The new
  `strategy-runtime-core` crate preserves the frozen model/orchestrator/riskgate
  source and tests but does not include the integrated runtime wrapper.
- Stage 5B-1a immutable correspondence, exact integrated-wrapper oracle and
  active high180 profile hardening are accepted.
- Stage 5B-1b structural freeze hardening and the Stage 5B-2 integrated-wrapper
  semantic inventory are accepted. Workspace membership, crate
  target configuration, library root and the complete Cargo/Rust target set now
  fail closed through parsed TOML plus content/path locks. Bracket
  terminal-reconciliation execution-status matrices, timeout suppression,
  mixed clock domains and source-compatible transient restart behavior are
  inventoried and fixture locked.
- Stage 5B-2a separate wrapper correspondence manifest and broker-neutral
  callback/state boundary hardening are accepted. The map
  matches the exact 15 source overrides and six generic seams, defines lossless
  Hybrid ACK/order/stop/position/bar/context/bootstrap contracts, requires
  context-complete callback inputs, validates attribution/bootstrap
  consistency, and applies an exact parsed-workspace wrapper activation lock
  under the accepted trusted-toolchain/clean-repository threat model.
- Stage 5B-2b exact mechanical wrapper import and public boundary closure are
  accepted. The wrapper is copied and compiled only in
  `strategy-runtime-core`, preserves all source tests, and uses broker-neutral
  request/order/stop identities. Its source-compatible host seam is
  crate-private; the only downstream callback path is the typed
  `BrokerNeutralHybridStrategy` facade, which rejects non-final/non-M10 bars and
  context/payload/target instrument mismatches before state mutation. Runtime
  host attachment, FINAM command consumption and live/send remain disabled.
- Stage 5C-a paper/mock host admission provenance/expiry hardening is a review
  accepted. It accepts only one opaque canonical Stage 4E→4I evidence bundle,
  stores the exact applied snapshot and minimum required-source expiry, rejects
  future or stale evidence at admission time, and binds the complete target
  `InstrumentId`, account scope and instrument price step. It invokes no
  strategy callback and attaches neither a runtime host nor an intent sink.
- Stage 5C-b linear bootstrap-notification type-state is accepted. It
  consumes Stage 4 evidence into strategy-bound admission, then consumes the
  concrete strategy plus admission, rechecks expiry/config symbol/tick binding,
  passes only the exact admitted snapshot into `on_bootstrap_snapshot`, and
  returns `Stage5cBootstrappedPaperStrategy` owning that same strategy instance.
  Every later lifecycle step remains false; active target orders stay blocked
  until ownership-complete mapping is accepted.
- Stage 5C-c runtime-state restore facade hardening is accepted. It
  validates provenance and loads persisted state before exact broker bootstrap,
  matching the ALOR lifecycle, then emits exactly one restored-state callback.
  Quantity/side and broker-owned order-ID postconditions preserve broker truth;
  positive legacy ALOR numeric IDs are genuinely normalized only under an
  explicit policy. Warmup, recovery, bars, intents and execution remain closed.
- Stage 5C-d canonical history warmup facade is accepted. It consumes
  the restored type-state, rechecks evidence freshness and lifecycle timestamp
  monotonicity, and accepts only an opaque Stage 3 provenance/gap-proven batch
  of exact-target final chronological M10 history with no future timestamps.
  It returns an opaque warmed type-state. Recovery, semantic bars, intent sink
  and all execution surfaces remain closed.
- Stage 5C-e pending-stream recovery facade is accepted. It consumes
  the warmed type-state, accepts only complete opaque recovery evidence,
  deterministically deduplicates replayed stream entries and rejects callback
  intents. Semantic bars, intent sink, runtime host and execution remain closed.
- Stage 5C-f first semantic-bar facade is accepted. It consumes the
  recovered type-state and a Stage 3-accepted final M10 capability, captures
  generated intents in an opaque paper result, and attaches no sink, Redis
  command stream or broker transport. Timers and execution remain closed.
- Stage 5C-g paper intent settlement/escrow is accepted. It consumes semantic
  results, validates captured intent shapes and returns an opaque state-bound
  batch without Redis, sink, transport or send.
- Stage 5C-h controlled next-bar loop is accepted. It advances only from
  settled zero-intent batches, preserves unresolved nonzero escrow batches, and
  keeps timer, sink, Redis and broker transport closed.
- Stage 5C-i paper intent lifecycle / ACK escrow resolution is accepted. It
  consumes nonzero settled batches, requires exact ordered ACK coverage for
  captured request IDs, preserves full escrow and typed ACK outcomes, applies
  only broker-neutral ACK callbacks, and still leaves Redis, transport and
  execution closed.
- Stage 5C-j paper broker lifecycle facade is accepted. It consumes
  only Stage 5C-i resolved batches, maps active ACK outcomes to expected
  `Order`/`StopOrder`/`Position` paper evidence, blocks terminal ACKs with
  broker-state events, canonicalizes event sequence, deduplicates identical
  events, and keeps timer, sink, Redis, transport, FINAM command consumer and
  runtime-live closed.
- Stage 5C-k controlled paper timer facade is accepted. It consumes
  only fully resolved Stage 5C-j broker-lifecycle type-state, checks timer
  monotonicity against the ACK/broker-event lifecycle watermark, captures
  timer-generated cleanup attribution before callback mutation, and still keeps
  the timer loop, sink, Redis, transport, FINAM command consumer and
  runtime-live closed.
- Stage 5C-l timer-result settlement facade is accepted. It consumes
  only Stage 5C-k timer type-state, turns zero-intent timers into continuation
  checkpoints, and routes nonzero timer-generated batches back through Stage
  5C-i/5C-j without opening sink, Redis, transport or runtime-live.
- Stage 5C-m timer/bar continuation arbitration is accepted. It consumes only
  Stage 5C-l timer settlements, stores the exact millisecond timer checkpoint,
  allows one ready checkpoint to continue to either one later final bar or one
  later timer, preserves ready settlement on recoverable next-bar blocks,
  exposes the settlement only as an opaque public capability, and blocks
  generated timer batches until Stage 5C-i/5C-j lifecycle resolution.
- Stage 5C-n bounded deterministic paper-loop coordinator is accepted. It
  consumes one accepted Stage 5C type-state and one explicit event per call,
  accepts broker lifecycle only as a terminal-complete atomic Stage 5C-j batch,
  rejects incomplete working-only batches before callbacks while preserving
  ACK-resolved state for retry, settles callback-generated broker intents back
  into ACK lifecycle, delegates only to existing Stage 5C facades, preserves
  recoverable state where available, and keeps autonomous loops, Redis, sink,
  transport, FINAM command consumer and runtime-live closed.
- Stage 5C acceptance/API-freeze package is accepted and Stage 5C is formally
  closed. The closure freezes accepted 5C-a...5C-n slices, startup ordering as
  state-load/clean-prepare -> broker-truth bootstrap ->
  runtime-state-restored, the full paper-host type-state API, manifest
  self-completeness checks, the actual coordinator transition matrix, and the
  no-send execution boundary. Any future change to frozen Stage 5C API/source
  requires an explicit Stage 5C reopening review.
- Stage 5D-a/a2/a3 ownership and additive-extension design is accepted. Stage
  5D-b1 dual-baseline enforcement migration is accepted. Stage 5D-b2a adds the
  schema-only versioned persistence envelope/API surface: snapshot identity and
  revision metadata, write generation, per-family timestamp policy, canonical config
  fingerprint, account/instrument/profile/runtime binding, versioned semantic
  StrategyState JSON payload with a strict Stage5d-owned HybridIntraday schema,
  strict instrument binding, strict unknown-field decoding, checksum
  validation, lifecycle watermarks, typed broker-neutral recovery indexes,
  source-compatible runtime-private enums, runtime-private extension DTOs,
  riskgate ledger/materialized/outbox DTOs, source-exact runtime pending
  riskgate finalization payloads with source-producible weekday vector chronology, an opaque validated-envelope
  capability for future gates, deterministic fixtures,
  corrupt/version/config/unknown-field/semantic/timestamp/source-roundtrip/
  pending-lifecycle/recovery-index/riskgate-finalization negative validation,
  and full public Stage5d API-surface freeze enforcement. Stage 5D-b2b-a opens
  the first controlled runtime-private export/apply bridge, and Stage 5D-b2b-b
  adds controlled broker-truth bootstrap notification after private apply: exact
  loaded-capability/envelope pair binding, validated-envelope gated private
  extension apply, retained opaque restore evidence, retry-capable opaque block
  preservation, required cleanup retry state, full pending
  riskgate-finalization vector restoration, persisted-owned semantic projection
  binding, crate-private persisted-vs-clean load provenance, semantic/recovery
  provenance fingerprints, authoritative Stage 5D canonical config/version/source
  build binding, hashed riskgate seed/ledger identity in the canonical
  fingerprint, pending-entry source-shape/config exactness checks,
  checker-owned exact private-layout extension enforcement with negative cases
  for self-authorized semantic drift, source-private invariant preflight, and no
  authoritative working-set rehydration from persistence. Stage 5D-b2b-b treats persistence working sets
  only as hints, requires broker-truth position match, blocks missing expected
  working orders, blocks stop hints until stop-truth surface opens, and still
  fails closed on confirmed active target orders until ownership mapping opens.
  Recoverable bootstrap blocks can now retry only through
  `stage5d_retry_broker_truth_bootstrap(...)` with a fresh matching Stage 5C
  admission; cross-binding refresh attempts preserve the blocked capability and
  fail closed. Stage 5D-b2b-c adds validated authoritative riskgate ledger
  evidence plus riskgate projection injection through
  `stage5d_inject_authoritative_riskgate(...)` after broker-truth bootstrap and
  before runtime-state-restored. It rebuilds materialized state from
  source-compatible normalized ledger records, checks all
  `RiskGateProfileIdentity` fields against runtime config, blocks disabled
  riskgate-profile callback no-ops, validates ledger-tail hash, enforces
  durable outbox crash-consistency/idempotency identity, and supports controlled
  retry with fresh validated ledger evidence without repeating private apply or
  broker bootstrap. Stage 5D-b2b-c1-r8 closed the review hardening around the
  riskgate-injected capability. It distinguishes full authoritative, durable materialized and
  semantic-runtime frontiers; accepts only exact outbox-explained crash lag;
  requires semantic current-shadow session/PnL to match authoritative materialized
  evidence exactly; rejects negative zero at Stage 5D riskgate authority
  boundaries; validates that the current-shadow tuple is source-producible; and
  retains a deterministic no-I/O recovery plan in the opaque injected capability.
  The plan is bound to
  envelope/evidence/identity/generation and exposes only redacted
  count/completion/fingerprint diagnostics. Source-exact decimal canonicality,
  row-derived `seed_loaded`, exact runtime-pending evidence for every lagging
  runtime frontier and stepwise multi-row recovery-to-complete tests with replay
  checks are enforced. c1-r8 additionally restores the canonical immutable Stage
  5C closure manifest, represents riskgate codec changes through Stage 5D-owned
  controlled semantic extension evidence, validates later processed watermarks by
  the bound source runtime policy, and proves source-produced current-shadow
  positives without post-export editing. CI requires the 82-case Stage 5D harness
  plus the isolated marker-pinned 87-case forbidden harness with positive-baseline and
  self-protection checks. The forbidden harness supported worker contract is
  pinned at default/max four workers, 180-second per-case timeout and a
  75-minute CI timeout. Review
  handoffs remain fail-closed and commit-bound. Stage 5D-b2b-d1-r6 is the active
  runtime-restored acceptance-evidence/blocker-uniformity hardening candidate:
  it consumes only the
  opaque `Stage5dRiskGateInjectedPaperStrategy`, requires complete
  source-produced recovery evidence before the callback, delegates through one
  checker-pinned crate-private Stage 5C bridge, returns the exact
  `Stage5cRuntimeStateRestoredPaperStrategy` on success, preserves the input
  capability on pre-callback blocks, treats post-callback failures as terminal,
  rejects callback intents through the runtime guard before any debug assertion,
  enforces bootstrap-notification chronology before callback, and treats
  flat/long/short broker side truth as exact. The Stage 5D checker pins the
  crate-private bootstrap, riskgate and runtime-state-restored bridges to one
  definition and one production call-site each, with negative cases for direct
  calls, aliases, forwarding wrappers, function references, extra Stage 5D
  calls, missing intent guard, debug-assert-only guard, missing timestamp guard,
  missing exact side guard, missing source-produced Long/Short/realized-PnL
  restored-transition proof, missing single/multi-row recovery restored
  transitions, missing pre-callback state-fingerprint preservation, and missing
  compile-fail type-state guards. r4/r5 additionally pin genuine
  broker-position Long/Short positives through strict JSON round-trip,
  non-empty known-order and pending-request retention through strict
  round-trip, open-position side-mismatch blockers, explicit paper-only and
  non-acknowledged recovery-decision blockers, blocker ownership evidence, and
  source pre-bind exact-state proof. r6 additionally converts every
  representable pre-callback blocker to the common retained-capability helper,
  checks retained paper-only/runtime-host/intent-sink flags, adds a
  machine-readable blocker-ownership inventory, pins ownership drift through
  93 Stage 5D negative cases, and documents strict malformed payload ownership.
  The formal mutation policy is
  `controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_then_restored_callback_only`;
  Redis bridge, FINAM execution, broker transport, runtime-live and autonomous
  loop remain closed.
- Stage 5D final restart r2 is retained only as the canonical durable-package
  foundation. Stage 5D-final-restart-r3 now has exactly 21 accepted executable
  rows and 0 `todo_source_produced` rows in the mandatory positive inventory.
  The latest riskgate-recovery r1-r4 evidence closes the final three riskgate
  rows with a crate-private production recovery executor, typed checkpoint and
  final receipts, fresh reader reopen checks, pre/post-commit crash idempotency,
  persisted final-receipt equality, source-produced pending finalizations,
  exact summary goldens and Stage 5C warmup continuation. Aggregate Stage 5D
  closure r2 is accepted as the no-I/O persistence/restart/recovery-semantics
  foundation. Stage 5E-B3F is accepted and closed at
  `e14654f7129aa61011931306140a3bfefe2fcfbc`: the one private callback and
  settlement route is protected by the accepted B3F checker, immutable source
  pins, two complete semantic-region digests, the 580-case provenance harness
  and the eight-case production UI contract. Its descriptor remains a closure
  descriptor and is not repointed by later work.
- Stage 5F is accepted and closed at
  `fb8245e2f91cfc1678548a1228e8558d9adc2181`. Its lineage began with the
  inherited Stage 5F-a atomic-Hybrid entry contract, which has a
  separate descriptor and gate that first runs the accepted B3F checker and UI
  harness from the exact accepted source snapshot. Canonical CI fetches that
  immutable predecessor and uses the same detached-snapshot wrapper as handoff
  packaging for the inherited 580-case provenance matrix; it never applies the
  frozen Stage 5E gate to a Stage 5F head. Stage 5F-a-r3 executes the verified
  wrapper before any Stage 5F repository-owned harness and freezes the workflow,
  wrapper, gate, verifier and both negative harnesses as one accepted authority
  set. Stage 5F-a-r6 restores the R3 gate bytes, so canonical CI, inventory,
  entry checker, handoff checker and the actual gate carry one exact digest.
  Its protected-base, read-only `pull_request_target` authority workflow treats
  PR content only as data. Ordinary PRs preserve the R3 and protected-base
  roots; a dedicated, versioned rotation manifest can advance them only when
  the old base contract binds the exact base SHA, state generation, complete
  candidate `{git_mode, sha256}` bindings and a coherent gate digest. R7 reads
  the committed Git tree, rejects gitlinks/non-blobs/unsupported modes and
  mode-only authority drift, blocks all ordinary PR changes under
  `.github/workflows/**`, and permits only the exact base-authority workflow in
  a reviewed rotation while canonical `ci.yml` remains immutable. Rotation
  admits only Stage 5F governance/docs/fixture paths, never Rust or operational
  surfaces. The first hosted activation exposed an undeclared `rg` dependency
  in the forbidden-surface scanner; the negative harness failed safely rather
  than accepting seven undetected scanner mutations. Stage 5F-a-r8 is merged
  as the generation-2 authority repair and R8a is merged as its governance-only
  generation-3 amendment. The present R9 generation-4 candidate replaces the
  scanner's six `rg` searches with the already-required Python 3.11+ path and
  makes the worker's infrastructure check portable; it atomically rebinds only
  the paired Stage 5D freeze manifest/checker. Its full 87-case negative
  harness passes with `rg` absent from `PATH`. Schema and generation values are
  exact JSON integers (never `true` or a float), and the capability cannot be
  replayed after the generation-4 state. R9 itself changed no workflow,
  Cargo/Rust or operational surface. A bounded R9a CI-preparation patch then
  primes only the exact B3F snapshot's locked Cargo dependencies before the
  unchanged offline UI harness runs on a clean hosted runner; it adds no
  runtime, broker, transport or order path. GitHub
  activation still requires an independent approval after the latest update,
  strict up-to-date branch and no direct/admin bypass. It then
  admits only the existing
  IMOEXF `canonical_final_m10` paper route through the broker-neutral Hybrid
  runtime, high180/riskgate, the single Hybrid orchestrator, ordered semantic
  intents and B3F settlement. Full BO/MR/riskgate/arbitration parity is one
  state-machine acceptance matrix; no BO-only or MR-only slice can claim
  success. ACK/order/position/timer/restart feedback remains Stage 5G and
  same-input ALOR differential replay remains Stage 5H. Redis, FINAM, broker
  transport, dispatch, persistence opening, runtime-live and real execution
  remain closed.
- FINAM REST read-only/auth/client DTO and mapper foundation.
- FINAM WebSocket market-data shadow path for `BARS`/`QUOTES`.
- Closed-bar finalizer and FINAM M1-to-canonical-M10 paper runtime path.
- Paper-only hybrid runtime-state projection.
- ALOR-oracle seeded FINAM paper runtime state for IMOEXF hybrid parity:
  previous-day features, current-day features, `next_cycle_seq`, and riskgate
  summary can be seeded from the ALOR runtime state stream before paper
  processing.
- Guarded operator one-shot actual FINAM order harness for controlled
  `MARKET`/`LIMIT`/`CANCEL` micro checks.
- Durable order-path and endpoint-boundary design/evidence for guarded
  one-shot use.

## Still disabled

- Continuous runtime-live trading.
- `command-consumer-to-real-FINAM`.
- Strategy-runtime-to-real-FINAM order routing.
- Runtime `LiveReady` for FINAM.
- Stop/SLTP/bracket/replace/multi-leg.
- RI/RTS expansion.
- Any automatic live send from strategy intents.

## Current parity status

The FINAM contour is a paper/shadow parity stand, not a drop-in replacement for
the ALOR gateway/runtime yet.

Stage 1B hard-freeze scope:

- in scope: IMOEXF `HybridIntradayRuntime` paper/shadow parity;
- out of scope: USDRUBF `AlorUsdrubfHybrid`, RI Author41/42,
  `SessionGapStandalone`, generic `CancelSent`/`Done` migration,
  Stop/SLTP/bracket, runtime-live.

Stage split:

- Stage 1A is a draft/spec foundation: README/status/workplan, seeded bridge,
  and safety boundary.
- Stage 1B is accepted as the hard compatibility-freeze work for IMOEXF
  `HybridIntradayRuntime` paper/shadow parity: field-by-field mappings, Redis
  stream/group mapping, fixtures, seed-required policy, accepted ADR, and
  stronger evidence.
- Stage 2A is accepted and closed: runtime source migration inventory and plan
  for the accepted broker-neutral `BrokerOrderId(String)` path are complete.
- Stage 2A-final inventory completion added concrete `HybridIntradayRuntime`,
  `trade_ledger`, runtime command-builder, and ALOR cancel/replace DTO surfaces
  to the migration inventory.
- Stage 2B implementation plan is accepted. Controlled runtime source migration
  implementation proceeded paper/mock/local only, in small reviewable patches.
  Runtime-live and the real FINAM command consumer remain blocked.
- Stage 2B is closed as the runtime source migration contract/foundation after
  Stage 2B-11 acceptance report. Stage 2B-N patch gates were implementation
  safety gates inside Stage 2 and do not replace Stage 3 market-data parity or
  later macro-stages.
- Stage 2B-1 and Stage 2B-1a are accepted. Stage 2B-2 adds passive DTO/state
  migration contracts for old ALOR numeric order ids and broker-native string
  ids; it does not attach runtime-live or real FINAM command consumption.
- Stage 2B-3 is accepted. It validates passive runtime order maps and bootstrap
  working-order maps: map keys must match payload broker order ids, and missing
  known order ids become readiness/manual-intervention blockers.
- Stage 2B-4 is accepted as CommandAck / OrderEvent / TradeEvent lifecycle
  boundary foundation.
- Stage 2B-4a is accepted: explicit ACK status policy hardening is complete.
- Stage 2B-5 is accepted as passive RuntimeCaches / ownership tracking
  foundation.
- Stage 2B-5a is accepted: explicit order ownership / attribution hardening is
  complete.
- Stage 2B-5b core BrokerTradeId invariant is accepted.
- Stage 2B-5c broker-finam trade_id fallible mapping is accepted.
- Stage 2B-6 TradeLedger migration is accepted as broker-neutral foundation.
- Stage 2B-6a TradeLedger blocker lifecycle and duplicate fill replay hardening
  is accepted.
- Stage 2B-7 HybridIntradayRuntime-owned id migration is accepted as
  broker-neutral contract layer.
- Stage 2B-8 command builders / CancelOrder / ReplaceOrder DTO shape migration
  is accepted.
- Stage 2B-9 deterministic request-id stability is accepted.
- Stage 2B-10 combined paper/mock compatibility test pack is accepted.
- Stage 2B-11 acceptance report closes Stage 2B as broker-neutral runtime
  contract/foundation.
- Stage 3 followed Stage 2B and is now accepted/closed as market-data parity to
  strategy input level.
- Stage 3A is accepted: market-data parity plan and evidence schema are
  accepted as the planning/schema foundation.
- Stage 3B is accepted after Stage 3B-1 hardening: source-only market-data
  parity comparator contract, synthetic fixture tests, strict M1-only
  derivation, and publication counters that do not treat blocked candidates as
  strategy/model bars.
- Stage 3C is accepted after Stage 3C-1 hardening: source/report-only
  multi-bucket redacted report generator with explicit duplicate bucket
  normalization and no silent overwrite.
- Stage 3D is accepted as offline/source-controlled evidence collector
  foundation: controlled inputs, FINAM M1-to-M10 derivation, Stage 3C report
  invocation, source/session metadata, and redacted JSON artifact writing.
- Stage 3D-1 is accepted as recovery/session/input-gate hardening foundation:
  blocks failed or missing recovery, unknown schedule state, invalid ALOR oracle
  shape, invalid source archive hashes, and invalid session dates before the
  report can be treated as synchronized strategy-input evidence.
- Stage 3D-2 is accepted: recovery/session consistency hardening rejects
  `AttemptedAndComplete` unless replay was attempted, gap absence was proven,
  the first fresh live final bar was observed, and entry stayed blocked while
  the gap was unproven; unknown schedules must remain blocking.
- Stage 3D-3 is accepted as offline controlled operator-run input adapter
  foundation: it reads approved/redacted ALOR native M10 and FINAM final M1
  source files, validates source kind/session/instrument/finality, invokes the
  Stage 3D collector, and writes a redacted parity report plus counts-only
  operator summary.
- Stage 3D-3a is accepted as approved input schema/session-window hardening:
  approved input source schema is
  explicitly versioned as v2 and config/source `session_window_utc` is required;
  ALOR/FINAM bars outside the approved window are rejected before evidence is
  accepted.
- Stage 3E is accepted as recovery/gap evidence foundation: reconnect/gap
  recovery evidence wraps the broker-neutral market-data recovery report, proves
  entry is blocked while gap is unproven, keeps exit/cancel/repair unblocked by
  that entry guard, suppresses replay/overlap bars from strategy/model
  publication, and allows strategy input only after gap proof plus first fresh
  live final bar.
- Stage 3E-1 is accepted as recovery-report consistency and counter hardening:
  recovery report must be M10 strategy timeframe, recovery timestamps must sit
  inside the approved session window, reconnect summary/report phases must not
  contradict each other, and post-recovery publication counters must be
  arithmetically valid.
- Stage 3E-2 is accepted as replay-window evidence completeness hardening:
  `RecoveryComplete` now requires explicit replay-window evidence fields,
  positive replay bar count, valid replay-window ordering, and first fresh live
  final strictly after replay.
- Stage 3E-3 is accepted and closes Stage 3E: replay window must cover the last
  final strategy-bar watermark, recovery mode must match warm/cold attempt
  flags, and `checked_ts` must not precede the first fresh live final.
- Stage 3F is accepted as the Stage 3 market-data parity acceptance report.
  Stage 3 is accepted/closed as market-data parity to strategy-input level.
- Stage 4 was accepted/closed as broker-truth bootstrap into runtime. Current
  active macro-stage is Stage 5: real strategy semantics attachment.
- Stage 4A is accepted/closed as broker-truth bootstrap
  planning/evidence-schema foundation.
- Stage 4A-1 is accepted/closed as plan/schema alignment: the Stage 4
  breakdown is expanded to 4A–4J; existing broker-truth/runtime-host type
  inventory is required before coding; lifecycle-order, explicit adoption,
  ownership/correlation, and numeric freshness evidence are represented in the
  schema.
- Stage 4B is accepted as existing broker-truth type inventory and v2 alignment
  decision. It chooses reuse/extend/wrap decisions around the
  existing `BrokerTruthSnapshot`, `RuntimeHostBootstrapSnapshot`,
  `RuntimeBootstrapSnapshotDto`, FINAM mapper, M3f/M3g issue machinery, and
  broker-truth parity helpers.
- Stage 4C is accepted after P1 hardening and final adoption-count guard as a
  validated wrapper around existing
  `BrokerTruthSnapshot` and `RuntimeHostBootstrapSnapshot`, with broker-truth
  source status, safe schedule freshness semantics, strict adoption validation,
  stronger target trade correlation, freshness,
  ownership/correlation summaries, dirty-start/adoption disposition, restored
  runtime-state checks, external issue bridge, and closed safety boundary.
- Stage 4D is accepted as FINAM read-only broker-truth source-normalization
  into the Stage 4C validator. It adds explicit FINAM source evidence
  (`Present`, `Missing`, `Unavailable`, `DecodeFailed`, `Incomplete`),
  per-section freshness for positions/orders/trades/cash/instruments/schedule,
  target-bound schedule state handling, placeholder snapshot semantics for
  missing/unavailable/decode-failed source, and fixture-backed blockers for
  active/unknown target orders, unowned target trades, missing/ambiguous
  instrument identity, stale source sections, and schedule-symbol mismatch.
- Stage 4E is accepted as an application-evidence gate around validated broker
  truth. Runtime bootstrap notification is allowed only for an
  internally consistent `BootstrapReady` report; all
  incomplete/stale/mismatch/unknown-schedule/manual/evidence/safety statuses
  remain blocked, and contradictory `BootstrapReady` reports are rejected as
  `ValidatedBootstrapInconsistent`. Restored runtime state is accepted only
  after broker truth, cannot overwrite broker truth, target/account scopes stay
  separated, and live/execution authorization remains closed.
- Stage 4F is accepted as a dirty-start policy gate after Stage 4E. It carries
  full adoption evidence into the application/operator decision,
  evaluates position and order adoption separately, requires explicit
  attempted/allowed/applied adoption with exact broker-truth qty/count matches,
  requires Stage 4E application evidence to exactly match the canonical
  decision for the same validated report,
  treats runtime-owned active target orders as non-adoptable lifecycle truth,
  keeps non-target account-wide dirty state diagnostic by default, and still
  forbids runtime-live, real FINAM command consumption, POST/DELETE, and
  Stop/SLTP/bracket.
- Stage 4G is accepted as runtime lifecycle ordering evidence after accepted
  Stage 4E/4F. It requires canonical application/policy evidence, validates
  ALOR-compatible lifecycle order, suppresses final bootstrap notification on
  any lifecycle blocker, and still forbids runtime-live, real FINAM command
  consumption, POST/DELETE, and Stop/SLTP/bracket.
- Stage 4H is accepted as paper/mock runtime-host bootstrap integration tests.
  It emits a deterministic mock runtime event trace only after accepted Stage
  4G and emits no bootstrap/restore/warmup/pending events for stale broker
  truth, unknown schedule, manual intervention, noncanonical policy, invalid
  lifecycle order, live authorization attempts, or internally
  inconsistent/tampered Stage 4G lifecycle DTOs.
- Stage 4I is accepted as a redacted operator-facing bootstrap evidence report.
  It summarizes Stage 4C validation, Stage 4D per-section source evidence,
  Stage 4E application, Stage 4F dirty-start/adoption policy, Stage 4G
  lifecycle ordering, and Stage 4H mock runtime trace. Required non-present
  source evidence blocks the report; blocked reports carry an explicit reason
  chain and emit no runtime events. Runtime-live, real FINAM command
  consumption, POST/DELETE, and Stop/SLTP/bracket remain forbidden.
- Stage 4J is accepted as the broker-core/FINAM Stage 4 report assembly
  bridge. It builds the full Stage 4C→4I report from a FINAM Stage 4D read-only
  package using the preferred source-evidence path, not the synthetic
  compatibility builder. It remains report/evidence only.
- Stage 4 is accepted/closed as broker-truth bootstrap foundation. It provides
  the accepted read-only/paper evidence chain required before real strategy
  semantics can attach. It does not authorize runtime-live, real FINAM command
  consumption, POST/DELETE, or Stop/SLTP/bracket.
- Next active macro-stage: Stage 5 — real strategy semantics attachment.

Green / mostly closed:

- FINAM WS live market-data reaches Redis.
- Fresh M1 final bars can produce canonical M10 runtime input.
- FINAM paper runtime state can now match ALOR IMOEXF hybrid state on the active
  M10 bar after ALOR-oracle seeding.
- ALOR-oracle seed now preserves pending/deferred/safe-mode/protective-state and
  dirty-start/manual-intervention placeholders as explicit paper parity fields.
- `seed_required=true` can hard-block a parity run when the ALOR oracle seed is
  missing or cannot be parsed.
- Stage 5C is closed and Stage 5D has a final restart-closure r2 candidate:
  canonical source-owned export from the actual `HybridIntradayRuntimeStrategy`
  to a strict Stage 5D restart package containing both the persistence envelope
  and durable riskgate ledger evidence. The r2 path proves strict package JSON
  decode after source drop, package/evidence checksum validation, loaded-state
  binding, private apply, broker-truth bootstrap, authoritative riskgate
  injection, return to the Stage 5C restored capability, explicit history
  warmup continuation, durable crash/replay states and golden-vector
  determinism in paper/no-send tests.
- Safety flags remain closed in paper state:
  `live_orders_enabled=false`, `runtime_live_ready_enabled=false`,
  `command_consumer_to_real_finam_enabled=false`,
  `external_order_endpoint_enabled=false`, `stop_sltp_bracket_enabled=false`.
- The isolated Stage 8B paper VPS now runs Redis and the unseeded P0 runtime
  projection service. A reproducible Redis DB 15 synthetic smoke consumed ten
  contiguous final M1 bars, formed one complete M10 runtime input, committed
  one paper runtime-state batch, XACKed all ten source entries, left no PEL or
  DLQ entries, preserved operational DB 0 and cleaned DB 15. No FINAM request,
  full-trade token, broker dispatch, real order or Generation-2 private
  material was used.

Amber:

- Full-session operator FINAM-vs-ALOR M10 evidence is still required before
  runtime-live/cutover decisions; Stage 3F closes the input contract but does
  not replace later same-session strategy evidence.
- Stage 4 makes validated broker truth and lifecycle ordering available as the
  mandatory foundation, but the real Hybrid strategy has not yet consumed that
  chain.
- Paper runtime projection has ALOR-compatible fields, but it is not yet the
  real ALOR hybrid BO/MR orchestrator.
- Riskgate state can be seeded/projected, but true riskgate ledger integration
  is not complete.
- Stage 8B P0 on the isolated VPS is operational only for transport/runtime
  projection. Its WebSocket service remains disabled until a separate FINAM
  token with `readonly=true` is supplied. The current local full-trade token is
  rejected by the paper preflight and has not been copied to the VPS.
- Stage 8B-P1 durable paper lifecycle composition is a design-review candidate.
  The accepted Stage 7B service is a library authority, not yet a deployable
  full strategy host. P1 must solve source-produced Stage 5G bootstrap,
  commitment-key custody, single-owner semantic bar continuation and one
  durable paper outcome/projection authority before operational activation.
- Stage 5D final restart r2 closure is still a review candidate until accepted.
  It proves the clean-process paper/no-send restart path through a durable
  package boundary and scenario inventory, but does not authorize Stage 6+
  durable command-chain work, command consumers or live execution.

Red / not yet implemented:

- Real ALOR strategy-runtime semantic attachment.
- Runtime command consumer under paper/mock ACK parity.
- Runtime-driven live micro.
- Orders/trades/positions streaming or polling reconciliation loop at ALOR-level
  maturity.
- Any default or implicit `i64` surrogate adapter for FINAM broker order ids.

## Required gates before runtime-driven live

1. ALOR runtime compatibility contract v1 accepted.
2. Runtime source adaptation vs binary-compatible adapter ADR accepted.
   Current accepted decision: runtime source migration to broker-neutral
   `BrokerOrderId(String)`; surrogate adapter remains forbidden without a new
   ADR.
3. Stage 3F accepted, plus any additional full-session operator parity evidence
   required by the later runtime-live/cutover review.
4. Broker truth bootstrap wired into runtime lifecycle.
5. Real hybrid BO/MR/riskgate semantics attached behind paper boundary.
6. Request-id/client-order-id/broker-order-id durable chain implemented.
7. Runtime command consumer proven in paper/mock ACK mode.
8. Orders/trades/positions reconciliation loop accepted.

Only after these gates should `command-consumer-to-real-FINAM` or
runtime-driven live micro be discussed.

## Stage 8A-0 / Stage 8A-1 closure history (2026-08-15)

- Transition Gate 7→8 R3 was independently accepted at
  `630bef3fb9aa07bbc377498fc052f085155a043c`.
- Stage 8A-0 refreshes the current public FINAM PLACE/CANCEL, enum/status and
  instrument prerequisite contracts and reports `MATCH` against the existing
  builder, classifier and registry sources.
- The slice is docs/evidence/checkers only. No production Rust, Cargo or CI
  workflow files change.
- The Stage 8A-0 R1 scanner and exact 41-case negative harness passed and its
  immutable handoff was independently accepted at `c949d7f`.
- FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain
  closed.

Stage 8A-0 R0 at `104cdf8f1ff0a645a5681eae653962ba59016123` was not
independently accepted due to regression-runner and handoff-evidence gaps. The
R1 candidate restores all-target serialized workspace/doc tests, packages and
SHA-binds every gate log, records the inherited Redis timing witness and raises
the mandatory matrix/negative count to 41/41. FINAM contract snapshot/parity
remain unchanged at `MATCH`; that external acceptance opened only Stage 8A-1.

Stage 8A-0 R1 was independently accepted and closed at `c949d7f` on
2026-08-15. Stage 8A-1 R0 at `29b8686` was not accepted because its opaque
capability could still be minted from caller-constructible proof data. The
Stage 8A-1 R1 at `ef6b9ac` was not accepted because its seal bridge was cached,
accepted-only and its non-durable proofs lacked production issuers. R2 at
`1664236` closed dispatch-ready/current-seal authority but was not accepted
because its caller-selected root, free-form arm nonce and PLACE-only
continuation remained forgeable or asymmetric. The accepted R3 implementation adds
an owner-mediated identity-pinned authority root with external config anchor,
opaque trusted current sources, one arm per durable request and symmetric
PLACE/CANCEL revalidation. The 76-row matrix and exact 70-case negative harness
are mandatory. R3 was independently accepted at `1ff0415`; it opened only
Stage 8A-2 request-builder composition behind a strict no-send boundary. HTTP,
Redis live consumer, broker dispatch, runtime-live and real strategy orders
remain closed.

Stage 8A-2 R1 was independently accepted and closed at
`16180ac4f8eab761b3b055c1f5515f62cd94bfb9`. Stage 8A-3 R1 at `aeef1bd` was
not accepted due to venue-symbol binding, historical-classifier alias closure
and current-stage governance findings. Stage 8A-3 R2 is the active corrective
candidate with independent acceptance pending. It remains an endpoint-specific,
local-observation FINAM PLACE/CANCEL classifier with no network send. The
2026-08-15 official REST refresh exactly matches the accepted Stage 8A-0
PLACE/CANCEL document hashes. Historical M3d2 classification is not Stage 8
authority. Stage 8A-4+, transport, Redis live consumption, broker dispatch,
runtime-live and real orders remain closed.

Stage 8A-3 R2 was independently accepted and closed at
`012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`; its final review SHA-256 is
`2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232`.
Stage 8A-4 Design R2 was independently accepted and closed at
`cc58c10d22db312cd83640f1c1e7fd86861a4594`; its final review SHA-256 is
`43315b4653482998f0d112adbdcfc857afde8d1b68de94b3663b929c1ebad99e`.
Implementation R1 at `245fea18f3f22bd4233eed4f9207445efd0a6d46`
was not accepted due to four pure-reducer P1 findings. Implementation R2 at
`3c445aef6dce3f38a81ee477eaa73e56ffdc0a80` closed those findings but was not
accepted due to three narrow P1 findings. Implementation R3 at
`5a846f9852c7fe58b9a24891c22e9d9dafeefd27` closed those findings but widened
trade support beyond Design R2. Implementation R4 was independently accepted
and closed at
`4caf07c16ddad021add7cffe6e887165e49e1bf0`; its final review SHA-256 is
`0f8de37819ccc005bbc609bc21f029f5783ccdd43c0a634b4c09614f507c2a0a`.
Durable-composition Design R1 at
`80fe35ef67e335540e0984781f63a99af794bfe1` was not accepted because it left
post-append seal, stable replay identity, post-effect controls and hold
settlement ambiguous. Durable-composition Design R2 was independently accepted
and closed at `6ddf54ef9d7f740dc59cd2450e78301be3d068cb`; its final review
SHA-256 is `160b674d661982b6dbaa6248c2c4acaf883543cb8be99318ef04b0787492f4ba`.
Implementation specification R1 at
`e3d0ac39dcff25439a7e78f51142b852d8347a2f` was not accepted because the full
V2 causal envelope, partial mixed-batch replay, complete-record suffix binding
and optional-ID compatibility projection were not fully frozen. The active
slice is corrected Stage 8A-4 durable-composition implementation specification
R2. It closes those specification gaps while retaining additive V2 and immutable
V1. Acceptance may open only I1 canonical codec and mixed replay, with no
writer. No production durable apply or execution surface is open.
Durable apply/journal,
ACK/readiness publication,
`ProvenNoMatch`, retry/resend, FINAM POST/DELETE, Redis live consumption, broker
dispatch, runtime-live, real orders, Stage 8A-5 and Stage 8B remain closed.
