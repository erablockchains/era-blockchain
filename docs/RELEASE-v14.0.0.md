# ERA Blockchain V14.0.0 release notes

This document describes the proposed `v14.0.0` source release. The tag and GitHub release do not exist until separately approved and published.

## Deployed identity

- Network: ERA; genesis `0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e`.
- Runtime: `era` specification 15, transaction version 1.
- Accepted compressed Wasm SHA-256: `122af167022227c46b2b74d99f5d3a73f41f8d65bb4a8a1de7b88b6e986de2af`.
- Accepted runtime metadata SHA-256: `c188b00f3589677fe7ecfa833d92d884858ad642a6d231d52696298edbe91d40`.
- Accepted node SHA-256: `fab3fb82f6f6de2fe1be02b9b6916f67413eb8b4e15a4eedca3852700e0536ad`.

The authenticated node release is accepted on all nine production hosts. The spec15 upgrade finalized once at block 118480. The deployed-source commit/tree/archive bindings are recorded in [source provenance](PROVENANCE.md) and `provenance/completion-development.json`.

## Scope and status

V14 deploys native ETKN transfers, BABE/GRANDPA consensus, staking, capped SecurityBudget accrual, founder vesting and custody, native Assets/NFT primitives, the allocator/API runtime capability, World registry, and restrictive AI onboarding controls. The dedicated ERA Metadata Service serves the four approved objects over HTTPS.

Production allocator initialization, NFT funding/mint/ownership/sale/swap commissioning, the first legitimate SecurityBudget reward payment, and penalty-policy activation remain ordered V14 acceptance work. Source presence and synthetic tests do not claim those actions occurred. AMM and AI predictive-tokenization/model-service commissioning remain inactive and are deferred to V15. Sentry/private metadata storage is outside V14.

Website R6 and the separately built native Windows/Android wallet downloads are outside this source archive. Android evidence is owner-performed. Checks for update/account preservation, ERA/spec15 connectivity, receiving QR switching and scanner behavior passed. A connection-readiness correction is prepared as Android build4 after build3 stopped before displaying reward eligibility; the affected read-only reward check remains open. Independent qualification and final programme acceptance are not claimed by this source release.

## Release contents and limits

This is a source-only release. It does not publish a newly reproduced or signed node binary. The tree contains the deployed runtime/node source snapshot, reviewed dependency backports, development-only clients and bounded test tools, public provenance, licence material and operator documentation. It excludes production keys, wallet secrets, private operational evidence, chain databases and private repository history.

Two retained lockfile versions match public dependency advisory ranges. See [dependency advisory status](DEPENDENCY-ADVISORIES-20260919.md). Review [security reporting](../SECURITY.md), [build instructions](BUILD.md), [validation limits](VALIDATION.md) and [operator guidance](OPERATORS.md) before use.
