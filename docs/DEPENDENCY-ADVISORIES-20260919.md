# Dependency advisory status — 19 September 2026

This source snapshot retains its deployed-source dependency bindings. Two public advisory ranges match versions in Cargo.lock. These findings were identified during a narrow publication review, not a new whole-project audit. Neither alert is claimed resolved; do not equate passing retained tests with absence of known vulnerabilities.

| Package | Locked version | Advisory | First patched version reported upstream | ERA disposition |
|---|---|---|---|---|
| libp2p-quic | 0.11.1 | [GHSA-5hq8-qhww-jm7q / CVE-2026-61544](https://github.com/advisories/GHSA-5hq8-qhww-jm7q) | 0.13.1 | Affected version present; exact release-feature/listener reachability not fully established. |
| serde_with | 3.15.1 | [GHSA-7gcf-g7xr-8hxj](https://github.com/advisories/GHSA-7gcf-g7xr-8hxj) | 3.21.0 | Affected version present; no KeyValueMap use found in the searched ERA node/runtime/pallet or pinned SDK source, but transitive/feature reachability is not exhaustively established. |

The QUIC advisory concerns a certificate-expiry race that can panic a QUIC listener. It was published upstream on 10 July and added to GitHub's advisory database on 15 September 2026. The inspected pinned sc-network transport builder constructs TCP/DNS, WebSocket/WSS or memory transports; it does not instantiate libp2p-quic. That static evidence is narrower than an exhaustive release-feature and deployed-listener proof.

The serde_with advisory concerns KeyValueMap serialization of empty entries. The inspected SDK network types use different serde_with adapters. Searches of ERA source and the pinned SDK found no KeyValueMap reference. An affected dependency version remains in the lockfile regardless of whether this particular API is reachable.

A targeted offline dependency-graph query failed because the local index lacked locked bytes 1.12.1. No fetch, dependency change, compilation, production test or transaction followed. Existing synthetic/native/Wasm validation and the completed 72-hour sampled observation remain valid for their documented scope. No exploitation or production incident is demonstrated by these dependency alerts.

Follow-up: bind each alert to the exact release features and affected API; then propose a compatible upstream update or narrowly reviewed backport if needed. Validate only the changed dependency/affected path in isolation, including fail-closed behavior and relevant node compatibility. Do not blindly merge a dependency bot's whole-lockfile update or change the production runtime/network. A fix that changes source or dependencies needs new provenance and appropriate isolated build/testing before any separate deployment approval.
