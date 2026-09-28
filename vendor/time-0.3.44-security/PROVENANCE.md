# time 0.3.44 security fork provenance

Base source is the exact crates.io `time-0.3.44.crate` archive, SHA-256 `91e7d9e3bb61134e77bde20dd4825b97c010155709965fedf0f49bb138e52a9d`. Frozen advisory database: RustSec commit `bf25f6575a93a35f30796c65c0ed91bee7fa19fd`, deterministic archive SHA-256 `bbe672c0efe782e4a4cfc499cf9de0f023a2901af90eaafcd784693dac4e88de`.

The fixed reference is the exact crates.io `time-0.3.47.crate` archive, SHA-256 `743bd48c283afc0388f9b8827b976905fb217ad9e647fae3a379a9283c4def2c`. Its transitive requirements are incompatible with the pinned ERA toolchain, so the fixed release's RFC 2822 depth limit was backported without its dependency-family update.

Modified upstream file:

- `src/parsing/combinator/rfc/rfc2822.rs` — RUSTSEC-2026-0009 depth-32 recursive-comment limit and regression coverage; SHA-256 `2a1544ae6bd08df3c8af2e73492544f18b9c337e7c65142c80a3a8d5f1335a2c`.

Every other archive file is byte-identical. `.cargo-ok` is an empty Cargo cache extraction marker. Package identity, dependencies and declared `MIT OR Apache-2.0` license are unchanged.
