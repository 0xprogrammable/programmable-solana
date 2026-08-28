# Generic effect capabilities spike

Status: Proposed private experiment; no implementation and no public interface

Date: 2026-08-28

This document authorizes one disposable architecture experiment after the routed
callback authentication result. It does not accept a public engine ABI, Core
account layout, SDK, product resource limit, deployment artifact, custody
design, or fee schedule.

## Decision to falsify

The experiment tests one narrow hypothesis:

> One permissionless engine can perform arbitrary engine-owned and opaque state
> transitions in a single authenticated pre-settlement callback, return a
> canonical product-neutral graph of protected-value moves indexed into a
> Core-validated settlement-capability table, and let Core execute those moves,
> mandatory fees, and domain-local accounting atomically without giving the
> engine any user, custody, fee, claim, or other protected authority.

The hypothesis includes routed execution. A permissionless router may invoke
Core without receiving a user signer. The engine callback contains only one
fixed read-only callback signer followed by an ordered opaque account tail. The
public design must not freeze one `engine_state` account: zero, one, or many
engine-owned state accounts are positions in that opaque tail and are bound by
their actual landing-time capabilities.

The hypothesis is false if a representative market requires any of the
following:

- a Core product or action enum;
- a fixed engine-state account in the engine interface;
- a user, vault, fee, claim, admission, or upgrade signer in the engine CPI;
- a protected writable asset account in the engine CPI;
- an arbitrary settlement-driver call carrying Core authority;
- a market declaration that grants itself access to an existing domain;
- an engine- or caller-selected protocol-fee policy or assessable label;
- authorization-specific fields in the engine effect result;
- unverifiable netting across protected accounts, assets, profiles, or domains;
- partial, zero-debit, or multi-intent execution that can replay without
  explicit Core state;
- future transaction-v1, account-lock, or CPI-stack capacity; or
- a transaction that exceeds the predeclared experimental resource headroom.

Passing this spike permits a decision about the next private architecture gate.
It does not name the candidate wire `v1`, make it stable, or authorize real
funds.

## Product-neutral boundary

The Core may understand protected authority facts. It must not understand why
an engine requested an effect.

The following belong to engines and their optional schemas:

- curves, price functions, bonding schedules, and spreads;
- auctions, matching, orders, inventory selection, and allocation;
- provider economics, rewards, points, referrals, and creator economics;
- game state, eligibility, oracle interpretation, and time phases;
- engine-owned positions, claims, receipts, and lifecycle state; and
- unnamed future market semantics.

The first protected effect under test is an exact classic SPL Token move. It is
an authority primitive, not a declaration that an action is a swap, deposit,
withdrawal, sale, royalty, or claim. Mint, burn, compressed-asset mutation,
Token-2022 behavior, Core-native positions, external custody, and custom
settlement drivers remain separate authority-profile decisions.

Arbitrary semantics and arbitrary protected authority are deliberately not the
same promise. An engine that needs authority outside an accepted protected
profile may still own or compose that authority in the opaque engine plane, but
the resulting effect is `EngineAttested`, not `CoreVerified`.

## Isolation rules

Any later implementation authorized by this document must live in a new
isolated nested workspace under `experiments/generic-effect-capabilities/`.

That workspace must:

- have its own workspace manifest and lockfile;
- remain outside root workspace membership and the default build;
- mark every crate `publish = false`;
- use disposable program IDs, discriminators, seeds, account layouts, codecs,
  limits, and fixture names;
- import no candidate wire through the canonical Core or public engine-interface
  crates;
- expose no maintained IDL, SDK, code generator, release tag, or compatibility
  promise;
- contain no deployment keypair, cluster address, deploy command, upgrade
  authority, or real-fund configuration; and
- execute only in an in-process or local test runtime. A forked local runtime is
  evidence about runtime behavior, not a deployment.

This document itself adds no implementation. Any implementation is a later
reviewable change and may be deleted after its result is recorded.

## Terminology

### Protected plane

The protected plane contains user funding accounts, Core-custodied domain
assets, exact credit recipients, protocol-fee accounts, domain admission state,
authorization state, Core accounting, and every signer or writable account that
can create a `CoreVerified` value effect or right.

### Opaque engine plane

The opaque engine plane contains the selected engine program, a rightless
callback-authentication signer, engine-owned state, and external accounts or
programs explicitly supplied to the engine. Core authenticates this plane's
actual capability closure but does not certify its economics, data, ownership
model, solvency, or side effects.

### Engine instance

A market binds an opaque 32-byte `engine_instance_id`. It is an identity input,
not an account address and not proof of code identity. The engine decides which
zero, one, or many opaque-tail accounts represent that instance. Core never
requires a fixed `engine_state` account in the callback prefix.

### Settlement capability

A settlement capability is a Core-internal indexed right to request one bounded
protected effect. It is not a PDA bearer token, signer, delegate, or account
forwarded to the engine.

### Opaque capability

An opaque capability is an actual ordered `AccountInfo` position passed to the
engine CPI after landing-time owner, executable state, and effective privileges
have been validated and committed.

### Canonical private hashing

The experiment uses SHA-256 for every candidate digest. To remove concatenation
ambiguity, the private helper is exactly:

```text
H(label, parts...) = SHA256(
  "programmable/private-effect-capabilities/2026-08-28" ||
  u16_le(byte_length(label)) || label ||
  u16_le(part_count) ||
  for each part in order: u32_le(byte_length(part)) || part
)
```

Labels are lower-case ASCII and are distinct for settlement capabilities,
opaque capabilities, intents, domains, assets, market binding, code identity,
admission, callback seed, engine request, canonical effects, fee assessment,
and evidence. A list digest is `H(label, u32_le(count), row_0, ..., row_n)`;
there is no implicit sorting except where this document explicitly requires it.
The complete encoded row, including its original numeric position and zero
reserved bytes, is the list element. Empty lists therefore have unambiguous
roots.

All roots are recomputed by Core from the landed instruction and current
accounts. A client-supplied root is an equality assertion, never an authority
source. These choices are disposable private test vectors, not a public hashing
contract.

## Two capability tables

The experiment has two disjoint tables. Combining them, forwarding one as the
other, or using one root for both is a falsification.

### Settlement-capability table

Core derives an ordered table `S = [S_0, ..., S_n]` from the actual protected
accounts, market and domain state, authorization witnesses, and accepted
settlement profile. The engine receives the table root and selected numeric
context, never these `AccountInfo`s.

Each private descriptor commits to at least:

```text
position
core program and experimental major
market
settlement profile
asset identity and asset program
exact endpoint account and landing-time owner
domain descriptor and revision, or an explicit no-domain marker
domain admission proof, when a domain is present
authority class and debit/credit/reserved-fee rights
authorization slot or domain-accounting slot
maximum debit and minimum credit relevant to that slot
fee class and policy revision
fee-shard index or explicit no-shard marker
lifecycle and profile facts required by the exact asset profile
```

The domain-separated settlement-capability root includes the table length and
every original position. Protected public keys are unique in this table. One
source account used for both a market debit and the Core-derived fee is one
capability with multiple outgoing moves, not two aliased semantic roles.

The first experiment may use private authority classes equivalent to:

- intent-funded debit;
- domain-accounted debit or credit;
- exact external credit recipient; and
- Core-reserved fee credit.

These are authority facts for the classic-SPL experiment, not public product
types. Unknown classes fail closed. A later authority primitive requires a
separate decision even if an engine can describe it in opaque bytes.

Every settlement capability is validated before the engine callback. At least:

