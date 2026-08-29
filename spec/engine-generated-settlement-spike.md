# Engine-generated settlement spike

Status: Implemented experiment; private non-production contract

This document authorizes one disposable experiment. It does not accept a public
engine interface, a Core account layout, a deployment artifact, or a product
feature.

## Decision to falsify

The experiment tests one narrow hypothesis:

> A direct user can authorize an exact classic-SPL input and bounded total debit,
> a permissionless engine can derive the exact classic-SPL output while using a
> small ordered set of opaque external capabilities, and Core can settle the
> result without exposing any fixed user, custody, fee, or Core-owned capability
> to that engine.

The hypothesis is false if the experiment needs to:

- give the engine a user or Core signer;
- forward a fixed settlement account into the opaque capability plane;
- interpret arbitrary external-program state inside Core;
- trust caller-declared privileges instead of the privileges actually available
  to the engine CPI;
- treat a receipt as proof that custom economic meaning is true;
- leave partial engine, token, fee, or Core state after a failed instruction; or
- exceed an active Solana packet, locked-account, call-depth, return-data, or
  compute limit at the experiment maximums.

Passing this spike permits a measured architecture comparison. It does not select
the engine-generated shape for a public ABI.

## Isolation rules

Any implementation authorized by this document must live in an isolated nested
Cargo workspace under `experiments/engine-generated-settlement/`.

The nested workspace must:

- have its own workspace manifest and lockfile;
- remain outside the root workspace membership and default build;
- mark every crate `publish = false`;
- use names, discriminators, account layouts, seeds, program IDs, and codecs that
  are explicitly disposable;
- avoid imports from, re-exports through, or additions to the canonical Core and
  engine-interface crates;
- expose no maintained IDL, SDK, client package, compatibility promise, release
  tag, migration path, or public ABI;
- contain no deployment keypair, upgrade authority, cluster address, deploy
  command, or mainnet/devnet configuration; and
- run only in an in-process or local test runtime. A local fork may be used for
  runtime realism but is not a deployment.

No account created by the spike may receive real funds. The entire workspace may
be deleted after its result is recorded through a reviewable change that also
removes its repository-policy and specification references.

## Fundamental security boundary

Core protects only the fixed classic SPL Token settlement plane defined below.
It understands the exact mints, user token accounts, domain vaults, fee vault,
Core-owned state, user limits, and Core fee involved in that plane.

The opaque plane is deliberately weaker. Core authenticates its capability
closure but does not understand the data, authority rules, economics, solvency,
or side effects of arbitrary external programs and accounts.

A hostile engine may read, write, close, corrupt, or economically misuse any
custom writable capability explicitly supplied in the opaque closure whenever
the owning external program and ambient engine authority permit it. The engine
may introduce signers for its own PDAs. A supplied external program may assign
those signers dangerous meaning. Those effects are expected opaque-plane risk,
not Core-verified settlement.

The containment claim is narrower and testable:

- the engine can affect only the external capabilities explicitly present in its
  validated CPI closure plus authority it already owns;
- no fixed settlement account, incoming signer, Core-owned account, or writable
  token-owned account enters that closure; and
- opaque-plane success or failure cannot create an accounted Core balance,
  protocol-fee liability, provider claim, or statement about custom economic
  meaning.

This experiment does not sandbox arbitrary Solana code. It proves a capability
boundary around one fixed settlement profile.

Programmable semantics do not imply arbitrary authority. An existing program
that requires the wallet signer during its own CPI cannot necessarily be called
unchanged through this engine boundary. It needs a separate top-level
instruction, a deliberately scoped intent or delegation, an engine PDA authority
that it recognizes, or a separately proven settlement driver. The intended
freedom is arbitrary semantics over explicitly declared capabilities, never an
untrusted engine receiving every wallet or custody capability.

## Fixed exact-input envelope

The only top-level shape under test is:

```text
direct user
  -> DisposableCore.execute_engine_generated_exact_in(request)
       1. authenticate the direct call, fixed accounts, user bounds, and expiry
       2. validate and digest the ordered opaque capability closure
       3. preflight exact classic-SPL input, output reserve, and Core fee funding
       4. invoke the selected engine once
       5. authenticate the engine-generated exact output receipt
       6. execute the fixed classic-SPL input, fee, and output transfers
       7. verify exact token deltas and update accounted state
       8. emit one disposable measurement event
```

