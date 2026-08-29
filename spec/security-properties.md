# Security properties

Status: Draft

These are candidate protocol invariants. They are requirements for design and
testing, not claims about code that does not yet exist.

## Asset safety

1. **Transaction authorization** — no asset leaves a user-controlled account
   without authorization valid for that exact transaction or an accepted future
   authorization protocol.
2. **Intent evidence** — for Core-native semantics, signed constraints bind
   exact Core, engine program, interface and code policy, market, participating
   domain descriptors, assets, recipients, amount bounds, fee ceilings, and
   expiry. For opaque semantics, the Core binds exact payload, accounts,
   capabilities, objective Core-native effects, and fees; unknown economic
   meaning remains engine-attested.
3. **Liquidity-domain isolation** — an execution cannot substitute, debit,
   close, redirect, or change Core-accounted state or rights in a
   non-participating liquidity domain. Unsolicited raw token credits remain
   possible because any holder can donate to a valid token account; they create
   no accounted liquidity, fee liability, position, or claim. Markets that
   select the same domain deliberately accept shared reserves, locks, economics,
   engine risk, and failure radius. Every participating relation among the
   immutable domain descriptor, market, engine program, interface, code policy,
   and capability profile is authorized by the domain's own local admission
   rule; a plan cannot self-declare access.
4. **Actual capability closure** — the Core validates the engine's actual ordered
   CPI accounts and selected program. It derives effective privileges and
   protected roles by public key, rejects opaque aliases into the protected
   plane, and hash-binds but does not semantically certify arbitrary external
   accounts. A manifest is not treated as a nested-call sandbox.
5. **Protected authority separation** — an engine receives no user signer,
   value-bearing Core signer, protected writable asset account, delegate, owner,
   close authority, permit PDA, or other capability accepted to move protected
   value. Engine PDA signers and pre-existing delegates are ambient authority
   and are tested as such. A future Core callback-authentication PDA is permitted
   only when scoped by Core major, selected engine, market or domain, exact plan
   digest, and callback phase, with forwarding, alias, replay, cross-engine, and
   cross-market tests. No Core instruction or `CoreVerified` asset profile may
   accept it for custody, fees, administration, upgrades, or protected-value
   movement. Arbitrary external programs can assign meaning to a forwarded
   signer; that remains opaque engine-plane risk, not Core authentication. For
   Core-native profiles and the first spike, the Core alone executes
   protected-value movement.
6. **Engine-risk honesty** — the selected engine is the economic authorization
   oracle for participating domains. Conservation and Core execution do not
   imply fair pricing. Compromise may drain those domains economically but must
   not reach non-participating domains.
7. **Core-native conservation** — every successful Core-native settlement
   balances authenticated inputs, outputs, protocol fees, and supported token
   behavior under checked arithmetic. Opaque assertions are not covered merely
   because a receipt says they balance.
8. **Protocol-fee authority** — Production Core V1 immutably derives five basis
   points with cumulative floor rounding from each exact
   `PrincipalFundedGrossDebitV1` group and binds one collector identity. Caller,
   engine, router, governance, or any other role cannot remove, zero, duplicate,
   redirect, replace, or update it. No charge exceeds the user's ceiling.
9. **Protocol-fee accounting** — only a Core-verified fee-vault credit creates
   accounted liability. Donations do not. Claims cannot exceed liability or use
   a caller-selected destination; liability reduction and transfer are atomic.
10. **Atomic protocol state** — validation, engine, transfer, fee, compute, or
    finalization failure leaves no partial account-state transition. Network fees
    and failed-transaction metadata are not described as rolled back.
11. **Cross-phase finality** — no later callback may mutate protected or
    receipt-bound state after the check on which a Core guarantee depends. A
    post-settlement engine callback, if selected, is the last account-bearing
    untrusted CPI.

## Authority and identity

12. **Canonical derivation** — market, domain, vault, and fee identities use
    domain-separated inputs including controller program and interface or
    revision context.
13. **Ownership and lifecycle** — every protocol-owned writable account is
    checked for expected address, owner, type, market or domain relation, and
    lifecycle state before use. Revival, reinitialization, closure, and type
    substitution fail closed.
