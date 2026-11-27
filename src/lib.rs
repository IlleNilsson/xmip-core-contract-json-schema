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
//!
//! **Built with its `declaration` feature alone** the crate is
//! [`declaration`].

#[cfg(feature = "declaration")]
pub mod declaration;

#[cfg(feature = "default")]
pub mod chain;
#[cfg(feature = "default")]
pub mod check;
#[cfg(feature = "default")]
pub mod format;
#[cfg(feature = "default")]
pub mod schema;

#[cfg(feature = "default")]
use contract::{
    Contract, ContractDescriptor, ContractError, ContractFactory, ContractId, ValidationIssue,
    ValidationResult,
};
#[cfg(feature = "default")]
use schema::Schema;
#[cfg(feature = "default")]
use serde_json::Value;
#[cfg(feature = "default")]
use stream::Stream;
#[cfg(feature = "default")]
use xcore::settings::Settings;

/// The JSON contract, bare or bound to a schema.
#[cfg(feature = "default")]
pub struct JsonSchema {
    descriptor: ContractDescriptor,
    schema: Option<Schema>,
}

#[cfg(feature = "default")]
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
    /// The schema must itself be a JSON Schema: an object or a boolean, whose
    /// references end ([`Schema::compile`]).
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

#[cfg(feature = "default")]
impl Default for JsonSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "default")]
fn descriptor(id: &str) -> ContractDescriptor {
    ContractDescriptor {
        id: ContractId(id.to_string()),
        version: "1".to_string(),
        representation: "application/json".to_string(),
    }
}

#[cfg(feature = "default")]
impl Contract for JsonSchema {
    fn descriptor(&self) -> &ContractDescriptor {
        &self.descriptor
    }

    fn identify(&self, stream: &Stream) -> Result<bool, ContractError> {
        if stream.media_type().is_some_and(is_json_media_type) {
            return Ok(true);
        }
        let first = stream
            .load()?
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
        let read = stream.load()?;
        let instance = match serde_json::from_slice::<Value>(read) {
            Ok(instance) => instance,
            Err(error) => return Ok(malformed(&error)),
        };
        Ok(ValidationResult::of(schema.check(&instance, read.len())))
    }
}

#[cfg(feature = "default")]
fn is_json_media_type(media_type: &str) -> bool {
    let essence = media_type.split(';').next().unwrap_or("").trim();
    essence.eq_ignore_ascii_case("application/json") || essence.ends_with("+json")
}

#[cfg(feature = "default")]
fn malformed(error: &serde_json::Error) -> ValidationResult {
    ValidationResult::of(vec![ValidationIssue::at(
        "malformed",
        format!("not valid JSON: {error}"),
        format!("line {} column {}", error.line(), error.column()),
    )])
}

/// Loads the contract a Location names: an empty reference is the bare
/// contract, anything else is the path of a schema file.
#[cfg(feature = "default")]
pub struct JsonSchemaFactory;

#[cfg(feature = "default")]
impl ContractFactory for JsonSchemaFactory {
    fn technology(&self) -> &'static str {
        declaration::NAME
    }

    fn settings(&self) -> &'static Settings {
        declaration::SETTINGS
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

// Declared here and not in `tests`: a path from an inline module passes
// through a directory named for it that does not exist, which Windows
// forgives and Linux does not.
#[cfg(feature = "default")]
#[cfg(test)]
#[path = "../tests/measured/order.rs"]
mod order;

#[cfg(test)]
mod tests {
    use super::*;
    use contract::fixture::stream_as as stream;
    use serde_json::json;

    use crate::order::order_schema;

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
        let dir = file::temporary::Folder::new("json-schema");
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

    /// The stack a schema is bound and held on here: Rust's default for a
    /// thread, the one a node's threads have. A walk that never ends
    /// overflows it and aborts the run, naming the thread; a check at its
    /// depth bound fits, unoptimized too (about 1.1 MiB, 2026-11-19).
    const STACK: usize = 2 * 1024 * 1024;

    /// A stack a bind reading schemas by recursion, one frame for each, runs
    /// out of within a few hundred; a bind from a worklist needs no more for
    /// a long schema than for a short one.
    const SMALL: usize = 64 * 1024;

    /// How long a bind and a hold may take before the test fails. Not a time
    /// held to (`rust-style.md` 5c): it catches a walk that never ends.
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

    /// [`held_on`] the [`STACK`].
    fn held(schema: Value, document: String) -> Result<Vec<ValidationIssue>, String> {
        held_on(STACK, schema, document)
    }

    /// Binds `schema` as a Location does and holds `document` to it, on a
    /// thread of its own with `stack` and [`DEADLINE`]: the refusal at bind,
    /// or every issue the document raised.
    fn held_on(
        stack: usize,
        schema: Value,
        document: String,
    ) -> Result<Vec<ValidationIssue>, String> {
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("bound-and-held".into())
            .stack_size(stack)
            .spawn(move || {
                let held = JsonSchema::with_schema(&schema)
                    .and_then(|bound| bound.validate(&stream(&document, None)))
                    .map(|result| result.issues)
                    .map_err(|refused| refused.message);
                let _ = done.send(held);
            })
            .expect("a thread");
        finished
            .recv_timeout(DEADLINE)
            .expect("bound and held within the deadline")
    }

