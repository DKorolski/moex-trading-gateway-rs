use super::*;
use crate::stage8b_p1_semantic::observed::Stage8bP1ObservedRecoveryContext;
use crate::stage8b_p1e_first_boot_source::observed::tests::fixture;
use crate::{Stage8bP1ObservedM10Binding, Stage8bP1ObservedPublishedWindow};
use broker_core::observed_m1::ObservedM1Receipt;

fn extend(old: &Stage8bP1ObservedM10Binding) -> Stage8bP1ObservedM10Binding {
    let mut rows = old.source().bars().to_vec();
    for minute in [1, 4, 7] {
        let mut row = rows[0].clone();
        row.open_ts = old.source().end() + Duration::minutes(minute);
        row.close_ts = row.open_ts + Duration::minutes(1);
        rows.push(row);
    }
    let source = ObservedM1Receipt::from_admitted_source(
        "cd".repeat(32),
        old.source().start(),
        old.source().end() + Duration::minutes(10),
        old.source().received_at() + Duration::minutes(10),
        old.source().instrument().clone(),
        rows,
    )
    .unwrap();
    let hash = source.sha256().to_string();
    Stage8bP1ObservedM10Binding::new(old.operational_identity_sha256(), source, &hash).unwrap()
}

#[tokio::test]
async fn observed_supervisor_published_window_is_atomic_and_exact() {
    for case in [
        "valid",
        "missing-first",
        "missing-second",
        "changed-second",
        "extra-field",
        "foreign-identity",
        "changed-initial-overlap",
    ] {
        let redis = RedisServer::start().await;
        let parent = durable_parent();
        let config = || {
            let mut config = supervisor_config(parent.clone());
            let profile = Stage8bP1RuntimeProfileKind::V2;
            config.runtime_profile_id = profile.profile_id().into();
            config.runtime_profile_sha256 = profile.profile_sha256().into();
            config.bootstrap.runtime_config_fingerprint_sha256 =
                profile.build_hybrid_runtime().unwrap().1;
            config.bootstrap.schema_version = 2;
            config.bootstrap.market_data_policy_sha256 =
                Some(strategy_runtime_core::STAGE8B_P1_OBSERVED_SOURCE_POLICY_SHA256.into());
            config
        };
        let hash =
            stage8b_p1e_test_provision_production_redis_v1(config(), [0x19; 16], &redis.url).await;
        let validated = stage8b_p1e_test_validated_production_supervisor_v1(
            config(),
            [0x19; 16],
            &redis.url,
            &hash,
        );
        let (bootstrap, _, plan, _) = validated.into_run_parts();
        let op = bootstrap.operational_identity_sha256();
        let (value, now) = fixture(op);
        let bytes = serde_json::to_vec(&value).unwrap();
        let source = crate::validate_stage8b_p1e_observed_first_boot_source_v4(
            &bytes,
            &sha256_hex(&bytes),
            op,
            "ACC_TEST_0001",
            now,
        )
        .unwrap();
        let receipt = source.observed_receipt().unwrap();
        let initial = Stage8bP1ObservedM10Binding::new(
            op,
            ObservedM1Receipt::restore(receipt.retained_bytes(), receipt.sha256()).unwrap(),
            receipt.sha256(),
        )
        .unwrap();
        let anchor = if case == "foreign-identity" {
            Stage8bP1ObservedM10Binding::new(
                &"ff".repeat(32),
                ObservedM1Receipt::restore(receipt.retained_bytes(), receipt.sha256()).unwrap(),
                receipt.sha256(),
            )
            .unwrap()
        } else if case == "changed-initial-overlap" {
            let mut rows = initial.source().bars().to_vec();
            rows[0].volume += rust_decimal::Decimal::ONE;
            let changed = ObservedM1Receipt::from_admitted_source(
                "ab".repeat(32),
                initial.source().start(),
                initial.source().end(),
                initial.source().received_at(),
                initial.source().instrument().clone(),
                rows,
            )
            .unwrap();
            let hash = changed.sha256().to_string();
            Stage8bP1ObservedM10Binding::new(op, changed, &hash).unwrap()
        } else {
            initial.clone()
        };
        let decision = extend(&anchor);
        let successor = extend(&decision);
        let canonical = |binding: &Stage8bP1ObservedM10Binding| {
            crate::build_stage8b_p1_observed_canonical_m10(
                binding.operational_identity_sha256(),
                binding.source().end().timestamp_millis() - 600_000,
                binding.source(),
            )
            .unwrap()
        };
        let decision_bytes = canonical(&decision);
        let successor_bytes = canonical(&successor);
        let window = Stage8bP1ObservedPublishedWindow::new(
            decision.clone(),
            decision_bytes.clone(),
            successor.clone(),
            successor_bytes.clone(),
        )
        .unwrap();
        // Disk presence grants nothing; these files are not read as fresh authority.
        decision.persist_retained_receipt(&parent).unwrap();
        successor.persist_retained_receipt(&parent).unwrap();
        let disk_before = fs::read_dir(&parent).unwrap().count();
        let mut session = attach_stage8b_p1e_verified_redis_with_source_context(
            &plan,
            Default::default(),
            Some(Stage8bP1ObservedRecoveryContext::new(initial.clone(), &bootstrap).unwrap()),
        )
        .await
        .unwrap();
        assert!(session
            .transport
            .parse_bound_m10(&decision_bytes, op)
            .is_err());
        let mut connection = redis.connection().await;
        for (i, binding) in [&decision, &successor].into_iter().enumerate() {
            if (i == 0 && case == "missing-first") || (i == 1 && case == "missing-second") {
                continue;
            }
            let id = format!("{}-0", binding.source().end().timestamp_millis());
            let bytes = if i == 1 && case == "changed-second" {
                b"{}".to_vec()
            } else {
                canonical(binding)
            };
            let mut cmd = redis::cmd("XADD");
            cmd.arg(&plan.namespace.canonical_m10_stream)
                .arg(&id)
                .arg("payload")
                .arg(&bytes);
            if i == 1 && case == "extra-field" {
                cmd.arg("extra").arg("1");
            }
            let _: String = cmd.query_async(&mut connection).await.unwrap();
        }
        let dump: Vec<u8> = redis::cmd("DUMP")
            .arg(&plan.namespace.canonical_m10_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let result = session
            .admit_observed_published_window(window.clone())
            .await;
        assert_eq!(result.is_ok(), case == "valid", "{case}");
        if case == "valid" {
            for bytes in [&decision_bytes, &successor_bytes] {
                assert!(session.transport.parse_bound_m10(bytes, op).is_ok());
            }
            assert!(session
                .admit_observed_published_window(window.clone())
                .await
                .is_err());
        } else {
            // Especially missing/changed second: the first is not partly admitted.
            assert!(
                session
                    .transport
                    .parse_bound_m10(&decision_bytes, op)
                    .is_err(),
                "{case}"
            );
            assert!(
                session
                    .transport
                    .parse_bound_m10(&successor_bytes, op)
                    .is_err(),
                "{case}"
            );
        }
        assert_eq!(
            redis::cmd("DUMP")
                .arg(&plan.namespace.canonical_m10_stream)
                .query_async::<Vec<u8>>(&mut connection)
                .await
                .unwrap(),
            dump,
            "{case}"
        );
        assert_eq!(
            session.redis_control_mut().pel_count().await.unwrap(),
            0,
            "{case}"
        );
        assert_eq!(
            redis::cmd("XLEN")
                .arg(&plan.namespace.canonical_command_stream)
                .query_async::<u64>(&mut connection)
                .await
                .unwrap(),
            0,
            "{case}"
        );
        assert_eq!(
            fs::read_dir(&parent).unwrap().count(),
            disk_before,
            "{case}"
        );
        // Execute the same pre-S06 helper used by the normal process owner,
        // not just the session API. Failed admission returns no session.
        let context =
            || Stage8bP1ObservedRecoveryContext::new(initial.clone(), &bootstrap).unwrap();
        let latch = crate::Stage8bP1eShutdownLatchV1::new();
        let startup = crate::stage8b_p1e_process::attach_stage8b_p1e_startup_sources_v1(
            &plan,
            Default::default(),
            Some(context()),
            Some(window.clone()),
            &latch,
        )
        .await;
        assert_eq!(startup.is_ok(), case == "valid", "shared startup {case}");
        if let Ok(Some(startup)) = startup {
            assert!(startup
                .transport
                .parse_bound_m10(&decision_bytes, op)
                .is_ok());
            assert!(startup
                .transport
                .parse_bound_m10(&successor_bytes, op)
                .is_ok());
        } else {
            assert_ne!(case, "valid", "clear latch must produce a verified session");
        }

        if case == "valid" {
            use crate::stage8b_p1e_process::attach_stage8b_p1e_startup_sources_v1;
            use crate::{
                Stage8bP1eProcessErrorV1, Stage8bP1eShutdownCauseV1, Stage8bP1eShutdownIntentV1,
            };
            let stop = || {
                Stage8bP1eShutdownIntentV1::new(
                    Stage8bP1eShutdownCauseV1::ExternalSignal,
                    10_000,
                    1,
                )
            };
            let audit = Stage8bP1fRedisCommandAuditHandleV1::default();
            assert!(matches!(
                attach_stage8b_p1e_startup_sources_v1(
                    &plan,
                    audit.clone(),
                    None,
                    Some(window.clone()),
                    &latch,
                )
                .await,
                Err(Stage8bP1eProcessErrorV1::Config)
            ));
            assert!(audit.snapshot().unwrap().is_empty());
            assert!(latch.request(stop()));
            assert!(attach_stage8b_p1e_startup_sources_v1(
                &plan,
                audit.clone(),
                Some(context()),
                Some(window.clone()),
                &latch,
            )
            .await
            .unwrap()
            .is_none());
            assert!(
                audit.snapshot().unwrap().is_empty(),
                "preset stop must do no I/O"
            );

            // Pause only disposable Redis: deterministic in-flight cancellation
            // without a test hook in production or an operational Redis server.
            let _: () = redis::cmd("CLIENT")
                .arg("PAUSE")
                .arg(1_000)
                .arg("ALL")
                .query_async(&mut connection)
                .await
                .unwrap();
            let clear_latch = crate::Stage8bP1eShutdownLatchV1::new();
            let attach = attach_stage8b_p1e_startup_sources_v1(
                &plan,
                audit.clone(),
                Some(context()),
                Some(window),
                &clear_latch,
            );
            tokio::pin!(attach);
            assert!(
                tokio::time::timeout(StdDuration::from_millis(25), &mut attach)
                    .await
                    .is_err()
            );
            assert!(clear_latch.request(stop()));
            assert!(
                tokio::time::timeout(StdDuration::from_millis(250), &mut attach)
                    .await
                    .expect("startup cancellation must not wait for Redis")
                    .unwrap()
                    .is_none()
            );
            let _: () = redis::cmd("CLIENT")
                .arg("UNPAUSE")
                .query_async(&mut connection)
                .await
                .unwrap();
        }
        assert_eq!(
            session.redis_control_mut().pel_count().await.unwrap(),
            0,
            "startup {case}"
        );
        assert_eq!(
            redis::cmd("DUMP")
                .arg(&plan.namespace.canonical_m10_stream)
                .query_async::<Vec<u8>>(&mut connection)
                .await
                .unwrap(),
            dump,
            "startup {case}"
        );
        assert_eq!(
            redis::cmd("XLEN")
                .arg(&plan.namespace.canonical_command_stream)
                .query_async::<u64>(&mut connection)
                .await
                .unwrap(),
            0,
            "startup {case}"
        );
        assert_eq!(
            fs::read_dir(&parent).unwrap().count(),
            disk_before,
            "startup {case}"
        );
        fs::remove_dir_all(&parent).unwrap();
    }
}
