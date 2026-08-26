// For God so loved the world, that he gave his only begotten Son,
// that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

//! Type-enforced ownership for x86 user address spaces.
//!
//! [`AddressSpaceHandleChirho`] expresses CLONE_VM sharing directly. Dropping
//! a handle only decrements an atomic owner count; it never walks page tables
//! or takes the frame-allocator lock. Expensive retirement is explicit, only
//! succeeds for the last owner, and refuses the PML4 currently installed in
//! CR3.

extern crate alloc;

use alloc::alloc::{alloc, dealloc, Layout};
use alloc::vec::Vec;
use core::fmt;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};
use kernel_core_chirho::frame_ownership_chirho::{
    AddressSpaceOwnerCountChirho, AddressSpaceOwnerErrorChirho, AddressSpaceOwnerReleaseChirho,
    AddressSpaceRetireGateErrorChirho, FrameOwnershipInitErrorChirho, FrameRangeChirho,
    FrameReleaseErrorChirho, FrameReleaseOutcomeChirho, FrameRetainErrorChirho,
    FrameRetainOutcomeChirho, LeafFrameOwnershipChirho,
};
use spin::Once;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTableFlags, PhysFrame};
use x86_64::PhysAddr;

use super::{
    get_boot_pml4_chirho, table_from_phys_chirho, ENTRIES_PER_TABLE_CHIRHO,
    KERNEL_PML4_START_CHIRHO, PAGE_SIZE_CHIRHO,
};

static LEAF_FRAME_OWNERSHIP_CHIRHO: Once<LeafFrameOwnershipChirho> = Once::new();
static UNRETIRED_LAST_HANDLE_DROPS_CHIRHO: AtomicU64 = AtomicU64::new(0);
static HANDLE_REFCOUNT_FAILURES_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Boot-time initialization failures for physical-frame ownership metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceOwnershipInitErrorChirho {
    AlreadyInitializedChirho,
    FrameTableChirho(FrameOwnershipInitErrorChirho),
    RangeAllocationFailedChirho,
}

/// Measured shape of the preallocated physical-frame reference table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressSpaceOwnershipInitStatsChirho {
    pub slot_count_chirho: usize,
    pub managed_frame_count_chirho: usize,
}

/// Boot-time registration failures for user mappings that predate the flat
/// ownership table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMappingRegistrationErrorChirho {
    HugeUserMappingChirho { physical_address_chirho: u64 },
    RetainChirho(FrameRetainErrorChirho),
    RollbackChirho(FrameReleaseErrorChirho),
    ScratchAllocationFailedChirho,
    TraversalChangedChirho,
}

/// Initialize all COW leaf metadata before any user task or page fault runs.
pub fn init_leaf_frame_ownership_chirho(
    memory_regions_chirho: &'static bootloader_api::info::MemoryRegions,
) -> Result<AddressSpaceOwnershipInitStatsChirho, AddressSpaceOwnershipInitErrorChirho> {
    if LEAF_FRAME_OWNERSHIP_CHIRHO.get().is_some() {
        return Err(AddressSpaceOwnershipInitErrorChirho::AlreadyInitializedChirho);
    }

    let mut managed_ranges_chirho = Vec::new();
    managed_ranges_chirho
        .try_reserve_exact(memory_regions_chirho.len())
        .map_err(|_| AddressSpaceOwnershipInitErrorChirho::RangeAllocationFailedChirho)?;

    for region_chirho in memory_regions_chirho.iter() {
        if region_chirho.kind != bootloader_api::info::MemoryRegionKind::Usable {
            continue;
        }
        let start_frame_chirho =
            region_chirho.start.saturating_add(PAGE_SIZE_CHIRHO - 1) / PAGE_SIZE_CHIRHO;
        let end_frame_exclusive_chirho = region_chirho.end / PAGE_SIZE_CHIRHO;
        if start_frame_chirho < end_frame_exclusive_chirho {
            managed_ranges_chirho.push(FrameRangeChirho {
                start_frame_chirho: start_frame_chirho as usize,
                end_frame_exclusive_chirho: end_frame_exclusive_chirho as usize,
            });
        }
    }

    let ownership_chirho = LeafFrameOwnershipChirho::try_new_chirho(&managed_ranges_chirho)
        .map_err(AddressSpaceOwnershipInitErrorChirho::FrameTableChirho)?;
    let stats_chirho = AddressSpaceOwnershipInitStatsChirho {
        slot_count_chirho: ownership_chirho.slot_count_chirho(),
        managed_frame_count_chirho: ownership_chirho.managed_frame_count_chirho(),
    };
    LEAF_FRAME_OWNERSHIP_CHIRHO.call_once(|| ownership_chirho);
    Ok(stats_chirho)
}

