# 0002: Use a core-mediated authority kernel

- Status: Draft
- Date: 2026-08-28

## Context

Programmable must let an engine define arbitrary market logic without handing it
ambient authority over users, Core custody, or unrelated liquidity. Solana does
not provide a hook sandbox. A program can combine every account and executable
program passed to it and can introduce its own PDA signers.

The architecture cannot make arbitrary code safe by trusting a manifest or
receipt. It must separate programmable economics from protected asset authority.

## Direction under evaluation

Use one Core-mediated execution envelope:

- a market permissionlessly selects an engine and new domains, while an existing
  domain's own local admission rule authorizes any additional market and exact
  engine revision;
- a user authorizes exact asset, recipient, amount, fee, and expiry limits;
- the Core invokes the engine with a reduced capability closure;
- the Core alone executes supported protected-value movement and the mandatory
  protocol fee; and
- the Core emits evidence that distinguishes observed facts from engine meaning.

No Programmable server, API, indexer, listing approval, or offchain signer is on
the execution path. Ordinary market execution has no global writable registry
or fee counter.

The exact number and ordering of engine callbacks is not decided here. The
disposable spike compares a pre-settlement state transition, read-only validation
plus post-settlement commit, and an engine-generated plan.

## Honest engine boundary

The engine is not a cryptographic Core signer. It is nevertheless the economic
authorization oracle for every domain that selected it. If compromised, it may
approve exchanges that drain or corrupt those domains while all Core transfers
remain technically valid and conservative.

The guarantee is containment: an engine receives no protected signer, delegate,
writable asset account, or non-participating Core domain. Its own PDA signers are
treated as ambient authority. The Core validates the actual CPI capability
closure, not a fictional nested-call script.

Markets that share a liquidity domain deliberately share reserves, locks,
economics, engine risk, and liveness risk. Domain identities include their
controller and revision context; a namespace string alone is not isolation.
Declaring a domain in a plan grants nothing. The Core verifies a domain-local
participation relation among domain, market, and engine revision. A domain may
choose an open rule; Programmable does not approve that choice.

## Programmability boundary

Curves, auctions, orders, spreads, dynamic fees, provider economics, game state,
and unnamed future market behavior belong to engines. The Core does not encode a
product enum.

Authority primitives are intentionally narrower. New ways to move protected
value may require a side-by-side Core major and new hostile tests. This is not a
limit on engine logic; it is the boundary that keeps an unknown program from
becoming a vault signer.

The first spike implements one exact SPL Token profile. Token-2022 callback
graphs, first-class external settlement drivers, direct-user-signer asset
programs, and asynchronous intents are separate decisions. They remain product
requirements, not unreviewed fields frozen into the first ABI.

An engine can already compose arbitrary programs over its own state and PDAs.
A first-class driver earns a Core abstraction only if experiments prove a real
shared settlement capability and a safe authority boundary.

## Fees

The Core-owned market record selects the mandatory protocol fee policy. Users
authorize ceilings. The Core derives the actual assessment once, executes it
through a supported Core-native profile, and records observed debit, fee-vault
credit, accounted liability, and later claims separately.

The engine and caller cannot replace the policy, select a zero fee, or redirect
the recipient. Donations do not create fee liability. Claims cannot exceed the
accounted liability or use a caller-selected destination.

The universal revenue claim is one mandatory fee per successfully committed
Core envelope. A volume fee is enforceable only on an exact Core-observed basis.
An opaque program can batch internal actions or expose a separate entrypoint, so
the Core cannot guarantee a percentage of unknown volume or a fee on every
semantic action outside it.

Provider, creator, engine, referral, spread, reserve growth, auction surplus,
rebate, and other market economics may be implicit or explicit. Their economic
meaning is engine-defined and is not Programmable revenue merely because an
asset effect is visible.

## Evidence

Core events contain Core and market identity, engine identity, plan digest,
participating domains, Core-verified supported-asset effects and protocol fees,
and opaque engine digests with an explicit attested evidence class.

Successful transaction status alone is not event authenticity. Indexers verify
the emitting program and invocation context, event discriminator, and a
shard- or state-bound checkpoint against Core state. No trade requires one
global or market-wide mutable sequence account.

## Liveness and exits

Website, RPC provider, API, indexer, or company-account failure cannot change
onchain rules. Engine failure is different: a market that requires that engine
can stop.

Persistent Core custody therefore cannot receive a universal escape claim. Each
market must bind an exit class: no persistent custody, an exact Core-verifiable
engine-independent claim, or disclosed engine-liveness dependence. A strong
escape profile is required before any persistent-custody configuration is
presented as independently withdrawable.

## Version and deployment model

Core majors are separate deployments with explicit interface meaning. New
majors coexist with old ones; clients and markets choose them explicitly. A new
major is a surgical extension, not a silent rewrite of old market rules.

No production Core is made immutable until its accepted interface has hostile
program tests, compatibility fixtures, realistic runtime measurements,
reproducible build evidence, and a public deployment manifest. A temporary
upgrade authority is itself a full trust assumption and cannot be described as
immutable or owner-compromise-safe.

## Deliberately not decided

- public engine callback and return-data ABI;
- stored intents, partial fills, matching, and funding delegation;
- external settlement-driver authority and evidence;
- Token-2022 support classes;
- persistent liquidity-position and exit accounting;
- loader-specific code pinning;
- fee amounts, assets, caps, recipients, and bounded update rules; and
- canonical event bytes.

These are separate decisions because each changes a different authority or
liveness boundary. No global plugin registry, allowlist, or arbitrary-call
administrator is introduced to avoid making them.

## Rejected assumptions

- **A manifest sandboxes arbitrary CPI:** it does not; actual accounts and
  privileges define the capability closure.
- **Conservation proves a fair trade:** a malicious engine may conserve assets
  while assigning them at a destructive price.
- **A narrow PDA is a safe generic delegate:** its first use can still be abused
  by an untrusted callee.
- **A receipt proves custom economics:** it proves which program returned bytes,
  not that their meaning is true.
- **All fees are explicit transfer legs:** provider economics may be implicit in
  state or reserves.
- **One immutable ABI can anticipate unknown Solana authorities:** new protected
  authority primitives may require a new Core major.
- **Future runtime proposals are current capacity:** launch design uses only
  active cluster limits.

## Acceptance gates

Before this direction becomes Accepted:

1. the engine-boundary spike must close all five first-stage proofs in
   [`../engine-boundary-spike.md`](../engine-boundary-spike.md);
2. measured transactions must retain packet, account, CPI-depth, and compute
   headroom under the pinned active runtime;
3. engine compromise and engine failure must be documented per participating
   domain without stronger claims;
4. mandatory fee derivation, accounting, and claim invariants must be
   executable; and
5. each deferred authority feature must remain outside the public ABI until its
   own decision and hostile tests exist.

Before any general or immutable engine ABI is accepted, stored and multi-intent
compatibility must prove an authorization-neutral plan boundary, and the
external settlement-driver decision must close the custom protected-value path.

This Draft authorizes an experiment and review, not a devnet or mainnet release.
