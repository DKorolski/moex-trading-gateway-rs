use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::broker::BrokerKind;

pub const OBSERVABILITY_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BrokerObservabilityChannelKind {
    GatewayHealth,
    GatewayReadiness,
    BrokerTruth,
    MarketData,
    CommandAckLifecycle,
    RuntimeState,
    OpsConsumerGroups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerObservabilityOwner {
    Gateway,
    Runtime,
    OpsCollector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerObservabilityContourRole {
    ActiveOracle,
    ShadowCandidate,
    CandidateAfterCutover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerObservabilityChannel {
    pub kind: BrokerObservabilityChannelKind,
    pub canonical_stream: String,
    pub raw_sources: Vec<String>,
    pub owner: BrokerObservabilityOwner,
    pub required_for_shadow_runtime: bool,
    pub required_for_live_runtime: bool,
    pub notes: String,
}

impl BrokerObservabilityChannel {
    pub fn new(
        kind: BrokerObservabilityChannelKind,
        canonical_stream: impl Into<String>,
        raw_sources: impl IntoIterator<Item = impl Into<String>>,
        owner: BrokerObservabilityOwner,
    ) -> Self {
        Self {
            kind,
            canonical_stream: canonical_stream.into(),
            raw_sources: raw_sources.into_iter().map(Into::into).collect(),
            owner,
            required_for_shadow_runtime: true,
            required_for_live_runtime: true,
            notes: String::new(),
        }
    }

    pub fn shadow_optional(mut self) -> Self {
        self.required_for_shadow_runtime = false;
        self
    }

    pub fn live_optional(mut self) -> Self {
        self.required_for_live_runtime = false;
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = notes.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerObservabilityContract {
    pub schema_version: u16,
    pub contour_label: String,
    pub role: BrokerObservabilityContourRole,
    pub broker: BrokerKind,
    pub channels: Vec<BrokerObservabilityChannel>,
    pub live_order_authorized: bool,
    pub command_consumer_to_real_broker_enabled: bool,
    pub continuous_runtime_live_enabled: bool,
    pub ten_minute_bar_parity_proven: bool,
    pub broker_truth_parity_proven: bool,
    pub runtime_decision_parity_proven: bool,
}

impl BrokerObservabilityContract {
    pub fn new(
        contour_label: impl Into<String>,
        role: BrokerObservabilityContourRole,
        broker: BrokerKind,
        channels: Vec<BrokerObservabilityChannel>,
    ) -> Self {
        Self {
            schema_version: OBSERVABILITY_SCHEMA_VERSION,
            contour_label: contour_label.into(),
            role,
            broker,
            channels,
            live_order_authorized: false,
            command_consumer_to_real_broker_enabled: false,
            continuous_runtime_live_enabled: false,
            ten_minute_bar_parity_proven: false,
            broker_truth_parity_proven: false,
            runtime_decision_parity_proven: false,
        }
    }

    pub fn channel_kinds(&self) -> BTreeSet<BrokerObservabilityChannelKind> {
        self.channels.iter().map(|channel| channel.kind).collect()
    }

    pub fn validate_for_shadow_runtime(&self) -> BrokerObservabilityReadinessReport {
        let mut blockers =
            self.required_channel_blockers(|channel| channel.required_for_shadow_runtime);
        blockers.extend(self.ownership_blockers());
        blockers.extend(self.pre_cutover_safety_blockers());

        BrokerObservabilityReadinessReport {
            contour_label: self.contour_label.clone(),
            shadow_runtime_observability_ready: blockers.is_empty(),
            continuous_live_runtime_preconditions_satisfied: false,
            continuous_runtime_live_enabled: self.continuous_runtime_live_enabled,
            command_consumer_to_real_broker_enabled: self.command_consumer_to_real_broker_enabled,
            live_order_authorized: self.live_order_authorized,
            blockers,
        }
    }

    pub fn validate_for_continuous_live_runtime(&self) -> BrokerObservabilityReadinessReport {
        let mut blockers =
            self.required_channel_blockers(|channel| channel.required_for_live_runtime);
        blockers.extend(self.ownership_blockers());
        blockers.extend(self.pre_cutover_safety_blockers());

        if !self.ten_minute_bar_parity_proven {
            blockers.push(BrokerObservabilityBlocker::TenMinuteBarParityMissing);
        }
        if !self.broker_truth_parity_proven {
            blockers.push(BrokerObservabilityBlocker::BrokerTruthParityMissing);
        }
        if !self.runtime_decision_parity_proven {
            blockers.push(BrokerObservabilityBlocker::RuntimeDecisionParityMissing);
        }

        let preconditions_satisfied = blockers.is_empty();
        BrokerObservabilityReadinessReport {
            contour_label: self.contour_label.clone(),
            shadow_runtime_observability_ready: self
                .validate_for_shadow_runtime()
                .blockers
                .is_empty(),
            continuous_live_runtime_preconditions_satisfied: preconditions_satisfied,
            continuous_runtime_live_enabled: self.continuous_runtime_live_enabled,
            command_consumer_to_real_broker_enabled: self.command_consumer_to_real_broker_enabled,
            live_order_authorized: self.live_order_authorized,
            blockers,
        }
    }

    fn required_channel_blockers(
        &self,
        required: impl Fn(&BrokerObservabilityChannel) -> bool,
    ) -> Vec<BrokerObservabilityBlocker> {
        self.channels
            .iter()
            .filter(|channel| required(channel))
            .filter(|channel| channel.raw_sources.is_empty() || channel.canonical_stream.is_empty())
            .map(
                |channel| BrokerObservabilityBlocker::MissingRawOrCanonicalSource {
                    channel: channel.kind,
                },
            )
            .chain(
                required_channel_kinds_for(&required)
                    .into_iter()
                    .filter(|kind| {
                        !self
                            .channels
                            .iter()
                            .any(|channel| channel.kind == *kind && required(channel))
                    })
                    .map(BrokerObservabilityBlocker::MissingChannel),
            )
            .collect()
    }

    fn ownership_blockers(&self) -> Vec<BrokerObservabilityBlocker> {
        self.channels
            .iter()
            .filter(|channel| channel.kind == BrokerObservabilityChannelKind::RuntimeState)
            .filter(|channel| channel.owner != BrokerObservabilityOwner::Runtime)
            .map(|_| BrokerObservabilityBlocker::RuntimeStateOwnedByGateway)
            .collect()
    }

    fn pre_cutover_safety_blockers(&self) -> Vec<BrokerObservabilityBlocker> {
        let mut blockers = Vec::new();
        if self.live_order_authorized {
            blockers.push(BrokerObservabilityBlocker::LiveOrderAuthorizedInParityContract);
        }
        if self.command_consumer_to_real_broker_enabled {
            blockers.push(BrokerObservabilityBlocker::CommandConsumerToRealBrokerEnabled);
        }
        if self.continuous_runtime_live_enabled {
            blockers.push(BrokerObservabilityBlocker::ContinuousRuntimeLiveEnabled);
        }
        blockers
    }
}

fn required_channel_kinds_for(
    required: &impl Fn(&BrokerObservabilityChannel) -> bool,
) -> BTreeSet<BrokerObservabilityChannelKind> {
    let mut probe_channels = vec![
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::GatewayHealth,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Gateway,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::GatewayReadiness,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Gateway,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::BrokerTruth,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Gateway,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::MarketData,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Gateway,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::RuntimeState,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Runtime,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::OpsConsumerGroups,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::OpsCollector,
        ),
        BrokerObservabilityChannel::new(
            BrokerObservabilityChannelKind::CommandAckLifecycle,
            "probe",
            ["probe"],
            BrokerObservabilityOwner::Gateway,
        )
        .shadow_optional(),
    ];
    probe_channels
        .drain(..)
        .filter(|channel| required(channel))
        .map(|channel| channel.kind)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerObservabilityReadinessReport {
    pub contour_label: String,
    pub shadow_runtime_observability_ready: bool,
    pub continuous_live_runtime_preconditions_satisfied: bool,
    pub continuous_runtime_live_enabled: bool,
    pub command_consumer_to_real_broker_enabled: bool,
    pub live_order_authorized: bool,
    pub blockers: Vec<BrokerObservabilityBlocker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerObservabilityBlocker {
    MissingChannel(BrokerObservabilityChannelKind),
    MissingRawOrCanonicalSource {
        channel: BrokerObservabilityChannelKind,
    },
    RuntimeStateOwnedByGateway,
    LiveOrderAuthorizedInParityContract,
    CommandConsumerToRealBrokerEnabled,
    ContinuousRuntimeLiveEnabled,
    TenMinuteBarParityMissing,
    BrokerTruthParityMissing,
    RuntimeDecisionParityMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerConsumerGroupSnapshot {
    pub stream: String,
    pub group: String,
    pub pending: u64,
    pub lag: Option<u64>,
    pub entries_read: Option<u64>,
}

impl BrokerConsumerGroupSnapshot {
    pub fn is_clean(&self, max_lag: u64) -> bool {
        self.pending == 0 && self.lag.unwrap_or(0) <= max_lag
    }
}

pub fn required_channel_kinds() -> Vec<BrokerObservabilityChannelKind> {
    vec![
        BrokerObservabilityChannelKind::GatewayHealth,
        BrokerObservabilityChannelKind::GatewayReadiness,
        BrokerObservabilityChannelKind::BrokerTruth,
        BrokerObservabilityChannelKind::MarketData,
        BrokerObservabilityChannelKind::RuntimeState,
        BrokerObservabilityChannelKind::OpsConsumerGroups,
    ]
}

pub fn live_required_channel_kinds() -> Vec<BrokerObservabilityChannelKind> {
    let mut kinds = required_channel_kinds();
    kinds.push(BrokerObservabilityChannelKind::CommandAckLifecycle);
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(
        kind: BrokerObservabilityChannelKind,
        canonical: &str,
        raw: &[&str],
        owner: BrokerObservabilityOwner,
    ) -> BrokerObservabilityChannel {
        BrokerObservabilityChannel::new(kind, canonical, raw.iter().copied(), owner)
    }

    fn alor_contract() -> BrokerObservabilityContract {
        BrokerObservabilityContract::new(
            "alor-imoexf-oracle",
            BrokerObservabilityContourRole::ActiveOracle,
            BrokerKind::Alor,
            vec![
                channel(
                    BrokerObservabilityChannelKind::GatewayHealth,
                    "canonical.broker.health.alor.imoexf",
                    &["events.health"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::GatewayReadiness,
                    "canonical.broker.readiness.alor.imoexf",
                    &["events.health", "GET /readiness"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::BrokerTruth,
                    "canonical.broker.truth.alor.imoexf",
                    &[
                        "broker.orders.PORTFOLIO_TEST",
                        "broker.trades.PORTFOLIO_TEST",
                        "broker.positions.PORTFOLIO_TEST",
                        "broker.snapshots.PORTFOLIO_TEST",
                    ],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::MarketData,
                    "canonical.broker.market_data.alor.imoexf.10m",
                    &["md.bars.PORTFOLIO_TEST.10m"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::CommandAckLifecycle,
                    "canonical.broker.command_acks.alor.imoexf",
                    &["cmd.acks.PORTFOLIO_TEST"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::RuntimeState,
                    "canonical.runtime.state.hybrid_intraday.imoexf",
                    &["runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.PORTFOLIO_TEST"],
                    BrokerObservabilityOwner::Runtime,
                ),
                channel(
                    BrokerObservabilityChannelKind::OpsConsumerGroups,
                    "canonical.ops.consumer_groups.alor.imoexf",
                    &[
                        "XINFO GROUPS md.bars.PORTFOLIO_TEST.10m",
                        "XINFO GROUPS cmd.orders.PORTFOLIO_TEST",
                    ],
                    BrokerObservabilityOwner::OpsCollector,
                ),
            ],
        )
    }

    fn finam_contract() -> BrokerObservabilityContract {
        BrokerObservabilityContract::new(
            "finam-imoexf-shadow",
            BrokerObservabilityContourRole::ShadowCandidate,
            BrokerKind::Finam,
            vec![
                channel(
                    BrokerObservabilityChannelKind::GatewayHealth,
                    "canonical.broker.health.finam.imoexf",
                    &["finam_shadow:health", "finam_ws_shadow:health"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::GatewayReadiness,
                    "canonical.broker.readiness.finam.imoexf",
                    &["finam_shadow:readiness", "finam_ws_shadow:readiness"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::BrokerTruth,
                    "canonical.broker.truth.finam.imoexf",
                    &[
                        "finam_shadow:portfolio:snapshot",
                        "finam_shadow:orders:snapshot",
                    ],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::MarketData,
                    "canonical.broker.market_data.finam.imoexf.10m",
                    &["finam_ws_shadow:market_data", "derived:M1_TO_10M"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::CommandAckLifecycle,
                    "canonical.broker.command_acks.finam.imoexf",
                    &["finam_ws_shadow:command_acks_disabled"],
                    BrokerObservabilityOwner::Gateway,
                ),
                channel(
                    BrokerObservabilityChannelKind::RuntimeState,
                    "canonical.runtime.state.hybrid_intraday.imoexf",
                    &["runtime.state.hybrid_intraday.shadow.finam.imoexf"],
                    BrokerObservabilityOwner::Runtime,
                ),
                channel(
                    BrokerObservabilityChannelKind::OpsConsumerGroups,
                    "canonical.ops.consumer_groups.finam.imoexf",
                    &["XINFO STREAM finam_ws_shadow:market_data"],
                    BrokerObservabilityOwner::OpsCollector,
                ),
            ],
        )
    }

    #[test]
    fn alor_and_finam_raw_outputs_map_to_same_canonical_observability_kinds() {
        let alor = alor_contract();
        let finam = finam_contract();

        assert_eq!(alor.channel_kinds(), finam.channel_kinds());
        assert!(alor
            .channel_kinds()
            .contains(&BrokerObservabilityChannelKind::RuntimeState));
        assert!(alor
            .channels
            .iter()
            .any(|channel| channel.raw_sources.contains(&"events.health".to_string())));
        assert!(finam.channels.iter().any(|channel| channel
            .raw_sources
            .contains(&"finam_ws_shadow:health".to_string())));
    }

    #[test]
    fn runtime_state_is_strategy_owned_not_gateway_owned() {
        let report = alor_contract().validate_for_shadow_runtime();

        assert!(report.shadow_runtime_observability_ready);
        assert!(!report.live_order_authorized);
        assert!(!report.command_consumer_to_real_broker_enabled);
        assert!(!report.continuous_runtime_live_enabled);
    }

    #[test]
    fn live_runtime_stays_blocked_until_parity_evidence_is_proven() {
        let mut finam = finam_contract();

        let report = finam.validate_for_continuous_live_runtime();

        assert!(!report.continuous_live_runtime_preconditions_satisfied);
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::TenMinuteBarParityMissing));
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::BrokerTruthParityMissing));
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::RuntimeDecisionParityMissing));

        finam.ten_minute_bar_parity_proven = true;
        finam.broker_truth_parity_proven = true;
        finam.runtime_decision_parity_proven = true;
        let ready = finam.validate_for_continuous_live_runtime();
        assert!(ready.continuous_live_runtime_preconditions_satisfied);
        assert!(!ready.continuous_runtime_live_enabled);
        assert!(!ready.live_order_authorized);
    }

    #[test]
    fn live_runtime_contract_rejects_early_enabled_order_surface() {
        let mut finam = finam_contract();
        finam.live_order_authorized = true;
        finam.command_consumer_to_real_broker_enabled = true;
        finam.continuous_runtime_live_enabled = true;

        let report = finam.validate_for_shadow_runtime();

        assert!(!report.shadow_runtime_observability_ready);
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::LiveOrderAuthorizedInParityContract));
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::CommandConsumerToRealBrokerEnabled));
        assert!(report
            .blockers
            .contains(&BrokerObservabilityBlocker::ContinuousRuntimeLiveEnabled));
    }

    #[test]
    fn consumer_group_snapshot_requires_clean_pending_and_lag() {
        let clean = BrokerConsumerGroupSnapshot {
            stream: "md.bars.PORTFOLIO_TEST.10m".to_string(),
            group: "strategy-runtime-hybrid-riskgate-shadow-PORTFOLIO_TEST".to_string(),
            pending: 0,
            lag: Some(0),
            entries_read: Some(42),
        };
        assert!(clean.is_clean(0));

        let lagged = BrokerConsumerGroupSnapshot {
            lag: Some(1),
            ..clean.clone()
        };
        assert!(!lagged.is_clean(0));

        let pending = BrokerConsumerGroupSnapshot {
            pending: 1,
            ..clean
        };
        assert!(!pending.is_clean(10));
    }
}