fn leaf_frame_ownership_chirho() -> Option<&'static LeafFrameOwnershipChirho> {
    LEAF_FRAME_OWNERSHIP_CHIRHO.get()
}

fn physical_frame_number_chirho(physical_address_chirho: PhysAddr) -> usize {
    (physical_address_chirho.as_u64() / PAGE_SIZE_CHIRHO) as usize
}

pub(super) fn retain_leaf_mapping_chirho(
    physical_address_chirho: PhysAddr,
) -> Result<FrameRetainOutcomeChirho, FrameRetainErrorChirho> {
    let Some(ownership_chirho) = leaf_frame_ownership_chirho() else {
        return Ok(FrameRetainOutcomeChirho::UnmanagedChirho);
    };
    ownership_chirho.retain_mapping_chirho(physical_frame_number_chirho(physical_address_chirho))
}

pub(super) fn release_leaf_mapping_chirho(
    physical_address_chirho: PhysAddr,
) -> Result<FrameReleaseOutcomeChirho, FrameReleaseErrorChirho> {
    let Some(ownership_chirho) = leaf_frame_ownership_chirho() else {
        return Ok(FrameReleaseOutcomeChirho::UnmanagedChirho);
    };
    ownership_chirho.release_mapping_chirho(physical_frame_number_chirho(physical_address_chirho))
}

pub(super) fn leaf_mapping_count_chirho(physical_address_chirho: PhysAddr) -> Option<u32> {
    leaf_frame_ownership_chirho()?
        .mapping_count_chirho(physical_frame_number_chirho(physical_address_chirho))
}

pub(super) fn frame_is_managed_chirho(physical_address_chirho: PhysAddr) -> bool {
    leaf_mapping_count_chirho(physical_address_chirho).is_some()
}

fn count_existing_user_leaves_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
) -> Result<usize, UserMappingRegistrationErrorChirho> {
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    let mut leaf_count_chirho = 0usize;
    for entry_chirho in table_chirho.iter() {
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if level_chirho > 1 && entry_chirho.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err(UserMappingRegistrationErrorChirho::HugeUserMappingChirho {
                physical_address_chirho: entry_chirho.addr().as_u64(),
            });
        }
        if level_chirho == 1 {
            leaf_count_chirho = leaf_count_chirho.saturating_add(1);
        } else {
            leaf_count_chirho = leaf_count_chirho.saturating_add(
                count_existing_user_leaves_chirho(entry_chirho.addr(), level_chirho - 1)?,
            );
        }
    }
    Ok(leaf_count_chirho)
}

fn retain_existing_user_leaves_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
    retained_frames_chirho: &mut Vec<PhysAddr>,
) -> Result<(), UserMappingRegistrationErrorChirho> {
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    for entry_chirho in table_chirho.iter() {
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if level_chirho == 1 {
            let physical_address_chirho = entry_chirho.addr();
            if retain_leaf_mapping_chirho(physical_address_chirho)
                .map_err(UserMappingRegistrationErrorChirho::RetainChirho)?
                != FrameRetainOutcomeChirho::UnmanagedChirho
            {
                retained_frames_chirho.push(physical_address_chirho);
            }
        } else {
            retain_existing_user_leaves_chirho(
                entry_chirho.addr(),
                level_chirho - 1,
                retained_frames_chirho,
            )?;
        }
    }
    Ok(())
}

