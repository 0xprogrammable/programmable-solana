# Repository boundaries

Status: Accepted

## Canonical repository

`0xprogrammable/programmable-solana` is the canonical repository for the
security-critical protocol and the artifacts that must be released against it.
It is a protocol monorepo, not a container for every Programmable product.

The intended structure is introduced incrementally:

```text
programs/
  core/                 shared settlement program
  reference-cpmm/       one maintained example engine
crates/
  engine-interface/     instruction and account contract, errors, CPI helpers
  core-client/          canonical Rust transaction and account client
clients/
  typescript/           canonical TypeScript client
idl/                    reviewed public interface artifacts
spec/                   protocol contracts and architecture decisions
tests/
  integration/          cross-component behavior
  compatibility/        fixtures for every published interface version
  adversarial/          isolation, authorization, and bypass attempts
test-programs/           deliberately hostile programs used only by tests
examples/                minimal integrations, never hidden protocol logic
experiments/             isolated disposable workspaces, never public interfaces
deployments/             append-only public release manifests
scripts/                 deterministic development and verification helpers
```

Directories are not created until they contain maintained code, tests, or
documentation. This keeps the tree descriptive instead of decorative.

The current authority experiment deliberately precedes those production names:

```text
programs/core/                    disposable Core V0 path
crates/engine-probe-interface/    private fixed-width experiment wire
test-programs/spike-engine/       configurable benign and hostile callee
```

None of these experiment names or bytes is the accepted `engine-interface` or a
reference market engine.

## Disposable experiment workspaces

An experiment that must test incompatible program IDs, wire formats, or Cargo
dependencies may use its own nested workspace under `experiments/<name>/`. Each
such workspace must have its own lockfile and build output, keep every package
unpublished, and remain dependency-isolated from the canonical root workspace in
both directions. Repository checks enforce that boundary from Cargo metadata.

Experiment programs are security-critical evidence but are not supported
protocol releases, production topology, or public interfaces. Their source may
be removed once a reviewable result record preserves the conclusion and every
repository, workflow, and specification reference is removed in the same
change. The current isolated workspace is:

```text
experiments/engine-generated-settlement/   disposable generated-output probe
```

## Dependency direction

The engine interface is the narrow compatibility boundary:

- it may contain wire formats, account types, error codes, version identifiers,
  and client helpers;
- it must not contain core business logic or depend on the core program;
- the core and reference engine may depend on it; and
- third-party engines must be able to depend on it without importing the core
  implementation.

Generated IDLs and clients must be reproducible from reviewed source. CI will
reject generated drift once generators exist.

## Components that stay separate

- The existing website remains in its current repository.
- A production indexer and API receive a separate repository when they become an
  independently deployed service. Canonical event and account schemas remain
  here.
- Community engines remain in their developers' repositories.
- Launchpads, scanners, routers, games, and other applications are not protocol
  packages.
- Secrets, keypairs, RPC credentials, and private deployment configuration are
  never committed.

## When a component may split

A component moves to another repository only when at least one real boundary
exists:

- an independent release cycle;
- an independent deployment or operational owner;
- different access controls;
- a stable public interface that no longer requires atomic protocol changes; or
- CI cost that cannot be isolated safely inside the monorepo.

Repository size or folder count alone is not a reason to split. Any split must
preserve immutable references between source commits, generated interfaces,
release artifacts, and onchain deployments.

## Change ownership

Until a GitHub organization and multiple maintainers exist, `@0xprogrammable` is
the only CODEOWNER. Exploratory work may begin under that boundary, but no
security-critical core implementation may be marked accepted and no artifact may
be presented as trusted until the core, engine interface, workflows, and
deployment manifests have an organizational owner and independent multi-person
review.
