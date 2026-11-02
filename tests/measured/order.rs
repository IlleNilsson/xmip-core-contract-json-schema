//! The order schema a bound schema is tested with: what the crate's tests
//! and its timing program both validate against. Shared, not copied:
//! `src/lib.rs`'s tests take it with `#[path]`.

/// An order: an id and at least one line, each a sku and a quantity.
pub fn order_schema() -> serde_json::Value {
    serde_json::json!({
        "$id": "order",
        "type": "object",
        "required": ["id", "lines"],
        "properties": {
            "id": { "type": "string", "minLength": 1 },
            "lines": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "required": ["sku", "qty"],
                    "properties": {
                        "sku": { "type": "string" },
                        "qty": { "type": "integer", "minimum": 1 }
                    }
                }
            }
        },
        "additionalProperties": false
    })
}
