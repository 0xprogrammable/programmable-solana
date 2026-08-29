#![allow(dead_code)]

mod direct;
mod domains;
mod evidence;
mod reference;
mod resources;

#[allow(unused_imports)]
pub use direct::*;
#[allow(unused_imports)]
pub use domains::*;
#[allow(unused_imports)]
pub use evidence::*;
#[allow(unused_imports)]
pub use reference::*;
#[allow(unused_imports)]
pub use resources::*;

use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anchor_lang::{prelude::Pubkey, AccountDeserialize, AccountSerialize};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use litesvm_cpi_tree::{CpiFrame, CpiTreeExt};
use litesvm_token::{
    get_spl_account,
    spl_token::state::{Account as SplTokenAccount, Mint as SplMint},
    CreateAssociatedTokenAccount, MintTo,
};
use solana_account::Account;
use solana_address_lookup_table_interface::instruction::{
    create_lookup_table, extend_lookup_table,
};
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_loader_v3_interface::{
    get_program_data_address, instruction as loader_v3_instruction, state::UpgradeableLoaderState,
};
use solana_message::{
    v0::Message as MessageV0, AccountMeta, AddressLookupTableAccount, Hash, Instruction, Message,
    VersionedMessage,
};
use solana_program_pack::Pack;
use solana_signer::Signer;
use solana_slot_hashes::SlotHashes;
use solana_transaction::{versioned::VersionedTransaction, Transaction};

pub const SBF_ARTIFACT_NAMES: [&str; 5] = [
    "callback_capability_probe.so",
    "generic_effect_engine_probe.so",
    "hostile_router_probe.so",
    "programmable_generic_effect_core.so",
    "replacement_effect_engine_probe.so",
];

pub const LOADER_V3_PROGRAM_ACCOUNT_LEN: usize = 36;
pub const LOADER_V3_PROGRAM_DATA_METADATA_LEN: usize = 45;
pub const LOADER_WRITE_CHUNK_BYTES: usize = 512;
pub const LOADER_EXTEND_BYTES: u32 = 10_240;
/// Pinned LiteSVM/Agave SIMD-0186 account-data meter constants.
pub const TRANSACTION_ACCOUNT_BASE_SIZE: usize = 64;
pub const ADDRESS_LOOKUP_TABLE_BASE_SIZE: usize = 8_248;

pub const PACKET_ACCEPTANCE_CEILING: usize = 985;
pub const UNIQUE_LOCK_ACCEPTANCE_CEILING: usize = 51;
pub const CPI_ACCOUNT_POSITION_ACCEPTANCE_CEILING: usize = 204;
pub const COMPUTE_ACCEPTANCE_CEILING: u64 = 1_120_000;
pub const STACK_HEIGHT_ACCEPTANCE_CEILING: u8 = 4;
pub const INSTRUCTION_TRACE_ACCEPTANCE_CEILING: usize = 51;
pub const RETURN_DATA_ACCEPTANCE_CEILING: usize = 819;
pub const INSTRUCTION_DATA_ACCEPTANCE_CEILING: usize = 8_192;
pub const LOADED_ACCOUNT_DATA_ACCEPTANCE_CEILING: usize = 53_687_091;
pub const CONTROLLED_HEAP_FRAME_BYTES: u32 =
    programmable_generic_effect_core::heap::CONTROLLED_HEAP_FRAME_BYTES;
pub const CONTROLLED_COMPUTE_UNIT_LIMIT: u32 = 1_120_000;

/// Test-only runtime override. The artifacts are deliberately built as SBPFv0
/// so the real loader-v3 deployment tests must turn off this exact feature.
/// This is not a statement about a production feature set.
pub const SBPF_V0_DEPLOYMENT_OVERRIDE_LABEL: &str =
    "test-only: deactivate disable_sbpf_v0_v1_v2_deployment for SBPFv0 loader fixtures";

#[derive(Clone, Debug)]
pub struct SbfArtifacts {
    pub core: Vec<u8>,
    pub engine: Vec<u8>,
    pub replacement_engine: Vec<u8>,
    pub router: Vec<u8>,
    pub helper: Vec<u8>,
}

impl SbfArtifacts {
    /// Read the disposable SBF closure and reject any missing or unexpected
    /// deploy entry. In particular, a retained keypair cannot be ignored.
    pub fn load_exact() -> Result<Self, String> {
        let directory = artifact_directory();
        let expected = SBF_ARTIFACT_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<BTreeSet<_>>();
        let mut observed = BTreeSet::new();
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "cannot inspect an entry in {}: {error}",
                    directory.display()
                )
            })?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "target/deploy contains a non-UTF8 entry".to_owned())?;
            observed.insert(name);
        }
        if observed != expected {
            return Err(format!(
                "target/deploy must contain exactly the five disposable SBF artifacts; expected {expected:?}, observed {observed:?}"
            ));
        }

        let read = |name: &str| -> Result<Vec<u8>, String> {
            let path = directory.join(name);
            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            if !bytes.starts_with(b"\x7fELF") {
                return Err(format!("{} is not an ELF artifact", path.display()));
            }
            Ok(bytes)
        };

        Ok(Self {
            core: read("programmable_generic_effect_core.so")?,
            engine: read("generic_effect_engine_probe.so")?,
            replacement_engine: read("replacement_effect_engine_probe.so")?,
            router: read("hostile_router_probe.so")?,
            helper: read("callback_capability_probe.so")?,
        })
    }

    /// Cache the four distinct program identities. The fifth ELF is retained
    /// separately because it intentionally declares the engine's exact ID and
    /// is installed only through a real loader-v3 Upgrade instruction.
    pub fn install_cached_programs(&self, svm: &mut LiteSVM) {
        assert_eq!(effect_engine_probe::ID, replacement_effect_engine_probe::ID);
        svm.add_program(programmable_generic_effect_core::ID, &self.core)
            .expect("load disposable Core SBF");
        svm.add_program(effect_engine_probe::ID, &self.engine)
            .expect("load primary engine SBF");
        svm.add_program(hostile_router_probe::ID, &self.router)
            .expect("load hostile router SBF");
        svm.add_program(callback_capability_probe::ID, &self.helper)
            .expect("load callback helper SBF");
    }
}

