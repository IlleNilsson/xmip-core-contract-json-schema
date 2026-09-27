//! Holding an instance to a compiled [`Schema`].
//!
//! An issue's `code` is the keyword that failed and its `path` the JSON
//! Pointer of the instance location (RFC 6901, the empty pointer for the
//! whole document), so an operator reads `/lines/0/qty: minimum`. The
//! pointer is spelled only for an issue raised; an instance that holds costs
//! none.

use crate::schema::{Additional, Keywords, Node, Schema};
use contract::ValidationIssue;
use contract::place::Place;
use serde_json::Value;

impl Schema {
    /// Every way `instance` departs from this schema.
    #[must_use]
    pub fn check(&self, instance: &Value) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        self.node(0, instance, &Place::Root, &mut issues);
        issues
    }

    /// Whether `instance` holds to node `id`, for a composition that only
    /// asks.
    fn holds(&self, id: usize, instance: &Value, place: &Place<'_>) -> bool {
        let mut issues = Vec::new();
        self.node(id, instance, place, &mut issues);
        issues.is_empty()
    }

    fn node(&self, id: usize, instance: &Value, place: &Place<'_>, out: &mut Vec<ValidationIssue>) {
        let keywords = match &self.nodes[id] {
            Node::Any => return,
            Node::Nothing => {
                out.push(issue("false", "nothing is allowed here", place));
                return;
            }
            Node::Keywords(keywords) => keywords,
        };
        match &keywords.reference {
            Some(Ok(target)) => self.node(*target, instance, place, out),
            Some(Err(reference)) => {
                out.push(issue("$ref", format!("unresolvable {reference}"), place));
            }
            None => {}
        }
        check_type(keywords, instance, place, out);
        check_values(keywords, instance, place, out);
        self.check_object(keywords, instance, place, out);
        self.check_array(keywords, instance, place, out);
        check_string(keywords, instance, place, out);
        check_number(keywords, instance, place, out);
        self.check_composition(keywords, instance, place, out);
    }

    fn check_object(
        &self,
        keywords: &Keywords,
        instance: &Value,
        place: &Place<'_>,
        out: &mut Vec<ValidationIssue>,
    ) {
        let Value::Object(members) = instance else {
            return;
        };
        for name in &keywords.required {
            if !members.contains_key(name) {
                let message = format!("missing property {name}");
                out.push(issue("required", message, &place.field(name)));
            }
        }
        for (name, value) in members {
            let member = place.field(name);
            match (keywords.properties.get(name), &keywords.additional) {
                (Some(&id), _) | (None, &Additional::Schema(id)) => {
                    self.node(id, value, &member, out);
                }
                (None, Additional::Forbidden) => {
                    out.push(issue(
                        "additionalProperties",
                        "is not a declared property",
                        &member,
                    ));
                }
                (None, Additional::Allowed) => {}
            }
        }
    }

    fn check_array(
        &self,
        keywords: &Keywords,
        instance: &Value,
        place: &Place<'_>,
        out: &mut Vec<ValidationIssue>,
    ) {
        let Value::Array(items) = instance else {
            return;
        };
        let length = items.len() as u64;
        if let Some(min) = keywords.min_items
            && length < min
        {
            let message = format!("has {length} items, at least {min} required");
            out.push(issue("minItems", message, place));
        }
        if let Some(max) = keywords.max_items
            && length > max
        {
            let message = format!("has {length} items, at most {max} allowed");
            out.push(issue("maxItems", message, place));
        }
        if let Some(id) = keywords.items {
            for (index, item) in items.iter().enumerate() {
                self.node(id, item, &place.index(index), out);
            }
        }
    }

    fn check_composition(
        &self,
        keywords: &Keywords,
        instance: &Value,
        place: &Place<'_>,
        out: &mut Vec<ValidationIssue>,
    ) {
        for &branch in &keywords.all_of {
            self.node(branch, instance, place, out);
        }
        if !keywords.any_of.is_empty()
            && !keywords
                .any_of
                .iter()
                .any(|&b| self.holds(b, instance, place))
        {
            out.push(issue("anyOf", "matches none of the alternatives", place));
        }
        if !keywords.one_of.is_empty() {
            let matching = keywords
                .one_of
                .iter()
                .filter(|&&b| self.holds(b, instance, place))
                .count();
            if matching != 1 {
                let message = format!("matches {matching} alternatives, exactly one required");
                out.push(issue("oneOf", message, place));
            }
        }
        if let Some(forbidden) = keywords.not
            && self.holds(forbidden, instance, place)
        {
            out.push(issue("not", "matches a schema it must not", place));
        }
    }
}

