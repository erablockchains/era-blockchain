# Architecture and trust boundaries

The native node runs the runtime Wasm and exposes Substrate JSON-RPC. BABE selects authors; GRANDPA finalizes; Session/Historical bind authorities. Staking elections are bounded to 16 targets/winners and 64 voters. The current active set has four approved founders. The benchmarking parameter for 48 nominators is not a live admission limit.

Balances and IssuanceCap enforce lifetime monetary accounting. FreshGenesis initializes the distinct chain; SecurityBudget reserves existing principal before permitted residual issuance and funds standard paged staking claims by transfer. RewardReserve remains in metadata for compatibility but its old engine is inactive/legacy-claim-only in fresh genesis. Native staking payout is not an independent mint route. Signed call filters constrain relevant issuance/staking administration, but retained Sudo/runtime replacement remains a material trust boundary.

Assets16, Nfts17 and EraWorlds18 implement native application calls. AiPredictions12 has disabled economic modes and restricted onboarding. Workspace membership does not imply runtime inclusion: AMM/application-primitives contracts are dormant. No general-purpose EVM/contract VM or proprietary World Engine is part of this release. Consult the metadata inventory for the deployed indices, particularly FounderCustody22/FreshGenesis23, before proposing integration.

Production operates behind sentries with separate RPC services and local health/storage supervision. This repository publishes public full-node instructions, not private fleet controls, credentials, root deployment scripts or signing tools.