The fixed account set is closed and positional:

| Role | Required privilege | Required relation |
| --- | --- | --- |
| user | signer; writable permitted | authorizes this exact instruction and may also pay the transaction fee |
| market | read-only | canonical disposable market |
| domain | writable, non-signer | canonical exclusive disposable domain |
| fee ledger | writable, non-signer | canonical ledger for this market and input mint |
| input mint | read-only, non-signer | classic SPL Token mint selected by the market |
| output mint | read-only, non-signer | distinct classic SPL Token mint selected by the market |
| user input account | writable, non-signer | classic SPL Token, exact mint and user authority |
| user output account | writable, non-signer | classic SPL Token, exact output mint and recipient |
| domain input vault | writable, non-signer | canonical vault owned by the domain PDA |
| domain output vault | writable, non-signer | canonical vault owned by the domain PDA |
| fee vault | writable, non-signer | canonical input-mint vault owned by the fee-ledger PDA |
| selected engine program | executable, read-only | exact program bound by the market |
| selected engine state | writable, non-signer | exact state bound by the market and owned by the engine |
| Instructions sysvar | read-only, non-signer | canonical address |
| classic SPL Token program | executable, read-only | exact supported program |

Core must reject every undeclared fixed account and every privilege outside the
table; user writability is explicitly permitted because transaction-level
privileges are unioned and the user may be the fee payer. The direct user
instruction contains no remaining settlement roles. The ordered opaque closure
is the only bounded trailing account region.

Before the engine CPI, Core must establish all of the following:

- the invocation is top-level, not routed through another program;
- every fixed PDA, owner, mint, vault, market, domain, engine, and fee relation is
  canonical and live;
- both mints and every protected token account satisfy the spike's exact classic
  SPL profile;
- fixed protected token accounts have no unsupported delegate, close authority,
  or lifecycle state;
- `amount_in > 0`, the request is unexpired, and checked arithmetic can represent
  the maximum total debit;
- the user input account can fund `amount_in + protocol_fee` before untrusted code
  runs;
- raw vault balances cover, but do not increase, their accounted balances; and
- the accounted output reserve can cover at least the user's requested minimum.

The engine does not receive any fixed mint, token account, vault, market, domain,
fee, user, token-program, or Core-owned account in its CPI account list.

## Ordered opaque capability closure

The spike accepts between zero and eight opaque external account positions after
the fixed envelope. It also accepts an opaque engine payload of at most 128
bytes.

`8` accounts and `128` payload bytes are measurement limits for this experiment.
They are not product limits, public constants, ABI promises, or evidence that a
future interface should use the same values.

### Position and duplicate semantics

External duplicate public keys are allowed because positional duplicates can be
part of an external program's ABI. Order and multiplicity are preserved.

Security validation is nevertheless by public key, not by a caller's positional
label:

1. Core reads the actual `AccountInfo` key, owner, executable flag, signer flag,
   and writable flag available to the current instruction.
2. Core groups duplicate positions by public key and derives that key's actual
   effective signer and writable privileges. A read-only-looking duplicate does
   not hide a writable or signer occurrence elsewhere.
3. Core applies every rejection rule to the normalized public-key capability.
4. Each original position is retained in order, but its digest entry includes
   the normalized actual effective privileges. All occurrences of one public key
   therefore commit to the same effective capability.
5. Core constructs the engine CPI metas from the validated normalized effective
   privileges. It does not trust an opaque manifest or requested privilege bit.

The closure digest uses a domain-separated canonical encoding. For every
position it binds at least:

```text
position || public_key || current_owner || executable
         || effective_signer || effective_writable
```

Changing order, multiplicity, owner, executable state, or any actual effective
privilege must change the digest.

The user supplies the expected closure digest in the signed Core instruction.
Core independently derives the digest from landing-time account owners,
executable state, order, multiplicity, and effective privileges, then requires
an exact match before the engine CPI. Core must not silently replace the user's
commitment with a digest derived only after the transaction was signed.

### Mandatory rejection rules

Core must reject the entire instruction before the engine CPI if any opaque
position or normalized duplicate group:

- aliases any fixed-envelope account or the Disposable Core program ID;
- has effective signer privilege, regardless of whether that signer is the user,
  payer, another transaction signer, or a duplicated alias;