14. **Permissionless admission** — any developer can create a market with an
    arbitrary executable engine that satisfies public deterministic interface,
    resource, fee, and authority rules. No API key, admin signer, platform
    allowlist, listing vote, or private registration is required. Strong
    Core-native asset profiles may remain objectively narrower.
15. **No offchain signer** — ordinary execution does not require a Programmable
    server key, keeper, API, or indexer authorization.
16. **No generic administrator** — no global authority can make arbitrary calls,
    move user or market assets, rewrite an engine, or silently expand an
    accepted Core major. Every Production Core major has no upgrade,
    configuration, fee, pause, quarantine, sweep, or migration authority.
17. **Upgrade-authority honesty** — any remaining Core upgrade authority marks a
    deployment as pre-production and it accepts no real user assets. Engine or
    external-program upgrade authorities may still defeat guarantees of domains
    that trust them; their controller, code policy, and removal state are
    explicit and never inherited as Core safety.

## Extensibility

18. **Versioned interpretation** — the same accepted interface version and bytes
    have one deterministic meaning across Core, engines, clients, and indexers.
19. **Fail-closed strong profiles** — unsupported versions, unknown behavior in
    a Core-native profile, undeclared capabilities, malformed return data, and
    ambiguous protected authority are rejected. Opaque behavior is allowed only
    under an explicitly weaker evidence and authority boundary.
20. **Resource bounds** — engine interaction has explicit account, packet,
    compute, stack, return-data, and settlement limits with measured headroom.
21. **Old-version evidence** — every accepted interface retains fixtures that
    either keep passing or prove why a new Core deployment is required.
22. **Open market semantics** — adding a curve, auction, order type, spread,
    provider model, game, or other engine-owned state machine requires no
    Programmable approval or Core product enum. A new protected authority
    primitive may require a new Core major.

## Liveness and offchain independence

23. **Onchain execution and composition** — a valid transaction can be built
    from public source and submitted through any compatible Solana RPC endpoint.
    A public Core envelope must also define a safe CPI-caller path; the current
    top-level-only Probe V0 is not that interface.
24. **Current-state discovery** — canonical markets and current protocol state
    can be discovered from versioned program accounts without a privileged
    index.
25. **Verifiable event stream** — successful Core settlements emit a versioned
    evidence header authenticated by Core invocation context and state. Global
    order comes from the ledger; shard or state digests avoid a mandatory global
    or market-wide writable counter.
26. **Archive boundary** — an independent live indexer can follow and verify
    events. Reconstructing pruned history requires an archival ledger source and
    is not a protocol-liveness guarantee.
27. **No global writable bottleneck** — ordinary execution does not depend on one
    global registry, fee accumulator, sequence, or authority account. State and
    fees are domain-local or sharded.
28. **Operator failure separation** — website, API, indexer, DNS, repository,
    company-account, and former deployment-account compromise cannot alter a
    Production Core. Only independently valid user/market transactions can
    exercise its already immutable rules.
29. **Exit-class honesty** — every persistent Core-custody domain immutably binds
    either an exact engine-independent Core claim or disclosed engine-liveness
    dependence. Every market sharing that domain inherits the same class. Only
    the former may claim an independent escape path.

## Required adversarial models

The staged test plan includes:

- a compromised engine approving an economically destructive but conservative
  plan in a participating domain;
- cross-domain vault, owner, mint, recipient, and account substitution;
- duplicate, reordered, aliased, undeclared, and shared writable accounts;
- engine PDA signers, token delegates, close authorities, and forwarded permit
  signers;
- substituted user limits, engine identity, interface, code policy, fee policy,
  recipients, and expiry;
- fee omission, redirection, double assessment, netting, dust splitting,
  rounding, donation, liability, and claim attacks;
- direct and indirect reentry, callback-signer forwarding, cross-engine and
  cross-market signer reuse, phase confusion, replay, return-data overwrite,
  and later callback mutation of earlier receipt-bound state;
- unsupported token behavior, Token-2022 extensions, and transfer-hook account
  aliases before those profiles are accepted;
- account closure, revival, reinitialization, type substitution, and stale
  references;
- compute, packet, locked-account, stack, and return-data exhaustion;
- event-shaped logs from a non-Core invocation or failed transaction;
- unavailable or changed engines with persistent Core custody; and
- proof that Production Core exposes no administrative or upgrade authority,
  plus compromise of every separately disclosed engine, token, asset, or
  pre-production authority.