pub fn artifact_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy")
}

pub fn fixture_keypair(tag: u8) -> Keypair {
    // Public, valueless local-test identities only.
    Keypair::new_from_array([tag; Keypair::SECRET_KEY_LENGTH])
}

/// Canonical pinned Solana compute-budget RequestHeapFrame encoding. The
/// direct dependency surface intentionally stays unchanged in this isolated
/// experiment, so the exact five-byte interface encoding is frozen here.
pub fn request_heap_frame_instruction(bytes: u32) -> Instruction {
    assert!(bytes >= 32 * 1_024, "heap frame is below Solana's minimum");
    assert!(bytes <= 256 * 1_024, "heap frame exceeds Solana's maximum");
    assert_eq!(bytes % 1_024, 0, "heap frame must be KiB aligned");
    let mut data = Vec::with_capacity(5);
    data.push(1);
    data.extend_from_slice(&bytes.to_le_bytes());
    Instruction {
        program_id: solana_sdk_ids::compute_budget::id(),
        accounts: vec![],
        data,
    }
}

/// Canonical pinned Solana compute-budget SetComputeUnitLimit encoding. The
/// exact-SBF happy path must not silently depend on the runtime's approximately
/// 200k-CU per-instruction default.
pub fn set_compute_unit_limit_instruction(units: u32) -> Instruction {
    assert!(units != 0, "compute-unit limit must be nonzero");
    assert!(
        units <= 1_400_000,
        "compute-unit limit exceeds Solana's maximum"
    );
    let mut data = Vec::with_capacity(5);
    data.push(2);
    data.extend_from_slice(&units.to_le_bytes());
    Instruction {
        program_id: solana_sdk_ids::compute_budget::id(),
        accounts: vec![],
        data,
    }
}

pub fn decode_set_compute_unit_limit(instruction: &Instruction) -> Option<u32> {
    if instruction.program_id != solana_sdk_ids::compute_budget::id()
        || !instruction.accounts.is_empty()
        || instruction.data.len() != 5
        || instruction.data[0] != 2
    {
        return None;
    }
    Some(u32::from_le_bytes(
        instruction.data[1..5]
            .try_into()
            .expect("compute-unit instruction has four value bytes"),
    ))
}

pub fn decode_requested_heap_frame(instruction: &Instruction) -> Option<u32> {
    if instruction.program_id != solana_sdk_ids::compute_budget::id()
        || !instruction.accounts.is_empty()
        || instruction.data.len() != 5
        || instruction.data[0] != 1
    {
        return None;
    }
    Some(u32::from_le_bytes(
        instruction.data[1..5]
            .try_into()
            .expect("heap-frame instruction has four value bytes"),
    ))
}

pub fn install_raw_account(
    svm: &mut LiteSVM,
    address: Pubkey,
    owner: Pubkey,
    data: Vec<u8>,
    executable: bool,
) {
    assert!(
        svm.get_account(&address).is_none(),
        "fixture account {address} already exists"
    );
    svm.set_account(
        address,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(data.len()),
            data,
            owner,
            executable,
            rent_epoch: 0,
        },
    )
    .unwrap_or_else(|error| panic!("install fixture account {address}: {error}"));
}

pub fn install_anchor_account<T: AccountSerialize>(
    svm: &mut LiteSVM,
    address: Pubkey,
    owner: Pubkey,
    state: &T,
    exact_space: usize,
) {
    let mut data = Vec::with_capacity(exact_space);
    state
        .try_serialize(&mut data)
        .expect("serialize fixture Anchor account");
    assert!(
        data.len() <= exact_space,
        "serialized Anchor fixture exceeds declared account space"
    );
    data.resize(exact_space, 0);
    install_raw_account(svm, address, owner, data, false);
}

pub fn read_anchor_account<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm
        .get_account(address)
        .unwrap_or_else(|| panic!("fixture Anchor account {address} is absent"));
    let mut data = account.data.as_slice();
    T::try_deserialize(&mut data)
        .unwrap_or_else(|error| panic!("decode fixture Anchor account {address}: {error}"))
}

pub fn install_fixture_mint(
    svm: &mut LiteSVM,
    address_tag: u8,
    authority: Pubkey,
    decimals: u8,
) -> Pubkey {
    let address = Pubkey::new_from_array([address_tag; 32]);
    let mut data = vec![0; SplMint::LEN];
    SplMint::pack(
        SplMint {
            mint_authority: Some(authority).into(),
            supply: 0,
            decimals,
            is_initialized: true,
            freeze_authority: None.into(),
        },
        &mut data,
    )
    .expect("pack fixture mint");
    svm.set_account(
        address,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(SplMint::LEN),
            data,
            owner: litesvm_token::TOKEN_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("install fixture mint");
    address
}

pub fn create_token_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Pubkey {
    CreateAssociatedTokenAccount::new(svm, payer, mint)
        .owner(owner)
        .send()
        .expect("create fixture token account")
}

pub fn mint_tokens(
    svm: &mut LiteSVM,
    mint_authority: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) {
    MintTo::new(svm, mint_authority, mint, destination, amount)
        .send()
        .expect("mint fixture tokens");
}

pub fn token_state(svm: &LiteSVM, address: &Pubkey) -> SplTokenAccount {
    get_spl_account(svm, address).expect("read classic SPL token account")
}

pub fn token_balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    token_state(svm, address).amount
}

