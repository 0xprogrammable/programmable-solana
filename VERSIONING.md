# Versioning

Programmable Solana versions independently released protocol components instead
of treating the repository as one application version.

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

Before the first deployment, the release process must define authenticated tags
or attestations, authorized signer rotation and recovery, and independent
verification that the recorded ELF hash matches both the reviewed source build
and the onchain program. The bootstrap commits are not release attestations.

Release tags and manifest formats will be finalized before the first devnet
deployment. No mainnet release is produced automatically by a merge.
