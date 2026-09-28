# ruint 1.15.0 security fork provenance

Base source is the exact crates.io `ruint-1.15.0.crate` archive, SHA-256 `11256b5fe8c68f56ac6f39ef0720e592f33d2367a4782740d9c9142e889c7fb4`. Frozen advisory database: RustSec commit `bf25f6575a93a35f30796c65c0ed91bee7fa19fd`, deterministic archive SHA-256 `bbe672c0efe782e4a4cfc499cf9de0f023a2901af90eaafcd784693dac4e88de`.

The fixed reference is the exact crates.io `ruint-1.20.0.crate` archive, SHA-256 `f5e99bff0393163bb25029a6af25d3d8d202ba5b5438a74d1bd8789f5c822970`. Its Rust requirement is incompatible with the pinned ERA toolchain, so only the two security corrections were adapted to the 1.15 implementation.

Modified upstream files:

- `src/algorithms/div/reciprocal.rs` — RUSTSEC-2025-0137 release-mode precondition enforcement and regression; SHA-256 `55bd1a140adcae96808911fab8655d5c6b8c56009c9b42f79df3f8d9e318670e`.
- `src/bits.rs` — RUSTSEC-2026-0220 discarded-limb/top-bit overflow accounting, non-truncating large `Uint` shift conversion, and regressions; SHA-256 `5f8e51fb3c43cf19fe1ce009322233845cb08d281971f9249e2e998210108860`.

Every other archive file is byte-identical. `.cargo-ok` is an empty Cargo cache extraction marker. Package identity, dependencies and declared `MIT` license are unchanged. Both debug and release-mode permanent smoke tests exercise the affected behavior.
