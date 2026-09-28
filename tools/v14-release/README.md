# Upgrade preparation tools

These tools prepare public local artifacts; they do not grant approval or perform deployment. `prepare_upgrade_release.py` verifies every file in the retained public release manifest, preserves its genesis/topology/policy files and substitutes the reviewed optimized node, equivalent runtime artifacts and four host-control modules. A distinct output is required. The manifest retains historical original-release approvals as historical inputs; it does not approve new execution or commissioning. Source/build/evidence gates must pass before its output can be proposed.

`prepare_continuous_rebind.py` checks the existing public receipt/state/acceptance hash chain and emits proposed records with only manifest/hash references changed. It preserves original commissioning start/deadline and continuous acceptance, policy, high-water counters and pause state. Capture operation-time records only after an approved per-host stop; never overwrite newer state with an earlier review snapshot.

`deployment_authority.py` uses a new upgrade-only host authorization schema. It refuses fresh-chain, network, endpoint, deletion, notification and transaction authority. The actual owner authorization reference and root-owned installation are separate future execution actions. Do not run the retained original fresh-start, stage-release, accept-continuous or endpoint scripts: they are historical artifacts, and the original exclusive-path stager cannot upgrade an existing installation.

`local_health.py` permits only era/transaction1/spec14 or15 during the rolling binary/runtime transition; it rejects identity or within-process version/finality regression. It keeps existing bounded RPC/stall behavior. Its version floor is in-memory; final acceptance independently checks spec15 and exact Wasm on every host. Storage policy and network topology do not change.

Run focused tests with `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/v14-release -p 'test*.py'`.

Upgrade authority verification requires an explicit approved operation. Only the proposed supervisor supplies `rolling_reviewed_host_restart`; unchanged legacy fresh/network/endpoint entry points omit it and are refused before authority-file access. The release manifest keeps `wasm_sha256` as the compressed candidate artifact hash, separately naming raw Wasm and the unchanged genesis Wasm.
