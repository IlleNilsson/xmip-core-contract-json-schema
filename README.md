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
ignored, as the specification requires.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
