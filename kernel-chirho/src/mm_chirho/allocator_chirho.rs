// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel heap allocator module.
//!
//! Uses the `buddy-alloc` crate — a battle-tested buddy system allocator
//! for no_std Rust. Handles large contiguous allocations (64MB+) without
//! fragmentation, with correct buddy merging via bitmap tracking.

// ---------------------------------------------------------------------------
// Allocation size classification
// ---------------------------------------------------------------------------

/// Allocation size classification for logging and policy decisions.
///
/// Centralises the thresholds used throughout the allocator so that policy
/// changes (e.g. adjusting what counts as "large") only need to happen in
/// one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationClassChirho {
    /// Normal allocation (< 64 KB).
    NormalChirho,
    /// Large allocation (64 KB – 1 MB) — logged for diagnostics.
    LargeChirho,
    /// Huge allocation (1 MB – 128 MB) — logged with a warning.
    HugeChirho,
    /// Oversized (> 128 MB) — rejected by the allocator.
    OversizedChirho,
}

/// Threshold constant: allocations >= this are classified as [`LargeChirho`].
const LARGE_THRESHOLD_CHIRHO: usize = 64 * 1024; // 64 KB

/// Threshold constant: allocations >= this are classified as [`HugeChirho`].
const HUGE_THRESHOLD_CHIRHO: usize = 1024 * 1024; // 1 MB

/// Hard cap: allocations larger than this are refused (returns null).
const OVERSIZED_THRESHOLD_CHIRHO: usize = 128 * 1024 * 1024; // 128 MB

impl AllocationClassChirho {
    /// Classify an allocation by its requested byte size.
    pub fn classify_chirho(size_chirho: usize) -> Self {
        if size_chirho >= OVERSIZED_THRESHOLD_CHIRHO {
            Self::OversizedChirho
        } else if size_chirho >= HUGE_THRESHOLD_CHIRHO {
            Self::HugeChirho
        } else if size_chirho >= LARGE_THRESHOLD_CHIRHO {
            Self::LargeChirho
        } else {
            Self::NormalChirho
        }
    }
}

// ---------------------------------------------------------------------------
// Buddy-alloc crate imports
// ---------------------------------------------------------------------------

use buddy_alloc::{BuddyAllocParam, FastAllocParam, NonThreadsafeAlloc};
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

/// Virtual address where the kernel heap begins.
pub const HEAP_START_CHIRHO: usize = 0x_4444_4444_0000;

/// Total heap: 288 MB. Works with QEMU -m 512M.
pub const HEAP_SIZE_CHIRHO: usize = 288 * 1024 * 1024;

/// Fast allocator: 32 MiB for small/medium objects.
const FAST_HEAP_SIZE_CHIRHO: usize = 32 * 1024 * 1024;

/// Buddy allocator: 256 MiB.
const BUDDY_HEAP_SIZE_CHIRHO: usize = 256 * 1024 * 1024;

/// Kernel heap configuration.
pub struct HeapConfigChirho;

impl HeapConfigChirho {
    pub const START_CHIRHO: usize = HEAP_START_CHIRHO;
    pub const TOTAL_SIZE_CHIRHO: usize = HEAP_SIZE_CHIRHO;
    pub const FAST_SIZE_CHIRHO: usize = FAST_HEAP_SIZE_CHIRHO;
    pub const BUDDY_SIZE_CHIRHO: usize = BUDDY_HEAP_SIZE_CHIRHO;
    pub const BUDDY_LEAF_SIZE_CHIRHO: usize = 4096;
}

