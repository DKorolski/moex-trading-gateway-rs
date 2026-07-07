# Stage 2B patch log

Status: patch-log scaffold; no implementation in this document.

Stage 2B implementation plan is accepted in
[`../stage-2b-runtime-source-migration-implementation-plan.md`](../stage-2b-runtime-source-migration-implementation-plan.md).

All Stage 2B implementation patches must remain paper/mock/local and must keep
these boundaries closed:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter.

## Patch acceptance-note rule

Each Stage 2B implementation patch should add or update a short acceptance note
under this directory before handoff. The note should include:

- what changed;
- what did not change;
- tests added or preserved;
- unsupported live blockers that remain closed;
- evidence that no real FINAM send path was enabled.

## Next planned patch

`2B-1` should be the foundation patch:

- broker-neutral runtime-facing id aliases/types;
- legacy numeric ALOR id -> decimal-string import helpers;
- string broker id preservation tests;
- no strategy behavior changes;
- no real FINAM endpoint calls.

Draft note path:

```text
docs/stage-2b/2b-1-id-types-acceptance.md
```
