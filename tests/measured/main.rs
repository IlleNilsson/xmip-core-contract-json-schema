//! The module's timing program (`doc/governance/rust-style.md` section
//! 5c): a bound schema held to a time. A plain `cargo test` leaves it out;
//! the landing runs it after every other test of the module, alone, one
//! test at a time (`cargo test --test measured --all-features --
//! --test-threads=1`), so what is measured is Xmip's work and not its
//! neighbors':
//!
//! - a bound schema is read once, so a message costs only its own walk.

mod order;

use contract::Contract;
use contract::fixture::stream_as as stream;
use order::order_schema;
use xmip_core_contract_json_schema::JsonSchema;

#[test]
fn a_bound_schema_is_read_once_so_a_message_costs_only_its_own_walk() {
    let bound = JsonSchema::with_schema(&order_schema()).expect("a schema");
    let order = stream(r#"{"id":"A1","lines":[{"sku":"X","qty":2}]}"#, None);
    let started = std::time::Instant::now();
    for _ in 0..10_000 {
        assert!(bound.validate(&order).expect("validates").valid);
    }
    // Generous for a debug build on a loaded machine; a schema read per
    // message, or a lock per pattern, is what it catches.
    let each = started.elapsed() / 10_000;
    assert!(
        each < std::time::Duration::from_micros(500),
        "{each:?} a message"
    );
}
