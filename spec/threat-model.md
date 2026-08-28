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
| Engine program | Is the economic authorization oracle for participating domains and controls its logic, PDA signers, closure, and state | May economically drain participating domains; receives no protected signer or writable asset account and cannot debit or alter accounted rights in non-participating Core domains; unsolicited raw credits remain possible |
| Engine upgrade authority | Can change that engine and damage markets that trust it | Engine identity and mutability are visible; risk is not presented as core certification |
| Future external settlement driver | Could own custom state and affect every account under its program authority | Has no accepted Core interface until its accounts, ambient authorities, code identity, evidence, fees, and liveness boundary are proven |
| Shared liquidity domain | Couples reserves, safety, locks, and availability across opt-in markets | Participation is explicit; isolation is promised only from non-participating domains |
| Domain admission authority | Can add market, engine, interface, code-policy, and capability relations when the local rule is mutable | Descriptor, rule, controller, revision, and changes are visible; liquidity providers accept that domain-local trust or select an immutable rule |
| Token program, mint, or authority | May impose extensions, freezes, fees, delegates, burns, transfer hooks, or malicious behavior | Strong Core-native support requires an exact safe profile; other behavior needs a separately accepted external settlement boundary |
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

A malicious engine may alias accounts, introduce its own PDA signers, exploit an
existing token delegate, combine every program in its CPI closure, substitute
vaults, exploit shared state, or return crafted settlement data. Engine phases
receive no protected writable or signing capability. Shared state is allowed
only through an explicit domain whose coupled risk is visible to every
participating market.

An attacker cannot make a victim domain participating by naming it in a new
market or plan. The domain's local admission rule must authorize the exact
domain, market, and engine-revision relation. A deliberately open domain is a
choice by that domain's liquidity providers, not a protocol-wide allowlist.

The Core cannot enforce an engine's claimed nested CPI script. It authenticates
the closure of available capabilities and keeps its own guarantees independent
of engine economic assertions.

An engine does not need a Core signer to cause economic loss in a domain that
selected it. It can approve an unfair plan and ask the Core to move protected
assets within that participating domain. The Core contains this risk but cannot
distinguish a novel pricing rule from a malicious one.

### Hostile asset behavior

Token extensions, transfer hooks, Permanent Delegates, freeze or close
authorities, fees, rebasing behavior, or nonstandard programs may invalidate
ordinary accounting assumptions. A callback can also revisit engine state after
an earlier receipt. The first Core major must publish exact strong profiles,
resolve callback extras, and reject cross-phase aliases. Behavior outside those
profiles remains possible only through a separately accepted weaker authority
boundary; it is not silently certified.

Raw vault balances are not accounted reserves. Donations cannot mint
Core-native shares or alter Core accounting. A free engine that deliberately
prices from visible raw balances owns that pricing risk; the Core does not claim
otherwise.

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

A market that leaves assets in Core custody may still depend on its engine to
calculate entitlement. Before mainnet, every such market must bind either a
Core-verifiable engine-independent exit rule or an explicit engine-liveness risk.

## Non-goals

The Core cannot guarantee profitable markets, honest token issuers, correct
engine or external-program economics, isolation between markets that deliberately
share one liquidity domain, percentage fees on unknowable opaque semantics,
engine-independent exit from an arbitrary engine-defined position,
complete third-party interfaces, unbounded Solana history from every RPC,
recovery of stolen user keys, or base-chain safety.

Before an implementation property moves from Draft to Accepted, its tests must
name the hostile actor, the accounts and authority it controls, the protected
asset, and the observable failure condition.
