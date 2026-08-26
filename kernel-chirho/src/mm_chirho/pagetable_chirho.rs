// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Per-process page table management for the Lineluya kernel.
//!
//! Provides:
//! - [`PHYS_MEM_OFFSET_CHIRHO`] — stored physical memory offset from the
//!   bootloader so we can translate physical addresses to virtual ones.
//! - [`create_user_page_table_chirho`] — allocate a fresh PML4, copy kernel
//!   mappings (upper half), return the root physical address.
//! - [`clone_page_table_chirho`] — clone a user page table, marking
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
use kernel_core_chirho::frame_ownership_chirho::{
    FrameReleaseErrorChirho, FrameReleaseOutcomeChirho, FrameRetainErrorChirho,
    FrameRetainOutcomeChirho,
};

#[path = "address_space_chirho.rs"]
mod address_space_chirho;
#[path = "address_space_build_chirho.rs"]
mod address_space_build_chirho;
pub use address_space_chirho::{
    handle_refcount_failures_chirho, init_leaf_frame_ownership_chirho,
    register_existing_user_mappings_chirho, unretired_last_handle_drops_chirho,
    AddressSpaceHandleChirho,
    AddressSpaceOwnershipInitErrorChirho, AddressSpaceOwnershipInitStatsChirho,
    AddressSpaceRetireErrorChirho, AddressSpaceRetireReasonChirho,
    AddressSpaceRetireStatsChirho, AddressSpaceShareErrorChirho,
    PageTableRetireErrorChirho, UserMappingRegistrationErrorChirho,
};
pub use address_space_build_chirho::{
    clone_page_table_chirho, clone_user_address_space_chirho,
    create_user_address_space_chirho, create_user_page_table_chirho,
    try_clone_page_table_chirho, AddressSpaceBuildErrorChirho,
    PageTableCloneErrorChirho,
};

// ============================================================================
// Constants
// ============================================================================

/// Page size (4 KiB).
pub(super) const PAGE_SIZE_CHIRHO: u64 = 4096;

/// Number of entries in a page table level (512 for x86_64 4-level paging).
pub(super) const ENTRIES_PER_TABLE_CHIRHO: usize = 512;

/// The boundary between user-space and kernel-space in the PML4.
/// Entries 0..255 are user-space, 256..511 are kernel-space.
pub(super) const KERNEL_PML4_START_CHIRHO: usize = 256;

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

/// Physical address of the boot PML4. Saved at boot so the page fault
/// handler can lazily migrate user-space mappings from the boot PT to
/// per-process page tables.
static BOOT_PML4_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Physical address of the fresh PDPT allocated for the module arena.
/// Set by `map_page_raw_chirho` during boot, read by the insmod path
/// to fix PML4[511] in per-process page tables.
static MODULE_ARENA_PDPT_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Get the module arena's fresh PDPT physical address.
pub fn module_arena_pdpt_phys_chirho() -> u64 {
    MODULE_ARENA_PDPT_PHYS_CHIRHO.load(Ordering::Relaxed)
}

/// Full PML4[511] entry value (phys + flags) for direct fixup.
static MODULE_ARENA_PML4_ENTRY_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Store the verified PML4[511] entry for insmod fixup.
pub fn set_module_arena_pml4_entry_chirho(entry_chirho: u64) {
    MODULE_ARENA_PML4_ENTRY_CHIRHO.store(entry_chirho, Ordering::Relaxed);
}

/// Get the verified PML4[511] entry.
pub fn module_arena_pml4_entry_chirho() -> u64 {
    MODULE_ARENA_PML4_ENTRY_CHIRHO.load(Ordering::Relaxed)
}

/// Store the physical memory offset. Called once during kernel init.
pub fn set_phys_mem_offset_chirho(offset_chirho: u64) {
    PHYS_MEM_OFFSET_CHIRHO.store(offset_chirho, Ordering::Release);
}

/// Retrieve the physical memory offset.
pub fn phys_mem_offset_chirho() -> u64 {
    PHYS_MEM_OFFSET_CHIRHO.load(Ordering::Acquire)
}

/// Save the boot PML4 physical address. Called once at boot before any
/// per-process page tables are created.
pub fn save_boot_pml4_chirho() {
    let (frame_chirho, _) = Cr3::read();
    BOOT_PML4_PHYS_CHIRHO.store(frame_chirho.start_address().as_u64(), Ordering::Release);
}

/// Return the boot PML4 physical address.
pub fn get_boot_pml4_chirho() -> PhysAddr {
    PhysAddr::new(BOOT_PML4_PHYS_CHIRHO.load(Ordering::Acquire))
}

/// Look up a virtual address in the BOOT page table. If mapped, returns
/// (physical_address, flags). Used by the page fault handler for lazy
/// migration of user-space pages to per-process page tables.
pub fn lookup_in_boot_pt_chirho(vaddr_chirho: u64) -> Option<(u64, PageTableFlags)> {
    let boot_pml4_chirho = BOOT_PML4_PHYS_CHIRHO.load(Ordering::Acquire);
    if boot_pml4_chirho == 0 {
        return None;
    }
    let boot_pml4_phys_chirho = PhysAddr::new(boot_pml4_chirho);

    // Walk the boot PT to find the mapping.
    let pte_ptr_chirho = walk_page_table_chirho(boot_pml4_phys_chirho, VirtAddr::new(vaddr_chirho))?;
    let pte_chirho = unsafe { &*pte_ptr_chirho };
    if pte_chirho.is_unused() {
        return None;
    }
    Some((pte_chirho.addr().as_u64(), pte_chirho.flags()))
}

/// Look up a virtual address in a SPECIFIC page table (by PML4 phys).
pub fn lookup_in_pt_chirho(pml4_phys_chirho: PhysAddr, vaddr_chirho: u64) -> Option<(u64, PageTableFlags)> {
    let pte_ptr_chirho = walk_page_table_chirho(pml4_phys_chirho, VirtAddr::new(vaddr_chirho))?;
    let pte_chirho = unsafe { &*pte_ptr_chirho };
    if pte_chirho.is_unused() { return None; }
    Some((pte_chirho.addr().as_u64(), pte_chirho.flags()))
}

