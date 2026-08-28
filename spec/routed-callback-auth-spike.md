# Routed callback authentication spike

Status: Locally implemented; canonical CI reproduction pending; private
non-production contract

This document authorizes one disposable experiment. It does not accept a public
engine interface, authorization format, Core account layout, deployment
artifact, product feature, or resource limit.

## Decision to falsify

The experiment tests one narrow hypothesis:

> A user can authorize one exact, bounded classic-SPL debit at transaction
> level; an untrusted caller that never receives the user signer can execute the
> same committed intent directly or through CPI; and the selected engine can
> authenticate a phase-scoped Core callback without receiving a capability that
> any protected Core path accepts.

The hypothesis is false if the experiment needs to:

- forward the user signer to a router or engine;
- trust inherited `is_signer` inside a routed Core CPI as exact user intent;
- allowlist a router, engine, market creator, or integration;
- give the engine a signer accepted for protected token movement, custody,
  protocol fees, intent administration, claims, or upgrades;
- use one PDA as both protected spend authority and callback authentication;
- bind authorization semantics to a particular router, invocation depth, or
  top-level execution shape;
- let a mutable engine transition survive when protected settlement fails;
- require a global writable registry, sequencer, or offchain signer; or
- exceed an active packet, account, compute, call-depth, instruction-count, or
  return-data limit at the experiment maximums.

Passing this spike permits a measured callback-shape decision. It does not make
the experiment bytes a product ABI.

## Isolation rules

The implementation authorized by this document lives only in the standalone
Cargo and Anchor workspace under `experiments/routed-callback-auth/`.

The workspace must:

- own its lockfile, toolchain declaration, build output, and four disposable
  program identities;
- remain outside the repository-root workspace and every predecessor
  experiment workspace;
- mark every package unpublished;
- use private names, discriminators, account layouts, seeds, hash domains, and
  codecs that make no compatibility promise;
- have no path dependency, symlink, shared include, or artifact dependency that
  crosses its workspace boundary;
- retain no deployment keypair, RPC secret, upgrade authority, cluster address,
  deploy command, maintained IDL, SDK, client release, or migration path; and
- run only in a local or in-process test runtime with valueless fixtures.

The root authority-kernel programs and the isolated engine-generated-settlement
workspace remain frozen evidence. This experiment may copy the minimum source
needed at the provenance baseline, but it does not edit or supersede that
source, its lockfiles, build scripts, result records, or hash manifests.

## Fixed experimental settlement control

The economic control remains the predecessor experiment's fixed exact-input
classic-SPL Token A-to-B settlement. Core validates the exact market, domain,
mints, source, recipient, vaults, fee ledger, fee vault, token program, engine,
engine state, user bounds, expiry, capability closure, payload, accounting, and
observed token deltas.

That fixed envelope exists only to compare authorization and callback behavior
against a known control. It is not a generic plan model and must not introduce a
Core product enum such as `swap`, `auction`, `order`, `position`, `NFT`, or
`game`.

## Exact wallet authorization and canonical top-level helper

Routed execution must not infer exact intent merely because some account still
appears as a signer after one or more CPIs. The canonical flow therefore has the
user call a separate top-level Core authorization before execution.

The security boundary is the resulting classic-SPL token-account state, not an
unprovable claim about which program produced those bytes. A token owner can set
the same exact delegate directly through the classic SPL Token program; that
state is indistinguishable from a delegate installed by Core and represents the
same owner-signed authority. Proving Core-instruction provenance would require
same-transaction instruction introspection or rent-bearing Core state, neither
of which this experiment needs. The Core helper is the canonical validation and
UX path, but execution deliberately accepts any equivalent exact owner-approved
delegate state.

The authorization instruction must:

1. run at transaction level and receive the actual classic-SPL token owner as a
   signer;
2. validate the exact source account, mint, current token owner, supported token
   profile, and absence of conflicting protected authority;
3. receive the complete fixed-width canonical intent binding, including expiry
   and callback timing mode, as an explicit authorization argument;
4. decode the binding, recompute its canonical intent digest, match the binding
   to the signer, source, mint, token program, and disposable Core, and derive
   the exact total debit from its input and protocol fee;
5. derive a dedicated spend-authority PDA from the user source and intent
   digest;
6. invoke the classic SPL Token program's checked delegate-approval path using
   the real token owner; and
7. approve exactly the request's total input debit, equal to the exact input
   amount plus the exact authenticated protocol fee.

