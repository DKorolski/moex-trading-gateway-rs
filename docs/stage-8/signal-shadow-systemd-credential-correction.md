# Signal-shadow systemd credential correction — source candidate

Baseline: c0f7bba1539c50bfdcac3ca5363d910f35b93647.

The authorized 26 October bounded attempt passed preflight and started at
07:00:03 MSK, then exited 1 with `credential_permissions`, before output creation.
This is not the expected interrupted bounded ending: no rotation or observed DATA
was demonstrated. The stop timer subsequently completed successfully at 07:24:40.

A separate native unit with dummy credentials and PrivateNetwork confirmed the
delivery contract: regular root:root 0440 file, root-owned 0550 credential
directory, separate read-only tmpfs mount, readable but not writable by uid999.
The original secret remains root:root0600. Neither real credentials nor FINAM
were used by this diagnostic. The operational shadow service was unchanged.

The CLI retains its private-file check. The only additional accepted case is
Linux `/run/credentials/signal-shadow.service/finam-read-only-secret`, matching
CREDENTIALS_DIRECTORY, root:root0440, root:root0550 parent, canonical path and a
unique exact read-only tmpfs/ramfs mount in `/proc/self/mountinfo`. Environment or
0440 mode alone does not grant acceptance. Mount-level ro is checked, not the
underlying superblock flag (systemd's superblock can remain rw).

No strategy, feedback, auth refresh, REST/WS, source inputs or execution surface
changes. The source secret provisioning contract is unchanged. No blanket chmod
or group-readable-secret exception is introduced.

Validation on macOS: four new predicate/file tests PASS; full
`cargo test --offline --locked -p finam-signal-shadow` PASS;
strict all-target clippy PASS; fmt/diff checks PASS.
The tests include the native captured mount shape, writable/wrong/duplicate
mount rejection, altered modes/owners, private file compatibility and symlinks.

Limit: the corrected Linux ELF has not yet been built or exercised on the native
systemd positive path. That remains release qualification after source review.
The native diagnostic proves the cause, not successful corrected observation.
The original attempt is consumed; no retry or replacement window is authorized.