/// Convert a physical address to a virtual address using the stored offset.
#[inline]
fn phys_to_virt_chirho(phys_chirho: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys_chirho.as_u64() + phys_mem_offset_chirho())
}

// ============================================================================
// Safe page table operation wrappers (audit unsafe-001)
// ============================================================================

/// Read a PML4 entry at the given index from a physical PML4 address.
/// Returns the raw u64 entry value.
pub fn read_pml4_entry_chirho(pml4_phys_chirho: u64, index_chirho: usize) -> u64 {
    assert!(index_chirho < 512, "PML4 index out of range");
    let phys_offset_chirho = phys_mem_offset_chirho();
    let table_ptr_chirho = (phys_offset_chirho + pml4_phys_chirho) as *const u64;
    unsafe { core::ptr::read_volatile(table_ptr_chirho.add(index_chirho)) }
}

/// Write a PML4 entry at the given index to a physical PML4 address.
pub fn write_pml4_entry_chirho(pml4_phys_chirho: u64, index_chirho: usize, value_chirho: u64) {
    assert!(index_chirho < 512, "PML4 index out of range");
    let phys_offset_chirho = phys_mem_offset_chirho();
    let table_ptr_chirho = (phys_offset_chirho + pml4_phys_chirho) as *mut u64;
    unsafe { core::ptr::write_volatile(table_ptr_chirho.add(index_chirho), value_chirho) }
}

/// Read a page table entry at any level (PML4/PDPT/PD/PT) given
/// the table's physical address and the entry index.
pub fn read_pt_entry_chirho(table_phys_chirho: u64, index_chirho: usize) -> u64 {
    assert!(index_chirho < 512, "Page table index out of range");
    let phys_offset_chirho = phys_mem_offset_chirho();
    let ptr_chirho = (phys_offset_chirho + table_phys_chirho) as *const u64;
    unsafe { core::ptr::read_volatile(ptr_chirho.add(index_chirho)) }
}

/// Write a page table entry at any level.
pub fn write_pt_entry_chirho(table_phys_chirho: u64, index_chirho: usize, value_chirho: u64) {
    assert!(index_chirho < 512, "Page table index out of range");
    let phys_offset_chirho = phys_mem_offset_chirho();
    let ptr_chirho = (phys_offset_chirho + table_phys_chirho) as *mut u64;
    unsafe { core::ptr::write_volatile(ptr_chirho.add(index_chirho), value_chirho) }
}

/// Extract the physical address from a raw page table entry (any level).
/// Masks out the flag bits, returning only the 4 KiB-aligned physical address.
#[inline]
pub fn pt_entry_addr_chirho(entry_chirho: u64) -> u64 {
    entry_chirho & 0x000F_FFFF_FFFF_F000
}

/// Check whether a raw page table entry is present (bit 0 set).
#[inline]
pub fn pt_entry_is_present_chirho(entry_chirho: u64) -> bool {
    entry_chirho & 0x1 != 0
}

/// Check whether a raw page table entry has the HUGE_PAGE / PS bit set (bit 7).
#[inline]
pub fn pt_entry_is_huge_chirho(entry_chirho: u64) -> bool {
    entry_chirho & (1 << 7) != 0
}

