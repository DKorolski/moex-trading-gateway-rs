//! Explicit observed-policy collection using the same five closed GET routes.
//! The retained raw snapshot accompanies the V4 source; neither is a live permit.
use super::super::{
    CollectionProgress, Stage8bP1fO2MaterializedSourceV1, Stage8bP1fO2MaterializerErrorV1,
};
use super::*;
use broker_finam::{
    AccessToken, AccountOrdersResponse, AccountResponse, AssetParamsResponse,
    AssetScheduleResponse, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteV1,
};

pub struct Stage8bP1fObservedCollectedSource {
    pub source: Stage8bP1fO2MaterializedSourceV1,
    pub snapshot: ClosedM1SnapshotEvidenceV1,
}

impl Stage8bP1fObservedM10Plan {
    pub async fn collect_first_boot_source(
        &self,
        account_id: &str,
        token: &AccessToken,
        template_bytes: &[u8],
    ) -> Result<Stage8bP1fObservedCollectedSource, Stage8bP1fO2MaterializerErrorV1> {
        let mut progress = CollectionProgress::default();
        let result = async {
            // Everything derivable without transport is checked before any GET.
            let current = Self::from_calendar_template(
                template_bytes,
                self.operational_identity_sha256(),
                Utc::now(),
            )
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
            if current.template_sha256() != self.template_sha256()
                || current.calendar_sha256() != self.calendar_sha256()
            {
                return Err(Stage8bP1fO2MaterializerErrorV1::Template);
            }
            self.request_plan()
                .preflight(Utc::now())
                .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
            let get_error = |_| Stage8bP1fO2MaterializerErrorV1::Get;
            let client = Stage8bP1fO2GetOnlyClientV1::new(account_id).map_err(get_error)?;
            let (account, a) = client
                .fetch_typed::<AccountResponse>(token, Stage8bP1fO2GetRouteV1::Account)
                .await
                .map_err(get_error)?;
            progress.gets += 1;
            progress.last_collected = Some("account_collected");
            let (orders, o) = client
                .fetch_typed::<AccountOrdersResponse>(token, Stage8bP1fO2GetRouteV1::AccountOrders)
                .await
                .map_err(get_error)?;
            progress.gets += 1;
            progress.last_collected = Some("orders_collected");
            let (params, p) = client
                .fetch_typed::<AssetParamsResponse>(token, Stage8bP1fO2GetRouteV1::AssetParams)
                .await
                .map_err(get_error)?;
            progress.gets += 1;
            progress.last_collected = Some("params_collected");
            let (schedule, s) = client
                .fetch_typed::<AssetScheduleResponse>(token, Stage8bP1fO2GetRouteV1::AssetSchedule)
                .await
                .map_err(get_error)?;
            progress.gets += 1;
            progress.last_collected = Some("schedule_collected");
            progress.chunk = Some((
                0,
                self.request_start().timestamp(),
                self.request_end().timestamp(),
            ));
            let snapshot = client
                .fetch_closed_m1_snapshot(token, self.request_plan())
                .await
                .map_err(get_error)?;
            progress.gets += snapshot.parts.len();
            progress.chunks = snapshot.parts.len();
            progress.last_collected = Some("collection_complete");
            let mut observations = vec![a, o, p, s];
            observations.extend(
                client
                    .observations_for_closed_m1_snapshot(&snapshot)
                    .map_err(get_error)?,
            );
            let now = Utc::now();
            let materialized = self
                .materialize(snapshot, now)
                .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
            let source = materialized.first_boot_source_v4(
                template_bytes,
                account_id,
                account,
                orders,
                params,
                schedule,
                observations,
                now,
            )?;
            Ok(Stage8bP1fObservedCollectedSource {
                source,
                snapshot: materialized.snapshot().evidence().clone(),
            })
        }
        .await;
        result.map_err(|error| progress.error(error))
    }
}