/// Register user PTEs that existed before ownership metadata was initialized.
///
/// This is the boot edge of
/// `spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md`. A failed
/// registration rolls back every counter it changed rather than leaving a
/// partially owned address space.
pub fn register_existing_user_mappings_chirho(
    root_phys_chirho: PhysAddr,
) -> Result<usize, UserMappingRegistrationErrorChirho> {
    let root_table_chirho = unsafe { table_from_phys_chirho(root_phys_chirho) };
    let mut leaf_count_chirho = 0usize;
    for pml4_index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &root_table_chirho[pml4_index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        leaf_count_chirho = leaf_count_chirho
            .saturating_add(count_existing_user_leaves_chirho(entry_chirho.addr(), 3)?);
    }

    let mut retained_frames_chirho = Vec::new();
    retained_frames_chirho
        .try_reserve_exact(leaf_count_chirho)
        .map_err(|_| UserMappingRegistrationErrorChirho::ScratchAllocationFailedChirho)?;
    for pml4_index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &root_table_chirho[pml4_index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if let Err(registration_error_chirho) =
            retain_existing_user_leaves_chirho(entry_chirho.addr(), 3, &mut retained_frames_chirho)
        {
            for retained_frame_chirho in retained_frames_chirho.iter().rev() {
                release_leaf_mapping_chirho(*retained_frame_chirho)
                    .map_err(UserMappingRegistrationErrorChirho::RollbackChirho)?;
            }
            return Err(registration_error_chirho);
        }
    }
    if retained_frames_chirho.len() > leaf_count_chirho {
        return Err(UserMappingRegistrationErrorChirho::TraversalChangedChirho);
    }
    Ok(leaf_count_chirho)
}

struct AddressSpaceRecordChirho {
    root_phys_chirho: PhysAddr,
    owners_chirho: AddressSpaceOwnerCountChirho,
}

/// Refcounted ownership handle for one physical PML4 tree.
///
/// A shared CLONE_VM task clones this handle without duplicating the PML4.
/// `Drop` deliberately performs atomic accounting only. The final owner must
/// invoke [`AddressSpaceHandleChirho::retire_chirho`] from a context that has
/// already switched to a different CR3.
pub struct AddressSpaceHandleChirho {
    record_chirho: NonNull<AddressSpaceRecordChirho>,
}

unsafe impl Send for AddressSpaceHandleChirho {}
unsafe impl Sync for AddressSpaceHandleChirho {}

impl AddressSpaceHandleChirho {
    pub(super) fn try_from_new_root_chirho(root_phys_chirho: PhysAddr) -> Option<Self> {
        let layout_chirho = Layout::new::<AddressSpaceRecordChirho>();
        let record_ptr_chirho = unsafe { alloc(layout_chirho) as *mut AddressSpaceRecordChirho };
        let record_chirho = NonNull::new(record_ptr_chirho)?;
        unsafe {
            record_chirho.as_ptr().write(AddressSpaceRecordChirho {
                root_phys_chirho,
                owners_chirho: AddressSpaceOwnerCountChirho::new_chirho(),
            });
        }
        Some(Self { record_chirho })
    }

    fn record_chirho(&self) -> &AddressSpaceRecordChirho {
        unsafe { self.record_chirho.as_ref() }
    }

    pub fn root_phys_chirho(&self) -> PhysAddr {
        self.record_chirho().root_phys_chirho
    }

    pub fn owner_count_chirho(&self) -> u32 {
        self.record_chirho().owners_chirho.owner_count_chirho()
    }

    pub fn try_share_chirho(&self) -> Result<Self, AddressSpaceShareErrorChirho> {
        let record_chirho = self.record_chirho();
        record_chirho
            .owners_chirho
            .try_retain_chirho()
            .map_err(AddressSpaceShareErrorChirho::OwnerCountChirho)?;
        Ok(Self {
            record_chirho: self.record_chirho,
        })
    }

