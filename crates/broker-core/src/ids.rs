use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub const CLIENT_ORDER_ID_MAX_LEN: usize = 20;
pub const RUNTIME_STATE_SCHEMA_VERSION_V2: u16 = 2;
pub const BROKER_ORDER_ID_ENCODING: &str = "broker_order_id_string";
pub const LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT: &str = "decimal_string";

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
string_id!(BrokerTradeId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BrokerOrderId(String);

impl BrokerOrderId {
    #[track_caller]
    pub fn new(value: impl Into<String>) -> Self {
        Self::from_broker_native_exact(value)
            .expect("BrokerOrderId::new requires a non-empty broker-native id")
    }

    pub fn from_broker_native_exact(
        value: impl Into<String>,
    ) -> Result<Self, BrokerOrderIdImportError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BrokerOrderIdImportError::EmptyBrokerNativeId);
        }
        Ok(Self(value))
    }

    pub fn try_from_legacy_alor_numeric(value: i64) -> Result<Self, BrokerOrderIdImportError> {
        if value <= 0 {
            return Err(BrokerOrderIdImportError::NonPositiveLegacyAlorId(value));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BrokerOrderId {
    type Error = BrokerOrderIdImportError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_broker_native_exact(value)
    }
}

impl From<BrokerOrderId> for String {
    fn from(value: BrokerOrderId) -> Self {
        value.0
    }
}

impl fmt::Display for BrokerOrderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

pub fn deserialize_broker_order_id_legacy_numeric_or_string<'de, D>(
    deserializer: D,
) -> Result<BrokerOrderId, D::Error>
where
    D: Deserializer<'de>,
{
    struct BrokerOrderIdVisitor;

    impl Visitor<'_> for BrokerOrderIdVisitor {
        type Value = BrokerOrderId;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .write_str("a non-empty broker order id string or a positive legacy ALOR integer")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            BrokerOrderId::from_broker_native_exact(value).map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            BrokerOrderId::from_broker_native_exact(value).map_err(E::custom)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            BrokerOrderId::try_from_legacy_alor_numeric(value).map_err(E::custom)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let value = i64::try_from(value).map_err(|_| {
                E::custom(format!(
                    "legacy ALOR numeric order id exceeds i64 range: {value}"
                ))
            })?;
            BrokerOrderId::try_from_legacy_alor_numeric(value).map_err(E::custom)
        }
    }

    deserializer.deserialize_any(BrokerOrderIdVisitor)
}

#[derive(Deserialize)]
#[serde(transparent)]
struct LegacyBrokerOrderIdSerde(
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    BrokerOrderId,
);

pub fn deserialize_option_broker_order_id_legacy_numeric_or_string<'de, D>(
    deserializer: D,
) -> Result<Option<BrokerOrderId>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<LegacyBrokerOrderIdSerde>::deserialize(deserializer)
        .map(|value| value.map(|value| value.0))
}

