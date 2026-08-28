# Engine-generated settlement spike results

Status: Implemented experiment, not an accepted protocol interface

Date: 2026-08-28

## Evidence identity

The original experiment and every repository-controlled build input were frozen
by commit `3136903186350b4e7f1581ae6768c096202c0adc`, tree
`42100d0c638a9fe1815aba038ce88a4a27022146`, in pull request
[`#5`](https://github.com/0xprogrammable/programmable-solana/pull/5). Its first
canonical Ubuntu 24.04 build and test run was
[`33182452794`](https://github.com/0xprogrammable/programmable-solana/actions/runs/33182452794).

The combined maximum-resource source and test delta is frozen by commit
`1ede704accad0cbeae58a6446c17e4264514f037`, tree
`c67f4c64ec26be0e688726de43739bc2b7d892f7`, in pull request
[`#6`](https://github.com/0xprogrammable/programmable-solana/pull/6). Ubuntu run
[`33186883797`](https://github.com/0xprogrammable/programmable-solana/actions/runs/33186883797)
rebuilt those inputs and printed all three artifact hashes, then deliberately
failed the exact-hash gate because the manifest still contained the predecessor
engine hash. That discovery run did not reach the runtime-test step and is not
reproduction proof by itself. Follow-up commit
`d2fa376e74f5b9691fbc97940c71384b94674d8d`, tree
`4a6efae810d868b40db11da665ac5b6386b853e3`, pins the new hash and the initial
combined-resource record. Run
[`33187360324`](https://github.com/0xprogrammable/programmable-solana/actions/runs/33187360324)
rebuilt the unchanged program inputs, matched all three hashes, and passed the
full 53-test suite. That successful run is the reproduction proof. Later commits
only clarify this record and leave every program, test, and build input
unchanged.

## Decision

**Continue the engine-generated architecture investigation, but reject this
experiment's wire format as a product ABI.**

The experiment proves that one untrusted engine can derive a single output
amount and use a bounded, user-precommitted closure of opaque capabilities while
Core alone settles a fixed exact-input classic-SPL A-to-B transfer. It does not
prove a generic execution-plan interpreter, product-neutral settlement,
multi-leg actions, auctions, orders, NFTs, asynchronous intent execution, or a
production custody design.

The test program name `generated-plan-engine` is a disposable fixture role. In
this experiment, its entire "plan" is one `amount_out` scalar. It must not be
read as a public or general plan interface.

## Implemented path

The isolated nested workspace implements this exact flow:

```text
initialize one two-mint market and one exclusive liquidity domain
  -> deposit classic SPL Token B into the domain
  -> direct user submits exact Token A input and signed limits
       -> Core validates the fixed settlement plane
       -> Core derives and matches the landing-time capability hash
       -> one stateful engine CPI receives only its prefix and opaque closure
       -> Core authenticates the immediate 57-byte engine receipt
       -> Core transfers user A to vault A
       -> Core transfers the separately charged protocol fee to the fee vault
       -> Core transfers the exact generated amount of vault B to the user
       -> Core reloads token accounts and requires exact observed deltas
       -> Core updates accounting and emits the settlement event
```

The user commits to `amount_in`, maximum total input debit, minimum output,
maximum protocol fee, expiry, opaque payload, and the expected capability hash.
Core computes the experiment-only protocol fee at 30 basis points with ceiling
rounding. The reference engine computes a checked constant-product quote:

```text
effective_input = floor(amount_in * (10_000 - lp_fee_bps) / 10_000)
amount_out = floor(reserve_out * effective_input / (reserve_in + effective_input))
```

The LP fee remains implicitly in the input reserve. Core never receives or
interprets that engine-specific policy.

The disposable program identities are:

- Core: `EJKx7XFp6CZQuAHD6AC14g7nUKeczJMr2TX9XRUEjs36`
- reference engine: `EAX2oQEejkYYTxaVCbQ3pfy9bySj3WMwtV36gvf77Mj1`
- opaque helper: `EsZGEzu3NgpwumgwdsjxW3c6xB9wR6gy3qj9Y86nZ7Uv`

## Capability and damage boundary

The engine receives a fixed two-account prefix followed by zero to eight opaque
positions:

1. its state, writable and non-signer;
2. the Instructions sysvar, read-only and non-signer; and
3. only the caller-selected opaque accounts that pass Core's closure gate.

Duplicate opaque positions and their order are preserved. Effective signer and
writable privileges are normalized by public key before hashing and CPI
construction. The hash also binds every position's key, landing-time owner,
executable flag, and effective privileges. Core recomputes this closure at
execution and rejects it unless it matches the user's precommitment before the
engine runs.

Adversarial review found one medium authorization gap before the build inputs
were frozen: an earlier draft derived the capability hash only at landing, so a
user had not committed to owner or executable-state drift before signing. The
final implementation adds `expected_capability_hash`, compares it before every
untrusted CPI, and tests both a wrong value and owner drift with complete
non-mutation. No critical or high runtime finding remained in the final spike
review.

The experiment deliberately demonstrates both sides of the trust boundary:

- a selected engine can invoke the supplied helper with an engine PDA signer
  and mutate the explicitly supplied writable helper state;
- a selected engine can return a valid but economically destructive output when
  the user's own minimum remains loose, demonstrated with a 9,999-basis-point
  LP fee and an output of 9; and
- a failing helper or any later validation failure rolls all engine, helper,
  token, fee, and Core mutations back atomically.

Core rejects, before the engine CPI, every opaque signer, every fixed-role
alias, every Core-owned account, every writable executable, and every writable
classic SPL Token or Token-2022-owned account. No fixed user, mint, vault, fee,
token-program, Core state, or Core program account reaches the engine closure.

This is **arbitrary semantics over explicitly declared capabilities**, not
arbitrary authority. An existing Solana program that requires the raw wallet
signer cannot necessarily be composed unchanged through this engine path. It
would need a separate top-level instruction, scoped intent or delegation, an
engine PDA authority that it recognizes, or another independently proven
settlement driver.

## Canonical deterministic fixtures

All integers below use little-endian encoding. Public-key notation `[n; 32]`
means 32 repetitions of byte `n`.

### Capability closure

The canonical closure targets engine key `[9; 32]` and contains these ordered
descriptors:

| Position | Key | Owner | Writable | Signer | Executable |
| ---: | --- | --- | --- | --- | --- |
| 0 | `[1; 32]` | `[33; 32]` | false | false | false |
| 1 | `[2; 32]` | `[34; 32]` | true | false | false |
| 2 | `[3; 32]` | `[35; 32]` | false | false | true |

Its domain-separated capability hash is
`f4f21fd6c78324165f111cb47011af14e818914259e506ef017597dc8ad8ba06`.
The domain-separated hash of payload `stable payload` is
`be0f453ac71553f3762fb26507e4953b9eff9ad6cf1a35085eea4419d5477442`.

### Request binding

The request-binding fixture uses public keys `[1; 32]` through `[15; 32]` in
field order, values 16 through 26 for its eleven `u64` fields, capability hash
`[27; 32]`, and payload hash `[28; 32]`. Its hash is
`89d0a2390c55bb05dc93961d1b4b1aadcbcfdd3a62dade3e5e92394542122a8b`.
Every authority-bearing field is independently mutated by the codec suite and
must change this hash.

### Engine request

The canonical 293-byte request contains version 0, request hash `[0xa5; 32]`,
market `[1; 32]`, domain `[2; 32]`, engine revision 3, input 4, accounted input
5, accounted output 6, opaque count 2, capability hash `[0x5a; 32]`, and the
22-byte payload `curve:constant-product` followed by 106 zero padding bytes.
Its complete hex encoding is:

```text
00a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a501010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202030000000000000004000000000000000500000000000000060000000000000002005a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a160063757276653a636f6e7374616e742d70726f6475637400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
```

The `evaluate` instruction is exactly 301 bytes after prefixing the fixed
eight-byte discriminator `b3d38eb76c6814d6`.

### Engine receipt

The canonical receipt binds request hash `[0x5a; 32]`, output
`0x0102030405060708`, and state sequence `0x1112131415161718`. Its complete
57-byte encoding is:

```text
504d424753523030005a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a08070605040302011817161514131211
```

## Test inventory

The pinned suite passes **53/53** tests: 11 engine, 12 wire, 2 helper, 8 Core
unit, and 20 exact-SBF integration tests. Three generated `test_id` checks are
included in those package counts.

### Engine tests, 11

- generated program-ID check;
- golden quote separates the implicit LP fee;
- zero-fee quote retains no input;
- LP-fee rounding is explicitly pool-favouring;
- helper amount-prefix encoding is fixed-width and little-endian;
- hostile receipt-mode values remain stable;
- invalid and unrepresentable quotes fail;
- maximum-width products remain checked;
- deterministic boundary grid preserves CPMM and monotonicity;
- 4,096 deterministic randomized quotes match an independent integer model;
- output is monotone across successful exact inputs.

### Wire tests, 12

- identities and discriminators remain stable;
- capability hash binds target, order, position, owner, and every flag;
- duplicate positions are retained rather than deduplicated;
- capability count and payload length are bounded;
- payload hash binds length and representative first, middle, and last bytes;
- request-binding codec is exact and round-trips;
- every request-binding field changes the request hash;
- engine request is fixed, zero-padded, and round-trips;
- request decoders reject wrong lengths, version, count, payload length, and
  non-canonical padding;
- evaluate decoder rejects a wrong discriminator and trailing data;
- binding and receipt decoders reject wrong lengths, magic, version, and
  trailing data; and
- stable hash and wire vectors do not drift.

### Helper and Core unit tests, 10

- two generated program-ID checks;
- helper return data is distinctive and fixed-width;
- protocol fee rounds up and rejects zero or overflow;
- duplicate opaque positions preserve order and normalized privileges;
- signer privilege on any duplicate position is rejected;
- normalized writable privilege rejects executable and token-owned accounts;
- payload and capability-count limits fail closed;
- signer, fixed-alias, and Core-owner classes are rejected; and
- duplicate fixed roles and wrong observed token deltas are rejected.

### Exact-SBF integration tests, 20

- happy exact-input settlement uses the engine output and changes only the
  predicted writable accounts;
- the helper CPI mutates only declared state and the engine's final receipt is
  consumed;
- omitted helper capability and deliberately failing helper calls leave no
  state;
- changing only the engine LP fee changes the generated output;
- a user-bounded but destructive engine quote is explicit engine risk;
- signer, fixed alias, Core owner, executable-writable, classic-SPL-writable,
  and Token-2022-writable capability escalation all fail before the engine;
- duplicate effective privileges are normalized at eight opaque positions;
- representative first, middle, and final fixed-role substitutions plus
  required-account omissions fail before the engine;
- external read-only duplicates are accepted but order-bound;
- malformed, wrong-magic, wrong-version, trailing, wrong-hash, zero-output,
  excessive-output, and below-minimum engine results roll back the protected
  state captured by the fixture;
- absent return data is distinguished from a wrong setter;
- expiry, maximum fee, maximum total debit, and zero input fail before untrusted
  code;
- wrong and owner-drifted capability precommitments fail before the engine;
- a second execution with identical user terms binds the changed accounting
  state to a new request hash;
- insufficient user source balance fails before the engine;
- raw output-vault donation changes neither accounting nor quote;
- input- and fee-vault donations become neither quote inputs nor liabilities;
- eight opaque positions and 128 payload bytes jointly execute the reference
  CPMM and nested helper CPI, while nine and 129 fail before the engine;
- all 16 fixed-envelope keys are rejected as opaque aliases; and
- a canonical direct top-level engine invocation is rejected without mutation.

These cases include deliberate failures before Core invokes untrusted code,
inside the engine/helper plane, during receipt validation, after engine return,
and during settlement-bound checks. Each relevant failure compares the
explicitly snapshotted protected Core, engine, token, fee, and applicable helper
state byte-for-byte. This is not a claim that every arbitrary message account is
snapshotted.

## Resource record

| Fixture | Packet | Accounts | Writable | Observed compute | Max depth | Total call frames |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Direct successful settlement | 716 B | 16 | 9 | 59,807–59,838 CU | 2 | 5 |
| Nested helper CPI | 824 B | 19 | 10 | 69,751–69,783 CU | 3 | 6 |
| 8 opaque accounts + 128-byte payload + nested helper CPI | 1,109 B | 24 | 10 | 76,500 CU | 3 | 6 |
| Rejected direct engine attack | 537 B | 4 | 2 | 4,711 CU | 1 | 1 |

At the maximum tested fixture:

- packet headroom is 123 bytes against the 1,232-byte legacy packet limit;
- locked-account headroom is 40 against the 64-account baseline;
- compute headroom is 123,500 CU against this fixture's 200,000-CU execution
  ceiling; the same consumption is 1,323,500 CU below the active 1,400,000-CU
  transaction maximum, which this one-instruction fixture would need to request
  explicitly;
- the engine CPI receives 10 accounts: its fixed prefix plus eight opaque
  positions;
- the receipt is exactly 57 bytes, leaving 967 bytes against the 1,024-byte
  return-data limit; and
- the maximum fixture reaches depth 3, leaving two call levels against the
  pinned height-5 baseline.

The maximum receipt length and setter were decoded from the engine's runtime log.
Later classic-SPL CPIs replace the transaction's globally last return-data
setter, which is safe for this path because Core reads and authenticates the
engine receipt immediately after the engine CPI.

The SBF build emits no stack-overflow diagnostic and the build script fails if
one appears. The pinned compiler does not emit a trustworthy exact maximum
stack-peak measurement, so this record makes no numerical stack-headroom claim.
The measurements cover these fixtures only and cannot be extrapolated to
arbitrary future engines.

## Reproduction and artifact identity

For local source, policy, and exact-SBF runtime verification from the repository
root:

```sh
./scripts/check-repository.sh
cd experiments/engine-generated-settlement
NO_DNA=1 ./scripts/check.sh
NO_DNA=1 cargo test -p programmable-generated-settlement-core \
  --test generated_settlement --locked -- --nocapture
```

The resource table records repeated local macOS exact-SBF samples against the
local hashes below. The canonical Ubuntu CI rebuilds and hash-checks the same
program inputs and enforces the same packet, account, call-shape, and compute
ceilings; its default `cargo test` output captures rather than prints successful
fixture metrics.

The canonical hash check runs only in the pinned Ubuntu CI environment after
that environment builds the artifacts:

```sh
sha256sum --check ../../spec/engine-generated-settlement-sbf-v0.sha256
```

CI pins host Rust 1.96.0, cargo-build-sbf/Agave 3.1.10, SBPFv0, and
platform-tools v1.52. Runtime tests use LiteSVM 0.16.0 against the repository's
pinned Agave 4.2.1 semantic baseline.

The canonical Ubuntu 24.04 artifacts are:

- Core: `abaa15b87555aae6fb78f657a667a08ab1709f148c63442c569c71aa1bf776ba`
- reference engine: `6c42a3e845b1d5ce93fe9fc069d05c96e88841b3110a33125e3cf830bc4d5bfa`
- opaque helper: `5bbd777c59894b60c533abd50f99bbe3afc24c2784c9b00714e341e99071ee77`

The local macOS runtime artifacts were stable for this test pass but are
non-canonical and differ from Ubuntu:

- Core: `799ecd1be8ae43678b1f943ce1cc41d76c02fd634956e5e3d91e26b16ff5b6ef`
- reference engine: `4e2439449f3f25b6d9c542400424b5cfb4474b8fe4d188e189b5de60e093d210`
- opaque helper: `7de222853bbc764eb4108e8c988b89ca32ae5d05402844e7bb6fd872f11fea65`

Agave 3.1.10's empty legacy syscall list produces known unknown-syscall
post-processing warnings, and `cdylib` plus `lib` disables LTO for these test
crates. Exact LiteSVM execution proves that the generated experiment artifacts
run; neither warning is release or deployment evidence.

The build is environment-pinned, not hermetic: `ubuntu-24.04`, Rustup, and the
crates registry remain availability or drift dependencies. There is no signed
artifact and no independent second builder. No hash in this record proves an
onchain deployment.

## Rejected alternatives

The experiment rejects these shortcuts:

- forwarding the wallet or a Core signer into an untrusted engine;
- forwarding fixed settlement accounts as generic remaining accounts;
- trusting caller-declared privileges instead of landing-time privileges;
- allowing the caller to supply `amount_out` while calling the result
  engine-generated;
- treating a receipt as proof that opaque custom economics are correct;
- interpreting arbitrary external-program state or product types inside Core;
- building a generic arbitrary-CPI sandbox with protected asset authority;
- freezing this swap-shaped probe as a public plan ABI; and
- adding a global allowlist merely to make an unsafe capability closure appear
  permissioned.

## Maturity checkpoint

This score applies only to the disposable experiment. It is a structured code
maturity review, not an audit and not a score for a future DEX.

| Category | Rating | Score | Evidence and limiting gap |
| --- | --- | ---: | --- |
| Arithmetic | Satisfactory | 3 | Checked fee and CPMM math, explicit rounding, maximum-width cases, and a 4,096-case independent differential model; no formal proof. |
| Auditing | Weak | 1 | Typed events exist, but no production monitoring, alerting, or incident process is designed. |
| Authentication and access control | Satisfactory | 3 | User-precommitted normalized capability closure, direct-call authentication, least-privilege CPI, and hostile exact-SBF tests; production code identity and governance are absent. |
| Complexity management | Moderate | 2 | Settlement phases, codec, math, and validation are separated, but the single Core execution route remains large and only one topology is proven. |
| Decentralization | Weak | 1 | No administrator participates in settlement execution, but the fixture engine has an authority-gated test mode and no withdrawal, exit, fee claim, deployment, governance, or upgrade policy exists. |
| Documentation | Moderate | 2 | Threats, boundaries, provenance, vectors, tests, and non-proofs are explicit; there is intentionally no accepted ABI or user documentation. |
| Transaction ordering | Moderate | 2 | Minimum output, fee and debit caps, expiry, capability precommitment, and accounted-state binding exist; general MEV and asynchronous ordering are unmodeled. |
| Low-level manipulation | Satisfactory | 3 | Manual fixed-width codec, CPI construction, and immediate return-data handling are justified and tested; no unsafe Rust or assembly exists. |
| Testing and verification | Moderate | 2 | 53 deterministic unit and exact-SBF tests plus pinned CI and artifact checks; no coverage, mutation, stateful fuzzing, or formal verification. |

**Overall: 19/36, or 2.1/4.0.** The weak categories are the reason this code is
not deployable production custody, not facts to hide by increasing test count.

## Not established and next gates

This result does not establish:

- a public engine ABI, SDK, IDL, compatibility promise, or product limits;
- loader-aware engine code identity or safe upgrade behavior;
- provider claims, withdrawal, fee claiming, or an engine-independent exit;
- shared-liquidity admission or isolation between multiple engines;
- bidirectional, multi-leg, partial-fill, stored, delegated, multi-party, or
  asynchronous intent semantics;
- protected Token-2022, NFT, compressed-asset, or custom-asset settlement;
- general MEV resistance, routing, indexing, monitoring, or incident response;
- deployment, migration, immutability, governance, or release-signing policy;
- devnet or mainnet execution; or
- immunity from implementation bugs.

Before any product ABI is accepted, run paper-level counterexamples against at
least a delayed auction/order, a multi-leg action, and a custom-authority asset
flow. Before any persistent custody deployment, separately prove loader-aware
engine identity, an engine-independent exit and withdrawal path, fee claiming,
shared-domain admission, artifact signing or independent reproduction, and the
applicable property and invariant suite.

Those are separate architecture decisions and reduced experiments. They must
not be guessed into this private wire format merely to make the spike look more
complete.
