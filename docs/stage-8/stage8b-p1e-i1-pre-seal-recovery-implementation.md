# Stage 8B-P1-e I1 pre-seal administrative recovery

Status: source-review candidate. Accepted predecessor:
`5e2e157e032406fdbb9047c33c641f5973514504`.

The predecessor is independently accepted as transaction V5 / receipt V2 /
provenance source with P1-TX01 closed. This slice implements only the next
authorised first-boot recovery boundary. It is not complete deployable I1.

## Exact authority boundary

`authorize_stage8b_p1e_pre_seal_recovery_v5` binds one opaque, one-shot
selector to:

- the validated operational identity and runtime configuration;
- one exact 64-character lowercase transaction ID;
- one typed action from the seven pre-seal actions;
- `RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V4`.

`recover_stage8b_p1e_first_boot_pre_seal_v5` consumes that selector and a
freshly reconstructed F00-F16 source package. Before mutation it authenticates
the source, recomputes the transaction ID from its original bootstrap attempt,
performs a fresh all-predicate V5 classification and requires the classification
action to equal the selector exactly. A stale or cross-transaction selector is
not reinterpreted.

The seven actions are:

```text
remove-marker-temp
resume-prepared
quarantine-root
finalize-quarantine
complete-prepared-to-root-published
complete-root-published-to-journal-durable
complete-journal-durable-to-seal-committed
```

The selector and returned runtime owner remain linear. Neither exposes a raw
journal, lifecycle key or filesystem descriptor.

## Continuation and retained evidence

`resume-prepared` and the three marker-temp completion actions continue the
same authenticated transaction. Existing journal recovery crosses a new
crate-private Stage 7 seam that opens the exact journal under the normal writer
lease and requires it to be empty. It cannot create another journal and it
cannot commit the initial seal before the authenticated JournalDurable marker
frontier. The final continuation uses the unchanged receipt V2 and adopted
marker protocol.

`quarantine-root` accepts only RootPublished/no-journal or
JournalDurable/existing-journal with no seal. It performs a no-replace rename
into the pre-provisioned transaction-ID child, fsyncs both parents and verifies
that the inode-bound canonical root identity survived the move. It never
deletes the root.

`finalize-quarantine` fsyncs the retained root, moves the authenticated marker
into that exact root with no-replace semantics, fsyncs both affected
directories and rereads the marker. The finalized tree remains nonauthoritative
evidence. A later ordinary bootstrap must use a generation greater than every
authenticated retained transaction for the same operational identity.

## Response-loss evidence

Filesystem tests cover:

- exact-selector rejection with a full path/type/mode/content snapshot;
- Prepared, all three pre-adoption marker-temp frontiers and convergence to one
  Adopted authority;
- RootPublished and JournalDurable quarantine/finalization;
- rejection of bootstrap generation reuse after retained quarantine history;
- response loss after marker-temp rename, root quarantine rename and retained
  marker rename, followed by fresh deterministic reclassification;
- absence of a second journal, second callback or recursive deletion.

## Deliberately deferred

The deployable `bootstrap-recover` CLI/systemd composition, owner loop and the
release subprocess signal/panic/SIGKILL/restart matrix remain separate I1
slices. The four already accepted post-seal recovery actions are unchanged.

Operational Redis DB0/DB15, VPS activation, operational credentials, FINAM
POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
