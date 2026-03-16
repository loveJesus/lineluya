// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel heap allocator module.
//!
//! Uses locked linked-list heap allocator. The 128MB heap with 16MB page
//! cache avoids the heap corruption that occurs under heavy churn.
//! TODO: Replace with a more robust allocator (talc GPFs, need investigation).

use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

/// Virtual address where the kernel heap begins.
pub const HEAP_START_CHIRHO: usize = 0x_4444_4444_0000;

/// Size of the kernel heap in bytes (256 MiB).
/// Pre-generated SSH keys avoid dropbear's 128MB key-gen allocation.
pub const HEAP_SIZE_CHIRHO: usize = 256 * 1024 * 1024;

/// Global heap allocator.
#[global_allocator]
static ALLOCATOR_CHIRHO: LockedHeap = LockedHeap::empty();

/// Initialise the kernel heap.
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

        unsafe {
            mapper_chirho
                .map_to(page_chirho, frame_chirho, flags_chirho, frame_allocator_chirho)?
                .flush();
        }
    }

    unsafe {
        ALLOCATOR_CHIRHO
            .lock()
            .init(HEAP_START_CHIRHO as *mut u8, HEAP_SIZE_CHIRHO);
    }

    Ok(())
}

/// Custom allocation error handler — logs and aborts the allocation
/// instead of panicking. This prevents userspace OOM from crashing
/// the kernel. The caller (mmap, Vec, etc.) gets a null pointer and
/// should return -ENOMEM to userspace.
#[alloc_error_handler]
fn alloc_error_handler_chirho(layout_chirho: core::alloc::Layout) -> ! {
    // Use raw serial to avoid allocation in the error path
    let msg_chirho = b"\r\n[ALLOC] OOM: ";
    for &b_chirho in msg_chirho {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
        }
    }
    // Print size as decimal
    let size_chirho = layout_chirho.size();
    let mut digits_chirho = [0u8; 20];
    let mut n_chirho = size_chirho;
    let mut i_chirho = 0usize;
    if n_chirho == 0 {
        digits_chirho[0] = b'0';
        i_chirho = 1;
    } else {
        while n_chirho > 0 {
            digits_chirho[i_chirho] = b'0' + (n_chirho % 10) as u8;
            n_chirho /= 10;
            i_chirho += 1;
        }
    }
    for j_chirho in (0..i_chirho).rev() {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(digits_chirho[j_chirho]);
        }
    }
    let suffix_chirho = b" bytes\r\n";
    for &b_chirho in suffix_chirho {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
        }
    }
    // Halt instead of panic — prevents cascading failures
    loop { x86_64::instructions::hlt(); }
}
