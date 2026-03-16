// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! User-space memory management module for the Lineluya kernel.
//!
//! Provides:
//! - [`VmaChirho`] — Virtual Memory Area descriptor, tracking a contiguous
//!   range of user-space virtual addresses with protection and mapping flags.
//! - [`MmChirho`] — Per-process memory descriptor (analogous to Linux's
//!   `struct mm_struct`), managing a list of VMAs, the program break, and
//!   anonymous mmap allocation.
//! - `mmap_chirho`, `munmap_chirho`, `mprotect_chirho` — the core memory
//!   management operations invoked by the corresponding syscalls.
//! - Linux-compatible protection and mapping flag constants.
//!
//! ## Temporary limitations
//!
//! This implementation uses the kernel's own page tables (via a global
//! mapper/allocator) rather than per-process page tables.  Real per-process
//! address spaces will be introduced in a future wave.

#![allow(dead_code)]

use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Protection flag constants (matching Linux <asm-generic/mman-common.h>)
// ============================================================================

/// No access permitted.
pub const PROT_NONE_CHIRHO: u32 = 0;
/// Pages may be read.
pub const PROT_READ_CHIRHO: u32 = 1;
/// Pages may be written.
pub const PROT_WRITE_CHIRHO: u32 = 2;
/// Pages may be executed.
pub const PROT_EXEC_CHIRHO: u32 = 4;

// ============================================================================
// Mapping flag constants (matching Linux <asm-generic/mman-common.h>)
// ============================================================================

/// Share this mapping (changes visible to other processes mapping the same
/// object).
pub const MAP_SHARED_CHIRHO: u32 = 0x01;
/// Create a private copy-on-write mapping.
pub const MAP_PRIVATE_CHIRHO: u32 = 0x02;
/// Place the mapping at exactly the specified address.
pub const MAP_FIXED_CHIRHO: u32 = 0x10;
/// The mapping is not backed by any file; its contents are initialised to
/// zero.
pub const MAP_ANONYMOUS_CHIRHO: u32 = 0x20;

// ============================================================================
// Internal constants
// ============================================================================

/// Starting address for anonymous mmap allocations.  We grow *downward* from
/// this point, mimicking the typical Linux user-space layout where the mmap
/// region sits below the stack and grows toward lower addresses.
const MMAP_BASE_ADDR_CHIRHO: u64 = 0x7F00_0000_0000;

/// Page size (4 KiB).
const PAGE_SIZE_CHIRHO: u64 = 4096;

// ============================================================================
// VmaChirho — Virtual Memory Area
// ============================================================================

/// Describes a single contiguous region of user-space virtual memory.
///
/// Analogous to Linux's `struct vm_area_struct`.  Each VMA tracks the address
/// range, protection bits, and mapping flags for one logically distinct
/// mapping (e.g., one `mmap` call, the heap, the stack, a loaded ELF
/// segment).
#[derive(Debug, Clone)]
pub struct VmaChirho {
    /// Start virtual address (inclusive, page-aligned).
    pub start_chirho: u64,
    /// End virtual address (exclusive, page-aligned).
    pub end_chirho: u64,
    /// Protection flags (`PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | …`).
    pub prot_chirho: u32,
    /// Mapping flags (`MAP_PRIVATE_CHIRHO | MAP_ANONYMOUS_CHIRHO | …`).
    pub flags_chirho: u32,
}

impl VmaChirho {
    /// Return the size of this VMA in bytes.
    pub fn size_chirho(&self) -> u64 {
        self.end_chirho - self.start_chirho
    }

    /// Return `true` if this VMA overlaps with `[start, start + len)`.
    pub fn overlaps_chirho(&self, start_chirho: u64, len_chirho: u64) -> bool {
        let end_chirho = start_chirho + len_chirho;
        self.start_chirho < end_chirho && start_chirho < self.end_chirho
    }

