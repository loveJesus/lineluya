// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Memory management module for the Lineluya kernel.
//!
//! Provides:
//! - **Page mapper initialisation** via [`init_mapper_chirho`], which reads the
//!   active CR3 register and constructs an [`OffsetPageTable`] backed by the
//!   bootloader's identity-mapped physical memory region.
//! - **Physical frame allocation** via [`BootInfoFrameAllocatorChirho`], which
//!   hands out usable physical frames reported by the bootloader's memory map.
//! - A small [`create_mapping_example_chirho`] helper for testing page mappings.

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
    Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

// ---------------------------------------------------------------------------
// Page mapper initialisation
// ---------------------------------------------------------------------------

/// Initialise an [`OffsetPageTable`] from the currently active level-4 page
/// table.
///
/// The bootloader maps *all* physical memory starting at
/// `physical_memory_offset_chirho`, so any physical address `p` is accessible
/// at virtual address `physical_memory_offset_chirho + p`.  This function
/// reads CR3 to locate the level-4 table, converts the physical address to a
/// virtual one using that offset, and wraps the result in a safe
/// `OffsetPageTable` abstraction.
///
/// # Safety
///
/// The caller must guarantee that:
/// 1. `physical_memory_offset_chirho` is the correct offset at which the
///    bootloader has identity-mapped all physical memory.
/// 2. The entire physical memory is mapped at this offset.
/// 3. This function is called only once to avoid aliasing `&mut` references to
///    the same page table.
pub unsafe fn init_mapper_chirho(
    physical_memory_offset_chirho: u64,
) -> OffsetPageTable<'static> {
    let virt_offset_chirho = VirtAddr::new(physical_memory_offset_chirho);
    let level_4_table_chirho = active_level_4_table_chirho(virt_offset_chirho);

    // SAFETY: The caller guarantees the physical-memory offset is correct and
    // that we hold the only mutable reference to the level-4 table.
    OffsetPageTable::new(level_4_table_chirho, virt_offset_chirho)
}

/// Return a mutable reference to the active level-4 page table.
///
/// # Safety
///
/// The caller must ensure:
/// - `physical_memory_offset_chirho` correctly maps all physical memory.
/// - This function is only invoked once (or behind an appropriate
///   synchronisation primitive) to prevent aliased `&mut` references.
unsafe fn active_level_4_table_chirho(
    physical_memory_offset_chirho: VirtAddr,
) -> &'static mut PageTable {
    let (level_4_frame_chirho, _flags_chirho) = Cr3::read();
    let phys_addr_chirho = level_4_frame_chirho.start_address();
    let virt_addr_chirho = physical_memory_offset_chirho + phys_addr_chirho.as_u64();
    let page_table_ptr_chirho: *mut PageTable = virt_addr_chirho.as_mut_ptr();

    // SAFETY: The pointer is derived from the live CR3 value and the
    // bootloader-provided physical-memory offset, so it points to a valid,
    // aligned `PageTable`.  The caller ensures no aliasing.
    &mut *page_table_ptr_chirho
}

// ---------------------------------------------------------------------------
// Physical frame allocator
// ---------------------------------------------------------------------------

/// A [`FrameAllocator`] backed by the bootloader-provided memory map.
///
/// Iterates over `Usable` memory regions and yields 4 KiB-aligned physical
/// frames on demand.  Allocation is bump-style (no deallocation) which is
/// sufficient for early kernel boot and heap setup.  A more sophisticated
/// allocator should be layered on top once the heap is available.
///
/// Because Rust iterators are difficult to store in a struct (they carry
/// complex generic types), this allocator uses a simple index-based approach:
/// it keeps a reference to the full memory region slice and a counter
/// (`next_frame_index_chirho`) that advances through the flattened list of
/// usable frames.
pub struct BootInfoFrameAllocatorChirho {
    /// Bootloader-provided memory regions (lifetime-bound to boot info).
    memory_regions_chirho: &'static MemoryRegions,
    /// Index of the next frame to hand out across *all* usable regions.
    next_frame_index_chirho: usize,
}

