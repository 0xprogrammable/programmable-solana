# Solana runtime baseline

Status: Observed on 2026-08-28

This file records the runtime contract used to design and test the first
Programmable Core. It is not an eternal protocol promise. Every production
release must recheck active mainnet features and pin the validator source it was
tested against.

## Source baseline

- Mainnet genesis `5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d` reported
  `solana-core 4.2.1`, feature set `565236538`, at finalized slot `442328231`
  during this review.
- The matching Agave v4.2.1 source baseline is commit
  `c4b48df969a9e4f121e14a389bd7bec34c752507`.
- Current behavior was compared against the official Solana core, transaction,
  CPI, compute-budget, program-execution, instruction-introspection, and token
  extension documentation.

An RPC version string is evidence about that RPC node, not proof that every
validator runs identical software. Activation checks and executable tests are
the release gate.

## Active design constraints

| Constraint | Current design value | Consequence |
| --- | --- | --- |
| Legacy/v0 serialized transaction | 1,232 bytes | Plan and account encodings need packet-size tests |
| Locked accounts | 64 | Address lookup tables compress addresses but do not remove current lock limits |
| Compute | 1,400,000 CU maximum per transaction | Core, engine, driver, token callbacks, and router share one meter |
| Instruction stack | height 5 | Top level plus four nested invocations; keep the protocol topology flat |
| Instruction trace | 64 executed instructions | Fan-out and callbacks are bounded independently of stack depth |
| Return data | 1,024 bytes | Any return data or receipt must fit and be read immediately |
| CPI instruction data | 10,240 bytes | Large opaque plans belong in authenticated accounts or hashes |
| CPI account infos | 128 under the current documented path | The transaction account-lock limit is still the tighter launch constraint |

Transaction v1, a 4,096-byte message, 128 locked accounts, a deeper CPI stack,
and other proposed limits are not assumed until the relevant features are active
on the deployment cluster and compatibility tests pass.

## CPI semantics that shape the protocol

- A callee cannot escalate inherited signer or writable privilege.
- A program can introduce signer privilege for its own PDAs with
  `invoke_signed`; that authority is real and must be threat-modeled.
- Every account and executable program used by a descendant must be in the
  transaction and forwarded through each ancestor.
- Duplicate positional account entries may be required by an ABI, while
  effective privileges for one public key must be normalized for security
  checks.
- Compute consumption is shared across the call tree.
- Direct self-reentry is allowed. Indirect reentry such as `A -> B -> A` is
  rejected by the current runtime.
- A failed CPI aborts its caller. There is no catchable error path for trying a
  second settlement strategy.
- Return data is one last-writer-wins slot and is cleared for a new invocation.
  A receipt setter must write after its final nested CPI and its caller must read
  immediately.

These rules justify a capability closure rather than a fictional nested-call
allowlist.

## Transaction and account semantics

- All successful instructions in a transaction commit atomically; account-state
  writes roll back on failure.
- The transaction fee and execution metadata are not rolled back. "Atomic" in
  protocol documents means account-state atomicity.
- Writable locks are taken from the outer transaction message. Passing an
  account read-only to an inner CPI reduces authority in that CPI but does not
  improve outer transaction parallelism.
- Only an account's owner program may modify its data or debit its lamports,
  subject to runtime rules. Passing a foreign account writable gives the callee
  the ability to ask the owner program to mutate it through CPI; it does not
  transfer ownership.
- Account closure, reinitialization, discriminator substitution, owner changes,
  and stale references are lifecycle attacks and require explicit tests.

## Token boundary

Token and Token-2022 behavior is part of settlement correctness, not a UI detail.
Profiles must account for transfer fees and inverse rounding, transfer hooks and
their extra-account lists, Permanent Delegates, freeze and close authorities,
mint closure or reinitialization, required memos, metadata, and checked decimal
semantics.

The Core-native class is an exact supported profile, not "all Token-2022". A
mint whose external authority can independently alter a Core vault cannot
receive a strong custody guarantee. Other behavior remains permissionless
through a separately accepted external settlement boundary with narrower
evidence.

## Event boundary

Logs from a failed transaction can still appear in RPC metadata, and an
untrusted program can emit Core-shaped bytes. An indexer verifies transaction
success, emitting program and invocation context, discriminator, and a Core
state checkpoint. Text log resemblance alone is not evidence.

## Required revalidation

Before any devnet release candidate and again before mainnet:

1. record cluster genesis hash, slot, software version, and active feature set;
2. pin the exact Agave source and supported loader/token program deployments;
3. run runtime-semantic fixtures under LiteSVM or Mollusk;
4. run realistic callback and extension flows under an embedded Surfpool fork;
5. run a small devnet smoke suite; and
6. update this file in a reviewable commit if any observed constraint changed.

## Primary references

- [Solana transactions](https://solana.com/docs/core/transactions)
- [Transaction structure](https://solana.com/docs/core/transactions/transaction-structure)
- [CPI execution and privileges](https://solana.com/docs/core/cpi/cpi-execution)
- [Compute budget](https://solana.com/docs/core/fees/compute-budget)
- [Program execution and return data](https://solana.com/docs/core/programs/program-execution)
- [Instruction introspection](https://solana.com/docs/core/instructions/instruction-introspection)
- [Token extensions](https://solana.com/docs/tokens/extensions)
- [Transfer-hook integration](https://solana.com/docs/tokens/extensions/transfer-hook-integration)
- [Pinned Agave source](https://github.com/anza-xyz/agave/tree/c4b48df969a9e4f121e14a389bd7bec34c752507)
