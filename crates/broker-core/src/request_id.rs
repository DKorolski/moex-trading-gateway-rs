use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::AccountId;
use crate::ids::StrategyRequestId;
use crate::instrument::InstrumentId;
use crate::order::OrderSide;

pub const DETERMINISTIC_REQUEST_ID_NAMESPACE: &str =
    "strategy_id|legacy_account_alias|legacy_symbol|action|bar_ts|seq";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeterministicRequestIdInput<'a> {
    pub strategy_id: &'a str,
    pub account_alias: &'a str,
    pub symbol: &'a str,
    pub action: &'a str,
    pub bar_ts: i64,
    pub seq: u8,
}

impl<'a> DeterministicRequestIdInput<'a> {
    pub fn legacy(
        strategy_id: &'a str,
        account_alias: &'a str,
        symbol: &'a str,
        action: &'a str,
        bar_ts: i64,
        seq: u8,
    ) -> Self {
        Self {
            strategy_id,
            account_alias,
            symbol,
            action,
            bar_ts,
            seq,
        }
    }

    pub fn account_instrument(
        strategy_id: &'a str,
        account_id: &'a AccountId,
        instrument: &'a InstrumentId,
        action: &'a str,
        bar_ts: i64,
        seq: u8,
    ) -> Self {
        Self {
            strategy_id,
            account_alias: account_id.as_str(),
            symbol: instrument.symbol.as_str(),
            action,
            bar_ts,
            seq,
        }
    }

    pub fn namespace_name(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.strategy_id, self.account_alias, self.symbol, self.action, self.bar_ts, self.seq
        )
    }
}

pub fn deterministic_request_id(input: DeterministicRequestIdInput<'_>) -> StrategyRequestId {
    StrategyRequestId::from(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        input.namespace_name().as_bytes(),
    ))
}

pub fn deterministic_request_id_from_legacy_parts(
    strategy_id: &str,
    portfolio: &str,
    symbol: &str,
    action: &str,
    bar_ts: i64,
    seq: u8,
) -> StrategyRequestId {
    deterministic_request_id(DeterministicRequestIdInput::legacy(
        strategy_id,
        portfolio,
        symbol,
        action,
        bar_ts,
        seq,
    ))
}

pub fn deterministic_request_id_for_account_instrument(
    strategy_id: &str,
    account_id: &AccountId,
    instrument: &InstrumentId,
    action: &str,
    bar_ts: i64,
    seq: u8,
) -> StrategyRequestId {
    deterministic_request_id(DeterministicRequestIdInput::account_instrument(
        strategy_id,
        account_id,
        instrument,
        action,
        bar_ts,
        seq,
    ))
}

pub fn market_request_seq(side: OrderSide) -> u8 {
    match side {
        OrderSide::Buy => 3,
        OrderSide::Sell => 4,
    }
}

