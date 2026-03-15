// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Per-process page table management for the Lineluya kernel.
//!
//! Provides:
//! - [`PHYS_MEM_OFFSET_CHIRHO`] — stored physical memory offset from the
//!   bootloader so we can translate physical addresses to virtual ones.
//! - [`create_user_page_table_chirho`] — allocate a fresh PML4, copy kernel
//!   mappings (upper half), return the root physical address.
//! - [`clone_page_table_chirho`] — deep-copy a user page table, marking
//!   writable user pages as read-only + COW for copy-on-write.
//! - [`switch_page_table_chirho`] — write a new PML4 physical address to CR3.
//! - [`handle_cow_fault_chirho`] — COW page fault handler: allocate a new
//!   frame, copy data, remap as writable.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::structures::paging::{
    FrameAllocator, PageTable, PageTableFlags, PhysFrame,
};
use x86_64::{PhysAddr, VirtAddr};

use crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO;

// ============================================================================
// Constants
// ============================================================================

/// Page size (4 KiB).
const PAGE_SIZE_CHIRHO: u64 = 4096;

/// Number of entries in a page table level (512 for x86_64 4-level paging).
const ENTRIES_PER_TABLE_CHIRHO: usize = 512;

/// The boundary between user-space and kernel-space in the PML4.
/// Entries 0..255 are user-space, 256..511 are kernel-space.
const KERNEL_PML4_START_CHIRHO: usize = 256;

/// Custom bit in page table entries to mark a page as COW (copy-on-write).
/// We use bit 9 (one of the "available to OS" bits in x86_64 PTEs).
pub const COW_BIT_CHIRHO: u64 = 1 << 9;

// ============================================================================
// Physical memory offset storage
// ============================================================================

/// Global storage for the bootloader-provided physical memory offset.
/// All physical addresses can be accessed at virtual address
/// `PHYS_MEM_OFFSET_CHIRHO + phys_addr`.
static PHYS_MEM_OFFSET_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Store the physical memory offset. Called once during kernel init.
pub fn set_phys_mem_offset_chirho(offset_chirho: u64) {
    PHYS_MEM_OFFSET_CHIRHO.store(offset_chirho, Ordering::Release);
}

/// Retrieve the physical memory offset.
pub fn phys_mem_offset_chirho() -> u64 {
    PHYS_MEM_OFFSET_CHIRHO.load(Ordering::Acquire)
}

/// Convert a physical address to a virtual address using the stored offset.
#[inline]
fn phys_to_virt_chirho(phys_chirho: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys_chirho.as_u64() + phys_mem_offset_chirho())
}

/// Translate a virtual address to its physical address by walking the page table.
/// Returns None if the address is not mapped.
pub fn virt_to_phys_chirho(virt_chirho: u64) -> Option<u64> {
    let phys_offset_chirho = phys_mem_offset_chirho();
    let (pml4_frame_chirho, _) = Cr3::read();
    let pml4_phys_chirho = pml4_frame_chirho.start_address().as_u64();

    let indices_chirho = [
        ((virt_chirho >> 39) & 0x1FF) as usize, // PML4
        ((virt_chirho >> 30) & 0x1FF) as usize, // PDPT
        ((virt_chirho >> 21) & 0x1FF) as usize, // PD
        ((virt_chirho >> 12) & 0x1FF) as usize, // PT
    ];

    let mut table_phys_chirho = pml4_phys_chirho;

    for (level_chirho, &idx_chirho) in indices_chirho.iter().enumerate() {
        let table_virt_chirho = table_phys_chirho + phys_offset_chirho;
        let entry_chirho = unsafe {
            let table_ptr_chirho = table_virt_chirho as *const PageTableEntry;
            core::ptr::read_volatile(table_ptr_chirho.add(idx_chirho))
        };

        if entry_chirho.is_unused() {
            return None;
        }

        let entry_phys_chirho = entry_chirho.addr().as_u64();

        // Check for huge pages (1GiB at level 1, 2MiB at level 2)
        let flags_chirho = entry_chirho.flags();
        if flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            if level_chirho == 1 {
                // 1 GiB page
                return Some(entry_phys_chirho + (virt_chirho & 0x3FFFFFFF));
            } else if level_chirho == 2 {
                // 2 MiB page
                return Some(entry_phys_chirho + (virt_chirho & 0x1FFFFF));
            }
        }

        if level_chirho == 3 {
            // Final level — add page offset
            return Some(entry_phys_chirho + (virt_chirho & 0xFFF));
        }

        table_phys_chirho = entry_phys_chirho;
    }

    None
}