- is owned by Disposable Core, whether read-only or writable;
- is both executable and effectively writable;
- is effectively writable and owned by classic SPL Token or Token-2022; or
- would cause the closure to exceed eight positions or the opaque payload to
  exceed 128 bytes.

Read-only executable external programs are allowed. Read-only token-owned
external accounts are allowed as opaque observations. Writable accounts owned by
custom external programs are allowed and carry the explicit risk described
above. Core makes no general safety claim about those owners.

The selected engine may forward the validated opaque closure, combine its
accounts and executable programs in any permitted order, and add signer authority
for its own PDAs. Core validates the capability ceiling; it does not attempt to
enforce a fictional nested CPI script.

## User request and engine binding

The direct user request authorizes exactly:

- Disposable Core experiment version and program ID;
- market, domain, selected engine program, and selected engine state;
- every fixed mint, token account, vault, fee ledger, and recipient;
- exact `amount_in`;
- minimum acceptable output credit;
- maximum protocol fee and maximum total input debit;
- expiry slot;
- current fee-policy revision;
- the ordered opaque closure digest; and
- the opaque payload bytes or their canonical digest.

Core creates a domain-separated request digest from those values plus the current
accounted input reserve, output reserve, and fee liability. The engine instruction
contains a fixed-width header, that request digest, the exact input, the
pre-settlement accounted input and output values, the capability digest, and the
bounded opaque payload. User bounds and fee state remain committed by the request
digest and are enforced by Core; the engine does not need to reinterpret them.

The engine CPI account list contains only:

1. selected engine state, writable and non-signer;
2. the Instructions sysvar, read-only and non-signer; and
3. the validated opaque positions in their original order with normalized actual
   effective privileges.

The engine must authenticate the exact direct Disposable Core instruction using
the Instructions sysvar. A direct call to the engine with correctly shaped bytes
must fail.

After its final nested CPI, the selected engine writes one fixed-width disposable
receipt to Solana return data. The receipt binds:

- receipt magic and experiment version;
- the exact request digest;
- exact output amount;
- an engine-owned state sequence or nonce.

Core reads return data immediately after the engine CPI. It must reject missing,
trailing, malformed, stale, or wrong-version bytes; a setter other than the
selected engine program; a mismatched request digest; zero output; output below
the user's minimum; output above accounted output liquidity; or arithmetic that
does not fit the fixed settlement profile.

Return data authenticates which program returned which bytes. It does not prove
that the engine's price or custom external effects are fair, solvent, or true.

## CPMM reference engine

One disposable reference engine demonstrates only that CPMM pricing can remain
outside Core within this fixed, intentionally swap-shaped envelope. It does not
prove that this Core, wire format, or exact-input A-to-B account model is the
eventual product-neutral protocol boundary. The fixture implements an exact-input
constant-product quote using only the accounted reserves supplied in the
authenticated request and an engine-owned liquidity-fee parameter.

For input reserve `x`, output reserve `y`, exact pool input `dx`, basis-point
denominator `D = 10_000`, and engine liquidity fee `f` where `0 <= f < D`, it
computes with checked `u128` arithmetic and two explicit floor operations:

```text
effective_input = floor(dx * (D - f) / D)
amount_out = floor(y * effective_input / (x + effective_input))
```

The engine rejects zero reserves, zero input, invalid fee parameters, overflow,
zero output, and a result that cannot fit `u64`. The full `dx` is credited to the
input vault; `dx - effective_input` remains implicit in the new reserves. The
early floor deliberately favours the pool and can differ by a base unit from a
single-rational constant-product implementation. That is measured engine
economics, not Core policy.

Core does not reproduce or certify this formula. A hostile engine may return any
exact output that satisfies the user's bounds and available accounted liquidity.
Participating liquidity selected that engine and bears its economic risk.

## Separate Core protocol fee

The protocol fee is not part of the reference CPMM formula and is not chosen by
the engine. Disposable Core derives it once from the authenticated Core-owned
measurement policy and exact `amount_in`, using checked ceiling rounding.

The user authorizes both a maximum protocol fee and a maximum total input debit:

```text
total_input_debit = amount_in + protocol_fee
```

