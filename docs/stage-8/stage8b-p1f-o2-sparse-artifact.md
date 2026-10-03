# Sparse O2 binary artifact — bounded review candidate

Date: 2026-10-03. BINARY REVIEW CANDIDATE, NOT INSTALLABLE.
Source correction `d63c378` is accepted; artifact acceptance and GitHub CI
are separate. This package introduces no operational permission.

## Source and build

Compiled ref: `4668b424a58a0bb3e0083380ceb33c0d8312b76d`.
Tree: `d2ee7e951c32ad8db1317b46d1a4cff787369506`.
This is the authority/docs-only successor to accepted `d63c378`; all Rust,
Cargo and workflow bytes are identical. Source authority passes with 172 files,
fingerprint `202527e77ad76534ae027e99397877bfbf95faa98ac65c807bd408e5a2aafe94`;
45/45 local authority negatives pass. The packaging commit adds only these
artifact tools and docs, not production changes. Both Git trees and raw commits
are reconstructed by the independent archive check; no Git checkout is needed.

The inherited pinned Rust image and default release feature recipe are
unchanged (Rust 1.98.0 in the Linux image versus 1.95.0 for canonical CI).
Clean offline Linux/amd64 build completed in 3m04s, network none. Three inherited
default-feature publisher dead-code warnings remain; they are not suppressed.
The release libraries and three binaries come from that single build.

| ELF | SHA-256 |
|---|---|
| materializer | `17a94958ab90b4f0deedd2e49b88adfa9d4af7547eb728c5f8f596bc2584309a` |
| operator | `67311f1c50290d2a99a4af9bdbfa6016a5ab081f907ce4eb6e02e7ce220e5957` |
| supervisor | `5940f130f265e5bdfe58be9f19c013b27c27d0518df63843a16fe7dc218b1f9c` |

## Executed qualification, not deployment

An offline executable links exact default-feature release rlibs, builds 404 real
History M10 (18 sparse) plus one synthetic seven-M1 candidate, creates source V4
and admits it through the production V4 parser. It verifies zero riskgate
observations, no-riskgate fingerprint, and rejection of the previous strict
operational identity. The probe does not invoke a Hybrid callback, service or
FINAM network. No fixture is a current operational calendar or authorization.

The actual supervisor ELF validates bootstrap schema 2 and rejects four mixed
configs. The materializer ELF validates explicit policy V3 and reaches the
account check, where a deliberately wrong synthetic credential stops it; a
foreign policy digest rejects before that boundary. Its successful network
collection is NOT claimed. Legacy config/custody probes execute unchanged.
The operator ELF is covered by the inherited no-selector/custody smoke; the
full O2 runner/guardian chain remains the separately authorized operational run.
All containers use network none, disposable roots and only public build/fixture
mounts. No Redis, FINAM token, real account credential, VPS or Docker socket is
mounted. The final durable/control roots remain empty. This is Linux/amd64
emulation on a Mac, not native VPS/systemd acceptance.

Artifact fixtures bind policy V3, bootstrap schema 2 and source V4 to policy hash
`8f3cac0b35529301ef2ea056a2e1ab00db0aa5e955786b8141cf1ef701d1e61d`.
The V3 calendar *template* is not relabelled as a V3 sparse source: the release
materializer emits V4. Legacy examples and installed files remain unchanged.
Qualification outputs have fixed hashes; regenerating generated-file hashes
alone cannot relabel these fixtures as a different profile.

## Reproduce and review

```sh
python3 scripts/stage8b_p1f_o2_sparse_build.py NEW_BUILD --registry-cache PUBLIC_CARGO_REGISTRY
python3 scripts/stage8b_p1f_o2_sparse_artifact.py qualify NEW_PROOF --build NEW_BUILD
python3 scripts/stage8b_p1f_o2_sparse_artifact.py package NEW.zip --build NEW_BUILD --proof NEW_PROOF
python3 scripts/stage8b_p1f_o2_sparse_artifact.py check NEW.zip
python3 scripts/test_stage8b_p1f_o2_sparse_artifact.py NEW.zip
python3 scripts/stage8b_p1f_o2_sparse_gate.py NEW.zip NEW_GATE_DIR
```

`package` requires a clean committed tree, refuses overwrite and exports Git
blobs, never incidental working files. Full source, hidden workflow files,
fixtures, lineage, ELFs, exact build and qualification logs are in the ZIP.
The postseal `.local-gate.zip` binds the ZIP hash and records fresh authority,
45 negatives, fmt, workspace Clippy and archive mutations. These are local
checks, not a claimed remote `rust`/`redis-smoke` result or another Rust full
suite run. The accepted full source gate is inherited from the source review.

O2 stays HOLD. Next, after independent artifact acceptance and ordinary CI,
prepare the separately reviewed installation with real fresh bound inputs.
Preserve FAILED/1/6 and prior terminal history. Never reset or auto-migrate the
old root. Installation and a single bounded O2 require separate authorization.
Continuous sparse delivery and WS/EOD remain O3/O4 checklist work, not acceptance
inferred from this one-shot artifact.
