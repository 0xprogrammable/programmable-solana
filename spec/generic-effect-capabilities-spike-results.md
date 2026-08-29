# Generic effect capabilities spike: signed-source result

## Disposition

The source snapshot is preserved as **private historical falsification evidence**. It is not a
public Engine ABI, a Protocol binding, a release candidate, a deployable production Core, or
evidence of conformance to Programmable Protocol V1.

The exact-source build and all tests passed, but the security review found six High findings that
falsify promotion of this design. Passing tests establish only that the frozen experiment behaves
as its private test suite specifies. They do not resolve the findings below.

## Source identity

- Source commit: `efda6d382eefead0423baf9cc25fca7d0627b13a`
- Source tree: `dc394c0d5efc50ae7f080dc10ce7a6cc0c978cb8`
- Subject: `Add generic effect capability experiment`
- Author and committer: `Hazar <258789013+hazarxyz@users.noreply.github.com>`
- Commit verification: `git verify-commit` returned a good Git SSH signature for that identity,
  using ED25519 key fingerprint
  `SHA256:RTXVJ3XspKUc+Qmj/daOWwU2WyT+qbRBtsJJwNpItdI`.
- The detached rebuild worktree was clean before the gate ran.

## Exact detached rebuild

The verification run took place on 2026-08-29 in a newly created detached worktree at
`/tmp/programmable-solana-spike.mAfSQ5/source`. The source checkout was created and verified with:

```sh
git worktree add --detach /tmp/programmable-solana-spike.mAfSQ5/source \
  efda6d382eefead0423baf9cc25fca7d0627b13a
cd /tmp/programmable-solana-spike.mAfSQ5/source
git status --short
git rev-parse HEAD
git rev-parse 'HEAD^{tree}'
git verify-commit HEAD
```

Status: passed. `git status --short` produced no output; `HEAD`, tree, and signature matched the
identities above.

The full experiment gate was then run from
`experiments/generic-effect-capabilities`:

```sh
NO_DNA=1 ./scripts/check.sh
```

Status: passed with exit status 0. The script ran, in order:

```text
cargo fmt --all --check
cargo metadata --locked --no-deps --format-version 1
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/build-sbf.sh
cargo test --workspace --all-targets --locked
```

The gate built five SBF programs and ran 215 tests: 124 library unit tests, 13 frozen-vector tests,
5 property tests, 1 security-row field-matrix test, and 72 exact-SBF/LiteSVM fixture integration
tests. Result: 215 passed, 0 failed, 0 ignored.

The SBF post-processing phase warned that the active platform-tools `syscalls.txt` was empty and
reported otherwise unknown syscalls. The checked script did not ignore that condition: it selected
the repository's pinned Agave 3.1.10 syscall list,
`scripts/sbpfv0-syscalls-agave-v3.1.10.txt`, and its explicit syscall gate passed. The build also
emitted the existing `cdylib` plus `lib` warning that this crate-type combination precludes LTO.
Neither warning changes the non-release disposition.

### Toolchain observed

| Component | Exact observed value |
| --- | --- |
| Host | macOS 26.5.2, arm64 |
| Git | 2.50.1 (Apple Git-155) |
| Rust host compiler | rustc 1.96.0 (`ac68faa20`, 2026-05-25) |
| Cargo | 1.96.0 (`30a34c682`, 2026-05-25) |
| Solana CLI | 3.1.10 (`src:7bc9c805`, Agave) |
| `cargo build-sbf` | 3.1.10 |
| SBF platform tools | 1.52, rustc 1.89.0 |
| Anchor CLI | 1.1.2 |
| Node.js | v24.14.0 |
| npm | 11.16.0 |
| pnpm | 11.24.0 |

## Exact clean-build SBF artifacts

The checksum manifest is
[`generic-effect-capabilities-sbf-v0.sha256`](generic-effect-capabilities-sbf-v0.sha256).

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `programmable_generic_effect_core.so` | 1,260,544 | `0bb50c3ef8c5269728aeb0c7b4f0207c7367cb92b379f7b7f8213312d82bd882` |
| `generic_effect_engine_probe.so` | 181,336 | `a5f6ca60a737ec20d011bc000f45eb0c00d8d4717d28e31e8bfb59bcac1f45c9` |
| `replacement_effect_engine_probe.so` | 135,152 | `e3885945bab3a9699b30201501164fbe2d8a7f7243e652b8ab190d97d188d494` |
| `hostile_router_probe.so` | 140,088 | `b82d03c33420a7c8913c5247ba5d0ac6e6b954504542aaf1dacc38475b3f68b5` |
| `callback_capability_probe.so` | 161,312 | `693fecf8af9e79f494366611c79624ade1f4c52e8dd35fe28de42021f0e65589` |