/// Get a mutable reference to the page table at the given physical address.
///
/// # Safety
///
/// The physical address must point to a valid, aligned `PageTable`.
/// The caller must ensure no aliasing mutable references exist.
unsafe fn table_from_phys_chirho(phys_chirho: PhysAddr) -> &'static mut PageTable {
    let virt_chirho = phys_to_virt_chirho(phys_chirho);
    &mut *(virt_chirho.as_mut_ptr::<PageTable>())
}

// ============================================================================
// create_user_page_table_chirho
// ============================================================================

/// Allocate a fresh PML4 frame and copy the kernel mappings (upper half)
/// from the currently active page table.
///
/// Returns `Some((pml4_phys, pml4_phys))` with the physical address of the
/// new PML4 on success, or `None` if frame allocation fails.
///
/// The user-space half (entries 0..255) is zeroed. The kernel-space half
/// (entries 256..511) is copied from the current PML4 so that the kernel
/// is mapped identically in all address spaces.
pub fn create_user_page_table_chirho() -> Option<PhysAddr> {
    // Allocate a frame for the new PML4.
    let pml4_frame_chirho = {
        let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        alloc_lock_chirho.as_mut()?.allocate_frame()?
    };

    let pml4_phys_chirho = pml4_frame_chirho.start_address();

    // Get a reference to the new PML4 and zero it.
    let new_pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    // Zero all entries first.
    for entry_chirho in new_pml4_chirho.iter_mut() {
        entry_chirho.set_unused();
    }

    // Read the current (kernel) PML4 from CR3.
    let (current_pml4_frame_chirho, _flags_chirho) = Cr3::read();
    let current_pml4_chirho =
        unsafe { table_from_phys_chirho(current_pml4_frame_chirho.start_address()) };

    // Copy kernel-space entries (upper half: 256..511).
    for i_chirho in KERNEL_PML4_START_CHIRHO..ENTRIES_PER_TABLE_CHIRHO {
        let entry_chirho = &current_pml4_chirho[i_chirho];
        if !entry_chirho.is_unused() {
            // Share the kernel page table entries directly — all processes
            // see the same kernel mapping. We copy the raw entry value so
            // both PML4s point to the same PDPT/PD/PT frames.
            new_pml4_chirho[i_chirho].set_addr(
                entry_chirho.addr(),
                entry_chirho.flags(),
            );
        }
    }

    crate::serial_println_chirho!(
        "[PAGETABLE] Created user page table: PML4 phys={:#x}",
        pml4_phys_chirho.as_u64()
    );

    Some(pml4_phys_chirho)
}

// ============================================================================
// clone_page_table_chirho — deep-copy for fork with COW
// ============================================================================

