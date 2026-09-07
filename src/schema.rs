//! The JSON Schema vocabulary this contract evaluates.
//!
//! Core: boolean schemas, `$ref` to a location inside the same document
//! (`#/$defs/...`, `#/definitions/...`, or any JSON pointer), `allOf`, `anyOf`,
//! `oneOf`, `not`. Validation: `type`, `enum`, `const`, `properties`,
//! `required`, `additionalProperties`, `items`, `minItems`, `maxItems`,
//! `minLength`, `maxLength`, `minimum`, `maximum`, `exclusiveMinimum`,
//! `exclusiveMaximum`. Every other keyword is ignored, as the specification
//! requires of an unknown one; `pattern` and `format` are among them until a
//! regular-expression contract exists to lean on.
//!
//! An issue's `code` is the keyword that failed and its `path` the JSON pointer
//! of the instance location, so an operator reads `/lines/0/qty: minimum`.

use contract::ValidationIssue;
use serde_json::{Map, Value};

/// Evaluate `instance` at `path` against `schema`, resolving `$ref` in `root`.
#[must_use]
pub fn check(root: &Value, schema: &Value, instance: &Value, path: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let keywords = match schema {
        Value::Bool(false) => {
            issues.push(issue("false", "nothing is allowed here", path));
            return issues;
        }
        Value::Object(keywords) => keywords,
        // `true`, and anything that is not a schema at all, admits everything.
        _ => return issues,
    };

    if let Some(reference) = keywords.get("$ref").and_then(Value::as_str) {
        match resolve(root, reference) {
            Some(target) => issues.extend(check(root, target, instance, path)),
            None => issues.push(issue("$ref", &format!("unresolvable {reference}"), path)),
        }
    }

    check_type(keywords, instance, path, &mut issues);
    check_values(keywords, instance, path, &mut issues);
    check_object(root, keywords, instance, path, &mut issues);
    check_array(root, keywords, instance, path, &mut issues);
    check_string(keywords, instance, path, &mut issues);
    check_number(keywords, instance, path, &mut issues);
    check_composition(root, keywords, instance, path, &mut issues);
    issues
}

fn issue(code: &str, message: &str, path: &str) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        message: message.to_string(),
        path: Some(if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        }),
    }
}

fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    if pointer.is_empty() {
        return Some(root);
    }
    let unescaped = pointer
        .replace("%25", "%")
        .replace("%7B", "{")
        .replace("%7D", "}");
    root.pointer(&unescaped)
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
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let Some(wanted) = keywords.get("type") else {
        return;
    };
    let allowed: Vec<&str> = match wanted {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => return,
    };
    if !allowed.iter().any(|name| has_type(instance, name)) {
        let message = format!(
            "is {}, expected {}",
            type_name(instance),
            allowed.join(" or ")
        );
        out.push(issue("type", &message, path));
    }
}

fn check_values(
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    if let Some(Value::Array(allowed)) = keywords.get("enum")
        && !allowed.contains(instance)
    {
        out.push(issue("enum", "is not one of the allowed values", path));
    }
    if let Some(expected) = keywords.get("const")
        && expected != instance
    {
        out.push(issue("const", &format!("must be {expected}"), path));
    }
}

fn check_object(
    root: &Value,
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let Value::Object(members) = instance else {
        return;
    };
    let properties = keywords.get("properties").and_then(Value::as_object);

    if let Some(Value::Array(required)) = keywords.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !members.contains_key(name) {
                out.push(issue(
                    "required",
                    &format!("missing property {name}"),
                    &child(path, name),
                ));
            }
        }
    }

    for (name, value) in members {
        let member_path = child(path, name);
        match properties.and_then(|p| p.get(name)) {
            Some(subschema) => out.extend(check(root, subschema, value, &member_path)),
            None => match keywords.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    out.push(issue(
                        "additionalProperties",
                        "is not a declared property",
                        &member_path,
                    ));
                }
                Some(subschema @ Value::Object(_)) => {
                    out.extend(check(root, subschema, value, &member_path));
                }
                _ => {}
            },
        }
    }
}

