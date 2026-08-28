# Threat model

Status: Draft

This document names the trust boundaries for Programmable Solana. It prevents a
security claim about the core from being mistaken for a guarantee about an
engine, token, interface, operator, or the Solana network.

## Protected assets and state

- user assets that have not been authorized for the current instruction;
- assets held in core-custodied market vaults;
- canonical market identity, fee configuration, and settlement state;
- declared capability boundaries around engine-owned accounts;
- source, release artifacts, deployment manifests, and authority records; and
- availability of unrelated markets when one market or engine fails.

## Trust matrix

| Actor or component | Power if compromised | Required boundary |
| --- | --- | --- |
| User key or wallet | Can authorize transactions for that user | The protocol cannot recover a stolen key; signed instructions still bind exact intent and limits |
| Frontend or client | Can lie, omit data, or construct a hostile transaction | Holds no protocol authority; settlement remains inside signed asset, recipient, amount, fee, and expiry bounds |
| RPC provider | Can censor or fabricate account data, signatures, slots, and finality claims returned to a client | Cannot alter canonical finalized state or create a validator signature; critical reads need independent comparison or verifiable ledger evidence |
| Core program | Controls shared validation and core-custodied settlement | Treated as systemic security-critical code with explicit releases and invariants |
| Core upgrade authority | Can replace the core and defeat code-level controls | Full trust is disclosed until the authority is constrained or removed |
| Configuration or pause authority | Can invoke only powers implemented for that role | Scope, controller, and every change are public and separately tested |
| Engine program | Controls its logic and declared engine-owned state | Cannot write another market's core state or move assets outside core settlement bounds |
| Engine upgrade authority | Can change that engine and damage markets that trust it | Engine identity and mutability are visible; risk is not presented as core certification |
| Shared engine state | Can couple the safety and availability of multiple opt-in markets | Requires an explicit capability and lies outside core cross-market isolation |
| Token program, mint, or authority | May impose extensions, freezes, fees, transfer hooks, or malicious behavior | Unsupported behavior fails closed; support never certifies the asset |
| Indexer or API | Can omit, delay, mislabel, or fabricate offchain views | Holds no settlement authority; canonical state remains independently discoverable |
| GitHub or maintainer account | Can alter future source and review evidence | Protected branches, independent reviewers, signed releases, and external artifact verification |
| Build and release pipeline | Can substitute an artifact or metadata | Reproducible builds, authenticated attestations, and onchain hash verification |
| Deployment signer | Can deploy or upgrade within its onchain authority | Identity, scope, rotation, recovery, and removal are recorded in public manifests |
| Solana runtime and validator set | Defines execution, consensus, and finality | Base-chain compromise is outside the protocol's ability to contain |

## Primary threat classes

### User-intent substitution

A hostile interface may request a valid signature for the wrong engine, market,
asset, amount, recipient, fee, or lifetime. Every asset-moving instruction must
carry explicit limits that the core enforces; visual wallet interpretation is an
additional defense, not the invariant.

### Cross-market authority escalation

A malicious engine may alias accounts, substitute vaults, exploit shared state,
or return crafted settlement data. Core-owned accounts and vaults remain
market-isolated. Engine-owned shared state is allowed only through an explicit
write capability whose coupled risk is visible to every participating market.

### Hostile asset behavior

Token extensions, transfer hooks, freeze authorities, fees, rebasing behavior,
or nonstandard programs may invalidate ordinary accounting assumptions. The
first core version must enumerate supported behavior and reject everything else
until an adapter has its own invariants and tests.

### Administrative compromise

A compromised website or indexer can deceive or censor but cannot settle a
transaction by itself. A compromised upgrade authority is different: it can
replace the program and may seize program-controlled assets. Documentation must
never collapse these threats into the same phrase.

### Supply-chain substitution

Repository protection cannot prove that a deployed binary came from reviewed
source. Releases need authenticated tags or attestations, reproducible artifacts,
and an independent match between source, ELF hash, deployment transaction, and
onchain program data.

### Availability coupling

One globally writable registry, accumulator, or authority account can serialize
all markets. Ordinary settlement must use market-local or sharded writes, and an
engine's compute or state failure must not propagate to unrelated markets.

## Non-goals

The core cannot guarantee profitable markets, honest token issuers, correct
engine economics, complete third-party interfaces, unbounded Solana history from
every RPC, recovery of stolen user keys, or base-chain safety.

Before an implementation property moves from Draft to Accepted, its tests must
name the hostile actor, the accounts and authority it controls, the protected
asset, and the observable failure condition.
