//! Independent V8 journal lineage. Typed source math/timing is checked by the
//! restart reader; this checks reason-specific episode/owner and single winner.
use super::*;

struct RestWinner {
    source: Value,
    episode: u64,
    generation: u64,
    committed: bool,
}

pub(super) fn validate(
    config: &WsPrimaryConfig,
    bytes: &[u8],
    required: &[i64],
    now: i64,
) -> Result<(), VerifyError> {
    let mut subscription = None::<Value>;
    let mut authority = None::<Value>;
    let mut episode = None::<Value>;
    let mut episode_revision = None;
    let mut recovery_requests = BTreeMap::<String, Value>::new();
    let mut rest_winner = None::<RestWinner>;
    let mut selected = None::<Value>;
    let mut pending = None::<Value>;
    let mut committed = BTreeSet::new();
    for line in std::str::from_utf8(bytes)
        .map_err(|_| VerifyError::Decode)?
        .lines()
    {
        let row = super::super::json::parse(line).map_err(|_| VerifyError::Decode)?;
        let e = &row["event"];
        match e["event"].as_str().unwrap_or("") {
            "primary_subscription_sent" => {
                subscription = Some(e.clone());
                authority = None;
                if rest_winner.as_ref().is_some_and(|w| !w.committed) {
                    return Err(VerifyError::Source);
                }
                rest_winner = None;
            }
            "primary_ws_data_gap_authorized" => {
                let s = subscription.as_ref().ok_or(VerifyError::Source)?;
                let label = integer(e, "label_ms")?;
                let close = label.checked_add(600000).ok_or(VerifyError::Source)?;
                if !config.primary.ws_historical_recovery()
                    || authority.is_some()
                    || pending.is_some()
                    || rest_winner.is_some()
                    || e["generation"] != s["generation"]
                    || e["subscribed_at_ms"] != s["sent_at_ms"]
                    || integer(s, "sent_at_ms")? > label
                    || e["physical_owner_bound"] != true
                    || e["reason"] != "LivePublicationDeadline"
                    || integer(e, "data_gap_at_ms")?
                        <= close + 2000 - config.base.clock_uncertainty_ms as i64
                    || integer(e, "data_gap_at_ms")? > now
                    || !required.contains(&label)
                    || required.last().copied() == Some(label)
                    || committed.contains(&label)
                {
                    return Err(VerifyError::Source);
                }
                authority = Some(e.clone());
            }
            "primary_recovery_episode_started" => {
                episode = Some(e["audit"].clone());
                episode_revision = e["continuity_revision"].as_u64();
            }
            "primary_recovery_request_issued" => {
                recovery_requests.insert(string(e, "ticket_sha256")?.to_owned(), e.clone());
            }
            "primary_recovery_attempt_prepared" | "primary_recovery_attempt_failed" => {
                authority = None;
                if rest_winner.as_ref().is_some_and(|w| w.committed) {
                    rest_winner = None;
                }
            }
            "primary_recovery_episode_ready" => {
                if let Some(winner) = &rest_winner {
                    let audit = &e["audit"];
                    if !winner.committed
                        || number(audit, "episode")? != winner.episode
                        || number(audit, "generation")? != winner.generation
                        || audit["stage"] != "Ready"
                        || audit["canonical_unresolved"] != false
                        || audit["transport_unresolved"] != false
                        || authority.is_none()
                    {
                        return Err(VerifyError::Source);
                    }
                    // A matching durable REST commit AND its Ready close this
                    // authority. Arbitrary source/commit/Ready records cannot.
                    authority = None;
                    rest_winner = None;
                }
            }
            "primary_ws_historical_source_selected" => {
                let a = authority.as_ref().ok_or(VerifyError::Source)?;
                let ep = episode.as_ref().ok_or(VerifyError::Source)?;
                if !config.primary.ws_historical_recovery()
                    || selected.is_some()
                    || e["generation"] != a["generation"]
                    || e["label_ms"] != a["label_ms"]
                    || e["continuity_revision"] != a["continuity_revision"]
                    || episode_revision != Some(number(a, "continuity_revision")?)
                    || e["episode"] != ep["episode"]
                    || ep["generation"] != e["generation"]
                    || ep["original"]["kind"] != serde_json::json!({"Source":"Continuity"})
                    || ep["original"]["generation"] != e["generation"]
                    || e["physical_reconnect"] != false
                    || integer(e, "at_ms")? < integer(ep, "fault_observed_at_ms")?
                    || integer(ep, "fault_observed_at_ms")? < integer(a, "data_gap_at_ms")?
                    || integer(e, "at_ms")? >= integer(ep, "deadline_ms")?
                    || integer(ep, "deadline_ms")? > integer(ep, "fault_observed_at_ms")? + 45000
                {
                    return Err(VerifyError::Source);
                }
                selected = Some(e.clone());
            }
            "recovery_step_durable_publication" if e["origin"] == "WS_RECOVERY_BACKFILL" => {
                let r = pending.as_ref().ok_or(VerifyError::Source)?;
                if e["source_sha256"] != r["source_sha256"]
                    || integer(e, "actual_model_start_at_ms")?
                        < integer(&r["receipt"], "canonical_ready_at_ms")?
                    || integer(e, "actual_model_result_at_ms")?
                        < integer(e, "actual_model_start_at_ms")?
                    || e["actual_source_received_at_ms"] != r["receipt"]["boundary_received_at_ms"]
                    || e["virtual_callback_at_ms"] != r["receipt"]["close_ms"]
                    || e["history_proves_live_readiness"] != false
                {
                    return Err(VerifyError::Source);
                }
            }
            _ => (),
        }
        if row["kind"] == "canonical_source" {
            if e["receipt"]["origin"] == "WS_RECOVERY_BACKFILL" {
                let s = selected.take().ok_or(VerifyError::Source)?;
                let a = authority.take().ok_or(VerifyError::Source)?;
                let ep = episode.as_ref().ok_or(VerifyError::Source)?;
                let r = &e["receipt"];
                if r["generation"] != s["generation"]
                    || r["label_ms"] != s["label_ms"]
                    || r["continuity_revision"] != a["continuity_revision"]
                    || r["subscribed_at_ms"] != a["subscribed_at_ms"]
                    || r["data_gap_at_ms"] != a["data_gap_at_ms"]
                    || r["episode"] != ep["episode"]
                    || r["episode_deadline_ms"] != ep["deadline_ms"]
                    || r["canonical_ready_at_ms"] != s["at_ms"]
                    || r["subscribed_at_ms"] != s["ws_witness"]["subscribed_at_ms"]
                    || r["boundary_received_at_ms"] != s["ws_witness"]["boundary_received_at_ms"]
                    || r["boundary_frame_sha256"] != s["ws_witness"]["boundary_frame_sha256"]
                    || r["versions"] != s["ws_witness"]["versions"]
                    || e["source_sha256"] != s["source_sha256"]
                    || pending.is_some()
                {
                    return Err(VerifyError::Source);
                }
                pending = Some(e.clone());
            } else if e["receipt"]["origin"] == "RECOVERY_BACKFILL" && authority.is_some() {
                let a = authority.as_ref().ok_or(VerifyError::Source)?;
                let ep = episode.as_ref().ok_or(VerifyError::Source)?;
                let r = &e["receipt"];
                let request = recovery_requests
                    .get(string(r, "recovery_plan_sha256")?)
                    .ok_or(VerifyError::Source)?;
                if selected.is_some()
                    || pending.is_some()
                    || rest_winner.is_some()
                    || r["label_ms"] != a["label_ms"]
                    || r["generation"] != a["generation"]
                    || r["subscribed_at_ms"] != a["subscribed_at_ms"]
                    || episode_revision != Some(number(a, "continuity_revision")?)
                    || r["generation"] != ep["generation"]
                    || request["generation"] != ep["generation"]
                    || request["scope"]["episode"] != ep["episode"]
                    || request["scope"]["deadline_ms"] != ep["deadline_ms"]
                    || ep["original"]["generation"] != ep["generation"]
                    || ep["original"]["kind"] != serde_json::json!({"Source":"Continuity"})
                    || integer(ep, "fault_observed_at_ms")? < integer(a, "data_gap_at_ms")?
                    || integer(r, "canonical_ready_at_ms")? < integer(ep, "fault_observed_at_ms")?
                    || integer(r, "replay_deadline_ms")? > integer(ep, "deadline_ms")?
                {
                    return Err(VerifyError::Source);
                }
                rest_winner = Some(RestWinner {
                    source: e.clone(),
                    episode: number(ep, "episode")?,
                    generation: number(ep, "generation")?,
                    committed: false,
                });
            } else if selected.is_some() || authority.is_some() {
                return Err(VerifyError::Source);
            }
        }
        if row["kind"] == "canonical_commit" {
            let label = integer(e, "model_label_utc_ms")?;
            if !committed.insert(label) {
                return Err(VerifyError::Source);
            }
            if let Some(p) = pending.take() {
                if e["source_sha256"] != p["source_sha256"]
                    || e["model_label_utc_ms"] != p["receipt"]["label_ms"]
                    || integer(e, "publication_checked_at_ms")?
                        >= integer(&p["receipt"], "episode_deadline_ms")?
                    || integer(e, "publication_checked_at_ms")?
                        > integer(&p["receipt"], "replay_deadline_ms")?
                {
                    return Err(VerifyError::Source);
                }
            }
            if let Some(winner) = &mut rest_winner {
                let r = &winner.source["receipt"];
                if winner.committed
                    || e["source_sha256"] != winner.source["source_sha256"]
                    || e["model_label_utc_ms"] != r["label_ms"]
                    || integer(e, "publication_checked_at_ms")?
                        < integer(r, "canonical_ready_at_ms")?
                    || integer(e, "publication_checked_at_ms")? > integer(r, "replay_deadline_ms")?
                {
                    return Err(VerifyError::Source);
                }
                winner.committed = true;
            } else if authority.is_some() {
                return Err(VerifyError::Source);
            }
        }
    }
    if selected.is_some() || pending.is_some() || rest_winner.as_ref().is_some_and(|w| !w.committed)
    {
        return Err(VerifyError::Source);
    }
    Ok(())
}

