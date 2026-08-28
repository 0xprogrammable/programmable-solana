# Programmable Solana

Programmable Solana is an open protocol for programmable liquidity and exchange
on Solana. It is designed as a DEX protocol, not as a launchpad or a hosted
application.

Developers will be able to define market behavior in their own Solana programs
while the protocol enforces shared settlement rules, fee collection, and market
isolation.

## Status

The protocol is in its design phase. One disposable authority-kernel experiment
is executable locally; its wire format is not a public interface. No program
from this repository is deployed or approved for production use.

## Principles

- **Permissionless integration.** A developer does not need approval, an API key,
  or a listing agreement to build an engine or create a market.
- **Onchain liveness.** Trading and settlement must not depend on the website,
  indexer, API, or a Programmable-operated signer.
- **Liquidity-domain isolation.** A faulty or hostile engine must not gain
  direct authority over Core custody or a non-participating domain. It remains
  the economic authorization oracle for participating domains, which share its
  disclosed risk when they share liquidity.
- **Versioned extensibility.** The engine contract evolves through explicit
  versions and compatibility tests instead of hidden assumptions.
- **Honest security boundaries.** The protocol secures its own invariants; it
  does not certify third-party engines, tokens, or interfaces as safe.

## Repository scope

This is the canonical repository for components that must change and be tested
together:

- the onchain core;
- the public engine interface;
- one reference engine;
- canonical Rust and TypeScript clients;
- IDLs, protocol specifications, and compatibility fixtures;
- integration, invariant, and adversarial tests; and
- public deployment and release manifests.

The website remains in its existing repository. Production indexer and API
services will use separate operational repositories once they have independent
deployment lifecycles. Community engines belong in their developers'
repositories.

Directories are added when they contain working code or a maintained document.
The intended boundaries are recorded in [`spec/`](spec/) before implementation.

## License

Licensed under the [Apache License 2.0](LICENSE).