pub fn deterministic_market_request_id_for_account_instrument(
    strategy_id: &str,
    account_id: &AccountId,
    instrument: &InstrumentId,
    created_ts_utc: i64,
    side: OrderSide,
) -> StrategyRequestId {
    deterministic_request_id_for_account_instrument(
        strategy_id,
        account_id,
        instrument,
        "market",
        created_ts_utc,
        market_request_seq(side),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::command::{CommandAck, CommandAckStatus};
    use crate::ids::{BrokerOrderId, ClientOrderId};
    use crate::instrument::{Exchange, Market};
    use crate::runtime_state::{RuntimeAckPendingDisposition, RuntimePendingRequestIdentity};

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_ALIAS")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn ack(
        request_id: StrategyRequestId,
        client_order_id: Option<ClientOrderId>,
        broker_order_id: Option<BrokerOrderId>,
    ) -> CommandAck {
        CommandAck {
            request_id,
            client_order_id,
            broker_order_id,
            status: CommandAckStatus::Submitted,
            reason: None,
            received_ts: Utc
                .with_ymd_and_hms(2026, 7, 8, 9, 10, 0)
                .single()
                .expect("ts"),
        }
    }

    #[test]
    fn deterministic_request_id_is_stable_after_account_alias_migration() {
        let legacy = deterministic_request_id_from_legacy_parts(
            "hybrid_intraday",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "market",
            1_783_410_600,
            3,
        );
        let migrated = deterministic_request_id_for_account_instrument(
            "hybrid_intraday",
            &account(),
            &instrument(),
            "market",
            1_783_410_600,
            3,
        );

        assert_eq!(legacy, migrated);
    }

    #[test]
    fn legacy_portfolio_string_and_account_id_alias_produce_same_request_id() {
        let account = account();

        assert_eq!(
            deterministic_request_id_from_legacy_parts(
                "strat",
                "ACC_TEST_ALIAS",
                "IMOEXF",
                "place",
                123,
                0,
            ),
            deterministic_request_id_for_account_instrument(
                "strat",
                &account,
                &instrument(),
                "place",
                123,
                0,
            )
        );
        assert_eq!(account.as_str(), "ACC_TEST_ALIAS");
    }

    #[test]
    fn legacy_symbol_string_and_instrument_alias_produce_same_request_id() {
        let instrument = instrument();
        let legacy = deterministic_request_id_from_legacy_parts(
            "strat",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "cancel",
            456,
            1,
        );
        let migrated = deterministic_request_id_for_account_instrument(
            "strat",
            &account(),
            &instrument,
            "cancel",
            456,
            1,
        );

        assert_eq!(legacy, migrated);
        assert_eq!(instrument.venue_symbol.as_deref(), Some("IMOEXF@RTSX"));
    }

    #[test]
    fn action_bar_ts_seq_namespace_unchanged() {
        let input = DeterministicRequestIdInput::legacy(
            "hybrid",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "place",
            789,
            0,
        );
        let expected = StrategyRequestId::from(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            b"hybrid|ACC_TEST_ALIAS|IMOEXF|place|789|0",
        ));

        assert_eq!(deterministic_request_id(input), expected);
        assert_ne!(
            deterministic_request_id_from_legacy_parts(
                "hybrid",
                "ACC_TEST_ALIAS",
                "IMOEXF",
                "cancel",
                789,
                0,
            ),
            expected
        );
        assert_ne!(
            deterministic_request_id_from_legacy_parts(
                "hybrid",
                "ACC_TEST_ALIAS",
                "IMOEXF",
                "place",
                790,
                0,
            ),
            expected
        );
        assert_ne!(
            deterministic_request_id_from_legacy_parts(
                "hybrid",
                "ACC_TEST_ALIAS",
                "IMOEXF",
                "place",
                789,
                1,
            ),
            expected
        );
    }

    #[test]
    fn old_pending_request_id_still_matches_new_ack_path() {
        let request_id = deterministic_request_id_from_legacy_parts(
            "hybrid_intraday",
            "ACC_TEST_ALIAS",
            "IMOEXF",
            "market",
            1_783_410_600,
            3,
        );
        let pending = RuntimePendingRequestIdentity {
            request_id,
            client_order_id: Some(ClientOrderId::new("CID000000000000090").expect("cid")),
            broker_order_id: Some(BrokerOrderId::new("FINAM-ORDER-090")),
        };
        let ack = ack(
            deterministic_market_request_id_for_account_instrument(
                "hybrid_intraday",
                &account(),
                &instrument(),
                1_783_410_600,
                OrderSide::Buy,
            ),
            Some(ClientOrderId::new("CID000000000000090").expect("cid")),
            Some(BrokerOrderId::new("FINAM-ORDER-090")),
        );

        let decision = pending.evaluate_ack(&ack);

        assert!(decision.request_id_matches);
        assert_eq!(
            decision.pending_disposition,
            RuntimeAckPendingDisposition::ClearPending
        );
    }

    #[test]
    fn client_order_id_does_not_affect_strategy_request_id() {
        let request_id = deterministic_request_id_for_account_instrument(
            "hybrid_intraday",
            &account(),
            &instrument(),
            "market",
            1_783_410_600,
            3,
        );
        let left_client = ClientOrderId::new("CID000000000000091").expect("cid");
        let right_client = ClientOrderId::new("CID000000000000092").expect("cid");

        assert_ne!(left_client, right_client);
        assert_eq!(
            request_id,
            deterministic_request_id_for_account_instrument(
                "hybrid_intraday",
                &account(),
                &instrument(),
                "market",
                1_783_410_600,
                3,
            )
        );
    }

    #[test]
    fn broker_order_id_does_not_affect_strategy_request_id() {
        let request_id = deterministic_request_id_for_account_instrument(
            "hybrid_intraday",
            &account(),
            &instrument(),
            "cancel",
            1_783_410_600,
            1,
        );
        let left_order = BrokerOrderId::new("FINAM-ORDER-A");
        let right_order = BrokerOrderId::new("FINAM-ORDER-B");

        assert_ne!(left_order, right_order);
        assert_eq!(
            request_id,
            deterministic_request_id_for_account_instrument(
                "hybrid_intraday",
                &account(),
                &instrument(),
                "cancel",
                1_783_410_600,
                1,
            )
        );
    }
}
