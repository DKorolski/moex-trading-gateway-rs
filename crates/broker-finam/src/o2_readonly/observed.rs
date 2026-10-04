//! Completion metadata for the additive observed-M1 input, collected by the
//! same closed-route, GET-only client. No retries, redirects or new scheduler.
use chrono::{SecondsFormat, Utc};

use super::*;
use crate::sparse_m10::{
    ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
    SNAPSHOT_MAX_BODY_BYTES,
};

impl Stage8bP1fO2GetOnlyClientV1 {
    /// Only the completed HTTP path sets `transport_complete`; no externally supplied completion flag
    /// or normalized bar vector can substitute for the retained response.
    pub async fn fetch_closed_m1_snapshot(
        &self,
        token: &AccessToken,
        plan: &ClosedM1RequestPlan,
    ) -> Result<ClosedM1SnapshotEvidenceV1, Stage8bP1fO2GetErrorV1> {
        if token.is_empty() {
            return Err(Stage8bP1fO2GetErrorV1::MissingToken);
        }
        plan.preflight(Utc::now())
            .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?;
        let mut parts = Vec::new();
        let mut remaining = SNAPSHOT_MAX_BODY_BYTES;
        for (start, end) in plan.ranges() {
            let url = self.render_and_validate(&Stage8bP1fO2GetRouteV1::Bars {
                start_time: start.to_rfc3339_opts(SecondsFormat::Secs, true),
                end_time: end.to_rfc3339_opts(SecondsFormat::Secs, true),
            })?;
            let requested_at = Utc::now();
            // A slow preceding part cannot extend history availability.
            plan.preflight(requested_at)
                .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?;
            let mut response = self
                .http
                .get(url)
                .bearer_auth(token.as_str())
                .send()
                .await
                .map_err(|_| Stage8bP1fO2GetErrorV1::Transport)?;
            // Preserve the original size expectation before draining the body.
            let declared_body_bytes = response.content_length();
            let mut body =
                CompletedBody::new(response.status().as_u16(), declared_body_bytes, remaining)?;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| Stage8bP1fO2GetErrorV1::Transport)?
            {
                body.push(&chunk)?;
            }
            let raw_body = body.finish()?;
            let received_at = Utc::now();
            if received_at < requested_at {
                return Err(Stage8bP1fO2GetErrorV1::Transport);
            }
            remaining -= raw_body.len();
            parts.push(ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: format!(
                    "{STAGE8B_P1F_O2_FINAM_BASE_URL}/v1/instruments/{STAGE8B_P1F_O2_VENUE_SYMBOL}/bars"
                ),
                symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
                timeframe: STAGE8B_P1F_O2_BARS_TIMEFRAME.into(),
                start, end, requested_at, received_at,
                status: 200,
                transport_complete: true,
                declared_body_bytes,
                response_sha256: lower_hex(&Sha256::digest(raw_body.as_bytes())),
                raw_body,
            });
        }
        Ok(ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: plan.start(),
            end: plan.end(),
            parts,
        })
    }

    /// Pure route evidence reconstruction from the retained raw response.
    /// This is NOT admission: the caller must also validate the complete
    /// snapshot against an independently constructed calendar request plan.
    pub fn observations_for_closed_m1_snapshot(
        &self,
        evidence: &ClosedM1SnapshotEvidenceV1,
    ) -> Result<Vec<Stage8bP1fO2GetObservationV1>, Stage8bP1fO2GetErrorV1> {
        evidence
            .parts
            .iter()
            .map(|part| {
                let url = self.render_and_validate(&Stage8bP1fO2GetRouteV1::Bars {
                    start_time: part.start.to_rfc3339_opts(SecondsFormat::Secs, true),
                    end_time: part.end.to_rfc3339_opts(SecondsFormat::Secs, true),
                })?;
                Ok(Stage8bP1fO2GetObservationV1 {
                    route: Stage8bP1fO2GetRouteKindV1::Bars,
                    request_sha256: request_sha256(&url),
                    response_sha256: lower_hex(&Sha256::digest(part.raw_body.as_bytes())),
                    exact_response_bytes: part.raw_body.as_bytes().to_vec(),
                })
            })
            .collect()
    }
}