The engine cannot omit, zero, duplicate, net, redirect, or replace that
assessment. The input transfer credits exactly `amount_in` to the domain input
vault. A separate transfer credits exactly `protocol_fee` to the canonical fee
vault. Only the verified fee-vault credit increases accounted fee liability.
Raw donations create no liability.

The measurement rate, asset, recipient, rounding rule, and storage layout are not
accepted product economics.

## Atomic settlement and postconditions

After authenticating the receipt, Core performs exactly three classic SPL Token
effects:

1. debit `amount_in` from the exact user input account and credit the exact domain
   input vault;
2. debit `protocol_fee` from the same user input account and credit the exact fee
   vault; and
3. debit `amount_out` from the exact domain output vault and credit the exact user
   output account.

Core reloads every affected token account and verifies the exact observed debit
and credit for each leg. It then applies checked accounted-state changes:

```text
accounted_input_after = accounted_input_before + amount_in
accounted_output_after = accounted_output_before - amount_out
fee_liability_after = fee_liability_before + protocol_fee
```

The measurement event is emitted only after those checks. It distinguishes the
Core-verified classic-SPL effects and protocol fee from the opaque closure,
payload, request, and settlement digests.

Engine state mutation, external custom-program mutation, all token movements,
fee accounting, Core accounting, and the event occur in one Solana instruction.
Any engine, external CPI, receipt, token transfer, fee transfer, post-balance, or
accounting failure must roll back all account-state changes, including engine and
custom external state. Transaction fees and failed-transaction metadata are not
described as rolled back.

There is no post-settlement engine callback in this variant.

## Must-pass falsification tests

The result is invalid unless executable tests cover every item below with the
exact SBF artifacts used for measurement.

### Fixed-plane containment

- Table-drive every fixed-envelope role across representative first, middle,
  and final opaque positions, plus maximum-width duplicate privilege cases, and
  prove rejection before engine execution. A full Cartesian repetition is not
  additional security evidence.
- Reject the Disposable Core program ID and every Disposable-Core-owned account,
  including a read-only one.
- Reject the user, payer, or any other effective signer in the opaque closure.
- Reject an executable account with effective writable privilege.
- Reject writable classic-SPL-owned and Token-2022-owned accounts, including a
  duplicate whose other position appears read-only.
- Prove that duplicate or reordered positions cannot conceal effective privilege,
  change a protected role, or preserve the old closure/request digest.
- Prove that omitted fixed accounts, substituted markets, domains, engines,
  mints, vaults, recipients, fee ledgers, or token programs fail before any
  untrusted CPI.
- Prove that independently donated raw vault balances create no accounted output
  liquidity or fee liability.

### Honest opaque risk

- Allow one custom writable external account and prove a hostile engine can
  mutate it directly or through an explicitly supplied read-only executable
  external program.
- Repeat that attack without supplying the custom account and prove it cannot be
  reached.
- Prove that the same hostile engine receives no fixed token account, Core-owned
  state, incoming signer, or protected PDA capability.
- Demonstrate an economically destructive but conservative engine receipt that
  remains within the user's bounds and participating domain liquidity. Core may
  settle it; the test must label this engine risk rather than a Core safety claim.

### Request and receipt integrity

- Mutate every authority-bearing request field, representative first, middle,
  and final payload bytes, opaque order and position, and every effective
  privilege; each mutation must change the request digest or fail validation.
- Reject direct engine invocation, wrong engine program, wrong receipt setter,
  missing return data, return-data overwrite, malformed length, trailing bytes,
  wrong magic/version, stale request digest, replay against changed accounted
  state, and expired requests. The engine must inspect the exact top-level Core
  discriminator. A wrong-discriminator CPI is an executable test only if it can
  be constructed without adding a bypass-only Core entrypoint; otherwise the
  result records why only mutable Core code could originate it.
- Reject zero input, zero output, output below minimum, output above accounted
  liquidity, protocol fee above the user's ceiling, total debit above its
  ceiling, and each externally reachable checked-arithmetic class. Pure numeric
  boundaries may use the independent host model; tests need not manufacture
  impossible corrupted Core-owned state merely to enumerate identical overflow
  branches.
- Reject an objectively underfunded user source before invoking the engine.
- Differentially test the CPMM reference calculation against an independent
  integer model across boundary values and randomized reserves, inputs, and fees.

### Atomicity