/// Translate a virtual address to its physical address by walking the page table.
/// Returns None if the address is not mapped.
///
/// Uses safe wrappers `read_pt_entry_chirho` / `pt_entry_addr_chirho` /
/// `pt_entry_is_present_chirho` / `pt_entry_is_huge_chirho` from audit
/// unsafe-001 instead of raw pointer arithmetic.
pub fn virt_to_phys_chirho(virt_chirho: u64) -> Option<u64> {
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
        let raw_entry_chirho = read_pt_entry_chirho(table_phys_chirho, idx_chirho);

        if !pt_entry_is_present_chirho(raw_entry_chirho) {
            return None;
        }

        let entry_phys_chirho = pt_entry_addr_chirho(raw_entry_chirho);

        // Check for huge pages (1GiB at level 1, 2MiB at level 2)
        if pt_entry_is_huge_chirho(raw_entry_chirho) {
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
pub(super) unsafe fn table_from_phys_chirho(phys_chirho: PhysAddr) -> &'static mut PageTable {
    let virt_chirho = phys_to_virt_chirho(phys_chirho);
    &mut *(virt_chirho.as_mut_ptr::<PageTable>())
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

    // Codex-directed: rebind GLOBAL_MAPPER to the new CR3 so that
    // mmap/brk/mprotect operate on the active page table, not the
    // stale boot PML4. Without this, PID 4's runtime mmap corrupts
    // boot PML4, breaking PID 3's lazy-mirrored musl library pages.
    crate::mm_chirho::reinit_mapper_for_current_cr3_chirho();
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
    // Workflow: spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md
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

    // An exclusive COW leaf needs no copy. This is the common path after the
    // sibling mapping has already exited or unmapped.
    let old_frame_phys_chirho = pte_chirho.addr();
    match address_space_chirho::leaf_mapping_count_chirho(old_frame_phys_chirho) {
        Some(1) => {
            let mut exclusive_flags_chirho = flags_chirho;
            exclusive_flags_chirho.insert(PageTableFlags::WRITABLE);
            exclusive_flags_chirho.remove(PageTableFlags::BIT_9);
            unsafe {
                (*pte_ptr_chirho).set_addr(old_frame_phys_chirho, exclusive_flags_chirho);
            }
            x86_64::instructions::tlb::flush(faulting_addr_chirho);
            return true;
        }
        Some(shared_count_chirho) if shared_count_chirho > 1 => {}
        Some(_) => {
            crate::serial_println_chirho!(
                "[COW] zero ownership count for frame {:#x}",
                old_frame_phys_chirho.as_u64(),
            );
            return false;
        }
        None => {
            crate::serial_println_chirho!(
                "[COW] unmanaged frame marked COW at {:#x}",
                old_frame_phys_chirho.as_u64(),
            );
            return false;
        }
    }

    let new_frame_chirho = {
        // A synchronous page fault must never spin on a lock held by the code
        // it interrupted. The caller treats allocator contention as a failed
        // COW resolution rather than self-deadlocking the CPU.
        let Some(mut alloc_guard_chirho) = GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() else {
            crate::serial_println_chirho!(
                "[COW] frame allocator busy at {:#x}",
                faulting_addr_chirho.as_u64(),
            );
            return false;
        };
        match alloc_guard_chirho.as_mut().and_then(|a_chirho| a_chirho.allocate_frame()) {
            Some(f_chirho) => f_chirho,
            None => {
                crate::serial_println_chirho!(
                    "[COW] OOM — cannot allocate frame for {:#x}",
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

    let new_retain_chirho =
        address_space_chirho::retain_leaf_mapping_chirho(new_frame_phys_chirho);
    match new_retain_chirho {
        Ok(FrameRetainOutcomeChirho::ManagedChirho { .. }) => {}
        Ok(FrameRetainOutcomeChirho::UnmanagedChirho) => {
            crate::serial_println_chirho!(
                "[COW] allocator returned unmanaged frame {:#x}",
                new_frame_phys_chirho.as_u64(),
            );
            crate::mm_chirho::deallocate_frame_chirho(new_frame_chirho);
            return false;
        }
        Err(retain_error_chirho) => {
            crate::serial_println_chirho!(
                "[COW] retain failed for frame {:#x}: {:?}",
                new_frame_phys_chirho.as_u64(),
                retain_error_chirho,
            );
            crate::mm_chirho::deallocate_frame_chirho(new_frame_chirho);
            return false;
        }
    }

    // Update the PTE: point to new frame, set WRITABLE, clear COW bit.
    let mut new_flags_chirho = flags_chirho;
    new_flags_chirho.insert(PageTableFlags::WRITABLE);
    new_flags_chirho.remove(PageTableFlags::BIT_9);

    unsafe {
        (*pte_ptr_chirho).set_addr(new_frame_phys_chirho, new_flags_chirho);
    }

    let old_release_chirho =
        address_space_chirho::release_leaf_mapping_chirho(old_frame_phys_chirho);
    let old_release_chirho = match old_release_chirho {
        Ok(outcome_chirho) => outcome_chirho,
        Err(release_error_chirho) => {
            unsafe {
                (*pte_ptr_chirho).set_addr(old_frame_phys_chirho, flags_chirho);
            }
            match address_space_chirho::release_leaf_mapping_chirho(new_frame_phys_chirho) {
                Ok(FrameReleaseOutcomeChirho::LastReferenceChirho) => {
                    crate::mm_chirho::deallocate_frame_chirho(new_frame_chirho);
                }
                rollback_chirho => {
                    crate::serial_println_chirho!(
                        "[COW] new-frame rollback failed: {:?}",
                        rollback_chirho,
                    );
                }
            }
            x86_64::instructions::tlb::flush(faulting_addr_chirho);
            crate::serial_println_chirho!(
                "[COW] old-frame release failed: {:?}",
                release_error_chirho,
            );
            return false;
        }
    };
    if old_release_chirho == FrameReleaseOutcomeChirho::LastReferenceChirho {
        crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
            old_frame_phys_chirho,
        ));
    }

    // Flush the TLB entry for this page.
    x86_64::instructions::tlb::flush(faulting_addr_chirho);

    true
}

/// Mark all writable user pages in a page table as COW (read-only + COW bit).
///
/// Called at fork time on the boot PML4 so that when the parent (which
/// continues on boot PML4) writes to any shared page, the COW fault handler
/// allocates a new frame and copies the data. The fork child gets its own
/// PT via `clone_page_table_chirho` with separate page-table frames.
///
/// Returns the number of pages marked as COW.
pub fn mark_user_pages_cow_chirho(pml4_phys_chirho: PhysAddr) -> u64 {
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    let mut marked_chirho: u64 = 0;

    for pml4_idx_chirho in 0..KERNEL_PML4_START_CHIRHO {
        if pml4_chirho[pml4_idx_chirho].is_unused() { continue; }
        let pdpt_flags_chirho = pml4_chirho[pml4_idx_chirho].flags();
        if !pdpt_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

        let pdpt_chirho = unsafe { table_from_phys_chirho(pml4_chirho[pml4_idx_chirho].addr()) };
        for pdpt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
            if pdpt_chirho[pdpt_idx_chirho].is_unused() { continue; }
            let pd_flags_chirho = pdpt_chirho[pdpt_idx_chirho].flags();
            if !pd_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }
            if pd_flags_chirho.contains(PageTableFlags::HUGE_PAGE) { continue; }

            let pd_chirho = unsafe { table_from_phys_chirho(pdpt_chirho[pdpt_idx_chirho].addr()) };
            for pd_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                if pd_chirho[pd_idx_chirho].is_unused() { continue; }
                let pt_flags_chirho = pd_chirho[pd_idx_chirho].flags();
                if !pt_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }
                if pt_flags_chirho.contains(PageTableFlags::HUGE_PAGE) { continue; }

                let pt_chirho = unsafe { table_from_phys_chirho(pd_chirho[pd_idx_chirho].addr()) };
                for pt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                    if pt_chirho[pt_idx_chirho].is_unused() { continue; }
                    let page_flags_chirho = pt_chirho[pt_idx_chirho].flags();
                    if !page_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }
                    if !page_flags_chirho.contains(PageTableFlags::WRITABLE) { continue; }

                    // Device/MMIO frames are outside the allocator-owned flat
                    // table and remain shared. No device-specific address
                    // heuristic belongs in the generic COW path.
                    let page_phys_chirho = pt_chirho[pt_idx_chirho].addr();
                    match address_space_chirho::leaf_mapping_count_chirho(page_phys_chirho) {
                        None => continue,
                        Some(0) => {
                            crate::serial_println_chirho!(
                                "[COW] unregistered writable leaf {:#x}",
                                page_phys_chirho.as_u64(),
                            );
                            continue;
                        }
                        Some(_) => {}
                    }

                    // Mark as COW: remove WRITABLE, add COW bit
                    let cow_flags_chirho = (page_flags_chirho & !PageTableFlags::WRITABLE)
                        | PageTableFlags::from_bits_truncate(COW_BIT_CHIRHO);
                    pt_chirho[pt_idx_chirho].set_addr(page_phys_chirho, cow_flags_chirho);
                    marked_chirho += 1;
                }
            }
        }
    }

    // Flush TLB since we changed page permissions
    unsafe { x86_64::registers::control::Cr3::write(
        x86_64::registers::control::Cr3::read().0,
        x86_64::registers::control::Cr3::read().1,
    ); }

    marked_chirho
}

