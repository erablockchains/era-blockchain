# ERA Phase 6B signed smoke tool

This is a standalone development tool. It is not a member of the ERA Cargo workspace and adds no dependency to the runtime, node, or pallet crates.

Safety properties:

- RPC is hard-coded to `ws://127.0.0.1:12944`.
- Only Substrate's public `//Alice` and `//Bob` development keypairs are used.
- Evidence is written only to `/tmp/era-phase6b-full-signed-smoke-evidence`.
- It must never be used with ERA-MAINNET, production ports, production paths, or private keys.

Build and run after starting the prescribed isolated development node:

```bash
cargo check --manifest-path tools/phase6b-signed-smoke/Cargo.toml --offline --locked
cargo run --manifest-path tools/phase6b-signed-smoke/Cargo.toml --offline --locked
```

The tool requires a fresh dev chain because it uses model, prediction, and token ID 0. It submits and finalizes 17 signed transactions, records per-extrinsic events and storage/balance snapshots, verifies spec version 9, and treats a successful duplicate payout claim as a fatal failure.
