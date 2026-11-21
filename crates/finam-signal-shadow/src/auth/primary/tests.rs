use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn owner() -> Arc<AuthLifecycle> {
    AuthLifecycle::new(SecretToken::new("fixture-only"), "selected".into()).unwrap()
}
fn details(extra: i64) -> TokenDetailsResponse {
    serde_json::from_value(serde_json::json!({"account_ids":["selected"],"readonly":true,
        "expires_at":(Utc::now()+chrono::Duration::seconds(900+extra)).to_rfc3339(),
        "md_permissions":[{"mic":"RTSX","delay_minutes":0,"quote_level":"QUOTE_LEVEL_DEPTH_OF_MARKET"}]})).unwrap()
}
async fn issue(extra: i64) -> Result<(AccessToken, TokenDetailsResponse), PrimaryAuthFailure> {
    Ok((AccessToken::new("fixture-access"), details(extra)))
}
async fn expire_cache(auth: &AuthLifecycle) {
    auth.state.lock().await.grant.as_mut().unwrap().renew_at =
        Instant::now() - Duration::from_millis(1);
}

#[tokio::test]
async fn primary_singleflight_and_cached_grant_are_fully_validated() {
    let auth = owner();
    let calls = AtomicUsize::new(0);
    let fetch = || async {
        calls.fetch_add(1, Ordering::SeqCst);
        tokio::task::yield_now().await;
        issue(0).await
    };
    let (a, b) = tokio::join!(
        auth.grant_primary_with(fetch, Duration::from_secs(1)),
        auth.grant_primary_with(fetch, Duration::from_secs(1))
    );
    assert_eq!(a.unwrap().generation(), 1);
    assert_eq!(b.unwrap().generation(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(auth.primary_readiness().await.can_open_new_commands);
}

#[tokio::test(start_paused = true)]
async fn command_budget_also_bounds_singleflight_lock_wait() {
    let auth = owner();
    let guard = auth.state.lock().await;
    let calls = AtomicUsize::new(0);
    let began = tokio::time::Instant::now();
    let error = auth
        .grant_primary_with(
            || async {
                calls.fetch_add(1, Ordering::SeqCst);
                issue(0).await
            },
            Duration::from_millis(50),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.class, PrimaryAuthClass::Unavailable);
    assert_eq!(began.elapsed(), Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(guard);
    assert!(!auth.primary_readiness().await.can_open_new_commands);
}

#[tokio::test(start_paused = true)]
async fn issue_uses_only_budget_left_after_singleflight_wait() {
    let auth = owner();
    let other = auth.clone();
    let guard = auth.state.lock().await;
    let release = async {
        tokio::time::sleep(Duration::from_millis(40)).await;
        drop(guard);
    };
    let began = tokio::time::Instant::now();
    let (_, result) = tokio::join!(
        release,
        other.grant_primary_with(std::future::pending, Duration::from_millis(100))
    );
    assert_eq!(result.err().unwrap().class, PrimaryAuthClass::Unavailable);
    assert_eq!(began.elapsed(), Duration::from_millis(100));
    assert_eq!(
        auth.primary_readiness().await.status,
        PrimaryAuthStatus::Unavailable
    );
}

#[tokio::test]
async fn known_network_failure_keeps_existing_socket_but_no_old_command_grant() {
    let auth = owner();
    auth.grant_primary_with(|| issue(0), Duration::from_secs(1))
        .await
        .unwrap();
    expire_cache(&auth).await;
    let error = auth
        .grant_primary_with(
            || async {
                Err(failure(
                    PrimaryAuthClass::Unavailable,
                    PrimaryAuthStage::Issue,
                ))
            },
            Duration::from_secs(1),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.class, PrimaryAuthClass::Unavailable);
    let status = auth.primary_readiness().await;
    assert_eq!(status.status, PrimaryAuthStatus::Unavailable);
    assert!(!status.can_open_new_commands);
    assert!(status.existing_socket_may_continue);
    let renewed = auth
        .grant_primary_with(|| issue(30), Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(renewed.generation(), 2);
    assert!(auth.primary_readiness().await.can_open_new_commands);
}

#[tokio::test]
async fn internal_budget_timeout_has_a_known_unavailable_outcome() {
    let auth = owner();
    auth.grant_primary_with(|| issue(0), Duration::from_secs(1))
        .await
        .unwrap();
    expire_cache(&auth).await;
    let error = auth
        .grant_primary_with(std::future::pending, Duration::from_millis(1))
        .await
        .err()
        .unwrap();
    assert_eq!(error.class, PrimaryAuthClass::Unavailable);
    assert!(auth.primary_readiness().await.existing_socket_may_continue);
    assert!(!auth.primary_readiness().await.can_open_new_commands);
}

#[tokio::test]
async fn caller_cancellation_is_not_a_safe_network_timeout_or_new_grant() {
    let auth = owner();
    let result = tokio::time::timeout(
        Duration::from_millis(1),
        auth.grant_primary_with(std::future::pending, Duration::from_secs(1)),
    )
    .await;
    assert!(result.is_err());
    let state = auth.primary_readiness().await;
    assert_eq!(state.status, PrimaryAuthStatus::CancelledOrHeld);
    assert!(!state.can_open_new_commands);
    assert!(!state.existing_socket_may_continue);
    let error = auth
        .grant_primary_with(
            || async { panic!("cancelled owner cannot silently retry") },
            Duration::from_secs(1),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.class, PrimaryAuthClass::CancelledOrHeld);
}

#[tokio::test]
async fn negative_binding_readonly_and_permissions_hold_permanently() {
    for kind in 0..3 {
        let auth = owner();
        let mut d = details(0);
        match kind {
            0 => d.readonly = Some(false),
            1 => d.account_ids = vec!["different".into()],
            _ => d.md_permissions.clear(),
        }
        let error = auth
            .grant_primary_with(
                || async { Ok((AccessToken::new("fixture"), d)) },
                Duration::from_secs(1),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.class, PrimaryAuthClass::Authentication);
        assert_eq!(
            auth.primary_readiness().await.status,
            PrimaryAuthStatus::SecurityHold
        );
        assert!(!auth.primary_readiness().await.existing_socket_may_continue);
        assert!(auth
            .grant_primary_with(
                || async { panic!("no reset after negative binding") },
                Duration::from_secs(1)
            )
            .await
            .is_err());
    }
}

#[tokio::test]
async fn malformed_or_short_expiry_is_not_unavailability() {
    for expiry in [
        "bad-time".to_owned(),
        (Utc::now() + chrono::Duration::seconds(90)).to_rfc3339(),
    ] {
        let auth = owner();
        let mut d = details(0);
        d.expires_at = Some(expiry);
        let error = auth
            .grant_primary_with(
                || async { Ok((AccessToken::new("fixture"), d)) },
                Duration::from_secs(1),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.class, PrimaryAuthClass::Validation);
        assert_eq!(
            auth.primary_readiness().await.status,
            PrimaryAuthStatus::SecurityHold
        );
    }
}

#[tokio::test]
async fn unknown_http_or_decode_failure_cannot_be_downgraded_to_network() {
    for class in [
        PrimaryAuthClass::Unknown,
        PrimaryAuthClass::Validation,
        PrimaryAuthClass::Authentication,
    ] {
        let auth = owner();
        let error = auth
            .grant_primary_with(
                || async { Err(failure(class, PrimaryAuthStage::Details)) },
                Duration::from_secs(1),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.class, class);
        assert!(!auth.primary_readiness().await.existing_socket_may_continue);
        assert_eq!(
            auth.primary_readiness().await.status,
            PrimaryAuthStatus::SecurityHold
        );
    }
}

#[tokio::test]
async fn clock_regression_stops_grant_issuance_before_polling_provider() {
    let auth = owner();
    auth.state.lock().await.last_wall = Utc::now() + chrono::Duration::seconds(30);
    let error = auth
        .grant_primary_with(
            || async { panic!("clock fault cannot issue") },
            Duration::from_secs(1),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.class, PrimaryAuthClass::Clock);
    assert!(!auth.primary_readiness().await.can_open_new_commands);
}

#[tokio::test]
async fn expired_renewal_margin_is_grant_needed_not_socket_reconnect() {
    let auth = owner();
    let grant = auth
        .grant_primary_with(|| issue(0), Duration::from_secs(1))
        .await
        .unwrap();
    assert!(grant.expires_at() > Utc::now());
    expire_cache(&auth).await;
    let readiness = auth.primary_readiness().await;
    assert_eq!(readiness.status, PrimaryAuthStatus::GrantNeeded);
    assert!(!readiness.can_open_new_commands);
    assert!(readiness.existing_socket_may_continue);
}

#[test]
fn classification_is_closed_and_exports_no_provider_error_text() {
    assert_eq!(
        classify(FinamErrorKind::TransportTimeout, PrimaryAuthStage::Issue).class,
        PrimaryAuthClass::Unavailable
    );
    assert_eq!(
        classify(FinamErrorKind::ApiAuthorization, PrimaryAuthStage::Details).class,
        PrimaryAuthClass::Authentication
    );
    assert_eq!(
        classify(FinamErrorKind::Decode, PrimaryAuthStage::Details).class,
        PrimaryAuthClass::Validation
    );
    assert_eq!(
        classify(FinamErrorKind::TransportHttp, PrimaryAuthStage::Issue).class,
        PrimaryAuthClass::Unknown
    );
    assert_eq!(
        serde_json::to_value(failure(
            PrimaryAuthClass::Unavailable,
            PrimaryAuthStage::Issue
        ))
        .unwrap(),
        serde_json::json!({"class":"Unavailable","stage":"Issue"})
    );
}