    /// Explicitly retire the PML4 tree. Active CR3 and shared-owner states are
    /// returned with the handle intact, so failure never silently loses the
    /// only path to later cleanup.
    pub fn retire_chirho(
        self,
    ) -> Result<AddressSpaceRetireStatsChirho, AddressSpaceRetireErrorChirho> {
        // Workflow: spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md
        let record_chirho = self.record_chirho();
        if let Err(gate_error_chirho) = record_chirho.owners_chirho.try_begin_retirement_chirho() {
            let reason_chirho = match gate_error_chirho {
                AddressSpaceRetireGateErrorChirho::NotLiveChirho => {
                    AddressSpaceRetireReasonChirho::NotLiveChirho
                }
                AddressSpaceRetireGateErrorChirho::SharedOwnersChirho { owners_chirho } => {
                    AddressSpaceRetireReasonChirho::SharedOwnersChirho { owners_chirho }
                }
            };
            return Err(AddressSpaceRetireErrorChirho {
                handle_chirho: self,
                reason_chirho,
            });
        }

        let root_phys_chirho = record_chirho.root_phys_chirho;
        if root_phys_chirho == get_boot_pml4_chirho() {
            record_chirho
                .owners_chirho
                .cancel_retirement_chirho()
                .expect("address-space retirement gate changed while refusing boot root");
            return Err(AddressSpaceRetireErrorChirho {
                handle_chirho: self,
                reason_chirho: AddressSpaceRetireReasonChirho::BootRootChirho,
            });
        }
        if Cr3::read().0.start_address() == root_phys_chirho {
            record_chirho
                .owners_chirho
                .cancel_retirement_chirho()
                .expect("address-space retirement gate changed while refusing active CR3");
            return Err(AddressSpaceRetireErrorChirho {
                handle_chirho: self,
                reason_chirho: AddressSpaceRetireReasonChirho::ActiveCr3Chirho,
            });
        }

        // A scheduler switch loads CR3 in assembly, so the global mapper may
        // still borrow the retired task's now-inactive root. Acquire through
        // the current-root API before testing the second liveness gate: this
        // safely ends that stale borrow in normal task context and prevents a
        // harmless refusal from becoming a permanent tree leak.
        drop(crate::mm_chirho::lock_current_mapper_chirho());
        if crate::mm_chirho::global_mapper_root_phys_chirho() == Some(root_phys_chirho) {
            record_chirho
                .owners_chirho
                .cancel_retirement_chirho()
                .expect("address-space retirement gate changed while refusing active mapper root");
            return Err(AddressSpaceRetireErrorChirho {
                handle_chirho: self,
                reason_chirho: AddressSpaceRetireReasonChirho::ActiveMapperRootChirho,
            });
        }

        let stats_chirho = match retire_page_table_tree_chirho(root_phys_chirho) {
            Ok(stats_chirho) => stats_chirho,
            Err(tree_error_chirho) => {
                record_chirho
                    .owners_chirho
                    .cancel_retirement_chirho()
                    .expect("address-space retirement gate changed after tree refusal");
                return Err(AddressSpaceRetireErrorChirho {
                    handle_chirho: self,
                    reason_chirho: AddressSpaceRetireReasonChirho::PageTableChirho(
                        tree_error_chirho,
                    ),
                });
            }
        };

        record_chirho
            .owners_chirho
            .finish_retirement_chirho()
            .expect("address-space retirement gate changed after page-table teardown");

        let handle_chirho = ManuallyDrop::new(self);
        let record_ptr_chirho = handle_chirho.record_chirho.as_ptr();
        unsafe {
            core::ptr::drop_in_place(record_ptr_chirho);
            dealloc(
                record_ptr_chirho.cast::<u8>(),
                Layout::new::<AddressSpaceRecordChirho>(),
            );
        }
        Ok(stats_chirho)
    }
}

impl Clone for AddressSpaceHandleChirho {
    fn clone(&self) -> Self {
        self.try_share_chirho()
            .expect("address-space handle cloned after retirement or owner overflow")
    }
}

impl fmt::Debug for AddressSpaceHandleChirho {
    fn fmt(&self, formatter_chirho: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter_chirho
            .debug_struct("AddressSpaceHandleChirho")
            .field("root_phys_chirho", &self.root_phys_chirho())
            .field("owner_count_chirho", &self.owner_count_chirho())
            .finish()
    }
}

