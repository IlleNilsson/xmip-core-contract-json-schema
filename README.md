# xmip-core-contract-json-schema

The JSON content contract, a technology of
[xmip-core-contract](https://github.com/IlleNilsson/xmip-core-contract).

Two claims. **Well-formedness is a given**: every Stream this contract sees is
parsed, and a Stream that is not JSON fails with the line and column. **Conformance
is a given once the contract is named**: a Receive or Send Location that refers to
this contract with a schema bound has every Stream validated against that schema,
and each departure is reported with the JSON pointer of where it happened and the
keyword that refused it.

`src/schema.rs` lists the JSON Schema vocabulary evaluated. Unknown keywords are
ignored, as the specification requires. A schema is compiled once, when it is
bound — every keyword read, every `$ref` resolved, every `pattern` compiled — and
a Stream is held to that tree (`src/check.rs`) without reading the schema again.
The pointer is RFC 6901's, the empty pointer for the whole document, and is
spelled only for an issue raised. `date`, `time` and `date-time` are RFC 3339's,
read by the estate's one calendar, `codec::civil`: the thirty-first of February
is no date. The TOON contract holds its documents to the same compiled schema.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
