# ERA-MAINNET V13 baseline and known gaps

## Deployed baseline identity

| Item | Value |
| --- | --- |
| Chain | `ERA-MAINNET` |
| Genesis hash | `0xba96ed0fe6c37790ee7da7ed9e83e9630b29fc8ba66e6753dc2e5c5704aa94e7` |
| Runtime spec version | `13` |
| Transaction version | `1` |
| Original source commit | `77eb28b519d2f83796e65d402a62671a8c777c83` |
| Original source tree | `1b0d6a42f1c6dbe091391474fd4b72445a5b2fdb` |
| Deployed compressed Wasm size | 997,122 bytes |
| Deployed compressed Wasm SHA-256 | `ecca5baddc60d4e8522c8ea6206fec0c29175d59ddf2359e6d43201f1faa44a2` |
| V13 activation block | `2,071,915` |
| V13 migration block | `2,071,916` |

The V13 monetary correction retained exactly `100,000,000 ETKN` issuance. The absolute lifetime cap
is `1,000,000,000 ETKN`, leaving a maximum post-correction issuance allowance of
`900,000,000 ETKN`. The baseline preserves continuity of the four-authority validator set.

## Accurate staking status

Staking remains live; it was not removed or globally retired.

- `pallet_staking` remains present in the runtime and its signed calls are available.
- Staking selects the four current authorities. Eras and elections advance normally.
- Bonding, nomination, unbonding, withdrawal, and authored reward-point accounting remain
  operational.
- The runtime configures `EraPayout=()`. Retained `ErasValidatorReward` entries are therefore zero,
  and validator/nominator era rewards are not currently funded.
- V13 force-unstaked only community/nominator account
  `5EHxtNeMeLs22XsazYiJ7BSBnntT8CiRBGev8JKw21umYj6r`: `270 ETKN` total, comprising
  `220 ETKN` active and `50 ETKN` unlocking.
- No explicit owner authorization exists for global staking retirement.

The approved future validator eligibility threshold is `10,000 ETKN`. Eligibility does not
guarantee an active validator seat. The approved validator/nominator reward allocation is
`20,000,000 ETKN` within the Airdrop/Validators category. Its payout implementation remains V14
work; this V13 baseline does not claim that those rewards are implemented or paid.

## Reproducible source and Wasm verification

The repository pins Rust `1.87.0`, the `wasm32-unknown-unknown` target, `rustfmt`, and `clippy` in
`rust-toolchain.toml`. Dependencies are locked in `Cargo.lock`; Substrate/FRAME dependencies are
pinned to the `polkadot-v1.19.1` tag and the exact resolved revisions in that lockfile.

Use a clean, isolated checkout with no production keys, databases, chain state, or endpoints. A
one-time dependency/toolchain acquisition may use the network only after separate review. Confirm
that `Cargo.lock` remains unchanged, then disconnect or enforce Cargo offline mode for all checks.

PowerShell setup and verification:

```powershell
rustup toolchain install 1.87.0 --profile minimal --component rustfmt,clippy --target wasm32-unknown-unknown
cargo fetch --locked
git diff --exit-code -- Cargo.lock
$env:CARGO_NET_OFFLINE = 'true'
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test -p era-runtime --features try-runtime --locked --offline
cargo clippy --workspace --all-targets --locked --offline
cargo build --workspace --release --locked --offline
cargo build -p era-runtime --release --locked --offline
$wasm = 'target\release\wbuild\era-runtime\era_runtime.compact.compressed.wasm'
(Get-Item -LiteralPath $wasm).Length
(Get-FileHash -LiteralPath $wasm -Algorithm SHA256).Hash.ToLowerInvariant()
```

The final two commands must report `997122` and
`ecca5baddc60d4e8522c8ea6206fec0c29175d59ddf2359e6d43201f1faa44a2`. A mismatch is a failed
reproducibility result and must not be explained away or used for deployment.

## Private vulnerability reporting

Do not open a public issue. Use the private draft-advisory workflow and isolated-testing rules in
[`SECURITY.md`](../../SECURITY.md). If the private advisory form is unavailable, contact an
`erablockchains` organization owner through the pre-agreed private channel without including exploit
details in the access request.

## Known gaps and non-claims

- This import was created from an audited tracked-source tar, not the original `.git` object store.
  The original commit and tree are provenance identities; the private review commit is a new commit.
- The tar normalized Git mode metadata. See `SOURCE_PROVENANCE.md` for the exact archive record and
  limitation.
- A fresh Wasm reproduction still must match the deployed size and SHA-256 exactly. Historical test
  evidence or source identity alone is not a substitute for a new reproducibility result.
- The historical root `LICENSE` text is the Unlicense while workspace manifests declare
  `Apache-2.0`. This import preserves both signals and does not decide the licensing question.
- Historical auxiliary files such as `runtime/src/lib.rs.save`, `runtime/src/lib`, and
  `node/src7service.rs` were tracked in the audited archive and are retained for provenance. They are
  not presented as active module entry points.
- The `20,000,000 ETKN` validator/nominator reward allocation and its payout mechanism are V14 work,
  not a statement of current reward payment.
- Nothing in this baseline authorizes or performs a mainnet query, extrinsic, runtime upgrade,
  validator change, signing operation, custody action, or production deployment.
