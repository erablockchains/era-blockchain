> Current completion candidate: the deployed spec15 source is bound to development commit `a2ba1935c342912c31cc8fad44a220ce63e8d594`, tree `09fd25bcf4206c4812a28543a2bee7c5e13e2e31`, source archive SHA-256 `641090e6ef5dffec65a8b716eadfaeba29dbe0c5a7be757928ccd83be3e07623` and compressed Wasm SHA-256 `122af167022227c46b2b74d99f5d3a73f41f8d65bb4a8a1de7b88b6e986de2af`. The isolated public candidate at parent commit `17c22ce897fa31d720eea632fd1e6a717185c20f` preserves runtime/node/lock/vendor bytes and adds reviewed public client/documentation material. [Current candidate provenance](../provenance/completion-development.json) and [current source manifest](../provenance/SOURCE-SHA256SUMS) bind this review tree. Publication remains separately gated.

# Current development provenance

This completion tree descends from preserved candidate `5ff1f84f84e6e1b809420c9899f5aceeb3f8b5be` through development checkpoints `76ce6f5ff02f4cd0ec8909b6bb5aadafa62473f0` and `6fb7813c91051f406d470f8057ccc6339ecd5162`. It contains the executable V14 changes and dependency backports used for the authenticated release. The historical manifests and release hashes below bind their named earlier artifacts. Public development-key test tools contain no production signer, key store or credentials. Source publication is a separate milestone requiring exact owner approval; unfinished NFT, reward, penalty and independent-qualification gates remain disclosed rather than silently waived.

## Preserved initial-release provenance

# Source and release provenance

The canonical source export identifies commit `b5d0476efaaebd98290d5bf6e32da8c3e915c782`, tree `21c3add55e9961e91452dccd3bfae56ff0db3c5a`, and a 1,666-file verified export manifest `e0f8e999c46ddcaddec27c81d37a7f67be2607d6524d242afb4361f43ce8e960`. The local export baseline commit is an import, not that canonical Git checkout.

The baseline alone is not the deployed source. Apply [fresh-relaunch.patch](../provenance/fresh-relaunch.patch), then [principal-first-r3.patch](../provenance/principal-first-r3.patch). This reconstruction matched all 675 selected source/manifests/vendor/test files before publication redactions. [DEPLOYED-SOURCE-SHA256SUMS](../provenance/DEPLOYED-SOURCE-SHA256SUMS) binds those original bytes. Two files have historical benchmark path/host comments or documentation redacted, and the custody README is updated for fresh genesis; [publication-redactions.json](../provenance/publication-redactions.json) binds before/after hashes. Executable Rust tokens are unchanged. [SOURCE-SHA256SUMS](../provenance/SOURCE-SHA256SUMS) binds the candidate source bytes.

Rust 1.87.0 and Polkadot SDK `f3969c7ddd34985e6e709ed458bcc519f651682a` / tag polkadot-v1.19.1 are pinned. The root Cargo.lock and all five vendor security forks are retained. Individual vendor PROVENANCE.md files bind original crates and corrections.

[release.json](../provenance/release.json) records the deployed node, Wasm, chain spec and corrected release manifest hashes. Matching source provenance plus retained build evidence is not a new independently reproducible build or a signed release tag. No binary is included in this source candidate. The raw genesis necessarily embeds the approved runtime Wasm.

The historical publication review proposed a new source snapshot with three unsigned local preparation commits (the preserved 16 September candidate and two separate 19 September documentation revisions), not an export of private history. Private refs, historical Actions logs, attachments and release artifacts are outside this candidate. Changing an existing repository's visibility would expose surfaces not reviewed here and requires separate review before that publication action.

The 19 September revision changes documentation and adds a sanitized [observation record](../provenance/observation-20260919.json). Runtime/node/pallet/vendor code, manifests, lockfile, genesis, metadata, source bindings, licences and existing test evidence are unchanged. No operator logs, private routes or private canonical history are imported. The exact parent-to-revision diff and archive manifest are retained in the local publication review package.

The later authenticated-review revision adds public dependency-advisory disclosure and clarifies reporting-route status. It preserves executable source, dependencies and all deployed artifact bindings. Authentication records, private destination history and Actions logs are excluded. Neither documentation revision remedies the affected dependencies or authorizes their deployment.