- its public key, owner, executable state, expected PDA or authorization
  relation, mint, asset program, and lifecycle are exact;
- its actual outer signer and writable privileges are no greater than the fixed
  Core route permits;
- duplicate public keys do not create two protected roles;
- a domain capability names one exact admitted domain and accounting slot;
- a user debit capability has one valid direct or stored authorization witness;
- an exact credit capability binds its recipient before the engine runs; and
- a fee capability is reserved to Core and cannot be referenced in the engine
  result.

### Ordered opaque engine-capability table

The engine CPI account metas are exactly:

```text
0. callback authority: read-only signer, non-executable
1..N. ordered opaque capability tail
```

The selected engine program is the CPI target. It is not a fixed engine-state
account. The opaque tail may contain zero, one, or many engine-owned state
accounts, arbitrary read-only programs, and explicitly risk-accepted external
state.

For every opaque-tail position Core records:

```text
position || key || landing-time owner || executable
         || effective signer || effective writable
```

Order and multiplicity are preserved. Security validation is by public key:
every occurrence of one key receives the union of signer and writable privilege
visible anywhere in the complete landing-time instruction. Static and
address-lookup-table resolution must not create distinct security identities for
the same key.

Before the engine CPI, Core rejects an opaque-tail key that:

- has effective signer privilege;
- aliases any fixed Core control account or any settlement capability;
- is owned by the experimental Core;
- is executable and effectively writable;
- is effectively writable and owned by classic SPL Token or Token-2022;
- aliases an authorization, domain-admission, fee, loader-policy, callback, or
  other protected account; or
- exceeds the private account or payload limits.

The engine may combine the surviving capabilities, forward them, invoke supplied
programs, and introduce signers for its own PDAs. Those are real opaque-plane
rights. Core does not treat a declared nested CPI script as a sandbox.

### Cross-table disjointness

Disjointness is checked after privilege union and before either hash is accepted.
It includes ancillary accounts introduced by the selected protected profile.
A future Token-2022 transfer-hook extra, for example, may not silently alias an
engine state, callback authority, domain account, fee account, or another
protected role.

The callback signer is not part of either economic capability table. It is a
fixed authentication-plane account bound separately to the exact callback.

## Market and engine binding

The candidate market binding commits to:

```text
Core experimental program and major
market identity
engine program
engine interface ID
engine instance ID
engine admission-policy digest
domain-admission profile
settlement-capability profile
fee policy revision
opaque payload or schema digest
```

`engine_instance_id` replaces a fixed `engine_state` address. Engine-owned
account addresses and privileges are instead committed by the opaque-capability
root for the exact execution.

Permissionless means any developer can deploy an executable engine and create a
new compatible market and domain under deterministic public rules. It does not
mean an engine may select a weaker protected profile, forge an existing domain's
admission, or receive undeclared authority.

The experimental Core configuration may identify supported interface majors,
loader parsers, settlement profiles, and fee-policy roots. It may not enumerate
approved engine program IDs, require a platform co-signer for market or domain
creation, or maintain a writable global execution registry. Domain-local
admission is the only engine allow/deny decision in an ordinary settlement.

## Loader-aware engine policy gate

A numeric engine revision is experiment metadata, not code identity. Before the
callback, Core validates the selected program against the exact loader-aware
policy bound by the market and every participating domain.

The experiment separates a long-lived admission policy from the exact code
snapshot observed for one execution. Collapsing them into one hash would either
make a mutable-controller policy accidentally pinned or let a pinned intent run
against different code.

The three private admission policies are:

1. **Immutable deployment** — the supported loader relation is exact and admits
   no future mutation route. For upgradeable-loader v3 this requires an exact
   Program/ProgramData relation and no upgrade authority. The admitted
   deployment slot is pinned.
2. **Pinned mutable deployment** — program ID, loader, ProgramData address,
   deployment slot, and exact controller are pinned. A later deployment or
   controller change invalidates the admission until a new domain-local
   revision is created.
3. **Explicit mutable-controller risk** — program ID, loader, ProgramData
   address, and one exact visible controller are admitted, while future
   deployment slots under that controller are deliberately accepted. This is
   never described as pinned or immutable.

Core derives both facts from parsed onchain loader state:

```text
EngineAdmissionPolicyCandidateV0 {
  policy_kind: u8
  reserved: [u8; 7]
  engine_program: [u8; 32]
  loader_program: [u8; 32]
  program_data_or_zero: [u8; 32]
  expected_controller_or_zero: [u8; 32]
  pinned_deployment_slot_or_zero: u64
}

EngineCodeSnapshotCandidateV0 {
  engine_program: [u8; 32]
  loader_program: [u8; 32]
  program_data_or_zero: [u8; 32]
  observed_deployment_slot_or_zero: u64
  observed_controller_or_zero: [u8; 32]
}

engine_admission_policy_digest = H(
  "engine-admission-policy-v0",
  encoded admission policy
)

engine_code_snapshot_digest = H(
  "engine-code-snapshot-v0",
  encoded execution snapshot
)
```

The admission policy is exactly 144 bytes and the execution snapshot is exactly
136 bytes. Private policy kinds `0`, `1`, and `2` mean immutable, pinned mutable,
and mutable-controller-risk. For kind `2`,
`pinned_deployment_slot_or_zero` must be zero, so its admission digest does not
change on an accepted controller deployment. Kinds `0` and `1` bind an exact
nonzero deployment slot for upgradeable-loader v3. Unknown kinds, nonzero
reserved bytes, impossible zero/nonzero combinations, and unsupported loaders
fail closed.

The market and every participating domain bind
`engine_admission_policy_digest`. The top-level envelope, every direct or stored
intent, and the engine request bind the exact
`engine_code_snapshot_digest` observed for that execution. Thus a domain may
accept a controller's future code without silently extending an already signed
user intent to that code.

For upgradeable-loader v3, the engine Program and ProgramData accounts must both
be effectively read-only after full privilege union, their relation and
controller must parse exactly, and `Clock.slot` must be strictly greater than
`ProgramData.deployment_slot`. A same-slot deployment/execute attempt fails
before the engine callback. Unknown loaders, missing ProgramData, changed pinned
deployment identity, unexpected controller removal or addition, and
policy/market/domain disagreement also fail before callback. Loader and
ProgramData control accounts are never opaque capabilities.

The immutable policy requires loader semantics that actually prevent later
mutation. Merely placing a zero controller in a candidate field is not proof if
the loader admits another mutation route.

The experiment must measure the account and compute cost of these checks. It does
not assume that hashing an entire deployed ELF on every settlement is viable.
Source, artifact, ELF, deployment, and onchain program-data identity remain a
release-evidence problem in addition to the executable onchain policy gate.

## Domain-local admission proof

A domain identity includes at least:

```text
controller program and controller identity
domain descriptor revision
namespace or instance identity
custody profile
asset profile
accounting profile
exit class
admission rule
protected-capability profile
```

Every domain debit or accounted credit in the settlement table requires a proof
owned or otherwise authorized by that domain's own admission rule. The proof
binds the exact relation among:

```text
domain descriptor and revision
market
engine program
engine interface ID
engine admission-policy digest
settlement-capability profile
```

A market, engine receipt, caller, plan, manifest, or global registry cannot
self-declare that relation. A domain may deliberately choose an open deterministic
rule. That remains a choice by the domain's liquidity providers, not approval by
Programmable.

The closed-rule fixture uses one Core-owned, domain-authorized private record:

