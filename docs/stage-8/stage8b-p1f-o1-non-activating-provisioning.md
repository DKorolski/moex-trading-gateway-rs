# Stage 8B-P1-f O1 — non-activating provisioning package

Status: **REVIEW CANDIDATE — PACKAGE PREPARED, EXECUTION NOT AUTHORIZED**.

O0 is closed at `98148b80dacddf44c58204c1af9403bb6b47f8d3`; its governance
closure is `9c0560b46dc54132fd65e80a6e3ce89ba13d7832`. O1 prepares one
immutable package for the fixed target but performs no SSH connection and no
remote mutation.

## Bound materials

The Linux x86_64 release binary is rebuilt from the accepted P1F-Ie source
`940377ab2bd406be31547200ca0b8cc3bb0f3e22`, whose Rust/Cargo bytes are
unchanged through this candidate. The build uses the pinned Rust image
`rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922`,
`Cargo.lock`, release mode, path remapping and stripped symbols.

The package carries the accepted fixed installer from `7f2e876` and its exact
five public payloads: three systemd units, one sysusers file and one tmpfiles
file. Their current bytes are required to equal the accepted source bytes.
The generated package manifest binds every source and destination path, mode,
owner/group and SHA-256, plus the rebuilt binary SHA-256 and the target SSH
host-key fingerprint.

The package intentionally excludes operator config, first-boot source and the
lifecycle credential. It also excludes every phase manifest and all O2/O3/O4
material.

## Sequence after a separate execution acceptance

The following sequence is documentary and is not authorized by this commit:

1. rerun and accept the complete O0 read-only preflight immediately before the
   first mutation;
2. verify the handoff, nested bundle, target identity and all artifact hashes;
3. copy the immutable bundle to a new root-only staging directory;
4. run the bundled installer once with `install --root /` and the bundled
   binary;
5. run the same installer with `status --root /` and require
   `EXACT_INSTALLED`;
6. reread all installed hashes, ownership and modes, prove P0 identities and
   Redis DB0 are unchanged, DB15 remains empty, and retain the O1 evidence;
7. do not run `systemctl daemon-reload`, `enable` or `start`.

The accepted rollback is the bundled installer's `rollback --root /`. It is
allowed only before operator material or durable state exists, verifies the
complete installation twice, removes only the public package and binary, and
retains the service identity and persistent directories.

## Closed boundary

This candidate does not execute provisioning. User/path creation, file
installation, daemon reload, unit enable/start, Redis mutation and paper
provider execution remain closed pending independent O1 package acceptance.
O2 bootstrap, O3 synthetic paper, O4 read-only FINAM bars, FINAM POST/DELETE,
broker dispatch, runtime-live and real orders are also closed.
