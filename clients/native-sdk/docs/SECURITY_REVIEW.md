# Security and reviewer guidance

Treat the package as untrusted review material and keep all evaluation isolated from real accounts
and networks.

- Read the source before running it. The package has no install step.
- Use only synthetic data, a disposable local chain spec, a reviewed binary, and a new empty base
  directory.
- Keep RPC and the sample server on loopback. Do not relax RPC safety or CORS for convenience.
- Do not paste account seeds, signed extrinsics, tokens, or private endpoints into the DApp.
- Verify source, genesis, Wasm, metadata, and dependency identities independently for any later
  integrated candidate.
- Report vulnerabilities through the owner-designated private channel. No public security contact
  is asserted by this draft.

`tools/scan-review-surface.sh` rejects likely secrets, non-loopback infrastructure literals,
account-like strings, stale runtime-version claims, unsafe node modes, symlinks, binary files,
archive/build artifacts, and selected unsupported positive claims. It is a bounded source scan,
not a full-history secret audit, license audit, or independent security audit.