/// Deep-copy a user-space page table for fork, setting up COW.
///
/// Walks the source PML4's user-space entries (0..255), recursively copies
/// the page table tree, and for leaf (4 KiB) pages that are writable:
/// - Marks them read-only in both the source and new page tables.
/// - Sets the COW bit (bit 9) so the page fault handler knows to copy
///   on write.
///
/// Returns the physical address of the cloned PML4, or `None` on OOM.
pub fn clone_page_table_chirho(source_pml4_phys_chirho: PhysAddr) -> Option<PhysAddr> {
    // Create a new PML4 with kernel mappings.
    let new_pml4_phys_chirho = create_user_page_table_chirho()?;

    let source_pml4_chirho = unsafe { table_from_phys_chirho(source_pml4_phys_chirho) };
    let new_pml4_chirho = unsafe { table_from_phys_chirho(new_pml4_phys_chirho) };

    // Clone user-space entries (0..255).
    for i_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &source_pml4_chirho[i_chirho];
        if entry_chirho.is_unused() {
            continue;
        }

        // Recursively clone the PDPT (level 3).
        let cloned_frame_chirho = clone_table_level_chirho(
            entry_chirho.addr(),
            entry_chirho.flags(),
            3, // PML4 points to level-3 tables (PDPTs)
            source_pml4_phys_chirho,
        )?;

        new_pml4_chirho[i_chirho].set_addr(cloned_frame_chirho, entry_chirho.flags());
    }

    crate::serial_println_chirho!(
        "[PAGETABLE] Cloned page table: source={:#x} -> new={:#x}",
        source_pml4_phys_chirho.as_u64(),
        new_pml4_phys_chirho.as_u64()
    );

    Some(new_pml4_phys_chirho)
}

/// Recursively clone a page table at a given level.
///
/// - `level == 1`: This is a PT (leaf level). For each present entry,
///   mark writable user pages as read-only + COW in both source and clone.
/// - `level > 1`: Allocate a new frame for the table, recurse into children.
///
/// Returns the physical address of the cloned table frame, or `None` on OOM.
fn clone_table_level_chirho(
    source_table_phys_chirho: PhysAddr,
    _parent_flags_chirho: PageTableFlags,
    level_chirho: u8,
    source_pml4_phys_chirho: PhysAddr,
) -> Option<PhysAddr> {
    // Allocate a new frame for this table level.
    let new_frame_chirho = {
        let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        alloc_lock_chirho.as_mut()?.allocate_frame()?
    };
    let new_table_phys_chirho = new_frame_chirho.start_address();

    // Zero the new table.
    let new_table_chirho = unsafe { table_from_phys_chirho(new_table_phys_chirho) };
    for entry_chirho in new_table_chirho.iter_mut() {
        entry_chirho.set_unused();
    }

    let source_table_chirho = unsafe { table_from_phys_chirho(source_table_phys_chirho) };

    for i_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
        if source_table_chirho[i_chirho].is_unused() {
            continue;
        }

        let entry_addr_chirho = source_table_chirho[i_chirho].addr();
        let flags_chirho = source_table_chirho[i_chirho].flags();

        // Check for huge pages (bit 7 = PS/PAT). If set at level 2 or 3,
        // this is a huge page mapping. We don't support COW on huge pages
        // yet — just share them directly.
        if level_chirho > 1 && flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            new_table_chirho[i_chirho].set_addr(entry_addr_chirho, flags_chirho);
            continue;
        }

        if level_chirho == 1 {
            // Leaf level (PT entries) — these point to actual 4 KiB frames.
            // For COW: if the page is user-accessible and writable, mark it
            // read-only and set the COW bit in both source and clone.
            let is_user_chirho = flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE);
            let is_writable_chirho = flags_chirho.contains(PageTableFlags::WRITABLE);

            if is_user_chirho && is_writable_chirho {
                // Remove WRITABLE, add COW bit.
                let cow_flags_chirho = (flags_chirho - PageTableFlags::WRITABLE)
                    | PageTableFlags::from_bits_truncate(COW_BIT_CHIRHO);

                // Update the source entry (parent's PT) to be COW too.
                source_table_chirho[i_chirho].set_addr(entry_addr_chirho, cow_flags_chirho);

                // Set the clone entry with the same COW flags.
                new_table_chirho[i_chirho].set_addr(entry_addr_chirho, cow_flags_chirho);
            } else {
                // Read-only or non-user page — share directly.
                new_table_chirho[i_chirho].set_addr(entry_addr_chirho, flags_chirho);
            }
        } else {
            // Intermediate level — recurse.
            let child_phys_chirho = clone_table_level_chirho(
                entry_addr_chirho,
                flags_chirho,
                level_chirho - 1,
                source_pml4_phys_chirho,
            )?;
            new_table_chirho[i_chirho].set_addr(child_phys_chirho, flags_chirho);
        }
    }

    Some(new_table_phys_chirho)
}

