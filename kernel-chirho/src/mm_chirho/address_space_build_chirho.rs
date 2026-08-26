// For God so loved the world, that he gave his only begotten Son,
// that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

//! Construction and COW cloning for owned x86 user address spaces.

use kernel_core_chirho::frame_ownership_chirho::{
    FrameReleaseErrorChirho, FrameReleaseOutcomeChirho, FrameRetainErrorChirho,
    FrameRetainOutcomeChirho,
};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{FrameAllocator, PageTableFlags, PhysFrame};
use x86_64::PhysAddr;

use crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO;

use super::{
    address_space_chirho, get_boot_pml4_chirho, get_current_pml4_phys_chirho,
    module_arena_pml4_entry_chirho, phys_mem_offset_chirho, switch_page_table_chirho,
    table_from_phys_chirho, AddressSpaceHandleChirho, ENTRIES_PER_TABLE_CHIRHO,
    KERNEL_PML4_START_CHIRHO,
};

/// Allocate a fresh PML4 frame and copy kernel-owned mappings from the boot
/// root. User-owned mappings always start empty.
pub fn create_user_page_table_chirho() -> Option<PhysAddr> {
    let pml4_frame_chirho = {
        let mut allocator_guard_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        allocator_guard_chirho.as_mut()?.allocate_frame()?
    };
    let pml4_phys_chirho = pml4_frame_chirho.start_address();
    let new_pml4_chirho = unsafe { table_from_phys_chirho(pml4_phys_chirho) };
    for entry_chirho in new_pml4_chirho.iter_mut() {
        entry_chirho.set_unused();
    }

    let boot_pml4_phys_chirho = get_boot_pml4_chirho();
    let source_pml4_chirho = if boot_pml4_phys_chirho.as_u64() != 0 {
        unsafe { table_from_phys_chirho(boot_pml4_phys_chirho) }
    } else {
        let (current_pml4_frame_chirho, _flags_chirho) = Cr3::read();
        unsafe { table_from_phys_chirho(current_pml4_frame_chirho.start_address()) }
    };

    for index_chirho in KERNEL_PML4_START_CHIRHO..ENTRIES_PER_TABLE_CHIRHO {
        let entry_chirho = &source_pml4_chirho[index_chirho];
        if !entry_chirho.is_unused() {
            new_pml4_chirho[index_chirho].set_addr(entry_chirho.addr(), entry_chirho.flags());
        }
    }

    // The module arena may have been added after older per-process roots were
    // created. Its stored boot-time entry is the authoritative source.
    let stored_module_entry_chirho = module_arena_pml4_entry_chirho();
    let physical_offset_chirho = phys_mem_offset_chirho();
    if stored_module_entry_chirho & 1 != 0 {
        let new_pml4_ptr_chirho = pml4_phys_chirho.as_u64() + physical_offset_chirho;
        unsafe {
            (new_pml4_ptr_chirho as *mut u64)
                .add(511)
                .write(stored_module_entry_chirho);
        }
    } else {
        let current_cr3_chirho: u64;
        unsafe {
            core::arch::asm!(
                "mov {}, cr3",
                out(reg) current_cr3_chirho,
                options(nostack)
            );
        }
        let current_root_chirho = current_cr3_chirho & 0x000F_FFFF_FFFF_F000;
        let current_module_entry_chirho = unsafe {
            ((current_root_chirho + physical_offset_chirho) as *const u64)
                .add(511)
                .read()
        };
        if current_module_entry_chirho & 1 != 0 {
            let new_pml4_ptr_chirho = pml4_phys_chirho.as_u64() + physical_offset_chirho;
            unsafe {
                (new_pml4_ptr_chirho as *mut u64)
                    .add(511)
                    .write(current_module_entry_chirho);
            }
        }
    }

    // Copy the physical-memory window and any other kernel-only lower-half
    // roots. User-accessible roots are address-space ownership and never copy.
    for index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &source_pml4_chirho[index_chirho];
        if !entry_chirho.is_unused()
            && !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            new_pml4_chirho[index_chirho].set_addr(entry_chirho.addr(), entry_chirho.flags());
        }
    }

    crate::serial_debug_chirho!(
        "[PAGETABLE] Created user page table: PML4 phys={:#x}",
        pml4_phys_chirho.as_u64(),
    );
    Some(pml4_phys_chirho)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceBuildErrorChirho {
    BootRootUnavailableChirho,
    RootFrameExhaustedChirho,
    CloneChirho(PageTableCloneErrorChirho),
    HandleAllocationFailedChirho,
    CleanupChirho(address_space_chirho::PageTableRetireErrorChirho),
}

