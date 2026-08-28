# Engine boundary spike

Status: Draft experiment plan

This document defines the smallest executable experiment needed before a public
Programmable engine ABI is designed. Names and structures here are descriptive,
not frozen wire formats.

## Question

Can one Solana Core program let a permissionless market engine approve arbitrary
market logic while the Core alone controls supported asset movement, user
limits, mandatory protocol fees, and isolation from non-participating liquidity
domains?

The spike answers that question. It does not implement the full DEX.

## Smallest execution

```text
direct user
  -> Core.execute(candidate plan)
       1. authenticate the exact top-level instruction and user limits
       2. derive the actual engine CPI capability closure
       3. invoke one selected engine
       4. execute bounded SPL Token legs
       5. derive and transfer the mandatory Core fee
       6. emit a minimal evidence header
```

Every failure aborts all account-state changes. Solana transaction and priority
fees and failed-transaction metadata still exist.

The engine may run arbitrary logic and CPIs over the capabilities made available
to it. The Core does not enumerate curves, actions, products, or engine-owned
state machines.

## Economic authority is explicit

The engine does not possess a Core signer, but it is the economic authorization
oracle for every participating liquidity domain. A compromised engine may
approve a terrible exchange and drain or corrupt those domains through transfers
the Core executes correctly. Conservation is not fair pricing.

The Core contains that risk; it does not erase it. It grants or forwards no new
authority over non-participating domains or unexposed user assets. Independently
pre-existing delegates or mint authorities remain outside that guarantee.

Markets must bind the selected engine revision or explicitly accept mutable
engine behavior. A later code-identity design will be loader-specific and must
be proven executable within current compute limits.

## Candidate plan

The direct caller supplies one canonical candidate plan. The Core and engine
bind the same bytes. At minimum the plan commits to:

- Core program and interface experiment version;
- market and exact engine identity;
- participating liquidity domains and their authorization proofs;
- ordered supported-asset legs with exact accounts and proposed amounts;
- separately signed user recipients, maximum debits, and minimum credits;
- an opaque engine payload digest;
- the authenticated Core fee policy and user fee ceiling; and
- expiry.

The engine may interpret the opaque payload however it wants. The Core interprets
only authority-bearing fields. It recomputes canonical hashes from actual
instruction bytes and accounts. In the caller-supplied variant, proposed leg
amounts are exact and checked against separate user bounds. In the
engine-generated variant, the engine returns exact legs and the Core applies the
same bounds. The Core then derives the protocol fee once from the accepted legs.

The spike compares this caller-supplied plan against an engine-generated plan.
No ABI choice is accepted until packet size, compute, return-data handling, and
client complexity are measured.

## CPI capability closure

The closure is the actual ordered account list passed to the engine CPI together
with the selected engine program. The Core derives effective privileges and
protected roles by public key. It does not serialize redundant privilege groups
or blindly deduplicate positional accounts.

The closure is a capability ceiling, not a nested-call script. Once an engine
sees multiple programs and accounts, it may combine them in any order allowed by
Solana. The Core cannot enforce claimed inner instruction bytes.

An engine can introduce signers for its own PDAs with `invoke_signed`. Those
signers are ambient engine authority inside the closure.

### Protected authority rule

The engine receives no effective capability that can move protected user or Core
value. This includes:

- no user signer;
- no Core vault, escrow, custody, fee, or market-authority signer;
- no intent, permit, delegate, owner, close authority, Permanent Delegate, or
  other PDA signer accepted by a protected asset program;
- no protected user asset account, Core vault, fee vault, or non-participating
  domain account as writable; and
- no ability to substitute the owner, mint, recipient, domain, or selected
  engine through aliases or duplicate account metas.

A narrow seed and one-shot nonce do not make a generic token delegate safe. A
malicious callee can abuse the first authorized use. The Core alone executes
protected-value movements through exact supported programs after engine
approval.

For the strong first SPL Token profile, Core vaults have no token delegate and
only the exact accepted close-authority configuration. A delegate independently
granted on a user account is not neutralized by the Core; the spike either
rejects that account or treats the external delegate risk as outside its
transaction authorization guarantee.

Passing an outer writable account read-only to the engine reduces authority for
that CPI but does not reduce the transaction's outer write lock.

## Participating domains

A domain reference is separated by controller program, interface or revision,
namespace, and identity. Two markets that select the same domain deliberately
share reserves, locks, economics, engine risk, and liveness risk.

Calling a domain "participating" does not authorize it. Every Core-owned domain
binds an immutable or explicitly controlled local admission rule. Each execution
proves the authorized relation among domain, market, and exact engine revision.
A rule may deliberately allow any compatible market or several engines, but its
liquidity providers accept that rule when they enter the domain.

Anyone may permissionlessly create a market with a new domain they control under
the public rules. Permissionless admission does not allow a new market to adopt
someone else's existing domain without that domain's own authorization. This is
a domain-local capability, not a Programmable allowlist or global registry.

The Core promises that the engine cannot reach a non-participating Core-owned
domain. It cannot infer universal domain labels for arbitrary opaque accounts
owned by other programs.

## Asset scope of the first spike

The first spike moves only a narrow, exact SPL Token profile implemented by the
Core. It is not the permanent product boundary.

Token-2022 transfer hooks, Permanent Delegates, transfer fees, and custom asset
programs require separate callback and accounting experiments. They are not
silently treated as ordinary SPL Token behavior.

