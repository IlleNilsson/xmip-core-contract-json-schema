# xmip-core-contract-json-schema

The JSON content contract, a technology of
[xmip-core-contract](https://github.com/IlleNilsson/xmip-core-contract).

Two claims. **Well-formedness is a given**: every Stream this contract sees is
parsed, and a Stream that is not JSON fails with the line and column. **Conformance
is a given once the contract is named**: a Receive or Send Location that refers to
this contract with a schema bound has every Stream validated against that schema,
and each departure is reported with the JSON pointer of where it happened and the
keyword that refused it.

The claims are this crate's when it is called, and the System Tests call it.
A node holding a Location's Streams to it is
[decided, not built](../../../../../doc/architecture/estate-map.md#arrival-validation): a node refuses to start a
Location that names a contract until it does.

`src/schema.rs` lists the JSON Schema vocabulary evaluated. Unknown keywords are
ignored, as the specification requires. A schema is compiled once, when it is
bound — every keyword read, every `$ref` resolved, every `pattern` compiled — and
a Stream is held to that tree (`src/check.rs`) without reading the schema again.
The pointer is RFC 6901's, the empty pointer for the whole document, and is
spelled only for an issue raised. `date`, `time` and `date-time` are RFC 3339's,
read by the estate's one calendar, `codec::civil`: the thirty-first of February
is no date. The TOON contract holds its documents to the same compiled schema.

A schema that would never be done is refused when it is bound (`src/chain.rs`).
`$ref`, `allOf`, `anyOf`, `oneOf` and `not` hold the same place in the instance
to another schema; where they come back round without descending, as
`{"$ref": "#"}` does, the refusal names the `$ref` that closes the cycle, and a
chain of them longer than 512 schemas is refused too. The compiler works from a
list, not by recursion, so a long schema is read in the same stack as a short
one. A check is bounded as well: at most 512 schemas deep, and at most the
schema's size times the Stream's bytes plus one steps, so a schema that holds one
place to the same schemas over and over, doubling at each level, stops. Past
either bound the check ends with one issue coded `bound`, at the place it stopped
(ADR-0042, amendment 2026-11-19).

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
