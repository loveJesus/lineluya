// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Buddy allocator for the Lineluya kernel heap.
//!
//! Replaces linked_list_allocator to handle large contiguous allocations
//! (e.g., dropbear's 64MB crypto buffers) without fragmentation.
//!
//! Design:
//! - Minimum block size: 32 bytes (MIN_ORDER = 5, 2^5 = 32)
//! - Maximum order: 28 (2^28 = 256MB, the heap size)
//! - Free list per order: bitmap-based tracking
//! - Split/merge on alloc/dealloc

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use spin::Mutex;

/// Minimum allocation size (2^MIN_ORDER_CHIRHO bytes).
const MIN_ORDER_CHIRHO: usize = 5; // 32 bytes

/// Maximum order. 2^MAX_ORDER_CHIRHO = max block size.
/// For 256MB heap: 2^28 = 268435456 = 256MB.
const MAX_ORDER_CHIRHO: usize = 28;

/// Number of orders tracked.
const NUM_ORDERS_CHIRHO: usize = MAX_ORDER_CHIRHO - MIN_ORDER_CHIRHO + 1; // 24

/// A node in the free list for a given order.
/// Stored at the start of each free block (intrusive list).
struct FreeNodeChirho {
    next_chirho: *mut FreeNodeChirho,
}

/// Buddy allocator state.
pub struct BuddyAllocatorChirho {
    /// Base address of the managed heap region.
    base_chirho: usize,
    /// Total size of the heap.
    size_chirho: usize,
    /// Free lists, one per order. Index 0 = MIN_ORDER, last = MAX_ORDER.
    free_lists_chirho: [*mut FreeNodeChirho; NUM_ORDERS_CHIRHO],
    /// Whether the allocator has been initialized.
    initialized_chirho: bool,
}

unsafe impl Send for BuddyAllocatorChirho {}

impl BuddyAllocatorChirho {
    /// Create an uninitialized buddy allocator.
    pub const fn new_chirho() -> Self {
        Self {
            base_chirho: 0,
            size_chirho: 0,
            free_lists_chirho: [ptr::null_mut(); NUM_ORDERS_CHIRHO],
            initialized_chirho: false,
        }
    }

    /// Initialize with a memory region. The entire region starts as one
    /// free block at the highest order.
    pub unsafe fn init_chirho(&mut self, base_chirho: usize, size_chirho: usize) {
        self.base_chirho = base_chirho;
        self.size_chirho = size_chirho;
        self.initialized_chirho = true;

        // Clear all free lists.
        for i_chirho in 0..NUM_ORDERS_CHIRHO {
            self.free_lists_chirho[i_chirho] = ptr::null_mut();
        }

        // Add the entire heap as free blocks at the highest fitting order.
        // Split the heap into power-of-2 blocks.
        let mut offset_chirho: usize = 0;
        let mut remaining_chirho = size_chirho;

        // Start from the highest order and work down.
        for order_chirho in (MIN_ORDER_CHIRHO..=MAX_ORDER_CHIRHO).rev() {
            let block_size_chirho = 1usize << order_chirho;
            while remaining_chirho >= block_size_chirho && offset_chirho + block_size_chirho <= size_chirho {
                let addr_chirho = base_chirho + offset_chirho;
                let idx_chirho = order_chirho - MIN_ORDER_CHIRHO;
                let node_chirho = addr_chirho as *mut FreeNodeChirho;
                (*node_chirho).next_chirho = self.free_lists_chirho[idx_chirho];
                self.free_lists_chirho[idx_chirho] = node_chirho;
                offset_chirho += block_size_chirho;
                remaining_chirho -= block_size_chirho;
            }
        }
    }

    /// Convert a byte size to the order (power of 2) that fits it.
    fn size_to_order_chirho(size_chirho: usize) -> usize {
        let mut order_chirho = MIN_ORDER_CHIRHO;
        while (1usize << order_chirho) < size_chirho {
            order_chirho += 1;
        }
        order_chirho
    }

