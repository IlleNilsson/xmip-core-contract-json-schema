#![forbid(unsafe_code)]

//! The JSON content contract — a technology of `xmip-core-contract`.
//!
//! Two claims, decided 2026-09-07: **well-formedness is a given** and is always
//! checked, and **conformance is a given once a contract is named** — a Receive
//! or Send Location that refers to this contract with a schema bound has every
//! Stream validated against that schema. An unbound contract is the first claim
//! alone.
//!
//! The schema dialect is the JSON Schema core and validation vocabulary that
//! [`schema`] documents; anything outside it is ignored, as the specification
//! says an unknown keyword must be.

pub mod check;
pub mod format;
pub mod schema;

use contract::{
    Contract, ContractDescriptor, ContractError, ContractFactory, ContractId, ValidationIssue,
    ValidationResult,
};
use schema::Schema;
use serde_json::Value;
use stream::Stream;
use xcore::settings::{Applies, Kind, Presence, Setting, Settings};

/// The JSON contract, bare or bound to a schema.
pub struct JsonSchema {
    descriptor: ContractDescriptor,
    schema: Option<Schema>,
}

impl JsonSchema {
    /// Well-formedness only.
    #[must_use]
    pub fn new() -> Self {
        Self {
            descriptor: descriptor("json-schema"),
            schema: None,
        }
    }

    /// Well-formedness and conformance to `schema`, compiled here, once.
    ///
    /// # Errors
    /// The schema must itself be a JSON Schema: an object or a boolean.
    pub fn with_schema(schema: &Value) -> Result<Self, ContractError> {
        let schema = Schema::compile(schema)?;
        Ok(Self {
            descriptor: descriptor(&format!("json-schema:{}", schema.name())),
            schema: Some(schema),
        })
    }

    /// Whether a schema is bound.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.schema.is_some()
    }
}

impl Default for JsonSchema {
    fn default() -> Self {
        Self::new()
    }
}

fn descriptor(id: &str) -> ContractDescriptor {
    ContractDescriptor {
        id: ContractId(id.to_string()),
        version: "1".to_string(),
        representation: "application/json".to_string(),
    }
}

impl Contract for JsonSchema {
    fn descriptor(&self) -> &ContractDescriptor {
        &self.descriptor
    }

    fn identify(&self, stream: &Stream) -> Result<bool, ContractError> {
        if stream.media_type().is_some_and(is_json_media_type) {
            return Ok(true);
        }
        let first = stream
            .bytes()
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace());
        Ok(matches!(first, Some(b'{' | b'[')))
    }

    /// Bare, the Stream is read a piece at a time and nothing of it is kept,
    /// so well-formedness holds in bounded memory however large the
    /// Stream; bound, the document is read whole, as a schema's keywords
    /// look across it.
    fn validate(&self, stream: &Stream) -> Result<ValidationResult, ContractError> {
        let Some(schema) = &self.schema else {
            use serde::Deserialize as _;
            let reader = stream.reader().map_err(|failed| {
                ContractError::new(format!("the Stream cannot be read: {failed}"))
            })?;
            let reading = std::io::BufReader::with_capacity(64 * 1024, reader);
            let mut document = serde_json::Deserializer::from_reader(reading);
            let held =
                serde::de::IgnoredAny::deserialize(&mut document).and_then(|_| document.end());
            return Ok(match held {
                Ok(()) => ValidationResult::of(Vec::new()),
                Err(error) => malformed(&error),
            });
        };
        let instance = match serde_json::from_slice::<Value>(stream.bytes()) {
            Ok(instance) => instance,
            Err(error) => return Ok(malformed(&error)),
        };
        Ok(ValidationResult::of(schema.check(&instance)))
    }
}

fn is_json_media_type(media_type: &str) -> bool {
    let essence = media_type.split(';').next().unwrap_or("").trim();
    essence.eq_ignore_ascii_case("application/json") || essence.ends_with("+json")
}

fn malformed(error: &serde_json::Error) -> ValidationResult {
    ValidationResult::of(vec![ValidationIssue::at(
        "malformed",
        format!("not valid JSON: {error}"),
        format!("line {} column {}", error.line(), error.column()),
    )])
}

/// Loads the contract a Location names: an empty reference is the bare
/// contract, anything else is the path of a schema file.
pub struct JsonSchemaFactory;

impl ContractFactory for JsonSchemaFactory {
    fn technology(&self) -> &'static str {
        "json-schema"
    }

    fn settings(&self) -> &'static Settings {
        SETTINGS
    }

    fn load(&self, reference: &str) -> Result<Box<dyn Contract>, ContractError> {
        if reference.trim().is_empty() {
            return Ok(Box::new(JsonSchema::new()));
        }
        let bytes = std::fs::read(reference).map_err(|error| ContractError {
            message: format!("cannot read schema {reference}: {error}"),
        })?;
        let schema = serde_json::from_slice::<Value>(&bytes).map_err(|error| ContractError {
            message: format!("schema {reference} is not valid JSON: {error}"),
        })?;
        Ok(Box::new(JsonSchema::with_schema(&schema)?))
    }
}

