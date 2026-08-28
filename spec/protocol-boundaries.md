# Protocol boundaries

Status: Draft

## Purpose

Programmable Solana is a DEX protocol for markets whose pricing, liquidity, and
state transitions can be defined by external Solana programs called engines.
The protocol provides common settlement without deciding what every market must
be.

The design separates three concerns:

1. **Core settlement** owns the shared rules that cannot be delegated safely.
2. **Engines** define market-specific behavior through a versioned public
   contract.
3. **Clients and indexers** derive transactions and views but hold no settlement
   authority.

## Core responsibilities

The core is expected to:

- derive a canonical identity for every market;
- bind each market to its engine, asset-validation policy, interface version, and
  fee configuration;
- authenticate the actual asset accounts and capabilities supplied for each
  settlement;
- authenticate every account participating in settlement;
- prevent one market from substituting or writing another market's core-owned
  state or core-custodied vaults;
- validate the engine-owned write set and any explicit shared-state capabilities;
- constrain asset movement to the settlement authorized by the current
  transaction;
- collect protocol fees through rules that an engine cannot bypass;
- reject unsupported interface versions and malformed settlement results;
- emit canonical events from committed onchain state; and
- leave all writes atomic when an instruction fails.

These responsibilities must remain onchain. An RPC provider, website, API,
indexer, keeper, or Programmable-controlled server must not be required for a
valid market interaction.

## Engine responsibilities

An engine is expected to:

- define its market logic and engine-owned state;
- validate the conditions specific to that logic;
- return or authorize a settlement request within the core's public contract;
- declare the interface version and writable-account capabilities it uses;
- identify engine state that is intentionally shared across markets; and
- expose any engine-specific semantics through an optional public schema or
  decoder.

Permissionless means a developer can deploy an engine and create a compatible
market without approval from Programmable. It does not mean the core executes
unbounded native code, grants authority over undeclared accounts, or transfers
assets without validation.

## Outside the core trust boundary

The protocol does not:

- provide liquidity to markets;
- approve listings or certify third-party engines;
- guarantee the economics, price, token behavior, or solvency of a market;
- guarantee that a third-party interface displays complete or honest data;
- depend on a canonical launchpad, explorer, scanner, or website; or
- make all possible Solana programs safe merely because they integrate with the
  protocol.

Third parties may build those products independently.

## Offchain components

Indexers and APIs are replaceable projections of onchain state. Every successful
core settlement must emit a versioned envelope containing enough common data to
identify the market, engine, assets, amounts, fees, and market-local sequence or
checkpoint. Global ordering comes from the Solana ledger position, not a
protocol-wide writable counter.
Engine-specific meaning is decoded through optional engine schemas or plugins;
the generic indexer does not pretend to understand arbitrary engine state.

Current canonical markets and state must be discoverable from program accounts.
An independent live indexer must be able to follow the settlement envelope and
detect gaps. Reconstructing history that a normal RPC node has already pruned
requires an archival ledger source and is not a protocol-liveness guarantee.

Indexer databases are not protocol state. If every Programmable-operated
offchain service disappears, a developer with a Solana RPC connection and the
public interface must still be able to construct a valid transaction.

## Decisions that remain open

The following choices are deliberately not frozen by this draft:

- whether the core invokes an engine, an engine invokes the core, or a bounded
  two-phase interaction is used;
- the binary settlement-intent format and its maximum resource bounds;
- the declaration and validation model for engine-owned writable accounts and
  shared state;
- which Solana token programs and asset behaviors the first core version
  supports;
- how engine code identity and engine upgrades are represented;
- how interface discovery and compatibility negotiation work;
- the canonical settlement envelope and gap-detection mechanism;
- the exact protocol-fee model and rounding rules; and
- whether the first core deployment is immutable or governed by a constrained
  upgrade process.

Each choice needs an accepted decision record, executable compatibility tests,
and adversarial tests before implementation is considered stable.
