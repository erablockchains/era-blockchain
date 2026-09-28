# Isolated non-authority node template

The launcher in `node/run-non-authority.sh` is for a disposable review environment only. It does
not build a node and ships no binary or chain spec. It starts the reviewed binary without an
authority role, with safe RPC methods, no telemetry, and loopback-only P2P/RPC exposure.

## Required inputs

Provide all three paths explicitly:

- a locally built and independently identified ERA node binary;
- a sanitized JSON chain spec with `chainType` equal to `Development` or `Local`, no bootnodes,
  no telemetry endpoint, and no production naming;
- a new or empty disposable base directory.

The validator checks JSON structure, chain type, topology fields, 18-decimal token accounting, and
the current display-symbol ambiguity. It accepts `ERA` because that is the checkpoint chain-spec
property and `ETKN` because that is the approved policy term; integration must resolve and pin one
display symbol before publication.

Review `node/node.env.example`, then export equivalent values in the current shell. Do not place
secrets in the configuration file and do not reuse any existing node database.

```sh
export ERA_REVIEW_NODE_BINARY=/path/to/reviewed/era-node
export ERA_REVIEW_NODEJS_BINARY=node
export ERA_REVIEW_CHAIN_SPEC=/path/to/disposable-local-chain-spec.json
export ERA_REVIEW_BASE_PATH=/path/to/new-disposable-node-data
export ERA_REVIEW_NODE_NAME=era-v14-review-node
bash node/run-non-authority.sh
```

The default DApp origin is `http://127.0.0.1:8080`. The launcher never enables external RPC or
unsafe RPC methods. It rejects a chain spec with embedded peers because such peers could disclose
or contact private infrastructure. A separately reviewed reviewer-to-reviewer peer configuration
belongs to the later disposable-network rehearsal, not this package.

## Verification

After starting only an authorized disposable node, serve the package root on loopback and use the
sample DApp to inspect the returned identity. Stop if `specVersion` is not 14 or
`transactionVersion` is not 1. A matching number alone is insufficient: release review must also
pin genesis, source, Wasm, and metadata hashes.

Never substitute a production chain spec, base path, key store, node key, service configuration,
endpoint, or peer address. The template is not a system service and makes no persistence,
availability, backup, monitoring, or hardening claim.