The manifest was checked from the root of the detached exact-source worktree with:

```sh
shasum -a 256 -c \
  /Users/hazar/Documents/Codex/2026-08-27/wei/programmable-solana/spec/generic-effect-capabilities-sbf-v0.sha256
```

All five paths returned `OK`.

## Security falsification findings

### High 1: the assessment policy is variable, not immutable five basis points

`FeePolicyCandidateV0` stores caller-selected rounding mode, rate, and denominator, and execution
constructs `RatePolicy` from those values. The tests exercise 30 and 37 basis-point policies and
both floor and ceiling rounding. This does not implement immutable
`A(B) = floor(B / 2,000)`, a fixed Constitution, or a fixed Collector.

Required correction: make the canonical assessment formula, cumulative-floor rule, Constitution,
and Collector immutable; remove every rate, rounding, exemption, and redirect setter from the
production Core path.

Evidence: `programs/generic-effect-core/src/state.rs:609`,
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:1975`, and
`programs/generic-effect-core/src/fees.rs:374`, all relative to the experiment root.

### High 2: Domain accounting identity does not bind the custody vault

The Domain accounting PDA is derived only from the Domain descriptor, and each accounting asset
slot stores asset identity, asset program, profile digest, and amount but no exact vault key.
Execution can therefore accept another Classic SPL token account owned by the public accounting
PDA for the same slot. Debiting that alternate account reduces the shared accounting ledger while
leaving the intended vault funded, stranding custody and breaking vault-to-ledger isolation.

Required correction: derive and validate custody by at least
`(Core Deployment, Domain Revision, Asset Profile, native Asset)`, bind the exact vault to that
identity, and reject every alternate account even when its token owner is the same PDA.

Evidence: `programs/generic-effect-core/src/state.rs:362`,
`programs/generic-effect-core/src/state.rs:389`, and
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:2363`.

### High 3: authorization, replay, fee, and evidence domains omit portable identity

Inline and stored identities bind the experimental major, Core program, actor, nonce, market,
loader snapshot, and fee policy, but they do not bind the immutable Constitution ID, native
binding version, or chain/genesis identity. Fee grouping is reduced to actor, intent, mutable
policy, asset, class, and revision instead of the Protocol Assessment V1 group:
`(Core Deployment, Constitution, Authorization Scope, Assessment Principal, Asset Profile,
native Asset)`.

Required correction: include the exact Constitution ID, binding version, and genesis/chain domain
in every authorization, replay, assessment, receipt, and evidence preimage, and use the exact
six-part assessment group.

Evidence: `programs/generic-effect-core/src/authorization.rs:37`,
`programs/generic-effect-core/src/authorization.rs:589`, and
`programs/generic-effect-core/src/fees.rs:190`.

### High 4: direct one-shot authorization is replayable and loses cumulative fee state

An inline direct identity requires `max_fills = 1`, but only stored authorization accounts receive
a durable pre-CPI reservation, post-execution state update, and consumed tombstone. A direct actor
can submit the same signed nonce and intent again in another transaction, and each replay starts
fee accumulation at zero. This breaks one-shot replay protection and split/merge fee invariance.

Required correction: create an uncloseable Core-owned Scope/nonce or intent tombstone for every
direct one-shot path before the untrusted CPI, and persist assessment state by Authorization Scope.

Evidence: `programs/generic-effect-core/src/authorization.rs:47`,
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:1404`, and
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:2147`.

### High 5: fee liabilities are not protected by canonical custody

The fee descriptor authenticates the vault key and execution checks the observed net credit, but
the Core does not require the vault to be a canonical Core-controlled token account with no
delegate or effective close authority. The reference fixture creates the fee vault with the test
payer as token owner. That external owner can drain or close it after liability is recorded. The
liability ledger also is not reconciled against spendable vault balance, and no Core-only claim
state machine proves observed debit.

Required correction: use a canonical Core PDA-owned fee vault with no delegate and an explicit
non-closeable policy, require spendable balance to cover liability, and implement claims as
Core-authorized observed debits with assessed, funded, claimable, claimed, externally withheld,
and offchain-valued amounts kept separate.

