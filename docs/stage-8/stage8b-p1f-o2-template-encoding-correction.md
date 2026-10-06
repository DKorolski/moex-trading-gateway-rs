# O2 — packaged supervisor template encoding correction

Date: 2026-10-06. Status: local correction / review candidate; **O2 HOLD**.
This is a packaging correction, not a new runtime feature or installation.

## Retained failure and cause

The October-6 one-shot reached the FINAM materializer successfully (exit 0),
then `guardian-materialize-fixed` rejected its inputs (exit 70, invalid authority
document). Cleanup preserved the previous history and reached **FAILED/1/10**.
P1 stopped; P0 and installed payload were unchanged. These are retained
observations, not a new operational acceptance.

The exact installed `supervisor.template.json` has 1752 bytes, including a final
LF. Guardian's `parse_canonical<serde_json::Value>` requires the 1751-byte compact
serialization **without LF**. JSON deserialization alone missed this boundary.
The original bytes fail the canonical check; removing only LF fixes that check.

- Installed package commit: `836a2973495b505fac56422308da4a65e921d930`.
- Accepted compiled source: `f3b349949802abd5eff80cad6b9e9fc37bc327e1`.
- Original template SHA-256: `04aed2dc5cdf5bdc44d48cf28c91986bd71e266f2965f76200caf796d87e191e`.
- Corrected template SHA-256: `2d23f48611a02345dd5c2a6dc7d9a25a8b4c6d5a0cd4db40fabcafde6d8dc2b5`.
- Retained result package: `finam-o2-v4-bounded-result-20261006.zip`, SHA-256
  `739abc9778eaa2bc55be58c02991ecb9345d58d58595bc5845ea97e8067ac030`.

## Narrow correction

`stage8b_p1f_o2_v4_timestamp_install_package.py` now uses a slot-specific encoder
for the supervisor template. Global newline-terminated policy/inventory/history
encoding is unchanged. The actual generated fixture differs in exactly two
payload slots: supervisor template and its binding `installation-o2-v1.json`.
`installation-v1.json`, policy, source template and all three ELF files stay
byte-identical. Hashes and sizes are regenerated from emitted bytes.
The current-tree authority refresh is limited to the two updated status/roadmap
document entries and their control-inventory aggregate. Production inventory,
closed-surface flags, workflows and historical acceptance remain unchanged.

The existing installation input probe now compares **packaged file bytes** with
`serde_json::to_vec`, instead of accepting any parseable JSON. The old LF fixture
is an explicit negative control for that same probe.

## Completed local evidence

- Seven packaging regressions against the pinned accepted installation ZIP.
- Twenty existing prepared-package negatives.
- Corrected existing installation probe PASS; original LF version rejected.
- New probe compiled with `-D warnings` against exact hash-pinned accepted release
  rlibs; no production Rust/Cargo changes or production ELF rebuild.
- Exact retained October-6 staged source passes the real V4 admission method;
  the stale-time negative still rejects.
- Real guardian `materialize_o2` reaches `ReadyForBootstrap`, persists source and
  config, rereads their exact hashes, checks mode 0440, and returns the same
  receipt on replay without another history event. LF/CRLF/pretty inputs reject
  without materialization writes. Bootstrap itself is not invoked.

The integration runs in a disposable Linux/amd64 Docker container, network none,
with an explicit historical clock and **fixture-only authority**. The signing
seed is public test data, not the production key. No SSH, Docker socket, ceremony
directory or host production state is mounted. Fixture history is fresh, not a
rollback or continuation of VPS history. The VPS is not modified by this gate.

Reproduction entry point:

```text
python3 -B scripts/stage8b_p1f_o2_template_encoding_gate.py \
  --build <accepted-1062691-linux-build-directory> \
  --retained <confidential-retained-staged.json> \
  --output <new-output-directory>
```

The default installation archive is the exact accepted `836a297` ZIP (SHA-256
`5dd40622580346c74e1172a1611a42d4cde8aa396b904db345771429d1edd897`).
The gate rejects alternate retained input or release rlibs. The raw retained
staged source contains private account data and is **excluded from handoff**;
only hashes and safe test logs are delivered. Consequently, full raw-source
replay needs separately controlled access to that input and the accepted build;
the ZIP alone is not claimed to reproduce that private-input integration.
The seven packaging tests additionally need the earlier accepted installation ZIP.

The generated test fixture deliberately retains the historical FAILED/1/8
installation descriptor and October-6 calendar. It is **NOT an operational
successor**, is not included as an installable payload, and must not be applied.

## Next boundary

Review this correction and offline evidence, then prepare the narrow successor
package against the actual **FAILED/1/10** history and a new allowed calendar.
Reuse accepted ELF bytes; no new recovery framework or strategy change is needed.
Only after package acceptance and separate permission: stopped installation with
terminal history preserved, then separately authorized bounded O2. No hot edit,
new phase, timer, DB0 activation, FINAM POST/DELETE or live order is authorized here.
No push/merge is performed in this slice. Existing WS/operational parity work
stays separate; O3/O4 checks remain ahead after O2 succeeds.
