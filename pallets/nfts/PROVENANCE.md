# Local NFT pallet

Copied from polkadot-sdk f3969c7ddd34985e6e709ed458bcc519f651682a, substrate/frame/nfts, version 35.0.0. Original Apache-2.0 headers and license retained. The nested runtime-api is a separate package and is not substituted. Copy checksums and pre-edit scope are in evidence/v14-nft-retirement-20260905.

Local modifications implement the owner-authorized bounded retirement and delegated cleanup. Manifest inheritance is resolved against the same pinned SDK; only pallet-nfts changes source identity.

## Package boundary and support preservation

The separate nested `runtime-api` package is not copied, substituted or registered. `README.md`,
`src/macros.rs`, `src/common_functions.rs`,
`src/impl_nonfungibles.rs` and `src/features/mod.rs` remain byte-for-byte copies. Their module
formatting exclusions preserve the upstream source formatting. The original benchmark support
changes only its four cleanup ranges to the adopted `CLEANUP_LIMIT`; all other fixtures remain
unchanged. The exact pinned SDK lint policy
is materialized locally; dependency inheritance is resolved without upgrading any dependency.

Semantic changes are confined to the owning mutation helpers and their guards, the new
`retirement.rs` engine, additive calls/storage/events/errors/constants, schema-v2 validation,
weight wiring and regression tests. Formatting changes in edited source are recorded separately
from the initial-copy checksums in the external evidence directory.

## Collection-owned storage inventory

| Storage | Treatment |
| --- | --- |
| Item | Zero count plus raw prefix emptiness required at start, continuation and finalization. Never deleted by retirement. |
| ItemMetadataOf | Phase0; independent payer or owner-sentinel refund, checked metadata counter. |
| Attribute | Phase1; canonical raw keys and bounded values, exact recorded payer, checked attribute counter. Positive owner sentinels require CollectionOwner namespace. |
| ItemConfigOf | Phase2; checked configuration counter; locked burned remnants are removable only after terminal retirement starts. |
| ItemPriceOf | Phase3; bounded removal of strictly decoded orphan records. |
| PendingSwapOf | Phase4; bounded removal of strictly decoded orphan records. |
| ItemAttributesApprovalsOf | Phase5; bounded removal of approval sets. |
| CollectionRoleOf | Phase6; bounded removal; direct destroy and team replacement preflight at most three roles plus one lookahead. |
| DelegateCleanup | Phase7; collection-first marker survives burn and is consumed after attributes. |
| CollectionMetadataOf | Fixed scalar; exact owner-aggregate refund, then removal. |
| CollectionConfigOf | Fixed scalar, strictly decoded before finalization. |
| CollectionAccount | Fixed current-owner index, strictly decoded and removed at finalization. |
| Collection | Fixed details and remaining aggregate liability; removed only after completion proofs. |
| CollectionRetirement | Versioned phase marker; removed at successful finalization. |
| Account | Owner-first live-item reverse index. Existing mint/transfer/burn maintain it; retirement cannot scan it by collection. Historical reverse-index consistency remains a snapshot prerequisite. |
| OwnershipAcceptance | Account-owned transfer intent, not a collection-owned liability. Remains account-controlled and may be cleared by its account. No global scan is introduced. |
| NextCollectionId | Global allocator state, unchanged. |

## Work and accounting bounds

`CLEANUP_LIMIT` is immutable. Direct destroy accepts each of its three existing witnesses up to
that ceiling; it can remove at most three times the ceiling plus three role records. Every direct
prefix preflight performs at most witness+1 successor probes. All variable and fixed records are
preflighted before deletion/refund. No typed iterator can skip malformed entries.

Retirement removes at most the accepted limit across all eight phases combined. The processing
loop makes at most limit+8 raw successor probes. Prior-phase validation, the live-item proof and
fixed finalization together keep total successor probes at most limit+18. Each record is decoded
once in a retirement batch. Attribute keys require three canonical concat-key decoding attempts;
other phase keys require one. The direct path decodes its materialized records again while
removing them: at most 2*(m+a+c+3) value decodes and 2*(m+3*a+c+3) key-component decodes,
plus bounded scalar decoding. Exact-prefix existence checks detect malformed prefix values.

