# Stage 8B-P R2B Generation-2 Composition Rebuild R0-R1

## Exact Phase-6 Evidence Closure

Status: implementation review candidate. Independent acceptance is required.

Accepted substantive predecessor:

- review commit: `1a1933f90075591a88d4631c7c72599a1262115d`;
- archive SHA-256: `df438c441e7646192c0dcc9160644e74a018d7095256302aec32748333e3cd04`;
- production build source: `c7667658288577229b7cf00e9dcef519ba2fd1d7`.

R0-R1 changes only Phase-6 rehearsal, typed evidence parsing, evidence
aggregation and review gates. It does not change or rebuild any of the eight
accepted Linux/amd64 ELF artifacts, the Generation-2 trust/account manifests,
the source-adapter authority, helper acceptance or its effect identity.

## Exact request-boundary oracle

The category-only `NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE` grep is
forbidden. The rehearsal must parse the complete root terminal envelope and
its `validated_helper_terminal`.

The sole accepted first request proof is:

```text
root admission                         SUCCEEDED
helper identity/receipt/authority      VALIDATED
projected credentials                  LOADED
typed child terminal protocol          VALID
root terminal evidence                 DURABLE root:root 0400 nlink=1
request attempt ordinal                1
request method                         POST
request route template                 /v1/sessions
request error                          NETWORK_CONNECT_FAILURE | TIMEOUT
HTTP status                            ABSENT
response body                          ABSENT
root lifecycle timeout                 FALSE
effect/dispatch/order flags             FALSE
```

`TIMEOUT` is accepted only when it belongs to the failed request attempt and
has a non-empty request timeout stage. A root supervisor lifecycle timeout,
`AUTH_SESSION_FAILURE` without an attempt, local client-construction failure,
an HTTP response or any opened effect flag fails closed.

`actual_read_attempts` is derived as `bool(request_attempts)` after exact typed
validation. It may not be a literal success claim.

## Preserved production composition

- Build evidence SHA-256 remains
  `202a02e646f14f096741078250b6bed0836eb63161af19bdd640059f32747507`.
- Helper ELF SHA-256 remains
  `90508e097c8668d6fe90a15ef6014e480a9042bb36f0613351c02465d10aaca1`.
- Helper effect identity remains exactly
  `ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0`.
- Build A and build B hashes remain identical for all eight artifacts.

## Closed boundary

Generation 2 remains inactive. Production credentials are not installed,
controlled installation is not performed and package authorization remains
`NOT_ISSUED`. The rehearsal uses `--network none`; FINAM/AuthService/broker
requests, POST/DELETE, dispatch, Redis live, runtime-live and real orders stay
closed.

The next allowed step is independent R0-R1 review. Full 31-service
Generation-2 transaction-contract rebinding and native Linux/amd64 rehearsal
belong to a later separately reviewed controlled-installation package.
