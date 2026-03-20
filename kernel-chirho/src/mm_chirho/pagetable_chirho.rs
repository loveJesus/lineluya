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

/// Physical address of the boot PML4. Saved at boot so the page fault
/// handler can lazily migrate user-space mappings from the boot PT to
/// per-process page tables.
static BOOT_PML4_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);

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

    // Copy kernel-space entries (upper half: 256..511) from the BOOT PML4.
    // CRITICAL: Always use the boot PML4 as the authoritative source for
    // kernel-space mappings, NOT the current CR3.  The boot PML4 is where
    // GLOBAL_MAPPER_CHIRHO writes all kernel mappings (heap, kernel stacks,
    // module memory).  Using Cr3::read() would copy from a per-process PML4
    // that may be missing entries added after it was created.
    let boot_pml4_phys_chirho = get_boot_pml4_chirho();
    let source_pml4_chirho = if boot_pml4_phys_chirho.as_u64() != 0 {
        unsafe { table_from_phys_chirho(boot_pml4_phys_chirho) }
    } else {
        // Fallback: before save_boot_pml4 is called, use current CR3
        let (current_pml4_frame_chirho, _flags_chirho) = Cr3::read();
        unsafe { table_from_phys_chirho(current_pml4_frame_chirho.start_address()) }
    };

    for i_chirho in KERNEL_PML4_START_CHIRHO..ENTRIES_PER_TABLE_CHIRHO {
        let entry_chirho = &source_pml4_chirho[i_chirho];
        if !entry_chirho.is_unused() {
            new_pml4_chirho[i_chirho].set_addr(
                entry_chirho.addr(),
                entry_chirho.flags(),
            );
        }
    }

    // CRITICAL: Also copy the physical memory window mapping from the
    // lower half.  The bootloader maps all physical memory at
    // PHYS_MEM_OFFSET (typically 0x10000000000 → PML4 entry 2).
    // Without this entry, the kernel can't access page tables, user
    // memory, or any physical address via phys_to_virt_chirho.
    // This was the cause of the triple fault when switching CR3.
    let phys_offset_chirho = phys_mem_offset_chirho();
    if phys_offset_chirho != 0 {
        let phys_pml4_idx_chirho = ((phys_offset_chirho >> 39) & 0x1FF) as usize;
        if phys_pml4_idx_chirho < KERNEL_PML4_START_CHIRHO {
            let entry_chirho = &source_pml4_chirho[phys_pml4_idx_chirho];
            if !entry_chirho.is_unused() {
                new_pml4_chirho[phys_pml4_idx_chirho].set_addr(
                    entry_chirho.addr(),
                    entry_chirho.flags(),
                );
                crate::serial_debug_chirho!(
                    "[PAGETABLE] Copied phys-mem window PML4[{}] (offset {:#x})",
                    phys_pml4_idx_chirho,
                    phys_offset_chirho,
                );
            }
        }
    }

    // Copy ALL non-zero lower-half entries from the boot PML4.
    // The bootloader places critical mappings in the lower half:
    //   PML4[0]: possible identity mapping for boot structures
    //   PML4[2]: kernel binary (virtual_address_offset = 0x10000000000)
    //   PML4[5]: physical memory window (phys_mem_offset = 0x28000000000)
    // Without ALL of these, the kernel code, page tables, and physical
    // memory are inaccessible after CR3 switch, causing triple faults.
    //
    // User-space program mappings (ELF code, heap, stack) are NOT in
    // the boot PML4's lower half — they're added per-process by execve
    // and the page fault handler's lazy migration.
    for i_chirho in 0..KERNEL_PML4_START_CHIRHO {
        if !source_pml4_chirho[i_chirho].is_unused() {
            new_pml4_chirho[i_chirho].set_addr(
                source_pml4_chirho[i_chirho].addr(),
                source_pml4_chirho[i_chirho].flags(),
            );
        }
    }

    crate::serial_debug_chirho!(
        "[PAGETABLE] Created user page table: PML4 phys={:#x}",
        pml4_phys_chirho.as_u64()
    );

    Some(pml4_phys_chirho)
}