Delegated continuation makes at most limit+1 successor probes and one value decode per removed
attribute. Direct cancellation uses at most witness+1 preflight probes plus one final emptiness
probe, and at most twice witness record/value decoding. The first continuation revokes approval
transactionally; each successful continuation removes the marker when the namespace is empty.
Renewal and namespace writes reject while a marker exists, including malformed markers.

Within a batch, traversal advances from the last processed raw key. Between batches, the
remaining prefix is the implicit cursor. Progress stores codec version1 and phase only, never an
arbitrary caller-provided key that could skip an entitlement. Earlier phases must be proven empty
on continuation. This avoids repeating prefix traversal for each deletion.

All cleanup helpers have their own storage transactions, including trait/internal entrypoints.
Every refund checks reserve sufficiency, checked free/reserved arithmetic, the unreserve return
value and exact post-operation balances. Owner-sentinel refunds reduce the aggregate once;
independent named payers never consume it. Any error rolls back previous refunds, counters,
markers and events in that call. Only a remaining non-monetary attribute overcount can be audited
and reconciled, after proven empty prefixes. Other inconsistent counters and all monetary
shortfalls reject without repair.

Burn is independently transactional, uses the same exact refund checks, strictly decodes removed
records and rejects counter underflow. It does not erase pending delegated entitlements. Live-item cleanup follows the current owner
after transfer; after burn, terminal collection retirement is the authorized remaining path.
Ordinary mutation helpers and recreation reject marked collections. Inspection remains available.

## Migration and external prerequisites

The pinned source's storage version is1. The additive schema is2. Before generated Executive
initializers run, strict bounded decoding rejects malformed/unsupported NFT versions and dirty
new prefixes. A missing version is accepted only with a completely pristine NFT namespace.
The owning hook installs2 without scanning historical collections. Version2 replay preserves
existing progress. The coordinated V13 replay accepts this authorized append-only NFT version.

The tests and benchmarks use synthetic externalities. They do not reconcile historical production
reserves, counters or reverse indices. Sudo can still replace outer declared weight through its
existing unchecked-weight privilege; backend bounds do not remove that privilege. This change
neither authorizes deployment nor satisfies the unavailable isolated finality rehearsal.

## Accepted processing and weight gate

The final ceiling is 685. Benchmark Wasm SHA256 `a97a102c69f2d0bf3805feb7543a590daab4a636b98b02466bbd018375526b12`; paired measurements and capture hashes are in `[private-benchmark-artifact-path]`. Generated full-domain envelopes include 25% reference-time margin, summed per-prefix proof bounds and explicit database operations. The runtime dispatch/budget regression checks the actual Normal max_extrinsic in both dimensions. Smaller and oversized requests retain the conservative maximum charge. All temporary Weight::MAX placeholders are removed.

The original SDK v1 migration utility is retained but unregistered. The actual owning hook and Executive use the bounded v2 path.

Historical reverse-index reconciliation covers both owner-first Account entries and stale CollectionAccount entries under prior owners; terminal completion removes the one current-owner CollectionAccount record required by the maintained ownership invariant. No global owner-index scan is introduced.

Ordinary guarded formulas add 24 measured guard envelopes, covering the maximum pre-signed owning call graph (at most 10 attributes and their collection/namespace guards); the envelope also includes complete bounded team work to cover strict role lookup. Burn, team and cleanup methods use their complete owning-path measurements. The fixed finalization cost is included in each terminal phase fixture; a separately measured reconciliation envelope is additionally charged to retirement.

## Exact per-file provenance

The upstream root is `substrate/frame/nfts` at commit `f3969c7ddd34985e6e709ed458bcc519f651682a`. `pallets/nfts/LICENSE` is an exact copy of that revision's `substrate/LICENSE-APACHE2` (SHA-256 `e93d18cb209947d5e75300c9cd9043e1e680034bcd7df0635e1012cc8d8311ab`). Every `MODIFIED_UPSTREAM` file begins with a prominent ERA modification notice. Exact-upstream files remain byte-for-byte unchanged and therefore do not receive a local header.