pub fn deserialize_vec_broker_order_id_legacy_numeric_or_string<'de, D>(
    deserializer: D,
) -> Result<Vec<BrokerOrderId>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<LegacyBrokerOrderIdSerde>::deserialize(deserializer)
        .map(|values| values.into_iter().map(|value| value.0).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerOrderIdEncoding {
    BrokerOrderIdString,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrokerOrderIdImportError {
    #[error("broker-native order id cannot be empty")]
    EmptyBrokerNativeId,
    #[error("legacy ALOR numeric order id must be positive: {0}")]
    NonPositiveLegacyAlorId(i64),
}

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

    #[test]
    fn legacy_alor_numeric_order_id_imports_as_decimal_string() {
        let broker_order_id =
            BrokerOrderId::try_from_legacy_alor_numeric(2_033_126_389_943_253_218)
                .expect("positive legacy ALOR id imports");

        assert_eq!(broker_order_id.as_str(), "2033126389943253218");
    }

    #[test]
    fn legacy_alor_non_positive_order_id_is_rejected() {
        assert_eq!(
            BrokerOrderId::try_from_legacy_alor_numeric(0).expect_err("zero is sentinel"),
            BrokerOrderIdImportError::NonPositiveLegacyAlorId(0)
        );
        assert_eq!(
            BrokerOrderId::try_from_legacy_alor_numeric(-1).expect_err("negative is invalid"),
            BrokerOrderIdImportError::NonPositiveLegacyAlorId(-1)
        );
    }

    #[test]
    fn broker_native_order_id_string_is_preserved_exactly() {
        let raw = "FINAM-ORDER-0001/ABC_ё";
        let broker_order_id =
            BrokerOrderId::from_broker_native_exact(raw).expect("non-empty native id imports");

        assert_eq!(broker_order_id.as_str(), raw);
    }

    #[test]
    fn empty_broker_native_order_id_is_rejected() {
        assert_eq!(
            BrokerOrderId::from_broker_native_exact("").expect_err("empty native id rejected"),
            BrokerOrderIdImportError::EmptyBrokerNativeId
        );
    }

    #[test]
    fn broker_order_id_public_constructor_cannot_create_empty() {
        let panic = std::panic::catch_unwind(|| BrokerOrderId::new(""))
            .expect_err("checked constructor must not construct empty broker order id");

        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .expect("panic message is present");
        assert!(message.contains("non-empty broker-native id"));
    }

    #[test]
    fn broker_order_id_deserialize_empty_string_rejected() {
        let error = serde_json::from_str::<BrokerOrderId>(r#""""#)
            .expect_err("empty broker order id cannot deserialize");

        assert!(error.to_string().contains("cannot be empty"));
    }

    #[test]
    fn broker_order_id_deserialize_nonempty_string_preserved_exact() {
        let broker_order_id = serde_json::from_str::<BrokerOrderId>(r#""FINAM-ORDER-0002""#)
            .expect("non-empty broker order id deserializes");

        assert_eq!(broker_order_id.as_str(), "FINAM-ORDER-0002");
        assert_eq!(
            serde_json::to_string(&broker_order_id).expect("broker order id serializes"),
            r#""FINAM-ORDER-0002""#
        );
    }

    #[test]
    fn legacy_numeric_or_string_broker_order_id_deserializer_imports_decimal_string() {
        #[derive(Debug, Deserialize)]
        struct Fixture {
            #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
            order_id: BrokerOrderId,
        }

        let legacy = serde_json::from_str::<Fixture>(r#"{"order_id":2033126389943253218}"#)
            .expect("legacy numeric id imports");
        assert_eq!(legacy.order_id.as_str(), "2033126389943253218");

        let native = serde_json::from_str::<Fixture>(r#"{"order_id":"FINAM-ORDER-0003"}"#)
            .expect("native string id imports");
        assert_eq!(native.order_id.as_str(), "FINAM-ORDER-0003");
    }

    #[test]
    fn legacy_numeric_or_string_broker_order_id_deserializer_rejects_empty_zero_and_null() {
        #[allow(dead_code)]
        #[derive(Debug, Deserialize)]
        struct Fixture {
            #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
            order_id: BrokerOrderId,
        }

        for payload in [
            r#"{"order_id":""}"#,
            r#"{"order_id":0}"#,
            r#"{"order_id":-1}"#,
            r#"{"order_id":null}"#,
        ] {
            serde_json::from_str::<Fixture>(payload).expect_err("invalid broker id rejected");
        }
    }

    #[test]
    fn optional_and_vec_legacy_broker_order_id_deserializers_import_numeric_ids() {
        #[derive(Debug, Deserialize)]
        struct Fixture {
            #[serde(
                default,
                deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
            )]
            optional: Option<BrokerOrderId>,
            #[serde(
                default,
                deserialize_with = "deserialize_vec_broker_order_id_legacy_numeric_or_string"
            )]
            ids: Vec<BrokerOrderId>,
        }

        let fixture =
            serde_json::from_str::<Fixture>(r#"{"optional":123,"ids":[456,"FINAM-789"]}"#)
                .expect("legacy option/vector imports");

        assert_eq!(
            fixture.optional.as_ref().map(BrokerOrderId::as_str),
            Some("123")
        );
        assert_eq!(
            fixture
                .ids
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["456", "FINAM-789"]
        );
    }

    #[test]
    fn stage2b_runtime_order_id_encoding_markers_are_explicit() {
        assert_eq!(RUNTIME_STATE_SCHEMA_VERSION_V2, 2);
        assert_eq!(BROKER_ORDER_ID_ENCODING, "broker_order_id_string");
        assert_eq!(LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT, "decimal_string");
        assert_eq!(
            serde_json::to_string(&BrokerOrderIdEncoding::BrokerOrderIdString)
                .expect("encoding serializes"),
            "\"broker_order_id_string\""
        );
    }
}