// ============================================================================
// switch_page_table_chirho
// ============================================================================

/// Switch the active address space by writing a new PML4 physical address
/// to the CR3 register.
///
/// This flushes the entire TLB, which is the standard behaviour on x86_64
/// when CR3 is written.
///
/// # Safety
///
/// The caller must ensure that `pml4_phys_chirho` points to a valid PML4
/// frame with correct kernel mappings (upper half), so that the kernel
/// can continue executing after the switch.
pub unsafe fn switch_page_table_chirho(pml4_phys_chirho: PhysAddr) {
    let frame_chirho =
        PhysFrame::containing_address(pml4_phys_chirho);

    // Write to CR3. The x86_64 crate's Cr3::write takes a frame and flags.
    // We use the default flags (no PCID, no write-through).
    Cr3::write(
        frame_chirho,
        Cr3::read().1, // preserve existing CR3 flags
    );
}

// ============================================================================
// COW page fault handler
// ============================================================================

/// Handle a Copy-On-Write page fault.
///
/// Called from the page fault handler when a write fault occurs on a page
/// marked with the COW bit. This function:
/// 1. Walks the current page table to find the faulting PTE.
/// 2. Allocates a new physical frame.
/// 3. Copies the contents of the old frame to the new one.
/// 4. Updates the PTE to point to the new frame with WRITABLE set and
///    COW bit cleared.
///
/// Returns `true` if the fault was successfully handled (was a COW fault),
/// `false` if the fault was not a COW situation (should be treated as a
/// real page fault / segfault).
pub fn handle_cow_fault_chirho(faulting_addr_chirho: VirtAddr) -> bool {
    // Read the current PML4 from CR3.
    let (pml4_frame_chirho, _cr3_flags_chirho) = Cr3::read();
    let pml4_phys_chirho = pml4_frame_chirho.start_address();

    // Walk the page table to find the PTE for the faulting address.
    let pte_result_chirho = walk_page_table_chirho(pml4_phys_chirho, faulting_addr_chirho);

    let pte_ptr_chirho = match pte_result_chirho {
        Some(ptr_chirho) => ptr_chirho,
        None => return false, // No PTE found — not a COW fault.
    };

    let pte_chirho = unsafe { &*pte_ptr_chirho };
    let flags_chirho = pte_chirho.flags();

    // Check if this is actually a COW page (has our COW bit set).
    if flags_chirho.bits() & COW_BIT_CHIRHO == 0 {
        return false; // Not a COW page — genuine fault.
    }

    // The page is COW. Allocate a new frame, copy data, remap writable.
    let old_frame_phys_chirho = pte_chirho.addr();

    let new_frame_chirho = {
        let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        match alloc_lock_chirho.as_mut().and_then(|a_chirho| a_chirho.allocate_frame()) {
            Some(f_chirho) => f_chirho,
            None => {
                crate::serial_println_chirho!(
                    "[PAGETABLE] COW: OOM — cannot allocate frame for {:#x}",
                    faulting_addr_chirho.as_u64()
                );
                return false;
            }
        }
    };

    let new_frame_phys_chirho = new_frame_chirho.start_address();

    // Copy the old frame's contents to the new frame.
    let old_virt_chirho = phys_to_virt_chirho(old_frame_phys_chirho);
    let new_virt_chirho = phys_to_virt_chirho(new_frame_phys_chirho);

    unsafe {
        core::ptr::copy_nonoverlapping(
            old_virt_chirho.as_ptr::<u8>(),
            new_virt_chirho.as_mut_ptr::<u8>(),
            PAGE_SIZE_CHIRHO as usize,
        );
    }

    // Update the PTE: point to new frame, set WRITABLE, clear COW bit.
    let new_flags_chirho = (flags_chirho | PageTableFlags::WRITABLE)
        & !PageTableFlags::from_bits_truncate(COW_BIT_CHIRHO);

    unsafe {
        (*pte_ptr_chirho).set_addr(new_frame_phys_chirho, new_flags_chirho);
    }

    // Flush the TLB entry for this page.
    x86_64::instructions::tlb::flush(faulting_addr_chirho);

    crate::serial_println_chirho!(
        "[PAGETABLE] COW resolved: addr={:#x}, old_frame={:#x}, new_frame={:#x}",
        faulting_addr_chirho.as_u64(),
        old_frame_phys_chirho.as_u64(),
        new_frame_phys_chirho.as_u64()
    );

    true
}

