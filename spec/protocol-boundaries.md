# Protocol boundaries

Status: Draft

## Purpose

Programmable Solana is a DEX protocol for markets whose pricing, liquidity, and
state transitions can be defined by external Solana programs called engines.
The protocol provides common settlement without deciding what every market must
be.

This document is the native SVM binding. Portable protocol semantics are shared
with the separate EVM binding; Program IDs, PDAs, accounts, CPI, asset profiles,
runtime limits, state, liquidity, and release evidence remain Solana-specific.

The design separates three concerns:

1. **Core settlement** owns the shared rules that cannot be delegated safely.
2. **Engines** define market-specific behavior through a versioned public
   contract.
3. **Clients and indexers** derive transactions and views but hold no settlement
   authority.

## Core responsibilities

The core is expected to:

- derive a canonical identity for every market;
- bind each market to its engine program, interface version, code policy,
  participating domain references, separate builder/integrator economics, and
  the immutable Core V1 protocol constitution;
- bind each domain to one immutable descriptor covering custody, asset,
  accounting, exit, admission, and protected-capability profiles;
- authenticate every protected asset account and capability supplied for each
  settlement;
- hash-bind the exact ordered opaque account metas and effective privileges
  without claiming to understand accounts owned by arbitrary external programs;
- prevent an execution from substituting, debiting, closing, redirecting, or
  changing Core-accounted state or rights in a non-participating liquidity
  domain; unsolicited raw token credits may still occur but create no accounted
  liquidity or claim;
- validate the protected write set, reject opaque aliases into protected roles,
  bind the actual CPI capability closure, and validate any explicit
  shared-liquidity domain;
- verify that each participating domain's own local rule authorizes the market,
  engine program, interface, code policy, capability profile, and exact domain
  descriptor;
- constrain asset movement to the settlement authorized by the current
  transaction;
- collect the immutable five-basis-point `ProtocolAssessmentV1` exactly once on
  each canonical `PrincipalFundedGrossDebitV1` group inside the Core envelope,
  while making no percentage claim about opaque or off-Core semantics;
- reject unsupported interface versions and malformed settlement results;
- emit canonical events from committed onchain state; and
- leave all writes atomic when an instruction fails.

These responsibilities must remain onchain. An RPC provider, website, API,
indexer, keeper, or Programmable-controlled server must not be required for a
valid market interaction.

## Engine responsibilities

An engine is expected to:

- define its market logic and any state it owns or composes through other
  programs;
- validate the conditions specific to that logic;
- participate through the selected versioned callback shape and bind the same
  exact candidate plan as the Core;
- declare the interface version and exact opaque account capabilities it uses;
- identify engine state that is intentionally shared across markets; and
- expose any engine-specific semantics through an optional public schema or
  decoder.

Permissionless means a developer can deploy an engine and create a compatible
market without Programmable approval, an admin signer, API key, allowlist, or
listing vote. Admission depends only on public deterministic interface,
resource, fee, and authority rules. It does not mean the Core executes with
unbounded authority, grants access to undeclared accounts, or transfers assets
without validation. Arbitrary market semantics remain open while
authority-bearing settlement capabilities are versioned and closed within each
Core major.

Core-native asset profiles receive the Core's strongest accounting and custody
guarantees. External programs remain permissionlessly composable in the opaque
engine plane, but their bytes and receipts are `EngineAttested`; Core does not
turn them into generic claims such as "NFT transferred" or "position closed."
A new `CoreVerified` asset profile requires exact understood semantics and a
separate accepted authority, admission, code-identity, fee, and liveness
decision.

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
Core settlement must emit a versioned evidence header containing the market,
engine, plan digest, participating domains, Core-verified asset effects and
protocol fees, and explicitly attested opaque digests. Global ordering comes
from the Solana ledger position, not a protocol-wide or market-wide writable
counter.
Engine-specific meaning is decoded through optional engine schemas or plugins;
the generic indexer does not pretend to understand arbitrary engine state.

Current canonical markets and state must be discoverable from program accounts.
An independent live indexer must be able to follow the evidence headers and
detect gaps. Reconstructing history that a normal RPC node has already pruned
requires an archival ledger source and is not a protocol-liveness guarantee.

The known set of side-by-side Core program IDs comes from an append-only signed
deployment manifest binding source, artifact, loader, and predecessor evidence.
It is discovery metadata, not a writable onchain trade registry or settlement
dependency.

Indexer databases are not protocol state. If every Programmable-operated
offchain service disappears, a developer with a Solana RPC connection and the
public interface must still be able to construct a valid transaction.

## Candidate execution decision

[`decisions/0002-core-mediated-capability-settlement.md`](decisions/0002-core-mediated-capability-settlement.md)
and [`engine-boundary-spike.md`](engine-boundary-spike.md) specify the candidate
Core-mediated direction and the smallest experiment. Both remain Draft until
their acceptance gates are executable.

## Decisions that remain open

The following choices are deliberately not frozen by this draft:

- the binary intent, plan, receipt, and event encodings and resource bounds;
- the winning engine callback shape from the disposable spike;
- which exact Token and Token-2022 profiles the first Core major supports;
- how engine code identity and engine upgrades are represented;
- the external settlement-driver authority boundary;
- stored-intent funding, cancellation, and revocation mechanics;
- persistent custody, position accounting, and exit profiles;
- exact supported fee asset profiles, collector address, shard topology, and
  fee codec; the five-basis-point rate, objective basis, floor rounding,
  same-asset rule, zero minimum, and immutable collector policy are decided;
- event transport and checkpoint encoding; and
- the exact pre-production release-candidate process; every Core that accepts
  production assets is already required to be adminless and immutable.

Each choice needs an accepted decision record, executable compatibility tests,
and adversarial tests before implementation is considered stable.
