# Public full node and operational boundaries

These instructions are a publication candidate. They were checked against source/retained help, not validated as a new external-node onboarding run. Obtain a reviewed build and verify its checksum; [release bindings](../provenance/release.json) identify the deployed binary. Never import a founder validator keystore or reuse an existing production database.

The exact public raw spec [era-v14.raw.json](../network/era-v14.raw.json) intentionally has empty bootNodes; pass the two public sentry addresses separately. Keep the raw bytes unchanged to preserve its published checksum. Genesis is `0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e`.

On a separate full-node machine, as its unprivileged operator, after an isolated build:

```bash
test ! -e "$PWD/era-public-node-data"
./target/release/era-node \
  --chain ./network/era-v14.raw.json \
  --base-path "$PWD/era-public-node-data" \
  --name era-public-full-node \
  --listen-addr /ip4/0.0.0.0/tcp/30333 \
  --bootnodes /ip4/46.62.184.123/tcp/31333/p2p/12D3KooWK57ku2nXVZrAC7NKouRaMr9ud2zmeiLcemuZh5tHKaG4 \
  --bootnodes /ip4/46.62.184.124/tcp/31333/p2p/12D3KooWGtChkkYktjHoVzH4XKTP6mzNvvwesE5VQud8BTGC68P9 \
  --rpc-port 19944 --rpc-methods Safe \
  --no-mdns --no-telemetry --no-prometheus --offchain-worker Never
```

RPC remains loopback: do not add rpc-external/unsafe flags. Do not combine `--port` with `--listen-addr`; the deployed CLI rejects that combination. A full node is not a validator: no validator flag or consensus keys are supplied here. Public sentry reachability/admission from a new independent host has not been validated by the internal fleet proof. Validate connectivity locally before describing this as a tested public onboarding workflow.

Use process supervision on the operator's host and monitor identity, finalized head progression, process state, peer count, free bytes/inodes and durable local warning history. Keep database, logs and compaction reservations separate from protected other-work commitments; unknown commitments are not zero. Preserve rollback files. This candidate does not apply fleet firewall/systemd/nginx configuration or prescribe the production fleet's disk floors for arbitrary machines.

Supported CLI paths: normal run, key, build-spec, chain-info, fresh-spec, fresh-check. The benchmark subcommand needs its build feature. check-block/export-blocks/export-state/import-blocks/revert dispatchers are explicitly unsupported; do not use them as a backup/recovery plan. Never run purge-chain on retained production data.

Validator expansion remains an owner-reviewed operation, with minimum self-bond10,000 ETKN and commission0–20%, current set4 and bounded election implementation. Independent admission proof and expansion procedures remain gaps. Possessing stake does not establish a commissioned external-validator workflow.