The immutable intent commitment binds at least:

- experiment version and exact Core program;
- user authority and a user-selected nonce;
- market, participating domain, selected engine program, selected engine state,
  and engine revision;
- token program, exact mints, user source, recipient, domain vaults, fee ledger,
  and fee vault;
- exact input amount, minimum output credit, maximum protocol fee, and maximum
  total input debit;
- fee-policy revision and expiry slot;
- callback timing mode, selecting Candidate A or Candidate B before execution;
- ordered normalized capability-closure hash;
- opaque payload hash; and
- the exact total debit that the SPL delegate must expose.

The routed execution instruction does not receive the user account as a signer.
The hostile router receives no user signer and no capability to approve, revoke,
or expand the token delegation. Any payer or executor may submit a request once
the exact owner-approved delegate state exists.

There is no Core-owned intent account, intent registry, rent-bearing permit, or
global replay account. The classic-SPL source account is the one-shot state. At
execution Core recomputes the intent digest and requires its exact spend PDA as
the source delegate with `delegated_amount` equal to the exact total debit. Core
uses that delegate only for the pinned checked input and fee transfers. Success
must leave `delegate = None` and `delegated_amount = 0`; replay then fails without
another explicit user approval. Failure rolls the delegate and token balances
back to their pre-transaction state. The token owner may cancel before execution
through the ordinary classic-SPL revoke path; Core stores no cancellation state.

## Separate spend and callback capabilities

The spend authority and callback authentication must be different PDAs with
different domain-separated seeds and disjoint acceptance sites.

### Spend-authority PDA

The spend PDA exists only so Core can exercise the user's exact request-specific
classic-SPL delegate approval. It must:

- derive as `[b"spend:v0", user_source, intent_digest]` under the disposable
  Core program;
- be accepted only as authority for the exact authorized user source debit;
- never enter the engine or opaque capability closure;
- never authorize a domain vault, fee vault, claim, withdrawal,
  administration, configuration, or upgrade;
- require the landing-time delegated amount to equal, not merely cover, the
  committed total debit; and
- consume that amount exactly so no delegate allowance survives success.

The spend PDA may be a non-existent account or have unsolicited lamports donated
to its address. Core derives no semantic authority, balance, liveness, owner, or
state claim from its lamports or data. Its only meaning is the exact PDA signer
produced by Core for the pinned token CPIs.

### Callback-authentication PDA

The callback PDA exists only so the selected engine can authenticate the exact
Core, engine, market, intent, and callback phase currently running.
It must:

- derive under the disposable Core program with a callback-specific domain;
- derive as `[b"engine-callback:v0", engine_program, engine_state, market,
  domain, intent_digest, phase]` with the canonical bump;
- be passed read-only and signer only for the matching Core-to-engine CPI;
- derive no meaning or authority from account lamports, owner, or data and
  tolerate unsolicited lamport donations; and
- never be accepted by any Core instruction or protected asset profile as a
  spend, fee, vault, domain, intent, administration, or upgrade authority.

Signer privilege can be forwarded by a callee. A hostile engine is therefore
allowed to pass the callback signer to the separate callback-capability probe.
The probe may mutate only explicitly supplied opaque state whose own program
chooses to trust that signer. That is expected opaque-plane damage. It must not
reach the spend PDA, user-source delegation, Core custody, protocol fees, or any
non-participating domain.

Runtime rejection of indirect reentrancy is defense in depth, not the semantic
authorization rule. No protected handler may become safe only because a
particular stack shape currently makes reentry fail.

The callback signer is a fixed derived CPI-prefix account, not an opaque
capability and not an input to the capability-closure hash. Both phase addresses
are computable before transaction construction from the immutable intent digest
and fixed market, domain, and engine identities. The engine independently
recomputes the address from those inputs and the Core program ID. Dynamic
landing-time or receipt evidence belongs in the callback instruction digest,
never in the PDA address.

## Direct and routed execution

The same request-specific delegate authorization and same semantic Core
execution bytes must be usable in both control paths:

```text
direct executor -> Core -> engine
```

```text
executor -> hostile router -> Core -> engine
```

The caller or router is not part of the intent, execution, receipt, or settlement
meaning. Core must not require a trusted router ID or a particular invocation
depth. The direct and routed paths must derive the same digests, apply the same
validation, consume the same exact delegate allowance, assess the same fee,
settle the same token deltas, and produce the same objective Core result.

