# Developer contract

Status: Draft

## Purpose

This document defines the candidate developer-facing boundaries for a
permissionless Programmable Solana protocol. It separates the small contract
that may eventually require wire compatibility from generated clients,
discovery metadata, and product-specific engine semantics.

This draft does not accept public ABI bytes, account layouts, discriminators,
resource maxima, package names, deployment addresses, or migration promises.
The existing authority-kernel, generated-settlement, and routed-callback wire
formats remain private experiment evidence. In particular, their fixed account
topologies, exact-input pair semantics, payload limits, receipt lengths, and
numeric revisions must not become defaults merely because they already exist.

The intended outcome is that an engine developer can deploy code, create a
market, construct transactions, and verify canonical state without approval,
an API key, a Programmable-operated service, or an implicit dependency on one
reference product.

## Four separate layers

The developer contract has four layers with different compatibility and trust
properties. A format or claim from one layer must not silently acquire the
authority of another.

### 1. Onchain interface

The onchain interface is the smallest compatibility boundary between a Core
major and an engine interface revision. It covers only:

- exact instruction dispatch and framing;
- the fixed accounts that are genuinely universal;
- the ordered opaque capability tail;
- canonical context and result bindings;
- Core-understood settlement and protected-capability profiles;
- account identity and PDA derivation rules;
- stable error identities; and
- canonical Core evidence events.

It must not contain UI actions, product names, quote APIs, an arbitrary-action
enum, a fixed pricing model, a fixed number of engine-state accounts, or an
open-ended interpreter for future authorities.

### 2. Interface description and codecs

Reviewed IDLs, a language-neutral codec, PDA helpers, and compatibility vectors
describe the exact onchain interface. Rust and TypeScript implementations must
produce and consume identical bytes, hashes, account order, error identities,
and event records.

This layer contains no Core business logic and must be usable by a third-party
engine without importing the Core implementation. A generated client is not
the specification by itself; checked source definitions and compatibility
fixtures remain authoritative.

### 3. Client SDK

The client SDK validates program-owned state, resolves engine-provided opaque
inputs, constructs the final transaction, calculates actual effective
privileges, presents a signing review, sends through caller-supplied RPC and
wallet interfaces, and verifies committed state.

Its ergonomic API and release version are independent from the onchain
interface version. A high-level SDK may become safer or easier to use without
changing wire bytes. Low-level codec and instruction-builder APIs must remain
available so no hosted router, quote server, indexer, or first-party UI is
required for liveness.

### 4. Discovery and semantic metadata

Engine manifests, action schemas, display metadata, decoders, risk statements,
resource estimates, source links, and optional adapters belong to an offchain
discovery layer. They may be content-addressed and publisher-attested, but they
are never settlement authority.

Core does not execute a manifest, dereference a URL, run arbitrary adapter
code, or infer protected asset semantics from a label. A first-party client may
support declarative schemas. It must not silently execute engine-supplied
JavaScript or present an engine assertion as Core-verified state.

## Exact version selection

Different version axes must remain explicit and independent:

- a Core major is selected by an exact deployed program ID;
- an engine interface is selected by an exact interface reference;
- Core-understood capability, asset, settlement, fee, custody, and exit
  profiles are selected by exact identifiers;
- an engine release is identified by its program ID and accepted code policy,
  not a self-declared numeric revision;
- a Codama standard version describes the IDL format, not the protocol wire;
- manifest schemas have their own version; and
- Rust and TypeScript packages use independent release versions.

An interface reference must logically bind an interface family, major,
revision, and canonical interface-schema hash. The exact representation remains
open until the public wire gate.

Offchain tooling may compare the sets or ranges supported by a Core release and
an engine before market creation. Market creation selects one exact interface
reference and stores it immutably. Execution performs exact equality and
support checks; there is no runtime range negotiation, highest-common-version
selection, fallback decoder, or silent downgrade.

Unknown interface revisions and unknown required Core profiles fail closed.
Optional discovery metadata may be ignored offchain only when doing so cannot
change transaction authority or settlement meaning.

A new Core major is a new deployment. A new engine wire revision uses an
explicit new dispatch identity. Deprecation metadata must not change the
meaning of an existing market. Side-by-side versions are containment and
compatibility, not automatic migration.

## Canonical market identity

Every market must be derived from a canonical immutable descriptor rather than
from the payer's identity or an arbitrary creator namespace.

The logical market descriptor must bind at least:

- its descriptor schema;
- the exact Core interface reference;
- the engine program;
- the accepted engine code and upgrade policy;
- the exact engine interface reference;
- an opaque engine-instance identifier;
- role-tagged immutable liquidity-domain descriptor references;
- exact protected capability, asset, settlement, fee, admission, custody, and
  exit profile references that apply to the market;
- any other immutable policy reference that changes market authority or
  accounting; and