// ============================================================================
// clone_page_table_chirho — clone for fork with COW
// ============================================================================

/// Clone a user-space page table for fork, setting up COW.
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

    if get_current_pml4_phys_chirho() == source_pml4_phys_chirho {
        unsafe {
            switch_page_table_chirho(source_pml4_phys_chirho);
        }
    }

    crate::serial_debug_chirho!(
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
            let is_user_chirho = flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE);
            let is_writable_chirho = flags_chirho.contains(PageTableFlags::WRITABLE);
            let is_cow_chirho = flags_chirho.contains(PageTableFlags::BIT_9);

            if is_user_chirho && (is_writable_chirho || is_cow_chirho) {
                // COW: share the same physical frame. Both boot PML4 and
                // child PT point to the same page, marked read-only + COW.
                // When either process writes, handle_cow_fault_chirho copies.
                let mut cow_flags_chirho = flags_chirho;
                cow_flags_chirho.remove(PageTableFlags::WRITABLE);
                cow_flags_chirho.insert(PageTableFlags::BIT_9);
                source_table_chirho[i_chirho].set_addr(entry_addr_chirho, cow_flags_chirho);
                new_table_chirho[i_chirho].set_addr(entry_addr_chirho, cow_flags_chirho);
            } else {
                // Kernel or non-user page — share directly.
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
        // Use try_lock to avoid deadlock if the allocator is already locked
        // (e.g., COW fault during mmap/brk allocation call).
        let mut alloc_guard_chirho = match GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() {
            Some(g_chirho) => g_chirho,
            None => {
                crate::serial_println_chirho!(
                    "[COW] Frame allocator locked, spinning for {:#x}",
                    faulting_addr_chirho.as_u64()
                );
                // Spin briefly — the allocator lock should be released quickly.
                loop {
                    if let Some(g_chirho) = GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() {
                        break g_chirho;
                    }
                    core::hint::spin_loop();
                }
            }
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

    // Update the PTE: point to new frame, set WRITABLE, clear COW bit.
    let mut new_flags_chirho = flags_chirho;
    new_flags_chirho.insert(PageTableFlags::WRITABLE);
    new_flags_chirho.remove(PageTableFlags::BIT_9);

    unsafe {
        (*pte_ptr_chirho).set_addr(new_frame_phys_chirho, new_flags_chirho);
    }

    // Flush the TLB entry for this page.
    x86_64::instructions::tlb::flush(faulting_addr_chirho);

    // GPT-directed watchpoint: check if COW resolution affected the watched page
    let watched_page_chirho = 0x7ffffeffe000u64;
    let fault_page_chirho = faulting_addr_chirho.as_u64() & !0xFFF;
    if fault_page_chirho == watched_page_chirho {
        crate::syscall_entry_chirho::check_stack_watch_chirho("cow-fault");
    }

    crate::serial_debug_chirho!(
        "[PAGETABLE] COW resolved: addr={:#x}, old_frame={:#x}, new_frame={:#x}",
        faulting_addr_chirho.as_u64(),
        old_frame_phys_chirho.as_u64(),
        new_frame_phys_chirho.as_u64()
    );

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

                    // Mark as COW: remove WRITABLE, add COW bit
                    let cow_flags_chirho = (page_flags_chirho & !PageTableFlags::WRITABLE)
                        | PageTableFlags::from_bits_truncate(COW_BIT_CHIRHO);
                    let page_addr_chirho = pt_chirho[pt_idx_chirho].addr();
                    pt_chirho[pt_idx_chirho].set_addr(page_addr_chirho, cow_flags_chirho);
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

/// Clear (unmap) all user-accessible pages from a page table's leaf entries.
///
/// Called during execve to clean the address space before loading a new binary.
/// Only clears leaf PT entries (4 KiB pages); intermediate page table frames
/// (PDPT, PD, PT) are left intact so they can be reused by the new mappings.
///
/// Returns the number of pages cleared.
pub fn clear_user_pages_chirho(pml4_phys_chirho: PhysAddr) -> u64 {
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    let mut cleared_chirho: u64 = 0;

    for pml4_idx_chirho in 0..KERNEL_PML4_START_CHIRHO {
        if pml4_chirho[pml4_idx_chirho].is_unused() { continue; }
        let pdpt_flags_chirho = pml4_chirho[pml4_idx_chirho].flags();
        if !pdpt_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

        let pdpt_chirho = unsafe { table_from_phys_chirho(pml4_chirho[pml4_idx_chirho].addr()) };
        for pdpt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
            if pdpt_chirho[pdpt_idx_chirho].is_unused() { continue; }
            if pdpt_chirho[pdpt_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) { continue; }
            if !pdpt_chirho[pdpt_idx_chirho].flags().contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

            let pd_chirho = unsafe { table_from_phys_chirho(pdpt_chirho[pdpt_idx_chirho].addr()) };
            for pd_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                if pd_chirho[pd_idx_chirho].is_unused() { continue; }
                if pd_chirho[pd_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) { continue; }
                if !pd_chirho[pd_idx_chirho].flags().contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

                let pt_chirho = unsafe { table_from_phys_chirho(pd_chirho[pd_idx_chirho].addr()) };
                for pt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                    if pt_chirho[pt_idx_chirho].is_unused() { continue; }
                    let page_flags_chirho = pt_chirho[pt_idx_chirho].flags();
                    if !page_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

                    pt_chirho[pt_idx_chirho].set_unused();
                    cleared_chirho += 1;
                }
            }
        }
    }

    // Flush TLB
    unsafe {
        let (frame_chirho, flags_chirho) = x86_64::registers::control::Cr3::read();
        x86_64::registers::control::Cr3::write(frame_chirho, flags_chirho);
    }

    cleared_chirho
}

