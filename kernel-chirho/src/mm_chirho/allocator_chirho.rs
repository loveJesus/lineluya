// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel heap allocator module.
//!
//! Uses the `buddy-alloc` crate — a battle-tested buddy system allocator
//! for no_std Rust. Handles large contiguous allocations (64MB+) without
//! fragmentation, with correct buddy merging via bitmap tracking.

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

/// Virtual address where the kernel heap begins.
pub const HEAP_START_CHIRHO: usize = 0x_4444_4444_0000;

/// Total mapped heap: 32MB fast + 256MB buddy = 288MB.
/// Works with QEMU -m 512M.
/// NOTE: dropbear needs QEMU -m 4G with 1GB buddy due to a mystery
/// Vec<align=8> growing to 128MB+ during ext4 path resolution.
/// TODO: Find and fix the root cause of the unbounded a=8 allocs.
pub const HEAP_SIZE_CHIRHO: usize = 288 * 1024 * 1024;

/// Fast allocator: 32 MiB for small/medium objects.
const FAST_HEAP_SIZE_CHIRHO: usize = 32 * 1024 * 1024;

/// Buddy allocator: 256 MiB.
const BUDDY_HEAP_SIZE_CHIRHO: usize = 256 * 1024 * 1024;

/// Global counter for large (>256KB) allocations.
/// Used by diagnostic logging to correlate allocs with code sections.
pub static LARGE_ALLOC_COUNT_CHIRHO: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Inner allocator from buddy-alloc crate.
static INNER_ALLOC_CHIRHO: NonThreadsafeAlloc = unsafe {
    let fast_param_chirho = FastAllocParam::new(
        HEAP_START_CHIRHO as *const u8,
        FAST_HEAP_SIZE_CHIRHO,
    );
    let buddy_param_chirho = BuddyAllocParam::new(
        (HEAP_START_CHIRHO + FAST_HEAP_SIZE_CHIRHO) as *const u8,
        BUDDY_HEAP_SIZE_CHIRHO,
        4096,
    );
    NonThreadsafeAlloc::new(fast_param_chirho, buddy_param_chirho)
};

/// Wrapper that logs large allocations for debugging.
struct TracingAllocChirho;

unsafe impl core::alloc::GlobalAlloc for TracingAllocChirho {
    unsafe fn alloc(&self, layout_chirho: core::alloc::Layout) -> *mut u8 {
        let size_chirho = layout_chirho.size();

        // Track large allocations
        if size_chirho > 256 * 1024 {
            LARGE_ALLOC_COUNT_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        // Cap: refuse allocations > 128MB.
        if size_chirho > 128 * 1024 * 1024 {
            return core::ptr::null_mut();
        }

        // Log allocations > 1MB with syscall name for debugging.
        if size_chirho > 1 * 1024 * 1024 {
            let sc_chirho = crate::syscall_chirho::LAST_SYSCALL_NR_CHIRHO
                .load(core::sync::atomic::Ordering::Relaxed);
            let sc_name_chirho = crate::syscall_chirho::syscall_name_chirho(sc_chirho);
            crate::serial_println_chirho!(
                "[ALLOC] {}B a={} sc={}({})",
                size_chirho, layout_chirho.align(), sc_chirho, sc_name_chirho,
            );
        }

        INNER_ALLOC_CHIRHO.alloc(layout_chirho)
    }

    unsafe fn dealloc(&self, ptr_chirho: *mut u8, layout_chirho: core::alloc::Layout) {
        // Track large deallocations to detect leaks vs. Vec doubling.
        if layout_chirho.size() > 1024 * 1024 && layout_chirho.align() == 8 {
            crate::serial_println_chirho!(
                "[DEALLOC] {}B a=8",
                layout_chirho.size(),
            );
        }
        INNER_ALLOC_CHIRHO.dealloc(ptr_chirho, layout_chirho)
    }
}

/// Global heap allocator with tracing for large allocations.
#[global_allocator]
static ALLOCATOR_CHIRHO: TracingAllocChirho = TracingAllocChirho;

/// Initialise the kernel heap by mapping pages.
///
/// The buddy-alloc crate initializes itself lazily on first use,
/// so we only need to map the virtual pages to physical frames here.
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

    Ok(())
}

/// Custom allocation error handler — logs size, align, and caller address.
#[alloc_error_handler]
fn alloc_error_handler_chirho(layout_chirho: core::alloc::Layout) -> ! {
    // Scan stack for kernel code addresses to find the caller.
    // Since frame pointers are optimized out, scan RSP upward for
    // values that look like kernel text addresses (0x10000XXXXXX).
    let mut rsp_chirho: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp_chirho); }

    let stack_msg_chirho = b"\r\n[ALLOC] Stack scan: ";
    for &b_chirho in stack_msg_chirho {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
        }
    }
    let hex_chars2_chirho = b"0123456789abcdef";
    let mut found_chirho = 0u32;
    for off_chirho in (0..256).step_by(8) {
        if found_chirho >= 6 { break; }
        let addr_chirho = rsp_chirho + off_chirho as u64;
        let val_chirho = unsafe { *(addr_chirho as *const u64) };
        // Check if it looks like a kernel text address
        if val_chirho > 0x10000100000 && val_chirho < 0x10000200000 {
            // Print the kernel offset (subtract base)
            let offset_chirho = val_chirho - 0x10000000000;
            for shift_chirho in (0..6).rev() {
                let nibble_chirho = ((offset_chirho >> (shift_chirho * 4)) & 0xF) as usize;
                unsafe {
                    while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                    x86_64::instructions::port::Port::<u8>::new(0x3F8).write(hex_chars2_chirho[nibble_chirho]);
                }
            }
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b' ');
            }
            found_chirho += 1;
        }
    }

    let caller_chirho: u64 = 0;

    let caller_msg_chirho = b"\r\n[ALLOC] OOM caller=0x";
    for &b_chirho in caller_msg_chirho {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
        }
    }
    let hex_chirho = b"0123456789abcdef";
    for shift_chirho in (0..16).rev() {
        let nibble_chirho = ((caller_chirho >> (shift_chirho * 4)) & 0xF) as usize;
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(hex_chirho[nibble_chirho]);
        }
    }

    // Use raw serial to avoid allocation in the error path
    let msg_chirho = b"\r\n[ALLOC] OOM: ";
    for &b_chirho in msg_chirho {
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
        }
    }
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
    loop { x86_64::instructions::hlt(); }
}
