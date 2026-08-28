# Contributing

Programmable Solana accepts protocol design, implementation, testing, and
documentation contributions through focused pull requests.

## Before opening a pull request

- Discuss a new public interface or security model in an issue or decision record
  before implementing it.
- Keep one behavioral change in one pull request.
- Update the specification and compatibility evidence with any public interface
  change.
- Add a regression or adversarial test for every corrected security failure.
- Run the pinned verification commands below.

The repository check rejects bare 64-byte JSON arrays because they are
indistinguishable from the canonical Solana CLI keypair format without further
context. Store a public signature fixture in a typed object, for example
`{"fixtureType":"ed25519-signature","bytes":[...]}`. Never add a real keypair as
a fixture or allowlist a secret path.

## Pinned spike toolchain

The disposable engine-boundary experiment uses host Rust 1.96.0, Anchor crates
1.1.2, and Solana CLI 3.1.10. Program crates retain a Rust 1.89.0 minimum so
Agave's v1.52 SBF compiler can build them. It has no deployment keypair and must
not be treated as a production toolchain or public engine interface.

Run:

```sh
./scripts/check-repository.sh
cargo fmt --all --check
./scripts/build-spike-sbf.sh
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

Build before running the tests because the LiteSVM harness loads the generated
program binaries. The build script invokes `cargo build-sbf` directly so it does
not retain a deployment identity; cargo-build-sbf's transient generated key is
isolated in a temporary directory. It rejects any wrapper other than Agave
3.1.10 and requests platform-tools v1.52 with the SBPFv0 target explicitly. On a
fresh machine Agave may first download that compiler; only CI verifies the exact
archive checksum and therefore produces the canonical Ubuntu build evidence.
Local macOS artifacts are valid runtime-test inputs but currently have different
ELF hashes and are not canonical release evidence. Do not run `anchor init
--force`, copy generated keypairs into the repository, or use a developer's
default Solana wallet for the spike.

The isolated engine-generated-settlement experiment has its own workspace and
lockfile. Run its complete gate from the repository root with:

```sh
./experiments/engine-generated-settlement/scripts/check.sh
```

Its Cargo packages and path dependencies must remain wholly inside that nested
workspace. The root workspace must never depend on experiment code.

The routed-callback-auth experiment is a separate private workspace with four
different disposable programs. Run its complete local gate from the repository
root with:

```sh
./experiments/routed-callback-auth/scripts/check.sh
```

It must not import the predecessor experiment, load predecessor build output, or
reuse its program IDs. A local successful build is not canonical artifact or
deployment evidence.

## Commit titles

Write short English titles in the imperative, with one concrete purpose and no
final punctuation. Aim for 72 characters or fewer.

Good:

```text
Bind settlement to the registered market
Reject accounts owned by another market
Record the devnet core deployment
```

Avoid type prefixes and generic summaries such as:

```text
feat: implement comprehensive scalable architecture
Improve robustness and security
Update files
```

Pull requests are squash-merged, so the pull request title becomes the commit on
`main`. Use the description for rationale, security impact, compatibility, and
verification evidence.

## Review boundaries

Changes to the following paths are protocol-critical:

- `programs/core/`
- `crates/engine-interface/`
- `crates/engine-probe-interface/`
- `experiments/`
- `deployments/`
- `.github/workflows/`
- the security properties and accepted architecture decisions

Those changes must identify affected invariants and the tests that establish
them. A build result alone is not security evidence.

No pull request or merge deploys a program to mainnet automatically. Deployment
requires a separate reviewed release process and an append-only public manifest.

## License

By submitting a contribution, you agree that it is licensed under the Apache
License 2.0 included in this repository.