#[cfg(test)]
mod reviewer_v8_multiple_data_gaps {
    use super::*;
    use crate::primary::{calendar::tests::Fixture, DeliveryPolicy};
    use serde_json::json;

    // Isolated ws-lineage validator regression, NOT a full runtime fixture.
    // A managed two-episode full-day regression is also required for closure.
    #[test]
    fn reviewer_v8_rest_winner_retires_authority_before_next_data_gap() {
        let mut f = Fixture::historical();
        f.config.primary.delivery = DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8;
        let t = f.open();
        let required: Vec<_> = (0..5).map(|i| t + i * 600000).collect();
        let mut rows = vec![json!({"kind":"transport_event", "event":{
            "event":"primary_subscription_sent", "generation":1, "sent_at_ms":t
        }})];
        let encode = |rows: &[Value]| -> Vec<u8> {
            rows.iter()
                .flat_map(|r| {
                    let mut b = serde_json::to_vec(r).unwrap();
                    b.push(b'\n');
                    b
                })
                .collect()
        };
        for n in 1..=2 {
            let label = t + n * 600000;
            let gap = label + 600000 + 1904;
            rows.push(json!({"kind":"transport_event", "event":{
                "event":"primary_ws_data_gap_authorized", "generation":1,
                "subscribed_at_ms":t, "label_ms":label, "data_gap_at_ms":gap,
                "physical_owner_bound":true, "reason":"LivePublicationDeadline",
                "continuity_revision":n
            }}));
            let audit = json!({"episode":n,"generation":1,
                "fault_observed_at_ms":gap,"deadline_ms":gap+45000,
                "original":{"generation":1,"kind":{"Source":"Continuity"}}});
            rows.push(json!({"kind":"transport_event", "event":{
                "event":"primary_recovery_episode_started", "audit":audit, "continuity_revision":n
            }}));
            let ticket = format!("{n}").repeat(64);
            rows.push(json!({"kind":"transport_event", "event":{
                "event":"primary_recovery_request_issued", "ticket_sha256":ticket,
                "generation":1, "scope":{"episode":n,"deadline_ms":gap+45000}
            }}));
            rows.push(json!({"kind":"canonical_source", "event":{
                "receipt":{"origin":"RECOVERY_BACKFILL", "label_ms":label,
                    "generation":1,"subscribed_at_ms":t,"recovery_plan_sha256":ticket,
                    "canonical_ready_at_ms":gap+10,"replay_deadline_ms":gap+10010},
                "source_sha256":format!("{n}").repeat(64)
            }}));
            rows.push(json!({"kind":"canonical_commit", "event":{
                "model_label_utc_ms":label,"publication_checked_at_ms":gap+50,
                "source_sha256":format!("{n}").repeat(64)
            }}));
            rows.push(json!({"kind":"transport_event", "event":{
                "event":"primary_recovery_episode_ready", "audit":{
                    "episode":n,"generation":1,"stage":"Ready",
                    "canonical_unresolved":false,"transport_unresolved":false
                }
            }}));
            assert!(
                validate(&f.config, &encode(&rows), &required, t + 3000000).is_ok(),
                "completed REST winner must retire ONLY its matching WS authority; episode {n}"
            );
            for case in 0..8 {
                let mut bad = rows.clone();
                match case {
                    0..=2 => {
                        let receipt = &mut bad
                            .iter_mut()
                            .find(|r| r["kind"] == "canonical_source")
                            .unwrap()["event"]["receipt"];
                        match case {
                            0 => receipt["generation"] = json!(2),
                            1 => receipt["label_ms"] = json!(t + 600001),
                            _ => receipt["recovery_plan_sha256"] = json!("0".repeat(64)),
                        }
                    }
                    3 => {
                        let r = bad
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_ws_data_gap_authorized")
                            .unwrap();
                        r["event"]["continuity_revision"] = json!(999);
                    }
                    4 => {
                        let r = bad
                            .iter_mut()
                            .find(|r| r["kind"] == "canonical_commit")
                            .unwrap();
                        r["event"]["source_sha256"] = json!("0".repeat(64));
                    }
                    5 => {
                        let at = bad
                            .iter()
                            .position(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                            .unwrap();
                        bad.insert(at, bad[1].clone());
                    }
                    6 => bad.retain(|r| r["kind"] != "canonical_commit"),
                    _ => {
                        let r = bad
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                            .unwrap();
                        r["event"]["audit"]["generation"] = json!(2);
                    }
                }
                assert!(
                    validate(&f.config, &encode(&bad), &required, t + 3000000).is_err(),
                    "foreign or premature authority retirement: {case}"
                );
            }
        }
    }
}
