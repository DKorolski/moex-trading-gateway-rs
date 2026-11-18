//! Local SQLite atomic boundary, not an external publisher or operational DB.
use super::{
    arms::Arm,
    contract::{cutoff, Candle, Manifest, Purpose},
    hash, VERSION,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn bytes_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_secs(2))?;
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;",
    )?;
    ensure!(
        conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? == "ok",
        "SQLite quick_check"
    );
    Ok(conn)
}
fn initial(manifest: &Manifest) -> Result<BTreeMap<String, Arm>> {
    let mut arms = BTreeMap::new();
    for seed in &manifest.seeds {
        for second in [false, true] {
            let arm = Arm::bootstrap(manifest, seed, second)?;
            ensure!(arms.insert(arm.id.clone(), arm).is_none(), "duplicate arm");
        }
    }
    ensure!(arms.len() == 6, "six branches required");
    Ok(arms)
}
fn genesis(manifest: &Manifest, arms: &BTreeMap<String, Arm>) -> Result<String> {
    let state: Vec<_> = arms
        .values()
        .map(|a| {
            Ok((
                a.id.clone(),
                bytes_hash(&a.checkpoint()?),
                hash(&a.state(manifest.session_date)?)?,
            ))
        })
        .collect::<Result<_>>()?;
    hash(&(VERSION, "genesis", manifest.sha256()?, state))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub id: String,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub schema: String,
    pub manifest_sha256: String,
    pub result_id: String,
    pub arm: String,
    pub source_sequence: u64,
    pub input_sha256: String,
    pub checkpoint_sha256: String,
    pub state_sha256: String,
    pub state: Value,
    pub marks: Vec<Mark>,
    pub evidence: String,
    pub order_emission_allowed: bool,
    pub observed_clean_session: bool,
}
fn stage(
    arms: &mut BTreeMap<String, Arm>,
    manifest: &Manifest,
    c: &Candle,
    manifest_sha: &str,
) -> Result<Vec<Outcome>> {
    let seed = manifest
        .seeds
        .iter()
        .find(|s| s.binding.instrument == c.instrument)
        .context("unmapped input")?;
    c.validate(&seed.binding)?;
    let slot = c
        .source_sequence
        .checked_sub(seed.history.len() as u64 + 1)
        .context("backdated source sequence")?;
    ensure!(
        slot < 101
            && c.purpose == Purpose::Model
            && c.start_ms == cutoff(manifest.session_date) + slot as i64 * 600_000,
        "missing/duplicate/out-of-session M10"
    );
    let mut outcomes = Vec::new();
    for arm in arms.values_mut().filter(|a| a.instrument == c.instrument) {
        let marks = arm.step(c)?;
        let input_sha = c.sha256()?;
        let id = hash(&(
            VERSION,
            &manifest_sha,
            &arm.id,
            c.source_sequence,
            &input_sha,
        ))?;
        let marks = marks
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                Ok(Mark {
                    id: hash(&(&id, i))?,
                    value: v,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let state = arm.state(manifest.session_date)?;
        outcomes.push(Outcome {
            schema: VERSION.into(),
            manifest_sha256: manifest_sha.into(),
            result_id: id,
            arm: arm.id.clone(),
            source_sequence: c.source_sequence,
            input_sha256: input_sha,
            checkpoint_sha256: bytes_hash(&arm.checkpoint()?),
            state_sha256: hash(&state)?,
            state,
            marks,
            evidence: "declared_clock_replay".into(),
            order_emission_allowed: false,
            observed_clean_session: false,
        });
    }
    ensure!(
        outcomes.len() == 2,
        "instrument must have exactly two policy arms"
    );
    Ok(outcomes)
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Commit {
    Committed {
        revision: u64,
        result_ids: Vec<String>,
    },
    /// A durable lookup, NOT re-emission of previously committed marks.
    AlreadyCommitted {
        revision: u64,
        result_ids: Vec<String>,
    },
}
/// Historical readback, not an admission, delivery queue or callback.
#[derive(Clone, Debug)]
pub struct CommittedInput {
    pub candle: Candle,
    pub revision: u64,
    pub result_ids: Vec<String>,
}
pub struct Journal {
    conn: Connection,
    manifest: Manifest,
    arms: BTreeMap<String, Arm>,
    revision: u64,
    chain: String,
    poisoned: bool,
}
impl Journal {
    /// One coherent, checked input/outcome read. No numerical step or marks
    /// are returned. Used to settle response-loss only for exact redelivery.
    pub fn committed_input(
        &self,
        instrument: &str,
        sequence: u64,
    ) -> Result<Option<CommittedInput>> {
        ensure!(!self.poisoned, "ambiguous commit: reopen required");
        let tx = self.conn.unchecked_transaction()?;
        let current: (u64, String) =
            tx.query_row("SELECT revision,chain FROM meta WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        ensure!(
            current == (self.revision, self.chain.clone()),
            "stale readback: reopen required"
        );
        let row: Option<(u64, String, String, String, String, i64)> = tx
            .query_row(
                "SELECT revision,input,input_sha,previous_chain,chain,write_prepared_at_ms
             FROM inputs WHERE instrument=?1 AND source_sequence=?2",
                params![instrument, sequence],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((revision, text, sha, previous, chain, prepared)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        let candle: Candle = serde_json::from_str(&text)?;
        ensure!(
            candle.instrument == instrument
                && candle.source_sequence == sequence
                && candle.sha256()? == sha,
            "readback input index/hash"
        );
        let outcomes = read_outcomes(&tx, revision)?;
        let manifest_sha = self.manifest.sha256()?;
        ensure!(
            outcomes.len() == 2
                && outcomes.iter().all(|o| o.input_sha256 == sha
                    && o.source_sequence == sequence
                    && o.manifest_sha256 == manifest_sha),
            "readback outcome linkage"
        );
        ensure!(
            hash(&(previous, revision, &sha, &outcomes, prepared))? == chain,
            "readback chain"
        );
        for o in &outcomes {
            let expected_arm = self.arms.get(&o.arm).context("readback unknown arm")?;
            let (symbol,seq,cp,cp_sha,state,state_sha): (String,u64,Vec<u8>,String,String,String) = tx.query_row(
                "SELECT instrument,source_sequence,checkpoint,checkpoint_sha,state,state_sha FROM arms WHERE id=?1",
                [&o.arm], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
            ensure!(
                symbol == instrument
                    && symbol == expected_arm.instrument
                    && seq >= sequence
                    && bytes_hash(&cp) == cp_sha
                    && bytes_hash(state.as_bytes()) == state_sha,
                "readback checkpoint identity/checksum"
            );
            // A historical requested input may precede the current checkpoint.
            // Bind that checkpoint to the ACTUAL latest committed input/outcome,
            // not merely a larger SQL sequence supplied by an untrusted row.
            let latest_revision: u64 = tx.query_row(
                "SELECT MAX(revision) FROM inputs WHERE instrument=?1",
                [instrument],
                |r| r.get(0),
            )?;
            let (text, latest_sha, previous, head, prepared): (
                String,
                String,
                String,
                String,
                i64,
            ) = tx.query_row(
                "SELECT input,input_sha,previous_chain,chain,write_prepared_at_ms
                    FROM inputs WHERE revision=?1 AND instrument=?2 AND source_sequence=?3",
                params![latest_revision, instrument, seq],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let latest_input: Candle = serde_json::from_str(&text)?;
            ensure!(
                latest_input.instrument == symbol
                    && latest_input.source_sequence == seq
                    && latest_input.sha256()? == latest_sha,
                "readback latest input identity/hash"
            );
            let latest_pair = read_outcomes(&tx, latest_revision)?;
            ensure!(
                latest_pair.len() == 2
                    && latest_pair.iter().all(|p| p.input_sha256 == latest_sha
                        && p.source_sequence == seq
                        && p.manifest_sha256 == manifest_sha),
                "readback latest outcome links"
            );
            ensure!(
                hash(&(
                    previous,
                    latest_revision,
                    &latest_sha,
                    &latest_pair,
                    prepared
                ))? == head,
                "readback latest chain"
            );
            let latest = latest_pair
                .iter()
                .find(|p| p.arm == o.arm)
                .context("readback latest arm outcome absent")?;
            let last_arm_revision: u64 = tx.query_row(
                "SELECT MAX(revision) FROM outcomes WHERE arm=?1",
                [&o.arm],
                |r| r.get(0),
            )?;
            ensure!(
                last_arm_revision == latest_revision
                    && latest_revision >= revision
                    && cp_sha == latest.checkpoint_sha256
                    && state_sha == latest.state_sha256
                    && state == serde_json::to_string(&latest.state)?,
                "readback latest checkpoint/outcome frontier"
            );
        }
        let record = CommittedInput {
            candle,
            revision,
            result_ids: outcomes.into_iter().map(|o| o.result_id).collect(),
        };
        tx.commit()?;
        Ok(Some(record))
    }
    /// A new file only. A failed create is not silently erased or reused.
    pub fn create(path: &Path, manifest: Manifest) -> Result<Self> {
        manifest.validate()?;
        let arms = initial(&manifest)?;
        let chain = genesis(&manifest, &arms)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)?.sync_all()?;
        let mut conn = connect(path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("
          CREATE TABLE meta(id INTEGER PRIMARY KEY CHECK(id=1), manifest TEXT NOT NULL,
            manifest_sha TEXT NOT NULL, revision INTEGER NOT NULL, chain TEXT NOT NULL);
          CREATE TABLE arms(id TEXT PRIMARY KEY, instrument TEXT NOT NULL, source_sequence INTEGER NOT NULL,
            checkpoint BLOB NOT NULL, checkpoint_sha TEXT NOT NULL, state TEXT NOT NULL, state_sha TEXT NOT NULL);
          CREATE TABLE inputs(revision INTEGER PRIMARY KEY, instrument TEXT NOT NULL, source_sequence INTEGER NOT NULL,
            input TEXT NOT NULL, input_sha TEXT NOT NULL, previous_chain TEXT NOT NULL, chain TEXT NOT NULL,
            write_prepared_at_ms INTEGER NOT NULL, UNIQUE(instrument,source_sequence));
          CREATE TABLE outcomes(arm TEXT NOT NULL REFERENCES arms(id), source_sequence INTEGER NOT NULL,
            revision INTEGER NOT NULL REFERENCES inputs(revision), result TEXT NOT NULL, result_sha TEXT NOT NULL,
            PRIMARY KEY(arm,source_sequence));
        ")?;
        tx.execute(
            "INSERT INTO meta VALUES(1,?1,?2,0,?3)",
            params![serde_json::to_string(&manifest)?, manifest.sha256()?, chain],
        )?;
        for a in arms.values() {
            let state = a.state(manifest.session_date)?;
            let checkpoint = a.checkpoint()?;
            let sequence = manifest
                .seeds
                .iter()
                .find(|s| s.binding.instrument == a.instrument)
                .unwrap()
                .history
                .len();
            tx.execute(
                "INSERT INTO arms VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    a.id,
                    a.instrument,
                    sequence as u64,
                    checkpoint,
                    bytes_hash(&checkpoint),
                    serde_json::to_string(&state)?,
                    hash(&state)?
                ],
            )?;
        }
        tx.commit()?;
        // Persist the new directory entry as well as SQLite contents.
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(Self {
            conn,
            manifest,
            arms,
            revision: 0,
            chain,
            poisoned: false,
        })
    }
    /// Reconstruct/compare the bounded causal log, then independently restore
    /// the saved numerical checkpoints. Reopen never returns marks to a caller.
    pub fn open(path: &Path, manifest: Manifest) -> Result<Self> {
        manifest.validate()?;
        let conn = connect(path)?;
        Self::open_connection(conn, manifest)
    }
    /// File-only inspection: SQLite enforces read-only access, including when
    /// validation fails. Uses the SAME replay/checkpoint/readback verifier.
    pub fn inspect(path: &Path, manifest: Manifest) -> Result<Value> {
        manifest.validate()?;
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(2))?;
        ensure!(
            conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? == "ok",
            "SQLite quick_check"
        );
        Self::open_connection(conn, manifest)?.export()
    }
    fn open_connection(conn: Connection, manifest: Manifest) -> Result<Self> {
        // A consistent bounded read snapshot; errors drop/rollback this
        // newly opened connection. A later writer is detected by apply().
        conn.execute_batch("BEGIN DEFERRED")?;
        let (text, sha, revision, head): (String, String, u64, String) = conn.query_row(
            "SELECT manifest,manifest_sha,revision,chain FROM meta WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        ensure!(
            text == serde_json::to_string(&manifest)?
                && sha == manifest.sha256()?
                && revision <= 303,
            "database generation/manifest mismatch"
        );
        let mut arms = initial(&manifest)?;
        let mut chain = genesis(&manifest, &arms)?;
        let mut seen = 0;
        let mut last_seq: BTreeMap<String, u64> = manifest
            .seeds
            .iter()
            .map(|s| (s.binding.instrument.clone(), s.history.len() as u64))
            .collect();
        {
            let mut stmt = conn.prepare("SELECT revision,instrument,source_sequence,input,input_sha,previous_chain,chain,write_prepared_at_ms FROM inputs ORDER BY revision")?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                seen += 1;
                ensure!(
                    seen <= 303 && r.get::<_, u64>(0)? == seen,
                    "journal revision gap"
                );
                let c: Candle = serde_json::from_str(&r.get::<_, String>(3)?)?;
                ensure!(
                    r.get::<_, String>(1)? == c.instrument
                        && r.get::<_, u64>(2)? == c.source_sequence
                        && c.sha256()? == r.get::<_, String>(4)?
                        && r.get::<_, String>(5)? == chain,
                    "input index/hash/chain mismatch"
                );
                ensure!(
                    c.source_sequence
                        == last_seq
                            .get(&c.instrument)
                            .context("unknown input instrument")?
                            + 1,
                    "input sequence gap"
                );
                let expected = stage(&mut arms, &manifest, &c, &sha)?;
                let actual = read_outcomes(&conn, seen)?;
                // Accepted kernel restore advances checkpoint bookkeeping.
                // Compare ALL semantics, but not uninterrupted checkpoint bytes.
                let mut semantic_actual = actual.clone();
                for (a, e) in semantic_actual.iter_mut().zip(&expected) {
                    a.checkpoint_sha256.clone_from(&e.checkpoint_sha256);
                }
                ensure!(
                    semantic_actual == expected,
                    "stored results differ from causal replay"
                );
                chain = hash(&(chain, seen, c.sha256()?, &actual, r.get::<_, i64>(7)?))?;
                ensure!(chain == r.get::<_, String>(6)?, "journal chain mismatch");
                last_seq.insert(c.instrument.clone(), c.source_sequence);
            }
        }
        ensure!(seen == revision && chain == head, "head/log mismatch");
        ensure!(
            conn.query_row("SELECT COUNT(*) FROM outcomes", [], |r| r.get::<_, u64>(0))?
                == seen * 2,
            "missing/orphan output"
        );
        ensure!(
            conn.query_row("SELECT COUNT(*) FROM arms", [], |r| r.get::<_, u64>(0))? == 6,
            "missing/extra checkpoint"
        );
        for a in arms.values_mut() {
            let (symbol,seq,bytes,cp_sha,state_text,state_sha): (String,u64,Vec<u8>,String,String,String) =
                conn.query_row("SELECT instrument,source_sequence,checkpoint,checkpoint_sha,state,state_sha FROM arms WHERE id=?1",
                    [&a.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
            let expected = a.state(manifest.session_date)?;
            ensure!(
                symbol == a.instrument
                    && seq == last_seq[&symbol]
                    && cp_sha == bytes_hash(&bytes)
                    && state_text == serde_json::to_string(&expected)?
                    && state_sha == hash(&expected)?,
                "checkpoint/result frontier mismatch"
            );
            let latest = conn.query_row(
                "SELECT arm,source_sequence,result,result_sha FROM outcomes WHERE arm=?1 ORDER BY source_sequence DESC LIMIT 1",
                [&a.id], outcome_row).optional()?;
            if let Some(row) = latest {
                let outcome = decode_outcome(row)?;
                ensure!(
                    outcome.checkpoint_sha256 == cp_sha && outcome.source_sequence == seq,
                    "checkpoint/latest outcome mismatch"
                );
            } else {
                ensure!(
                    cp_sha == bytes_hash(&a.checkpoint()?),
                    "genesis checkpoint mismatch"
                );
            }
            a.restore(&bytes, &manifest)?;
            ensure!(
                a.state(manifest.session_date)? == expected,
                "checkpoint restore changed state"
            );
        }
        conn.execute_batch("COMMIT")?;
        Ok(Self {
            conn,
            manifest,
            arms,
            revision,
            chain,
            poisoned: false,
        })
    }
    pub fn apply(&mut self, candle: Candle) -> Result<Commit> {
        ensure!(!self.poisoned, "ambiguous commit: reopen required");
        let input_sha = candle.sha256()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (revision, head): (u64, String) =
            tx.query_row("SELECT revision,chain FROM meta WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        ensure!(
            revision == self.revision && head == self.chain,
            "stale writer: reopen required"
        );
        let existing: Option<(u64,String,String)> = tx.query_row(
            "SELECT revision,input_sha,input FROM inputs WHERE instrument=?1 AND source_sequence=?2",
            params![candle.instrument,candle.source_sequence], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((revision, sha, bytes)) = existing {
            ensure!(
                sha == input_sha && bytes == serde_json::to_string(&candle)?,
                "conflicting duplicate"
            );
            return Ok(Commit::AlreadyCommitted {
                revision,
                result_ids: read_outcomes(&tx, revision)?
                    .into_iter()
                    .map(|o| o.result_id)
                    .collect(),
            });
        }
        let seed = self
            .manifest
            .seeds
            .iter()
            .find(|s| s.binding.instrument == candle.instrument)
            .context("unknown instrument")?;
        let last: u64 = tx.query_row(
            "SELECT MAX(source_sequence) FROM arms WHERE instrument=?1",
            [&candle.instrument],
            |r| r.get(0),
        )?;
        ensure!(
            candle.source_sequence == last + 1 && last >= seed.history.len() as u64,
            "source sequence gap"
        );
        let mut staged: BTreeMap<_, _> = self
            .arms
            .iter()
            .filter(|(_, a)| a.instrument == candle.instrument)
            .map(|(id, a)| (id.clone(), a.clone()))
            .collect();
        let outcomes = stage(
            &mut staged,
            &self.manifest,
            &candle,
            &self.manifest.sha256()?,
        )?;
        let next = revision + 1;
        let prepared = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let chain = hash(&(&head, next, &input_sha, &outcomes, prepared))?;
        tx.execute(
            "INSERT INTO inputs VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                next,
                candle.instrument,
                candle.source_sequence,
                serde_json::to_string(&candle)?,
                input_sha,
                head,
                chain,
                prepared
            ],
        )?;
        #[cfg(test)]
        fault("after_input");
        for (i, o) in outcomes.iter().enumerate() {
            tx.execute(
                "INSERT INTO outcomes VALUES(?1,?2,?3,?4,?5)",
                params![
                    o.arm,
                    candle.source_sequence,
                    next,
                    serde_json::to_string(o)?,
                    hash(o)?
                ],
            )?;
            let checkpoint = staged[&o.arm].checkpoint()?;
            tx.execute("UPDATE arms SET source_sequence=?1,checkpoint=?2,checkpoint_sha=?3,state=?4,state_sha=?5 WHERE id=?6",
                params![candle.source_sequence,checkpoint,o.checkpoint_sha256,serde_json::to_string(&o.state)?,o.state_sha256,o.arm])?;
            #[cfg(test)]
            if i == 0 {
                fault("after_first_arm");
            }
            #[cfg(not(test))]
            let _ = i;
        }
        tx.execute(
            "UPDATE meta SET revision=?1,chain=?2 WHERE id=1",
            params![next, chain],
        )?;
        #[cfg(test)]
        fault("before_commit");
        // Never reuse a possibly committed cache after an ambiguous IO result.
        self.poisoned = true;
        tx.commit()?;
        #[cfg(test)]
        fault("after_commit");
        self.arms.extend(staged);
        self.revision = next;
        self.chain = chain;
        self.poisoned = false;
        Ok(Commit::Committed {
            revision: next,
            result_ids: outcomes.into_iter().map(|o| o.result_id).collect(),
        })
    }
    /// Explicit readback/export. These are historical records, not a delivery
    /// queue; neither reopen nor duplicate apply calls emit or execute marks.
    pub fn export(&self) -> Result<Value> {
        ensure!(!self.poisoned, "reopen required");
        let tx = self.conn.unchecked_transaction()?;
        let current: u64 =
            tx.query_row("SELECT revision FROM meta WHERE id=1", [], |r| r.get(0))?;
        ensure!(current == self.revision, "stale export: reopen required");
        let mut results = Vec::new();
        for revision in 1..=self.revision {
            results.extend(read_outcomes(&tx, revision)?);
        }
        let states: Vec<_> = self
            .arms
            .values()
            .map(|a| a.state(self.manifest.session_date))
            .collect::<Result<_>>()?;
        let mut inputs = Vec::new();
        let mut statement =
            tx.prepare("SELECT revision,input,write_prepared_at_ms FROM inputs ORDER BY revision")?;
        let mut rows = statement.query([])?;
        while let Some(r) = rows.next()? {
            let c: Candle = serde_json::from_str(&r.get::<_, String>(1)?)?;
            inputs.push(json!({"revision":r.get::<_,u64>(0)?, "candle":c,
                "local_write_prepared_at_ms":r.get::<_,i64>(2)?}));
        }
        drop(rows);
        drop(statement);
        let value = json!({"schema":VERSION,"manifest_sha256":self.manifest.sha256()?,"revision":self.revision,
            "chain_sha256":self.chain,"evidence":"declared_clock_replay","states":states,"results":results,"inputs":inputs,
            "observed_clean_session":false,"order_emission_allowed":false,"external_delivery_exactly_once":false,
            "operational_activation":false});
        tx.commit()?;
        Ok(value)
    }
}
fn read_outcomes(conn: &Connection, revision: u64) -> Result<Vec<Outcome>> {
    let mut stmt = conn.prepare(
        "SELECT arm,source_sequence,result,result_sha FROM outcomes WHERE revision=?1 ORDER BY arm",
    )?;
    let rows = stmt.query_map([revision], outcome_row)?;
    rows.map(|r| decode_outcome(r?)).collect()
}
fn outcome_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(String, u64, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}
fn decode_outcome((arm, sequence, text, sha): (String, u64, String, String)) -> Result<Outcome> {
    let o: Outcome = serde_json::from_str(&text)?;
    ensure!(hash(&o)? == sha, "outcome checksum");
    ensure!(
        arm == o.arm && sequence == o.source_sequence,
        "outcome SQL index/payload mismatch"
    );
    Ok(o)
}
#[cfg(test)]
fn fault(frontier: &str) {
    // Present ONLY in the Rust test executable. No production env kill switch.
    if std::env::var("FINAM_LOCAL_TEST_CRASH_FRONTIER")
        .ok()
        .as_deref()
        == Some(frontier)
    {
        if let Ok(path) = std::env::var("FINAM_LOCAL_TEST_MARKER") {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            f.write_all(frontier.as_bytes()).unwrap();
            f.sync_all().unwrap();
        }
        println!("FINAM_LOCAL_CRASH_FRONTIER={frontier}");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
