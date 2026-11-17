//! Independent V8 journal lineage. Typed source math/timing is checked by the
//! restart reader; this checks reason-specific episode/owner and single winner.
use super::*;

pub(super) fn validate(
    config: &WsPrimaryConfig,
    bytes: &[u8],
    required: &[i64],
    now: i64,
) -> Result<(), VerifyError> {
    let mut subscription = None::<Value>;
    let mut authority = None::<Value>;
    let mut episode = None::<Value>;
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
            }
            "primary_ws_data_gap_authorized" => {
                let s = subscription.as_ref().ok_or(VerifyError::Source)?;
                let label = integer(e, "label_ms")?;
                let close = label.checked_add(600000).ok_or(VerifyError::Source)?;
                if !config.primary.ws_historical_recovery()
                    || authority.is_some()
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
            }
            "primary_recovery_attempt_prepared" | "primary_recovery_attempt_failed" => {
                authority = None;
            }
            "primary_ws_historical_source_selected" => {
                let a = authority.as_ref().ok_or(VerifyError::Source)?;
                let ep = episode.as_ref().ok_or(VerifyError::Source)?;
                if !config.primary.ws_historical_recovery()
                    || selected.is_some()
                    || e["generation"] != a["generation"]
                    || e["label_ms"] != a["label_ms"]
                    || e["continuity_revision"] != a["continuity_revision"]
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
            } else if selected.is_some() {
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
                {
                    return Err(VerifyError::Source);
                }
            }
        }
    }
    if selected.is_some() || pending.is_some() {
        return Err(VerifyError::Source);
    }
    Ok(())
}