/// Walk a 4-level page table to find the PTE (level 1 entry) for a given
/// virtual address. Returns a raw mutable pointer to the PTE, or `None`
/// if any level is not present.
///
/// Uses safe wrappers `read_pt_entry_chirho` / `pt_entry_is_present_chirho` /
/// `pt_entry_addr_chirho` / `pt_entry_is_huge_chirho` from audit unsafe-001
/// for the intermediate level reads (PML4, PDPT, PD).  The final PT level
/// still needs `table_from_phys_chirho` because the caller requires a
/// mutable pointer for COW write-back.
pub fn walk_page_table_chirho(
    pml4_phys_chirho: PhysAddr,
    addr_chirho: VirtAddr,
) -> Option<*mut PageTableEntry> {
    let addr_u64_chirho = addr_chirho.as_u64();

    // Extract the indices for each level from the virtual address.
    let pml4_idx_chirho = ((addr_u64_chirho >> 39) & 0x1FF) as usize;
    let pdpt_idx_chirho = ((addr_u64_chirho >> 30) & 0x1FF) as usize;
    let pd_idx_chirho = ((addr_u64_chirho >> 21) & 0x1FF) as usize;
    let pt_idx_chirho = ((addr_u64_chirho >> 12) & 0x1FF) as usize;

    // Level 4: PML4 — safe read via wrapper
    let pml4_raw_chirho = read_pt_entry_chirho(pml4_phys_chirho.as_u64(), pml4_idx_chirho);
    if !pt_entry_is_present_chirho(pml4_raw_chirho) {
        return None;
    }
    let pdpt_phys_chirho = pt_entry_addr_chirho(pml4_raw_chirho);

    // Level 3: PDPT — safe read via wrapper
    let pdpt_raw_chirho = read_pt_entry_chirho(pdpt_phys_chirho, pdpt_idx_chirho);
    if !pt_entry_is_present_chirho(pdpt_raw_chirho) {
        return None;
    }
    // Check for 1 GiB huge page.
    if pt_entry_is_huge_chirho(pdpt_raw_chirho) {
        return None; // COW not supported on huge pages.
    }
    let pd_phys_chirho = pt_entry_addr_chirho(pdpt_raw_chirho);

    // Level 2: PD — safe read via wrapper
    let pd_raw_chirho = read_pt_entry_chirho(pd_phys_chirho, pd_idx_chirho);
    if !pt_entry_is_present_chirho(pd_raw_chirho) {
        return None;
    }
    // Check for 2 MiB huge page.
    if pt_entry_is_huge_chirho(pd_raw_chirho) {
        return None; // COW not supported on huge pages.
    }
    let pt_phys_chirho = pt_entry_addr_chirho(pd_raw_chirho);

    // Level 1: PT — needs mutable pointer for COW write-back, so we
    // still use table_from_phys_chirho here.
    let pt_chirho = unsafe { table_from_phys_chirho(PhysAddr::new(pt_phys_chirho)) };
    let pt_entry_chirho = &mut pt_chirho[pt_idx_chirho];
    if pt_entry_chirho.is_unused() {
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

/// Mirror all user-space page mappings from the current (active) page table
/// into a target page table.
///
/// Walks the current PML4's user-space entries (0..255) and for every
/// present leaf PTE, creates the same mapping in `target_pml4_phys_chirho`.
/// This allows execve to load ELF segments using the global mapper (which
/// maps into the current CR3) and then transfer all mappings to a per-process
/// page table before switching CR3.
///
/// Returns the number of pages mirrored.
pub fn mirror_user_mappings_chirho(target_pml4_phys_chirho: PhysAddr) -> usize {
    let (current_pml4_frame_chirho, _) = Cr3::read();
    let source_pml4_phys_chirho = current_pml4_frame_chirho.start_address();

    let source_pml4_chirho = unsafe { table_from_phys_chirho(source_pml4_phys_chirho) };
    let mut count_chirho: usize = 0;

    for pml4_idx_chirho in 0..KERNEL_PML4_START_CHIRHO {
        if source_pml4_chirho[pml4_idx_chirho].is_unused() {
            continue;
        }
        let pdpt_phys_chirho = source_pml4_chirho[pml4_idx_chirho].addr();
        let pdpt_chirho = unsafe { table_from_phys_chirho(pdpt_phys_chirho) };

        for pdpt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
            if pdpt_chirho[pdpt_idx_chirho].is_unused() {
                continue;
            }
            if pdpt_chirho[pdpt_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) {
                continue; // Skip 1 GiB huge pages
            }
            let pd_phys_chirho = pdpt_chirho[pdpt_idx_chirho].addr();
            let pd_chirho = unsafe { table_from_phys_chirho(pd_phys_chirho) };

            for pd_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                if pd_chirho[pd_idx_chirho].is_unused() {
                    continue;
                }
                if pd_chirho[pd_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) {
                    continue; // Skip 2 MiB huge pages
                }
                let pt_phys_chirho = pd_chirho[pd_idx_chirho].addr();
                let pt_chirho = unsafe { table_from_phys_chirho(pt_phys_chirho) };

                for pt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                    if pt_chirho[pt_idx_chirho].is_unused() {
                        continue;
                    }
                    let page_phys_chirho = pt_chirho[pt_idx_chirho].addr();
                    let flags_chirho = pt_chirho[pt_idx_chirho].flags();

                    // Only mirror USER_ACCESSIBLE pages (skip kernel-only)
                    if !flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
                        continue;
                    }

                    let vaddr_chirho = ((pml4_idx_chirho as u64) << 39)
                        | ((pdpt_idx_chirho as u64) << 30)
                        | ((pd_idx_chirho as u64) << 21)
                        | ((pt_idx_chirho as u64) << 12);

                    if map_page_in_pt_chirho(
                        target_pml4_phys_chirho,
                        vaddr_chirho,
                        page_phys_chirho.as_u64(),
                        flags_chirho,
                    )
                    .is_ok()
                    {
                        count_chirho += 1;
                    }
                }
            }
        }
    }

    count_chirho
}

// ============================================================================
// map_page_in_pt_chirho — map a page in a SPECIFIC page table
// ============================================================================