```text
DomainAdmissionCandidateV0 {
  wire_version: u8
  reserved: [u8; 7]
  domain_descriptor: [u8; 32]
  domain_revision: u64
  market: [u8; 32]
  engine_program: [u8; 32]
  engine_interface_id: [u8; 32]
  engine_instance_policy_digest: [u8; 32]
  engine_admission_policy_digest: [u8; 32]
  settlement_profile_digest: [u8; 32]
  admission_rule_digest: [u8; 32]
  active_from_slot: u64
  expires_at_slot_or_zero: u64
  revoked_at_slot_or_zero: u64
}
```

The private record payload is exactly 296 bytes. Reserved bytes must be zero.

Its candidate address is:

```text
PDA(
  experimental Core program,
  "domain-admission-v0",
  H(
    "domain-admission-address-v0",
    domain_descriptor,
    u64_le(domain_revision),
    market,
    engine_program,
    engine_interface_id,
    engine_instance_policy_digest,
    engine_admission_policy_digest,
    settlement_profile_digest,
    admission_rule_digest
  )
)
```

The one digest seed keeps every PDA seed at most 32 bytes. Creation or revision
requires the exact authority described by the domain descriptor; this may be a
direct controller signer or a controller-program CPI, but never the market,
engine, router, callback signer, or a platform-wide signer. Ordinary settlement
parses the domain descriptor again, derives the relation and address, verifies
the record fields and active interval, and includes the complete record digest
in the settlement capability root and `domain_set_digest`.

An open-rule fixture contains its deterministic predicate and policy revision in
the domain descriptor. Core evaluates that predicate from the same engine,
market, code, and profile facts. "Open" therefore means the domain chose an open
predicate; it does not mean that the market can omit the proof or choose the
predicate at execution time.

Multiple admitted markets may share one domain. They then deliberately share
reserves, locks, engine risk, economics, and liveness risk. A non-participating
domain cannot be debited, closed, redirected, or have its accounted rights
changed. An unsolicited raw credit remains possible but creates no accounted
liquidity, fee liability, or claim.

Admission proofs are domain-local and read-only on ordinary execution unless the
selected admission profile explicitly requires a local mutable replay or quota
fact. No global writable registry participates in settlement.

## Authorization-neutral internal model

The engine result and settlement algebra must not encode how user authority was
obtained. Core normalizes every valid witness into a private in-memory view:

```text
AuthorizationView {
  authorization_id
  intent_digest
  actor
  settlement_capability_set_digest
  remaining_debit by capability
  minimum_credit by capability
  fee ceilings by bucket
  expiry
  fill sequence
}
```

The field names are explanatory, not an accepted account layout. The engine sees
only an ordered `intent_set_digest` and the context required for its economics.

### Direct authorization

A direct actor signer authorizes exact capabilities and bounds in the current
Core invocation. The signer and writable user asset accounts remain in the
protected Core plane and are never forwarded to the engine. Core constructs an
ephemeral `AuthorizationView`; no engine byte indicates that it was direct.

The experiment may also retain the predecessor's exact one-shot classic-SPL
delegate as a routed direct-intent fixture. For every delegated funding account,
that witness is valid only when:

```text
aggregate observed engine-source debit
+ aggregate Core-derived fee debit
== delegated_amount_before

delegated_amount_after == 0
```

A declared maximum, a leftover delegate, or consumption spread over an unknown
later fill is not one-shot replay protection. Any variable-amount, partial-fill,
zero-debit, or reusable routed authorization must use explicit
`StoredAuthorizationCandidateV0` replay state even if the token delegate could
technically move more funds.

### Stored authorization

A user-created Core authorization account binds the canonical intent and tracks
at least remaining debit, fill sequence, cumulative fee basis per bucket,
expiry, cancellation, and terminal consumption. Its exact account layout is
private to the experiment.

Core reserves or updates the current fill before the untrusted callback. Every
later failure rolls that update back with the engine and settlement writes. A
successful fill increments the sequence and cannot exceed any remaining debit or
fee ceiling. Cancellation, exhausted authorization, wrong sequence, and terminal
replay fail before the engine runs.

A token delegate may still provide the asset-program authority needed to execute
a stored classic-SPL debit. Delegate state alone is not replay state: leftover
delegation after a partial fill must not authorize more than the stored remaining
amount.

### Multi-intent authorization

One settlement may combine several direct or stored intents. Core sorts their
individual canonical digests, rejects duplicates, binds their order and count,
and derives:

```text
intent_set_digest = H(
  "intent-set-v0",
  domain_set_digest,
  u32_le(intent_count),
  ordered canonical intent digests
)
```

Every debit and required credit is attributed to one exact authorization slot.
Fees are attributed to the source capability and user that fund them. One user's
surplus or ceiling cannot satisfy another user's deficit or fee.

The first experiment rejects the same protected public key appearing in multiple
authorization slots in one envelope. Supporting intentional aggregation over one
funding account is a later alias and attribution decision.

### Authorization-neutrality test

The same semantic fixture is executed from cloned state through:

- one direct authorization;
- one stored authorization;
- one partially filled stored authorization; and
- a multi-intent set.

After normalizing for intended participant count and fill amount, all paths use
the same engine request type, callback prefix, move rows, Core move validator,
protected asset profile, and event evidence classes. If the effect format gains
`direct`, `order`, `partial`, `auction`, or other authorization/product fields,
the candidate boundary is rejected.

## Candidate protected effect algebra

The engine returns an ordered list:

```text
MoveCandidateV0 {
  source_capability_index: u8
  destination_capability_index: u8
  amount: u64 little-endian
}
```

Each row is exactly 10 bytes. Core applies the following normal form before any
protected movement:

1. `amount` is nonzero.
2. Both indices are in range and distinct.
3. Source and destination select the same exact asset identity, asset program,
   and settlement profile.
4. Source has debit rights; destination has credit rights.
5. The engine cannot reference a Core-reserved fee destination.
6. Rows are strictly increasing by `(source_index, destination_index)`.
7. Duplicate pairs are rejected rather than silently aggregated.
8. One capability may appear on only one side of the engine graph: source or
   destination, never both.
9. Checked `u128` aggregation derives gross debit and gross credit for every
   capability and fee bucket.
10. For every `(asset identity, asset program, settlement profile)`, aggregate
    debit equals aggregate credit.
11. Every user maximum debit and minimum credit holds after the Core-derived fee
    is added.
12. Every domain debit is covered by its own accounted balance and admission;
    every domain's accounted change is derived only from that domain's local
    capability deltas.

The source-or-destination normal form removes cycles and makes the observed
classic-SPL gross debit equal to the canonical plan debit. An engine can express
the same final fungible allocation by netting an intermediate account before it
returns the graph. If the gross intermediate path itself affects transfer hooks,
fees, rights, or product meaning, this first protected profile does not support
that behavior.

For every domain asset slot, Core derives:

```text
accounted_after = accounted_before + local_credits - local_debits
```

with checked arithmetic. Global conservation does not authorize a local debit.
Raw balances must cover accounted balances before and after settlement. Raw
donations do not change accounting.

Core executes each accepted classic-SPL move through one exact
`TransferChecked` call under its source capability's validated authority. Core
reloads all affected accounts and verifies exact aggregate source debits and
destination credits. No engine receipt can substitute for those observations.

## Protocol-fee algebra

The engine does not return protocol-fee moves. Core derives the mandatory
assessment once from the authenticated Core fee policy and the canonical pre-fee
engine graph, then appends reserved fee moves before checking final user bounds.

The experiment supports only fee facts that Core can observe objectively:

1. a fixed amount per successfully committed Core envelope; and
2. a rate over exact protected gross-debit buckets.