    /// Return `true` if this VMA fully contains `[start, start + len)`.
    pub fn contains_range_chirho(&self, start_chirho: u64, len_chirho: u64) -> bool {
        start_chirho >= self.start_chirho && (start_chirho + len_chirho) <= self.end_chirho
    }
}

// ============================================================================
// MmChirho — Per-process memory descriptor
// ============================================================================

/// Per-process memory management descriptor.
///
/// Analogous to Linux's `struct mm_struct`.  Tracks all virtual memory areas
/// for a process, the program break (`brk`), and the next address to hand
/// out for anonymous `mmap` allocations.
pub struct MmChirho {
    /// List of VMAs, kept sorted by `start_chirho` for efficient lookup.
    pub vmas_chirho: Vec<VmaChirho>,
    /// Current program break (top of the heap).
    pub brk_chirho: u64,
    /// Initial program break (set at process creation / exec).
    pub brk_start_chirho: u64,
    /// Next address for anonymous mmap.  Grows downward from
    /// [`MMAP_BASE_ADDR_CHIRHO`].
    pub next_mmap_addr_chirho: u64,
}

impl MmChirho {
    /// Create a new, empty memory descriptor.
    pub fn new_chirho() -> Self {
        Self {
            vmas_chirho: Vec::new(),
            brk_chirho: 0,
            brk_start_chirho: 0,
            next_mmap_addr_chirho: MMAP_BASE_ADDR_CHIRHO,
        }
    }

    // --------------------------------------------------------------------
    // mmap
    // --------------------------------------------------------------------