fn issue(code: &'static str, message: impl Into<String>, place: &Place<'_>) -> ValidationIssue {
    ValidationIssue::at(code, message, place.pointer())
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_integer(value: &Value) -> bool {
    match value {
        Value::Number(number) => {
            number.is_i64() || number.is_u64() || number.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        _ => false,
    }
}

fn has_type(instance: &Value, wanted: &str) -> bool {
    match wanted {
        "integer" => is_integer(instance),
        "number" => instance.is_number(),
        other => type_name(instance) == other,
    }
}

fn check_type(
    keywords: &Keywords,
    instance: &Value,
    place: &Place<'_>,
    out: &mut Vec<ValidationIssue>,
) {
    let Some((allowed, listed)) = &keywords.types else {
        return;
    };
    if !allowed.iter().any(|name| has_type(instance, name)) {
        let message = format!("is {}, expected {listed}", type_name(instance));
        out.push(issue("type", message, place));
    }
}

fn check_values(
    keywords: &Keywords,
    instance: &Value,
    place: &Place<'_>,
    out: &mut Vec<ValidationIssue>,
) {
    if let Some(allowed) = &keywords.enumeration
        && !allowed.contains(instance)
    {
        out.push(issue("enum", "is not one of the allowed values", place));
    }
    if let Some(expected) = &keywords.constant
        && expected != instance
    {
        out.push(issue("const", format!("must be {expected}"), place));
    }
}

fn check_string(
    keywords: &Keywords,
    instance: &Value,
    place: &Place<'_>,
    out: &mut Vec<ValidationIssue>,
) {
    let Value::String(text) = instance else {
        return;
    };
    if keywords.min_length.is_some() || keywords.max_length.is_some() {
        let length = text.chars().count() as u64;
        if let Some(min) = keywords.min_length
            && length < min
        {
            let message = format!("is {length} characters, at least {min} required");
            out.push(issue("minLength", message, place));
        }
        if let Some(max) = keywords.max_length
            && length > max
        {
            let message = format!("is {length} characters, at most {max} allowed");
            out.push(issue("maxLength", message, place));
        }
    }
    match &keywords.pattern {
        Some(Ok(regex)) if !regex.is_match(text) => {
            out.push(issue("pattern", "does not match the pattern", place));
        }
        Some(Err(pattern)) => {
            let message = format!("{pattern:?} is not a pattern");
            out.push(issue("pattern", message, place));
        }
        _ => {}
    }
    if let Some((format, name)) = &keywords.format
        && !format.holds(text)
    {
        out.push(issue("format", format!("is not a {name}"), place));
    }
}

fn check_number(
    keywords: &Keywords,
    instance: &Value,
    place: &Place<'_>,
    out: &mut Vec<ValidationIssue>,
) {
    let Some(number) = instance.as_f64() else {
        return;
    };
    if keywords.minimum.is_some_and(|min| number < min) {
        let message = format!("{number} is below the minimum");
        out.push(issue("minimum", message, place));
    }
    if keywords.maximum.is_some_and(|max| number > max) {
        let message = format!("{number} is above the maximum");
        out.push(issue("maximum", message, place));
    }
    if keywords.exclusive_minimum.is_some_and(|min| number <= min) {
        let message = format!("{number} is not above the minimum");
        out.push(issue("exclusiveMinimum", message, place));
    }
    if keywords.exclusive_maximum.is_some_and(|max| number >= max) {
        let message = format!("{number} is not below the maximum");
        out.push(issue("exclusiveMaximum", message, place));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn codes(schema: &Value, instance: &Value) -> Vec<String> {
        Schema::compile(schema)
            .expect("a schema")
            .check(instance)
            .into_iter()
            .map(|i| i.code.into_owned())
            .collect()
    }

    #[test]
    fn a_local_ref_resolves_through_defs() {
        let schema = json!({
            "$defs": { "id": { "type": "string" } },
            "properties": { "id": { "$ref": "#/$defs/id" } }
        });
        assert!(codes(&schema, &json!({"id": "x"})).is_empty());
        assert_eq!(codes(&schema, &json!({"id": 1})), ["type"]);
        let dangling = json!({ "$ref": "#/$defs/nowhere" });
        assert_eq!(codes(&dangling, &json!(1)), ["$ref"]);
        let escaped = json!({
            "$defs": { "a b": { "type": "null" } },
            "$ref": "#/$defs/a%20b"
        });
        assert_eq!(codes(&escaped, &json!(1)), ["type"]);
    }

    #[test]
    fn integer_admits_a_whole_float_and_refuses_a_fraction() {
        let schema = json!({ "type": "integer" });
        assert!(codes(&schema, &json!(2.0)).is_empty());
        assert_eq!(codes(&schema, &json!(2.5)), ["type"]);
    }

    #[test]
    fn composition_keywords_report_as_themselves() {
        let one = json!({ "oneOf": [{ "type": "string" }, { "type": "number" }] });
        assert!(codes(&one, &json!("s")).is_empty());
        assert_eq!(codes(&one, &json!(true)), ["oneOf"]);
        let not = json!({ "not": { "type": "null" } });
        assert_eq!(codes(&not, &json!(null)), ["not"]);
        let any = json!({ "anyOf": [{ "minimum": 10 }, { "maximum": 0 }] });
        assert_eq!(codes(&any, &json!(5)), ["anyOf"]);
    }

    #[test]
    fn a_false_schema_admits_nothing_and_a_pointer_escapes() {
        assert_eq!(codes(&json!(false), &json!(1)), ["false"]);
        let schema = json!({ "properties": { "a/b": { "type": "null" } } });
        let issues = Schema::compile(&schema)
            .expect("a schema")
            .check(&json!({"a/b": 1}));
        assert_eq!(issues[0].path.as_deref(), Some("/a~1b"));
        let whole = Schema::compile(&json!(false))
            .expect("false")
            .check(&json!(1));
        assert_eq!(whole[0].path.as_deref(), Some(""), "the whole document");
    }

    #[test]
    fn a_pattern_is_an_unanchored_search_and_a_bad_one_is_named() {
        let schema = json!({ "pattern": "[A-Z]{2}\\d{4}" });
        assert!(codes(&schema, &json!("ref SE1234 ok")).is_empty());
        assert_eq!(codes(&schema, &json!("se1234")), ["pattern"]);
        let broken = json!({ "pattern": "(" });
        let issues = Schema::compile(&broken)
            .expect("a schema")
            .check(&json!("x"));
        assert!(issues[0].message.contains("is not a pattern"));
    }

    #[test]
    fn formats_are_asserted_not_annotated() {
        let of = |format: &str| json!({ "format": format });
        assert!(codes(&of("date"), &json!("2026-09-07")).is_empty());
        assert_eq!(codes(&of("date"), &json!("2026-13-07")), ["format"]);
        assert_eq!(codes(&of("date"), &json!("2026-02-31")), ["format"]);
        assert_eq!(codes(&of("date"), &json!("2026-02-29")), ["format"]);
        assert!(codes(&of("date"), &json!("2028-02-29")).is_empty());
        assert!(codes(&of("time"), &json!("13:45:00Z")).is_empty());
        assert_eq!(codes(&of("time"), &json!("25:45:00Z")), ["format"]);
        assert!(codes(&of("date-time"), &json!("2026-09-07T13:45:00+02:00")).is_empty());
        assert_eq!(
            codes(&of("date-time"), &json!("2026-02-31T13:45:00Z")),
            ["format"]
        );
        assert!(codes(&of("email"), &json!("ilian@example.se")).is_empty());
        assert_eq!(codes(&of("email"), &json!("nobody")), ["format"]);
        assert!(codes(&of("uri"), &json!("xmip:///playground")).is_empty());
        assert!(codes(&of("ipv6"), &json!("::1")).is_empty());
        assert!(codes(&of("uuid"), &json!("0192b6d4-7c3e-7f3a-9b2a-3d4e5f6a7b8c")).is_empty());
        assert_eq!(codes(&of("uuid"), &json!("not-a-uuid")), ["format"]);
        assert_eq!(codes(&of("regex"), &json!("(")), ["format"]);
        assert!(codes(&of("hostname-we-do-not-check"), &json!("anything")).is_empty());
    }
}
