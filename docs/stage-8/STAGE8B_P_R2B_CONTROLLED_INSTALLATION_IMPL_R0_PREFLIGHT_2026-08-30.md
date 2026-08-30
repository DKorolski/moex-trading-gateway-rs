# Stage 8B-P R2B Controlled Installation — Implementation R0 preflight

Status: preflight contract, independent review required, execution not started.

## Purpose

This package turns the accepted Controlled Installation R0 design at
`1e4db79288b0809fd5975edfdd0fc14740bcc8c6` into an executable, reviewable
plan. It does not install, enable or start a unit and does not materialize a
private key. A subsequent reviewed execution package is required before the
controlled proof may run.

The proof contour is a disposable Linux/amd64 systemd container on the local
developer workstation. The container is not the production account host. It
is created with Docker `--network none`; it receives only read-only source and
accepted-artifact mounts plus a dedicated evidence output. Host root, Docker
socket, `.env`, broker configuration and production runtime directories are
not mounted.

## Immutable predecessors

- accepted design commit:
  `1e4db79288b0809fd5975edfdd0fc14740bcc8c6`;
- accepted design archive: `moex-trading-project-1e4db79.zip`;
- accepted design archive SHA-256:
  `5d55ccd8a585d6da780531aa237c9fba215328bce502b1099a8dc5aa3c22faea`;
- accepted Implementation R0-R1A commit:
  `6672819e357a3c2a2c1e73e5408c393da01913a1`;
- accepted Implementation R0-R1A archive SHA-256:
  `2bfb9653b71d942cdda46f7da6bc53f4f59b01e117e5475ef936f36c66c23d77`.

The inherited controlled-installation authority, supersession record and full
transaction contract are exact-hash inputs. This package does not rewrite
them.

## External ELF root

The execution runner must receive `--artifact-root`. The root must contain all
12 Linux/amd64 executables listed in the staging inventory and every byte must
match the accepted transaction SHA-256. The accepted `6672819` archive carries
the four final Phase 5/6 binaries in
`handoff-evidence/linux-amd64/build-a`; the other accepted binaries must be
materialized from their own accepted immutable evidence. Missing, duplicate,
rebuilt-without-evidence or hash-mismatched binaries abort before container
creation.

This deliberately avoids copying predecessor ELF into every design handoff.

## Canary ceremony

The reviewed ceremony ID is
`stage8b-r2b-canary-offline-20260830-r0`. It is a new canary trust domain and
does not claim continuity with generation 1 of the pre-production authority.
Private material may be generated only after the networkless contour exists,
inside a dedicated tmpfs. It must never enter source, reports, the handoff,
shell arguments or host persistent storage. Only public fingerprints and
redacted lifecycle evidence may leave the contour. Real FINAM token, account
ID and operator identity are forbidden; fixed canary labels are used instead.

## Planned proof transaction

After separate execution approval, one runner must:

1. validate the host/contour inventory and all inherited hashes;
2. create a fresh networkless systemd container and verify absence of a
   default route and DNS reachability;
3. generate the named canary ceremony in tmpfs;
4. install exactly 18 unit/target files and 12 exact accepted ELF files;
5. run the 31-invocation success graph;
6. prove failure blocking for each of the six phases, including partial
   producer and issuer fanout;
7. prove stale ceremony, stale nonce, stale receipt and stale output rejection;
8. remove all transaction state and prove a second clean success run;
9. stop/disable/reset every unit, remove units, ELF, state and canary material,
   run `daemon-reload`, and prove the post-uninstall inventory is empty;
10. destroy the container and emit only redacted evidence.

Any failure enters cleanup. Cleanup failure makes the whole proof fail.

## Closed boundary

- `R2B = NOT_ISSUED`;
- no installation or execution is authorized by this preflight package;
- no production host, real operator or real credential;
- no FINAM AuthService, broker GET, POST or DELETE;
- no broker dispatch, Redis live, runtime-live or real orders.