fn check_array(
    root: &Value,
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let Value::Array(items) = instance else {
        return;
    };
    if let Some(min) = keywords.get("minItems").and_then(Value::as_u64)
        && (items.len() as u64) < min
    {
        out.push(issue(
            "minItems",
            &format!("has {} items, at least {min} required", items.len()),
            path,
        ));
    }
    if let Some(max) = keywords.get("maxItems").and_then(Value::as_u64)
        && (items.len() as u64) > max
    {
        out.push(issue(
            "maxItems",
            &format!("has {} items, at most {max} allowed", items.len()),
            path,
        ));
    }
    if let Some(subschema) = keywords.get("items") {
        for (index, item) in items.iter().enumerate() {
            out.extend(check(
                root,
                subschema,
                item,
                &child(path, &index.to_string()),
            ));
        }
    }
}

fn check_string(
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let Value::String(text) = instance else {
        return;
    };
    let length = text.chars().count() as u64;
    if let Some(min) = keywords.get("minLength").and_then(Value::as_u64)
        && length < min
    {
        out.push(issue(
            "minLength",
            &format!("is {length} characters, at least {min} required"),
            path,
        ));
    }
    if let Some(max) = keywords.get("maxLength").and_then(Value::as_u64)
        && length > max
    {
        out.push(issue(
            "maxLength",
            &format!("is {length} characters, at most {max} allowed"),
            path,
        ));
    }
}

fn check_number(
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let Some(number) = instance.as_f64() else {
        return;
    };
    let bound = |name: &str| keywords.get(name).and_then(Value::as_f64);
    if bound("minimum").is_some_and(|min| number < min) {
        out.push(issue(
            "minimum",
            &format!("{number} is below the minimum"),
            path,
        ));
    }
    if bound("maximum").is_some_and(|max| number > max) {
        out.push(issue(
            "maximum",
            &format!("{number} is above the maximum"),
            path,
        ));
    }
    if bound("exclusiveMinimum").is_some_and(|min| number <= min) {
        out.push(issue(
            "exclusiveMinimum",
            &format!("{number} is not above the minimum"),
            path,
        ));
    }
    if bound("exclusiveMaximum").is_some_and(|max| number >= max) {
        out.push(issue(
            "exclusiveMaximum",
            &format!("{number} is not below the maximum"),
            path,
        ));
    }
}

fn check_composition(
    root: &Value,
    keywords: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut Vec<ValidationIssue>,
) {
    let branches = |name: &str| -> Vec<&Value> {
        keywords
            .get(name)
            .and_then(Value::as_array)
            .map(|b| b.iter().collect())
            .unwrap_or_default()
    };
    for branch in branches("allOf") {
        out.extend(check(root, branch, instance, path));
    }
    let any = branches("anyOf");
    if !any.is_empty()
        && !any
            .iter()
            .any(|b| check(root, b, instance, path).is_empty())
    {
        out.push(issue("anyOf", "matches none of the alternatives", path));
    }
    let one = branches("oneOf");
    if !one.is_empty() {
        let matching = one
            .iter()
            .filter(|b| check(root, b, instance, path).is_empty())
            .count();
        if matching != 1 {
            out.push(issue(
                "oneOf",
                &format!("matches {matching} alternatives, exactly one required"),
                path,
            ));
        }
    }
    if let Some(forbidden) = keywords.get("not")
        && check(root, forbidden, instance, path).is_empty()
    {
        out.push(issue("not", "matches a schema it must not", path));
    }
}

fn child(path: &str, name: &str) -> String {
    format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn codes(schema: &Value, instance: &Value) -> Vec<String> {
        check(schema, schema, instance, "")
            .into_iter()
            .map(|i| i.code)
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
        let issues = check(&schema, &schema, &json!({"a/b": 1}), "");
        assert_eq!(issues[0].path.as_deref(), Some("/a~1b"));
    }
}