/// Build a fresh owned address space for exec.
///
/// This is the exec-construction edge in
/// `spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md`.
pub fn create_user_address_space_chirho(
) -> Result<AddressSpaceHandleChirho, AddressSpaceBuildErrorChirho> {
    let root_phys_chirho = create_user_page_table_chirho()
        .ok_or(AddressSpaceBuildErrorChirho::RootFrameExhaustedChirho)?;
    own_new_user_address_space_chirho(root_phys_chirho)
}

fn own_new_user_address_space_chirho(
    root_phys_chirho: PhysAddr,
) -> Result<AddressSpaceHandleChirho, AddressSpaceBuildErrorChirho> {
    if let Some(handle_chirho) =
        AddressSpaceHandleChirho::try_from_new_root_chirho(root_phys_chirho)
    {
        return Ok(handle_chirho);
    }
    address_space_chirho::retire_unowned_page_table_chirho(root_phys_chirho)
        .map_err(AddressSpaceBuildErrorChirho::CleanupChirho)?;
    Err(AddressSpaceBuildErrorChirho::HandleAllocationFailedChirho)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableCloneErrorChirho {
    RootFrameExhaustedChirho,
    TableFrameExhaustedChirho,
    HugeUserMappingChirho { physical_address_chirho: u64 },
    UnregisteredLeafChirho { physical_address_chirho: u64 },
    LeafRetainChirho(FrameRetainErrorChirho),
    CleanupLeafReleaseChirho(FrameReleaseErrorChirho),
    CleanupLostSourceReferenceChirho { physical_address_chirho: u64 },
    CleanupLeafRestoreChirho(FrameRetainErrorChirho),
}

/// Compatibility wrapper for lifecycle call sites that still store a raw
/// `PhysAddr`. New code consumes [`clone_user_address_space_chirho`] instead.
pub fn clone_page_table_chirho(source_pml4_phys_chirho: PhysAddr) -> Option<PhysAddr> {
    match try_clone_page_table_chirho(source_pml4_phys_chirho) {
        Ok(root_phys_chirho) => Some(root_phys_chirho),
        Err(clone_error_chirho) => {
            crate::serial_println_chirho!("[PT-CLONE] failed: {:?}", clone_error_chirho);
            None
        }
    }
}

/// Clone the user half of a PML4 and retain every shared managed leaf.
pub fn try_clone_page_table_chirho(
    source_pml4_phys_chirho: PhysAddr,
) -> Result<PhysAddr, PageTableCloneErrorChirho> {
    let new_pml4_phys_chirho = create_user_page_table_chirho()
        .ok_or(PageTableCloneErrorChirho::RootFrameExhaustedChirho)?;
    let source_pml4_chirho = unsafe { table_from_phys_chirho(source_pml4_phys_chirho) };
    let new_pml4_chirho = unsafe { table_from_phys_chirho(new_pml4_phys_chirho) };

    for index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let source_entry_chirho = &source_pml4_chirho[index_chirho];
        if source_entry_chirho.is_unused()
            || !source_entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        let cloned_frame_chirho = match clone_table_level_chirho(source_entry_chirho.addr(), 3) {
            Ok(frame_chirho) => frame_chirho,
            Err(clone_error_chirho) => {
                let cleanup_result_chirho = discard_cloned_root_chirho(new_pml4_phys_chirho);
                flush_source_after_clone_chirho(source_pml4_phys_chirho);
                return match cleanup_result_chirho {
                    Ok(()) => Err(clone_error_chirho),
                    Err(cleanup_error_chirho) => Err(cleanup_error_chirho),
                };
            }
        };
        new_pml4_chirho[index_chirho].set_addr(cloned_frame_chirho, source_entry_chirho.flags());
    }

    flush_source_after_clone_chirho(source_pml4_phys_chirho);
    crate::serial_debug_chirho!(
        "[PAGETABLE] Cloned page table: source={:#x} -> new={:#x}",
        source_pml4_phys_chirho.as_u64(),
        new_pml4_phys_chirho.as_u64(),
    );
    Ok(new_pml4_phys_chirho)
}

fn flush_source_after_clone_chirho(source_pml4_phys_chirho: PhysAddr) {
    if get_current_pml4_phys_chirho() == source_pml4_phys_chirho {
        unsafe {
            switch_page_table_chirho(source_pml4_phys_chirho);
        }
    }
}

/// Clone COW mappings and immediately wrap the new root in its owner type.
pub fn clone_user_address_space_chirho(
    source_address_space_chirho: &AddressSpaceHandleChirho,
) -> Result<AddressSpaceHandleChirho, AddressSpaceBuildErrorChirho> {
    let root_phys_chirho =
        try_clone_page_table_chirho(source_address_space_chirho.root_phys_chirho())
            .map_err(AddressSpaceBuildErrorChirho::CloneChirho)?;
    own_new_user_address_space_chirho(root_phys_chirho)
}

