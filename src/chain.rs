//! The chains of schemas one place in an instance is held to: a schema's
//! `$ref`, `allOf`, `anyOf`, `oneOf` and `not` each hold the same place to
//! another schema ([`Node::in_place`]), without descending.
//!
//! When a schema is bound, those edges are ordered by Kahn's algorithm,
//! without recursion. A schema whose edges come back round would hold a
//! place to itself for ever: it is refused, naming the `$ref` that closes
//! the cycle. So is one that holds a place to a chain of more than
//! [`DEPTH`] schemas. A check goes no deeper than [`DEPTH`] either
//! ([`crate::check`]).

use crate::schema::{Keywords, Node};
use contract::ContractError;
use serde_json::Value;

/// The most schemas one place in an instance is held to at once: the
/// longest chain of `$ref`, `allOf`, `anyOf`, `oneOf` and `not` a schema is
/// bound with, and the deepest a check's walk goes before it stops.
pub const DEPTH: usize = 512;

/// Kahn's algorithm over the edges [`Node::in_place`] names: the nodes
/// ordered so each comes after every node holding its place to it, and the
/// longest chain to each counted on the way. A chain past [`DEPTH`] is
/// refused; so are nodes left unordered, which wait on a cycle, naming the
/// `$ref` that closes it. `sources` is the schema each node was compiled
/// from, by id.
pub(crate) fn ordered(nodes: &[Node], sources: &[&Value]) -> Result<(), ContractError> {
    let mut waiting = vec![0_usize; nodes.len()];
    for to in nodes.iter().flat_map(Node::in_place) {
        waiting[to] += 1;
    }
    let mut ready: Vec<usize> = (0..nodes.len()).filter(|&id| waiting[id] == 0).collect();
    // The most schemas a chain ending at each node holds its place to.
    let mut longest = vec![1_usize; nodes.len()];
    let mut placed = 0;
    while let Some(id) = ready.pop() {
        placed += 1;
        if longest[id] > DEPTH {
            return Err(ContractError::new(format!(
                "the schema holds one place in the instance to a chain of more than {DEPTH} \
                 schemas through $ref, allOf, anyOf, oneOf and not, past the {DEPTH} Xmip follows"
            )));
        }
        for to in nodes[id].in_place() {
            longest[to] = longest[to].max(longest[id] + 1);
            waiting[to] -= 1;
            if waiting[to] == 0 {
                ready.push(to);
            }
        }
    }
    if placed == nodes.len() {
        return Ok(());
    }
    let reference = sources[closing(nodes, &waiting)]
        .get("$ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Err(ContractError::new(format!(
        "the schema's $ref '{reference}' closes a cycle that holds one place in the instance \
         to itself without descending, so a Stream held to it would never be done"
    )))
}

/// The node whose `$ref` closes a cycle among the nodes [`ordered`] left,
/// those still `waiting`. Each of them waits on another of them, so walking
/// back from the first comes round; of the cycle it comes round on, read
/// from the node compiled first, the last edge that is a `$ref` closes it,
/// and one is, as only a `$ref` reaches a schema not inside the one naming
/// it.
fn closing(nodes: &[Node], waiting: &[usize]) -> usize {
    let left = |id: usize| waiting[id] > 0;
    let mut before = vec![0_usize; nodes.len()];
    for (from, node) in nodes.iter().enumerate().filter(|&(id, _)| left(id)) {
        for to in node.in_place().filter(|&to| left(to)) {
            before[to] = from;
        }
    }
    let mut walked = vec![false; nodes.len()];
    let mut at = (0..nodes.len()).find(|&id| left(id)).unwrap_or_default();
    while !walked[at] {
        walked[at] = true;
        at = before[at];
    }
    let mut cycle = vec![at];
    let mut back = before[at];
    while back != at {
        cycle.push(back);
        back = before[back];
    }
    cycle.reverse();
    let first = (0..cycle.len())
        .min_by_key(|&i| cycle[i])
        .unwrap_or_default();
    cycle.rotate_left(first);
    (0..cycle.len())
        .rev()
        .map(|i| (cycle[i], cycle[(i + 1) % cycle.len()]))
        .find(|&(from, to)| {
            nodes[from]
                .keywords()
                .and_then(Keywords::reference)
                .is_some_and(|target| target == to)
        })
        .map_or(cycle[0], |(from, _)| from)
}
