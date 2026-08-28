# 0001: Use one canonical protocol monorepo

- Status: Accepted
- Date: 2026-08-28

## Context

The onchain core, engine interface, reference engine, clients, IDLs, and attack
tests will evolve together while the protocol contract is being established.
Splitting them now would allow incompatible changes to merge independently and
would make one protocol release difficult to reproduce.

Solana DEX repositories use several models. Raydium commonly separates major
pool programs into distinct repositories. Meteora keeps a program, its Rust SDK,
and integration tests together, while some TypeScript SDKs live separately.
Orca Whirlpools keeps a broader set of tightly coupled programs and clients in a
monorepo. None of those layouts should be copied without matching its lifecycle.

## Decision

Use `0xprogrammable/programmable-solana` as one public Apache-2.0 protocol
monorepo. Keep version-coupled source, interface artifacts, tests, specifications,
and public deployment evidence together.

Do not move the existing website into this repository. Do not host community
engines here. Move the production indexer and API into their own operational
repository when they have an independent deployment lifecycle; retain their
canonical decoding contract here.

Do not pre-create empty directories. Introduce each path with the code or
maintained document that gives it meaning.

## Consequences

- One pull request can change an interface and prove compatibility across all
  maintained consumers.
- Security-sensitive changes share one review and release history.
- CI can test malicious engines directly against the proposed core change.
- Package releases still use independent versions and tags.
- Repository ownership must remain strict so the monorepo does not become a
  general product workspace.

## Revisit conditions

Revisit this decision when a component has an independent deployment, team,
access policy, or stable release cycle. Record the split in a new decision; do
not rewrite this history.