The router may forward the exact Core bytes and ordered accounts. Adversarial
transactions deliberately mutate bytes, reorder or substitute accounts, omit
or add accounts, request excess privilege, and invoke the wrong Core
discriminator through that permissionless router. The router fixture separately
tests double execution, direct spend attempts, and callback reuse. None of those
paths may alter the user's committed terms or make inherited caller privileges
authoritative.

## Hash phases and non-circular derivation

The experiment must use explicit domain-separated stages instead of one hash
whose inputs are unavailable when its PDA must be derived.

At minimum:

1. `intent_digest` commits every immutable user-authorized request term,
   including nonce, exact debit, expiry, capability and payload commitments, and
   callback timing mode;
2. the phase-specific `execution_digest` commits `intent_digest`, the selected
   phase, landing-time authenticated Core accounting, engine and fee revisions,
   engine pre-sequence, normalized closure, payload, and the exact callback
   request bytes;
3. `receipt_digest` commits the matching execution digest, selected engine's
   exact returned result, and authenticated engine sequence or state evidence;
   and
4. `settlement_digest` commits the accepted execution and receipt evidence,
   exact protected effects, protocol fee, and expected or observed post-state.

The intent-bound timing mode determines the legal callback phase sequence.
Candidate A uses one `TRANSITION` phase. Candidate B uses one read-only `PREPARE`
phase followed by one writable `COMMIT` phase. Substituting the timing mode or
phase changes authentication and fails closed.

Candidate B's final commit instruction binds the settlement digest and its exact
commit effect. That dynamic evidence authenticates the commit bytes but does not
derive the already supplied COMMIT callback PDA. Every callback account required
by either mode is therefore computable before the outer transaction is built.
No digest preimage may contain a PDA address that is itself derived from that
same digest. Spend and callback addresses are derived only from the immutable,
precomputable intent digest and their explicit capability or phase domains. The
execution, receipt, and settlement digests are not PDA seeds. In particular,
neither the normalized opaque closure nor an execution digest may absorb a
callback account descriptor and then attempt to derive that callback account
from the result.

No intent, execution, receipt, callback, or settlement digest binds the current
stack height, top-level instruction index, router identity, or whether the
execution happened directly or through CPI. Stack depth remains a measured
resource property, never authorization meaning.

Unknown versions, phases, hash domains, non-canonical padding, extra bytes, and
ambiguous encodings fail closed.

## Callback candidates

The experiment compares exactly two candidates. Passing both does not select
either as a public interface.

### Candidate A: one writable engine transition before settlement

```text
Core validates the exact delegate and derives the TRANSITION execution_digest
  -> Core invokes the engine once with the matching callback signer
  -> engine may update its declared writable state and returns an exact receipt
  -> Core authenticates the receipt immediately
  -> Core performs protected settlement and fee transfers
  -> Core verifies exact deltas, zero remaining delegation, and accounting
```

Any later Core, token, fee, compute, or validation failure must roll back the
earlier engine transition atomically. There is no post-settlement engine call.

### Candidate B: fully read-only prepare and final writable commit

```text
Core validates the exact delegate and derives the PREPARE execution_digest
  -> Core invokes a fully read-only engine prepare with its phase signer
  -> Core authenticates the exact prepare receipt
  -> Core performs protected settlement and verifies exact deltas
  -> Core derives settlement_digest from the verified result and post-state
  -> Core invokes one final writable engine commit with the COMMIT phase signer
  -> Core requires zero remaining delegation and records the verified Core result
```

The prepare phase may not receive any writable engine or opaque account. The
writable commit is the final account-bearing untrusted CPI. No protected
guarantee may depend on mutable engine or opaque state inspected only after that
commit. Commit failure must roll back the entire transaction, including every
protected transfer and Core accounting change.

The candidates are compared on containment, cross-phase finality, signer
surface, account metas, packet bytes, compute, stack depth, developer ergonomics,
and failure behavior. The simpler candidate wins only if its executable evidence
establishes the same required safety properties.

## Required adversarial evidence

Every rejection must occur before unauthorized protected movement, and every
failure after any mutation must roll the entire transaction back. The evidence
is deliberately layered: deterministic codec tests prove field commitment and
canonical encoding, Core and fixture unit tests prove local validation rules,
and exact-SBF integration tests prove the runtime CPI, signer, return-data,
rollback, transaction, and resource behavior. A claim does not require a
duplicative SBF case when a lower-level test establishes only pure codec or
arithmetic behavior, but every cross-program security claim requires exact-SBF
evidence.

