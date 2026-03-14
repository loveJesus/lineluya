// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel heap allocator module.
//!
//! Provides a locked linked-list heap allocator for dynamic memory allocation
//! in a bare-metal `#![no_std]` environment. Maps virtual pages to physical
//! frames and initialises the global allocator over the mapped region.

use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

/// Virtual address where the kernel heap begins.
pub const HEAP_START_CHIRHO: usize = 0x_4444_4444_0000;

/// Size of the kernel heap in bytes (1 MiB).
pub const HEAP_SIZE_CHIRHO: usize = 1024 * 1024;

/// Global heap allocator backed by a locked linked-list allocator.
#[global_allocator]
static ALLOCATOR_CHIRHO: LockedHeap = LockedHeap::empty();

/// Initialise the kernel heap.
///
/// Creates a contiguous range of virtual pages starting at [`HEAP_START_CHIRHO`]
/// spanning [`HEAP_SIZE_CHIRHO`] bytes, maps each page to a freshly allocated
/// physical frame with `PRESENT | WRITABLE` flags, and then hands the region
/// to the global [`ALLOCATOR_CHIRHO`].
///
/// # Errors
///
/// Returns [`MapToError`] if any page cannot be mapped (e.g. the frame
/// allocator is exhausted or the page is already mapped).
///
/// # Safety
///
/// This function must be called exactly once, after paging has been set up and
/// before any heap allocation is attempted.
pub fn init_heap_chirho(
    mapper_chirho: &mut impl Mapper<Size4KiB>,
    frame_allocator_chirho: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range_chirho = {
        let heap_start_chirho = VirtAddr::new(HEAP_START_CHIRHO as u64);
        let heap_end_chirho = heap_start_chirho + HEAP_SIZE_CHIRHO as u64 - 1u64;
        let heap_start_page_chirho = Page::containing_address(heap_start_chirho);
        let heap_end_page_chirho = Page::containing_address(heap_end_chirho);
        Page::range_inclusive(heap_start_page_chirho, heap_end_page_chirho)
    };

    let flags_chirho = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    for page_chirho in page_range_chirho {
        let frame_chirho = frame_allocator_chirho
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;

        // SAFETY: We are mapping freshly allocated frames that are not yet in
        // use, so there is no aliasing.  The caller guarantees that paging is
        // properly initialised and that this function is invoked only once.
        unsafe {
            mapper_chirho
                .map_to(page_chirho, frame_chirho, flags_chirho, frame_allocator_chirho)?
                .flush();
        }
    }

    // SAFETY: The virtual address range [HEAP_START_CHIRHO, HEAP_START_CHIRHO +
    // HEAP_SIZE_CHIRHO) has just been mapped to valid physical memory, so it is
    // safe to hand to the allocator.
    unsafe {
        ALLOCATOR_CHIRHO
            .lock()
            .init(HEAP_START_CHIRHO as *mut u8, HEAP_SIZE_CHIRHO);
    }

    Ok(())
}
