# ERA V14 public-review surface — private draft

Status: **private preparation only**. This package is integration-ready review material, not a
publication, release, deployment, audit result, or statement about a live network.

This deliberately small surface contains:

- a scope-accurate V14 litepaper;
- a dependency-free JavaScript client with named read-only RPC operations and metadata-bound V14
  native SCALE call/signing-payload/opaque-extrinsic codecs;
- a static sample DApp that displays node and runtime identity only after a user clicks Connect;
- a fail-closed non-authority node launcher for a separately reviewed disposable chain spec;
- provenance, licensing, public/private-boundary, security, and review notes;
- offline tests and sanitization scans.

It contains no chain specification, runtime Wasm, node binary, account address, key, credential,
bootnode, database, production endpoint, private topology, generated bundle, or proprietary engine.

## Review locally

Prerequisites: Node.js 20 or newer, a POSIX shell, and ripgrep. No package installation is required;
the package has no third-party JavaScript dependencies.

```sh
node --test tests/*.test.mjs
sh tools/scan-review-surface.sh
```

To view the sample DApp without contacting any network, serve this directory on loopback and open
`http://127.0.0.1:8080/sample-dapp/`. It remains idle until Connect is selected.

```sh
python3 -m http.server 8080 --bind 127.0.0.1
```

A disposable node is optional. Read [`docs/NODE_DEPLOYMENT.md`](docs/NODE_DEPLOYMENT.md) before
using the launcher. Never supply a production chain spec, database, key, endpoint, or bootnode.

## Boundaries

RPC operations are named, read-only calls supported by the integrated node: system identity/health,
chain header/block/genesis, runtime version/metadata, one-key storage, account nonce, and transaction
fee queries. The native codec constructs and decodes selected ETKN, registered-asset, NFT and World
registry SCALE calls plus externally signed version-4 extrinsics. The codec has no internal signer, key access, raw
RPC escape hatch, or submission helper. World calls use generated production weights. AMM/API
bindings remain dormant. There are no configured contracts, EVM, Ethereum RPC,
Solidity, ERC-20, bridge, oracle, or proprietary-engine execution interfaces.

Owner-adopted Option C retains native FRAME dApps in V14 and defers true ERC-20 compatibility to a
V15 architecture comparison. Native Assets and Wasm/PSP22 are not presented as ERC-20.

See [`docs/PUBLIC_PRIVATE_BOUNDARY.md`](docs/PUBLIC_PRIVATE_BOUNDARY.md) and
[`docs/BUILD_USE_VERSION_PROVENANCE_LICENSE.md`](docs/BUILD_USE_VERSION_PROVENANCE_LICENSE.md) for
the publication blockers and source lineage.

## Injected-wallet compatibility

The native SDK includes an offline-tested adapter for the standard injected Substrate wallet
`enable`, `accounts.get` and `signer.signPayload` contract. It verifies the pinned genesis,
specification and transaction versions, captured metadata, signed-extension order, mortality,
nonce, tip and checkpoint hash before constructing an opaque extrinsic. It never handles private
keys and has no submission method. The sample wallet example and all signatures are synthetic and
mocked; live wallet compatibility remains an external prerequisite.
