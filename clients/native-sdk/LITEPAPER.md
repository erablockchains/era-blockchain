# ERA Blockchain V14 — technical review litepaper

Status: private review draft. V14 is a development candidate. This document does not announce an
upgrade, public repository, deployed application, validator opening, asset launch, liquidity, or
investment return.

## Purpose and architecture

ERA is a Substrate/FRAME-based blockchain. The V14 target is one integrated runtime upgrade with
`spec_version = 14` and `transaction_version = 1`. Consensus remains staking-based with BABE block
production, GRANDPA finality, sessions, validator election, and nominations. GPU and digital-twin
compute is deferred to V15 and must not execute in V14 consensus.

V14 is organized as bounded workstreams whose outputs require integration, migration rehearsal,
benchmarking, metadata compatibility checks, independent security review, and separate activation
authority. A workstream implementation is not evidence that the corresponding feature is deployed.

## Monetary and staking policy

The approved economic model uses native ETKN with 18 decimal places and these binding limits:

- lifetime issuance may never exceed 1,000,000,000 ETKN;
- burns never restore previously consumed issuance allowance;
- the staking issuance target is 10% annualized eligible active stake and is not a guaranteed yield;
- annual gross new issuance is capped at 6,000,000 ETKN;
- new issuance is split 90% to the staking reward pot and 10% to a keyless accumulation-only
  treasury, with persistent carry for exact long-run conservation;
- corrected normal fees are split 90% to staking and 10% to treasury;
- corrected tips use 90% author and 10% treasury when the author can receive them;
- a missing or unreceivable author routes tips 50% to staking and 50% to treasury;
- fee and tip routing transfers already issued ETKN and never changes issuance allowance;
- the issued legacy 20,000,000-ETKN validator/nominator allocation stays separate and is not V14
  funding.

Reward eligibility is performance- and points-based. The minimum validator self-bond is 10,000
ETKN; nominated backing cannot substitute for that self-bond. Validator commission above 20% is
rejected. Neither eligibility nor the 10% target guarantees a validator seat or a return.

Migration retains four active validators. The approved growth direction is four to seven, one
separately gated addition at a time, followed by one qualified validator at a time. There is no
permanent policy cap. Sixteen is only the current technical bound and requires separate bounded,
weight, network, and concentration proof.

## Custody and governance boundary

Presale, ecosystem, and liquidity allocations target three domain-separated deterministic keyless
accounts. The ordinary runtime path is configured with one exact three-founder signer set. The
signer and derived account identities are public inputs; an ordinary withdrawal requires identical
approvals by all three configured AccountIds. Runtime checks do not prove key possession or
operational three-founder control.

For V14, custody temporarily uses the disclosed centralized production Sudo/Root authority.
`sudo_as` remains an explicit system-wide exception to the ordinary 3-of-3 path. Operational and
cryptographically attested 3-of-3 custody, together with removal or strict limitation of the
Sudo/Root bypass, is a mandatory V15 handover gate. The founding vesting policy remains
20,000,000 ETKN allocated as 8M/5M/3M/2M/2M over one year.

## Assets and application foundations

The integrated V14 source contains reachable FRAME-native registered assets, unique/NFTs and a
bounded World registry. All six configured World calls use benchmark-generated production weights.
The versioned asset query contract and constant-product AMM remain dormant source declarations.
No asset, collection, pool, oracle, bridge, privileged origin, deposit, fee, liquidity incentive,
or launch liquidity is created by this review package. ETKN remains the native fee and staking
token.

Contracts and EVM/Frontier are not enabled. Owner-adopted Option C retains native FRAME dApps in V14
and defers true ERC-20 compatibility to a V15 architecture comparison of `pallet-revive` plus its
ERC-20 precompile and integration, Frontier/EVM, and Wasm `pallet-contracts`/PSP22. PSP22 and native
Assets are not ERC-20. This package makes no Solidity, Ethereum RPC, ERC-20, unmodified
Ethereum-DApp, Wasm-contract, bridge, or interoperability promise.

## Client review surface

The included RPC client and sample DApp retain their implemented read-only behavior. A separate
metadata-bound codec can encode and decode selected V14 native calls, construct immortal or mortal
signing payloads, and assemble an opaque extrinsic from a signature supplied by an external signer.
The injected-wallet adapter passes that request through the standard external signer contract. It
cannot itself sign, manage a key, submit an extrinsic, or activate a dormant interface.

The non-authority launcher requires an external, disposable Development/Local chain spec. It binds
P2P and RPC to loopback, selects safe RPC methods, rejects nonempty embedded bootnodes or telemetry,
and refuses a nonempty base path. No chain spec or executable is shipped here.

## Assurance and activation gates

Before any publication or release proposal, the integrated source must pass license and trademark
decisions, provenance and full-history secret review, source freeze, module and workspace tests,
migration rehearsal, generated-weight review, metadata/wallet compatibility, independent security
review, SBOM/third-party notice completion, and two fresh reproducible Wasm builds.

V14-6 migration/testnet execution, V14-7 final assurance, and V14-9 production, website, wallet,
explorer, and listing actions require later authorization. Nothing in this package grants it.

### Native wallet integration status

The V14 review SDK now has an offline-tested adapter for a standard injected Substrate signer. It
constructs metadata-bound immortal or mortal signing requests for the selected reachable native
calls and assembles the returned typed signature into an opaque extrinsic. Tests use only a mocked
provider, synthetic account and synthetic signature. This is client compatibility evidence, not a
live-wallet, submission, public-network or deployment result.
