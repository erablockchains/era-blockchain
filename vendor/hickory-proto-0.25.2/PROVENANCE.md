# hickory-proto 0.25.2 security fork provenance

Base source is the exact crates.io `hickory-proto-0.25.2.crate` archive, SHA-256 `f8a6fe56c0038198998a6f217ca4e7ef3a5e51f46163bd6dd60b5c71ca6c6502`. Frozen advisory database: RustSec commit `bf25f6575a93a35f30796c65c0ed91bee7fa19fd`, deterministic archive SHA-256 `bbe672c0efe782e4a4cfc499cf9de0f023a2901af90eaafcd784693dac4e88de`.

The RUSTSEC-2026-0119 backport is bound to Hickory PR 3615 and exact fix commits `3dcbdbcb02783de54d522382dc39a0d3207424b2` and `f55d3690d32b29e241f1f2f3418f61ac21967441`; the sealed PR/commit/file response hashes are `93383c2ca0ff7ad238d02d9621c8e1a93656753924e2e40289c33cc5bc26d782`, `024531b743c1c1f19dfe66388dffb2402fe8a882340842c719b5bce40ebae143` and `bea69c5d9e9bbfe371b85f702d97e79f32d45c1a753be3818913fb03de295329`.

Modified upstream files:

- `src/rr/domain/name.rs` — bounded compression-candidate search and tests; SHA-256 `5f8e02a1d08e2dcc768efc465e2273c0496e72c6c3adc72c6e4d4b6ae7381bf7`.
- `src/serialize/binary/encoder.rs` — bounded stored-name pointers and tests; SHA-256 `ff74a29f43feacb941985f189046d4294733b0a55d52861e3fedb876a07dfff7`.

Every other archive file is byte-identical. `.cargo-ok` is an empty Cargo cache extraction marker. Package identity, dependencies and declared `MIT OR Apache-2.0` license are unchanged. RUSTSEC-2026-0118 is separately guarded: its affected `dnssec-ring` and `dnssec-aws-lc-rs` features must remain absent from every release surface.