- an explicit instance salt when multiple intentionally distinct markets may
  otherwise share the same descriptor.

The descriptor must exclude:

- payer or initializer identity unless that identity is an actual immutable
  market rule;
- display name, symbol, icon, URL, social metadata, or indexer keys;
- mutable balances, quotes, counters, or engine state; and
- an address derived from the market itself when including it would create a
  derivation cycle.

A versioned canonical encoder hashes the complete descriptor with explicit
domain separation. The Core derives the market PDA from a versioned market
namespace and that descriptor hash under the selected Core program. The exact
encoding and seed bytes remain a public-wire decision; the invariant is that
the same descriptor and salt under the same Core deployment produce one market
and a different authority-relevant descriptor cannot alias it.

An engine-instance identifier is not required to be an account address. A
stateless engine, one-state-account engine, shared-state engine, and
multi-account engine must all fit the same market identity. Engine accounts
that are naturally derived from the market can be created after the market PDA
is known and supplied through the opaque capability plane.

The globally meaningful identity also includes the cluster or deployment
manifest, Core program ID, and market PDA. An equal address on another cluster
is not evidence of the same deployment.

## Market and engine discovery

Each Core major must own a small versioned market header whose discriminator,
minimum length, descriptor hash, engine program, exact interface reference,
engine-instance identifier, and essential immutable policy references can be
validated without an indexer. The exact account bytes remain open.

An independent client discovers current markets by scanning accounts owned by
each known Core program ID, filtering the versioned header, and then validating
owner, discriminator, length, PDA, descriptor hash, and referenced Core-owned
state. Creation and settlement events may accelerate an indexer but are not the
only source of current truth.

Known Core program IDs come from append-only authenticated deployment
manifests. These manifests are discovery and provenance evidence, not an
onchain settlement registry.

The engine program is discovered transitively from a validated market. An
engine that has no market may be shared by program ID, package, canonical
Program Metadata, or a permissionless offchain directory. No global writable
engine registry, listing vote, allowlist, or Programmable-controlled signer may
be required to create or execute a compatible market. Optional directories can
rank, hide, or annotate entries, but cannot create canonicality.

Canonical publisher metadata does not certify an engine's code, economics, or
safety. Core must not fetch IDLs, manifests, or schemas during settlement.

## IDL, Codama, and client generation

Every actual deployable program must have an IDL with its real program ID. A
generic engine interface must not masquerade as a deployable Codama program by
using a fake default address. The common interface is maintained as reviewed
wire definitions and codecs; each real engine can include the shared contract
in its own program-specific IDL.

The candidate release pipeline is:

1. generate the program IDL deterministically from reviewed source;
2. normalize it into the pinned Codama standard with locked generator and
   visitor versions;
3. add only reviewed PDA, lifecycle, documentation, display, and resolver
   metadata;
4. validate explicit dispatch identities, account order, types, errors, events,
   and program ID against source fixtures;
5. generate canonical Rust and TypeScript clients;
6. format, compile, and test both clients against cross-language golden vectors;
7. reject generated drift in CI; and
8. bind source, artifact, IDL, toolchain, and generated-client evidence in the
   release manifest.

Codama's standard version, a program's release version, and the engine
interface revision must not be substituted for one another. Plugin payloads
are opaque tooling data and cannot carry unreviewed protocol authority.
Instruction lifecycle metadata may identify private experiments as draft and
publicly deprecated instructions as deprecated, but it does not alter runtime
semantics.

The current Codama remaining-account model cannot by itself describe every
arbitrary ordered tail with independently mixed writable privileges. The
protocol must not reorder or partition capability accounts merely to satisfy a
generator limitation. Until the representation is proven lossless, generated
low-level codecs must be wrapped by an explicit high-level capability resolver
that preserves the exact requested order, duplicates, and per-account metas.

Private experiment IDLs and generated clients remain unpublished, carry no
compatibility promise, and must not be installed as canonical Program Metadata.
At an actual public release, one canonical IDL per program may be published and
its content hash must be bound by the deployment manifest.

## Capability declarations

Three different declarations must remain separate.

### Actual capability closure

The authority available to an invocation comes from the accounts and effective
privileges that actually land at Core, not from a manifest. Core must derive and
bind an ordered capability closure containing each supplied account's key,
owner, executable state, effective writable privilege, and effective signer
privilege.

Duplicate keys remain at their original positions because an engine result may
refer to a capability by index. Their effective privileges are normalized using
the privileges that Solana actually presents to the invocation. Account order
must not be sorted or deduplicated before hashing.

The client must calculate the expected closure only after composing the full
transaction. Another instruction can escalate the effective privilege of a
shared address. Core independently calculates the landing-time closure and
rejects any mismatch, alias into a protected role, undeclared signer,
unexpected writable privilege, owner substitution, or executable-state
substitution.

### Core-verified profiles