/// Restore COW-marked user pages back to writable in a page table.
///
/// Called during execve: after fork marked boot PML4 pages as COW, the
/// child has its own PT copy. When the parent (or child) exec's a new binary,
/// the old COW pages need to become writable again so the new ELF can be
/// loaded without PageAlreadyMapped conflicts from read-only COW pages.
///
/// Safe because the fork child has its own PT — restoring writability in
/// boot PML4 doesn't affect the child's view.
pub fn restore_cow_to_writable_chirho(pml4_phys_chirho: PhysAddr) -> u64 {
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    let mut restored_chirho: u64 = 0;

    for pml4_idx_chirho in 0..KERNEL_PML4_START_CHIRHO {
        if pml4_chirho[pml4_idx_chirho].is_unused() { continue; }
        let pdpt_flags_chirho = pml4_chirho[pml4_idx_chirho].flags();
        if !pdpt_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

        let pdpt_chirho = unsafe { table_from_phys_chirho(pml4_chirho[pml4_idx_chirho].addr()) };
        for pdpt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
            if pdpt_chirho[pdpt_idx_chirho].is_unused() { continue; }
            if pdpt_chirho[pdpt_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) { continue; }
            if !pdpt_chirho[pdpt_idx_chirho].flags().contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

            let pd_chirho = unsafe { table_from_phys_chirho(pdpt_chirho[pdpt_idx_chirho].addr()) };
            for pd_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                if pd_chirho[pd_idx_chirho].is_unused() { continue; }
                if pd_chirho[pd_idx_chirho].flags().contains(PageTableFlags::HUGE_PAGE) { continue; }
                if !pd_chirho[pd_idx_chirho].flags().contains(PageTableFlags::USER_ACCESSIBLE) { continue; }

                let pt_chirho = unsafe { table_from_phys_chirho(pd_chirho[pd_idx_chirho].addr()) };
                for pt_idx_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
                    if pt_chirho[pt_idx_chirho].is_unused() { continue; }
                    let page_flags_chirho = pt_chirho[pt_idx_chirho].flags();
                    // Only restore pages that have the COW bit set
                    if page_flags_chirho.bits() & COW_BIT_CHIRHO == 0 { continue; }

                    let page_addr_chirho = pt_chirho[pt_idx_chirho].addr();
                    let restored_flags_chirho = (page_flags_chirho | PageTableFlags::WRITABLE)
                        & !PageTableFlags::from_bits_truncate(COW_BIT_CHIRHO);
                    pt_chirho[pt_idx_chirho].set_addr(page_addr_chirho, restored_flags_chirho);
                    restored_chirho += 1;
                }
            }
        }
    }

    // Flush TLB
    unsafe {
        let (frame_chirho, flags_chirho) = x86_64::registers::control::Cr3::read();
        x86_64::registers::control::Cr3::write(frame_chirho, flags_chirho);
    }

    restored_chirho
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