| File | Classification | Local SHA-256 | Upstream SHA-256 |
| --- | --- | --- | --- |
| `Cargo.toml` | MODIFIED_UPSTREAM | `031053e1e2ef3bcf7f89630b0ce81764abd446a89083be400110b79f70a4d423` | `9fba51ac72b2475e50e846dfb90368c83f73c82a29ae60da0924f0d97f246378` |
| `LICENSE` | LOCAL_ONLY | `e93d18cb209947d5e75300c9cd9043e1e680034bcd7df0635e1012cc8d8311ab` | `NONE` |
| `README.md` | EXACT_UPSTREAM | `56637b0c05825b3781b04f0a75f6d3cf94a08db93a72fcd2efc2e0bee2b8879b` | `56637b0c05825b3781b04f0a75f6d3cf94a08db93a72fcd2efc2e0bee2b8879b` |
| `src/benchmarking.rs` | MODIFIED_UPSTREAM | `f9b9dff58f74b37e6aa95cf6ba8a6c8dc550db986b7213c62991d24fbb6ca4ca` | `eb62dea5458a8c597a4c1851060654366d5c51fd232dc3f42925f301f056c6db` |
| `src/common_functions.rs` | EXACT_UPSTREAM | `6505d6d16508140cc70f1c426d0d979492cfcce8515631e58727b65ad3b8afe1` | `6505d6d16508140cc70f1c426d0d979492cfcce8515631e58727b65ad3b8afe1` |
| `src/features/approvals.rs` | MODIFIED_UPSTREAM | `17c56929882f254ea0c13ae568ece1eab0f3719da410a9462ec0163b2637f498` | `2a6885be95fe862c37cee34ca242dc0aba8b73d653bae97431677cd197c64ad6` |
| `src/features/atomic_swap.rs` | MODIFIED_UPSTREAM | `a2928d9f0d36d25b7f960b30a716ad380640512feaa14e466a79d23571accf74` | `745638742fa9e643773bf5f011801fbd55e72aee230e6857a1b65d14d1121283` |
| `src/features/attributes.rs` | MODIFIED_UPSTREAM | `cbb4b65a19d67d79f1c653f4b386a8ec9107c46a274e6d9fa5d3e401fa886436` | `e840c1d650243e714b562956979d01c43cb92c016dbb78d02e510fd5feaf6316` |
| `src/features/buy_sell.rs` | MODIFIED_UPSTREAM | `7d7fd86eebec092890324cf86a82c745843148d73a8747a0fa372db20cf1220d` | `0ecac697ac82a43f4c06155bbbc9baa0067bf48096f041340da957015115b5a7` |
| `src/features/create_delete_collection.rs` | MODIFIED_UPSTREAM | `e6534887a72dcf6650215e8d8a8625f992ff9095e6fe1aed13885dcbbdab27ba` | `d63e0caa51eb3fb84955a4906fc6c180ce7df2a4cbb7ca7821d1efe051e4317b` |
| `src/features/create_delete_item.rs` | MODIFIED_UPSTREAM | `e115c2dd9b3a6d1de9f630d67a7380521005852ba8464b0941d527c54e6a2922` | `bbad5edd19b42672e1eff4545af183a67032010a5af36fadd7af2a67e6812803` |
| `src/features/lock.rs` | MODIFIED_UPSTREAM | `36a29abede6e6e3c9fcc68d6c54a86daa052a9930ae0c091477aab8272a3ec03` | `56caf4c4bbedb7bd19ae1e5698b14108a73a58e6bd7d4d7e563ec7de97be2068` |
| `src/features/metadata.rs` | MODIFIED_UPSTREAM | `c4731a0999d1e4ff7c8e819097cc3c446e4bae2744aca59a1747ee2808620b29` | `943b098dedd0a40a00aaccfda53f475b2ba9fe5f6da9ef6ba693d039732eb9a3` |
| `src/features/mod.rs` | EXACT_UPSTREAM | `46c9e65b58d694d9706434864c71c8fb6e1a926791e5fa10d1740eb73d87d3b6` | `46c9e65b58d694d9706434864c71c8fb6e1a926791e5fa10d1740eb73d87d3b6` |
| `src/features/roles.rs` | MODIFIED_UPSTREAM | `31f774d2197e7482093a69b90d2b9df073a0eb8b8cef564c9e7b38db48efe3a2` | `ec47ed83e938a7ee6f6a301045fffb9a293e5fd275efe6a8b00ed6f1aea6529f` |
| `src/features/settings.rs` | MODIFIED_UPSTREAM | `081de886ee91be4deef921118bf62cff19a1134664b291ca73720a8383e9d28f` | `6bdff8fbeac6cc9c9113ea922c7fefb4c9af9dc460a171a0457765c9ee6fb855` |
| `src/features/transfer.rs` | MODIFIED_UPSTREAM | `8aa97915d8a0971417590b6f4ac77d64b29851d5a59f3e9b709b2818253beb96` | `57f2d7a7cb26d26669c78a5acb63a5e9e0002a03395a89db0f3920ee28adec07` |
| `src/impl_nonfungibles.rs` | EXACT_UPSTREAM | `758a6340273f8b10e17db81d9170668ede5ddde1853a50addeaf050a6f160fae` | `758a6340273f8b10e17db81d9170668ede5ddde1853a50addeaf050a6f160fae` |
| `src/lib.rs` | MODIFIED_UPSTREAM | `98b895518c109db7cfa595e700d69e83e4f11976423e55968507d4b994579284` | `a232170e68ad4c3f2ddad408f177683a9d1023c0e48348fe6871dba735436538` |
| `src/macros.rs` | EXACT_UPSTREAM | `13ea84ca3fbd589a7170235d343cd1b0c15b39ff9515c8e57fd479def830ad61` | `13ea84ca3fbd589a7170235d343cd1b0c15b39ff9515c8e57fd479def830ad61` |
| `src/migration.rs` | MODIFIED_UPSTREAM | `f51d0b347133c8e8f29b5ffd1200451e28433e1e1a3167f7511e442268c0b676` | `a5c2c4ab276a50d26cf0825734169d950b2a031de7d553009cfe6381fed0ca80` |
| `src/mock.rs` | MODIFIED_UPSTREAM | `049d25fe91399aab1f9601e925b056718ae8c2359d4a63f723155d2d99681787` | `7882dba9f8bf7cb9c68ddbd7f661909a56f377ca947799d523a43bddcef61c6f` |
| `src/retirement.rs` | LOCAL_ONLY | `caaa9ed08cb64122deaf81bffc0bcfc46f90db634ffc2e0622078d59a67f122b` | `NONE` |
| `src/tests.rs` | MODIFIED_UPSTREAM | `bd477fb5a331e23473e47be072de385a0cbfe96a1dee774d4f92b6f897e35c5d` | `efcff784fc3f9ecc37f0bf09ac5c34445c6fb30a2adf6ef79b83008d99d5563a` |
| `src/types.rs` | MODIFIED_UPSTREAM | `cc54cb95eb065c3626fd8a3a794e0b061c06b3ca9b9324a8b5fb1cc07a13bc09` | `d705cf5784c77c5cdfc5b53e04ea6e9797fa707e7532f761f40b8dd61a1cabab` |
| `src/weights.rs` | MODIFIED_UPSTREAM | `d234292bb42d2cbefbd7636a8092f2e844ba768d34b2d366c16ef91eb00a4718` | `b66077ab9aea3da9dcf2b1cc672424ad2b3c7bbda94639045628c3756528f60b` |

## V14 allocator owning guards — 20 September2026

The owning item-creation, role and mint-settings helpers now consult configurable managed-allocation guards. Runtime wiring protects admitted collections even through bypass-filter entry points. Mock configuration uses unrestricted item creation and no managed collections, preserving upstream test semantics. No upstream licence or package boundary changes.