/// Failure modes for allocating and mapping one zero-filled demand page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandPageMapErrorChirho {
    AllocatorBusyChirho,
    AllocatorNotInitializedChirho,
    LeafFrameExhaustedChirho,
    PageTableFrameExhaustedChirho,
}

/// Transactional mapping failures. On every error the requested leaf frame is
/// still owned by the caller and no newly allocated intermediate table remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMapErrorChirho {
    UnalignedVirtualAddressChirho,
    UnalignedPhysicalAddressChirho,
    UserAddressOutOfRangeChirho,
    NonPresentIntermediateChirho,
    HugeIntermediateChirho,
    IntermediateFrameExhaustedChirho,
    LeafRetainChirho(FrameRetainErrorChirho),
    ReplacedLeafReleaseChirho(FrameReleaseErrorChirho),
    RollbackLeafReleaseChirho(FrameReleaseErrorChirho),
    OwnershipClassChangeChirho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPageUnmapOutcomeChirho {
    NotMappedChirho,
    NotUserMappingChirho,
    UnmanagedMappingChirho,
    StillReferencedChirho { references_chirho: u32 },
    FrameFreedChirho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPageUnmapErrorChirho {
    LeafReleaseChirho(FrameReleaseErrorChirho),
}

/// Allocate, zero, and map one demand page without recursively holding the
/// global frame-allocator lock.
///
/// [`map_page_in_pt_chirho`] may allocate intermediate page-table levels, so
/// its caller must not retain the allocator guard used for the leaf frame.
/// Keeping that ownership sequence here prevents a page fault at a new 2 MiB
/// boundary from self-deadlocking while the mapper creates its PT level.
pub fn map_zeroed_demand_page_chirho(
    pml4_phys_chirho: PhysAddr,
    vaddr_chirho: u64,
    flags_chirho: PageTableFlags,
) -> Result<PhysAddr, DemandPageMapErrorChirho> {
    let leaf_frame_chirho = {
        let Some(mut allocator_guard_chirho) = GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() else {
            return Err(DemandPageMapErrorChirho::AllocatorBusyChirho);
        };
        let Some(allocator_chirho) = allocator_guard_chirho.as_mut() else {
            return Err(DemandPageMapErrorChirho::AllocatorNotInitializedChirho);
        };
        allocator_chirho
            .allocate_frame()
            .ok_or(DemandPageMapErrorChirho::LeafFrameExhaustedChirho)?
    };
    let leaf_phys_chirho = leaf_frame_chirho.start_address();
    zero_frame_chirho(leaf_phys_chirho);

    if map_page_in_pt_chirho(
        pml4_phys_chirho,
        vaddr_chirho,
        leaf_phys_chirho.as_u64(),
        flags_chirho,
    )
    .is_err()
    {
        crate::mm_chirho::deallocate_frame_chirho(leaf_frame_chirho);
        return Err(DemandPageMapErrorChirho::PageTableFrameExhaustedChirho);
    }

    Ok(leaf_phys_chirho)
}

/// Map a 4 KiB virtual page to a physical frame in a specific page table.
///
/// Unlike the global mapper (which maps into the current CR3 page table),
/// this function can populate any PML4, making it suitable for preparing
/// a per-process address space before switching CR3.
///
/// Intermediate page table levels (PDPT, PD, PT) are allocated on demand
/// from the frame allocator.
/// Callers must not hold [`GLOBAL_FRAME_ALLOCATOR_CHIRHO`] because allocating
/// one of those levels acquires it internally.
///
/// Mapping replacement retains the new leaf before publishing it, then
/// releases the displaced leaf. This ordering prevents a transient zero count
/// when an existing physical frame is remapped to itself.
pub fn map_page_in_pt_chirho(
    pml4_phys_chirho: PhysAddr,
    vaddr_chirho: u64,
    paddr_chirho: u64,
    flags_chirho: PageTableFlags,
) -> Result<(), PageMapErrorChirho> {
    // Workflow: spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md
    if vaddr_chirho & (PAGE_SIZE_CHIRHO - 1) != 0 {
        return Err(PageMapErrorChirho::UnalignedVirtualAddressChirho);
    }
    if paddr_chirho & (PAGE_SIZE_CHIRHO - 1) != 0 {
        return Err(PageMapErrorChirho::UnalignedPhysicalAddressChirho);
    }
    let addr_chirho = vaddr_chirho;
    let pml4_idx_chirho = ((addr_chirho >> 39) & 0x1FF) as usize;
    let pdpt_idx_chirho = ((addr_chirho >> 30) & 0x1FF) as usize;
    let pd_idx_chirho = ((addr_chirho >> 21) & 0x1FF) as usize;
    let pt_idx_chirho = ((addr_chirho >> 12) & 0x1FF) as usize;

    if flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE)
        && pml4_idx_chirho >= KERNEL_PML4_START_CHIRHO
    {
        return Err(PageMapErrorChirho::UserAddressOutOfRangeChirho);
    }

    let mut intermediate_flags_chirho =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    if flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
        intermediate_flags_chirho.insert(PageTableFlags::USER_ACCESSIBLE);
    }

    let mut allocated_pdpt_chirho = false;
    let mut allocated_pd_chirho = false;
    let mut allocated_pt_chirho = false;

    // Level 4: PML4 → ensure PDPT exists
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    if pml4_chirho[pml4_idx_chirho].is_unused() {
        let frame_chirho = alloc_frame_chirho()
            .ok_or(PageMapErrorChirho::IntermediateFrameExhaustedChirho)?;
        zero_frame_chirho(frame_chirho);
        pml4_chirho[pml4_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
        allocated_pdpt_chirho = true;
    } else {
        let existing_flags_chirho = pml4_chirho[pml4_idx_chirho].flags();
        if !existing_flags_chirho.contains(PageTableFlags::PRESENT) {
            return Err(PageMapErrorChirho::NonPresentIntermediateChirho);
        }
    }
    let pdpt_phys_chirho = pml4_chirho[pml4_idx_chirho].addr();

    // Level 3: PDPT → ensure PD exists
    let pdpt_chirho = unsafe { table_from_phys_chirho(pdpt_phys_chirho) };
    if pdpt_chirho[pdpt_idx_chirho].is_unused() {
        let Some(frame_chirho) = alloc_frame_chirho() else {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                false,
                false,
            );
            return Err(PageMapErrorChirho::IntermediateFrameExhaustedChirho);
        };
        zero_frame_chirho(frame_chirho);
        pdpt_chirho[pdpt_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
        allocated_pd_chirho = true;
    } else {
        let existing_flags_chirho = pdpt_chirho[pdpt_idx_chirho].flags();
        if existing_flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                false,
                false,
            );
            return Err(PageMapErrorChirho::HugeIntermediateChirho);
        }
        if !existing_flags_chirho.contains(PageTableFlags::PRESENT) {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                false,
                false,
            );
            return Err(PageMapErrorChirho::NonPresentIntermediateChirho);
        }
    }
    let pd_phys_chirho = pdpt_chirho[pdpt_idx_chirho].addr();

    // Level 2: PD → ensure PT exists
    let pd_chirho = unsafe { table_from_phys_chirho(pd_phys_chirho) };
    if pd_chirho[pd_idx_chirho].is_unused() {
        let Some(frame_chirho) = alloc_frame_chirho() else {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                allocated_pd_chirho,
                false,
            );
            return Err(PageMapErrorChirho::IntermediateFrameExhaustedChirho);
        };
        zero_frame_chirho(frame_chirho);
        pd_chirho[pd_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
        allocated_pt_chirho = true;
    } else {
        let existing_flags_chirho = pd_chirho[pd_idx_chirho].flags();
        if existing_flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                allocated_pd_chirho,
                false,
            );
            return Err(PageMapErrorChirho::HugeIntermediateChirho);
        }
        if !existing_flags_chirho.contains(PageTableFlags::PRESENT) {
            rollback_new_mapping_tables_chirho(
                pml4_phys_chirho,
                pml4_idx_chirho,
                pdpt_idx_chirho,
                pd_idx_chirho,
                allocated_pdpt_chirho,
                allocated_pd_chirho,
                false,
            );
            return Err(PageMapErrorChirho::NonPresentIntermediateChirho);
        }
    }
    let pt_phys_chirho = pd_chirho[pd_idx_chirho].addr();

    // Level 1: publish the retained leaf, then retire any displaced leaf.
    let pt_chirho = unsafe { table_from_phys_chirho(pt_phys_chirho) };
    let old_unused_chirho = pt_chirho[pt_idx_chirho].is_unused();
    let old_phys_chirho = pt_chirho[pt_idx_chirho].addr();
    let old_flags_chirho = pt_chirho[pt_idx_chirho].flags();
    let old_owned_chirho = !old_unused_chirho
        && old_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE);
    let new_phys_chirho = PhysAddr::new(paddr_chirho);
    let new_owned_chirho = flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE);

    if old_owned_chirho && !new_owned_chirho && old_phys_chirho == new_phys_chirho {
        rollback_new_mapping_tables_chirho(
            pml4_phys_chirho,
            pml4_idx_chirho,
            pdpt_idx_chirho,
            pd_idx_chirho,
            allocated_pdpt_chirho,
            allocated_pd_chirho,
            allocated_pt_chirho,
        );
        return Err(PageMapErrorChirho::OwnershipClassChangeChirho);
    }

    let new_retain_chirho = if new_owned_chirho {
        match address_space_chirho::retain_leaf_mapping_chirho(new_phys_chirho) {
            Ok(outcome_chirho) => outcome_chirho,
            Err(retain_error_chirho) => {
                rollback_new_mapping_tables_chirho(
                    pml4_phys_chirho,
                    pml4_idx_chirho,
                    pdpt_idx_chirho,
                    pd_idx_chirho,
                    allocated_pdpt_chirho,
                    allocated_pd_chirho,
                    allocated_pt_chirho,
                );
                return Err(PageMapErrorChirho::LeafRetainChirho(retain_error_chirho));
            }
        }
    } else {
        FrameRetainOutcomeChirho::UnmanagedChirho
    };

    pt_chirho[pt_idx_chirho].set_addr(new_phys_chirho, flags_chirho);

    if old_owned_chirho {
        let old_release_chirho =
            address_space_chirho::release_leaf_mapping_chirho(old_phys_chirho);
        let old_release_chirho = match old_release_chirho {
            Ok(outcome_chirho) => outcome_chirho,
            Err(release_error_chirho) => {
                pt_chirho[pt_idx_chirho].set_addr(old_phys_chirho, old_flags_chirho);
                if new_retain_chirho != FrameRetainOutcomeChirho::UnmanagedChirho {
                    address_space_chirho::release_leaf_mapping_chirho(new_phys_chirho)
                        .map_err(PageMapErrorChirho::RollbackLeafReleaseChirho)?;
                }
                rollback_new_mapping_tables_chirho(
                    pml4_phys_chirho,
                    pml4_idx_chirho,
                    pdpt_idx_chirho,
                    pd_idx_chirho,
                    allocated_pdpt_chirho,
                    allocated_pd_chirho,
                    allocated_pt_chirho,
                );
                return Err(PageMapErrorChirho::ReplacedLeafReleaseChirho(
                    release_error_chirho,
                ));
            }
        };
        if old_release_chirho == FrameReleaseOutcomeChirho::LastReferenceChirho {
            zero_frame_chirho(old_phys_chirho);
            crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
                old_phys_chirho,
            ));
        }
    }

    if new_owned_chirho {
        let pml4_flags_chirho = pml4_chirho[pml4_idx_chirho].flags();
        if !pml4_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
            let pml4_addr_chirho = pml4_chirho[pml4_idx_chirho].addr();
            pml4_chirho[pml4_idx_chirho].set_addr(
                pml4_addr_chirho,
                pml4_flags_chirho | PageTableFlags::USER_ACCESSIBLE,
            );
        }
        let pdpt_flags_chirho = pdpt_chirho[pdpt_idx_chirho].flags();
        if !pdpt_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
            let pdpt_addr_chirho = pdpt_chirho[pdpt_idx_chirho].addr();
            pdpt_chirho[pdpt_idx_chirho].set_addr(
                pdpt_addr_chirho,
                pdpt_flags_chirho | PageTableFlags::USER_ACCESSIBLE,
            );
        }
        let pd_flags_chirho = pd_chirho[pd_idx_chirho].flags();
        if !pd_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
            let pd_addr_chirho = pd_chirho[pd_idx_chirho].addr();
            pd_chirho[pd_idx_chirho]
                .set_addr(pd_addr_chirho, pd_flags_chirho | PageTableFlags::USER_ACCESSIBLE);
        }
    }

    Ok(())
}