impl BootInfoFrameAllocatorChirho {
    /// Create a new allocator from the bootloader memory map.
    ///
    /// No frames are allocated until [`FrameAllocator::allocate_frame`] is
    /// called.
    pub fn init_chirho(memory_regions_chirho: &'static MemoryRegions) -> Self {
        Self {
            memory_regions_chirho,
            next_frame_index_chirho: 0,
        }
    }

    /// Return an iterator over all usable physical frames reported by the
    /// bootloader.
    ///
    /// Each `Usable` memory region is split into 4 KiB frames.  The iterator
    /// yields [`PhysFrame`] values in ascending physical-address order.
    fn usable_frames_chirho(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_regions_chirho
            .iter()
            .filter(|region_chirho| region_chirho.kind == MemoryRegionKind::Usable)
            .flat_map(|region_chirho| {
                let start_addr_chirho = region_chirho.start;
                let end_addr_chirho = region_chirho.end;

                // Align the start address up to the next 4 KiB boundary.
                let first_frame_addr_chirho = align_up_chirho(start_addr_chirho, 4096);

                // Generate frame-start addresses within this region.
                (first_frame_addr_chirho..end_addr_chirho)
                    .step_by(4096)
                    .map(|addr_chirho| {
                        PhysFrame::containing_address(PhysAddr::new(addr_chirho))
                    })
            })
    }
}

// SAFETY: `allocate_frame` only returns frames from memory regions the
// bootloader has marked as `Usable`, meaning they are not in use by the
// bootloader, kernel image, or hardware.  Each frame is returned at most
// once because `next_frame_index_chirho` advances monotonically.
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocatorChirho {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame_chirho = self
            .usable_frames_chirho()
            .nth(self.next_frame_index_chirho);
        self.next_frame_index_chirho += 1;
        frame_chirho
    }
}

// ---------------------------------------------------------------------------
// Mapping helper (useful for testing / demonstration)
// ---------------------------------------------------------------------------

/// Create an example mapping that maps `page_chirho` to `frame_chirho`.
///
/// This is primarily useful for testing the paging infrastructure.  In
/// production, higher-level allocation routines should be preferred.
///
/// # Arguments
///
/// * `page_chirho`            - The virtual [`Page`] to map.
/// * `frame_chirho`           - The physical [`PhysFrame`] to map it to.
/// * `mapper_chirho`          - The active page table mapper.
/// * `frame_allocator_chirho` - Allocator for intermediate page-table frames.
///
/// # Safety
///
/// Creating arbitrary mappings can violate memory safety (e.g., aliasing the
/// same physical frame, mapping over existing entries).  The caller must
/// ensure the mapping is sound.
pub unsafe fn create_mapping_example_chirho(
    page_chirho: Page<Size4KiB>,
    frame_chirho: PhysFrame<Size4KiB>,
    mapper_chirho: &mut OffsetPageTable,
    frame_allocator_chirho: &mut impl FrameAllocator<Size4KiB>,
) {
    let flags_chirho =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    // SAFETY: The caller guarantees the mapping is valid and does not alias
    // existing mappings or violate memory safety.
    let map_result_chirho = unsafe {
        mapper_chirho.map_to(page_chirho, frame_chirho, flags_chirho, frame_allocator_chirho)
    };

    // `map_to` returns a `MapperFlush` that must be consumed to flush the TLB
    // entry for this page.  Forgetting to flush would leave stale translations
    // in the TLB, which can cause incorrect memory accesses.
    map_result_chirho
        .expect("map_to failed")
        .flush();
}

// ---------------------------------------------------------------------------
// Internal utilities
// ---------------------------------------------------------------------------

/// Align `addr_chirho` upward to the nearest multiple of `align_chirho`.
///
/// `align_chirho` must be a power of two.
const fn align_up_chirho(addr_chirho: u64, align_chirho: u64) -> u64 {
    // Classic bit-manipulation trick: (addr + align - 1) & !(align - 1)
    (addr_chirho + align_chirho - 1) & !(align_chirho - 1)
}
