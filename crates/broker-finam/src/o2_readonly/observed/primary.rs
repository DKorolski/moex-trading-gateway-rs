//! Additive typed single-GET path; legacy observed collection is unchanged.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PrimaryGetFailure {
    Timeout,
    Unavailable,
    Authentication,
    Integrity,
    Unknown,
}

fn status(code: u16) -> Result<(), PrimaryGetFailure> {
    match code {
        200 => Ok(()),
        401 | 403 => Err(PrimaryGetFailure::Authentication),
        408 | 429 | 500..=599 => Err(PrimaryGetFailure::Unavailable),
        _ => Err(PrimaryGetFailure::Integrity),
    }
}
fn transport(error: reqwest::Error) -> PrimaryGetFailure {
    if error.is_timeout() {
        PrimaryGetFailure::Timeout
    } else if error.is_connect() {
        PrimaryGetFailure::Unavailable
    } else if error.is_body() || error.is_decode() {
        PrimaryGetFailure::Integrity
    } else {
        PrimaryGetFailure::Unknown
    }
}

async fn complete(
    request: reqwest::RequestBuilder,
    attempts: &AtomicU64,
) -> Result<(String, Option<u64>), PrimaryGetFailure> {
    attempts.fetch_add(1, Ordering::SeqCst);
    let mut response = request.send().await.map_err(transport)?;
    status(response.status().as_u16())?;
    let declared = response.content_length();
    let mut body = CompletedBody::new(200, declared, SNAPSHOT_MAX_BODY_BYTES)
        .map_err(|_| PrimaryGetFailure::Integrity)?;
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        body.push(&chunk)
            .map_err(|_| PrimaryGetFailure::Integrity)?;
    }
    Ok((
        body.finish().map_err(|_| PrimaryGetFailure::Integrity)?,
        declared,
    ))
}

impl Stage8bP1fO2GetOnlyClientV1 {
    /// One real bounded-body GET, no retries/redirects/alternate origin. The
    /// outer command owner supplies STOP and the total <=5s deadline.
    pub async fn fetch_closed_m1_primary(
        &self,
        token: &AccessToken,
        plan: &ClosedM1RequestPlan,
        attempts: &AtomicU64,
    ) -> Result<ClosedM1SnapshotEvidenceV1, PrimaryGetFailure> {
        if token.is_empty() {
            return Err(PrimaryGetFailure::Authentication);
        }
        plan.preflight(Utc::now())
            .map_err(|_| PrimaryGetFailure::Integrity)?;
        let ranges: Vec<_> = plan.ranges().collect();
        if ranges.len() != 1 {
            return Err(PrimaryGetFailure::Integrity);
        }
        let (start, end) = ranges[0];
        let url = self
            .render_and_validate(&Stage8bP1fO2GetRouteV1::Bars {
                start_time: start.to_rfc3339_opts(SecondsFormat::Secs, true),
                end_time: end.to_rfc3339_opts(SecondsFormat::Secs, true),
            })
            .map_err(|_| PrimaryGetFailure::Integrity)?;
        let requested_at = Utc::now();
        plan.preflight(requested_at)
            .map_err(|_| PrimaryGetFailure::Integrity)?;
        let (raw_body, declared_body_bytes) =
            complete(self.http.get(url).bearer_auth(token.as_str()), attempts).await?;
        let received_at = Utc::now();
        if received_at < requested_at {
            return Err(PrimaryGetFailure::Integrity);
        }
        Ok(ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start,
            end,
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: format!(
                    "{STAGE8B_P1F_O2_FINAM_BASE_URL}/v1/instruments/{}/bars",
                    self.venue_symbol
                ),
                symbol: self.venue_symbol.clone(),
                timeframe: STAGE8B_P1F_O2_BARS_TIMEFRAME.into(),
                start,
                end,
                requested_at,
                received_at,
                status: 200,
                transport_complete: true,
                declared_body_bytes,
                response_sha256: lower_hex(&Sha256::digest(raw_body.as_bytes())),
                raw_body,
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[test]
    fn typed_status_never_confuses_auth_or_unknown_with_unavailable() {
        assert_eq!(status(401), Err(PrimaryGetFailure::Authentication));
        assert_eq!(status(403), Err(PrimaryGetFailure::Authentication));
        for code in [408, 429, 500, 503] {
            assert_eq!(status(code), Err(PrimaryGetFailure::Unavailable));
        }
        for code in [204, 206, 302, 400, 404] {
            assert_eq!(status(code), Err(PrimaryGetFailure::Integrity));
        }
    }
    async fn server(response: Vec<u8>) -> (reqwest::RequestBuilder, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            socket.write_all(&response).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        // Private loopback test seam; no production endpoint override exists.
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        (client.get(format!("http://{address}/test")), task)
    }
    #[tokio::test]
    async fn real_http_complete_body_and_attempt_counter() {
        let (request, task) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\ntest".to_vec(),
        )
        .await;
        let attempts = AtomicU64::new(0);
        assert_eq!(
            complete(request, &attempts).await.unwrap(),
            ("test".into(), Some(4))
        );
        task.await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn real_http_authorization_and_redirect_are_not_transient() {
        for code in [401, 403, 302] {
            let (request, task) = server(
                format!(
                    "HTTP/1.1 {code} Refusal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .into_bytes(),
            )
            .await;
            let result = complete(request, &AtomicU64::new(0)).await;
            assert_eq!(
                result.err().unwrap(),
                if code == 302 {
                    PrimaryGetFailure::Integrity
                } else {
                    PrimaryGetFailure::Authentication
                }
            );
            task.await.unwrap();
        }
    }
    #[tokio::test]
    async fn declared_length_truncation_never_becomes_a_complete_snapshot() {
        let (request, task) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ntest".to_vec(),
        )
        .await;
        assert!(complete(request, &AtomicU64::new(0)).await.is_err());
        task.await.unwrap();
    }
}