A rate bucket is keyed by at least:

```text
funding capability
authorizing actor
asset identity and settlement profile
fee class
fee policy revision
```

The basis excludes every protocol-fee move. No engine verb, product label,
caller flag, receipt claim, spread, reserve growth, auction surplus, or opaque
state change can make a leg assessable or exempt.

Within one envelope Core aggregates the complete bucket before rounding:

```text
basis = sum(canonical pre-fee gross debits in the bucket)
fee = R_policy(basis * rate / denominator)
```

For the private experiment, `basis` and cumulative basis are checked `u128`,
`rate` and the nonzero `denominator` are unsigned 64-bit policy facts, and the
multiplication/division model is exact unsigned 256-bit arithmetic before a
checked downcast. `R_policy` is exactly either floor or ceiling as committed by
the authenticated policy revision:

```text
floor(n / d) = quotient
ceil(n / d)  = quotient + (remainder != 0)
```

Unknown rounding modes, a zero denominator, overflow, a fee that does not fit
the protected amount type, or a cumulative basis decrease fail closed. The
fixture matrix exercises both accepted rounding modes; a product decision may
later narrow this to one.

Per-leg rounding is forbidden. Per-leg floor permits dust splitting; per-leg
ceiling charges different fees for equivalent split graphs. Net-basis charging
is also forbidden for this profile.

For a stored or partially filled intent, the authorization account stores the
cumulative basis for each bucket. The incremental assessment is:

```text
fee_delta = R_policy((basis_before + fill_basis) * rate / denominator)
          - R_policy(basis_before * rate / denominator)
```

Both terms use checked wide arithmetic and the same exact rounding function.
This makes equivalent fill partitions produce the same cumulative rate fee.
A fixed per-envelope fee remains separate and is charged once per successful
envelope.

Every fee assessment has a unique private identity derived from Core major,
market, policy revision, intent set, fill sequence, and bucket. A second
assessment with the same identity fails. Each fee move uses an exact authorized
funding capability, policy-derived recipient shard, and user ceiling.

The first rate fixture is payable in its basis asset. A fixed fee may use one
exact separately authorized asset. Cross-asset notional conversion, oracle
pricing, and a universal percentage of an engine-defined trade are non-goals.

Every accepted private fee shard has all three protected components:

```text
FeeShardDescriptorCandidateV0   // authenticated read-only control account
FeeLiabilityLedgerCandidateV0   // Core-owned writable control account
exact fee-vault capability      // writable settlement endpoint
```

The descriptor binds fee policy revision, shard index, asset and settlement
profile, vault, liability ledger, and recipient policy. The FeeShard wire row
binds the descriptor and ledger offsets to the exact reserved fee-vault
capability. None is supplied to the engine. Missing, aliased, reordered,
read-only liability, wrong-vault, wrong-asset, or wrong-policy components fail
before callback.

After fee transfers and exact balance reload, Core applies only:

```text
liability_after = liability_before + observed_net_fee_vault_credit
```

with checked wide arithmetic. Only that exact Core-created, observed net credit
creates liability. A pre-existing balance or donation does not. Fee claims,
recipient withdrawals, and liability reduction are separate from this
experiment. A later transfer-fee profile must distinguish gross source debit,
issuer-withheld amount, net destination credit, and accounted revenue.

If one protected capability profile can emulate another profile's effects, its
mandatory fee floor cannot be selected merely because the engine gives the
action a cheaper name. Fee classes are keyed only to objective authority and
effect facts.

The strongest universal revenue claim remains one mandatory fee per successfully
committed Core envelope. An engine may batch semantic actions or expose a route
outside Core; this experiment does not claim a fee on unknowable off-Core
behavior.

## Callback and phase binding

The experiment uses only the selected single writable transition before
settlement. There is no post-settlement engine callback.

The callback PDA is derived from a domain-separated digest equivalent to:

```text
callback_seed = H(
  "callback-seed-v0",
  Core experimental major,
  selected engine program,
  engine interface ID,
  engine instance ID,
  engine code snapshot digest,
  market,
  intent set,
  participating domain set,
  settlement-capability root,
  opaque-capability root,
  payload digest,
  TRANSITION phase
)
```

The digest is reduced to one fixed 32-byte seed so a variable domain or account
set does not become a variable PDA seed list. The callback address itself is not
inside the digest, avoiding a hash/address cycle.

The callback account is read-only and non-executable in the Core instruction and
is signed only inside the exact Core-to-selected-engine CPI. The engine may
forward it to opaque programs. No Core instruction, protected profile, admission
rule, fee path, claim path, or administrative path accepts it as authority.

Core authenticates engine return data immediately after the callback. The
selected engine must set its receipt after its final nested CPI. A missing value,
wrong setter, descendant setter, malformed bytes, unsupported version, wrong
phase, wrong request binding, or trailing byte fails closed.

Any later Core, token, fee, accounting, postcondition, or compute failure rolls
back the earlier engine and opaque mutations. Token-2022 transfer hooks would
introduce a later untrusted callback graph and are outside this profile.

## Private wire candidates

Every encoding in this section is experiment-local. All integers are
little-endian. Every reserved byte must be zero. Decoders require exact lengths
and reject unknown flags, versions, padding, and trailing bytes.

### Top-level execution envelope

The private top-level header is:

```text
ExecuteEnvelopeHeaderCandidateV0 {
  wire_version: u8
  loader_policy_account_count: u8
  domain_control_account_count: u8
  authorization_account_count: u8
  protected_profile_account_count: u8
  fee_control_account_count: u8
  settlement_capability_count: u8
  opaque_capability_count: u8
  domain_count: u8
  intent_count: u8
  inline_intent_row_count: u8
  asset_count: u8
  fee_shard_count: u8
  context_row_count: u8
  maximum_engine_moves: u8
  flags: u8
  payload_len: u16
  reserved_0: [u8; 6]
  expires_at_slot: u64
  expected_engine_sequence: u64
  intent_set_digest: [u8; 32]
  domain_set_digest: [u8; 32]
  settlement_capability_root: [u8; 32]
  expected_opaque_capability_root: [u8; 32]
  fee_policy_digest: [u8; 32]
  expected_engine_code_snapshot_digest: [u8; 32]
  payload_digest: [u8; 32]
}
```

The header is exactly 264 bytes. `flags` and every reserved byte must be zero.
It is followed, in this exact order, by:

```text
DomainControlRowCandidateV0[domain_count]
IntentRowCandidateV0[inline_intent_row_count]
FeeShardRowCandidateV0[fee_shard_count]
SettlementCapabilityRowCandidateV0[settlement_capability_count]
payload[payload_len]
```

The private declaration rows are:

```text
DomainControlRowCandidateV0 {                    // exactly 8 bytes
  descriptor_control_offset: u8
  admission_control_offset_or_none: u8
  accounting_control_offset: u8
  flags: u8
  reserved: [u8; 4]
}

IntentRowCandidateV0 {                           // exactly 48 bytes
  authorization_slot: u8
  witness_kind: u8
  primary_authorization_offset_or_none: u8
  flags: u8
  expected_fill_sequence: u32
  expires_at_slot: u64
  intent_digest: [u8; 32]
}

FeeShardRowCandidateV0 {                         // exactly 8 bytes
  descriptor_control_offset: u8
  liability_control_offset: u8
  vault_settlement_capability_index: u8
  asset_index: u8
  flags: u8
  reserved: [u8; 3]
}

SettlementCapabilityRowCandidateV0 {            // exactly 28 bytes
  asset_index: u8
  domain_index_or_none: u8
  authorization_slot_or_none: u8
  authority_class: u8
  fee_shard_index_or_none: u8
  rights_bits: u16
  fee_class: u8
  flags: u8
  reserved: [u8; 3]
  maximum_debit: u64
  minimum_credit: u64
}
```