/// Bounded incremental body read; oversized unknown-length responses are
/// rejected before appending, not after allocating the entire body.
struct CompletedBody {
    declared: Option<u64>,
    limit: usize,
    bytes: Vec<u8>,
}

impl CompletedBody {
    fn new(
        status: u16,
        declared: Option<u64>,
        limit: usize,
    ) -> Result<Self, Stage8bP1fO2GetErrorV1> {
        if status != 200 {
            return Err(Stage8bP1fO2GetErrorV1::HttpStatus);
        }
        if limit == 0 || declared.is_some_and(|n| n > limit as u64) {
            return Err(Stage8bP1fO2GetErrorV1::ResponseTooLarge);
        }
        Ok(Self {
            declared,
            limit,
            bytes: Vec::new(),
        })
    }

    fn push(&mut self, chunk: &[u8]) -> Result<(), Stage8bP1fO2GetErrorV1> {
        let length = self
            .bytes
            .len()
            .checked_add(chunk.len())
            .filter(|n| *n <= self.limit)
            .ok_or(Stage8bP1fO2GetErrorV1::ResponseTooLarge)?;
        if self.declared.is_some_and(|n| length as u64 > n) {
            return Err(Stage8bP1fO2GetErrorV1::Transport);
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    fn finish(self) -> Result<String, Stage8bP1fO2GetErrorV1> {
        if self.bytes.is_empty() || self.declared.is_some_and(|n| n != self.bytes.len() as u64) {
            return Err(Stage8bP1fO2GetErrorV1::Transport);
        }
        String::from_utf8(self.bytes).map_err(|_| Stage8bP1fO2GetErrorV1::Decode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_http_body_checks_status_length_and_stream_limit() {
        assert!(CompletedBody::new(206, None, 10).is_err());
        assert!(CompletedBody::new(204, None, 10).is_err());
        assert!(CompletedBody::new(200, Some(11), 10).is_err());
        let mut b = CompletedBody::new(200, None, 10).unwrap();
        b.push(b"12345").unwrap();
        assert!(b.push(b"678901").is_err());
        assert_eq!(b.bytes.len(), 5); // Failed chunk was not retained.
        let mut b = CompletedBody::new(200, Some(10), 10).unwrap();
        b.push(b"12345").unwrap();
        assert!(b.finish().is_err());
        let mut b = CompletedBody::new(200, Some(5), 10).unwrap();
        assert!(b.push(b"123456").is_err());
        assert!(CompletedBody::new(200, None, 10).unwrap().finish().is_err());
        let mut b = CompletedBody::new(200, None, 10).unwrap();
        b.push(&[0xff]).unwrap();
        assert!(b.finish().is_err());
        for declared in [None, Some(4)] {
            let mut b = CompletedBody::new(200, declared, 10).unwrap();
            b.push(b"ab").unwrap();
            b.push(b"cd").unwrap();
            assert_eq!(b.finish().unwrap(), "abcd");
        }
    }

    #[test]
    fn observed_plan_preflight_does_not_confuse_chunk_span_and_history_depth() {
        let start = "2026-09-28T04:00:00Z".parse().unwrap();
        let end = start + chrono::Duration::minutes(10);
        let plan = ClosedM1RequestPlan::new(start, end, vec![end]).unwrap();
        assert_eq!(plan.ranges().collect::<Vec<_>>(), vec![(start, end)]);
        assert!(plan.preflight(end).is_ok());
        assert!(plan.preflight(end - chrono::Duration::seconds(1)).is_err());
        assert!(plan.preflight(start + chrono::Duration::days(8)).is_err());
    }
}