/// Walk a 4-level page table to find the PTE (level 1 entry) for a given
/// virtual address. Returns a raw mutable pointer to the PTE, or `None`
/// if any level is not present.
fn walk_page_table_chirho(
    pml4_phys_chirho: PhysAddr,
    addr_chirho: VirtAddr,
) -> Option<*mut PageTableEntry> {
    let addr_u64_chirho = addr_chirho.as_u64();

    // Extract the indices for each level from the virtual address.
    let pml4_idx_chirho = ((addr_u64_chirho >> 39) & 0x1FF) as usize;
    let pdpt_idx_chirho = ((addr_u64_chirho >> 30) & 0x1FF) as usize;
    let pd_idx_chirho = ((addr_u64_chirho >> 21) & 0x1FF) as usize;
    let pt_idx_chirho = ((addr_u64_chirho >> 12) & 0x1FF) as usize;

    // Level 4: PML4
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    let pml4_entry_chirho = &pml4_chirho[pml4_idx_chirho];
    if pml4_entry_chirho.is_unused() || !pml4_entry_chirho.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    // Level 3: PDPT
    let pdpt_chirho = unsafe { table_from_phys_chirho(pml4_entry_chirho.addr()) };
    let pdpt_entry_chirho = &pdpt_chirho[pdpt_idx_chirho];
    if pdpt_entry_chirho.is_unused() || !pdpt_entry_chirho.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }
    // Check for 1 GiB huge page.
    if pdpt_entry_chirho.flags().contains(PageTableFlags::HUGE_PAGE) {
        return None; // COW not supported on huge pages.
    }

    // Level 2: PD
    let pd_chirho = unsafe { table_from_phys_chirho(pdpt_entry_chirho.addr()) };
    let pd_entry_chirho = &pd_chirho[pd_idx_chirho];
    if pd_entry_chirho.is_unused() || !pd_entry_chirho.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }
    // Check for 2 MiB huge page.
    if pd_entry_chirho.flags().contains(PageTableFlags::HUGE_PAGE) {
        return None; // COW not supported on huge pages.
    }

    // Level 1: PT
    let pt_chirho = unsafe { table_from_phys_chirho(pd_entry_chirho.addr()) };
    let pt_entry_chirho = &mut pt_chirho[pt_idx_chirho];
    if pt_entry_chirho.is_unused() || !pt_entry_chirho.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    Some(pt_entry_chirho as *mut PageTableEntry)
}

// ============================================================================
// get_current_pml4_phys_chirho
// ============================================================================

/// Read the current PML4 physical address from CR3.
pub fn get_current_pml4_phys_chirho() -> PhysAddr {
    let (frame_chirho, _flags_chirho) = Cr3::read();
    frame_chirho.start_address()
}