/// Clone the boot PML4's user mappings into a newly owned address space.
///
/// Early init tasks run directly on the boot root and therefore have no
/// [`AddressSpaceHandleChirho`] to borrow. Keeping the raw boot root inside
/// this constructor prevents lifecycle call sites from acquiring a general
/// raw-root-to-owner escape hatch. The caller marks the boot mappings COW
/// before invoking this function.
pub fn clone_boot_user_address_space_chirho(
) -> Result<AddressSpaceHandleChirho, AddressSpaceBuildErrorChirho> {
    let boot_root_chirho = get_boot_pml4_chirho();
    if boot_root_chirho.as_u64() == 0 {
        return Err(AddressSpaceBuildErrorChirho::BootRootUnavailableChirho);
    }
    let root_phys_chirho = try_clone_page_table_chirho(boot_root_chirho)
        .map_err(AddressSpaceBuildErrorChirho::CloneChirho)?;
    own_new_user_address_space_chirho(root_phys_chirho)
}

fn clone_table_level_chirho(
    source_table_phys_chirho: PhysAddr,
    level_chirho: u8,
) -> Result<PhysAddr, PageTableCloneErrorChirho> {
    let new_frame_chirho = {
        let mut allocator_guard_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        allocator_guard_chirho
            .as_mut()
            .and_then(|allocator_chirho| allocator_chirho.allocate_frame())
            .ok_or(PageTableCloneErrorChirho::TableFrameExhaustedChirho)?
    };
    let new_table_phys_chirho = new_frame_chirho.start_address();
    let new_table_chirho = unsafe { table_from_phys_chirho(new_table_phys_chirho) };
    for entry_chirho in new_table_chirho.iter_mut() {
        entry_chirho.set_unused();
    }
    let source_table_chirho = unsafe { table_from_phys_chirho(source_table_phys_chirho) };

    for index_chirho in 0..ENTRIES_PER_TABLE_CHIRHO {
        if source_table_chirho[index_chirho].is_unused() {
            continue;
        }
        let entry_addr_chirho = source_table_chirho[index_chirho].addr();
        let flags_chirho = source_table_chirho[index_chirho].flags();

        if level_chirho > 1 && !flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
            new_table_chirho[index_chirho].set_addr(entry_addr_chirho, flags_chirho);
            continue;
        }
        if level_chirho > 1 && flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            let clone_error_chirho = PageTableCloneErrorChirho::HugeUserMappingChirho {
                physical_address_chirho: entry_addr_chirho.as_u64(),
            };
            return cleanup_after_clone_error_chirho(
                new_table_phys_chirho,
                level_chirho,
                clone_error_chirho,
            );
        }

        if level_chirho == 1 {
            if let Err(clone_error_chirho) = clone_leaf_chirho(
                source_table_chirho,
                new_table_chirho,
                index_chirho,
                entry_addr_chirho,
                flags_chirho,
            ) {
                return cleanup_after_clone_error_chirho(
                    new_table_phys_chirho,
                    level_chirho,
                    clone_error_chirho,
                );
            }
        } else {
            let child_phys_chirho =
                match clone_table_level_chirho(entry_addr_chirho, level_chirho - 1) {
                    Ok(child_phys_chirho) => child_phys_chirho,
                    Err(clone_error_chirho) => {
                        return cleanup_after_clone_error_chirho(
                            new_table_phys_chirho,
                            level_chirho,
                            clone_error_chirho,
                        );
                    }
                };
            new_table_chirho[index_chirho].set_addr(child_phys_chirho, flags_chirho);
        }
    }
    Ok(new_table_phys_chirho)
}

fn clone_leaf_chirho(
    source_table_chirho: &mut x86_64::structures::paging::PageTable,
    new_table_chirho: &mut x86_64::structures::paging::PageTable,
    index_chirho: usize,
    entry_addr_chirho: PhysAddr,
    flags_chirho: PageTableFlags,
) -> Result<(), PageTableCloneErrorChirho> {
    if !flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
        new_table_chirho[index_chirho].set_addr(entry_addr_chirho, flags_chirho);
        return Ok(());
    }

    if address_space_chirho::leaf_mapping_count_chirho(entry_addr_chirho) == Some(0) {
        return Err(PageTableCloneErrorChirho::UnregisteredLeafChirho {
            physical_address_chirho: entry_addr_chirho.as_u64(),
        });
    }
    let retain_chirho = address_space_chirho::retain_leaf_mapping_chirho(entry_addr_chirho)
        .map_err(PageTableCloneErrorChirho::LeafRetainChirho)?;
    let managed_chirho = retain_chirho != FrameRetainOutcomeChirho::UnmanagedChirho;
    let mut cloned_flags_chirho = flags_chirho;
    if managed_chirho
        && (flags_chirho.contains(PageTableFlags::WRITABLE)
            || flags_chirho.contains(PageTableFlags::BIT_9))
    {
        cloned_flags_chirho.remove(PageTableFlags::WRITABLE);
        cloned_flags_chirho.insert(PageTableFlags::BIT_9);
        source_table_chirho[index_chirho].set_addr(entry_addr_chirho, cloned_flags_chirho);
    }
    new_table_chirho[index_chirho].set_addr(entry_addr_chirho, cloned_flags_chirho);
    Ok(())
}

