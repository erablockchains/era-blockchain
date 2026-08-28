# ERA Blockchain — deployed V13 private review baseline

This private repository contains the tracked source baseline associated with the deployed
ERA-MAINNET V13 runtime. The original source identity is commit
`77eb28b519d2f83796e65d402a62671a8c777c83`, tree
`1b0d6a42f1c6dbe091391474fd4b72445a5b2fdb`. The deployed compressed runtime Wasm is 997,122 bytes
with SHA-256 `ecca5baddc60d4e8522c8ea6206fec0c29175d59ddf2359e6d43201f1faa44a2`.

The commit in this private repository is a review/import commit. It is not represented as the
original historical source commit. See
[`docs/review/SOURCE_PROVENANCE.md`](docs/review/SOURCE_PROVENANCE.md) for the import record and
[`docs/review/V13_MAINNET_BASELINE_AND_KNOWN_GAPS.md`](docs/review/V13_MAINNET_BASELINE_AND_KNOWN_GAPS.md)
for deployed facts, staking status, known gaps, and reproducible verification instructions.

## Review commands

Use the pinned Rust 1.87.0 toolchain and `Cargo.lock`. After a separately reviewed dependency fetch,
run the checks offline:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test -p era-runtime --features try-runtime --locked --offline
cargo clippy --workspace --all-targets --locked --offline
cargo build --workspace --release --locked --offline
```

Never use production keys, node databases, chain data, or production endpoints for development or
review. Nothing in this repository authorizes a runtime deployment, live-chain query, signed
extrinsic, validator change, or custody action.

## Staking status

Staking is live and was not globally retired. `pallet_staking` remains in the runtime; its signed
calls and normal bonding, nomination, unbonding, withdrawal, election, era, and authored reward-point
flows remain available. `EraPayout=()`, so era reward records are zero and validator/nominator era
rewards are not currently funded. Future validator/nominator reward economics remain V14 work.

## Security and licence

Report vulnerabilities privately as described in [`SECURITY.md`](SECURITY.md). Do not open a public
issue containing vulnerability details.

The imported historical tree retains the original root `LICENSE` and `THIRD_PARTY_NOTICES.md`.
Workspace manifests declare `Apache-2.0`, while the root licence text is the Unlicense; this
pre-existing inconsistency is recorded as a known gap and is not resolved by this review import.
