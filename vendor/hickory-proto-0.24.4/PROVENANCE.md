# hickory-proto 0.24.4 security fork provenance

Base source is the exact crates.io `hickory-proto-0.24.4.crate` archive, SHA-256 `92652067c9ce6f66ce53cc38d1169daa36e6e7eb7dd3b63b5103bd9d97117248`. Frozen advisory database: RustSec commit `bf25f6575a93a35f30796c65c0ed91bee7fa19fd`, deterministic archive SHA-256 `bbe672c0efe782e4a4cfc499cf9de0f023a2901af90eaafcd784693dac4e88de`.

The backport is bound to Hickory PR 3615. The sealed API response hashes are `93383c2ca0ff7ad238d02d9621c8e1a93656753924e2e40289c33cc5bc26d782` (PR), `024531b743c1c1f19dfe66388dffb2402fe8a882340842c719b5bce40ebae143` (commits) and `bea69c5d9e9bbfe371b85f702d97e79f32d45c1a753be3818913fb03de295329` (files). Exact upstream fix commits are `3dcbdbcb02783de54d522382dc39a0d3207424b2` and `f55d3690d32b29e241f1f2f3418f61ac21967441` (with `d41d83ce19fe964ae418bea825720204d4754309` for disabled-compression behavior and `17821c224bbbe1c6f9a369781ba6ad5c5a39534e` for benchmark coverage).

Modified upstream files:

- `src/rr/domain/name.rs` — bounded compression-candidate search and tests; SHA-256 `7a148c4aa6f354c19b3c45db9128fdb11686e5116673f5611d1673ab0fce3d7d`.
- `src/serialize/binary/encoder.rs` — bounded stored-name pointers and tests; SHA-256 `c81f0e3d7c78446c92ced0bf77ef5e2f6c30cd1fc3ad17a539bd4f1947410784`.

Every other archive file is byte-identical. `.cargo-ok` is an empty Cargo cache extraction marker. Package identity, dependencies and declared `MIT OR Apache-2.0` license are unchanged. The permanent cross-version smoke suite covers normal wire bytes and 200 shared-suffix names.
