# Routed callback authentication spike results

Status: Implemented experiment; pinned Ubuntu reproduction pending

Date: 2026-08-28

## Decision

**Select Candidate A, one writable `TRANSITION` before settlement, as the
minimal callback shape for the next private architecture gate. Retain Candidate
B only as measured evidence for integrations that genuinely require a
post-settlement commit. Do not publish either experiment wire as an ABI.**

Both candidates preserve the tested authorization, capability-separation,
receipt-authentication, and atomic-rollback properties. The controlled maximum
case gives both candidates the same router, fixed accounts, eight-account
opaque closure, 128-byte payload, real v0 address lookup table, helper CPI, and
classic-SPL settlement. Candidate B then costs one additional engine frame and
14,691 additional compute units without reducing the authority exposed in that
fixture.

The selection is deliberately narrow. It chooses callback timing for the next
experiment; it does not accept a public engine interface, generic settlement
plan, product account layout, resource limit, deployment artifact, or
production custody design.

## Evidence identity

The experiment was created from repository commit
`1028824fdd22e4058a9ac97cd009283cdb838e63`, tree
`463d96bc1326a9e8a57d06e84a57088495b85936`. The complete implementation is
frozen by commit `77e944c413c2188cdc59c032f7290ad3e4e82be0`, tree
`e52552409b24f2ef10a30f53532b0990b5f67395`, in pull request
[`#7`](https://github.com/0xprogrammable/programmable-solana/pull/7). Ubuntu run
[`33196359651`](https://github.com/0xprogrammable/programmable-solana/actions/runs/33196359651)
built that source, printed all four artifact hashes, and passed every repository
and experiment test. Those hashes are now pinned. A second unchanged program
build must match the manifest before this record calls them reproduced.

## Implemented flow

The isolated workspace executes this control path:

```text
owner -> top-level Core authorization -> exact classic-SPL delegate

direct executor -----------------------> Core -> engine -> optional helper
permissionless executor -> router -----> Core -> engine -> optional helper
                                             -> exact protected settlement
```

The owner authorizes a complete canonical 642-byte intent binding. Core decodes
and rehashes that binding, verifies the owner, source, mint, token program,
expiry, timing, exact debit and limits, then installs an exact classic-SPL
delegate derived from:

```text
[b"spend:v0", user_source, intent_digest]
```

Execution receives neither the user account nor the user signer. Core
reconstructs the intent, requires that exact delegate and exact delegated
amount, and alone signs the three pinned token movements. Successful execution
fully consumes the allowance; replay fails unless the owner authorizes again.

The engine receives a separate phase capability derived from:

```text
[b"engine-callback:v0", engine_program, engine_state, market,
 domain, intent_digest, phase]
```

That callback PDA is a read-only signer only inside the matching Core-to-engine
CPI. It is never accepted as the spend authority and can affect only opaque
accounts explicitly supplied to programs that choose to trust it.

Candidate A calls one writable `TRANSITION`, authenticates its exact receipt,
then settles. Candidate B calls a fully read-only `PREPARE`, settles and reloads
the protected accounts, derives a settlement digest, then calls one writable
`COMMIT`. A late Candidate B commit failure and a Candidate A failure after
engine/helper mutation both roll the entire transaction back.

## Deterministic resource evidence

The test identities, mints, opaque accounts, transactions, and PDA inputs are
deterministic. Repeated serial runs produced the same measurements below. These
are local LiteSVM executions of the four real SBF-v0 artifacts, not host-native
program calls and not cluster fee or latency benchmarks.

| Path | Packet bytes | Locked | Writable | CU | Frames | Depth | Engine calls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| top-level authorization | 953 | 6 | 2 | 9,964 | 2 | 2 | 0 |
| direct Candidate A | 822 | 18 | 9 | 75,627 | 5 | 2 | 1 |
| routed Candidate A | 869 | 19 | 9 | 88,196 | 6 | 3 | 1 |
| direct Candidate B with helper | 898 | 20 | 10 | 99,722 | 7 | 3 | 2 |
| routed max Candidate A, v0 ALT | 522 | 27 | 10 | 113,349 | 7 | 4 | 1 |
| routed max Candidate B, v0 ALT | 522 | 27 | 10 | 128,040 | 8 | 4 | 2 |

The controlled maximum transaction is 1,261 bytes as a legacy transaction and
therefore cannot fit the 1,232-byte packet limit. Its real v0 form with a live
lookup table is 522 bytes, leaving 710 bytes of packet headroom. It remains at
27 locked accounts, 10 writable accounts, depth 4, and at most 8 executed
frames. The test ceiling is 250,000 CU, substantially below the repository's
1,400,000-CU conservative runtime baseline.

For the otherwise identical direct Candidate A path, routing adds 47 packet
bytes, one locked account, 12,569 CU, one frame, and one stack level. It adds no
writable account and no authority. In the controlled maximum comparison,
Candidate B adds 14,691 CU and one frame over Candidate A, or approximately
13.0% of Candidate A's measured compute.

The private wire sizes are:

- canonical intent binding: 642 bytes;
- execution binding: 382 bytes;
- engine request: 414 bytes;
- engine instruction including discriminator: 422 bytes;
- engine receipt: 90 bytes; and
- settlement binding: 217 bytes.

Every engine return recorded in the runtime log has the exact 90-byte receipt
length. Candidate A's later token CPIs replace the transaction's final return
data setter, which is normal Solana return-data behavior; Core authenticates the
engine setter and receipt immediately after the engine CPI. Candidate B's final
commit remains the final setter.

## Test evidence

The pinned local suite passes **61/61** tests:

- callback-capability probe: 4;
- hostile router: 4;
- Core unit tests: 12;
- exact-SBF integration tests: 19;
- private wire codec: 11; and
- routed plan engine: 11.

The 19 integration tests load and execute all four `.so` artifacts. Their
combined cases establish:

- equivalent direct and permissionless-routed intent digests, execution
  digests, economics, protected state, and engine state from cloned initial
  ledgers;
- no user account or signer in the routed execution closure;
- top-level-only canonical authorization, equivalent direct owner approval,
  wrong-owner rejection, explicit revoke, exact lower/upper delegate bounds,
  one-shot consumption, post-success replay rejection, and atomic double-call
  rollback;
- fail-closed mutation of the discriminator, instruction bytes, user terms,
  account order, omitted and added accounts, requested callback privilege, and
  fifteen fixed account roles through the permissionless router;
- phase-specific callback authentication, downstream signer forwarding, and
  rejection of direct engine calls, wrong entrypoints, forged callbacks, and
  callback reuse after Core returns;
- capability order, duplicate-position preservation, effective privilege
  normalization, fixed-role alias rejection, and protected token-account
  exclusion;
- exact receipt setter, length, version, phase, intent and execution binding,
  missing return data, trailing bytes, output bounds, and mutation rollback;
- Candidate A rollback after engine/helper mutation and Candidate B rollback
  after protected settlement and a late commit failure; and
- successful maximum Candidate A and Candidate B paths through
  `router -> Core -> engine -> helper` using a real v0 lookup table.

Wire tests separately mutate every intent and execution field, reject unknown
versions/phases and non-canonical padding, and pin all private lengths and hash
domains. Core unit tests cover fixed-role uniqueness, opaque signer and owner
constraints, protected token owners, normalized duplicate privileges, checked
fees, and disjoint spend/callback namespaces.

## Honest security boundary

The result proves a one-shot exact-delegate and phase-callback construction for
this fixed classic-SPL A-to-B control. It does not prove that Core's helper was
the historical setter of the delegate. A token owner can install the identical
delegate directly through classic SPL Token; the resulting state is
indistinguishable and carries the same owner-signed authority. Setter
provenance would require instruction introspection or persistent Core state,
which this design intentionally avoids.

The selected engine remains the economic policy authority for its participating
domain. It may mutate explicitly granted opaque state and may produce a bad but
user-permitted result. The Core boundary prevents it from acquiring protected
Core authority; it does not certify engine code, economics, tokens, interfaces,
or integrations as safe.

## Local artifacts

Built locally with Rust 1.96.0, `solana-cargo-build-sbf` 3.1.10, SBF architecture
v0, and platform-tools v1.52:

```text
8e98096abd9e2090bd091879ee70e4c6806d2c0f91f304f99e8835edb590fcd2  target/deploy/programmable_routed_callback_core.so
9a1d209535e4169b839f3a150f8734e98f7fafeb5c054ca631a3abda453db5a0  target/deploy/routed_plan_engine.so
1e94c99a5fa22bdb05d555a3aac795f6decc0e21069634f07cce34d1f6c248f9  target/deploy/hostile_router_probe.so
e228f8eca2710b8274a59331f40aeac2455da1c9107bb734ee7850adf45f347c  target/deploy/callback_capability_probe.so
```

These local hashes are not assumed to equal the canonical Ubuntu hashes.

## Pinned Ubuntu artifacts

The first successful Ubuntu 24.04 CI build produced:

```text
8bdad75b02e9fb17955a02d3a00cfa9d3c62d5939eaab895375c4e39d722449d  target/deploy/programmable_routed_callback_core.so
5a801bd093b30a82d9c82f54e2a053cc1e8761f6e494a9e12321f20ce8d5012c  target/deploy/routed_plan_engine.so
7f72a27455f29bbdbce3507fbaafcb37946ed230b2438a5b9390fa1e038ac925  target/deploy/hostile_router_probe.so
9cabf26d46bfcf4959e3eda7b8a9f648c780efcb8cc5a123055d93d8f79d1028  target/deploy/callback_capability_probe.so
```

The manifest records discovery evidence until a later CI run rebuilds the
unchanged program inputs and passes the exact checksum gate.

## Not established

This experiment does not establish:

- a public intent, engine, receipt, settlement, SDK, IDL, or account ABI;
- product resource maxima or a generic arbitrary-action interpreter;
- multi-leg, partial-fill, stored-order, auction, multi-party, NFT,
  Token-2022, compressed-asset, or custom-authority settlement;
- loader-aware engine code identity or safe engine upgrades;
- provider claims, withdrawals, fee claims, shared-liquidity admission, or an
  engine-independent exit;
- all possible failure behavior of future token or settlement profiles;
- profitable or honest engine economics;
- deployment, migration, governance, immutability, release signing, or onchain
  artifact verification;
- devnet or mainnet behavior; or
- immunity from implementation defects.

No program in this experiment may be deployed or entrusted with real funds.
