# Security properties

Status: Draft

These are candidate protocol invariants. They are requirements for design and
testing, not claims about code that does not yet exist.

## Asset safety

1. **Transaction authorization** — no asset leaves a user-controlled account
   without authorization valid for that transaction.
2. **User-intent binding** — a trade settles only within signed constraints that
   bind the core and interface version, engine, market, input asset and maximum,
   output asset and recipient, minimum output, fee ceiling, and expiry. Liquidity
   and other asset actions require equivalent action-specific bounds.
3. **Core market isolation** — an instruction for market A cannot write, debit,
   close, or redirect market B's core-owned state or core-custodied vaults.
4. **Declared engine capabilities** — every engine-owned writable account is
   declared and bound to the current instruction. Intentionally shared engine
   state requires an explicit capability and remains outside the core's
   cross-market isolation guarantee.
5. **Engine confinement** — an engine can influence only the accounts, state, and
   settlement explicitly exposed to it for the current instruction.
6. **Conservation** — every successful settlement balances inputs, outputs, fees,
   and explicitly permitted token behavior under checked arithmetic.
7. **Fee enforcement** — no supported engine path can settle a trade while
   bypassing the authenticated market fee configuration effective for that
   execution or exceeding the user's signed fee ceiling.
8. **Atomic failure** — validation failure, engine failure, exhausted compute,
   or a failed token transfer leaves no partial protocol state transition.

## Authority and identity

9. **Canonical derivation** — market and vault authorities are derived from
   domain-separated seeds that include the market identity.
10. **Explicit ownership** — every protocol-owned writable account is checked for
    the expected owner, address, market binding, and lifecycle state before use.
    Engine-owned accounts are checked against their declared capabilities.
11. **No offchain signer** — ordinary trading and liquidity actions do not require
   a Programmable server key, keeper, or API authorization.
12. **Explicit administration** — under the currently deployed code, each pause
    or configuration authority can invoke only its documented instructions. Its
    scope and every change are visible onchain and in the deployment manifest.
13. **Upgrade-authority honesty** — any remaining upgrade authority can replace
    program behavior and may therefore defeat every code-level invariant over
    program-controlled assets without a user's new signature. Its controller,
    constraints, delay, recovery, and removal path are explicit. A deployment
    with unilateral immediate upgrade power cannot claim minimized trust.

## Extensibility

14. **Versioned interpretation** — the same interface version and bytes have one
    deterministic meaning across the core, engine, clients, and indexers.
15. **Fail-closed compatibility** — unsupported versions, unknown token behavior,
    undeclared, unbound, or unauthorized writable accounts, malformed return
    data, and ambiguous state are rejected.
16. **Resource bounds** — engine interaction has explicit account, compute,
    recursion, return-data, and settlement limits that can be tested.
17. **Old-version evidence** — every published interface version retains fixtures
    that either keep passing or prove why a new core deployment is required.

## Liveness and offchain independence

18. **Onchain execution** — a valid transaction can be built from public source
    and submitted through any compatible Solana RPC endpoint.
19. **Current-state discovery** — canonical markets and current protocol state can
    be discovered from versioned program accounts without a privileged index.
20. **Verifiable event stream** — successful core settlements emit a versioned
    common envelope with a detectable market-local sequence or checkpoint.
    Global ordering uses the Solana ledger position instead of protocol-wide
    mutable state. Engine-specific semantics remain optional schemas rather than
    implied core knowledge.
21. **Archive boundary** — an independent live indexer can follow and verify the
    event stream. Reconstructing already-pruned history requires an archival
    ledger source and is not a protocol-liveness guarantee.
22. **No global writable bottleneck** — ordinary market operations do not depend
    on one globally writable registry, fee accumulator, or authority account.
    State and fee writes are market-local or safely sharded so one market cannot
    serialize or halt unrelated markets.
23. **Failure separation** — website, API, indexer, DNS, and company-account
    compromise cannot alter onchain state without an independently valid Solana
    transaction.

## Required adversarial models

The test plan must include at least:

- a malicious engine returning crafted settlement data;
- cross-market vault and account substitution;
- duplicate, reordered, aliased, undeclared, and shared writable accounts;
- substituted user intent, recipients, fee bounds, and expired transactions;
- fee rounding, zero amounts, maximum values, and arithmetic boundaries;
- re-entry and nested cross-program invocation attempts;
- unsupported token extensions and transfer behavior;
- stale interface versions and corrupted account discriminators;
- compute exhaustion, oversized return data, and global hot-account contention;
- missing event checkpoints and pruned history; and
- compromise of each declared administrative authority.