Evidence: `programs/generic-effect-core/src/state.rs:667`,
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:2580`, and
`programs/generic-effect-core/tests/common/direct.rs:269`.

### High 6: gross-debit assessment has no origin-bound refund model

Every intent-funded debit contributes `plan.gross_debits[index]` unconditionally to the assessment
basis. The cumulative assessment path has no debit-occurrence identifier, refund origin, or refund
type. A same-fill return to a Principal therefore cannot be distinguished from an unrelated
credit, and refunded amounts remain assessed as gross debit without a defined Protocol V1 rule.

Required correction: derive a Core-owned debit occurrence ID for every protected debit and subtract
a refund only when it occurs in the same fill, names that exact origin, matches asset and profile,
and does not exceed the unused amount of the originating debit.

Evidence: `programs/generic-effect-core/src/instructions/execute_effect_full.rs:1915` and
`programs/generic-effect-core/src/fees.rs:216`.

### Medium 7: Engine loader identity is not revalidated after CPI

The complete Engine loader/revision closure is authenticated before the Engine CPI, but after CPI
the code proceeds from protected-account checks directly to receipt decoding without rebuilding
and comparing the validated identity.

Required correction: after the CPI, reload the Engine Program and canonical ProgramData/release
closure, repeat the full effective-privilege and identity validation, and require an exact match
before accepting return data.

Evidence: `programs/generic-effect-core/src/instructions/execute_effect_full.rs:325` and
`programs/generic-effect-core/src/instructions/execute_effect_full.rs:1430`.

### Low 8: loader-v3 Program parsing accepts trailing bytes

`parse_loader_v3_program` accepts every account with length at least 36 and decodes only the first
36 bytes. A canonical loader-v3 Program account has exactly 36 bytes.

Required correction: require exact length 36 before decoding the tag and ProgramData address.

Evidence: `programs/generic-effect-core/src/engine_identity.rs:598`.

## Official Solana program autofixer evidence

The official Solana MCP program autofixer was run read-only, once per Rust program file, over the
frozen Core modules, state/support files, and instruction files. It reported no Critical or High
finding and requested no further fix pass. No source file changed.

The following Medium fingerprints were dismissed only after source verification and then rerun to
`require_another_tool_call_after_fixing: false`:

- `unchecked-arithmetic:b7e8be710b6f` at `execute_effect_full.rs:529:29`
- `unchecked-arithmetic:4ff7a625c26a` at `execute_effect_full.rs:785:13` and `:787:13`
- `unchecked-arithmetic:d3d454398c8f` at `execution_preflight.rs:134:24`

Those four occurrences compute account-array positions from segment starts and bounded `u8`
offsets. `AccountSegments::parse` and the envelope count/index checks validate the ranges. They are
count/index arithmetic, not token or balance arithmetic.

The seven verified fixed-layout/count/index findings in `state.rs` were:

- `unchecked-arithmetic:7f07332a9bbf`
- `unchecked-arithmetic:213bc7296f0d`
- `unchecked-arithmetic:7ca57f22e6c7`
- `unchecked-arithmetic:e2ab8dfb5c3d`
- `unchecked-arithmetic:1a27949467a9`
- `unchecked-arithmetic:b439ea940d35`
- `unchecked-arithmetic:94c054a41f9d`

The first five are compile-time fixed storage-layout arithmetic using the 12-row limits. The last
two are fee-row offsets bounded respectively by a `0..MAX_STORED_FEE_STATES` loop and an immediate
`index < MAX_STORED_FEE_STATES` guard. Their maximum row end is the exact allocated account end.
All seven were dismissed with those source facts and rerun cleanly.

One additional Medium advisory, `unchecked-arithmetic:c59d0d4a98fa` at `fees.rs:463:17`, was not
dismissed. It is a test-only `cumulative_fee += delta`: the fixed test basis and 37/10,000 rate
bound the total to 258, while production fee arithmetic uses checked operations. The tool did not
make it gating or request another pass.

This static result is supporting evidence only. It does not supersede the six High design findings,
the Medium post-CPI identity finding, the Low loader-length finding, independent review, runtime
conformance, or release gates.

## Release boundary

No artifact in this record was signed for deployment, published as a client/interface package, or
sent to any cluster. The checksums identify local clean-build outputs from the exact signed source
commit only. Promotion remains rejected until the canonical implementation fixes all findings and
passes its own Protocol lock, public ABI, hostile-runtime, reproducibility, independent-security,
and owner-controlled deployment gates.