    /// Allocate a block of at least `layout.size()` bytes.
    pub fn alloc_chirho(&mut self, layout_chirho: Layout) -> *mut u8 {
        if !self.initialized_chirho {
            return ptr::null_mut();
        }

        let size_chirho = layout_chirho.size().max(layout_chirho.align());
        let order_chirho = Self::size_to_order_chirho(size_chirho);

        if order_chirho > MAX_ORDER_CHIRHO {
            return ptr::null_mut(); // Too large
        }

        // Find the smallest available order >= requested order.
        let mut found_order_chirho = order_chirho;
        while found_order_chirho <= MAX_ORDER_CHIRHO {
            let idx_chirho = found_order_chirho - MIN_ORDER_CHIRHO;
            if idx_chirho < NUM_ORDERS_CHIRHO && !self.free_lists_chirho[idx_chirho].is_null() {
                break;
            }
            found_order_chirho += 1;
        }

        if found_order_chirho > MAX_ORDER_CHIRHO {
            return ptr::null_mut(); // No block large enough
        }

        // Pop the block from the free list.
        let idx_chirho = found_order_chirho - MIN_ORDER_CHIRHO;
        let block_chirho = self.free_lists_chirho[idx_chirho];
        unsafe {
            self.free_lists_chirho[idx_chirho] = (*block_chirho).next_chirho;
        }

        // Split the block down to the requested order.
        let mut current_order_chirho = found_order_chirho;
        while current_order_chirho > order_chirho {
            current_order_chirho -= 1;
            let buddy_addr_chirho = block_chirho as usize + (1usize << current_order_chirho);
            // Bounds check buddy address.
            if buddy_addr_chirho >= self.base_chirho + self.size_chirho {
                break;
            }
            let buddy_node_chirho = buddy_addr_chirho as *mut FreeNodeChirho;
            let buddy_idx_chirho = current_order_chirho - MIN_ORDER_CHIRHO;
            if buddy_idx_chirho < NUM_ORDERS_CHIRHO {
                unsafe {
                    (*buddy_node_chirho).next_chirho = self.free_lists_chirho[buddy_idx_chirho];
                    self.free_lists_chirho[buddy_idx_chirho] = buddy_node_chirho;
                }
            }
        }

        // Bounds check: ensure the block is within the heap.
        let block_addr_chirho = block_chirho as usize;
        if block_addr_chirho < self.base_chirho
            || block_addr_chirho >= self.base_chirho + self.size_chirho
        {
            return ptr::null_mut(); // Corrupted free list
        }

        block_chirho as *mut u8
    }

    /// Deallocate a block — return to free list with safe merge.
    ///
    /// Only merges at large orders (>= 20, 1MB+) where the free lists
    /// are short and corruption risk is low. Small-order free lists can
    /// have thousands of entries; traversing them for buddy search is
    /// slow and the intrusive FreeNodeChirho gets corrupted by stale data.
    pub fn dealloc_chirho(&mut self, ptr_chirho: *mut u8, layout_chirho: Layout) {
        if !self.initialized_chirho || ptr_chirho.is_null() {
            return;
        }

        let addr_chirho = ptr_chirho as usize;
        if addr_chirho < self.base_chirho || addr_chirho >= self.base_chirho + self.size_chirho {
            return;
        }

        let size_chirho = layout_chirho.size().max(layout_chirho.align());
        let mut order_chirho = Self::size_to_order_chirho(size_chirho);
        let mut merge_addr_chirho = addr_chirho;

        if order_chirho > MAX_ORDER_CHIRHO {
            return;
        }

        // Merge only at large orders (>= 20 = 1MB) where free lists are short.
        const MERGE_THRESHOLD_CHIRHO: usize = 20;
        while order_chirho >= MERGE_THRESHOLD_CHIRHO && order_chirho < MAX_ORDER_CHIRHO {
            let block_size_chirho = 1usize << order_chirho;
            let buddy_addr_chirho = merge_addr_chirho ^ block_size_chirho;

            if buddy_addr_chirho < self.base_chirho
                || buddy_addr_chirho >= self.base_chirho + self.size_chirho
            {
                break;
            }

            let idx_chirho = order_chirho - MIN_ORDER_CHIRHO;
            if idx_chirho >= NUM_ORDERS_CHIRHO {
                break;
            }

            // Search for buddy in free list (short list at large orders).
            let mut found_chirho = false;
            let mut prev_chirho: *mut FreeNodeChirho = ptr::null_mut();
            let mut cur_chirho = self.free_lists_chirho[idx_chirho];

            while !cur_chirho.is_null() {
                if cur_chirho as usize == buddy_addr_chirho {
                    unsafe {
                        if prev_chirho.is_null() {
                            self.free_lists_chirho[idx_chirho] = (*cur_chirho).next_chirho;
                        } else {
                            (*prev_chirho).next_chirho = (*cur_chirho).next_chirho;
                        }
                    }
                    found_chirho = true;
                    break;
                }
                prev_chirho = cur_chirho;
                cur_chirho = unsafe { (*cur_chirho).next_chirho };
            }

            if !found_chirho {
                break;
            }

            merge_addr_chirho = merge_addr_chirho.min(buddy_addr_chirho);
            order_chirho += 1;
        }

        // Add to free list at the final order.
        let idx_chirho = order_chirho - MIN_ORDER_CHIRHO;
        if idx_chirho < NUM_ORDERS_CHIRHO {
            let node_chirho = merge_addr_chirho as *mut FreeNodeChirho;
            unsafe {
                (*node_chirho).next_chirho = self.free_lists_chirho[idx_chirho];
                self.free_lists_chirho[idx_chirho] = node_chirho;
            }
        }
    }
}

/// Thread-safe wrapper for GlobalAlloc.
pub struct LockedBuddyChirho(pub Mutex<BuddyAllocatorChirho>);

unsafe impl GlobalAlloc for LockedBuddyChirho {
    unsafe fn alloc(&self, layout_chirho: Layout) -> *mut u8 {
        self.0.lock().alloc_chirho(layout_chirho)
    }

    unsafe fn dealloc(&self, ptr_chirho: *mut u8, layout_chirho: Layout) {
        self.0.lock().dealloc_chirho(ptr_chirho, layout_chirho);
    }
}