/// Global counter for large (>256KB) allocations.
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
        let class_chirho = AllocationClassChirho::classify_chirho(size_chirho);

        // Track large+ allocations via the global counter.
        match class_chirho {
            AllocationClassChirho::LargeChirho
            | AllocationClassChirho::HugeChirho => {
                LARGE_ALLOC_COUNT_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            AllocationClassChirho::OversizedChirho => {
                // Hard cap — refuse the allocation.
                crate::serial_debug_chirho!(
                    "[ALLOC] REJECTED {:?} {}B a={}",
                    class_chirho, size_chirho, layout_chirho.align(),
                );
                return core::ptr::null_mut();
            }
            AllocationClassChirho::NormalChirho => {}
        }

        // Log huge allocations with syscall context for debugging.
        if matches!(class_chirho, AllocationClassChirho::HugeChirho) {
            let sc_chirho = crate::syscall_chirho::LAST_SYSCALL_NR_CHIRHO
                .load(core::sync::atomic::Ordering::Relaxed);
            let sc_name_chirho = crate::syscall_chirho::syscall_name_chirho(sc_chirho);
            crate::serial_debug_chirho!(
                "[ALLOC] {:?} {}B a={} sc={}({})",
                class_chirho, size_chirho, layout_chirho.align(), sc_chirho, sc_name_chirho,
            );
        }

        // For allocations > 2MB, use frame-based large pool to avoid
        // buddy allocator fragmentation/OOM (e.g., ext4 Vec doubling).
        if size_chirho > 2 * 1024 * 1024 {
            let p_chirho = large_alloc_chirho(size_chirho);
            if !p_chirho.is_null() {
                return p_chirho;
            }
            // Fall through to buddy if large pool fails
        }
        INNER_ALLOC_CHIRHO.alloc(layout_chirho)
    }

    unsafe fn dealloc(&self, ptr_chirho: *mut u8, layout_chirho: core::alloc::Layout) {
        // Check if this pointer is in the large pool range
        let addr_chirho = ptr_chirho as usize;
        if addr_chirho >= LARGE_POOL_START_CHIRHO
            && addr_chirho < LARGE_POOL_START_CHIRHO + LARGE_POOL_SIZE_CHIRHO
        {
            large_dealloc_chirho(ptr_chirho, layout_chirho.size());
            return;
        }
        INNER_ALLOC_CHIRHO.dealloc(ptr_chirho, layout_chirho)
    }
}

/// Global heap allocator with tracing for large allocations.
#[global_allocator]
static ALLOCATOR_CHIRHO: TracingAllocChirho = TracingAllocChirho;

// ---------------------------------------------------------------------------
// Large allocation pool — bypasses buddy allocator for >1MB allocs
// ---------------------------------------------------------------------------
// Uses a simple bump allocator in a dedicated virtual address range.
// Each allocation gets its own contiguous page-aligned region.
// Freed regions are tracked in a free list for reuse.

/// Start of the large allocation virtual address space.
/// Placed right after the main heap (0x4444_4444_0000 + HEAP_SIZE).
const LARGE_POOL_START_CHIRHO: usize = HEAP_START_CHIRHO + HEAP_SIZE_CHIRHO;
/// Maximum size of the large allocation pool (256 MB).
const LARGE_POOL_SIZE_CHIRHO: usize = 256 * 1024 * 1024;
/// Current bump pointer for the large pool.
static LARGE_POOL_NEXT_CHIRHO: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(LARGE_POOL_START_CHIRHO);

/// Allocate from the large pool by mapping physical frames directly.
unsafe fn large_alloc_chirho(size_chirho: usize) -> *mut u8 {
    let aligned_size_chirho = (size_chirho + 0xFFF) & !0xFFF; // page-align
    let addr_chirho = LARGE_POOL_NEXT_CHIRHO.fetch_add(
        aligned_size_chirho,
        core::sync::atomic::Ordering::SeqCst,
    );

    // Check bounds
    if addr_chirho + aligned_size_chirho > LARGE_POOL_START_CHIRHO + LARGE_POOL_SIZE_CHIRHO {
        // Out of pool space — revert bump pointer
        LARGE_POOL_NEXT_CHIRHO.fetch_sub(
            aligned_size_chirho,
            core::sync::atomic::Ordering::SeqCst,
        );
        return core::ptr::null_mut();
    }

    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let num_pages_chirho = aligned_size_chirho / 0x1000;

    // Allocate and map pages one at a time.
    // Map into BOOT PML4 (not current CR3) so pages persist across context switches.
    let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
    let flags_chirho = x86_64::structures::paging::PageTableFlags::PRESENT
        | x86_64::structures::paging::PageTableFlags::WRITABLE;

    for i_chirho in 0..num_pages_chirho {
        let vaddr_chirho = addr_chirho + i_chirho * 0x1000;
        // Allocate one frame
        let phys_chirho = {
            let mut alloc_lock_chirho = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
            if let Some(alloc_chirho) = alloc_lock_chirho.as_mut() {
                if let Some(frame_chirho) = alloc_chirho.allocate_frame() {
                    frame_chirho.start_address().as_u64()
                } else {
                    return core::ptr::null_mut();
                }
            } else {
                return core::ptr::null_mut();
            }
        }; // Lock released
        // Zero the frame via physical memory offset
        core::ptr::write_bytes(
            (phys_chirho + phys_offset_chirho) as *mut u8, 0, 0x1000,
        );
        // Map into boot PML4 (persistent across all tasks' page tables)
        let _ = crate::pagetable_chirho::map_page_in_pt_chirho(
            boot_pml4_chirho, vaddr_chirho as u64, phys_chirho, flags_chirho,
        );
        // Also map in current CR3 for immediate access
        let (cur_cr3_chirho, _) = x86_64::registers::control::Cr3::read();
        if cur_cr3_chirho.start_address() != boot_pml4_chirho {
            let _ = crate::pagetable_chirho::map_page_in_pt_chirho(
                cur_cr3_chirho.start_address(), vaddr_chirho as u64, phys_chirho, flags_chirho,
            );
        }
    }

    addr_chirho as *mut u8
}

