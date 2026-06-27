use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerKind {
    Alor,
    Finam,
    Sim,
    Other(String),
}