fn rollback_new_mapping_tables_chirho(
    pml4_phys_chirho: PhysAddr,
    pml4_idx_chirho: usize,
    pdpt_idx_chirho: usize,
    pd_idx_chirho: usize,
    allocated_pdpt_chirho: bool,
    allocated_pd_chirho: bool,
    allocated_pt_chirho: bool,
) {
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    let pdpt_phys_chirho = pml4_chirho[pml4_idx_chirho].addr();
    if allocated_pt_chirho {
        let pdpt_chirho = unsafe { table_from_phys_chirho(pdpt_phys_chirho) };
        let pd_phys_chirho = pdpt_chirho[pdpt_idx_chirho].addr();
        let pd_chirho = unsafe { table_from_phys_chirho(pd_phys_chirho) };
        let pt_phys_chirho = pd_chirho[pd_idx_chirho].addr();
        pd_chirho[pd_idx_chirho].set_unused();
        crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
            pt_phys_chirho,
        ));
    }
    if allocated_pd_chirho {
        let pdpt_chirho = unsafe { table_from_phys_chirho(pdpt_phys_chirho) };
        let pd_phys_chirho = pdpt_chirho[pdpt_idx_chirho].addr();
        pdpt_chirho[pdpt_idx_chirho].set_unused();
        crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
            pd_phys_chirho,
        ));
    }
    if allocated_pdpt_chirho {
        pml4_chirho[pml4_idx_chirho].set_unused();
        crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
            pdpt_phys_chirho,
        ));
    }
}

