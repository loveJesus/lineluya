// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM memory management — uses WASM linear memory instead of page tables.
//!
//! In WASM, memory is a contiguous linear array that grows via `memory.grow`.
//! There are NO page tables, NO MMU, NO TLB flushes. WASM's bounds checking
//! provides memory safety at near-zero cost (validated at compile time).
//!
//! This replaces:
//! - Physical frame allocator → bump allocator in linear memory
//! - Page tables → not needed (WASM bounds checking)
//! - mmap → allocate from linear memory pool
//! - mprotect → not applicable (WASM has no page-level permissions)

/// Current WASM memory size in pages (each page = 64 KiB).
pub fn current_pages_chirho() -> u32 {
    core::arch::wasm32::memory_size(0) as u32
}

/// Grow WASM memory by `delta_chirho` pages. Returns previous size or -1 on failure.
pub fn grow_memory_chirho(delta_chirho: u32) -> i32 {
    core::arch::wasm32::memory_grow(0, delta_chirho as usize) as i32
}

/// Total available memory in bytes.
pub fn total_bytes_chirho() -> usize {
    current_pages_chirho() as usize * 65536
}

/// Simple bump allocator for WASM linear memory.
pub struct WasmBumpAllocatorChirho {
    next_chirho: usize,
    end_chirho: usize,
}

impl WasmBumpAllocatorChirho {
    pub const fn new_chirho() -> Self {
        Self {
            next_chirho: 0,
            end_chirho: 0,
        }
    }

    /// Initialize with a starting offset and size.
    pub fn init_chirho(&mut self, start_chirho: usize, size_chirho: usize) {
        self.next_chirho = start_chirho;
        self.end_chirho = start_chirho + size_chirho;
    }

    /// Allocate `size_chirho` bytes aligned to `align_chirho`.
    pub fn alloc_chirho(&mut self, size_chirho: usize, align_chirho: usize) -> Option<usize> {
        let aligned_chirho = (self.next_chirho + align_chirho - 1) & !(align_chirho - 1);
        if aligned_chirho + size_chirho > self.end_chirho {
            // Try to grow memory
            let pages_needed_chirho = ((aligned_chirho + size_chirho - self.end_chirho) / 65536) + 1;
            let result_chirho = grow_memory_chirho(pages_needed_chirho as u32);
            if result_chirho == -1 {
                return None;
            }
            self.end_chirho = current_pages_chirho() as usize * 65536;
        }
        self.next_chirho = aligned_chirho + size_chirho;
        Some(aligned_chirho)
    }
}