`255` is the only absent-index sentinel. Domain rows and fee rows are in their
canonical index order. Inline intent rows are strictly increasing by
`authorization_slot`. Settlement row position is both its capability index and
the relative position of its endpoint in the settlement-account segment.
Private witness kinds `0`, `1`, and `2` mean direct actor signer, exact one-shot
delegate, and stored authorization. Kind `2` is loaded from a stored account;
kinds `0` and `1` are inline. Unknown witness kinds, authority classes, rights,
fee classes, flags, nonzero reserved bytes, overlapping control offsets,
unreferenced control accounts, and out-of-range indices fail before callback.

Core never guesses a capability role, authorization slot, domain, fee class, or
bound from an account owner or position. It decodes the row, then proves every
declared fact from the exact account, mint, domain, fee-policy, and authorization
state. `settlement_capability_root` commits the complete FeeShard and Settlement
rows plus the validated landing-time endpoint and fee-control facts.

Direct current-transaction intents have inline rows and point to their exact
actor signer in the authorization-control segment. A stored intent has no
inline row: one Core-owned `StoredAuthorizationCandidateV0` in that segment
contains the same canonical Intent row, actor, capability-declaration digest,
remaining debit and credit bounds per capability, cumulative fee basis per
bucket, fill sequence, expiry, cancellation, and terminal state. Core parses
these exact tagged fields; it does not infer them from token accounts. Inline
and stored rows must resolve to exactly the half-open slot range
`[0, intent_count)`, without a duplicate or missing slot, and every
authorization-control account must be consumed by one resolved row.

The full instruction-data length is therefore exactly:

```text
8 + 264 + 8*domain_count + 48*inline_intent_row_count
        + 8*fee_shard_count + 28*settlement_capability_count
        + payload_len
```

At the independent private encoding maxima of four domains, four inline intents,
four fee shards, 12 settlement capabilities, and a 128-byte payload, this is 992
bytes. That is an encoding ceiling, not proof that the complete transaction
packet fits; every actual matrix point is serialized and measured.

The outer Core account order is exact for this candidate:

```text
fixed prefix
  0. experimental Core configuration                 read-only, non-signer
  1. market descriptor                               read-only, non-signer
  2. authenticated protocol-fee policy               read-only, non-signer
  3. selected engine program                          read-only, executable
  4. callback authority PDA                           read-only, non-signer

dynamic segments
  5..L. loader-policy closure                         read-only, non-signer
  next. domain descriptor/admission/accounting        exact row privileges
  next. direct or stored authorization controls       exact derived privileges
  next. protected-profile programs, mints, and facts  read-only, non-signer
  next. fee-shard descriptors and liability ledgers   exact row privileges
  next. settlement-capability endpoints               exact derived privileges
  last. ordered opaque capability tail                actual accepted privileges
```

The five control-segment counts before `settlement_capability_count` delimit the
control segments. `settlement_capability_count` and
`opaque_capability_count` delimit the last two. Exact consumption must reach the
end of the landed account slice; there are no unclassified remaining accounts.
A zero-length dynamic segment is valid only where its policy permits it.

For the classic-SPL fixture, the protected-profile segment is exactly the
read-only classic SPL Token program followed by one read-only mint per asset in
asset-index order. Each closed-rule participating domain consumes exactly three
domain controls in its row: descriptor read-only, admission read-only, and local
accounting writable. An open-rule domain consumes descriptor and accounting;
its admission offset is `255`. Each fee shard consumes exactly two fee controls:
an authenticated read-only shard descriptor and its Core-owned writable
liability ledger. Its exact fee vault is the settlement endpoint named by the
FeeShard row, not an omitted or inferred account.

The fixed engine CPI is assembled from account 4 plus only the final opaque
segment as callee metas. Accounts 0 through 2, account 4 before signing, and
every intervening control or settlement account are absent from the callee meta
list. The engine program `AccountInfo` at position 3 is still supplied to the
caller's `invoke_signed` account-info slice as the executable target, as Solana
requires; it is not exposed to the engine as a callee capability. The callee
account order remains exactly the read-only signed callback PDA followed by the
unmodified opaque tail, with no engine-state prefix.

Before interpreting segments, Core unions signer and writable privilege for
every equal public key across the complete outer instruction. It then applies
the fixed and segment-specific privilege rules and the cross-table alias rules.
Instruction account order is part of the private envelope digest. A builder may
use address lookup tables, but resolution cannot change this effective order or
security identity.

`expected_engine_sequence` is executor-selected freshness and is not silently
treated as a user-authorized economic term. Every direct or stored intent still
has its own expiry and bounds. Every resolved intent digest also commits the
exact `expected_engine_code_snapshot_digest`, so mutable-controller admission
does not make an old user authorization float across deployments.

### Engine request

The engine CPI begins with an eight-byte private discriminator followed by:

```text
EngineRequestHeaderCandidateV0 {
  magic: [u8; 8]
  wire_version: u8
  phase: u8
  settlement_capability_count: u8
  opaque_capability_count: u8
  intent_count: u8
  domain_count: u8
  asset_count: u8
  context_row_count: u8
  payload_len: u16
  reserved: [u8; 6]
  market_binding_digest: [u8; 32]
  engine_instance_id: [u8; 32]
  engine_interface_id: [u8; 32]
  intent_set_digest: [u8; 32]
  domain_set_digest: [u8; 32]
  settlement_capability_root: [u8; 32]
  opaque_capability_root: [u8; 32]
  engine_code_snapshot_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
}
```

The header is exactly 312 bytes, or 320 bytes with the private discriminator.
The reserved bytes are zero. It is followed first by exactly `asset_count`
authenticated asset rows:

```text
EngineAssetRowCandidateV0 {
  asset_index: u8
  asset_flags: u8
  decimals: u8
  reserved: u8
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
}
```

Each asset row is exactly 100 bytes. Indices are contiguous from zero and rows
are in index order. The classic-SPL fixture requires `asset_flags == 0`; unknown
bits and nonzero reserved bytes fail. Asset identity is the exact mint for this
profile. The engine receives identity facts as data without receiving any
protected mint or token `AccountInfo`.

The asset rows are followed by exactly `domain_count` authenticated domain rows:

```text
EngineDomainRowCandidateV0 {
  domain_index: u8
  reserved_0: [u8; 7]
  domain_descriptor: [u8; 32]
  domain_revision: u64
  admission_digest: [u8; 32]
  accounting_profile_digest: [u8; 32]
}
```

Each domain row is exactly 112 bytes. Indices are contiguous from zero and rows
are in index order. These are authenticated economic inputs, not authorities;
the corresponding domain control accounts stay in Core.

The domain rows are followed by exactly `context_row_count` settlement-context
rows:

```text
EngineContextRowCandidateV0 {
  settlement_capability_index: u8
  asset_index: u8
  domain_index_or_none: u8
  authorization_slot_or_none: u8
  rights_bits: u16
  fee_class: u8
  context_flags: u8
  endpoint_key: [u8; 32]
  observed_before: u64
  accounted_before_or_zero: u64
  remaining_maximum_debit: u64
  remaining_minimum_credit: u64
}
```

