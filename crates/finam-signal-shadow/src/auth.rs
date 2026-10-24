//! Single-flight, fixed-origin authentication. No order surface, fallback account,
//! synthetic TTL or logging of provider errors. Failure/cancellation latches HOLD.
use broker_finam::{
    AccessToken, AccessTokenLease, FinamAuthPolicy, FinamRestClient, SecretToken,
    TokenDetailsResponse,
};
use chrono::{DateTime, Utc};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const MARGIN: Duration = Duration::from_secs(120);
const MAX_TTL: Duration = Duration::from_secs(86_400);

#[derive(Clone)]
pub(crate) struct Grant {
    pub lease: AccessTokenLease,
    pub generation: u64,
    pub renew_at: Instant,
    pub expires_at: DateTime<Utc>,
}

impl Grant {
    pub fn valid(&self, wall: DateTime<Utc>, mono: Instant) -> bool {
        wall < self.expires_at && mono < self.lease.expires_at()
    }
    pub fn current(&self, wall: DateTime<Utc>, mono: Instant) -> bool {
        mono < self.renew_at && wall < self.expires_at - chrono::Duration::seconds(120)
    }
}

struct State {
    grant: Option<Grant>,
    held: bool,
    last_wall: DateTime<Utc>,
}

/// Account is an exact operator-selected identifier, never selected from a list.
/// Its presence in details is rechecked on EVERY renewal, along with readonly
/// and zero-delay RTSX depth-of-market permission for BARS + QUOTES.
pub struct AuthLifecycle {
    client: FinamRestClient,
    secret: SecretToken,
    account: String,
    state: Mutex<State>,
}

impl AuthLifecycle {
    pub fn new(secret: SecretToken, account: String) -> Result<Arc<Self>, &'static str> {
        if secret.as_str().is_empty() || account.is_empty() {
            return Err("auth_binding");
        }
        Ok(Arc::new(Self {
            client: FinamRestClient::shadow_auth_client().map_err(|_| "auth_client")?,
            secret,
            account,
            state: Mutex::new(State {
                grant: None,
                held: false,
                last_wall: Utc::now(),
            }),
        }))
    }

    pub(crate) fn binds(&self, account: &str) -> bool {
        self.account == account
    }

    pub(crate) async fn grant(&self) -> Result<Grant, &'static str> {
        self.grant_with(|| async {
            let response = self
                .client
                .auth(&self.secret)
                .await
                .map_err(|_| "auth_issue")?;
            let details = self
                .client
                .token_details_typed(&response.token)
                .await
                .map_err(|_| "auth_details")?;
            Ok((response.token, details))
        })
        .await
    }

    // Private seam: tests use deterministic responses; no public alternate origin
    // or unchecked grant constructor is available to the executable.
    async fn grant_with<F, Fut>(&self, issue: F) -> Result<Grant, &'static str>
    where
        F: FnOnce() -> Fut,
        Fut:
            std::future::Future<Output = Result<(AccessToken, TokenDetailsResponse), &'static str>>,
    {
        let mut state = self.state.lock().await;
        let wall = Utc::now();
        if state.held || wall < state.last_wall {
            state.held = true;
            return Err("auth_held_or_clock");
        }
        state.last_wall = wall;
        if let Some(grant) = &state.grant {
            if grant.current(wall, Instant::now()) {
                return Ok(grant.clone());
            }
        }
        // Set before await. Cancellation cannot leave a reusable old grant.
        state.held = true;
        let generation = state.grant.as_ref().map_or(1, |g| g.generation + 1);
        if generation > 128 {
            return Err("auth_rotation_budget");
        }
        let grant = tokio::time::timeout(Duration::from_secs(25), async {
            let (token, details) = issue().await?;
            let now = Utc::now();
            if now < wall {
                return Err("auth_clock");
            }
            validate(
                token,
                &details,
                &self.account,
                now,
                Instant::now(),
                generation,
            )
        })
        .await
        .map_err(|_| "auth_timeout")??;
        if state
            .grant
            .as_ref()
            .is_some_and(|old| grant.expires_at <= old.expires_at)
        {
            return Err("auth_expiry_not_advanced");
        }
        state.grant = Some(grant.clone());
        state.last_wall = Utc::now();
        state.held = false;
        Ok(grant)
    }
}

