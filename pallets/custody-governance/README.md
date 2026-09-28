# ERA V14 three-category founding custody

The active FRAME pallet controls Presale, Ecosystem, and Liquidity balances held by three distinct,
deterministic, keyless pallet subaccounts. The runtime reuses the existing `era/vamm` `PalletId`
with version `2`, subdomain `founding`, and the SCALE category discriminant. Version `1` remains
reserved for the dormant AMM derivation, so the namespaces do not collide.

Each category has an independent request counter and at most one pending withdrawal. A withdrawal
proposal binds its category, request ID, destination, and nonzero amount. The first and second
distinct configured signers record approvals only. The third matching approval transfers the exact
amount atomically and advances that category's request counter. Duplicate approvals, changed
proposal fields, malformed approval state, insufficient reducible balance, wrong request IDs, and
unauthorized signers fail closed. Category accounts and configured signer accounts are forbidden
destinations.

The configured signer AccountIds are exact, unique, canonically byte-sorted sr25519 public
identities. There is no pallet call for Root, force-withdrawal, threshold reduction, signer change,
proxy, recovery, cancellation, or alternate delegate. Standard Multisig and Proxy pallets remain
in the runtime only for compatibility and historical rehearsal; neither controls these accounts.

The runtime's operational Sudo/Root authority is intentionally retained. In particular,
`Sudo::sudo_as` can construct a signed origin for a configured signer and is therefore an explicit
system-wide bypass of ordinary custody authorization. This exception is disclosed, tested, and
must not be described as keyless or strict 3-of-3 security against Root.

Fresh V14 genesis initializes the three accounts and approved category allocations directly. Historical V13 migration code remains for source continuity; this deployed chain did not import historical state.

The legacy dependency-free policy/preflight types in `src/lib.rs` and synthetic SDK derivation
tests are retained for historical evidence and vesting arithmetic. They are not the active custody
authority.

The three founder identities and ordinary three-approval custody policy were confirmed for the fresh relaunch. This is an owner-confirmed operating posture, not a claim of independent signer-control attestation. Retained Sudo powers and any future limitation/removal remain explicit governance boundaries. See [current feature status](../../docs/FEATURE-STATUS.md).