Each context row is exactly 72 bytes. `255` is the only no-domain or
no-authorization sentinel. Private `rights_bits` are debit, credit,
domain-accounted, and exact-external-recipient; all other bits and all
`context_flags` fail. A Core-reserved fee destination has no engine context row.
The exact row meaning is fixed by the classic-SPL capability profile; it does
not introduce product verbs. Rows are strictly increasing by settlement
capability index, and every referenced asset, domain, and authorization index is
in range. Endpoint keys are authenticated data only and their protected
`AccountInfo`s stay in Core.

The request ends with exactly `payload_len` opaque bytes. Its private request
digest is the domain-separated hash of the discriminator, complete header,
asset rows, domain rows, context rows, and payload. That digest is bound by the
engine receipt.

At the private maxima of eight assets, four domains, 12 context rows, and a
128-byte payload, the complete engine instruction data is 2,560 bytes, below
both the private 8,192-byte headroom ceiling and current 10,240-byte CPI data
limit. This is an encoding maximum, not evidence that every independent account
and move maximum fits the transaction packet, lock, or compute gates.

### Engine effect receipt

The candidate return data is:

```text
EffectReceiptCandidateV0 {
  magic: [u8; 8]
  wire_version: u8
  phase: u8
  move_count: u8
  flags: u8
  request_digest: [u8; 32]
  intent_set_digest: [u8; 32]
  settlement_capability_root: [u8; 32]
  engine_sequence: u64
  engine_evidence_digest: [u8; 32]
  moves: [MoveCandidateV0; move_count]
}
```

The fixed receipt is exactly 148 bytes. Every move is 10 bytes, so the private
12-move maximum is 268 bytes. `flags` must be zero. Core computes the canonical
effect digest itself after decoding and validation.

The receipt deliberately contains no action, product, authorization mode,
engine-state account, fee move, position mutation, escrow mutation, asset
adapter call, or arbitrary program instruction.

### Private measurement limits

The experiment uses these ceilings only to create deterministic resource cases:

| Item | Private maximum |
| --- | ---: |
| Participating domains | 4 |
| Authorization intents | 8 |
| Inline intent rows | 4 |
| Authenticated assets | 8 |
| Loader-policy accounts | 1 |
| Domain-control accounts | 12 |
| Authorization-control accounts | 8 |
| Protected-profile accounts | 9 |
| Fee shards | 4 |
| Fee-control accounts | 8 |
| Settlement capabilities | 12 |
| Engine moves | 12 |
| Opaque-tail account positions | 8 |
| Engine context rows | 12 |
| Opaque payload | 128 bytes |
| Top-level instruction data | 992 bytes at all encoding maxima |
| Engine request | 2,560 bytes at all encoding maxima |
| Engine receipt | 268 bytes at 12 moves |

Fields remain wider where convenient so hitting one experiment ceiling does not
silently define a public integer width. A result may recommend smaller, larger,
or structurally different product limits only through a later decision. These
are independent sweep maxima, not a claim that their Cartesian product can fit
one transaction.

## Reference semantics

The same Core binary, move decoder, validator, fee kernel, and event evidence
must execute all reference engines. Core code may not branch on a reference
engine identity or semantic label.

### Stateless allocation engine

A stateless engine uses no engine-owned state account in its opaque tail. It
derives one deterministic allocation from authenticated context and payload,
returns moves, and increments no engine sequence except an agreed stateless
sentinel. This proves that zero fixed state accounts are valid.

### Constant-product engine

One engine-owned state account in the opaque tail stores its curve parameters
and sequence. It computes a conventional exact-input two-asset result and
returns the same authority-level moves Core would use for any two-asset
allocation. Core does not reproduce its formula or label the result a swap.

### Stored partial-fill auction

An engine uses multiple engine-owned opaque-tail accounts for auction state and
orders. One or more stored Core authorizations contribute bounded debits and
credits. A fill updates engine state, returns several moves, and exercises the
cumulative fee equation. A second fill from cloned state proves partition-
independent cumulative fees and explicit replay state.

### Multi-intent batch allocation

At least four independently authorized actors and two classic-SPL assets settle
one batch. The engine may direct several source capabilities to several exact
credit recipients. Core sees only capabilities, moves, bounds, admissions, and
fees. One actor's bounds, balance, fee, or authorization state cannot subsidize
another actor.

### Multi-recipient inventory distribution

A 0-decimal classic-SPL inventory asset and a payment asset settle to an exact
buyer recipient plus multiple payment recipients. The Core verifies transfers
but does not claim that the inventory is an NFT, that a payment is a royalty, or
that an engine-defined split is economically correct.

Together the fixtures exercise zero, one, and many engine-owned state accounts
without changing the fixed callback prefix.

## Required hostile evidence

Every cross-program security claim requires exact-SBF evidence. Pure codec,
hash, canonicalization, arithmetic, and model properties may use deterministic
unit or property tests in addition to SBF cases.

### Engine callback and identity

Tests cover:

- direct engine invocation without Core;
- wrong callback address, signer privilege, phase, market, interface, instance,
  intent set, domain set, capability root, or payload;
- callback forwarding to an opaque helper and attempted reuse after Core
  returns;
- wrong engine program or substituted executable account;
- zero, one, and many engine-owned state accounts in the opaque tail;
- an engine that assumes one fixed state prefix and therefore fails the
  candidate contract;
- return data set by a descendant, missing return data, stale data, trailing
  bytes, unknown version, nonzero flags, wrong request digest, and oversized
  move count; and
- engine mutation followed by every later failure class with complete rollback.

### Capability closure and aliasing

Tests cover:

- duplicate, reordered, omitted, and added opaque positions;
- under-counted, over-counted, overlapping, or trailing outer account segments;
- landing-time owner or executable-state drift after authorization;
- duplicate public keys with signer or writable privilege hidden in another
  position;
- static-account and address-lookup-table aliasing;
- settlement/opaque cross-table aliases;
- callback, fee, admission, loader-policy, authorization, domain, mint, token
  program, and Core-program aliases;
- opaque signers, Core-owned accounts, writable executables, writable classic
  token accounts, and writable Token-2022 accounts;
- protected profile ancillary-account aliases; and
- engine PDA signers exercising all authority already available in the opaque
  plane while remaining unable to reach the protected plane.

Canonical-wire tests also mutate every count, index, length prefix, reserved
byte, rights bit, asset row, domain row, context row, list position, and digest
label. Golden vectors cover empty, singleton, and maximum lists so an
ambiguous concatenation or client-side sort cannot accidentally pass.

### Move decoding and accounting

Tests cover:

- zero amount, identical endpoints, out-of-range indices, wrong rights, reserved
  fee indices, and asset/profile mismatch;
- duplicate and non-canonical row order;
- a capability used as both source and destination;
- checked aggregation overflow at `u64` and `u128` boundaries;
- source debit over authorization, destination credit below a user's minimum,
  domain debit over accounted balance, and insufficient raw vault balance;
- a globally balanced graph containing an unauthorized victim-domain debit;
- exact per-domain accounting when several admitted domains participate;
- donation before execution, with raw balance increasing but no accounted right;
- unexpected token debit or credit and post-balance mismatch; and
- failure during every protected transfer position with complete rollback.

### Domain admission

Tests cover:

- a new attacker market naming an existing victim domain;
- wrong market, engine program, interface, instance policy, code policy,
  capability profile, descriptor, or revision;
- one domain deliberately admitting several markets;
- one open deterministic domain rule;
- revoked or superseded mutable local admission where the selected private rule
  permits changes;
- a non-participating domain supplied as a credit destination, proving that a raw
  donation creates no accounting; and
- no global writable registry or platform signer in any successful path.