fn validate(
    token: AccessToken,
    details: &TokenDetailsResponse,
    account: &str,
    now: DateTime<Utc>,
    mono: Instant,
    generation: u64,
) -> Result<Grant, &'static str> {
    if token.is_empty()
        || details.readonly != Some(true)
        || !details.account_ids.iter().any(|id| id == account)
        || !details.md_permissions.iter().any(|p| {
            p.mic.as_deref() == Some("RTSX")
                && p.delay_minutes == Some(0)
                && p.quote_level.as_deref() == Some("QUOTE_LEVEL_DEPTH_OF_MARKET")
        })
    {
        return Err("auth_permissions");
    }
    let expires_at =
        DateTime::parse_from_rfc3339(details.expires_at.as_deref().ok_or("auth_expiry")?)
            .map_err(|_| "auth_expiry")?
            .with_timezone(&Utc);
    let ttl = (expires_at - now).to_std().map_err(|_| "auth_expiry")?;
    if ttl <= MARGIN + Duration::from_secs(30) || ttl > MAX_TTL {
        return Err("auth_lifetime");
    }
    // Conservative 1s clock/processing guard; never extend provider expiration.
    let ttl = ttl - Duration::from_secs(1);
    Ok(Grant {
        lease: AccessTokenLease::new(
            token,
            mono,
            FinamAuthPolicy {
                token_ttl: ttl,
                renew_before: Duration::ZERO,
            },
        ),
        generation,
        renew_at: mono + ttl - MARGIN,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn details(now: DateTime<Utc>) -> TokenDetailsResponse {
        serde_json::from_value(serde_json::json!({"account_ids":["other", "selected"],
            "readonly":true,"created_at":now.to_rfc3339(),
            "expires_at":(now+chrono::Duration::minutes(15)).to_rfc3339(),
            "md_permissions":[{"mic":"RTSX","delay_minutes":0,"quote_level":"QUOTE_LEVEL_DEPTH_OF_MARKET"}]})).unwrap()
    }

    fn owner() -> Arc<AuthLifecycle> {
        AuthLifecycle::new(SecretToken::new("fixture-only"), "selected".into()).unwrap()
    }

    async fn issue() -> Result<(AccessToken, TokenDetailsResponse), &'static str> {
        Ok((AccessToken::new("fixture-access"), details(Utc::now())))
    }

    #[tokio::test]
    async fn single_flight_cache_and_real_rotation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let auth = owner();
        let calls = AtomicUsize::new(0);
        let fetch = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            issue().await
        };
        let (a, b) = tokio::join!(auth.grant_with(fetch), auth.grant_with(fetch));
        assert_eq!(a.unwrap().generation, 1);
        assert_eq!(b.unwrap().generation, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        auth.state.lock().await.grant.as_mut().unwrap().renew_at = Instant::now();
        let g = auth
            .grant_with(|| async {
                let mut d = details(Utc::now());
                d.expires_at = Some((Utc::now() + chrono::Duration::minutes(30)).to_rfc3339());
                Ok((AccessToken::new("renewed-fixture"), d))
            })
            .await
            .unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(g.lease.token().as_str(), "renewed-fixture");
    }

    #[tokio::test]
    async fn failed_refresh_and_permission_drift_latch_no_fallback() {
        for case in 0..4 {
            let auth = owner();
            auth.grant_with(issue).await.unwrap();
            auth.state.lock().await.grant.as_mut().unwrap().renew_at = Instant::now();
            assert!(auth
                .grant_with(|| async {
                    if case == 0 {
                        return Err("injected_issue_failure");
                    }
                    let mut d = details(Utc::now());
                    match case {
                        1 => d.readonly = Some(false),
                        2 => d.account_ids = vec!["wrong".into()],
                        _ => d.md_permissions.clear(),
                    };
                    Ok((AccessToken::new("bad-grant"), d))
                })
                .await
                .is_err());
            assert!(auth
                .grant_with(|| async {
                    panic!("must not retry");
                })
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn cancellation_is_terminal_and_never_reuses_cached_token() {
        let auth = owner();
        assert!(tokio::time::timeout(
            Duration::from_millis(5),
            auth.grant_with(std::future::pending)
        )
        .await
        .is_err());
        assert!(auth
            .grant_with(|| async {
                panic!("must remain held");
            })
            .await
            .is_err());
    }
    #[test]
    fn exact_binding_and_real_expiration() {
        let now = Utc::now();
        let mono = Instant::now();
        let d = details(now);
        let g = validate(AccessToken::new("private"), &d, "selected", now, mono, 1).unwrap();
        assert!(g.current(now, mono));
        assert!(!g.current(now + chrono::Duration::minutes(13), mono));
        assert!(!g.current(now, mono + Duration::from_secs(780)));
        assert!(g.lease.expires_at() < mono + Duration::from_secs(900));
        assert!(!g.valid(now + chrono::Duration::minutes(15), mono));
        assert!(!g.valid(now, mono + Duration::from_secs(900)));
        assert!(validate(AccessToken::new("private"), &d, "missing", now, mono, 1).is_err());
    }
    #[test]
    fn reject_permission_and_expiry_drift() {
        let now = Utc::now();
        let mono = Instant::now();
        for case in 0..6 {
            let mut d = details(now);
            match case {
                0 => d.readonly = Some(false),
                1 => d.readonly = None,
                2 => d.md_permissions[0].delay_minutes = Some(15),
                3 => d.expires_at = None,
                4 => d.expires_at = Some(now.to_rfc3339()),
                _ => d.account_ids = vec!["other".into()],
            };
            assert!(validate(AccessToken::new("private"), &d, "selected", now, mono, 2).is_err());
        }
    }
}
