# Stage 8B-P R2A4 production authority topology

Status: controlled qualification only. R2B remains closed until an independent
review accepts this slice and a separately signed operator run package is
issued. The production key ceremony is complete; private material remains
outside the repository and no FINAM credential was created or read.

The R2A4 helper keeps the R2A3 GET-only FINAM boundary. It adds no broker
effect route. Before any credential read it verifies a signed package bound to
the exact run identity, manifest, account HMAC/key generation, effect build,
helper, contract snapshot, trust set, eleven source generations and binaries,
and operator decision.

## Source topology

Each authority has a root-owned authoritative store, a dedicated producer UID,
a producer-owned source/generation directory, a distinct issuer UID and an
issuer-owned receipt directory. No issuer writes into another issuer's parent.
The systemd instances are addressed by numeric UID (`@8101` … `@8111` and
`@8201` … `@8211`). Each executable derives its source from its effective UID;
an optional source argument is only an assertion and cannot redirect authority.
Compact sysusers names avoid platform username-length limits.

| Source | Producer UID | Issuer UID | Reader contract |
|---|---:|---:|---|
| trusted_clock | 8101 | 8201 | linux-clock-realtime-boottime-reader-v1 |
| stage7b_current_recovery_seal | 8102 | 8202 | stage7b-current-recovery-seal-reader-v1 |
| stage6_exact_dispatch_ready_command | 8103 | 8203 | stage6-journal-dispatch-ready-command-reader-v2 |
| stage8a_root_config_policy_control | 8104 | 8204 | stage8a-root-config-policy-reader-v1 |
| composite_readiness | 8105 | 8205 | stage8a-composite-readiness-reader-v1 |
| kill_switch_run_allowed | 8106 | 8206 | stage8a-persistent-kill-switch-reader-v1 |
| single_finam_ownership | 8107 | 8207 | stage8a-single-finam-lease-reader-v1 |
| schedule | 8108 | 8208 | stage8a-schedule-window-reader-v1 |
| instrument_specification | 8109 | 8209 | stage8a-instrument-registry-reader-v1 |
| ambiguity_orphan_unresolved_lifecycle | 8110 | 8210 | stage8a-lifecycle-ambiguity-reader-v1 |
| durable_micro_budget | 8111 | 8211 | stage8a-durable-micro-budget-reader-v1 |

The producer accepts only the closed source-specific claim schema and a
monotonic non-zero store generation. It atomically publishes a snapshot and
durably consumes that generation. The issuer verifies source owner, schema,
nonce, producer identity and pinned source key before atomically publishing a
signed receipt. Both producer and issuer executable hashes enter the signed
receipt and the package's source-generation commitment.

## Key and rotation model

The root-owned trust manifest binds each source to an Ed25519 key ID,
generation, public key fingerprint and validity interval. Its aggregate public
key-set digest is signed into the exact run package. The helper additionally
pins the package authorization public-key fingerprint at compile time. A key
rotation invalidates existing packages and requires a new reviewed R2A4/R2B
authority.

The account-key manifest binds generation to an exact relative key path,
fingerprint and validity interval. The signed package binds the manifest hash
and exact generation. Account ID, account key and read-only secret use exact
UTF-8 single-line grammar with at most one terminal LF; general trimming is
forbidden.

## Controlled Linux rehearsal

`scripts/stage8b_p_r2a4_linux_rehearsal.sh` uses synthetic credentials and a
loopback TLS server. It exercises the exact fixed directories, all 22 UIDs,
producer/issuer/package binaries, package signature, nonce registry, fd-bound
launcher and complete PLACE (2 POST + 3 GET) and CANCEL (2 POST + 4 GET)
sequences. The production `--r2b-one-shot` entry does not accept the controlled
endpoint or caller-selected paths.

No real FINAM credential, broker GET, order POST/DELETE, Redis consumer,
dispatch attempt, effect transport or runtime-live path is enabled by R2A4.
