# Build, use, version, provenance, and license notes

## Package version and runtime target

The review-surface package version is `0.1.0-private-review`. Its remediation source base is commit
`85d84c06b6298a3260bce3ff2d1cf0f60d6773df`, tree
`9f68cb2fcb83502915d8f3764891ef20c0943a3a`. The target runtime declares
`spec_version = 14` and `transaction_version = 1`.

The final remediation commit/tree, runtime Wasm hash, metadata hash, toolchain, dependency locks and
reproducible-build pair are deliberately not self-asserted by a file inside that commit. They must
be bound by the checksum-sealed postcommit release evidence. No binary or generated runtime artifact
is included in this review surface.

## Build and use

The SDK and DApp are hand-written ES modules with no bundling step and no third-party JavaScript
dependencies. Offline checks require Node.js 20 or newer, a POSIX shell, and ripgrep. A static
loopback server is needed to load browser modules. The DApp waits for the reviewer to select Connect.

The node launcher consumes a separately built binary. Building, benchmarking, metadata generation,
runtime Wasm generation, and reproducibility validation are integration/release activities and are
not performed by this package.

## Provenance

All files in this package were created for WS8 from the approved V14 policy records and direct
inspection of the checkpoint's node RPC wiring and runtime version. No image, font, bundled code,
chain state, production configuration, or proprietary engine source was copied into the package.
`provenance.json` is the machine-readable companion record.

## License and publication boundary

ERA-authored code in this private review package is declared `Apache-2.0`, consistent with the
owner's repository license decision. Third-party dependencies and copied or modified upstream work
retain their own terms and are enumerated in the repository license inventory and notice bundle.
This correction is not a publication approval, legal-approval claim, or production brand approval;
the package remains private until the separate release gate approves publication.

No legal, audit, security, or production assurance follows from the existence of this draft.