    /// Map anonymous private memory into the process's address space.
    ///
    /// Implements the core of `mmap(2)` for `MAP_ANONYMOUS | MAP_PRIVATE`
    /// mappings.  Other combinations (file-backed, shared) are not yet
    /// supported.
    ///
    /// # Arguments
    ///
    /// * `addr_chirho`   — Hint address (or required address if `MAP_FIXED`).
    /// * `len_chirho`    — Length in bytes (will be rounded up to page size).
    /// * `prot_chirho`   — Protection flags (`PROT_*`).
    /// * `flags_chirho`  — Mapping flags (`MAP_*`).
    /// * `fd_chirho`     — File descriptor (-1 for anonymous).
    /// * `offset_chirho` — File offset (ignored for anonymous).
    ///
    /// # Returns
    ///
    /// The start address of the new mapping on success, or a negative errno
    /// on failure.
    pub fn mmap_chirho(
        &mut self,
        addr_chirho: u64,
        len_chirho: u64,
        prot_chirho: u32,
        flags_chirho: u32,
        fd_chirho: i32,
        _offset_chirho: u64,
    ) -> Result<u64, i64> {
        use crate::syscall_chirho::{EINVAL_CHIRHO, ENOSYS_CHIRHO};

        // Validate length.
        if len_chirho == 0 {
            return Err(-EINVAL_CHIRHO);
        }

        let is_anonymous_chirho = (flags_chirho & MAP_ANONYMOUS_CHIRHO) != 0;
        let _is_private_chirho = (flags_chirho & MAP_PRIVATE_CHIRHO) != 0;

        // For file-backed mappings (fd >= 0), we treat them as anonymous
        // private mappings and copy the file data in afterward.  This is
        // a simplification — real Linux would fault pages in lazily.
        // Special case: /dev/fb0 mmap maps the physical framebuffer directly.
        let has_file_chirho = fd_chirho >= 0 && !is_anonymous_chirho;
        let is_fb_mmap_chirho = has_file_chirho
            && crate::fb_device_chirho::is_fb_fd_chirho(fd_chirho as u64);

        // Round length up to page boundary.
        let aligned_len_chirho = align_up_page_chirho(len_chirho);

        // Determine the mapping address.
        let is_fixed_chirho = (flags_chirho & MAP_FIXED_CHIRHO) != 0;
        let map_addr_chirho = if is_fixed_chirho {
            // MAP_FIXED: must use the exact address.
            if addr_chirho % PAGE_SIZE_CHIRHO != 0 {
                return Err(-EINVAL_CHIRHO);
            }
            // Remove any existing overlapping mappings (Linux MAP_FIXED
            // semantics).
            self.remove_overlapping_vmas_chirho(addr_chirho, aligned_len_chirho);
            addr_chirho
        } else if addr_chirho != 0 && addr_chirho % PAGE_SIZE_CHIRHO == 0 {
            // Hint address provided — use it if the region is free.
            if self.is_region_free_chirho(addr_chirho, aligned_len_chirho) {
                addr_chirho
            } else {
                self.allocate_mmap_addr_chirho(aligned_len_chirho)?
            }
        } else {
            // No hint — pick an address automatically.
            self.allocate_mmap_addr_chirho(aligned_len_chirho)?
        };

        // For /dev/fb0 mmap, map the physical framebuffer pages directly
        // instead of allocating new anonymous pages.
        if is_fb_mmap_chirho {
            let fb_phys_chirho = crate::fb_device_chirho::fb_phys_addr_chirho();
            let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
            // The framebuffer is already accessible via phys_offset + fb_phys.
            // Return that address so userspace can write pixels directly.
            let fb_virt_chirho = fb_phys_chirho + phys_offset_chirho;
            // Record the VMA but don't allocate new pages.
            let vma_chirho = VmaChirho {
                start_chirho: fb_virt_chirho,
                end_chirho: fb_virt_chirho + aligned_len_chirho,
                prot_chirho,
                flags_chirho,
            };
            self.insert_vma_chirho(vma_chirho);
            return Ok(fb_virt_chirho);
        }

        // Map the physical frames into the kernel page tables.
        // For file-backed mappings, map as RW initially so we can copy
        // file data in, then change to the requested protection after.
        let initial_prot_chirho = if has_file_chirho {
            PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | PROT_EXEC_CHIRHO
        } else {
            prot_chirho
        };
        map_anonymous_pages_chirho(map_addr_chirho, aligned_len_chirho, initial_prot_chirho)?;

        // For file-backed mappings, read the file data ONCE and copy it
        // into the mapped region. The old approach (read in 4K loop via
        // sys_read_real) was O(n²) because each read re-loaded the entire
        // file from ext4. Now we read the file's data directly from the
        // VFS and memcpy it in one shot.
        if has_file_chirho {
            let file_data_chirho = crate::fs_chirho::read_file_data_at_offset_chirho(
                fd_chirho as u64,
                _offset_chirho,
                aligned_len_chirho,
            );
            if let Some(data_chirho) = file_data_chirho {
                let copy_len_chirho = core::cmp::min(data_chirho.len(), aligned_len_chirho as usize);
                if copy_len_chirho > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data_chirho.as_ptr(),
                            map_addr_chirho as *mut u8,
                            copy_len_chirho,
                        );
                    }
                }
                // Now change protection to what was requested (may be read-only)
                if prot_chirho != initial_prot_chirho {
                    update_page_protection_chirho(map_addr_chirho, aligned_len_chirho, prot_chirho);
                }
            }
        }

        // Record the VMA.
        let vma_chirho = VmaChirho {
            start_chirho: map_addr_chirho,
            end_chirho: map_addr_chirho + aligned_len_chirho,
            prot_chirho,
            flags_chirho,
        };
        self.insert_vma_chirho(vma_chirho);

        // Debug log removed — serial prints during syscalls can deadlock
        // with the page fault handler's serial output.

        Ok(map_addr_chirho)
    }

    // --------------------------------------------------------------------
    // munmap
    // --------------------------------------------------------------------

    /// Unmap pages from the process's address space.
    ///
    /// Implements the core of `munmap(2)`.  The address must be page-aligned,
    /// and the length is rounded up to a page boundary.
    ///
    /// # Errors
    ///
    /// Returns `-EINVAL` if `addr_chirho` is not page-aligned.
    pub fn munmap_chirho(
        &mut self,
        addr_chirho: u64,
        len_chirho: u64,
    ) -> Result<(), i64> {
        use crate::syscall_chirho::EINVAL_CHIRHO;

        if addr_chirho % PAGE_SIZE_CHIRHO != 0 {
            return Err(-EINVAL_CHIRHO);
        }
        if len_chirho == 0 {
            return Err(-EINVAL_CHIRHO);
        }

        let aligned_len_chirho = align_up_page_chirho(len_chirho);

        // Unmap physical pages from the page tables.
        unmap_pages_chirho(addr_chirho, aligned_len_chirho);

        // Remove or split overlapping VMAs.
        self.remove_overlapping_vmas_chirho(addr_chirho, aligned_len_chirho);


        Ok(())
    }

    // --------------------------------------------------------------------
    // mprotect
    // --------------------------------------------------------------------

    /// Change the protection on a region of the process's address space.
    ///
    /// Implements the core of `mprotect(2)`.  The address must be
    /// page-aligned and the region must be currently mapped.
    ///
    /// # Errors
    ///
    /// Returns `-EINVAL` if `addr_chirho` is not page-aligned or `len` is 0.
    /// Returns `-ENOMEM` if the region is not fully mapped.
    pub fn mprotect_chirho(
        &mut self,
        addr_chirho: u64,
        len_chirho: u64,
        prot_chirho: u32,
    ) -> Result<(), i64> {
        use crate::syscall_chirho::{EINVAL_CHIRHO, ENOMEM_CHIRHO};

        if addr_chirho % PAGE_SIZE_CHIRHO != 0 {
            return Err(-EINVAL_CHIRHO);
        }
        if len_chirho == 0 {
            return Err(-EINVAL_CHIRHO);
        }

        let aligned_len_chirho = align_up_page_chirho(len_chirho);
        let end_chirho = addr_chirho + aligned_len_chirho;

        // Verify the entire region is covered by existing VMAs.
        if !self.is_region_mapped_chirho(addr_chirho, aligned_len_chirho) {
            return Err(-ENOMEM_CHIRHO);
        }

        // Update protection in page tables.
        update_page_protection_chirho(addr_chirho, aligned_len_chirho, prot_chirho);

        // Update VMA protection flags.  This may require splitting VMAs if
        // the mprotect range partially overlaps a VMA.
        self.update_vma_prot_chirho(addr_chirho, end_chirho, prot_chirho);


        Ok(())
    }

    // --------------------------------------------------------------------
    // Internal VMA management
    // --------------------------------------------------------------------

    /// Insert a VMA, keeping the list sorted by start address.
    fn insert_vma_chirho(&mut self, vma_chirho: VmaChirho) {
        let pos_chirho = self
            .vmas_chirho
            .iter()
            .position(|existing_chirho| existing_chirho.start_chirho > vma_chirho.start_chirho)
            .unwrap_or(self.vmas_chirho.len());
        self.vmas_chirho.insert(pos_chirho, vma_chirho);
    }

    /// Remove or split VMAs that overlap with `[addr, addr + len)`.
    fn remove_overlapping_vmas_chirho(&mut self, addr_chirho: u64, len_chirho: u64) {
        let end_chirho = addr_chirho + len_chirho;
        let mut new_vmas_chirho: Vec<VmaChirho> = Vec::new();

        for vma_chirho in self.vmas_chirho.drain(..) {
            if vma_chirho.end_chirho <= addr_chirho || vma_chirho.start_chirho >= end_chirho {
                // No overlap — keep as-is.
                new_vmas_chirho.push(vma_chirho);
            } else {
                // Overlap detected.  Possibly split into before/after pieces.
                if vma_chirho.start_chirho < addr_chirho {
                    // Keep the portion before the unmapped region.
                    new_vmas_chirho.push(VmaChirho {
                        start_chirho: vma_chirho.start_chirho,
                        end_chirho: addr_chirho,
                        prot_chirho: vma_chirho.prot_chirho,
                        flags_chirho: vma_chirho.flags_chirho,
                    });
                }
                if vma_chirho.end_chirho > end_chirho {
                    // Keep the portion after the unmapped region.
                    new_vmas_chirho.push(VmaChirho {
                        start_chirho: end_chirho,
                        end_chirho: vma_chirho.end_chirho,
                        prot_chirho: vma_chirho.prot_chirho,
                        flags_chirho: vma_chirho.flags_chirho,
                    });
                }
                // The overlapping portion is discarded.
            }
        }

        self.vmas_chirho = new_vmas_chirho;
    }

    /// Check whether the region `[addr, addr + len)` is free (no overlapping
    /// VMAs).
    fn is_region_free_chirho(&self, addr_chirho: u64, len_chirho: u64) -> bool {
        !self
            .vmas_chirho
            .iter()
            .any(|vma_chirho| vma_chirho.overlaps_chirho(addr_chirho, len_chirho))
    }

    /// Check whether the region `[addr, addr + len)` is fully covered by
    /// existing VMAs.
    fn is_region_mapped_chirho(&self, addr_chirho: u64, len_chirho: u64) -> bool {
        // Walk through the required range, checking that every page is
        // covered by at least one VMA.
        let end_chirho = addr_chirho + len_chirho;
        let mut cursor_chirho = addr_chirho;

        while cursor_chirho < end_chirho {
            let covered_chirho = self.vmas_chirho.iter().find(|vma_chirho| {
                vma_chirho.start_chirho <= cursor_chirho && cursor_chirho < vma_chirho.end_chirho
            });
            match covered_chirho {
                Some(vma_chirho) => {
                    // Advance cursor to the end of this VMA.
                    cursor_chirho = vma_chirho.end_chirho;
                }
                None => return false,
            }
        }

        true
    }

    /// Allocate a virtual address range for an anonymous mmap mapping.
    ///
    /// Grows downward from [`MMAP_BASE_ADDR_CHIRHO`].
    fn allocate_mmap_addr_chirho(
        &mut self,
        len_chirho: u64,
    ) -> Result<u64, i64> {
        use crate::syscall_chirho::ENOMEM_CHIRHO;

        if self.next_mmap_addr_chirho < len_chirho {
            return Err(-ENOMEM_CHIRHO);
        }

        let addr_chirho = self.next_mmap_addr_chirho - len_chirho;
        // Page-align downward.
        let addr_chirho = addr_chirho & !(PAGE_SIZE_CHIRHO - 1);

        if addr_chirho == 0 {
            return Err(-ENOMEM_CHIRHO);
        }

        self.next_mmap_addr_chirho = addr_chirho;
        Ok(addr_chirho)
    }

    /// Update VMA protection flags for the range `[start, end)`.
    ///
    /// VMAs that are fully contained in the range get their prot updated.
    /// VMAs that partially overlap are split so that only the overlapping
    /// portion is updated.
    fn update_vma_prot_chirho(
        &mut self,
        start_chirho: u64,
        end_chirho: u64,
        prot_chirho: u32,
    ) {
        let mut new_vmas_chirho: Vec<VmaChirho> = Vec::new();

        for vma_chirho in self.vmas_chirho.drain(..) {
            if vma_chirho.end_chirho <= start_chirho || vma_chirho.start_chirho >= end_chirho {
                // No overlap — keep unchanged.
                new_vmas_chirho.push(vma_chirho);
            } else {
                // There is overlap.  Potentially split into up to 3 pieces:
                // [before | updated | after].

                // Before portion (unchanged protection).
                if vma_chirho.start_chirho < start_chirho {
                    new_vmas_chirho.push(VmaChirho {
                        start_chirho: vma_chirho.start_chirho,
                        end_chirho: start_chirho,
                        prot_chirho: vma_chirho.prot_chirho,
                        flags_chirho: vma_chirho.flags_chirho,
                    });
                }

                // Middle portion (updated protection).
                let mid_start_chirho = if vma_chirho.start_chirho > start_chirho {
                    vma_chirho.start_chirho
                } else {
                    start_chirho
                };
                let mid_end_chirho = if vma_chirho.end_chirho < end_chirho {
                    vma_chirho.end_chirho
                } else {
                    end_chirho
                };
                new_vmas_chirho.push(VmaChirho {
                    start_chirho: mid_start_chirho,
                    end_chirho: mid_end_chirho,
                    prot_chirho,
                    flags_chirho: vma_chirho.flags_chirho,
                });

                // After portion (unchanged protection).
                if vma_chirho.end_chirho > end_chirho {
                    new_vmas_chirho.push(VmaChirho {
                        start_chirho: end_chirho,
                        end_chirho: vma_chirho.end_chirho,
                        prot_chirho: vma_chirho.prot_chirho,
                        flags_chirho: vma_chirho.flags_chirho,
                    });
                }
            }
        }

        self.vmas_chirho = new_vmas_chirho;
    }
}

