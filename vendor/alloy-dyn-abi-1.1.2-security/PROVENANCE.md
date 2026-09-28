# alloy-dyn-abi 1.1.2 security fork provenance

Base source is the exact crates.io `alloy-dyn-abi-1.1.2.crate` archive, SHA-256 `18cc14d832bc3331ca22a1c7819de1ede99f58f61a7d123952af7dde8de124a6`, corresponding to the frozen registry identity `alloy-dyn-abi@1.1.2`. Frozen advisory database: RustSec commit `bf25f6575a93a35f30796c65c0ed91bee7fa19fd`, deterministic archive SHA-256 `bbe672c0efe782e4a4cfc499cf9de0f023a2901af90eaafcd784693dac4e88de`.

The fixed reference is the exact crates.io `alloy-dyn-abi-1.4.1.crate` archive, SHA-256 `3fdff496dd4e98a81f4861e66f7eaf5f2488971848bb42d9c892f871730245c8`. The backport implements the fixed release's empty-linearization rejection without adopting the incompatible 1.4 dependency family.

Modified upstream file:

- `src/eip712/resolver.rs` — RUSTSEC-2025-0073 / CVE-2025-62370 typed-error fix and regression coverage; SHA-256 `f51218ee67ad3a43fd241727fa722b2053a631e223fc500cbe3790b10559420d`.

Every other archive file is byte-identical. `.cargo-ok` is an empty Cargo cache extraction marker and contains no source or executable content. Package identity, public API, dependencies and declared `MIT OR Apache-2.0` license are unchanged. This local fork is limited to the active affected crate and required security regression.