pub fn anchor_state<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm.get_account(address).expect("state account exists");
    let mut bytes = account.data.as_slice();
    T::try_deserialize(&mut bytes).expect("decode Anchor state")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub address: Pubkey,
    pub account: Option<NormalizedAccount>,
}

/// Program-controlled account state used by rollback assertions.
///
/// `rent_epoch` is deliberately excluded: it is deprecated runtime
/// bookkeeping, and LiteSVM canonicalizes a rent-exempt writable account from
/// `0` to `u64::MAX` while loading a successful transaction. Programs cannot
/// control that transition, so it is not protected protocol state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedAccount {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
    pub executable: bool,
}

pub fn snapshot_accounts(svm: &LiteSVM, addresses: &[Pubkey]) -> Vec<AccountSnapshot> {
    let mut unique = HashSet::with_capacity(addresses.len());
    assert!(
        addresses.iter().all(|address| unique.insert(*address)),
        "rollback snapshot address list must be unique"
    );
    addresses
        .iter()
        .map(|address| AccountSnapshot {
            address: *address,
            account: svm.get_account(address).map(|account| NormalizedAccount {
                lamports: account.lamports,
                data: account.data,
                owner: account.owner,
                executable: account.executable,
            }),
        })
        .collect()
}

pub fn signed_legacy_transaction(
    payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
    blockhash: Hash,
) -> Transaction {
    let mut signers = Vec::with_capacity(1 + additional_signers.len());
    let mut seen = HashSet::with_capacity(1 + additional_signers.len());
    signers.push(payer);
    seen.insert(payer.pubkey());
    for signer in additional_signers {
        if seen.insert(signer.pubkey()) {
            signers.push(*signer);
        }
    }
    Transaction::new(
        &signers,
        Message::new(instructions, Some(&payer.pubkey())),
        blockhash,
    )
}