/// Deallocate from the large pool (unmap pages but don't return frames).
/// Simple: just unmap. The bump pointer never reclaims — acceptable for
/// temporary Vec doubling where the old buffer is freed immediately.
unsafe fn large_dealloc_chirho(ptr_chirho: *mut u8, size_chirho: usize) {
    let addr_chirho = ptr_chirho as usize;
    if addr_chirho < LARGE_POOL_START_CHIRHO
        || addr_chirho >= LARGE_POOL_START_CHIRHO + LARGE_POOL_SIZE_CHIRHO
    {
        // Not from the large pool — fall back to buddy
        use core::alloc::GlobalAlloc;
        INNER_ALLOC_CHIRHO.dealloc(ptr_chirho, core::alloc::Layout::from_size_align_unchecked(size_chirho, 8));
        return;
    }
    // Unmap pages and return frames to the frame allocator
    let aligned_size_chirho = (size_chirho + 0xFFF) & !0xFFF;
    let num_pages_chirho = aligned_size_chirho / 0x1000;
    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();

    let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
    let mut alloc_lock_chirho = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
    for i_chirho in 0..num_pages_chirho {
        let vaddr_chirho = (addr_chirho + i_chirho * 0x1000) as u64;
        // Walk boot PML4 to find the physical frame
        if let Some(pte_ptr_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
            boot_pml4_chirho, x86_64::VirtAddr::new(vaddr_chirho),
        ) {
            let pte_chirho = &*pte_ptr_chirho;
            if pte_chirho.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                let phys_chirho = pte_chirho.addr().as_u64();
                // Clear the PTE
                (*pte_ptr_chirho).set_unused();
                x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(vaddr_chirho));
                // Return frame to allocator
                use x86_64::structures::paging::PhysFrame;
                use x86_64::PhysAddr;
                if let Some(alloc_ref_chirho) = alloc_lock_chirho.as_mut() {
                    let frame_chirho = PhysFrame::containing_address(PhysAddr::new(phys_chirho));
                    alloc_ref_chirho.deallocate_frame_chirho(frame_chirho);
                }
            }
        }
    }
}

/// Initialise the kernel heap by mapping pages.
///
/// Maps heap pages and initializes the linked-list allocator.
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
    // Kill the current task instead of halting the entire kernel.
    // This allows other tasks (xterm, twm, dropbear) to continue.
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let pid_chirho = task_arc_chirho.lock().pid_chirho;
        let oom_msg_chirho = b"\r\n[ALLOC] Killing PID ";
        for &b_chirho in oom_msg_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        let mut pd_chirho = [0u8; 10];
        let mut pi_chirho = 0usize;
        let mut pn_chirho = pid_chirho as usize;
        if pn_chirho == 0 { pd_chirho[0] = b'0'; pi_chirho = 1; }
        else { while pn_chirho > 0 { pd_chirho[pi_chirho] = b'0' + (pn_chirho % 10) as u8; pn_chirho /= 10; pi_chirho += 1; } }
        for j_chirho in (0..pi_chirho).rev() {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(pd_chirho[j_chirho]);
            }
        }
        unsafe {
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b'\r');
            while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
            x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b'\n');
        }
        // Allocation-failure context must not recurse into VFS/pipe teardown.
        // Mark the task and let the next ordinary syscall retire descriptors.
        crate::process_chirho::exit_task_with_deferred_descriptor_retirement_chirho(
            &task_arc_chirho,
            137,
        );
        // Yield to scheduler — since we're zombie, we won't be scheduled again
        crate::scheduler_chirho::schedule_chirho();
    }
    // Fallback: HLT loop if no current task
    loop { x86_64::instructions::hlt(); }
}
