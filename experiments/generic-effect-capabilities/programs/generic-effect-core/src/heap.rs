//! Controlled heap support for the full generic-effect execution path.
//!
//! Solana's standard entrypoint allocator always caps its backwards bump at
//! the default 32 KiB, even when the transaction requests a larger VM heap
//! frame. The complete execute path has a measured requirement above that
//! default. This private experiment therefore uses a bounded forward
//! allocator and authenticates the matching transaction-level Compute Budget
//! instruction before doing any large allocation. A top allocation is
//! reclaimed when it is released in stack order; non-top releases remain
//! charged to the invocation. This preserves bump-allocation simplicity while
//! reclaiming ordinary short-lived nested temporaries.

use anchor_lang::prelude::*;

use crate::error::CoreError;

pub const CONTROLLED_HEAP_FRAME_BYTES: u32 = 208 * 1_024;
const REQUEST_HEAP_FRAME_DISCRIMINATOR: u8 = 1;

/// Requires one canonical RequestHeapFrame instruction for the measured
/// controlled budget.
/// Compute Budget preprocessing applies transaction-wide regardless of
/// instruction order, so this scans the complete authenticated Instructions
/// sysvar.
pub fn require_controlled_heap_frame(instructions_sysvar: &AccountInfo<'_>) -> Result<()> {
    use solana_instructions_sysvar::ID;

    require_keys_eq!(*instructions_sysvar.key, ID, CoreError::InvalidWireEncoding);
    require_keys_eq!(
        *instructions_sysvar.owner,
        solana_sdk_ids::sysvar::ID,
        CoreError::InvalidWireEncoding
    );
    require!(
        !instructions_sysvar.is_signer && !instructions_sysvar.is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    let data = instructions_sysvar
        .try_borrow_data()
        .map_err(|_| CoreError::InvalidWireEncoding)?;
    let instruction_count = read_u16(&data, 0).ok_or(CoreError::InvalidWireEncoding)? as usize;
    let offset_table_end = 2usize
        .checked_add(
            instruction_count
                .checked_mul(2)
                .ok_or(CoreError::ArithmeticOverflow)?,
        )
        .ok_or(CoreError::ArithmeticOverflow)?;
    let serialized_instruction_end = data
        .len()
        .checked_sub(2)
        .ok_or(CoreError::InvalidWireEncoding)?;
    require!(
        offset_table_end <= serialized_instruction_end,
        CoreError::InvalidWireEncoding
    );

    // Parse borrowed sysvar bytes instead of repeatedly materializing owned
    // `Instruction` values. Avoiding owned values also avoids charging the
    // controlled allocator for an unrelated full-transaction scan before
    // execution begins.
    let mut matched = false;
    for index in 0..instruction_count {
        let offset_position = 2usize
            .checked_add(index.checked_mul(2).ok_or(CoreError::ArithmeticOverflow)?)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let instruction_start =
            read_u16(&data, offset_position).ok_or(CoreError::InvalidWireEncoding)? as usize;
        require!(
            instruction_start >= offset_table_end,
            CoreError::InvalidWireEncoding
        );

        let account_count =
            read_u16(&data, instruction_start).ok_or(CoreError::InvalidWireEncoding)? as usize;
        let accounts_end = instruction_start
            .checked_add(2)
            .and_then(|value| {
                account_count
                    .checked_mul(33)
                    .and_then(|accounts_len| value.checked_add(accounts_len))
            })
            .ok_or(CoreError::ArithmeticOverflow)?;
        let program_id_end = accounts_end
            .checked_add(32)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let data_len =
            read_u16(&data, program_id_end).ok_or(CoreError::InvalidWireEncoding)? as usize;
        let instruction_data_start = program_id_end
            .checked_add(2)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let instruction_data_end = instruction_data_start
            .checked_add(data_len)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            instruction_data_end <= serialized_instruction_end,
            CoreError::InvalidWireEncoding
        );

        let program_id = data
            .get(accounts_end..program_id_end)
            .ok_or(CoreError::InvalidWireEncoding)?;
        let instruction_data = data
            .get(instruction_data_start..instruction_data_end)
            .ok_or(CoreError::InvalidWireEncoding)?;
        if program_id != solana_sdk_ids::compute_budget::ID.as_ref()
            || instruction_data.first().copied() != Some(REQUEST_HEAP_FRAME_DISCRIMINATOR)
        {
            continue;
        }
        let bytes = instruction_data
            .get(1..5)
            .and_then(|value| <[u8; 4]>::try_from(value).ok())
            .map(u32::from_le_bytes)
            .ok_or(CoreError::ControlledHeapFrameRequired)?;
        require!(
            !matched
                && account_count == 0
                && instruction_data.len() == 5
                && bytes == CONTROLLED_HEAP_FRAME_BYTES
                && bytes % 1_024 == 0,
            CoreError::ControlledHeapFrameRequired
        );
        matched = true;
    }
    require!(matched, CoreError::ControlledHeapFrameRequired);
    Ok(())
}

