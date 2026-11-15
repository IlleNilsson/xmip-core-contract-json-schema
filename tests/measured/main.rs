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
    // Each message timed, the fastest of five (`xcore::latency::spread`): a
    // schema read per message, or a lock per pattern, is in every one, a
    // busy machine's preemption in some. It waits on no wake, so no plain
    // wake is measured beside it. Until 2026-11-15 the ten thousand were
    // timed together.
    let each = (0..10_000)
        .map(|_| {
            let started = std::time::Instant::now();
            let valid = bound.validate(&order).expect("validates").valid;
            let took = started.elapsed();
            assert!(valid);
            took
        })
        .collect();
    let each = xcore::latency::spread("a conforming order validated", each);
    assert!(
        each.fastest_of_five < std::time::Duration::from_micros(500),
        "{each:?} a message"
    );
}
