# Sparse O2 installation package — non-activating successor

2026-10-04. INSTALLATION_PACKAGE_REVIEW_CANDIDATE. O2 remains HOLD.
No VPS access, installation, signing, phase issuance or execution is part of
preparing this package. Rust/Cargo, deploy units and GitHub workflows are unchanged.

## Accepted inputs

Artifact review `823fd350c05be7b0003fffeb651bcaee4b5c6f62`: ARTIFACT / GOVERNANCE
ACCEPT (`REVIEW_823fd35_SPARSE_O2_ARTIFACT_RU.txt`, retained byte-for-byte).
Archive SHA-256:
`accb724cbe2b99d11847546e37b482a51a36483744b23fc58ebc0fecb94b0a38`.
Compiled commit: `4668b424a58a0bb3e0083380ceb33c0d8312b76d`.
No rebuild; all three exact accepted ELF bytes are reused.

[CI run 37139561774](https://github.com/DKorolski/moex-trading-gateway-rs/actions/runs/37139561774)
completed successfully for the exact artifact head: `rust` and `redis-smoke`.
PR #12 merged normally at `2bc3e49b2951e9bd977e7e6627df0361f5e57a75`;
its tree equals the reviewed head. This is inherited artifact CI, not a claim
of CI or independent acceptance for the new installation scripts.

Prior full installation ZIP SHA:
`7c5515dec039b6b2ada62baf819c739bf14eb7ed7b487df2749ac5383f3c1710`.
Prior installed manifest SHA:
`5b1ed8cc5553694d836cc0fbf2c5b9855cc46f08da88f7c3ac46e85c7aac1ed2`.
Retained bounded failure evidence ZIP SHA:
`0fa9ad27adc850c929ae709b9fc6f543cb6d36aa4315523d2417a745d59bb8f8`.
This evidence records **FAILED / generation 1 / sequence 6**; latest event
`e68b805c360a4ee463767cd50815e1b59f084e5d30ca6cffa0a378c4b7a0f7de`.
It is not a fresh host observation or O2 success.

To avoid recursive 196-MiB archives, preparation verifies the full originals,
then retains a compact predecessor extract (all old installed bytes bound by
the pinned old manifest) and three exact hash-pinned public observations.
The old archive hashes identify lineage; standalone validation of compact
extracts uses the independent manifest/observation pins, not an untrusted
extract's self-reported origin. Original artifact ZIP stays intact in the package.
No private key, account credential, token or Redis dump is included.

## Exact eight-slot replacement

| Slot | Replacement |
| --- | --- |
| materializer | `17a94958ab90b4f0deedd2e49b88adfa9d4af7547eb728c5f8f596bc2584309a` |
| operator | `67311f1c50290d2a99a4af9bdbfa6016a5ab081f907ce4eb6e02e7ce220e5957` |
| supervisor | `5940f130f265e5bdfe58be9f19c013b27c27d0518df63843a16fe7dc218b1f9c` |
| materialization-policy.json | V3; explicit sparse policy and operational identity |
| source-template.json | V3 calendar-only scaffold; production emits V4 |
| supervisor.template.json | bootstrap schema 2; no-riskgate runtime profile V2 |
| installation-v1.json | compatibility inventory hashes |
| installation-o2-v1.json | full bound inventory, written last |

The genesis installation ID stays `stage8b-p1f-o2-baseline07-7196aaa-v1`.
New payload identity is the new installation manifest SHA/revision. Operational
identity changes from strict `9b357261…962c0` to sparse
`fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3` through
the accepted bootstrap policy binding, not through a new genesis or history reset.
Policy SHA is `8f3cac0b35529301ef2ea056a2e1ab00db0aa5e955786b8141cf1ef701d1e61d`.
This hash identifies the accepted V4 source contract constant, not a newly
invented JSON source-plan file. The exact defining Rust source is retained.

No automatic durable-root migration. Existing state, old transactions,
authority history/events 0–6, receipts 2/4/6, keys, selectors, staging,
systemd units and file/directory custody must remain identical.

## Candidate calendar — 5 October, not authorization

Four explicit weekday sessions: 29/30 September, 1/2 October; candidate session
5 October 2026. Declared closes 07:10–23:50 MSK. No synthetic weekend intervals
or M1 fill bars. History selection retains the accepted explicit weekday scope;
it does not claim the venue has no weekend trading.

Fetch upper bounds: `2026-09-29T04:00:00Z`–`2026-10-05T20:50:00Z` (under seven days).
Potential execution window: `2026-10-05T04:10:00Z`–`2026-10-05T20:50:00Z`.
Production selects only a completed fresh candidate and preceding prefix;
candidate age <=900 seconds, broker truth <=300 seconds. A full-session upper
bound does not authorize future bars or an empty/partial/untrusted response.

Calendar basis rechecked 2026-10-04: [July hours](https://www.moex.com/n101220),
[continuous session](https://www.moex.com/n98363),
[September hours](https://www.moex.com/n103379). Applying these general hours
to the candidate weekday is a planning inference, not evidence of FINAM feed
availability or absence of exceptional halts. Actual schedule/data checks remain
part of the separately authorized attempt. If the window is missed, rebind the
calendar/templates/installation manifest explicitly; never auto-roll or reuse
a signed expired phase. Changing the calendar alone does not change the stable
operational namespace hash; the installation/template hashes bind its exact date.

## Update, resume, rollback

`scripts/stage8b_p1f_o2_sparse_install.py` is a fixed successor using the existing
custody helpers and fsync/backup/journal protocol, not a generic installer or
new recovery framework. Previous updater remains unchanged.
Transaction directory: `/usr/local/share/moex/stage8b-p1e/o2-sparse-4668b42`.
Existing execution/guardian lock inodes are acquired before checks/writes.
Preflight reads host identity, stopped P1 units, exact P0 observation and the
retained authority/config/durable/staging inventories. It refuses drift, a new
claim, modified receipt, extra input, running process or foreign installed bytes.
No call to start/reload/reset-failed, no Redis/FINAM connection.

Apply durably backs up eight old slots, then writes the exact replacements;
resume accepts only old/new bytes and the exact journal; rollback restores only
those eight bytes. Invariants are checked before and between writes. A newer
authority history blocks both update and rollback. Unknown/incomplete backup
preparation stops for inspection, not an automatic history repair.

The retained P0 observation includes process-property hashes. If the live P0
has legitimately restarted since capture, preflight will reject: collect and
review a narrow fresh observation separately, never silently substitute it or
restart P0 to fit the package. No claim of unchanged live P0 is made offline.

## Local gate and reviewer entry

Run the included installation gate against a clean commit and the accepted build:

```text
python3 scripts/stage8b_p1f_o2_sparse_install_review.py gate OUTPUT --prepared PREPARED --build ACCEPTED_BUILD
python3 scripts/stage8b_p1f_o2_sparse_install_review.py package ZIP --prepared PREPARED --gate OUTPUT
python3 scripts/stage8b_p1f_o2_sparse_install_review.py check ZIP
```

The attached immutable gate binds actual logs to that commit and input inventory:
syntax/fmt; current-tree authority and 45 negatives; 18 prepared tamper cases;
14 Linux filesystem fixture tests including 16 post-fsync replacement frontiers;
accepted-release-library calendar/identity checks; exact ELF config/policy smoke;
unchanged production/deploy/workflows. Only control inventory refresh is needed
for the current-status update; production fingerprint/authority rules do not change.

Filesystem tests use disposable native ARM Linux with mocked systemd observations.
Injected exceptions test reopening after each durable slot, not actual SIGKILL or
power loss. ELF/probe smoke uses linux/amd64 emulation, network none. No fresh
full Rust-suite run is claimed for this packaging-only change. The retained
artifact proves source V4 generation on fixtures; this installation probe checks
fresh calendar admission, policy/identity agreement and date/stale rejection,
not fresh FINAM materialization or paper-trading success.

Next: independent package review → separately permitted stopped installation →
review of its evidence → separately permitted single bounded O2 attempt.
O3/O4 still cover continuous sparse feed, WS subscriptions/reconnect, freshness,
REST/WS overlap and EOD timeliness, then paper sessions versus ALOR.
FINAM POST/DELETE, broker execution and live micro remain closed.
