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
- Run `./scripts/check-repository.sh`.

The Rust, Solana, and JavaScript commands will be added here with their pinned
toolchains when the first workspace is introduced. Do not infer a production
toolchain from a developer's local installation.

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