// ============================================================================
// Global MmChirho instance (temporary — one per process in the future)
// ============================================================================

/// Global memory descriptor.
///
/// Because we do not yet have per-process page tables, we maintain a single
/// global `MmChirho` that is shared by all "processes".  This will be replaced
/// by a per-task `MmChirho` stored in [`TaskChirho`] once per-process address
/// spaces are implemented.
pub static GLOBAL_MM_CHIRHO: Mutex<Option<MmChirho>> = Mutex::new(None);

/// Get or initialise the global memory descriptor.
///
/// Lazily creates an [`MmChirho`] on first access.  Returns a reference
/// suitable for use in the `mmap`/`munmap`/`mprotect` syscall handlers.
pub fn get_or_init_mm_chirho() -> &'static Mutex<Option<MmChirho>> {
    {
        let mut mm_lock_chirho = GLOBAL_MM_CHIRHO.lock();
        if mm_lock_chirho.is_none() {
            *mm_lock_chirho = Some(MmChirho::new_chirho());
        }
    }
    &GLOBAL_MM_CHIRHO
}

// ============================================================================
// Page-table manipulation helpers (temporary — uses kernel mapper)
// ============================================================================

/// Map anonymous (zero-filled) pages at the given virtual address range.
///
/// Uses the kernel page tables directly.  In the future, each process will
/// have its own set of page tables.
fn map_anonymous_pages_chirho(
    addr_chirho: u64,
    len_chirho: u64,
    prot_chirho: u32,
) -> Result<(), i64> {
    use crate::syscall_chirho::ENOMEM_CHIRHO;
    use x86_64::structures::paging::{
        FrameAllocator, Mapper, Page, Size4KiB,
    };
    use x86_64::VirtAddr;

    let flags_chirho = prot_to_page_flags_chirho(prot_chirho);

    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    for i_chirho in 0..num_pages_chirho {
        let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
        let page_chirho: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(page_addr_chirho));

        // Allocate a physical frame.
        let frame_chirho = {
            let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
            match alloc_lock_chirho.as_mut() {
                Some(alloc_chirho) => match alloc_chirho.allocate_frame() {
                    Some(f_chirho) => f_chirho,
                    None => return Err(-ENOMEM_CHIRHO),
                },
                None => {
                    return Err(-ENOMEM_CHIRHO);
                }
            }
        };

        // Map the page.
        let result_chirho = {
            let mut mapper_lock_chirho = GLOBAL_MAPPER_CHIRHO.lock();
            match mapper_lock_chirho.as_mut() {
                Some(mapper_chirho) => {
                    let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
                    match alloc_lock_chirho.as_mut() {
                        Some(alloc_chirho) => {
                            // SAFETY: We are mapping a freshly allocated frame
                            // to a user-space address that is not currently
                            // mapped.  The frame allocator guarantees the frame
                            // is unused.
                            unsafe {
                                mapper_chirho.map_to(
                                    page_chirho,
                                    frame_chirho,
                                    flags_chirho,
                                    alloc_chirho,
                                )
                            }
                        }
                        None => return Err(-ENOMEM_CHIRHO),
                    }
                }
                None => return Err(-ENOMEM_CHIRHO),
            }
        };

        match result_chirho {
            Ok(flush_chirho) => flush_chirho.flush(),
            Err(_e_chirho) => {
                // Page may already be mapped (e.g., overlapping ELF segments
                // sharing the same page).  This is normal with MAP_FIXED.
                // Try to update flags instead of failing.
                let page_chirho: Page<Size4KiB> =
                    Page::containing_address(VirtAddr::new(page_addr_chirho));
                if let Some(ref mut mapper_chirho) = *GLOBAL_MAPPER_CHIRHO.lock() {
                    let _ = unsafe {
                        mapper_chirho.update_flags(page_chirho, flags_chirho)
                    }.map(|flush_chirho| flush_chirho.flush());
                }
                // Skip zero-fill for already-mapped pages — data will be
                // written by the caller (exec_chirho copies segment data).
                continue;
            }
        }

        // Zero-fill the page.
        // SAFETY: The page was just mapped and backed by a fresh physical
        // frame; writing zeroes to it is safe.
        unsafe {
            core::ptr::write_bytes(page_addr_chirho as *mut u8, 0, PAGE_SIZE_CHIRHO as usize);
        }
    }

    Ok(())
}

