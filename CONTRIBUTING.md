# Contribution and review policy

The proposed initial publication is a read-only review snapshot. Maintainers are the existing `erablockchains/era-blockchain` owners; this file does not invent a reviewer roster or response commitment. Contribution acceptance/PR opening is a later owner decision. Public technical feedback must contain no credentials or exploitable vulnerability detail.

When changes are accepted for review, use an isolated fork/branch, describe the problem and observable behavior, preserve licence notices, provide focused tests and disclose runtime/storage/metadata/API impacts. Include toolchain/lockfile changes explicitly. Monetary, custody, authority, migration and weight changes need specific review; a CI pass does not authorize deployment. Use disposable synthetic identities only. Never connect test code or CI to production signing accounts.

Do not combine source cleanup, runtime activation and deployment. Keep all chain indices and SCALE compatibility explicit. SECURITY.md governs private vulnerability reporting. No CLA or rights transfer is inferred from this draft.