/// Map a 4 KiB virtual page to a physical frame in a specific page table.
///
/// Unlike the global mapper (which maps into the current CR3 page table),
/// this function can populate any PML4, making it suitable for preparing
/// a per-process address space before switching CR3.
///
/// Intermediate page table levels (PDPT, PD, PT) are allocated on demand
/// from the frame allocator.
///
/// Returns `Ok(())` on success, or `Err(())` on OOM or failure.
pub fn map_page_in_pt_chirho(
    pml4_phys_chirho: PhysAddr,
    vaddr_chirho: u64,
    paddr_chirho: u64,
    flags_chirho: PageTableFlags,
) -> Result<(), ()> {
    let addr_chirho = vaddr_chirho;
    let pml4_idx_chirho = ((addr_chirho >> 39) & 0x1FF) as usize;
    let pdpt_idx_chirho = ((addr_chirho >> 30) & 0x1FF) as usize;
    let pd_idx_chirho = ((addr_chirho >> 21) & 0x1FF) as usize;
    let pt_idx_chirho = ((addr_chirho >> 12) & 0x1FF) as usize;

    let intermediate_flags_chirho = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE;

    // Level 4: PML4 → ensure PDPT exists
    let pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    if pml4_chirho[pml4_idx_chirho].is_unused() {
        let frame_chirho = alloc_frame_chirho().ok_or(())?;
        zero_frame_chirho(frame_chirho);
        pml4_chirho[pml4_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
    }
    let pdpt_phys_chirho = pml4_chirho[pml4_idx_chirho].addr();

    // Level 3: PDPT → ensure PD exists
    let pdpt_chirho = unsafe { table_from_phys_chirho(pdpt_phys_chirho) };
    if pdpt_chirho[pdpt_idx_chirho].is_unused() {
        let frame_chirho = alloc_frame_chirho().ok_or(())?;
        zero_frame_chirho(frame_chirho);
        pdpt_chirho[pdpt_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
    }
    let pd_phys_chirho = pdpt_chirho[pdpt_idx_chirho].addr();

    // Level 2: PD → ensure PT exists
    let pd_chirho = unsafe { table_from_phys_chirho(pd_phys_chirho) };
    if pd_chirho[pd_idx_chirho].is_unused() {
        let frame_chirho = alloc_frame_chirho().ok_or(())?;
        zero_frame_chirho(frame_chirho);
        pd_chirho[pd_idx_chirho].set_addr(frame_chirho, intermediate_flags_chirho);
    }
    let pt_phys_chirho = pd_chirho[pd_idx_chirho].addr();

    // Level 1: PT → set the leaf entry
    let pt_chirho = unsafe { table_from_phys_chirho(pt_phys_chirho) };
    pt_chirho[pt_idx_chirho].set_addr(
        PhysAddr::new(paddr_chirho),
        flags_chirho,
    );

    Ok(())
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
