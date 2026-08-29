# Versioning

Programmable Solana versions independently released SVM-binding components
instead of treating the repository as one application version. Portable
protocol major semantics and golden vectors are shared with other native
bindings; Solana artifacts and deployments are versioned independently.

Planned tag families are:

- `core-vMAJOR.MINOR.PATCH`
- `engine-interface-vMAJOR.MINOR.PATCH`
- `sdk-rust-vMAJOR.MINOR.PATCH`
- `sdk-ts-vMAJOR.MINOR.PATCH`

Before `1.0.0`, a minor version may contain a breaking interface change. Every
such change must identify the last compatible core, preserve a fixture for the
old version, and state whether an onchain migration or a new program deployment
is required.

An onchain release is not identified by a tag alone. Its append-only deployment
manifest must bind the cluster, program ID, source commit, artifact hash, IDL
hash, toolchain, deployment transaction and slot, and upgrade authority state.

For every Core that can accept production assets, that manifest must prove
`upgrade_authority = None`, no replaceable program data or administrative Core
path, and the immutable protocol-constitution and collector identity selected
by that major. The V1 manifest specifically binds every
`ProtocolAssessmentV1` constant. A release candidate with a remaining authority
is pre-production, accepts no real user assets, and is not a Production Core.

Before the first deployment, the release process must define authenticated tags
or attestations, authorized signer rotation and recovery, and independent
verification that the recorded ELF hash matches both the reviewed source build
and the onchain program. The bootstrap commits are not release attestations.

A new Core program cannot sign for an older Core's PDAs. Side-by-side majors are
therefore compatibility and containment, not automatic migration. Any persistent
domain that needs a future escape or migration must bind that engine-independent
or user-authorized path in its original immutable descriptor; a later program ID
cannot add it retroactively.

Release tags and manifest formats will be finalized before the first devnet
deployment. No mainnet release is produced automatically by a merge.