### Loader-aware policy

Tests cover:

- supported immutable, pinned mutable-deployment, and explicit mutable-controller
  policy fixtures;
- wrong loader, program-data address, deployment slot, or upgrade authority;
- changed program deployment under an exact policy;
- upgradeable-loader v3 execution at the ProgramData deployment slot;
- writable effective Program or ProgramData privilege hidden by a duplicate;
- addition or removal of an upgrade authority contrary to the bound policy;
- a later deployment accepted by mutable-controller admission only with a new
  exact execution snapshot and user intent;
- an old intent or envelope snapshot replayed after that later deployment;
- policy disagreement between market and one participating domain; and
- unknown loader or malformed loader state.

No test may describe a mutable-controller fixture as pinned or immutable.

### Authorization neutrality and replay

Tests cover:

- equivalent direct, exact delegated, stored, partial, and multi-intent paths;
- wrong actor, source, recipient, cap root, intent order, nonce, fill sequence,
  expiry, cancellation state, and remaining amount;
- post-success replay of direct and stored intents;
- exact one-shot delegate success only when aggregate source plus Core-fee debit
  consumes the complete delegated amount and leaves zero delegation;
- a maximum-only, leftover, variable, partial, or zero-debit delegate rejected
  unless a StoredAuthorization supplies explicit replay state;
- partial fills whose sum exactly reaches the authorized maximum;
- one additional partial fill after exhaustion;
- zero-debit or fee-only execution replay;
- duplicate intent digest in one multi-intent set;
- the same protected public key in multiple authorization slots;
- one user's surplus, minimum, fee ceiling, or remaining amount being applied to
  another user; and
- mutation after stored authorization reservation followed by rollback to the
  exact pre-execution authorization state.

### Protocol fees

Tests cover:

- missing, zeroed, redirected, caller-selected, engine-selected, duplicated, or
  wrong-policy fee effects;
- missing, aliased, reordered, read-only, or wrong-policy shard descriptor,
  liability ledger, or fee-vault capability;
- engine attempts to reference the reserved fee destination;
- equivalent unsplit and split engine graphs producing one identical aggregate
  rate assessment;
- dust splitting around every rounding boundary;
- a source/destination cycle rejected before fee calculation;
- cumulative fee equality for at least 4,096 partitions and boundary values;
- fixed fee once per committed envelope, including a zero-debit semantic action;
- independent fee attribution for multiple actors and assets;
- fee above a user ceiling;
- raw fee-vault donation creating no liability; and
- fee transfer or post-credit failure rolling back engine, authorization,
  protected, fee, and Core state.

### Product neutrality

Tests or compile-time fixtures prove that:

- the Core crate contains no reference-engine program ID;
- the Core instruction and effect codec contain no action or product enum;
- every reference engine uses the same callback account prefix;
- zero, one, and many engine state accounts are all expressed only through the
  opaque tail;
- the same move bytes can arise from different engine-defined semantics; and
- no engine-provided label changes validation, fee class, evidence class, or
  authority.

## Atomicity and liveness boundary

The selected engine remains the economic authorization oracle for every admitted
domain. A compromised engine may return a conservative but economically
destructive graph that drains participating domains within the capabilities and
user bounds those domains accepted. Core containment does not imply fair pricing
or provider solvency.

Engine, helper, stored-intent, token, fee, domain-accounting, and Core writes
occur in one Solana transaction. A failed CPI is not catchable as an alternate
settlement branch. Any failure rolls back account-state changes. Transaction
fees and failed-transaction metadata are not rolled back.

An unavailable, upgraded, or permanently failing engine can halt a market.
Before persistent Core custody, every domain still requires a separately
accepted exit class: no persistent custody, an exact Core-verifiable
engine-independent claim, or disclosed engine-liveness dependence. This
experiment creates no provider entitlement and proves no withdrawal path.

Shared domains intentionally share locks and liveness. Unrelated domains use no
global writable registry, accumulator, sequence, or fee account, so one engine's
failure does not create a protocol-wide state dependency.

## Resource sweep

The private gate pins and must revalidate the active 2026-08-28 legacy/v0
runtime baseline before recording results:

- 1,232 serialized transaction bytes;
- 64 unique locked accounts;
- 255 `AccountInfo` positions in one CPI under the active SIMD-0339 gate, which
  does not increase the 64 unique-account lock limit;
- 1,400,000 compute units when explicitly requested;
- 946 base compute units per invocation under the same active CPI cost gate,
  before the invoked work and account-data charges;
- instruction stack height 5, meaning top level plus four nested invocations;
- 64 total executed instructions;
- 1,024 bytes in the single transaction return-data buffer;
- 10,240 bytes of instruction and CPI data; and
- 64 MiB, or 67,108,864 bytes, of loaded account data.

Address lookup tables compress v0 addresses but do not remove locks, cannot
supply signer keys, and require the active table state to be warmed up. Their
256-address table capacity is not transaction account capacity. Transaction v1,
4,096-byte messages, a 128-account lock gate, and a deeper CPI stack are not
assumed.

The experiment records successful and failing sweeps across:

| Dimension | Values |
| --- | --- |
| Engine moves | 2, 4, 8, 12 |
| Settlement capabilities | 4, 8, 12 |
| Participating domains | 1, 2, 4 |
| Intents | 1, 2, 4, 8 |
| Inline intent rows | 0, 1, 2, 4 |
| Assets | 1, 2, 4, 8 |
| Fee shards | 1, 2, 4 |
| Opaque-tail positions | 0, 4, 8 |
| Payload bytes | 0, 32, 128 |
| Engine state accounts | 0, 1, several within the opaque tail |
| Execution | direct and permissionless-routed |
| Nested engine behavior | no helper and one helper CPI |
| Authorization | direct, exact delegate, stored, partial, multi-intent |

At least one maximum controlled path uses a real v0 address lookup table and
executes:

```text
permissionless router -> Core -> engine -> opaque helper
```

After the engine returns, the same Core invocation executes every real
classic-SPL protected move and fee move. The helper and token calls need not be
nested simultaneously, but their frames and shared compute all count.

The complete Core account-position equation is:

```text
5 fixed + loader + domain controls + authorization controls
        + protected-profile controls + fee controls
        + settlement endpoints + opaque positions
```

A closed domain contributes three controls; an open domain contributes two. A
classic-SPL profile contributes one program plus one mint per asset. Each fee
shard contributes two fee controls in addition to its vault settlement endpoint.
No descriptor, admission, accounting, fee ledger, shard, or vault may be omitted
from a resource row.

The predeclared combined case uses the routed path, two closed domains, four
stored intents, two assets, two fee shards, one loader-policy account, ten
settlement capabilities, ten moves, four opaque positions, and a 32-byte
payload. Its exact Core positional count is 37 by the equation above. The test
must serialize the actual v0 message and derive unique locks after privilege
union, including payer and every invoked program ID; this document makes no
unmeasured unique-lock or packet-size prediction.

Separate one-axis boundary cases raise settlement capabilities and moves to 12,
opaque positions and intents to eight, assets to eight, domains and fee shards
to four, and every control segment to its private maximum while holding other
dimensions at their documented baseline. Every reported row includes the exact
resolved unique-key set; position counts alone are not lock evidence.

For the controlled maximum accepted case, the experiment requires at least 20%
headroom against every countable active limit and one unused stack level:

