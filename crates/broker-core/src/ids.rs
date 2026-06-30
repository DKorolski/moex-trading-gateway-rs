use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CLIENT_ORDER_ID_MAX_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyRequestId(pub Uuid);

impl StrategyRequestId {
    pub fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl From<Uuid> for StrategyRequestId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl fmt::Display for StrategyRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

string_id!(BrokerAccountId);
string_id!(BrokerOrderId);
string_id!(BrokerTradeId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClientOrderId(String);

impl ClientOrderId {
    pub fn new(value: impl Into<String>) -> Result<Self, ClientOrderIdError> {
        let value = value.into();
        validate_client_order_id(&value)?;
        Ok(Self(value))
    }

    pub fn from_strategy_request(request_id: StrategyRequestId) -> Self {
        Self(encode_uuid_prefix_base32(request_id.0.as_bytes()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ClientOrderId {
    type Error = ClientOrderIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ClientOrderId> for String {
    fn from(value: ClientOrderId) -> Self {
        value.0
    }
}

impl fmt::Display for ClientOrderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientOrderIdError {
    #[error("client_order_id cannot be empty")]
    Empty,
    #[error("client_order_id exceeds {max} characters: got {actual}")]
    TooLong { max: usize, actual: usize },
    #[error("client_order_id contains unsupported character: {0:?}")]
    UnsupportedCharacter(char),
}

fn validate_client_order_id(value: &str) -> Result<(), ClientOrderIdError> {
    if value.is_empty() {
        return Err(ClientOrderIdError::Empty);
    }
    let len = value.chars().count();
    if len > CLIENT_ORDER_ID_MAX_LEN {
        return Err(ClientOrderIdError::TooLong {
            max: CLIENT_ORDER_ID_MAX_LEN,
            actual: len,
        });
    }
    if let Some(ch) = value
        .chars()
        .find(|ch| !ch.is_ascii_alphanumeric() && *ch != '-' && *ch != '_')
    {
        return Err(ClientOrderIdError::UnsupportedCharacter(ch));
    }
    Ok(())
}

fn encode_uuid_prefix_base32(bytes: &[u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    let mut output = String::with_capacity(CLIENT_ORDER_ID_MAX_LEN);
    let mut buffer: u16 = 0;
    let mut bits: u8 = 0;

    for byte in &bytes[..12] {
        buffer = (buffer << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((buffer >> bits) & 0b1_1111) as usize;
            output.push(ALPHABET[index] as char);
        }
    }

    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0b1_1111) as usize;
        output.push(ALPHABET[index] as char);
    }

    debug_assert_eq!(output.len(), CLIENT_ORDER_ID_MAX_LEN);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_order_id_accepts_safe_20_char_value() {
        let id = ClientOrderId::new("0123456789ABCDEFGHJK").expect("valid id");
        assert_eq!(id.as_str(), "0123456789ABCDEFGHJK");
    }

    #[test]
    fn client_order_id_rejects_uuid_length() {
        let error =
            ClientOrderId::new("ACC_TEST_0001_TOO_LONG").expect_err("value must be too long");
        assert!(matches!(error, ClientOrderIdError::TooLong { .. }));
    }

    #[test]
    fn derived_client_order_id_is_deterministic_and_finam_safe() {
        let uuid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("uuid");
        let request_id = StrategyRequestId::from(uuid);

        let first = ClientOrderId::from_strategy_request(request_id);
        let second = ClientOrderId::from_strategy_request(request_id);

        assert_eq!(first, second);
        assert_eq!(first.as_str().len(), CLIENT_ORDER_ID_MAX_LEN);
        validate_client_order_id(first.as_str()).expect("derived id is valid");
    }
}
