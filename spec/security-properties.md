# Security properties

Status: Draft

These are candidate protocol invariants. They are requirements for design and
testing, not claims about code that does not yet exist.

## Asset safety

1. **Transaction authorization** — no asset leaves a user-controlled account
   without authorization valid for that transaction.
2. **Market isolation** — an instruction for market A cannot write, debit, close,
   or redirect accounts belonging to market B.
3. **Engine confinement** — an engine can influence only the state and settlement
   explicitly exposed to it for the current market and instruction.
4. **Conservation** — every successful settlement balances inputs, outputs, fees,
   and explicitly permitted token behavior under checked arithmetic.
5. **Fee enforcement** — no supported engine path can settle a trade while
   bypassing the protocol fee required by that market's immutable transaction
   inputs.
6. **Atomic failure** — validation failure, engine failure, exhausted compute,
   or a failed token transfer leaves no partial protocol state transition.

## Authority and identity

7. **Canonical derivation** — market and vault authorities are derived from
   domain-separated seeds that include the market identity.
8. **Explicit ownership** — every writable account is checked for the expected
   owner, address, market binding, and lifecycle state before use.
9. **No offchain signer** — ordinary trading and liquidity actions do not require
   a Programmable server key, keeper, or API authorization.
10. **Bounded administration** — any upgrade, pause, or configuration authority
    is explicit in the deployment manifest and cannot directly sign user
    transactions. Its actual ability to change program behavior must never be
    hidden.

## Extensibility

11. **Versioned interpretation** — the same interface version and bytes have one
    deterministic meaning across the core, engine, clients, and indexers.
12. **Fail-closed compatibility** — unsupported versions, unknown token behavior,
    extra writable accounts, malformed return data, and ambiguous state are
    rejected.
13. **Resource bounds** — engine interaction has explicit account, compute,
    recursion, return-data, and settlement limits that can be tested.
14. **Old-version evidence** — every published interface version retains fixtures
    that either keep passing or prove why a new core deployment is required.

## Liveness and offchain independence

15. **Onchain execution** — a valid transaction can be built from public source
    and submitted through any compatible Solana RPC endpoint.
16. **Rebuildable indexing** — all canonical market discovery and trade history
    can be reconstructed from versioned onchain accounts and events.
17. **Failure separation** — website, API, indexer, DNS, and company-account
    compromise cannot alter onchain state without an independently valid Solana
    transaction.

Property 17 does not make an upgradeable program immune to compromise of its
upgrade authority. The deployment model must state whether that systemic power
exists, who controls it, and how it can be removed. Immutability and upgrade
safety are a later deployment decision, not a documentation shortcut.

## Required adversarial models

The test plan must include at least:

- a malicious engine returning crafted settlement data;
- cross-market vault and account substitution;
- duplicate, reordered, aliased, and unexpected writable accounts;
- fee rounding, zero amounts, maximum values, and arithmetic boundaries;
- re-entry and nested cross-program invocation attempts;
- unsupported token extensions and transfer behavior;
- stale interface versions and corrupted account discriminators;
- compute exhaustion and oversized return data; and
- compromise of each declared administrative authority.
