<p align="center">
  <picture>
    <source
      media="(prefers-reduced-motion: reduce)"
      srcset="./assets/readme/programmable-solana-night-garden-v2.png"
    />
    <img
      src="./assets/readme/programmable-solana-night-garden-v2.gif"
      alt="The white Solana mark above a vivid night garden while small round stars twinkle in a black sky"
      width="100%"
    />
  </picture>
</p>

# Programmable Solana

Programmable Solana is the native SVM binding of the open Programmable Protocol
for programmable liquidity and exchange. It is a DEX protocol, not a launchpad
or hosted application.

Developers will be able to define market behavior in their own Solana programs
while the protocol enforces shared settlement rules, fee collection, and market
isolation.

The portable protocol specification will be implemented separately by native
bindings. Robinhood Chain is the first planned production deployment through
the EVM binding; SVM development proceeds in parallel and has its own release
gates. Bindings share semantics and conformance vectors, not bytecode, state,
custody, liquidity, fee accounting, addresses, or security evidence.

## Status

The protocol is in its design phase. Four disposable experiments are
executable locally: the original authority kernel, an isolated
engine-generated-output probe, and an isolated routed-callback-authentication
probe. The third experiment selects one pre-settlement writable transition over
a prepare/commit pair for the next private architecture gate. These experiments
test narrow security and runtime hypotheses; none of their wire formats is a
public interface or a generic settlement-plan design. No program from this
repository is deployed or approved for production use.

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
- **Immutable production majors.** A Core that can accept real assets has no
  upgrade, configuration, fee, pause, sweep, or migration authority. New majors
  use new Program IDs and opt-in migration.
- **Exact protocol assessment.** Production V1 charges five basis points with
  cumulative floor rounding on `PrincipalFundedGrossDebitV1`; experiments may
  use other explicitly test-only values.
- **Honest security boundaries.** The protocol secures its own invariants; it
  does not certify third-party engines, tokens, or interfaces as safe.

## Repository scope

This is the canonical repository for SVM-binding components that must change
and be tested together:

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