/// Clear one user leaf and release its physical-frame reference. The frame is
/// returned to the allocation-free intrusive free list only after the final
/// PTE disappears.
pub fn unmap_user_page_chirho(
    pml4_phys_chirho: PhysAddr,
    vaddr_chirho: VirtAddr,
) -> Result<UserPageUnmapOutcomeChirho, UserPageUnmapErrorChirho> {
    // Workflow: spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md
    let Some(pte_ptr_chirho) = walk_page_table_chirho(pml4_phys_chirho, vaddr_chirho) else {
        return Ok(UserPageUnmapOutcomeChirho::NotMappedChirho);
    };
    let pte_chirho = unsafe { &mut *pte_ptr_chirho };
    let flags_chirho = pte_chirho.flags();
    if !flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
        return Ok(UserPageUnmapOutcomeChirho::NotUserMappingChirho);
    }
    let physical_address_chirho = pte_chirho.addr();
    pte_chirho.set_unused();
    let release_chirho = match address_space_chirho::release_leaf_mapping_chirho(
        physical_address_chirho,
    ) {
        Ok(outcome_chirho) => outcome_chirho,
        Err(release_error_chirho) => {
            pte_chirho.set_addr(physical_address_chirho, flags_chirho);
            return Err(UserPageUnmapErrorChirho::LeafReleaseChirho(
                release_error_chirho,
            ));
        }
    };

    if Cr3::read().0.start_address() == pml4_phys_chirho {
        x86_64::instructions::tlb::flush(vaddr_chirho);
    }

    match release_chirho {
        FrameReleaseOutcomeChirho::UnmanagedChirho => {
            Ok(UserPageUnmapOutcomeChirho::UnmanagedMappingChirho)
        }
        FrameReleaseOutcomeChirho::StillReferencedChirho { references_chirho } => {
            Ok(UserPageUnmapOutcomeChirho::StillReferencedChirho { references_chirho })
        }
        FrameReleaseOutcomeChirho::LastReferenceChirho => {
            zero_frame_chirho(physical_address_chirho);
            crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
                physical_address_chirho,
            ));
            Ok(UserPageUnmapOutcomeChirho::FrameFreedChirho)
        }
    }
}

/// Allocate a single physical frame from the global frame allocator.
fn alloc_frame_chirho() -> Option<PhysAddr> {
    let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
    let frame_chirho = alloc_lock_chirho.as_mut()?.allocate_frame()?;
    Some(frame_chirho.start_address())
}

/// Zero a physical frame via the physical memory window.
fn zero_frame_chirho(phys_chirho: PhysAddr) {
    let virt_chirho = phys_to_virt_chirho(phys_chirho);
    unsafe {
        core::ptr::write_bytes(virt_chirho.as_mut_ptr::<u8>(), 0, 4096);
    }
}

// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/// Map a single 4KB page using raw u64 pointer arithmetic.
/// Bypasses x86_64 crate's PhysAddr validation (which panics on PML4[511]
/// entries with non-standard bit patterns from BIOS/bootloader setup).
/// Used to map module arena to high-canonical addresses for R_X86_64_32S.
pub fn map_page_raw_chirho(
    pml4_phys_chirho: u64,
    vaddr_chirho: u64,
    paddr_chirho: u64,
) -> Result<(), &'static str> {
    let phys_off_chirho = PHYS_MEM_OFFSET_CHIRHO.load(Ordering::Acquire);
    if phys_off_chirho == 0 {
        return Err("phys offset not set");
    }

    let pml4i_chirho = ((vaddr_chirho >> 39) & 0x1FF) as usize;
    let pdpti_chirho = ((vaddr_chirho >> 30) & 0x1FF) as usize;
    let pdi_chirho   = ((vaddr_chirho >> 21) & 0x1FF) as usize;
    let pti_chirho   = ((vaddr_chirho >> 12) & 0x1FF) as usize;

    let flags_inter_chirho: u64 = 0x03; // PRESENT | WRITABLE
    let flags_leaf_chirho: u64  = 0x03; // PRESENT | WRITABLE

    // Read/write a PTE as raw u64
    let read_entry_chirho = |table_phys: u64, idx: usize| -> u64 {
        let ptr_chirho = (table_phys + phys_off_chirho) as *const u64;
        unsafe { core::ptr::read_volatile(ptr_chirho.add(idx)) }
    };
    let write_entry_chirho = |table_phys: u64, idx: usize, val: u64| {
        let ptr_chirho = (table_phys + phys_off_chirho) as *mut u64;
        unsafe { core::ptr::write_volatile(ptr_chirho.add(idx), val); }
    };
    let addr_from_entry_chirho = |e: u64| -> u64 { e & 0x000F_FFFF_FFFF_F000 };

    // Allocate a zeroed frame (returns PhysAddr, convert to u64 immediately)
    let new_frame_chirho = || -> Result<u64, &'static str> {
        let pa_chirho = alloc_frame_chirho().ok_or("OOM")?;
        let pa_u64_chirho = pa_chirho.as_u64();
        unsafe {
            core::ptr::write_bytes((pa_u64_chirho + phys_off_chirho) as *mut u8, 0, 4096);
        }
        Ok(pa_u64_chirho)
    };

    // For high-canonical module arena: the bootloader's intermediate page
    // table frames at PML4[511] are read-only. We must allocate entirely
    // new PDPT/PD/PT frames, copy existing entries to preserve kernel text
    // mappings, then point PML4[511] to our new writable PDPT.
    // These statics are defined at function scope but have 'static lifetime.
    // They're used to cache freshly allocated page table frames for the
    // module arena. MODULE_PDPT_PHYS is exposed via module_arena_pdpt_phys.
    static MODULE_PDPT_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);
    static MODULE_PD_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);
    static MODULE_PT_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);

    let is_module_arena_chirho = vaddr_chirho >= 0xFFFF_FFFF_0000_0000;

    // Walk PML4 → PDPT
    let pml4e_chirho = read_entry_chirho(pml4_phys_chirho, pml4i_chirho);
    let pdpt_phys_chirho = if is_module_arena_chirho {
        let cached_chirho = MODULE_PDPT_PHYS_CHIRHO.load(Ordering::Relaxed);
        if cached_chirho != 0 {
            cached_chirho
        } else {
            // Allocate completely FRESH PDPT (already zeroed by new_frame).
            // Do NOT copy bootloader entries — kernel text is at PML4[1],
            // not PML4[511]. PML4[511] was the bootloader's UEFI mapping
            // which may have reserved bits that cause RSVD page faults.
            let new_pdpt_chirho = new_frame_chirho()?;
            MODULE_PDPT_PHYS_CHIRHO.store(new_pdpt_chirho, Ordering::Relaxed);
            MODULE_ARENA_PDPT_PHYS_CHIRHO.store(new_pdpt_chirho, Ordering::Relaxed);
            // Update PML4 to point to our writable PDPT
            write_entry_chirho(pml4_phys_chirho, pml4i_chirho, new_pdpt_chirho | flags_inter_chirho);
            // Flush the PML4 entry's cache line + reload CR3 to commit
            unsafe {
                let pml4_entry_virt_chirho = (pml4_phys_chirho + phys_off_chirho) as *const u8;
                core::arch::asm!(
                    "clflush [{}]",
                    in(reg) pml4_entry_virt_chirho.add(pml4i_chirho * 8),
                    options(nostack)
                );
                core::arch::asm!("mfence", options(nostack));
                core::arch::asm!("mov rax, cr3; mov cr3, rax", out("rax") _, options(nostack));
            }
            new_pdpt_chirho
        }
    } else if pml4e_chirho & 1 == 0 {
        let f_chirho = new_frame_chirho()?;
        write_entry_chirho(pml4_phys_chirho, pml4i_chirho, f_chirho | flags_inter_chirho);
        f_chirho
    } else {
        addr_from_entry_chirho(pml4e_chirho)
    };

    // Walk PDPT → PD
    let pdpte_chirho = read_entry_chirho(pdpt_phys_chirho, pdpti_chirho);
    let pd_phys_chirho = if is_module_arena_chirho {
        let cached_chirho = MODULE_PD_PHYS_CHIRHO.load(Ordering::Relaxed);
        if cached_chirho != 0 {
            cached_chirho
        } else {
            // Fresh PD — copy from old PD if it existed
            let new_pd_chirho = new_frame_chirho()?;
            if pdpte_chirho & 1 != 0 {
                let old_pd_chirho = addr_from_entry_chirho(pdpte_chirho);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        (old_pd_chirho + phys_off_chirho) as *const u8,
                        (new_pd_chirho + phys_off_chirho) as *mut u8,
                        4096,
                    );
                }
            }
            MODULE_PD_PHYS_CHIRHO.store(new_pd_chirho, Ordering::Relaxed);
            write_entry_chirho(pdpt_phys_chirho, pdpti_chirho, new_pd_chirho | flags_inter_chirho);
            new_pd_chirho
        }
    } else if pdpte_chirho & 1 == 0 {
        let f_chirho = new_frame_chirho()?;
        write_entry_chirho(pdpt_phys_chirho, pdpti_chirho, f_chirho | flags_inter_chirho);
        f_chirho
    } else {
        addr_from_entry_chirho(pdpte_chirho)
    };

    // Walk PD → PT
    let pde_chirho = read_entry_chirho(pd_phys_chirho, pdi_chirho);
    let pt_phys_chirho = if is_module_arena_chirho {
        let cached_chirho = MODULE_PT_PHYS_CHIRHO.load(Ordering::Relaxed);
        if cached_chirho != 0 {
            cached_chirho
        } else {
            // Fresh PT — zeroed (no old entries to copy for our PD range)
            let new_pt_chirho = new_frame_chirho()?;
            MODULE_PT_PHYS_CHIRHO.store(new_pt_chirho, Ordering::Relaxed);
            write_entry_chirho(pd_phys_chirho, pdi_chirho, new_pt_chirho | flags_inter_chirho);
            new_pt_chirho
        }
    } else if pde_chirho & 1 == 0 {
        let f_chirho = new_frame_chirho()?;
        write_entry_chirho(pd_phys_chirho, pdi_chirho, f_chirho | flags_inter_chirho);
        f_chirho
    } else {
        addr_from_entry_chirho(pde_chirho)
    };

    // Write leaf PTE
    write_entry_chirho(pt_phys_chirho, pti_chirho, paddr_chirho | flags_leaf_chirho);

    // Flush TLB for this virtual address
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) vaddr_chirho, options(nostack, preserves_flags));
    }

    Ok(())
}
