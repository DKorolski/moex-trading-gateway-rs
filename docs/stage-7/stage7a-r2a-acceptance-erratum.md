# Stage 7A-R2a acceptance erratum

This erratum clarifies frozen acceptance row A-019 without weakening its
identity or exactly-once intent.

Authority marker: `stage5g_compatible_duplicate_after_canonical_publication`.

After the canonical terminal ACK has been published, exact Redis redelivery
must preserve the request, client-order and broker-order identities and emit
the accepted Stage 5G-compatible `Duplicate / DuplicateCommand` outcome. It
must not publish a second terminal `Accepted` outcome.

Before any canonical ACK has been published, a same-authority replay may emit
the canonical ACK only when process-local recovery evidence proves the exact
normalized paper outcome and command digest. Without that evidence, handling
fails closed as reconciliation-required.

This wording supersedes only the ambiguous “equivalent ACK” phrase in A-019.
The frozen 52-row matrix, paper-only boundary and live-surface prohibitions are
otherwise unchanged.
