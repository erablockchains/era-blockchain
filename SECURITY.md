# Security policy

This repository is private and contains consensus/runtime source. Do not disclose a suspected
vulnerability in a public GitHub issue, discussion, pull request, commit message, chat, or public
security mailing list.

## Private reporting procedure

1. If you have repository security permission, create a private draft advisory at
   `https://github.com/erablockchains/era-blockchain/security/advisories/new`.
2. If that private advisory form is unavailable, contact an `erablockchains` organization owner
   through the pre-agreed private channel and request a private advisory. Do not include exploit
   details in the access request.
3. In the advisory, include the affected commit and files, impact, prerequisites, a minimal
   reproduction that uses no production secrets or live-chain action, and a proposed mitigation if
   known.
4. Do not test against ERA-MAINNET, RPC3, validators, production wallets, production node databases,
   or custody/signing systems. Use an isolated local test environment with synthetic keys and state.

No response time, bounty, embargo, or disclosure date is promised by this baseline. Coordinate all
disclosure decisions privately with the repository owner.
