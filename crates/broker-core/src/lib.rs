//! Broker-neutral contracts shared by MOEX broker adapters and strategy bridges.
//!
//! This crate deliberately contains no Finam-, T-Bank-, or Alor-specific API
//! shapes. Adapters translate broker payloads into these contracts.

pub mod account;
pub mod command;
pub mod event;
pub mod instrument;
pub mod order;
pub mod readiness;
pub mod subscription;
pub mod time;

pub use account::{AccountId, PortfolioSnapshot, Position};
pub use command::{BrokerCommand, CancelOrder, CommandAck, PlaceOrder, PlaceSltpOrder};
pub use event::{BrokerEvent, MarketDataEvent};
pub use instrument::{Exchange, Instrument, InstrumentId, Market, Money, Price, Quantity};
pub use order::{
    ClientOrderId, Order, OrderId, OrderSide, OrderStatus, OrderType, StopKind, TimeInForce, Trade,
};
pub use readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
pub use subscription::{SubscriptionIntent, SubscriptionKind, SubscriptionState};