An engine may already call arbitrary programs over its own accounts and PDAs.
Before the protocol makes a general-purpose asset claim, a separate required
decision must prove whether and how an external settlement driver safely moves
custom protected value that the engine closure intentionally cannot. That design
must bind its actual accounts, code identity, ambient authorities, call order,
fee funding, evidence class, and liveness boundary. Direct-user-signer asset
programs are intentionally outside this spike.

This staging limits the experiment, not what the eventual protocol may express.

## Authoritative protocol fee

There is one fee truth:

1. a Core-owned market record binds the protocol fee policy;
2. the user authorizes only a ceiling;
3. the Core derives the mandatory assessment once;
4. the Core transfers it through the supported asset profile; and
5. the event reports the observed result.

The engine and caller cannot supply a zero policy, alternate recipient, or
second fee schedule.

For the direct single-user spike, a protocol assessment binds:

- the exact Core-native asset profile and basis leg;
- gross or net basis and whether the leg has already been assessed;
- fixed amount or rate, denominator, rounding, minimum, maximum, and overflow
  behavior;
- authorizing user and funding source;
- policy-derived recipient shard; and
- observed source debit and fee-vault credit.

Only a Core-verified fee-vault credit increases accounted protocol-fee
liability. Donations do not. Claims cannot exceed that liability, use a
caller-selected recipient, or reduce liability without the atomic transfer.

The universal business claim is one mandatory fee per successfully committed
Core envelope. An opaque program can batch several semantic actions or expose a
separate entrypoint, so the Core cannot honestly claim a fee per unknowable
internal trade.

Market-defined provider economics may be explicit transfers, implicit reserve
growth, position fee growth, spread, auction surplus, funding, rebates, or other
engine logic. Their meaning is engine-attested. They are not Programmable
protocol revenue merely because a Core-native debit or credit is observable.

## Minimal evidence

A successful spike emits only:

- Core, market, engine, and plan identity;
- participating domain identities;
- Core-verified supported-asset debits and credits;
- the Core-verified protocol-fee assessment and accounted liability change;
- an opaque engine result or state digest labeled `EngineAttested`; and
- a shard- or state-bound checkpoint that creates no global or market-wide hot
  counter.

Indexer acceptance requires transaction success, the actual Core invocation
context and discriminator, and consistency with Core state. Event-shaped bytes
from an engine are not canonical evidence.

## Engine callback variants to measure

The spike implements these variants behind disposable test interfaces:

1. one writable `validate_and_transition` CPI before settlement;
2. read-only validation before settlement plus a writable commit after all asset
   effects; and
3. an engine-generated plan returned before settlement.

The post-settlement variant must be the last account-bearing untrusted CPI. It
exists to test whether downstream callbacks can invalidate earlier engine state.
Solana return data is read immediately after its setter's final nested CPI and
is treated as authenticated bytes, not proof of economic truth.

The chosen public shape is decided by measured safety, compute, account count,
packet size, and engine ergonomics. `prepare`, `commit`, receipt structs, and
return-data formats are not accepted by this document.

## Five first-stage proofs

1. A hostile engine cannot write or move value in a non-participating domain or
   protected user account through authority granted by Core, including through
   aliases, forwarded delegates, or engine PDA signers. Relabeling a victim
   domain as participating without its own admission proof fails.
2. Duplicate and reordered metas do not escalate effective privilege or change
   a protected role.
3. Engine failure, transfer failure, fee failure, or a failed final callback
   rolls back all protocol account state.
4. User limits and the protocol fee each have exactly one authoritative source;
   a missing, zeroed, redirected, duplicated, netted, or dust-split fee attempt
   fails or follows the bound policy.
5. The realistic transaction retains explicit headroom under current packet,
   locked-account, CPI-depth, and compute limits.

Fast invariant and hostile-program tests use LiteSVM or Mollusk. Realistic CPI
and fork scenarios use embedded Surfpool after the first stage. Devnet smoke
tests are release evidence, not a PR gate.

Start with Anchor because it makes account constraints and experiment code
clearer. Compare a lower-level implementation only if measured compute, stack,
heap, packet, or account margins are inadequate.

## Required before persistent Core custody

An engine can disappear, reject every call, or fail its pinned code policy. Any
market that leaves assets in Core custody must therefore bind an exit class at
creation:

- no persistent custody;
- a Core-verifiable engine-independent claim and withdrawal rule; or
- explicit engine-liveness dependence with no claim of an independent escape.

Arbitrary engine economics do not imply that the Core can derive every user's
entitlement. A strong "escapable" label is reserved for an exact Core-native
exit profile. Defining and testing that profile is a mainnet blocker, not part
of the first topology spike.

## Deferred into separate decisions

- stored and detached intents, partial fills, matching, cancellation, and
  cumulative fee attribution;
- first-class external asset or settlement drivers;
- Token-2022 support profiles and callback alias rules;
- loader-specific mutable, pinned, and immutable code identity;
- persistent custody, position accounting, and exit profiles;
- fee rates, assets, caps, recipients, and any bounded update mechanism; and
- canonical event transport and compatibility bytes.

Each deferred capability gets its own decision and hostile test. None is solved
by a global administrator, allowlist, or arbitrary upgrade call.

No general or immutable engine ABI is accepted until either its plan boundary is
proven authorization-neutral or compatibility fixtures cover direct, stored,
and multi-intent authorization and fee attribution. Likewise, no general-purpose
asset claim is made until the external settlement-driver decision is closed.

## Output of the spike

The result is a measured decision record containing the winning callback shape,
exact source commit, tests, CU and account profiles, packet fixtures, rejected
alternatives, and unresolved risks. It may propose a topology boundary for the
next experiment; it does not by itself authorize naming a general engine ABI
`v0`.