The combined evidence must cover at least:

- direct and routed success from identical starting ledgers with identical
  semantic results, while the router receives no user signer;
- top-level-only Core authorization, equivalent direct owner approval, wrong
  owner, revoke, missing, inexact, reused, expired, mutated, and consumed
  delegate states;
- byte, discriminator, account order, omission, addition, privilege, fixed-role,
  timing, limit, nonce, capability, payload, and expiry substitution through or
  against the permissionless execution path;
- forged or substituted spend and callback authorities, wrong callback phase,
  wrong entrypoint, wrong intent or execution evidence, and callback reuse;
- callback signer forwarding into the hostile downstream probe while protected
  fixed roles and protected token accounts remain outside the opaque closure;
- duplicate and reordered capabilities, effective privilege normalization,
  signer and fixed-role aliases, executable accounts, arbitrary writable state,
  and protected classic-Token and Token-2022 owners;
- missing, wrong-setter, malformed, unknown-version, wrong-phase,
  wrongly-bound, out-of-bounds, and trailing-byte engine receipts;
- rollback after Candidate A engine/downstream mutation and after Candidate B
  settlement plus a late commit failure; and
- exact private wire lengths, hash-domain separation, checked arithmetic, and
  rejection of unknown or non-canonical encodings.

This fixed classic-SPL control does not claim to exercise every failure stage of
future token profiles or a future generic settlement interpreter. Those remain
explicitly outside this gate rather than being implied by a broad test label.

At least one successful maximum path must execute:

```text
hostile router -> Core -> engine -> callback-capability probe
```

It must use the experiment's maximum ordered opaque closure and maximum payload,
then complete the real classic-SPL settlement. Candidate A and Candidate B must
be measured separately if both can reach that maximum.

## Resource acceptance criteria

The tests must record, from real SBF execution:

- serialized transaction size and packet headroom;
- total and writable account counts;
- compute units for direct and routed candidates;
- maximum CPI depth and executed frame count;
- callback instruction and return-data sizes; and
- the delta introduced by top-level authorization, the router, and the second
  phase, plus delegate and callback overhead where a controlled comparison can
  isolate it.

The gate must fit the active conservative baseline used by this repository:

- 1,232-byte legacy/v0 transaction packet;
- 64 locked accounts;
- current five-level instruction-stack limit;
- 64 executed instructions;
- 1,024 bytes of return data; and
- 1,400,000 compute units when explicitly requested.

The experiment may use lower local ceilings to preserve headroom. It must not
rely on transaction v1, a future deeper CPI stack, a future larger account
limit, or any inactive runtime feature. The predecessor's eight opaque accounts
and 128-byte payload are test maximums, not product limits; the new measurements
must determine whether they remain viable even for this private probe.

## Acceptance and rejection

The gate passes only if:

- exact owner-signed, intent-bound SPL delegation is necessary, and the
  canonical Core authorization helper cannot be invoked through CPI;
- the router receives no user signer and needs no allowlist;
- direct and routed execution preserve identical semantic authorization;
- spend and callback capabilities are cryptographically and semantically
  disjoint;
- a forwarded callback signer damages at most explicitly supplied opaque state;
- every implemented intent, fixed-role, replay, phase, alias, receipt, and
  rollback substitution in the evidence matrix fails closed;
- both callback candidates have honest executable safety and resource evidence;
- one candidate is selected or both are rejected with explicit reasons; and
- no private byte, program ID, seed, bound, or fixture name is presented as a
  public compatibility promise.

The gate fails if safe execution requires a trusted router, user signer
forwarding, one dual-purpose Core signer, a global mutable registry, an offchain
authorization service, a product-type enum, unverifiable opaque economics, or
future Solana limits.

## Not established

Even a passing result does not establish:

- a public engine ABI, intent ABI, SDK, IDL, compatibility version, or product
  resource limit;
- generic multi-leg, partial-fill, stored-order, auction, multi-party, NFT,
  Token-2022, compressed-asset, or custom-authority settlement;
- loader-aware engine code identity or safe engine upgrades;
- provider claims, withdrawals, fee claims, shared-liquidity admission, or an
  engine-independent exit;
- profitable or honest engine economics;
- deployment, migration, governance, immutability, release signing, or onchain
  artifact verification;
- devnet or mainnet behavior; or
- immunity from implementation defects.

No program in this experiment may be deployed or entrusted with real funds.