impl Drop for AddressSpaceHandleChirho {
    fn drop(&mut self) {
        match self.record_chirho().owners_chirho.release_chirho() {
            Ok(AddressSpaceOwnerReleaseChirho::LastOwnerChirho) => {
                UNRETIRED_LAST_HANDLE_DROPS_CHIRHO.fetch_add(1, Ordering::Relaxed);
            }
            Ok(AddressSpaceOwnerReleaseChirho::OwnersRemainChirho { .. }) => {}
            Err(_) => {
                HANDLE_REFCOUNT_FAILURES_CHIRHO.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceShareErrorChirho {
    OwnerCountChirho(AddressSpaceOwnerErrorChirho),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableRetireErrorChirho {
    ActiveRootChirho,
    BootRootChirho,
    UnmanagedTableFrameChirho { physical_address_chirho: u64 },
    HugeUserMappingChirho { physical_address_chirho: u64 },
    LeafReleaseChirho(FrameReleaseErrorChirho),
    LeafRollbackChirho(FrameRetainErrorChirho),
    ScratchAllocationFailedChirho,
    TraversalChangedChirho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceRetireReasonChirho {
    NotLiveChirho,
    SharedOwnersChirho { owners_chirho: u32 },
    ActiveCr3Chirho,
    ActiveMapperRootChirho,
    BootRootChirho,
    PageTableChirho(PageTableRetireErrorChirho),
}

#[derive(Debug)]
pub struct AddressSpaceRetireErrorChirho {
    pub handle_chirho: AddressSpaceHandleChirho,
    pub reason_chirho: AddressSpaceRetireReasonChirho,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AddressSpaceRetireStatsChirho {
    pub leaf_mappings_released_chirho: u64,
    pub leaf_frames_freed_chirho: u64,
    pub unmanaged_leaf_mappings_cleared_chirho: u64,
    pub table_frames_freed_chirho: u64,
}

#[derive(Debug, Clone, Copy)]
struct ReleasedLeafChirho {
    physical_address_chirho: PhysAddr,
    free_frame_chirho: bool,
    managed_chirho: bool,
}

fn validate_table_tree_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
) -> Result<usize, PageTableRetireErrorChirho> {
    if !frame_is_managed_chirho(table_phys_chirho) {
        return Err(PageTableRetireErrorChirho::UnmanagedTableFrameChirho {
            physical_address_chirho: table_phys_chirho.as_u64(),
        });
    }
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    let mut leaf_count_chirho = 0usize;

    for entry_chirho in table_chirho.iter() {
        if entry_chirho.is_unused() {
            continue;
        }
        let flags_chirho = entry_chirho.flags();
        if !flags_chirho.contains(PageTableFlags::USER_ACCESSIBLE) {
            continue;
        }
        if level_chirho > 1 && flags_chirho.contains(PageTableFlags::HUGE_PAGE) {
            return Err(PageTableRetireErrorChirho::HugeUserMappingChirho {
                physical_address_chirho: entry_chirho.addr().as_u64(),
            });
        }
        if level_chirho == 1 {
            leaf_count_chirho = leaf_count_chirho.saturating_add(1);
        } else {
            leaf_count_chirho = leaf_count_chirho.saturating_add(validate_table_tree_chirho(
                entry_chirho.addr(),
                level_chirho - 1,
            )?);
        }
    }
    Ok(leaf_count_chirho)
}

fn release_tree_leaves_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
    released_leaves_chirho: &mut Vec<ReleasedLeafChirho>,
) -> Result<(), PageTableRetireErrorChirho> {
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    for entry_chirho in table_chirho.iter() {
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if level_chirho == 1 {
            let physical_address_chirho = entry_chirho.addr();
            let release_chirho = release_leaf_mapping_chirho(physical_address_chirho)
                .map_err(PageTableRetireErrorChirho::LeafReleaseChirho)?;
            released_leaves_chirho.push(ReleasedLeafChirho {
                physical_address_chirho,
                free_frame_chirho: release_chirho == FrameReleaseOutcomeChirho::LastReferenceChirho,
                managed_chirho: release_chirho != FrameReleaseOutcomeChirho::UnmanagedChirho,
            });
        } else {
            release_tree_leaves_chirho(
                entry_chirho.addr(),
                level_chirho - 1,
                released_leaves_chirho,
            )?;
        }
    }
    Ok(())
}

fn rollback_released_leaves_chirho(
    released_leaves_chirho: &[ReleasedLeafChirho],
) -> Result<(), PageTableRetireErrorChirho> {
    for released_leaf_chirho in released_leaves_chirho {
        if released_leaf_chirho.managed_chirho {
            retain_leaf_mapping_chirho(released_leaf_chirho.physical_address_chirho)
                .map_err(PageTableRetireErrorChirho::LeafRollbackChirho)?;
        }
    }
    Ok(())
}

fn clear_and_free_table_chirho(
    table_phys_chirho: PhysAddr,
    level_chirho: u8,
    released_leaves_chirho: &[ReleasedLeafChirho],
    released_index_chirho: &mut usize,
    stats_chirho: &mut AddressSpaceRetireStatsChirho,
) -> Result<(), PageTableRetireErrorChirho> {
    let table_chirho = unsafe { table_from_phys_chirho(table_phys_chirho) };
    for entry_chirho in table_chirho.iter_mut() {
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if level_chirho == 1 {
            let Some(released_leaf_chirho) = released_leaves_chirho.get(*released_index_chirho)
            else {
                return Err(PageTableRetireErrorChirho::TraversalChangedChirho);
            };
            if released_leaf_chirho.physical_address_chirho != entry_chirho.addr() {
                return Err(PageTableRetireErrorChirho::TraversalChangedChirho);
            }
            entry_chirho.set_unused();
            *released_index_chirho += 1;
            stats_chirho.leaf_mappings_released_chirho += 1;
            if released_leaf_chirho.free_frame_chirho {
                crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
                    released_leaf_chirho.physical_address_chirho,
                ));
                stats_chirho.leaf_frames_freed_chirho += 1;
            } else if !released_leaf_chirho.managed_chirho {
                stats_chirho.unmanaged_leaf_mappings_cleared_chirho += 1;
            }
        } else {
            let child_table_phys_chirho = entry_chirho.addr();
            clear_and_free_table_chirho(
                child_table_phys_chirho,
                level_chirho - 1,
                released_leaves_chirho,
                released_index_chirho,
                stats_chirho,
            )?;
            entry_chirho.set_unused();
            crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(
                child_table_phys_chirho,
            ));
            stats_chirho.table_frames_freed_chirho += 1;
        }
    }
    Ok(())
}

fn retire_page_table_tree_chirho(
    root_phys_chirho: PhysAddr,
) -> Result<AddressSpaceRetireStatsChirho, PageTableRetireErrorChirho> {
    if root_phys_chirho == get_boot_pml4_chirho() {
        return Err(PageTableRetireErrorChirho::BootRootChirho);
    }
    if Cr3::read().0.start_address() == root_phys_chirho {
        return Err(PageTableRetireErrorChirho::ActiveRootChirho);
    }
    if !frame_is_managed_chirho(root_phys_chirho) {
        return Err(PageTableRetireErrorChirho::UnmanagedTableFrameChirho {
            physical_address_chirho: root_phys_chirho.as_u64(),
        });
    }

    let root_table_chirho = unsafe { table_from_phys_chirho(root_phys_chirho) };
    let mut leaf_count_chirho = 0usize;
    for pml4_index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &root_table_chirho[pml4_index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        leaf_count_chirho =
            leaf_count_chirho.saturating_add(validate_table_tree_chirho(entry_chirho.addr(), 3)?);
    }

    let mut released_leaves_chirho = Vec::new();
    released_leaves_chirho
        .try_reserve_exact(leaf_count_chirho)
        .map_err(|_| PageTableRetireErrorChirho::ScratchAllocationFailedChirho)?;
    for pml4_index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &root_table_chirho[pml4_index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        if let Err(release_error_chirho) =
            release_tree_leaves_chirho(entry_chirho.addr(), 3, &mut released_leaves_chirho)
        {
            rollback_released_leaves_chirho(&released_leaves_chirho)?;
            return Err(release_error_chirho);
        }
    }

    let mut stats_chirho = AddressSpaceRetireStatsChirho::default();
    let mut released_index_chirho = 0usize;
    for pml4_index_chirho in 0..KERNEL_PML4_START_CHIRHO {
        let entry_chirho = &mut root_table_chirho[pml4_index_chirho];
        if entry_chirho.is_unused()
            || !entry_chirho
                .flags()
                .contains(PageTableFlags::USER_ACCESSIBLE)
        {
            continue;
        }
        let pdpt_phys_chirho = entry_chirho.addr();
        clear_and_free_table_chirho(
            pdpt_phys_chirho,
            3,
            &released_leaves_chirho,
            &mut released_index_chirho,
            &mut stats_chirho,
        )?;
        entry_chirho.set_unused();
        crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(pdpt_phys_chirho));
        stats_chirho.table_frames_freed_chirho += 1;
    }
    if released_index_chirho != released_leaves_chirho.len() {
        return Err(PageTableRetireErrorChirho::TraversalChangedChirho);
    }

    crate::mm_chirho::deallocate_frame_chirho(PhysFrame::containing_address(root_phys_chirho));
    stats_chirho.table_frames_freed_chirho += 1;
    Ok(stats_chirho)
}

pub(super) fn retire_unowned_page_table_chirho(
    root_phys_chirho: PhysAddr,
) -> Result<AddressSpaceRetireStatsChirho, PageTableRetireErrorChirho> {
    retire_page_table_tree_chirho(root_phys_chirho)
}

pub fn unretired_last_handle_drops_chirho() -> u64 {
    UNRETIRED_LAST_HANDLE_DROPS_CHIRHO.load(Ordering::Acquire)
}

pub fn handle_refcount_failures_chirho() -> u64 {
    HANDLE_REFCOUNT_FAILURES_CHIRHO.load(Ordering::Acquire)
}