- Force failure before the engine, inside the engine, inside a supplied custom
  program, after engine and custom state mutation, during receipt validation,
  and after the engine has returned but before settlement. Exercise a later
  token or compute failure when it can be constructed without weakening the
  fixed classic-SPL profile merely to manufacture a test hook.
- For every failure, compare byte-for-byte snapshots of every writable account
  in that fixture's possible mutation closure: engine state, supplied writable
  custom state, fixed writable Core state, protected token accounts, and fee
  liability. Read-only mints, programs, sysvars, and token accounts absent from
  the instruction are not included merely to inflate the snapshot count.
- Prove that a successful run changes only the exact fixed and explicitly
  supplied writable accounts predicted by the test.

### Bounds and resources

- Accept zero and eight opaque positions; reject nine before engine execution.
- Accept zero and 128 opaque payload bytes; reject 129 before engine execution.
- Cover external duplicates at the eight-position limit.
- Measure the fully serialized transaction, locked accounts, writable accounts,
  CPI account infos, call-tree depth, return-data length, and total compute for the
  maximum closure and payload.
- Exercise at least one nested external-program CPI and the reference CPMM with
  the maximum closure and payload under the active pinned runtime. Maximum-width
  integer arithmetic is covered by the independent host differential model
  unless it is reachable from valid initialized SBF state without test-only
  corruption.
- Fail the candidate if any measured fixture exceeds the pinned active limits.
  Record exact consumption and remaining headroom; do not extrapolate the result
  to arbitrary engines or future runtime features.

Fast invariant, exact-SBF, callback, and nested-CPI resource cases run under the
pinned LiteSVM runtime. A second embedded runtime is useful only if LiteSVM
cannot reproduce a relevant active-runtime behavior; it is not added for test
count alone. No devnet transaction is part of this gate.

## Required result record

A passing experiment produces one reviewable result document containing:

- the exact source commit and disposable artifact hashes;
- the canonical request, closure, and receipt byte fixtures;
- the complete passing and deliberately failing test inventory;
- observed packet, account, writable-lock, CPI, stack, return-data, and compute
  measurements at the experiment maximums;
- the demonstrated external custom-account damage boundary;
- the demonstrated fixed-plane non-reachability boundary;
- rejected alternatives and any runtime-specific assumptions; and
- a blunt recommendation to continue, revise, or reject the engine-generated
  callback shape.

A result that merely completes a successful swap is a failed experiment.

## Explicit non-goals

This spike does not design or establish:

- a public or immutable engine ABI;
- a production Core, SDK, IDL, client, router, indexer, or user interface;
- a safe generic arbitrary-call sandbox;
- CPI callers or a Core callback-authentication signer;
- stored, detached, delegated, multi-party, or asynchronous intents;
- partial fills, matching, positions, bidirectional or multi-leg settlement;
- shared-liquidity domain admission;
- loader-aware engine code identity or an engine upgrade policy;
- Token-2022 settlement, transfer hooks, NFTs, compressed assets, custom asset
  programs, or any general-purpose protected-value claim;
- an external settlement-driver authority boundary;
- provider shares, withdrawal, an engine-independent exit, fee claiming, or
  persistent custody;
- final protocol-fee implementation or collection topology; ADR 0005 later
  selected the V1 economics and immutable collector rule, but this experiment
  does not implement or prove them;
- canonical production event bytes or historical indexing guarantees;
- deployment, release, upgrade, migration, or immutability policy; or
- universal product limits for account count, payload size, compute, or return
  data.

## Blockers after this spike

Even a fully passing result does not authorize a public interface or deployment.
Before a general engine ABI is accepted, separate decisions and hostile tests
must close at least:

1. the comparison against the other callback shapes;
2. a safe CPI-caller authentication path;
3. authorization-neutral direct, stored, and multi-intent plan semantics;
4. loader-aware engine identity and upgrade behavior;
5. shared-domain descriptors and local admission;
6. the external settlement-driver boundary for custom protected value;
7. persistent accounting, provider claims, withdrawal, and exit classes;
8. protocol-fee claim, recipient, and bounded policy rules;
9. canonical event and compatibility bytes; and
10. reproducible release evidence and deployment-authority policy.

No persistent Core custody may be deployed or funded until its exact exit class
and claim path are implemented and proven independently of engine liveness.
