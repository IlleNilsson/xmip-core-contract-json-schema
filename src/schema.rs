//! The JSON Schema vocabulary this contract evaluates, compiled once when a
//! schema is bound.
//!
//! Core: boolean schemas, `$ref` to a location inside the same document
//! (`#/$defs/...`, `#/definitions/...`, or any JSON pointer, percent-decoded
//! as a URI fragment is), `allOf`, `anyOf`, `oneOf`, `not`. Validation:
//! `type`, `enum`, `const`, `properties`, `required`, `additionalProperties`,
//! `items`, `minItems`, `maxItems`, `minLength`, `maxLength`, `pattern`,
//! `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, and
//! `format` for `date`, `time`, `date-time`, `email`, `uri`, `ipv4`, `ipv6`,
//! `uuid` and `regex` — an assertion here, not an annotation, because a
//! Location that names a format means it. Every other keyword and format is
//! ignored, as the specification requires of an unknown one.
//!
//! The schema is read once, into a tree of [`Node`]s: every keyword looked up,
//! every `$ref` resolved, every `pattern` compiled to its automaton. A
//! Stream is then held to the tree ([`crate::check`]) without reading the
//! schema document again.

use crate::format::Format;
use contract::ContractError;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A bound JSON Schema, compiled.
#[derive(Debug)]
pub struct Schema {
    pub(crate) nodes: Vec<Node>,
    name: String,
}

/// One schema in the tree; the root is node 0.
#[derive(Debug)]
pub(crate) enum Node {
    /// `true`, or anything that is not a schema at all: admits everything.
    Any,
    /// `false`: admits nothing.
    Nothing,
    Keywords(Box<Keywords>),
}

/// The keywords one schema object carries, each read once.
#[derive(Debug, Default)]
pub(crate) struct Keywords {
    /// The node `$ref` lands on, or the reference that does not land.
    pub reference: Option<Result<usize, String>>,
    /// The type names `type` allows, and how a message lists them.
    pub types: Option<(Vec<String>, String)>,
    pub enumeration: Option<Vec<Value>>,
    pub constant: Option<Value>,
    pub required: Vec<String>,
    pub properties: HashMap<String, usize>,
    pub additional: Additional,
    pub items: Option<usize>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    /// The compiled `pattern`, or the text that is not a pattern.
    pub pattern: Option<Result<Regex, String>>,
    /// A `format` this contract asserts, and its name.
    pub format: Option<(Format, String)>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub exclusive_minimum: Option<f64>,
    pub exclusive_maximum: Option<f64>,
    pub all_of: Vec<usize>,
    pub any_of: Vec<usize>,
    pub one_of: Vec<usize>,
    pub not: Option<usize>,
}

/// What `additionalProperties` says of a member `properties` does not name.
#[derive(Debug, Default)]
pub(crate) enum Additional {
    #[default]
    Allowed,
    Forbidden,
    Schema(usize),
}

impl Schema {
    /// Compile `document` as a JSON Schema.
    ///
    /// # Errors
    /// The document must itself be a JSON Schema: an object or a boolean.
    pub fn compile(document: &Value) -> Result<Self, ContractError> {
        if !(document.is_object() || document.is_boolean()) {
            return Err(ContractError::new(
                "a JSON Schema is an object or a boolean",
            ));
        }
        let name = document
            .get("$id")
            .or_else(|| document.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("bound")
            .to_string();
        let mut compiler = Compiler {
            root: document,
            nodes: Vec::new(),
            seen: HashMap::new(),
        };
        compiler.node(document);
        Ok(Self {
            nodes: compiler.nodes,
            name,
        })
    }

    /// The schema's `$id`, else its `title`, else `bound`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Compiles each schema object once, however many `$ref`s reach it, so a
/// recursive schema is a cycle in the tree rather than an endless one.
struct Compiler<'a> {
    root: &'a Value,
    nodes: Vec<Node>,
    seen: HashMap<*const Value, usize>,
}

impl<'a> Compiler<'a> {
    fn node(&mut self, schema: &'a Value) -> usize {
        if let Some(&id) = self.seen.get(&std::ptr::from_ref(schema)) {
            return id;
        }
        let id = self.nodes.len();
        self.seen.insert(std::ptr::from_ref(schema), id);
        self.nodes.push(Node::Any);
        let node = match schema {
            Value::Bool(false) => Node::Nothing,
            Value::Object(keywords) => Node::Keywords(Box::new(self.keywords(keywords))),
            _ => Node::Any,
        };
        self.nodes[id] = node;
        id
    }