    fn codes(issues: &[ValidationIssue]) -> Vec<&str> {
        issues.iter().map(|issue| issue.code.as_ref()).collect()
    }

    /// `levels` objects, each the `child` of the one above, the deepest
    /// holding `{"value": leaf}`.
    fn nested(levels: usize, leaf: &str) -> String {
        format!(
            "{}{{\"value\":{leaf}}}{}",
            "{\"child\":".repeat(levels),
            "}".repeat(levels)
        )
    }

    /// `$defs` of `links` schemas, each made by `link` from the reference to
    /// the next, the last a string; the root refers to the first.
    fn linked(links: usize, link: impl Fn(String) -> Value) -> Value {
        let mut defs = serde_json::Map::new();
        for at in 0..links {
            defs.insert(format!("s{at}"), link(format!("#/$defs/s{}", at + 1)));
        }
        defs.insert(format!("s{links}"), json!({ "type": "string" }));
        json!({ "$defs": defs, "$ref": "#/$defs/s0" })
    }

    #[test]
    fn a_schema_referring_to_itself_without_descending_is_refused_at_bind() {
        let refused = held(json!({ "$ref": "#" }), "1".into()).expect_err("refused at bind");
        assert!(refused.contains("$ref '#'"), "{refused}");
    }

    #[test]
    fn mutual_references_without_descending_are_refused() {
        let mutual = json!({
            "$defs": { "a": { "$ref": "#/$defs/b" }, "b": { "$ref": "#/$defs/a" } },
            "$ref": "#/$defs/a"
        });
        let refused = held(mutual, "1".into()).expect_err("refused at bind");
        assert!(refused.contains("$ref '#/$defs/a'"), "{refused}");
    }

    #[test]
    #[expect(
        non_snake_case,
        reason = "the keywords are named as JSON Schema spells them"
    )]
    fn a_cycle_through_allOf_anyOf_oneOf_or_not_is_refused() {
        for cycle in [
            json!({ "allOf": [{ "$ref": "#" }] }),
            json!({ "anyOf": [{ "type": "string" }, { "$ref": "#" }] }),
            json!({ "oneOf": [{ "type": "string" }, { "$ref": "#" }] }),
            json!({ "not": { "$ref": "#" } }),
        ] {
            let refused = held(cycle.clone(), "1".into()).expect_err("refused at bind");
            assert!(refused.contains("$ref '#'"), "{cycle}: {refused}");
        }
    }

    #[test]
    fn a_recursive_tree_that_descends_holds_100_levels_and_names_a_departure_at_depth() {
        let tree = json!({
            "$defs": { "node": {
                "type": "object",
                "properties": {
                    "child": { "$ref": "#/$defs/node" },
                    "value": { "type": "integer" }
                }
            } },
            "$ref": "#/$defs/node"
        });
        assert_eq!(held(tree.clone(), nested(100, "1")), Ok(Vec::new()));
        let departed = held(tree, nested(100, "\"x\"")).expect("bound");
        assert_eq!(codes(&departed), ["type"]);
        let path = format!("{}/value", "/child".repeat(100));
        assert_eq!(departed[0].path.as_deref(), Some(path.as_str()));
        // Five schemas a level: past the depth a check follows, it stops there.
        let winding = json!({
            "$defs": { "node": { "properties": { "child": { "allOf": [{ "allOf": [
                { "allOf": [{ "$ref": "#/$defs/node" }] }
            ] }] } } } },
            "$ref": "#/$defs/node"
        });
        let stopped = held(winding, nested(110, "1")).expect("bound");
        assert_eq!(codes(&stopped), ["bound"], "{stopped:?}");
        let deep = stopped[0].path.as_deref().unwrap_or_default();
        assert!(deep.starts_with(&"/child".repeat(100)), "{deep}");
    }

    #[test]
    fn a_chain_of_1000_references_is_refused_at_bind_without_overflow() {
        let chain = linked(1000, |next| json!({ "$ref": next }));
        let refused = held_on(SMALL, chain, "\"x\"".into()).expect_err("refused at bind");
        assert!(refused.contains("512"), "{refused}");
    }

    #[test]
    fn a_schema_reevaluating_exponentially_stops_at_its_bound() {
        // Each schema holds the place to the next twice: 2^40 walks unbounded.
        let doubling = linked(
            40,
            |next| json!({ "allOf": [{ "$ref": next.clone() }, { "$ref": next }] }),
        );
        let stopped = held(doubling, "\"x\"".into()).expect("bound, not refused");
        assert_eq!(codes(&stopped), ["bound"], "{stopped:?}");
        assert_eq!(stopped[0].path.as_deref(), Some(""));
    }

    #[test]
    fn json_schema_declares_its_settings_and_reads_through_them() {
        use declaration::SETTINGS;
        use xcore::settings::Applies;
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
