# V13 source provenance

## Historical identity

- Original source commit: `77eb28b519d2f83796e65d402a62671a8c777c83`
- Original source tree: `1b0d6a42f1c6dbe091391474fd4b72445a5b2fdb`
- Deployed runtime Wasm: 997,122 bytes
- Deployed runtime Wasm SHA-256:
  `ecca5baddc60d4e8522c8ea6206fec0c29175d59ddf2359e6d43201f1faa44a2`

## Local tracked-source record used for this import

- Local archive:
  `C:\ERA-VM\R9R3\work\review-20260824T202753Z\r8r5\era-blockchain-r8r5-77eb28b-tracked-source.tar`
- Archive size: 1,300,480 bytes
- Archive SHA-256: `b0e5e0174e38ba99a367138b75e670cc97021ebf76e6a62ed48b81d7e831f286`
- Archive inventory: 75 entries, comprising 56 regular tracked-source files and directories
- Archive safety validation: zero unsafe paths; zero link or special entries
- Audited identity record:
  `C:\ERA-VM\R9R3\evidence\artifact-and-account-identities.txt`
- Audited identity record SHA-256:
  `4b9c23bb258457a56208b030e8b6a14260500b286403192f3d4a54a138e7d32f`
- Audited archive-validation record:
  `C:\ERA-VM\R9R3\evidence\archive-validation.txt`
- Audited archive-validation record SHA-256:
  `87e5811e7cb2bd83c569ab8600f237e3a9ced615c20a610325344d9aaed9f619`

The audited identity record binds the exact archive size and SHA-256 above to the original commit,
original tree, and deployed Wasm identity. A separate local copy of that deployed compressed Wasm
was also re-hashed at the required size and SHA-256 before this import.

## Import semantics

The source tar is a tracked-file export, not a native Git archive. It contains no `.git` directory or
Git PAX commit header and normalizes regular-file modes to `0600`; original Git mode metadata cannot
be reconstructed from the tar alone. The historical tree identity therefore comes from the audited
identity record bound to the exact archive hash, not from the new repository index.

The new private GitHub commit is an import/review commit that contains the historical source files
plus review documentation, a security policy, and ignore rules. It is not the original historical
commit and must never be cited as such. No build output, node state, key material, private evidence
archive, foreign `.git` directory, or workstation untracked data was imported.