    fn keywords(&mut self, keywords: &'a Map<String, Value>) -> Keywords {
        let number = |name: &str| keywords.get(name).and_then(Value::as_f64);
        let count = |name: &str| keywords.get(name).and_then(Value::as_u64);
        let mut compiled = Keywords {
            enumeration: keywords.get("enum").and_then(Value::as_array).cloned(),
            constant: keywords.get("const").cloned(),
            min_items: count("minItems"),
            max_items: count("maxItems"),
            min_length: count("minLength"),
            max_length: count("maxLength"),
            minimum: number("minimum"),
            maximum: number("maximum"),
            exclusive_minimum: number("exclusiveMinimum"),
            exclusive_maximum: number("exclusiveMaximum"),
            ..Keywords::default()
        };
        if let Some(reference) = keywords.get("$ref").and_then(Value::as_str) {
            compiled.reference = Some(
                contract::reference::resolve(self.root, reference)
                    .map(|target| self.node(target))
                    .ok_or_else(|| reference.to_string()),
            );
        }
        compiled.types = match keywords.get("type") {
            Some(Value::String(one)) => Some(vec![one.clone()]),
            Some(Value::Array(many)) => Some(
                many.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        }
        .map(|allowed: Vec<String>| {
            let listed = allowed.join(" or ");
            (allowed, listed)
        });
        if let Some(Value::Array(required)) = keywords.get("required") {
            compiled.required = required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if let Some(properties) = keywords.get("properties").and_then(Value::as_object) {
            for (name, schema) in properties {
                let id = self.node(schema);
                compiled.properties.insert(name.clone(), id);
            }
        }
        compiled.additional = match keywords.get("additionalProperties") {
            Some(Value::Bool(false)) => Additional::Forbidden,
            Some(schema @ Value::Object(_)) => Additional::Schema(self.node(schema)),
            _ => Additional::Allowed,
        };
        compiled.items = keywords.get("items").map(|schema| self.node(schema));
        compiled.pattern = keywords
            .get("pattern")
            .and_then(Value::as_str)
            .map(|pattern| Regex::new(pattern).map_err(|_| pattern.to_string()));
        compiled.format = keywords
            .get("format")
            .and_then(Value::as_str)
            .and_then(|name| Format::named(name).map(|format| (format, name.to_string())));
        compiled.all_of = self.branches(keywords, "allOf");
        compiled.any_of = self.branches(keywords, "anyOf");
        compiled.one_of = self.branches(keywords, "oneOf");
        compiled.not = keywords.get("not").map(|schema| self.node(schema));
        compiled
    }

    fn branches(&mut self, keywords: &'a Map<String, Value>, name: &str) -> Vec<usize> {
        keywords
            .get(name)
            .and_then(Value::as_array)
            .map(|branches| branches.iter().map(|b| self.node(b)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_schema_is_an_object_or_a_boolean_and_is_named_by_id_or_title() {
        assert!(Schema::compile(&json!("no")).is_err());
        assert_eq!(Schema::compile(&json!(true)).expect("true").name(), "bound");
        let titled = json!({ "title": "order", "$id": "urn:order" });
        assert_eq!(
            Schema::compile(&titled).expect("titled").name(),
            "urn:order"
        );
    }

    #[test]
    fn a_recursive_schema_compiles_each_object_once() {
        let tree = json!({
            "$defs": { "node": { "properties": { "children": {
                "items": { "$ref": "#/$defs/node" } } } } },
            "$ref": "#/$defs/node"
        });
        let schema = Schema::compile(&tree).expect("compiles");
        // The root, node, children and its items: four objects, four nodes.
        assert_eq!(schema.nodes.len(), 4);
    }
}