pub fn must_send_legacy(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
    label: &str,
) -> TransactionMetadata {
    let transaction = signed_legacy_transaction(
        payer,
        instructions,
        additional_signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(transaction).unwrap_or_else(|failure| {
        panic!(
            "{label} failed: {:?}\n{}\n{}",
            failure.err,
            failure.meta.pretty_logs(),
            failure.meta.pretty_cpi_tree(),
        )
    })
}

pub fn send_legacy_failure(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
) -> FailedTransactionMetadata {
    let transaction = signed_legacy_transaction(
        payer,
        instructions,
        additional_signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(transaction)
        .expect_err("transaction unexpectedly succeeded")
}

pub fn lookup_candidates(instructions: &[Instruction], payer: Pubkey) -> Vec<Pubkey> {
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    for instruction in instructions {
        if instruction.program_id != payer && seen.insert(instruction.program_id) {
            addresses.push(instruction.program_id);
        }
        for meta in &instruction.accounts {
            if !meta.is_signer && meta.pubkey != payer && seen.insert(meta.pubkey) {
                addresses.push(meta.pubkey);
            }
        }
    }
    addresses
}

/// Create, extend, and warm up a real lookup-table account in the same VM that
/// later executes the versioned transaction.
pub fn install_lookup_table(
    svm: &mut LiteSVM,
    payer: &Keypair,
    addresses: Vec<Pubkey>,
) -> AddressLookupTableAccount {
    assert!(!addresses.is_empty(), "lookup table must not be empty");
    assert!(addresses.len() <= 256, "lookup table capacity exceeded");
    let mut unique = HashSet::with_capacity(addresses.len());
    assert!(
        addresses.iter().all(|address| unique.insert(*address)),
        "lookup table addresses must be unique"
    );

    let recent_slot = svm
        .get_sysvar::<SlotHashes>()
        .slot_hashes()
        .first()
        .map(|(slot, _)| *slot)
        .expect("SlotHashes contains a runtime-authenticated recent slot");
    let authority = payer.pubkey();
    let (create, table_key) = create_lookup_table(authority, authority, recent_slot);
    let extend = extend_lookup_table(table_key, authority, Some(authority), addresses.clone());
    must_send_legacy(
        svm,
        payer,
        &[create, extend],
        &[],
        "create and extend real v0 lookup table",
    );
    advance_one_slot(svm);
    let account = svm
        .get_account(&table_key)
        .expect("lookup-table account exists after creation");
    assert_eq!(
        account.owner,
        solana_address_lookup_table_interface::program::id(),
        "lookup-table account owner"
    );
    let actual_addresses =
        solana_address_lookup_table_interface::state::AddressLookupTable::deserialize(
            &account.data,
        )
        .expect("lookup-table account contains canonical state")
        .addresses
        .to_vec();

    AddressLookupTableAccount {
        key: table_key,
        // The same recent-slot PDA can be reused when one fixture compiles more
        // than once. Always hand the compiler the runtime's complete table, not
        // merely the addresses requested by the latest extension.
        addresses: actual_addresses,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V0MessageResources {
    pub packet_bytes: usize,
    pub static_keys: Vec<Pubkey>,
    pub lookup_table_keys: Vec<Pubkey>,
    pub loaded_writable_keys: Vec<Pubkey>,
    pub loaded_readonly_keys: Vec<Pubkey>,
    pub resolved_unique_keys: Vec<Pubkey>,
    pub unique_locks: usize,
    pub writable_locks: usize,
    pub loaded_account_data_bytes: usize,
}

pub fn compile_v0_transaction(
    svm: &LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    lookup_tables: &[AddressLookupTableAccount],
) -> Result<(VersionedTransaction, V0MessageResources), String> {
    compile_v0_transaction_with_signers(svm, payer, instructions, lookup_tables, &[])
}

pub fn compile_v0_transaction_with_signers(
    svm: &LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    lookup_tables: &[AddressLookupTableAccount],
    additional_signers: &[&Keypair],
) -> Result<(VersionedTransaction, V0MessageResources), String> {
    let message = MessageV0::try_compile(
        &payer.pubkey(),
        instructions,
        lookup_tables,
        svm.latest_blockhash(),
    )
    .map_err(|error| format!("compile v0 message: {error}"))?;
    let resources = measure_v0_message(svm, payer, &message, lookup_tables)?;
    let mut signers = Vec::with_capacity(1 + additional_signers.len());
    let mut seen = HashSet::with_capacity(1 + additional_signers.len());
    signers.push(payer);
    seen.insert(payer.pubkey());
    for signer in additional_signers {
        if seen.insert(signer.pubkey()) {
            signers.push(*signer);
        }
    }
    let transaction = VersionedTransaction::try_new(VersionedMessage::V0(message), &signers)
        .map_err(|error| format!("sign v0 transaction: {error}"))?;
    let packet_bytes = wincode::serialize(&transaction)
        .map_err(|error| format!("serialize v0 transaction: {error}"))?
        .len();
    if packet_bytes != resources.packet_bytes {
        return Err("v0 packet measurement changed after signing".to_owned());
    }
    Ok((transaction, resources))
}

fn measure_v0_message(
    svm: &LiteSVM,
    payer: &Keypair,
    message: &MessageV0,
    lookup_tables: &[AddressLookupTableAccount],
) -> Result<V0MessageResources, String> {
    let mut loaded_writable_keys = Vec::new();
    let mut loaded_readonly_keys = Vec::new();
    let mut lookup_table_keys = Vec::new();
    for lookup in &message.address_table_lookups {
        let table = lookup_tables
            .iter()
            .find(|candidate| candidate.key == lookup.account_key)
            .ok_or_else(|| format!("missing lookup-table contents for {}", lookup.account_key))?;
        lookup_table_keys.push(lookup.account_key);
        for index in &lookup.writable_indexes {
            loaded_writable_keys.push(
                *table
                    .addresses
                    .get(usize::from(*index))
                    .ok_or_else(|| format!("writable lookup index {index} is out of range"))?,
            );
        }
        for index in &lookup.readonly_indexes {
            loaded_readonly_keys.push(
                *table
                    .addresses
                    .get(usize::from(*index))
                    .ok_or_else(|| format!("readonly lookup index {index} is out of range"))?,
            );
        }
    }

    let mut resolved_unique_keys = message.account_keys.clone();
    resolved_unique_keys.extend(loaded_writable_keys.iter().copied());
    resolved_unique_keys.extend(loaded_readonly_keys.iter().copied());
    let unique = resolved_unique_keys.iter().copied().collect::<HashSet<_>>();
    if unique.len() != resolved_unique_keys.len() {
        return Err("v0 compilation produced duplicate resolved account keys".to_owned());
    }

    let required_signatures = usize::from(message.header.num_required_signatures);
    let writable_signed = required_signatures
        .checked_sub(usize::from(message.header.num_readonly_signed_accounts))
        .ok_or_else(|| "invalid signed-account header".to_owned())?;
    let unsigned = message
        .account_keys
        .len()
        .checked_sub(required_signatures)
        .ok_or_else(|| "invalid static-account header".to_owned())?;
    let writable_unsigned = unsigned
        .checked_sub(usize::from(message.header.num_readonly_unsigned_accounts))
        .ok_or_else(|| "invalid unsigned-account header".to_owned())?;
    let writable_locks = writable_signed + writable_unsigned + loaded_writable_keys.len();

    // LiteSVM mirrors the runtime's SIMD-0186 meter: each resolved lookup
    // table costs a fixed 8,248 bytes, then each existing message account costs
    // 64 bytes plus data. The constructed Instructions sysvar and nonexistent
    // default accounts are explicitly charged zero by the pinned loader.
    let lookup_table_bytes = lookup_table_keys
        .len()
        .checked_mul(ADDRESS_LOOKUP_TABLE_BASE_SIZE)
        .ok_or_else(|| "lookup-table data measurement overflow".to_owned())?;
    let loaded_account_data_bytes =
        resolved_unique_keys
            .iter()
            .try_fold(lookup_table_bytes, |total, key| {
                let account_bytes = if solana_sdk_ids::sysvar::instructions::check_id(key) {
                    0
                } else if let Some(account) = svm.get_account(key) {
                    TRANSACTION_ACCOUNT_BASE_SIZE
                        .checked_add(account.data.len())
                        .ok_or_else(|| "account data measurement overflow".to_owned())?
                } else {
                    0
                };
                total
                    .checked_add(account_bytes)
                    .ok_or_else(|| "loaded-account-data measurement overflow".to_owned())
            })?;

    let unsigned_transaction = VersionedTransaction {
        signatures: vec![Default::default(); usize::from(message.header.num_required_signatures)],
        message: VersionedMessage::V0(message.clone()),
    };
    let packet_bytes = wincode::serialize(&unsigned_transaction)
        .map_err(|error| format!("serialize measured v0 transaction: {error}"))?
        .len();
    // Signature byte widths are fixed, so the unsigned placeholder packet has
    // the exact same length as the signed packet.
    assert_eq!(
        unsigned_transaction.signatures.len(),
        usize::from(message.header.num_required_signatures)
    );
    assert_eq!(message.account_keys[0], payer.pubkey());

    Ok(V0MessageResources {
        packet_bytes,
        static_keys: message.account_keys.clone(),
        lookup_table_keys,
        loaded_writable_keys,
        loaded_readonly_keys,
        unique_locks: resolved_unique_keys.len(),
        writable_locks,
        resolved_unique_keys,
        loaded_account_data_bytes,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResources {
    pub compute_units: u64,
    pub instruction_trace_len: usize,
    pub cpi_tree_frames: usize,
    pub maximum_stack_height: u8,
    pub cpi_tree_depth: usize,
    pub maximum_cpi_account_positions: usize,
    pub maximum_instruction_data_bytes: usize,
    pub return_data_bytes: usize,
}

pub fn measure_execution(
    metadata: &TransactionMetadata,
    top_level_instructions: &[Instruction],
) -> ExecutionResources {
    let inner_count = metadata
        .inner_instructions
        .iter()
        .map(Vec::len)
        .sum::<usize>();
    let instruction_trace_len = top_level_instructions.len() + inner_count;
    let maximum_stack_height = metadata
        .inner_instructions
        .iter()
        .flatten()
        .map(|inner| inner.stack_height)
        .max()
        .unwrap_or(u8::from(!top_level_instructions.is_empty()));
    let maximum_cpi_account_positions = metadata
        .inner_instructions
        .iter()
        .flatten()
        .map(|inner| inner.instruction.accounts.len())
        .max()
        .unwrap_or(0);
    let maximum_instruction_data_bytes = top_level_instructions
        .iter()
        .map(|instruction| instruction.data.len())
        .chain(
            metadata
                .inner_instructions
                .iter()
                .flatten()
                .map(|inner| inner.instruction.data.len()),
        )
        .max()
        .unwrap_or(0);
    let tree = metadata.cpi_tree();

    ExecutionResources {
        compute_units: metadata.compute_units_consumed,
        instruction_trace_len,
        cpi_tree_frames: tree.iter().map(frame_count).sum(),
        maximum_stack_height,
        cpi_tree_depth: tree.iter().map(frame_depth).max().unwrap_or(0),
        maximum_cpi_account_positions,
        maximum_instruction_data_bytes,
        return_data_bytes: metadata.return_data.data.len(),
    }
}

/// Enforce the spec's private 20% acceptance threshold on an actually signed
/// message and its executed metadata. These are falsification thresholds, not
/// a public product limit.
pub fn assert_controlled_resource_headroom(
    label: &str,
    message: &V0MessageResources,
    execution: &ExecutionResources,
) {
    eprintln!(
        "RESOURCE {label}: packet={} locks={} writable={} loaded_data={} compute={} trace={} frames={} stack={} tree_depth={} cpi_positions={} instruction_data={} return_data={}",
        message.packet_bytes,
        message.unique_locks,
        message.writable_locks,
        message.loaded_account_data_bytes,
        execution.compute_units,
        execution.instruction_trace_len,
        execution.cpi_tree_frames,
        execution.maximum_stack_height,
        execution.cpi_tree_depth,
        execution.maximum_cpi_account_positions,
        execution.maximum_instruction_data_bytes,
        execution.return_data_bytes,
    );
    assert!(message.packet_bytes <= PACKET_ACCEPTANCE_CEILING);
    assert!(message.unique_locks <= UNIQUE_LOCK_ACCEPTANCE_CEILING);
    assert!(message.writable_locks <= message.unique_locks);
    assert!(message.loaded_account_data_bytes <= LOADED_ACCOUNT_DATA_ACCEPTANCE_CEILING);
    assert!(execution.compute_units <= COMPUTE_ACCEPTANCE_CEILING);
    assert!(execution.instruction_trace_len <= INSTRUCTION_TRACE_ACCEPTANCE_CEILING);
    assert!(execution.cpi_tree_frames <= INSTRUCTION_TRACE_ACCEPTANCE_CEILING);
    assert!(execution.maximum_stack_height <= STACK_HEIGHT_ACCEPTANCE_CEILING);
    assert!(execution.cpi_tree_depth <= usize::from(STACK_HEIGHT_ACCEPTANCE_CEILING));
    assert!(execution.maximum_cpi_account_positions <= CPI_ACCOUNT_POSITION_ACCEPTANCE_CEILING);
    assert!(execution.maximum_instruction_data_bytes <= INSTRUCTION_DATA_ACCEPTANCE_CEILING);
    assert!(execution.return_data_bytes <= RETURN_DATA_ACCEPTANCE_CEILING);
}

fn frame_count(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_count).sum::<usize>()
}

fn frame_depth(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_depth).max().unwrap_or(0)
}

/// Materialize every root-to-frame program path so integration assertions can
/// distinguish sibling CPIs from a falsely nested authority path.
pub fn cpi_program_paths(metadata: &TransactionMetadata) -> Vec<Vec<Pubkey>> {
    fn visit(frame: &CpiFrame, prefix: &mut Vec<Pubkey>, output: &mut Vec<Vec<Pubkey>>) {
        prefix.push(frame.program_id);
        output.push(prefix.clone());
        for child in &frame.children {
            visit(child, prefix, output);
        }
        prefix.pop();
    }

    let mut output = Vec::new();
    for root in metadata.cpi_tree() {
        visit(&root, &mut Vec::new(), &mut output);
    }
    output
}

pub fn frame_program_count(metadata: &TransactionMetadata, program_id: Pubkey) -> usize {
    fn count(frame: &CpiFrame, program_id: Pubkey) -> usize {
        usize::from(frame.program_id == program_id)
            + frame
                .children
                .iter()
                .map(|child| count(child, program_id))
                .sum::<usize>()
    }
    metadata
        .cpi_tree()
        .iter()
        .map(|root| count(root, program_id))
        .sum()
}

pub fn contains_program_path(metadata: &TransactionMetadata, expected: &[Pubkey]) -> bool {
    fn matches_from(frame: &CpiFrame, expected: &[Pubkey]) -> bool {
        let Some((head, tail)) = expected.split_first() else {
            return true;
        };
        frame.program_id == *head
            && (tail.is_empty() || frame.children.iter().any(|child| matches_from(child, tail)))
    }
    !expected.is_empty()
        && metadata
            .cpi_tree()
            .iter()
            .any(|root| matches_from(root, expected))
}

pub fn advance_one_slot(svm: &mut LiteSVM) -> u64 {
    let next = svm.get_sysvar::<Clock>().slot.saturating_add(1);
    svm.warp_to_slot(next);
    svm.expire_blockhash();
    next
}

/// Build a normal LiteSVM environment while making the SBPFv0 deployment
/// exception explicit and inspectable in the fixture source.
pub fn loader_v3_test_vm() -> LiteSVM {
    let feature_id = agave_feature_set::disable_sbpf_v0_v1_v2_deployment::id();
    let mut feature_set = LiteSVM::mainnet_feature_set();
    // Activate then explicitly deactivate so this fixture cannot silently rely
    // on whether LiteSVM's pinned mainnet list happens to contain the feature.
    feature_set.activate(&feature_id, 0);
    assert!(feature_set.is_active(&feature_id));
    feature_set.deactivate(&feature_id);
    assert!(!feature_set.is_active(&feature_id));
    assert!(SBPF_V0_DEPLOYMENT_OVERRIDE_LABEL.contains("test-only"));

    LiteSVM::default()
        .with_feature_set(feature_set)
        .with_builtins()
        .with_lamports(1_000_000_000_000_000)
        .with_sysvars()
        .with_feature_accounts()
        .with_default_programs()
        .with_sigverify(true)
        .with_blockhash_check(true)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutableProgramDeployment {
    pub program_id: Pubkey,
    pub program_data: Pubkey,
    pub deployment_slot: u64,
    pub program_data_len: usize,
}

/// Install a loader-owned 36-byte Uninitialized Program account at a fixed
/// declared program ID, then use real Buffer, Write, and
/// DeployWithMaxDataLen instructions. No synthetic ProgramData state is used.
pub fn deploy_fixed_id_mutable_program(
    svm: &mut LiteSVM,
    payer_and_authority: &Keypair,
    program_id: Pubkey,
    elf: &[u8],
    max_data_len: usize,
    buffer_key_tag: u8,
) -> MutableProgramDeployment {
    assert!(elf.starts_with(b"\x7fELF"));
    assert!(max_data_len >= elf.len());
    assert!(svm.get_account(&program_id).is_none());
    assert_eq!(
        LOADER_V3_PROGRAM_ACCOUNT_LEN,
        UpgradeableLoaderState::size_of_program()
    );
    assert_eq!(
        LOADER_V3_PROGRAM_DATA_METADATA_LEN,
        UpgradeableLoaderState::size_of_programdata_metadata()
    );
    svm.set_account(
        program_id,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(LOADER_V3_PROGRAM_ACCOUNT_LEN),
            data: vec![0; LOADER_V3_PROGRAM_ACCOUNT_LEN],
            owner: solana_sdk_ids::bpf_loader_upgradeable::id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("preinsert fixed-ID Uninitialized Program account");
    let uninitialized_program = svm
        .get_account(&program_id)
        .expect("preinserted fixed-ID Program account");
    assert_eq!(
        wincode::deserialize_exact::<UpgradeableLoaderState>(
            &uninitialized_program.data[..UpgradeableLoaderState::size_of_uninitialized()]
        )
        .expect("decode fixed-ID Uninitialized loader state"),
        UpgradeableLoaderState::Uninitialized
    );
    assert!(uninitialized_program.data.iter().all(|byte| *byte == 0));

    let buffer = load_loader_buffer(
        svm,
        payer_and_authority,
        elf,
        buffer_key_tag,
        "primary deployment buffer",
    );
    #[allow(deprecated)]
    let deploy_instructions = loader_v3_instruction::deploy_with_max_program_len(
        &payer_and_authority.pubkey(),
        &program_id,
        &buffer,
        &payer_and_authority.pubkey(),
        svm.minimum_balance_for_rent_exemption(LOADER_V3_PROGRAM_ACCOUNT_LEN),
        max_data_len,
        true,
    )
    .expect("construct DeployWithMaxDataLen");
    assert_eq!(deploy_instructions.len(), 2);
    // The fixed program account is already present because no private key can
    // be produced for a declared ID. Execute only the real loader instruction.
    let deploy = deploy_instructions[1].clone();
    must_send_legacy(
        svm,
        payer_and_authority,
        &[deploy],
        &[],
        "real fixed-ID DeployWithMaxDataLen",
    );

    let program_data = get_program_data_address(&program_id);
    let state = read_program_data_state(svm, &program_data);
    let (deployment_slot, authority) = match state {
        UpgradeableLoaderState::ProgramData {
            slot,
            upgrade_authority_address,
        } => (slot, upgrade_authority_address),
        other => panic!("unexpected ProgramData state after deploy: {other:?}"),
    };
    assert_eq!(authority, Some(payer_and_authority.pubkey()));
    assert_eq!(
        read_program_state(svm, &program_id),
        UpgradeableLoaderState::Program {
            programdata_address: program_data,
        }
    );
    let program_account = svm.get_account(&program_id).expect("deployed Program");
    assert!(program_account.executable);
    let program_data_account = svm
        .get_account(&program_data)
        .expect("deployed ProgramData");
    assert_eq!(
        program_data_account.data.len(),
        UpgradeableLoaderState::size_of_programdata(max_data_len)
    );
    assert_eq!(
        &program_data_account.data
            [LOADER_V3_PROGRAM_DATA_METADATA_LEN..LOADER_V3_PROGRAM_DATA_METADATA_LEN + elf.len()],
        elf
    );
    assert!(
        program_data_account.data[LOADER_V3_PROGRAM_DATA_METADATA_LEN + elf.len()..]
            .iter()
            .all(|byte| *byte == 0)
    );

    MutableProgramDeployment {
        program_id,
        program_data,
        deployment_slot,
        program_data_len: program_data_account.data.len(),
    }
}

pub fn extend_mutable_program(
    svm: &mut LiteSVM,
    payer: &Keypair,
    deployment: &MutableProgramDeployment,
    additional_bytes: u32,
) -> MutableProgramDeployment {
    assert!(additional_bytes > 0);
    let before = svm
        .get_account(&deployment.program_data)
        .expect("ProgramData exists before extension");
    let instruction = loader_v3_instruction::extend_program(
        &deployment.program_id,
        Some(&payer.pubkey()),
        additional_bytes,
    );
    must_send_legacy(
        svm,
        payer,
        &[instruction],
        &[],
        "real loader-v3 ExtendProgram",
    );
    let after = svm
        .get_account(&deployment.program_data)
        .expect("ProgramData exists after extension");
    assert_eq!(
        after.data.len(),
        before.data.len() + usize::try_from(additional_bytes).expect("u32 fits usize")
    );
    let state = read_program_data_state(svm, &deployment.program_data);
    let slot = match state {
        UpgradeableLoaderState::ProgramData { slot, .. } => slot,
        other => panic!("unexpected ProgramData state after extension: {other:?}"),
    };
    MutableProgramDeployment {
        program_id: deployment.program_id,
        program_data: deployment.program_data,
        deployment_slot: slot,
        program_data_len: after.data.len(),
    }
}

pub fn prepare_upgrade_buffer(
    svm: &mut LiteSVM,
    payer_and_authority: &Keypair,
    replacement_elf: &[u8],
    buffer_key_tag: u8,
) -> Pubkey {
    load_loader_buffer(
        svm,
        payer_and_authority,
        replacement_elf,
        buffer_key_tag,
        "replacement upgrade buffer",
    )
}

pub fn upgrade_instruction(
    deployment: &MutableProgramDeployment,
    buffer: Pubkey,
    payer_and_authority: &Keypair,
) -> Instruction {
    loader_v3_instruction::upgrade(
        &deployment.program_id,
        &buffer,
        &payer_and_authority.pubkey(),
        &payer_and_authority.pubkey(),
        true,
    )
}

pub fn upgrade_mutable_program(
    svm: &mut LiteSVM,
    payer_and_authority: &Keypair,
    deployment: &MutableProgramDeployment,
    buffer: Pubkey,
    replacement_elf: &[u8],
) -> MutableProgramDeployment {
    assert!(
        LOADER_V3_PROGRAM_DATA_METADATA_LEN + replacement_elf.len() <= deployment.program_data_len
    );
    let instruction = upgrade_instruction(deployment, buffer, payer_and_authority);
    must_send_legacy(
        svm,
        payer_and_authority,
        &[instruction],
        &[],
        "real loader-v3 Upgrade",
    );
    let account = svm
        .get_account(&deployment.program_data)
        .expect("ProgramData exists after upgrade");
    assert_eq!(account.data.len(), deployment.program_data_len);
    assert_eq!(
        &account.data[LOADER_V3_PROGRAM_DATA_METADATA_LEN
            ..LOADER_V3_PROGRAM_DATA_METADATA_LEN + replacement_elf.len()],
        replacement_elf
    );
    assert!(
        account.data[LOADER_V3_PROGRAM_DATA_METADATA_LEN + replacement_elf.len()..]
            .iter()
            .all(|byte| *byte == 0)
    );
    let state = read_program_data_state(svm, &deployment.program_data);
    let slot = match state {
        UpgradeableLoaderState::ProgramData {
            slot,
            upgrade_authority_address,
        } => {
            assert_eq!(
                upgrade_authority_address,
                Some(payer_and_authority.pubkey())
            );
            slot
        }
        other => panic!("unexpected ProgramData state after upgrade: {other:?}"),
    };
    MutableProgramDeployment {
        program_id: deployment.program_id,
        program_data: deployment.program_data,
        deployment_slot: slot,
        program_data_len: account.data.len(),
    }
}

fn load_loader_buffer(
    svm: &mut LiteSVM,
    payer_and_authority: &Keypair,
    elf: &[u8],
    buffer_key_tag: u8,
    label: &str,
) -> Pubkey {
    assert!(elf.starts_with(b"\x7fELF"));
    let buffer_keypair = fixture_keypair(buffer_key_tag);
    let buffer = buffer_keypair.pubkey();
    assert!(svm.get_account(&buffer).is_none(), "buffer key reused");
    let buffer_len = UpgradeableLoaderState::size_of_buffer(elf.len());
    let create = loader_v3_instruction::create_buffer(
        &payer_and_authority.pubkey(),
        &buffer,
        &payer_and_authority.pubkey(),
        svm.minimum_balance_for_rent_exemption(buffer_len),
        elf.len(),
    )
    .expect("construct loader buffer creation");
    must_send_legacy(
        svm,
        payer_and_authority,
        &create,
        &[&buffer_keypair],
        &format!("create {label}"),
    );
    for (index, chunk) in elf.chunks(LOADER_WRITE_CHUNK_BYTES).enumerate() {
        let offset = index
            .checked_mul(LOADER_WRITE_CHUNK_BYTES)
            .and_then(|value| u32::try_from(value).ok())
            .expect("loader write offset fits u32");
        let write = loader_v3_instruction::write(
            &buffer,
            &payer_and_authority.pubkey(),
            offset,
            chunk.to_vec(),
        );
        must_send_legacy(
            svm,
            payer_and_authority,
            &[write],
            &[],
            &format!("write {label} chunk {index}"),
        );
        svm.expire_blockhash();
    }
    buffer
}

pub fn read_program_state(svm: &LiteSVM, program: &Pubkey) -> UpgradeableLoaderState {
    let account = svm.get_account(program).expect("Program account exists");
    assert_eq!(account.owner, solana_sdk_ids::bpf_loader_upgradeable::id());
    assert_eq!(account.data.len(), LOADER_V3_PROGRAM_ACCOUNT_LEN);
    wincode::deserialize_exact(&account.data).expect("decode loader-v3 Program state")
}

pub fn read_program_data_state(svm: &LiteSVM, program_data: &Pubkey) -> UpgradeableLoaderState {
    let account = svm
        .get_account(program_data)
        .expect("ProgramData account exists");
    assert_eq!(account.owner, solana_sdk_ids::bpf_loader_upgradeable::id());
    assert!(account.data.len() >= LOADER_V3_PROGRAM_DATA_METADATA_LEN);
    let metadata = &account.data[..LOADER_V3_PROGRAM_DATA_METADATA_LEN];
    let state: UpgradeableLoaderState =
        wincode::deserialize(metadata).expect("decode loader-v3 ProgramData state prefix");
    assert!(
        matches!(state, UpgradeableLoaderState::ProgramData { .. }),
        "loader-v3 ProgramData account contains unexpected state: {state:?}"
    );

    // Loader-v3 reserves the maximum 45-byte metadata region even when a
    // None authority makes the canonical serialized state only 13 bytes. Keep
    // the reserved suffix out of the exact decode while still rejecting a
    // malformed or non-canonical state prefix.
    let serialized_len = usize::try_from(
        wincode::serialized_size(&state).expect("size loader-v3 ProgramData state"),
    )
    .expect("loader-v3 ProgramData state length fits usize");
    let exact_metadata = metadata
        .get(..serialized_len)
        .expect("loader-v3 ProgramData state fits reserved metadata region");
    assert_eq!(
        wincode::deserialize_exact::<UpgradeableLoaderState>(exact_metadata)
            .expect("exactly decode loader-v3 ProgramData state prefix"),
        state,
        "loader-v3 ProgramData state prefix changed across exact decode"
    );
    state
}

pub fn readonly_meta(key: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(key, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreExecuteAccountClosure {
    pub configuration: Pubkey,
    pub market: Pubkey,
    pub fee_policy: Pubkey,
    pub engine_program: Pubkey,
    pub callback_authority: Pubkey,
    pub loader_policy: Vec<Pubkey>,
    pub domain_controls: Vec<AccountMeta>,
    pub authorization_controls: Vec<AccountMeta>,
    pub protected_profile: Vec<Pubkey>,
    pub fee_controls: Vec<AccountMeta>,
    pub settlement: Vec<AccountMeta>,
    pub opaque: Vec<AccountMeta>,
}

/// Encode one canonical top-level Core instruction. The fixed prefix cannot be
/// privilege-upgraded by callers; variable control and capability privileges
/// remain explicit so negative fixtures can exercise Core's exact checks.
pub fn build_core_execute_instruction(
    envelope: &generic_effect_private_wire::ExecuteEnvelopeCandidateV0,
    closure: &CoreExecuteAccountClosure,
) -> Result<Instruction, String> {
    let header = &envelope.header;
    require_segment_len(
        "loader-policy closure",
        closure.loader_policy.len(),
        header.loader_policy_account_count,
    )?;
    require_segment_len(
        "domain-control closure",
        closure.domain_controls.len(),
        header.domain_control_account_count,
    )?;
    require_segment_len(
        "authorization-control closure",
        closure.authorization_controls.len(),
        header.authorization_account_count,
    )?;
    require_segment_len(
        "protected-profile closure",
        closure.protected_profile.len(),
        header.protected_profile_account_count,
    )?;
    require_segment_len(
        "fee-control closure",
        closure.fee_controls.len(),
        header.fee_control_account_count,
    )?;
    require_segment_len(
        "settlement closure",
        closure.settlement.len(),
        header.settlement_capability_count,
    )?;
    require_segment_len(
        "opaque closure",
        closure.opaque.len(),
        header.opaque_capability_count,
    )?;

    let dynamic_count = closure.loader_policy.len()
        + closure.domain_controls.len()
        + closure.authorization_controls.len()
        + closure.protected_profile.len()
        + closure.fee_controls.len()
        + closure.settlement.len()
        + closure.opaque.len();
    let mut accounts = Vec::with_capacity(6 + dynamic_count);
    accounts.extend([
        readonly_meta(closure.configuration),
        readonly_meta(closure.market),
        readonly_meta(closure.fee_policy),
        readonly_meta(closure.engine_program),
        readonly_meta(closure.callback_authority),
        readonly_meta(solana_sdk_ids::sysvar::instructions::id()),
    ]);
    accounts.extend(closure.loader_policy.iter().copied().map(readonly_meta));
    accounts.extend(closure.domain_controls.iter().cloned());
    accounts.extend(closure.authorization_controls.iter().cloned());
    accounts.extend(closure.protected_profile.iter().copied().map(readonly_meta));
    accounts.extend(closure.fee_controls.iter().cloned());
    accounts.extend(closure.settlement.iter().cloned());
    accounts.extend(closure.opaque.iter().cloned());

    let data = envelope
        .encode()
        .map_err(|error| format!("encode canonical Core envelope: {error:?}"))?;
    Ok(Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts,
        data,
    })
}

fn require_segment_len(label: &str, actual: usize, declared: u8) -> Result<(), String> {
    if actual == usize::from(declared) {
        Ok(())
    } else {
        Err(format!(
            "{label} length mismatch: declared {declared}, observed {actual}"
        ))
    }
}
