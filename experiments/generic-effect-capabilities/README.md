# Generic effect capabilities experiment

> [!CAUTION]
> **DISPOSABLE LOCAL EXPERIMENT. PRIVATE WIRE. NO PUBLIC ABI, SDK, IDL, OR DEPLOYMENT.**

This isolated workspace tests the capability-indexed Move hypothesis frozen in
[`../../spec/generic-effect-capabilities-spike.md`](../../spec/generic-effect-capabilities-spike.md).
The reviewed specification revision is commit
`c1150a0c6be896e8599ff98f85ecba8aa7fe0e22` with tree
`3fb719a3fa5a1ea225d546ce8a3dd848c11746a2`. It is not a source revision for
this implementation. The experiment remains unbound implementation work until
its result record names a later exact source commit and tree.
It asks whether permissionless engines can mutate only an authenticated opaque
account tail and return a product-neutral graph of protected classic-SPL moves
that Core validates, fees, observes, and accounts atomically.

This is not a DEX release or maintained engine interface. All package names,
program IDs, discriminators, seeds, account layouts, codecs, limits, and test
fixtures are disposable. Nothing here is approved for devnet, mainnet, funded
custody, or downstream compatibility.

## Isolation boundary

This directory has its own Cargo and Anchor workspaces, lockfile, Rust toolchain,
and build output. It is not a member of the repository-root workspace. It must
not depend on canonical Core or public engine-interface crates, publish crates,
retain a deployment keypair, emit a maintained IDL, or load predecessor build
artifacts. See [`PROVENANCE.md`](PROVENANCE.md).

## Components

- `generic-effect-private-wire`: exact experiment-local codecs and hashes;
- `programmable-generic-effect-core`: disposable protected-plane validator;
- `generic-effect-engine-probe`: configurable zero/one/many-state engine;
- `replacement-effect-engine-probe`: different SBF code with the same declared
  engine program ID for loader-policy tests;
- `hostile-router-probe`: permissionless routed-entry and mutation fixture;
- `callback-capability-probe`: opaque helper and callback-forwarding fixture.

The Core-to-engine callee prefix is one read-only callback signer followed only
by the ordered opaque tail. Protected settlement, authorization, admission,
fee, loader, and accounting accounts remain outside that callee capability set.

The outer Core prefix has six accounts: configuration, market, fee policy,
selected engine program, callback PDA, and the Instructions sysvar. Dynamic
segments start after that prefix. The private envelope row order is domain
controls, one 8-byte authorization snapshot per intent, zero to four 80-byte
inline immutable identities, fee shards, 48-byte settlement capabilities, then
payload. The independent all-axis encoding ceiling is 1,424 bytes and is an
expected packet-failure boundary, not an accepted transaction claim. Its
`authorization_snapshot_row_count` equals `intent_count`; Core derives the
distinct engine `context_row_count` from validated capabilities.

The engine request is a separate exact encoding: its fixed prefix is followed
by 100-byte asset rows, 112-byte domain rows, 120-byte resolved-intent rows, one
32-byte objective fee-policy row, 88-byte settlement-context rows, and payload.
Its independent maximum is 3,744 bytes. No packet-acceptance statement is made
until the integration fixture serializes the complete versioned transaction.

Mutable fill sequence and witness placement are not part of immutable intent
identity. The protected execution root binds witness-neutral authorization
views together with exact protected endpoints, capabilities, and fee facts.
Only objective gross-debit rate fees exist in this experiment; the earlier
fixed-envelope fee candidate is disabled.

The private Wire crate owns the exact 16-byte stored-authorization header,
control arguments, row codecs, and every security hash preimage. Core alone
owns the account discriminator and exact 4,784-byte stored-account
serialization, including its 312-byte identity and 4,776-byte payload. Core
tests freeze offsets, total length, round trips, and trailing-byte rejection;
there is intentionally no second mutable-state serializer in Wire.

Solana message compilation unions signer and writable privileges by public key.
Accordingly, duplicate account positions preserve their order and key but expose
the same effective privileges; the Instructions sysvar cannot recover each
position's pre-compilation flags. A transaction-root initializer may use one
wallet as both payer and actor only when both duplicate positions authenticate
as that same effective signer-writable key. The program-actor path rejects that
alias, and no result claims that an original read-only actor meta was observed.

Loader-v3 evidence is intentionally narrow. The immutable class relies on the
runtime and loader invariant that ProgramData with no authority has no later
code-mutation route; its captured ProgramData slot is observation and release
evidence, not a cryptographic ELF identity or finalized-deployment claim. The
mutable-controller class explicitly trusts its visible controller and its
liveness. A nominal pinned-mutable class exists only as a rejected hostile
fixture because permissionless program extension can deny liveness. Clients and
indexers must separately wait for finalized-fork evidence before presenting any
public artifact relation.

The full execute path currently uses a production-source forward bump allocator
because Solana's standard entrypoint allocator remains capped at the default
32 KiB even when the runtime maps a larger requested heap. Every execute
transaction must therefore contain exactly one canonical transaction-level
`RequestHeapFrame` for the provisional 208 KiB controlled frame; instruction
order does not matter because Compute Budget preprocessing is transaction-wide.
The accepted-path fixtures also request the 1,120,000-CU private acceptance
ceiling. Control instructions that remain below the default frame need neither
request. The 208 KiB frame is measured private headroom, not a supported or
accepted product bound; the declared maximum exact-SBF matrix must still measure and
justify a smaller final bound or falsify this candidate.

## Checks

Run host checks without producing SBF artifacts:

```sh
SKIP_SBF=1 ./scripts/check.sh
```

Run the complete local gate, including all five SBF artifacts and exact-SBF
integration tests:

```sh
./scripts/check.sh
```

Generated local artifacts stay under `target/deploy`. The scripts reject any
retained deployment keypair and contain no cluster or deploy operation.