#[inline]
fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(bytes).ok()?))
}

#[cfg(all(target_os = "solana", not(feature = "no-entrypoint")))]
mod allocator {
    use std::{
        alloc::{GlobalAlloc, Layout},
        mem::size_of,
        ptr::{null_mut, read_volatile, write_volatile},
    };

    use super::CONTROLLED_HEAP_FRAME_BYTES;

    const HEAP_START: usize = anchor_lang::solana_program::entrypoint::HEAP_START_ADDRESS as usize;
    const HEAP_END: usize = HEAP_START + CONTROLLED_HEAP_FRAME_BYTES as usize;
    const WORD_BYTES: usize = size_of::<usize>();
    const GLOBAL_HEADER_WORDS: usize = 1;
    const GLOBAL_HEADER_BYTES: usize = GLOBAL_HEADER_WORDS * WORD_BYTES;
    const BLOCK_HEADER_WORDS: usize = 1;
    const BLOCK_HEADER_BYTES: usize = BLOCK_HEADER_WORDS * WORD_BYTES;
    const HEAP_DATA_START: usize = HEAP_START + GLOBAL_HEADER_BYTES;
    const CURSOR_WORD: *mut usize = HEAP_START as *mut usize;

    pub struct ControlledForwardBumpAllocator;

    #[global_allocator]
    static ALLOCATOR: ControlledForwardBumpAllocator = ControlledForwardBumpAllocator;

    // The runtime zero-initializes the mapped heap. One global word retains the
    // current cursor. Every allocation stores its previous cursor immediately
    // before the aligned payload. Deallocation rewinds only when that
    // allocation is still at the top, so no live block can be reused.
    // ExecuteEffect authenticates the requested frame before it can cross the
    // default 32 KiB boundary.
    unsafe impl GlobalAlloc for ControlledForwardBumpAllocator {
        #[inline(never)]
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let stored_cursor = unsafe { read_volatile(CURSOR_WORD) };
            let position = if stored_cursor == 0 {
                HEAP_DATA_START
            } else if (HEAP_DATA_START..=HEAP_END).contains(&stored_cursor) {
                stored_cursor
            } else {
                return null_mut();
            };

            let required_alignment = layout.align().max(WORD_BYTES);
            let alignment_mask = required_alignment - 1;
            let Some(payload_base) = position.checked_add(BLOCK_HEADER_BYTES) else {
                return null_mut();
            };
            let Some(aligned) = payload_base
                .checked_add(alignment_mask)
                .map(|value| value & !alignment_mask)
            else {
                return null_mut();
            };
            let Some(block_header) = aligned.checked_sub(BLOCK_HEADER_BYTES) else {
                return null_mut();
            };
            let allocation_bytes = layout.size().max(1);
            let Some(next) = aligned.checked_add(allocation_bytes) else {
                return null_mut();
            };
            if block_header < position || next > HEAP_END {
                return null_mut();
            }

            unsafe {
                write_volatile(block_header as *mut usize, position);
                write_volatile(CURSOR_WORD, next);
            }
            aligned as *mut u8
        }

        #[inline(never)]
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            if pointer.is_null() {
                return;
            }
            let pointer_address = pointer as usize;
            let Some(block_header) = pointer_address.checked_sub(BLOCK_HEADER_BYTES) else {
                return;
            };
            if block_header < HEAP_DATA_START
                || block_header
                    .checked_add(BLOCK_HEADER_BYTES)
                    .is_none_or(|end| end > HEAP_END)
                || block_header % WORD_BYTES != 0
            {
                return;
            }
            let Some(expected_end) = pointer_address.checked_add(layout.size().max(1)) else {
                return;
            };
            if unsafe { read_volatile(CURSOR_WORD) } != expected_end {
                return;
            }
            let previous_cursor = unsafe { read_volatile(block_header as *const usize) };
            if !(HEAP_DATA_START..=block_header).contains(&previous_cursor) {
                return;
            }
            unsafe { write_volatile(CURSOR_WORD, previous_cursor) };
        }
    }
}
