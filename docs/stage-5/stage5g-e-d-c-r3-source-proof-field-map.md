# Stage 5G-e-d-c R3 - exact source-proof field-map closure

Stage 5G-e-d-c R3 is a narrow provenance-seal patch on top of
`95901eb9bf19e103e9acb82fb9726708f356b4cd`.

R3 keeps the accepted R2 application authority design unchanged and adds a
machine-checked field-to-source contract for
`Stage5gFreshTruthApplicationSourceProof::from_application_parts`.

The reviewed source-map authority is:

```text
docs/stage-5/stage5g-e-d-c-r3-source-proof-field-map.json
```

The descriptor is acceptance authority. The R3 checker verifies that every
source-proof field is assigned from the exact approved reduction, restart,
fresh-truth or owned-candidate source expression. The checker also rejects
constructor drift, struct spread/default syntax and any production route that
attempts to build a source proof from application evidence.

R3 also adds independent source-oracle tests. Those tests capture expected
provenance values directly from the restart authority, validated fresh truth,
reduction metadata and owned candidate before source-proof construction. They
then apply the reduction and compare the final application evidence against
that independently captured oracle.

Parent binding defense-in-depth is limited to fields already present in the
canonical package. The package exposes an independent predecessor revision via
`package_instance.previous_revision`; R3 requires it to match
`application_evidence.parent_snapshot_revision`. The package format does not
carry a second independent predecessor snapshot ID, so parent ID authenticity
continues to be supplied by the source-proof commitment, final application
authority and HMAC over the authenticated package.

R3 does not add external durable storage, fsync, CAS, anti-rollback, Redis,
FINAM, HTTP POST/DELETE, broker dispatch, strategy callbacks, runtime-live,
real orders, Stage 5G-f or Stage 6. Policy B `ExactReplay` remains disabled.