/// Unmap pages from the page tables.
///
/// Failures are silently ignored — the VMA is removed regardless (matching
/// Linux's `munmap` best-effort approach for the page tables).
fn unmap_pages_chirho(addr_chirho: u64, len_chirho: u64) {
    use x86_64::structures::paging::{Mapper, Page, Size4KiB};
    use x86_64::VirtAddr;

    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    let mut mapper_lock_chirho = GLOBAL_MAPPER_CHIRHO.lock();
    if let Some(mapper_chirho) = mapper_lock_chirho.as_mut() {
        for i_chirho in 0..num_pages_chirho {
            let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(VirtAddr::new(page_addr_chirho));

            // Attempt to unmap.  If the page was never mapped (e.g., lazy
            // allocation) we silently ignore the error.
            match mapper_chirho.unmap(page_chirho) {
                Ok((_frame_chirho, flush_chirho)) => {
                    flush_chirho.flush();
                    // TODO: Return the frame to the allocator once we have a
                    // frame deallocator.
                }
                Err(_e_chirho) => {
                    // Page was not mapped — nothing to do.
                }
            }
        }
    }
}

/// Update page-table protection flags for a range of pages.
///
/// For each page in the range, if it is currently mapped, we update its flags
/// to match `prot_chirho`.
fn update_page_protection_chirho(addr_chirho: u64, len_chirho: u64, prot_chirho: u32) {
    use x86_64::structures::paging::{Mapper, Page, Size4KiB};
    use x86_64::VirtAddr;

    let flags_chirho = prot_to_page_flags_chirho(prot_chirho);
    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    let mut mapper_lock_chirho = GLOBAL_MAPPER_CHIRHO.lock();
    if let Some(mapper_chirho) = mapper_lock_chirho.as_mut() {
        for i_chirho in 0..num_pages_chirho {
            let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(VirtAddr::new(page_addr_chirho));

            // SAFETY: We are updating flags on a page that is already mapped
            // in the current address space.  The caller has verified via the
            // VMA list that this region is valid.
            unsafe {
                let _ = mapper_chirho.update_flags(page_chirho, flags_chirho);
            }
            // Flush the TLB entry for this page since its permissions changed.
            x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
        }
    }
}

