# Public-review and private-system boundary

This directory is a proposed public technical-review surface, currently held privately.

## Included in the proposed surface

- status-qualified V14 policy and architecture summaries;
- minimal source for implemented, standard read-only RPC calls;
- a static read-only sample DApp;
- an isolated non-authority launcher and placeholder-only configuration;
- offline validation, sanitization, provenance, license, and security notes.

## Excluded from the surface

- keys, seed phrases, account addresses, call payloads, signatures, credentials, and personal data;
- production or private endpoints, bootnodes, topology, service definitions, databases, logs, and
  monitoring data;
- chain specifications, node binaries, runtime Wasm, generated metadata, build caches, and release
  artifacts;
- custody identities and derived addresses;
- proprietary World Engine, AI-engine, model, dataset, operator, and infrastructure internals;
- V15 GPU/digital-twin worker, verifier, compute-market, and proof-system implementation;
- publication automation, release credentials, website content, wallet/explorer changes, and
  listing submissions.

The generic V14 asset/application/payment foundations do not expose or transfer ownership of any
private engine. The absence of private code is intentional and is not a claim that an external
engine is integrated, available, compatible, or authorized.

Any future expansion of this boundary requires a new provenance review, secret/private-topology
scan, license decision, security review, and explicit publication approval.