Core majors support a closed set of exact protected-capability and settlement
profiles whose authority and accounting semantics they understand. Profile
identifiers are not a generic promise that arbitrary future Solana programs are
safe. A new protected authority primitive may require an accepted decision and
a new Core major.

A public profile set should use bounded exact references rather than a global
bitset whose unused bits become accidental future authority. The concrete
encoding and limits remain open.

### Semantic engine metadata

An engine may advertise market style, quote methods, oracle use, supported
assets, UI actions, risk properties, or decoder schemas. These claims help
developers and interfaces choose an adapter. They neither sandbox CPI nor turn
opaque results into Core-verified facts.

The Core evidence model must continue to distinguish `CoreVerified` effects
from `EngineAttested` bytes and claims.

## No fixed engine-state account

Neither the market descriptor nor a future callback ABI may assume that an
engine has exactly one program-owned state account. Engine-owned state,
external program accounts, sysvars, oracles, and other composable inputs belong
to the ordered opaque capability tail unless a fixed role is proven universal
and authority-neutral.

The next private experiment should retain the selected single writable
`TRANSITION` callback while removing the fixed `engine_state` callback role. It
should use an opaque engine-instance identifier and prove stateless,
single-account, multi-account, and intentionally shared-state engines. The
phase-scoped Core callback authority may remain the sole fixed signer; it must
hold no user, custody, asset, or lasting value authority. Opaque tail accounts
must not receive signer privilege in that experiment.

The private request must bind the exact interface, Core market, engine instance,
intent, execution, participating domains, and actual capability closure. The
private result must bind the same execution and the experiment-local effect
result. The effect body, field layout, account count, payload size, receipt size,
hash encoding, and callback seeds remain private experiment choices rather than
public ABI decisions.

## Client transaction construction

A safe high-level client should:

1. select a cluster and exact Core program from authenticated deployment
   evidence;
2. load and validate the canonical market header and immutable descriptor;
3. validate the engine program, exact interface, code policy, domains, and Core
   profiles;
4. obtain opaque payload and requested account metas from an explicit engine
   adapter without giving that adapter a wallet signer;
5. compose the complete instruction list, including router and compute-budget
   instructions;
6. derive effective privileges and the expected capability closure from the
   final message;
7. bind expiry, replay protection, exact debit ceilings, minimum credits, and
   fee ceilings in the user's authorization;
8. simulate the same message and expose compute and priority-fee estimates
   separately from protocol fees;
9. present cluster, Core, market, engine, code policy, signers, writable
   accounts, asset bounds, fees, expiry, and transaction version before signing;
10. sign and submit through caller-provided wallet and RPC interfaces; and
11. confirm the requested commitment and re-read committed Core state.

Direct and routed construction must bind the same canonical intent and
execution semantics. A generated builder must not silently append instructions,
accounts, signers, hosted endpoints, or fallback versions. Quotes and decoded
engine semantics are untrusted inputs; user bounds and Core validation remain
authoritative.

Transaction format, address lookup use, and compute budgets are selected from
the pinned active runtime and measured headroom. Future runtime proposals do not
become current client requirements.

## Error contract

Public Core errors must use explicit stable numeric identities grouped by
documented categories such as wire and version, identity and ownership,
capability and privilege, user authorization and bounds, engine invocation,
settlement and accounting, fees, custody and exit, and unsupported resources.
Exact values and ranges remain open until the public ABI gate.

Once published, error identities are append-only. Reordering source declarations
must not change them; removed identities remain reserved and are never reused.
Human messages are UX, not the parse contract.

An SDK error must preserve at least the originating program ID, raw numeric
code, known symbolic name only when matched against the exact IDL, invocation
context, and an SDK-level retry hint. Engine, token-program, loader, and runtime
errors must not be flattened into a misleading Core error. Numeric custom error
codes are namespaced by program ID.

The protocol must not assume that a failed CPI can be caught and translated
like a normal application exception. Compatibility tests must prove how raw
failure origin remains observable through transaction status and invocation
evidence.

## Event contract

Canonical Core events report only facts Core can verify from committed state.
The logical evidence header must bind the event schema, market, engine program,
exact interface, canonical intent and execution, participating domains,
Core-verified effects, protocol fees, explicit opaque attestation digests, and
an appropriate post-state checkpoint. Exact bytes and transport remain open.

Engine-specific interpretation remains `EngineAttested`. An engine may emit its
own events, but an indexer or UI must not relabel them as Core-verified facts.

Ordering comes from the Solana ledger position, not a protocol-wide or
market-wide writable counter introduced solely for indexing. Indexers must
verify transaction success, Core program ID, invocation position, event
identity and version, and post-state consistency. Logs from failed transactions
are not canonical events.

Provider log truncation and the additional compute, account, frame, and depth
cost of self-CPI event transport must be measured before transport bytes are
accepted. Current-state discovery from Core accounts remains available even if
historical event infrastructure fails.

