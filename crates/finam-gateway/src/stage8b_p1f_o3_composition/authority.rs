//! The guardian store deliberately uses an exclusive, nonblocking inspection
//! lock. Signer and initializer are separate readers of that same store. A
//! short collision is not a new phase or an authority failure; no action is
//! authorized until a NEW successful inspection observes the exact Active head.
use super::*;
use durable::{
    Stage8bP1fAuthorityErrorV1 as Error, Stage8bP1fAuthorityStoreV1, Stage8bP1fPhaseStateV1,
};

pub(crate) async fn require_active(
    store: &Stage8bP1fAuthorityStoreV1,
    manifest: &str,
    deadline: DateTime<Utc>,
) -> Result<(), O3Error> {
    now_before(deadline)?;
    wait_for_inspection(|| {
        let head = store.inspect()?;
        Ok(head.state == Stage8bP1fPhaseStateV1::Active
            && head.active_manifest_sha256.as_deref() == Some(manifest))
    })
    .await?;
    now_before(deadline)?;
    Ok(())
}

async fn wait_for_inspection(
    mut inspect: impl FnMut() -> Result<bool, Error>,
) -> Result<(), O3Error> {
    // This bound does not extend the phase or retain cached Active authority.
    // It is much shorter than the existing 30-second stop grace.
    tokio::time::timeout(StdDuration::from_millis(250), async {
        loop {
            match inspect() {
                Ok(true) => return Ok(()),
                Err(Error::ConcurrentGuardian) => {
                    tokio::time::sleep(StdDuration::from_millis(10)).await;
                }
                Ok(false) | Err(_) => return Err(O3Error::Input),
            }
        }
    })
    .await
    .map_err(|_| O3Error::Input)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn o3_authority_wait_requires_fresh_active_and_retries_only_lock_contention() {
        let mut calls = 0;
        wait_for_inspection(|| {
            calls += 1;
            if calls < 3 {
                Err(Error::ConcurrentGuardian)
            } else {
                Ok(true)
            }
        })
        .await
        .unwrap();
        assert_eq!(calls, 3);

        let mut calls = 0;
        assert!(wait_for_inspection(|| {
            calls += 1;
            if calls == 1 {
                Err(Error::ConcurrentGuardian)
            } else {
                Ok(false)
            }
        })
        .await
        .is_err());
        assert_eq!(calls, 2); // A terminal/foreign head is not retried.

        let mut calls = 0;
        assert!(wait_for_inspection(|| {
            calls += 1;
            Err(Error::InvalidCustody)
        })
        .await
        .is_err());
        assert_eq!(calls, 1);

        let start = std::time::Instant::now();
        assert!(wait_for_inspection(|| Err(Error::ConcurrentGuardian))
            .await
            .is_err());
        assert!(start.elapsed() < StdDuration::from_secs(2));
    }
}