/// Convert Linux `PROT_*` flags to x86_64 [`PageTableFlags`].
fn prot_to_page_flags_chirho(prot_chirho: u32) -> x86_64::structures::paging::PageTableFlags {
    use x86_64::structures::paging::PageTableFlags;

    let mut flags_chirho = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    if prot_chirho & PROT_WRITE_CHIRHO != 0 {
        flags_chirho |= PageTableFlags::WRITABLE;
    }

    if prot_chirho & PROT_EXEC_CHIRHO == 0 {
        flags_chirho |= PageTableFlags::NO_EXECUTE;
    }

    flags_chirho
}

// ============================================================================
// Global mapper + frame allocator storage (temporary)
// ============================================================================
//
// In the future, each process will have its own page tables.  For now, we
// store references to the kernel's mapper and frame allocator in global
// statics so that the mm module can map/unmap pages.

use x86_64::structures::paging::{OffsetPageTable, PhysFrame, Size4KiB, FrameAllocator};

/// Wrapper around `BootInfoFrameAllocatorChirho` that can be stored in a
/// static.  We need this because `BootInfoFrameAllocatorChirho` borrows
/// `MemoryRegions` with a `'static` lifetime, which is fine for a static.
///
/// This is a simple newtype that delegates `allocate_frame` calls.
pub struct GlobalFrameAllocatorChirho {
    next_frame_index_chirho: usize,
    memory_regions_chirho: &'static bootloader_api::info::MemoryRegions,
}

