//! Opt-in WS-primary command grants. Known endpoint unavailability does not
//! revoke an already validated independent socket; negative/unknown outcomes do.
//! Legacy grant_with keeps its existing terminal failure/cancellation policy.
use super::*;
use broker_finam::FinamErrorKind;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrimaryAuthClass {
    Unavailable,
    Authentication,
    Validation,
    Clock,
    CancelledOrHeld,
    Budget,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrimaryAuthStage {
    Issue,
    Details,
    Grant,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PrimaryAuthFailure {
    pub class: PrimaryAuthClass,
    pub stage: PrimaryAuthStage,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrimaryAuthStatus {
    NotIssued,
    Issuing,
    Ready,
    GrantNeeded,
    Unavailable,
    SecurityHold,
    CancelledOrHeld,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PrimaryAuthReadiness {
    pub status: PrimaryAuthStatus,
    pub can_open_new_commands: bool,
    pub existing_socket_may_continue: bool,
}

/// No Debug/Serialize or public token accessor. New commands get the actual
/// validated lease, never a manufactured expiry or the previous stale grant.
pub struct PrimaryAuthGrant {
    pub(crate) grant: Grant,
}
impl PrimaryAuthGrant {
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.grant.expires_at
    }
    pub fn generation(&self) -> u64 {
        self.grant.generation
    }
}

fn failure(class: PrimaryAuthClass, stage: PrimaryAuthStage) -> PrimaryAuthFailure {
    PrimaryAuthFailure { class, stage }
}
fn classify(kind: FinamErrorKind, stage: PrimaryAuthStage) -> PrimaryAuthFailure {
    use FinamErrorKind::*;
    let class = match kind {
        TransportTimeout | TransportConnect | ApiTimeout | ApiServer | ApiRateLimited => {
            PrimaryAuthClass::Unavailable
        }
        ApiAuthentication | ApiAuthorization => PrimaryAuthClass::Authentication,
        Decode | InvalidConfiguration | MissingToken | ApiBadRequest => {
            PrimaryAuthClass::Validation
        }
        _ => PrimaryAuthClass::Unknown,
    };
    failure(class, stage)
}

impl AuthLifecycle {
    /// Caller must still bound the whole command to its proof/run deadline and
    /// STOP. This method does not run a retry loop or rotate an existing socket.
    pub async fn grant_primary(&self) -> Result<PrimaryAuthGrant, PrimaryAuthFailure> {
        self.grant_primary_budget(Duration::from_secs(5)).await
    }

    pub async fn grant_primary_budget(
        &self,
        budget: Duration,
    ) -> Result<PrimaryAuthGrant, PrimaryAuthFailure> {
        self.grant_primary_with(
            || async {
                let response = self
                    .client
                    .auth(&self.secret)
                    .await
                    .map_err(|e| classify(e.kind(), PrimaryAuthStage::Issue))?;
                let details = self
                    .client
                    .token_details_typed(&response.token)
                    .await
                    .map_err(|e| classify(e.kind(), PrimaryAuthStage::Details))?;
                Ok((response.token, details))
            },
            budget,
        )
        .await
    }

    pub async fn primary_readiness(&self) -> PrimaryAuthReadiness {
        let mut state = self.state.lock().await;
        if Utc::now() < state.last_wall {
            state.held = true;
            state.primary_status = PrimaryAuthStatus::SecurityHold;
        }
        let status = if state.held && state.primary_status == PrimaryAuthStatus::Issuing {
            PrimaryAuthStatus::CancelledOrHeld
        } else if state.primary_status == PrimaryAuthStatus::Ready
            && state
                .grant
                .as_ref()
                .is_some_and(|g| !g.current(Utc::now(), Instant::now()))
        {
            PrimaryAuthStatus::GrantNeeded
        } else {
            state.primary_status
        };
        PrimaryAuthReadiness {
            status,
            can_open_new_commands: !state.held && status == PrimaryAuthStatus::Ready,
            existing_socket_may_continue: !state.held
                && state.grant.is_some()
                && matches!(
                    status,
                    PrimaryAuthStatus::Ready
                        | PrimaryAuthStatus::GrantNeeded
                        | PrimaryAuthStatus::Unavailable
                ),
        }
    }

    async fn grant_primary_with<F, Fut>(
        &self,
        issue: F,
        budget: Duration,
    ) -> Result<PrimaryAuthGrant, PrimaryAuthFailure>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<
            Output = Result<(AccessToken, TokenDetailsResponse), PrimaryAuthFailure>,
        >,
    {
        if budget.is_zero() || budget > Duration::from_secs(5) {
            return Err(failure(PrimaryAuthClass::Budget, PrimaryAuthStage::Grant));
        }
        let began = tokio::time::Instant::now();
        let mut state = tokio::time::timeout(budget, self.state.lock())
            .await
            .map_err(|_| failure(PrimaryAuthClass::Unavailable, PrimaryAuthStage::Grant))?;
        let budget = budget
            .checked_sub(began.elapsed())
            .filter(|b| !b.is_zero())
            .ok_or_else(|| failure(PrimaryAuthClass::Unavailable, PrimaryAuthStage::Grant))?;
        let wall = Utc::now();
        if state.held {
            return Err(failure(
                PrimaryAuthClass::CancelledOrHeld,
                PrimaryAuthStage::Grant,
            ));
        }
        if wall < state.last_wall {
            state.held = true;
            state.primary_status = PrimaryAuthStatus::SecurityHold;
            return Err(failure(PrimaryAuthClass::Clock, PrimaryAuthStage::Grant));
        }
        state.last_wall = wall;
        if state.primary_status != PrimaryAuthStatus::Unavailable {
            if let Some(grant) = &state.grant {
                if grant.current(wall, Instant::now()) {
                    let grant = grant.clone();
                    state.primary_status = PrimaryAuthStatus::Ready;
                    return Ok(PrimaryAuthGrant { grant });
                }
            }
        }
        state.held = true;
        state.primary_status = PrimaryAuthStatus::Issuing;
        let generation = state.grant.as_ref().map_or(1, |g| g.generation + 1);
        if generation > 128 {
            state.primary_status = PrimaryAuthStatus::SecurityHold;
            return Err(failure(PrimaryAuthClass::Budget, PrimaryAuthStage::Grant));
        }
        let outcome = tokio::time::timeout(budget, async {
            let (token, details) = issue().await?;
            let now = Utc::now();
            if now < wall {
                return Err(failure(PrimaryAuthClass::Clock, PrimaryAuthStage::Grant));
            }
            validate(
                token,
                &details,
                &self.account,
                now,
                Instant::now(),
                generation,
            )
            .map_err(|code| {
                failure(
                    if code == "auth_permissions" {
                        PrimaryAuthClass::Authentication
                    } else {
                        PrimaryAuthClass::Validation
                    },
                    PrimaryAuthStage::Grant,
                )
            })
        })
        .await
        .unwrap_or_else(|_| {
            Err(failure(
                PrimaryAuthClass::Unavailable,
                PrimaryAuthStage::Grant,
            ))
        });
        match outcome {
            Ok(grant) => {
                if state
                    .grant
                    .as_ref()
                    .is_some_and(|old| grant.expires_at <= old.expires_at)
                {
                    state.primary_status = PrimaryAuthStatus::SecurityHold;
                    return Err(failure(
                        PrimaryAuthClass::Validation,
                        PrimaryAuthStage::Grant,
                    ));
                }
                state.grant = Some(grant.clone());
                state.held = false;
                state.primary_status = PrimaryAuthStatus::Ready;
                state.last_wall = Utc::now();
                Ok(PrimaryAuthGrant { grant })
            }
            Err(error) => {
                // Only a positively classified COMPLETE unavailable outcome
                // reopens permission to attempt a fresh, fully checked grant.
                // The previous grant is not returned and cannot open commands.
                if error.class == PrimaryAuthClass::Unavailable {
                    state.held = false;
                    state.primary_status = PrimaryAuthStatus::Unavailable;
                } else {
                    state.primary_status = PrimaryAuthStatus::SecurityHold;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests;