| Resource | Experimental acceptance ceiling |
| --- | ---: |
| Serialized legacy/v0 transaction | at most 985 bytes |
| Unique locked accounts | at most 51 |
| CPI `AccountInfo` positions | at most 204 |
| Compute | at most 1,120,000 CU |
| Maximum stack height reached | at most 4 |
| Executed instructions | at most 51 |
| Return data | at most 819 bytes |
| Instruction or CPI data | at most 8,192 bytes |
| Loaded account data | at most 53,687,091 bytes |

The exact 20% policy is a private falsification threshold, not a public product
limit. The test records writable-lock counts and the observed invoke cost even
where they are not the first failing limit.

If the full 12-capability or 12-move private ceiling fails but all required
reference semantics pass a smaller predeclared matrix with the required
headroom, the result may recommend a smaller next experiment. It may not call
the failed maximum supported or hide which dimension failed.

## Acceptance criteria

The private direction passes only if executable evidence establishes all of the
following:

1. Every reference semantic executes through one unchanged Core effect path with
   no product enum, engine-specific branch, or engine-specific fixed account.
2. The engine callback prefix is exactly one read-only callback signer followed
   by the ordered opaque tail; zero, one, and many engine-owned state accounts
   all work there.
3. No protected account or authority enters the engine CPI or any descendant
   through authority granted by Core.
4. Settlement and opaque capability roots bind the actual landing-time closure,
   preserve opaque position and multiplicity, normalize privilege by key, and
   reject every cross-plane alias; canonical Intent, DomainControl, FeeShard,
   and SettlementCapability rows leave no inferred role or bound.
5. Every domain debit has an exact domain-local admission proof; self-declared
   or non-participating domains fail before the engine callback.
6. The Move normal form, asset conservation, per-domain accounting, user bounds,
   exact observed deltas, and donation boundary all pass differential and hostile
   tests.
7. Direct, stored, partial, and multi-intent authorization use one unchanged
   engine request and Move result, with explicit replay state wherever delegate
   consumption is not sufficient.
8. Core alone derives one mandatory fee assessment, cumulative partial-fill fees
   are partition-independent, and every omission, redirection, duplication,
   netting, and dust-splitting attack fails; each observed fee credit updates its
   exact protected shard liability ledger once.
9. Loader-aware immutable, exact-deployment, and explicitly mutable policies are
   distinguished through separate admission-policy and per-execution snapshot
   digests, including the loader-v3 read-only and strict later-slot gates.
10. Wrong receipt setters, malformed plans, late transfer or fee failures, and
    resource exhaustion leave no partial account-state transition.
11. Direct and routed paths from cloned initial state produce the same authorized
    semantic outcome and evidence classes.
12. The controlled maximum retains the declared packet, lock, compute, stack,
    trace, and return-data headroom under the pinned active runtime.
13. No private byte, seed, discriminator, bound, fixture, program ID, or account
    layout is exported as a compatibility promise.

Passing means the capability-indexed Move direction survived this experiment.
It does not mean the exact candidate wire or limits are accepted.

## Falsification criteria

The direction is rejected, narrowed, or sent to a new architecture decision if:

- one reference engine needs a Core-visible product/action discriminator;
- the fixed callback prefix needs any engine-state account;
- the engine needs a protected signer, protected writable account, or arbitrary
  Core-authorized driver to produce the required result;
- an account can occupy inconsistent protected roles after privilege union;
- Core must infer an intent role, authorization slot, bound, fee class, domain,
  shard, or liability target from undeclared account position or ownership;
- global conservation can hide an unauthorized domain debit or accounting loss;
- direct, stored, partial, or multi-intent authorization changes the effect
  semantics or engine result format;
- a nonce without explicit state permits any replay not stopped by complete
  exact-delegate consumption;
- equivalent fee bases produce different cumulative fees because of split,
  ordering, netting, or rounding;
- an engine or caller can choose a cheaper semantic fee label for equivalent
  protected effects;
- a market can adopt an existing domain without that domain's local proof;
- loader or upgrade-state drift remains indistinguishable under a policy that
  claims exact code;
- mutable-controller admission either pins every future deployment slot or lets
  an old intent float to a different code snapshot;
- a post-settlement engine callback is required for the baseline semantics;
- correctness relies on a future Solana feature or unmeasured runtime behavior;
- the required reference matrix cannot retain the declared resource headroom; or
- the implementation needs a generic arbitrary-CPI interpreter in Core.

A failed hypothesis is a useful result. The result record must identify the
smallest failing fixture and authority or resource reason rather than weakening
the boundary until it passes.

## Required evidence outputs

The later experiment result must record:

- exact source commit and tree, parent commit, branch, and pull request;
- exact host Rust, Solana/Agave, SBF/SBPF compiler, platform-tools, Anchor,
  LiteSVM, Mollusk or Surfpool versions;
- exact cluster genesis, software, feature-set, and active-feature observations
  used to revalidate the runtime baseline;
- every disposable program ID and exact canonical Ubuntu SBF artifact hash;
- at least one independent reproducible artifact comparison or an explicit
  statement that it remains absent;
- complete private wire layouts, exact lengths, domain-separated hash inputs,
  canonical Intent/DomainControl/FeeShard/SettlementCapability byte vectors,
  stored-row equivalence vectors, and mutation coverage;
- the complete unit, property, differential, exact-SBF, router, engine, helper,
  authorization, domain, loader, fee, and rollback test inventory;
- direct and routed packet bytes, static and ALT-loaded keys, unique and writable
  locks, CPI `AccountInfo` positions, compute and invoke cost, maximum stack
  height, total frames, instruction trace length, CPI data, return data, and
  loaded-account data for every resource fixture;
- zero-, one-, and many-engine-state tail evidence;
- reference-semantic parity showing one unchanged Core effect path;
- per-authorization-mode effect and evidence parity;
- cumulative fee vectors across partition and rounding boundaries;
- domain-admission, non-participating-domain, alias, callback-forwarding,
  admission-policy/code-snapshot drift, same-slot loader rejection, replay, fee-
  liability, and late-failure attack traces;
- a CoreVerified-versus-EngineAttested event classification example;
- rejected alternatives, failed maxima, remaining uncertainty, and the exact
  smallest next gate; and
- a maturity checkpoint that cannot be presented as a production audit.

The result must distinguish local host tests, exact-SBF execution, CI artifact
reproduction, fork behavior, devnet behavior, deployment, and onchain source
verification. This experiment requires only the first three. No program from it
may be deployed or entrusted with real funds.

## Explicit non-goals

This experiment does not establish:

- a public engine, intent, capability, move, receipt, event, IDL, SDK, or account
  ABI;
- final product account, payload, move, intent, domain, packet, or compute limits;
- a generic arbitrary-action interpreter or safe arbitrary CPI;
- fair prices, honest engines, profitable markets, provider solvency, or MEV
  resistance;
- full order placement, cancellation, matching, auction, or asynchronous intent
  lifecycle semantics beyond the minimum authorization-neutral fixtures;
- Token-2022, transfer hooks, SOL/WSOL lifecycle, mint, burn, NFTs, compressed
  assets, external settlement drivers, or custom asset authority;
- Core-native positions, provider claims, fee claims, withdrawals, recovery,
  migration, or an engine-independent exit;
- final loader support, release manifests, governance, upgrade process,
  immutability, or deployment policy;
- a fee rate, recipient, asset schedule, governance right, or business forecast;
- router, wallet, indexer, archive, monitoring, or incident-response readiness;
- devnet or mainnet execution; or
- immunity from implementation defects.

Each omitted protected authority receives its own decision and hostile tests. It
is not added as a flag, product enum, arbitrary adapter instruction, or opaque
receipt field merely to make this private candidate appear universal.