## Reference engines and conformance

One maintained reference engine may demonstrate the supported interface, but it
must not define the general contract by itself or become an allowlist.

Before a general engine interface is public, compatibility evidence must
include materially different implementations:

- a synchronous inventory or constant-product engine;
- a stored- or multi-intent clearing engine that challenges pair, immediate
  swap, single-user, and single-state assumptions; and
- after its authority decision, an engine using the external settlement-driver
  path.

At least one implementation should not share the reference engine's framework
or decoder implementation. Stateless, one-state, multi-state, shared-state,
benign, malformed, and hostile engines and routers must remain permanent
fixtures. `Reference` means maintained example and conformance target, not
approved economics, audited code, or safe liquidity.

## Public graduation gates

No engine interface may be described as public, general, stable, production,
or immutable until all applicable gates below have immutable evidence.

### Scope and semantics

- An accepted decision names the interface narrowly and records explicit
  non-goals.
- The effect algebra is product-neutral and authorization-neutral; it contains
  no accidental swap, pair, position, action-enum, or single-state requirement.
- Stored- and multi-intent compatibility has been executed.
- The external settlement-driver authority boundary has an accepted decision.
- At least two materially different engines and stateless, one-state, and
  multi-state topologies pass the same interface.

### Wire and identity

- Instruction, account, result, error, event, discriminator, hash, and PDA
  encodings are specified canonically and have reviewed golden vectors.
- Market identity changes for every authority-relevant descriptor change and
  excludes payer identity and mutable or presentational data.
- Unknown version, profile, flag, length, and capability behavior is explicit
  and fails closed where authority could change.
- Ordered capabilities, duplicate accounts, effective privilege escalation,
  alias rejection, and callback authentication are specified and executable.
- Every accepted resource maximum follows measured product and runtime evidence,
  not a private experiment constant.

### Compatibility and developer tooling

- Anchor, Codama, generator, formatter, and language toolchains are pinned.
- Reviewed program-specific IDLs use real program IDs and match source and
  artifacts.
- Rust and TypeScript low-level codecs and high-level clients pass identical
  cross-language fixtures.
- Compatibility tests cover each supported Core/interface pair, old clients,
  new clients, rejection of unsupported pairs, deprecation, and side-by-side
  deployments.
- Generated drift is rejected by CI and published package contents match the
  release manifest.
- A developer outside the implementation team integrates an engine from public
  documentation without private assistance or hosted settlement authority.

### Security and runtime

- Hostile engine, router, helper, account-alias, duplicate-meta, signer,
  writable-escalation, CPI-forwarding, return-data, callback replay,
  reentrancy, program-upgrade, and rollback tests pass.
- Decoder and resolver fuzzing covers malformed and adversarial inputs.
- Core accounting, fee, authorization, isolation, and liveness invariants are
  executable for every accepted profile.
- Exact-SBF measurements retain documented packet, account-lock, compute,
  stack, CPI-frame, and CPI-depth headroom on the pinned active runtime.
- Independent security review covers both onchain code and the client behavior
  that determines the user's signed transaction.

### Discovery, events, and release

- An independent client reconstructs current canonical markets from program
  accounts without a first-party API or registry.
- An independent indexer follows canonical events, detects gaps, and validates
  post-state checkpoints.
- Source commit, artifact hash, IDL hash, toolchain, program ID, deployment
  transaction and slot, loader state, upgrade authority, and predecessor are
  bound in an append-only authenticated deployment manifest.
- Canonical Program Metadata, if published, matches the release manifest and is
  not presented as a safety certification.
- Devnet execution, mainnet deployment, public package availability,
  immutability, migration, custody exit, and incident response are reported as
  separate evidence axes. Passing the ABI gate alone proves none of them.

## Current repository gaps

The current repository intentionally does not satisfy this contract:

- `MarketV0` is initializer-namespaced, pair-shaped, and binds exactly one
  `engine_state` account and a numeric engine revision;
- the isolated callback experiment still includes one fixed engine-state role;
- current intent, execution, receipt, effect, event, and account sizes are
  private experiment values;
- current errors depend on source declaration order;
- current events encode exact-input pair and single-engine-state semantics;
- generated Anchor IDL output is ignored and no pinned Codama normalization or
  maintained Rust/TypeScript client pipeline exists;
- canonical market header bytes, engine code policy, public error ranges,
  event transport, schemas, resource limits, migration, and release metadata
  remain undecided; and
- no independent external engine developer has validated a public contract.

The next architecture gate should close the fixed-engine-state assumption and
exercise the private Codama/client pipeline. It must not publish packages,
canonical IDL metadata, or compatibility claims. Public ABI design begins only
after that evidence and the existing stored-/multi-intent and external-driver
gates are complete.