/// What a Location gives this contract (ADR-0064, amendment 2026-09-26).
const SETTINGS: &Settings = &Settings {
    technology: env!("CARGO_PKG_NAME"),
    settings: &[Setting {
        name: "reference",
        kind: Kind::Address,
        presence: Presence::Optional,
        meaning: "The path of the JSON Schema documents are held to; left out, any JSON holds.",
        applies: Applies::Both,
    }],
};

#[cfg(test)]
mod tests {
    use super::*;
    use contract::fixture::stream_as as stream;
    use serde_json::json;

    #[path = "../../tests/measured/order.rs"]
    mod order;
    use order::order_schema;

    #[test]
    fn bare_contract_holds_well_formed_json_only() {
        let bare = JsonSchema::new();
        let held = bare
            .validate(&stream(r#"{"any":"shape"}"#, None))
            .expect("validates");
        assert!(held.valid);
        let broken = bare
            .validate(&stream("{not json", None))
            .expect("validates");
        assert_eq!(broken.issues[0].code, "malformed");
        assert!(
            broken.issues[0]
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with("line 1"))
        );
    }

    #[test]
    fn bound_contract_holds_a_conforming_order() {
        let bound = JsonSchema::with_schema(&order_schema()).expect("a schema");
        assert_eq!(bound.descriptor().id.0, "json-schema:order");
        let text = r#"{"id":"A1","lines":[{"sku":"X","qty":2}]}"#;
        let held = bound.validate(&stream(text, None)).expect("validates");
        assert!(held.valid, "issues: {:?}", held.issues);
    }

    #[test]
    fn bound_contract_names_every_departure_with_its_path() {
        let bound = JsonSchema::with_schema(&order_schema()).expect("a schema");
        let text = r#"{"id":"","lines":[{"sku":1,"qty":0}],"extra":true}"#;
        let held = bound.validate(&stream(text, None)).expect("validates");
        assert!(!held.valid);
        let paths: Vec<&str> = held
            .issues
            .iter()
            .filter_map(|i| i.path.as_deref())
            .collect();
        assert!(paths.contains(&"/id"), "{paths:?}");
        assert!(paths.contains(&"/lines/0/sku"), "{paths:?}");
        assert!(paths.contains(&"/lines/0/qty"), "{paths:?}");
        assert!(paths.contains(&"/extra"), "{paths:?}");
    }

    #[test]
    fn a_schema_must_be_an_object_or_boolean() {
        assert!(JsonSchema::with_schema(&json!("no")).is_err());
        assert!(JsonSchema::with_schema(&json!(true)).is_ok());
    }

    #[test]
    fn identifies_by_media_type_or_first_byte() {
        let bare = JsonSchema::new();
        assert!(
            bare.identify(&stream("x", Some("application/json")))
                .expect("identifies")
        );
        assert!(
            bare.identify(&stream("x", Some("application/ld+json; charset=utf-8")))
                .expect("identifies")
        );
        assert!(bare.identify(&stream("  [1]", None)).expect("identifies"));
        assert!(
            !bare
                .identify(&stream("a,b", Some("text/csv")))
                .expect("identifies")
        );
    }

    #[test]
    fn the_factory_loads_bare_and_bound() {
        let factory = JsonSchemaFactory;
        assert_eq!(factory.technology(), "json-schema");
        let bare = factory.load("").expect("bare");
        assert_eq!(bare.descriptor().id.0, "json-schema");
        let dir = std::env::temp_dir().join("xmip-json-schema-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("order.schema.json");
        std::fs::write(&file, order_schema().to_string()).expect("write schema");
        let bound = factory
            .load(file.to_str().expect("utf-8 path"))
            .expect("bound");
        assert_eq!(bound.descriptor().id.0, "json-schema:order");
        assert!(
            factory
                .load(dir.join("missing.json").to_str().expect("path"))
                .is_err()
        );
    }

    #[test]
    fn json_schema_declares_its_settings_and_reads_through_them() {
        assert!(SETTINGS.problems().is_empty(), "{:?}", SETTINGS.problems());
        let given = |name: &str, value: &str| {
            (
                name.to_string(),
                xcore::settings::Given::Text(value.to_string()),
            )
        };
        assert!(JsonSchemaFactory.open(Applies::Both, &[]).is_ok(), "bare");
        let unread = JsonSchemaFactory
            .open(
                Applies::Receive,
                &[given("reference", "/no/such/order.json")],
            )
            .err()
            .expect("an unread file is refused");
        assert!(
            unread.message.contains("/no/such/order.json"),
            "{}",
            unread.message
        );
        let refused = JsonSchemaFactory
            .open(Applies::Send, &[given("unheard_of", "x")])
            .err()
            .expect("an unknown setting is refused");
        assert!(
            refused.message.contains("unheard_of"),
            "{}",
            refused.message
        );
    }
}