impl GlobalFrameAllocatorChirho {
    /// Create a new global frame allocator from the bootloader memory
    /// regions.
    pub fn new_chirho(
        memory_regions_chirho: &'static bootloader_api::info::MemoryRegions,
        start_index_chirho: usize,
    ) -> Self {
        Self {
            next_frame_index_chirho: start_index_chirho,
            memory_regions_chirho,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for GlobalFrameAllocatorChirho {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        use bootloader_api::info::MemoryRegionKind;
        use x86_64::PhysAddr;

        let frame_chirho = self
            .memory_regions_chirho
            .iter()
            .filter(|r_chirho| r_chirho.kind == MemoryRegionKind::Usable)
            .flat_map(|r_chirho| {
                let start_chirho = (r_chirho.start + PAGE_SIZE_CHIRHO - 1) & !(PAGE_SIZE_CHIRHO - 1);
                (start_chirho..r_chirho.end)
                    .step_by(PAGE_SIZE_CHIRHO as usize)
                    .map(|a_chirho| PhysFrame::containing_address(PhysAddr::new(a_chirho)))
            })
            .nth(self.next_frame_index_chirho);

        self.next_frame_index_chirho += 1;
        frame_chirho
    }
}

/// Global page table mapper.  Set during kernel init.
pub static GLOBAL_MAPPER_CHIRHO: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);

/// Global frame allocator.  Set during kernel init.
pub static GLOBAL_FRAME_ALLOCATOR_CHIRHO: Mutex<Option<GlobalFrameAllocatorChirho>> =
    Mutex::new(None);

/// Initialise the mm subsystem's global mapper and frame allocator.
///
/// This must be called after the kernel's paging and heap are set up, passing
/// ownership of the mapper and a fresh frame allocator to the mm module.
///
/// # Safety
///
/// The caller must guarantee that:
/// 1. The mapper references a valid, active page table.
/// 2. The frame allocator will only yield frames that are not in use.
/// 3. This function is called exactly once.
pub unsafe fn init_mm_chirho(
    mapper_chirho: OffsetPageTable<'static>,
    frame_allocator_chirho: GlobalFrameAllocatorChirho,
) {
    *GLOBAL_MAPPER_CHIRHO.lock() = Some(mapper_chirho);
    *GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock() = Some(frame_allocator_chirho);
    crate::serial_println_chirho!("[OK] MM subsystem initialized");
}

// ============================================================================
// Internal utilities
// ============================================================================

/// Align `val_chirho` upward to the nearest multiple of [`PAGE_SIZE_CHIRHO`].
const fn align_up_page_chirho(val_chirho: u64) -> u64 {
    (val_chirho + PAGE_SIZE_CHIRHO - 1) & !(PAGE_SIZE_CHIRHO - 1)
}