fn cleanup_after_clone_error_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
    clone_error_chirho: PageTableCloneErrorChirho,
) -> Result<PhysAddr, PageTableCloneErrorChirho> {
    match discard_cloned_table_chirho(table_phys_chirho, level_chirho) {
        Ok(()) => Err(clone_error_chirho),
        Err(cleanup_error_chirho) => Err(cleanup_error_chirho),
    }
}

fn discard_cloned_table_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
) -> Result<(), PageTableCloneErrorChirho> {
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    let mut first_cleanup_error_chirho = None;
    for entry_chirho in table_chirho.iter_mut() {
        if entry_chirho.is_unused() {
            continue;
        }
        let entry_phys_chirho = entry_chirho.addr();
        let entry_flags_chirho = entry_chirho.flags();
        if level_chirho == 1 {
            if entry_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
                match address_space_chirho::release_leaf_mapping_chirho(entry_phys_chirho) {
                    Ok(FrameReleaseOutcomeChirho::LastReferenceChirho) => {
                        // A failed clone is never published and its source PTE
                        // is still live. Reaching zero therefore proves the
                        // source reference was missing from the accounting
                        // table. Restore that source ownership before clearing
                        // the unpublished clone; freeing here would corrupt the
                        // source address space.
                        if let Err(restore_error_chirho) =
                            address_space_chirho::retain_leaf_mapping_chirho(entry_phys_chirho)
                        {
                            first_cleanup_error_chirho.get_or_insert(
                                PageTableCloneErrorChirho::CleanupLeafRestoreChirho(
                                    restore_error_chirho,
                                ),
                            );
                        } else {
                            first_cleanup_error_chirho.get_or_insert(
                                PageTableCloneErrorChirho::CleanupLostSourceReferenceChirho {
                                    physical_address_chirho: entry_phys_chirho.as_u64(),
                                },
                            );
                        }
                    }
                    Ok(FrameReleaseOutcomeChirho::StillReferencedChirho { .. })
                    | Ok(FrameReleaseOutcomeChirho::UnmanagedChirho) => {}
                    Err(release_error_chirho) => {
                        // Underflow also means the still-live source PTE is not
                        // represented. Re-establish its one known reference,
                        // then finish dismantling the unpublished clone so a
                        // diagnostic failure cannot strand page-table frames.
                        if let Err(restore_error_chirho) =
                            address_space_chirho::retain_leaf_mapping_chirho(entry_phys_chirho)
                        {
                            first_cleanup_error_chirho.get_or_insert(
                                PageTableCloneErrorChirho::CleanupLeafRestoreChirho(
                                    restore_error_chirho,
                                ),
                            );
                        } else {
                            first_cleanup_error_chirho.get_or_insert(
                                PageTableCloneErrorChirho::CleanupLeafReleaseChirho(
                                    release_error_chirho,
                                ),
                            );
                        }
                    }
                }
            }
        } else if entry_flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE)
            && !entry_flags_chirho.contains(PageTableFlags::HUGE_PAGE)
        {
            if let Err(cleanup_error_chirho) =
                discard_cloned_table_chirho(entry_phys_chirho, level_chirho - 1)
            {
                first_cleanup_error_chirho.get_or_insert(cleanup_error_chirho);
            }
        }
        entry_chirho.set_unused();
    }
    crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(table_phys_chirho));
    match first_cleanup_error_chirho {
        Some(cleanup_error_chirho) => Err(cleanup_error_chirho),
        None => Ok(()),
    }
}

fn discard_cloned_root_chirho(root_phys_chirho: PhysAddr) -> Result<(), PageTableCloneErrorChirho> {
    let root_chirho = unsafe { table_from_phys_chirho(root_phys_chirho) };
    let mut first_cleanup_error_chirho = None;
    for index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &mut root_chirho[index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        let child_phys_chirho = entry_chirho.addr();
        if let Err(cleanup_error_chirho) = discard_cloned_table_chirho(child_phys_chirho, 3) {
            first_cleanup_error_chirho.get_or_insert(cleanup_error_chirho);
        }
        entry_chirho.set_unused();
    }
    crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(root_phys_chirho));
    match first_cleanup_error_chirho {
        Some(cleanup_error_chirho) => Err(cleanup_error_chirho),
        None => Ok(()),
    }
}
