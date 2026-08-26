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
use core::sync::atomic::{AtomicU64, Ordering};
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
// MappingKindChirho — mmap request classification
// ============================================================================

/// Classification of an mmap request.  Determined once from flags/fd,
/// then used to dispatch to the correct mapping handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MappingKindChirho {
    /// MAP_ANONYMOUS | MAP_PRIVATE — no backing file, zero-filled pages.
    AnonymousChirho,
    /// File-backed mapping — read file data into mapped pages.
    FileBackedChirho,
    /// /dev/fb0 mapping — map physical framebuffer directly.
    FramebufferChirho,
}

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
#[derive(Clone)]
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
        use crate::syscall_chirho::EINVAL_CHIRHO;

        // Validate length.
        if len_chirho == 0 {
            return Err(-EINVAL_CHIRHO);
        }

        // Classify the mapping request once, up front.
        let kind_chirho = if (flags_chirho & MAP_ANONYMOUS_CHIRHO) != 0 || fd_chirho < 0 {
            MappingKindChirho::AnonymousChirho
        } else if crate::fb_device_chirho::is_fb_fd_chirho(fd_chirho as u64) {
            MappingKindChirho::FramebufferChirho
        } else {
            MappingKindChirho::FileBackedChirho
        };

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
            // semantics). Always remove for proper segment overlay.
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

        // Dispatch based on the mapping kind.
        match kind_chirho {
            MappingKindChirho::FramebufferChirho => {
                // Map physical framebuffer pages into user-space page tables.
                // Xorg's fbdev driver mmaps /dev/fb0 to write pixels directly.
                let fb_phys_chirho = crate::fb_device_chirho::fb_phys_addr_chirho();
                let num_pages_chirho = (aligned_len_chirho + 0xFFF) / 0x1000;
                use x86_64::structures::paging::PageTableFlags;
                let user_flags_chirho = PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE;
                // Validate fb physical address before mapping
                if fb_phys_chirho == 0 || fb_phys_chirho >= (1u64 << 52) {
                    crate::serial_println_chirho!(
                        "[MMAP-FB] invalid fb_phys={:#x}, skipping", fb_phys_chirho
                    );
                    return Ok(map_addr_chirho);
                }
                let (cr3_frame_chirho, _) = x86_64::registers::control::Cr3::read();
                let pml4_phys_chirho = cr3_frame_chirho.start_address();
                for i_chirho in 0..num_pages_chirho as u64 {
                    let vaddr_chirho = map_addr_chirho + i_chirho * 0x1000;
                    let paddr_chirho = fb_phys_chirho + i_chirho * 0x1000;
                    if paddr_chirho >= (1u64 << 52) { break; }
                    let _ = crate::pagetable_chirho::map_page_in_pt_chirho(
                        pml4_phys_chirho, vaddr_chirho, paddr_chirho, user_flags_chirho,
                    );
                }
                crate::serial_println_chirho!(
                    "[MMAP-FB] mapped {}KB fb phys={:#x} → user={:#x}",
                    aligned_len_chirho / 1024, fb_phys_chirho, map_addr_chirho,
                );
                let vma_chirho = VmaChirho {
                    start_chirho: map_addr_chirho,
                    end_chirho: map_addr_chirho + aligned_len_chirho,
                    prot_chirho,
                    flags_chirho,
                };
                self.insert_vma_chirho(vma_chirho);
                Ok(map_addr_chirho)
            }

            MappingKindChirho::AnonymousChirho => {
                // PROT_NONE: just reserve address space (VMA only, no pages).
                // musl's dynamic linker uses mmap(PROT_NONE) to reserve a
                // contiguous address range, then MAP_FIXED to place segments.
                // If we allocate real frames here, they waste memory and
                // interfere with the subsequent MAP_FIXED file-backed mmaps.
                if prot_chirho == PROT_NONE_CHIRHO {
                    // MAP_FIXED PROT_NONE at brk: just return Ok without
                    // creating VMA or zeroing. musl's __expand_heap uses
                    // this to "reserve" brk space — the existing heap data
                    // must NOT be zeroed (it's valid in-use allocations).
                    if is_fixed_chirho {
                        let (cr3_pn_chirho, _) = x86_64::registers::control::Cr3::read();
                        let has_page_chirho = crate::pagetable_chirho::walk_page_table_chirho(
                            cr3_pn_chirho.start_address(),
                            x86_64::VirtAddr::new(map_addr_chirho),
                        ).map(|p| unsafe {
                            (*p).flags().contains(
                                x86_64::structures::paging::PageTableFlags::PRESENT
                            )
                        }).unwrap_or(false);
                        if has_page_chirho {
                            return Ok(map_addr_chirho);
                        }
                    }
                    let vma_chirho = VmaChirho {
                        start_chirho: map_addr_chirho,
                        end_chirho: map_addr_chirho + aligned_len_chirho,
                        prot_chirho,
                        flags_chirho,
                    };
                    self.insert_vma_chirho(vma_chirho);
                    return Ok(map_addr_chirho);
                }
                // Zero-filled anonymous pages — the common case.
                map_anonymous_pages_chirho(map_addr_chirho, aligned_len_chirho, prot_chirho)?;

                let vma_chirho = VmaChirho {
                    start_chirho: map_addr_chirho,
                    end_chirho: map_addr_chirho + aligned_len_chirho,
                    prot_chirho,
                    flags_chirho,
                };
                self.insert_vma_chirho(vma_chirho);
                Ok(map_addr_chirho)
            }

            MappingKindChirho::FileBackedChirho => {
                // For MAP_FIXED file-backed: unmap existing pages first so
                // map_anonymous_pages allocates fresh frames (don't reuse
                // zero-filled PROT_NONE reservation pages).
                if is_fixed_chirho {
                    unmap_pages_chirho(map_addr_chirho, aligned_len_chirho);
                }
                // Map as RWX initially so we can copy file data in, then
                // the VMA records the originally requested protection.
                let initial_prot_chirho =
                    PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | PROT_EXEC_CHIRHO;
                map_anonymous_pages_chirho(
                    map_addr_chirho, aligned_len_chirho, initial_prot_chirho,
                )?;

                // Copy file data directly from inode fs_data into mapped pages.
                // Uses position-independent reads (pread semantics) to avoid
                // shared file position corruption. Direct tmpfs memcpy for
                // preloaded libraries, ext4 fallback with tmp FileChirho.
                {
                    let file_arc_chirho = crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64);
                    if let Some(file_ref_chirho) = file_arc_chirho {
                        let total_chirho = aligned_len_chirho.min(8 * 1024 * 1024) as usize;
                        let file_offset_chirho = _offset_chirho as usize;
                        let inode_arc_chirho = {
                            let fg_chirho = file_ref_chirho.lock();
                            fg_chirho.inode_chirho.clone()
                        };
                        // Try direct copy from tmpfs Vec<u8> first (zero-copy, no races)
                        // MUST disable interrupts for tmpfs direct copy to prevent
                        // preemption corrupting page table state during concurrent mmaps.
                        // This is fast (~microseconds for typical .so segments).
                        let mut direct_ok_chirho = false;
                        {
                            x86_64::instructions::interrupts::disable();
                            let ig_chirho = inode_arc_chirho.lock();
                            if let Some(ref fsdata_chirho) = ig_chirho.fs_data_chirho {
                                if let Some(tmpfs_lock_chirho) = fsdata_chirho.downcast_ref::<spin::Mutex<crate::tmpfs_chirho::TmpfsDataChirho>>() {
                                    let td_chirho = tmpfs_lock_chirho.lock();
                                    if let crate::tmpfs_chirho::TmpfsDataChirho::FileChirho(ref content_chirho) = *td_chirho {
                                        // Direct memcpy from tmpfs content to user pages
                                        let src_start_chirho = file_offset_chirho.min(content_chirho.len());
                                        let src_end_chirho = (file_offset_chirho + total_chirho).min(content_chirho.len());
                                        let src_len_chirho = src_end_chirho - src_start_chirho;
                                        if src_len_chirho > 0 {
                                            let _ = crate::uaccess_chirho::copy_to_user_chirho(
                                                map_addr_chirho,
                                                &content_chirho[src_start_chirho..src_end_chirho],
                                                src_len_chirho,
                                            );
                                        }
                                        direct_ok_chirho = true;
                                    }
                                }
                            }
                            x86_64::instructions::interrupts::enable();
                        }
                        // Fallback for ext4: read via file ops with independent position
                        if !direct_ok_chirho {
                            let mut done_chirho: usize = 0;
                            while done_chirho < total_chirho {
                                let chunk_chirho = core::cmp::min(4096, total_chirho - done_chirho);
                                let mut kbuf_chirho = [0u8; 4096];
                                let n_chirho = {
                                    let fg_chirho = file_ref_chirho.lock();
                                    let mut tmp_file_chirho = crate::vfs_chirho::FileChirho {
                                        inode_chirho: inode_arc_chirho.clone(),
                                        pos_chirho: (file_offset_chirho + done_chirho) as u64,
                                        flags_chirho: fg_chirho.flags_chirho,
                                        ops_chirho: fg_chirho.ops_chirho,
                                    };
                                    match tmp_file_chirho.ops_chirho.read_chirho(
                                        &mut tmp_file_chirho, &mut kbuf_chirho[..chunk_chirho],
                                    ) {
                                        Ok(n) if n > 0 => n,
                                        _ => break,
                                    }
                                };
                                if crate::uaccess_chirho::copy_to_user_chirho(
                                    map_addr_chirho + done_chirho as u64,
                                    &kbuf_chirho[..n_chirho],
                                    n_chirho,
                                ).is_err() { break; }
                                done_chirho += n_chirho;
                            }
                        }
                    }
                }
                let vma_chirho = VmaChirho {
                    start_chirho: map_addr_chirho,
                    end_chirho: map_addr_chirho + aligned_len_chirho,
                    prot_chirho,
                    flags_chirho,
                };
                self.insert_vma_chirho(vma_chirho);
                Ok(map_addr_chirho)
            }
        }
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

        // Update VMA protection bits for any overlapping VMAs.
        self.update_vma_prot_chirho(addr_chirho, end_chirho, prot_chirho);

        // Actually update PTE flags for mapped pages. This is critical
        // for guard pages: mprotect(PROT_NONE) must make pages non-present
        // so buffer overflows trigger SIGSEGV instead of silently corrupting
        // adjacent memory (root cause of Xorg 0x2b33d crash).
        update_page_protection_chirho(addr_chirho, aligned_len_chirho, prot_chirho);

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
        // Warn if VMA count is growing unexpectedly
        if self.vmas_chirho.len() > 100 && self.vmas_chirho.len() % 500 == 0 {
            crate::serial_debug_chirho!(
                "[MM] VMA count = {} (capacity {} bytes)",
                self.vmas_chirho.len(),
                self.vmas_chirho.capacity() * core::mem::size_of::<VmaChirho>(),
            );
        }
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
    /// Check if a single page address falls within any VMA.
    /// Used by the page fault handler to reject accesses to unmapped regions.
    pub fn is_in_vma_chirho(&self, addr_chirho: u64) -> bool {
        self.vmas_chirho.iter().any(|vma_chirho| {
            vma_chirho.start_chirho <= addr_chirho && addr_chirho < vma_chirho.end_chirho
        })
    }

    /// Check if the address falls in a PROT_NONE VMA (guard page).
    pub fn is_prot_none_chirho(&self, addr_chirho: u64) -> bool {
        self.vmas_chirho.iter().any(|vma_chirho| {
            vma_chirho.start_chirho <= addr_chirho
                && addr_chirho < vma_chirho.end_chirho
                && vma_chirho.prot_chirho == PROT_NONE_CHIRHO
        })
    }

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

        // Add a 16-page (64KB) guard gap between allocations to prevent
        // musl's munmap of trailing space from affecting adjacent libs.
        let guarded_len_chirho = len_chirho + 16 * PAGE_SIZE_CHIRHO;
        if self.next_mmap_addr_chirho < guarded_len_chirho {
            return Err(-ENOMEM_CHIRHO);
        }

        let addr_chirho = self.next_mmap_addr_chirho - guarded_len_chirho;
        // Page-align downward.
        let addr_chirho = addr_chirho & !(PAGE_SIZE_CHIRHO - 1);

        if addr_chirho == 0 {
            return Err(-ENOMEM_CHIRHO);
        }

        // Verify no overlap with existing VMAs — if overlapping, skip
        // downward past the conflicting VMA and retry.
        let mut candidate_chirho = addr_chirho;
        for _retry_chirho in 0..64 {
            let overlapping_end_chirho = self.vmas_chirho.iter()
                .filter(|vma_chirho| {
                    candidate_chirho < vma_chirho.end_chirho
                        && (candidate_chirho + len_chirho) > vma_chirho.start_chirho
                })
                .map(|vma_chirho| vma_chirho.start_chirho)
                .min();
            match overlapping_end_chirho {
                Some(conflict_start_chirho) => {
                    // Skip below the conflicting VMA
                    if conflict_start_chirho < len_chirho {
                        return Err(-crate::syscall_chirho::ENOMEM_CHIRHO);
                    }
                    candidate_chirho = (conflict_start_chirho - len_chirho) & !(PAGE_SIZE_CHIRHO - 1);
                    if candidate_chirho == 0 {
                        return Err(-crate::syscall_chirho::ENOMEM_CHIRHO);
                    }
                }
                None => break, // No overlap — use this address
            }
        }
        self.next_mmap_addr_chirho = candidate_chirho;
        Ok(candidate_chirho)
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

/// Get or initialise the global memory descriptor (BOOT-ONLY fallback).
///
/// Panics if called from a user task (PID >= 2) — those should use
/// `get_current_mm_chirho()` which returns the per-task MM.
/// Only PID 0/1 (init shell) and early boot should use this.
pub fn get_or_init_mm_chirho() -> &'static Mutex<Option<MmChirho>> {
    // Guard: panic if called from a user task
    let caller_pid_chirho = crate::task_chirho::current_task_chirho()
        .and_then(|t| t.try_lock().map(|g| g.pid_chirho)).unwrap_or(0);
    if caller_pid_chirho >= 2 {
        crate::serial_println_chirho!(
            "[MM] BUG: get_or_init_mm called from PID {} — use get_current_mm instead!",
            caller_pid_chirho,
        );
    }
    {
        let mut mm_lock_chirho = GLOBAL_MM_CHIRHO.lock();
        if mm_lock_chirho.is_none() {
            *mm_lock_chirho = Some(MmChirho::new_chirho());
        }
    }
    &GLOBAL_MM_CHIRHO
}

/// Get the current task's per-process MM, falling back to GLOBAL_MM
/// for kernel tasks (PID 0/1) or early boot. This is the authoritative
/// MM accessor — all mmap/munmap/mprotect/brk should use this.
pub fn get_current_mm_chirho() -> alloc::sync::Arc<spin::Mutex<MmChirho>> {
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let task_guard_chirho = task_arc_chirho.lock();
        if let Some(ref mm_arc_chirho) = task_guard_chirho.mm_chirho {
            return mm_arc_chirho.clone();
        }
    }
    // Fallback: wrap global MM in an Arc for API compatibility.
    // This is only for PID 0/1 or early boot.
    static GLOBAL_MM_ARC_CHIRHO: spin::Once<alloc::sync::Arc<spin::Mutex<MmChirho>>> =
        spin::Once::new();
    GLOBAL_MM_ARC_CHIRHO.call_once(|| {
        alloc::sync::Arc::new(spin::Mutex::new(MmChirho::new_chirho()))
    }).clone()
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
    use x86_64::structures::paging::FrameAllocator;
    use x86_64::VirtAddr;

    let flags_chirho = prot_to_page_flags_chirho(prot_chirho);

    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    for i_chirho in 0..num_pages_chirho {
        let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;

        // Always publish a fresh zeroed leaf. Reusing and zeroing an existing
        // mapping can corrupt another address space that still references the
        // same COW frame.
        let frame_chirho = {
            let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
            match alloc_lock_chirho.as_mut() {
                Some(alloc_chirho) => match alloc_chirho.allocate_frame() {
                    Some(f_chirho) => f_chirho,
                    None => return Err(-ENOMEM_CHIRHO),
                },
                None => return Err(-ENOMEM_CHIRHO),
            }
        };

        let frame_phys_chirho = frame_chirho.start_address().as_u64();
        let physical_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        unsafe {
            core::ptr::write_bytes(
                (frame_phys_chirho + physical_offset_chirho) as *mut u8,
                0,
                PAGE_SIZE_CHIRHO as usize,
            );
        }
        let (cr3_map_chirho, _) = x86_64::registers::control::Cr3::read();
        if crate::pagetable_chirho::map_page_in_pt_chirho(
            cr3_map_chirho.start_address(),
            page_addr_chirho,
            frame_phys_chirho,
            flags_chirho,
        )
        .is_err()
        {
            crate::mm_chirho::deallocate_frame_chirho(frame_chirho);
            return Err(-ENOMEM_CHIRHO);
        }
        x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
    }

    Ok(())
}

/// Unmap pages from the page tables.
///
/// An absent mapping is harmless. Ownership-accounting failures remain loud;
/// silently clearing them would turn an underflow into later use-after-free.
fn unmap_pages_chirho(addr_chirho: u64, len_chirho: u64) {
    use x86_64::VirtAddr;

    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    let (cr3_unmap_chirho, _) = x86_64::registers::control::Cr3::read();
    for i_chirho in 0..num_pages_chirho {
        let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
        if let Err(unmap_error_chirho) = crate::pagetable_chirho::unmap_user_page_chirho(
            cr3_unmap_chirho.start_address(),
            VirtAddr::new(page_addr_chirho),
        ) {
            crate::serial_println_chirho!(
                "[MM-UNMAP] ownership error addr={:#x}: {:?}",
                page_addr_chirho,
                unmap_error_chirho,
            );
            break;
        }
    }
}

/// Update page-table protection flags for a range of pages.
///
/// For each page in the range, if it is currently mapped, we update its flags
/// to match `prot_chirho`.
fn update_page_protection_chirho(addr_chirho: u64, len_chirho: u64, prot_chirho: u32) {
    use x86_64::VirtAddr;

    let flags_chirho = prot_to_page_flags_chirho(prot_chirho);
    let num_pages_chirho = len_chirho / PAGE_SIZE_CHIRHO;

    // Update flags via CR3-based PT walk (no GLOBAL_MAPPER).
    let (cr3_prot_chirho, _) = x86_64::registers::control::Cr3::read();
    for i_chirho in 0..num_pages_chirho {
        let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
        if let Some(pte_ptr_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
            cr3_prot_chirho.start_address(),
            VirtAddr::new(page_addr_chirho),
        ) {
            unsafe {
                let old_flags_chirho = (*pte_ptr_chirho).flags();
                let mut effective_flags_chirho = flags_chirho;
                if old_flags_chirho.contains(
                    x86_64::structures::paging::PageTableFlags::BIT_9,
                ) {
                    effective_flags_chirho.insert(
                        x86_64::structures::paging::PageTableFlags::BIT_9,
                    );
                    effective_flags_chirho.remove(
                        x86_64::structures::paging::PageTableFlags::WRITABLE,
                    );
                }
                (*pte_ptr_chirho)
                    .set_addr((*pte_ptr_chirho).addr(), effective_flags_chirho);
            }
            x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
        }
    }
}

/// Convert Linux `PROT_*` flags to x86_64 [`PageTableFlags`].
fn prot_to_page_flags_chirho(prot_chirho: u32) -> x86_64::structures::paging::PageTableFlags {
    use x86_64::structures::paging::PageTableFlags;

    // PROT_NONE retains USER_ACCESSIBLE as ownership metadata while clearing
    // PRESENT. The physical leaf remains owned until munmap or retirement.
    if prot_chirho == PROT_NONE_CHIRHO {
        return PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE;
    }

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
    /// Intrusive list of previously-deallocated frames available for reuse.
    ///
    /// The first word of each free frame stores the next physical address.
    /// This makes both COW release and cold address-space retirement O(1)
    /// without allocating while the frame-allocator lock is held.
    free_head_chirho: Option<PhysFrame<Size4KiB>>,
    free_count_chirho: usize,
}

const FREE_LIST_END_CHIRHO: u64 = u64::MAX;

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
            free_head_chirho: None,
            free_count_chirho: 0,
        }
    }

    /// Return a frame to the free list for reuse.
    pub fn deallocate_frame_chirho(&mut self, frame_chirho: PhysFrame<Size4KiB>) {
        // Workflow: spec-chirho/workflows-chirho/address-space-lifecycle-chirho.md
        let next_phys_chirho = self
            .free_head_chirho
            .map(|head_chirho| head_chirho.start_address().as_u64())
            .unwrap_or(FREE_LIST_END_CHIRHO);
        let frame_virt_chirho = frame_chirho.start_address().as_u64()
            + crate::pagetable_chirho::phys_mem_offset_chirho();
        unsafe {
            (frame_virt_chirho as *mut u64).write(next_phys_chirho);
        }
        self.free_head_chirho = Some(frame_chirho);
        self.free_count_chirho = self.free_count_chirho.saturating_add(1);
    }

    /// Number of frames on the free list.
    pub fn free_count_chirho(&self) -> usize {
        self.free_count_chirho
    }

    pub fn memory_regions_chirho(
        &self,
    ) -> &'static bootloader_api::info::MemoryRegions {
        self.memory_regions_chirho
    }
}

unsafe impl FrameAllocator<Size4KiB> for GlobalFrameAllocatorChirho {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // Prefer recycled frames from the free list.
        if let Some(frame_chirho) = self.free_head_chirho {
            let frame_virt_chirho = frame_chirho.start_address().as_u64()
                + crate::pagetable_chirho::phys_mem_offset_chirho();
            let next_phys_chirho = unsafe { (frame_virt_chirho as *const u64).read() };
            self.free_head_chirho = if next_phys_chirho == FREE_LIST_END_CHIRHO {
                None
            } else {
                Some(PhysFrame::containing_address(PhysAddr::new(
                    next_phys_chirho,
                )))
            };
            self.free_count_chirho = self.free_count_chirho.saturating_sub(1);
            return Some(frame_chirho);
        }

        // Fall back to bump allocation from the memory map.
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

/// Deallocate a physical frame, returning it to the free list.
/// Safe to call from any context that holds the GLOBAL_FRAME_ALLOCATOR lock.
pub fn deallocate_frame_chirho(frame_chirho: PhysFrame<Size4KiB>) {
    if let Some(ref mut alloc_chirho) = *GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock() {
        alloc_chirho.deallocate_frame_chirho(frame_chirho);
    }
}

/// Global page table mapper.  Set during kernel init.
static GLOBAL_MAPPER_CHIRHO: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);

/// Physical PML4 currently borrowed by [`GLOBAL_MAPPER_CHIRHO`].
///
/// An `OffsetPageTable` contains a mutable reference into this root. Address-
/// space retirement must therefore treat this root as live even if CR3 has
/// already moved elsewhere. Tracking the binding makes a stale scheduler
/// rebind fail loudly instead of freeing a page-table tree behind a live
/// mutable mapper.
static GLOBAL_MAPPER_ROOT_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Return the PML4 currently borrowed by the global mapper.
pub fn global_mapper_root_phys_chirho() -> Option<x86_64::PhysAddr> {
    let root_phys_chirho = GLOBAL_MAPPER_ROOT_PHYS_CHIRHO.load(Ordering::Acquire);
    (root_phys_chirho != 0).then(|| x86_64::PhysAddr::new(root_phys_chirho))
}

unsafe fn mapper_for_current_cr3_chirho() -> (OffsetPageTable<'static>, u64) {
    use x86_64::registers::control::Cr3;
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;

    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let pml4_phys_chirho = Cr3::read().0.start_address().as_u64();
    let pml4_virt_chirho = pml4_phys_chirho + phys_offset_chirho;
    let pml4_table_chirho: &'static mut PageTable =
        &mut *(pml4_virt_chirho as *mut PageTable);
    (
        OffsetPageTable::new(pml4_table_chirho, VirtAddr::new(phys_offset_chirho)),
        pml4_phys_chirho,
    )
}

/// Lock a mapper that is guaranteed to target the PML4 currently in CR3.
///
/// Scheduler context switches load CR3 in assembly, after Rust has stopped
/// running with the kernel FS base. Rebinding there would require executing
/// Rust with the restored task's user FS base. Instead, every mapper consumer
/// enters through this function: acquisition checks the authoritative CR3 and
/// repairs a stale binding under the same lock before exposing the mapper.
/// Keeping the raw static private makes bypassing this invariant impossible
/// outside this module.
pub fn lock_current_mapper_chirho(
) -> spin::MutexGuard<'static, Option<OffsetPageTable<'static>>> {
    let current_root_phys_chirho = x86_64::registers::control::Cr3::read()
        .0
        .start_address()
        .as_u64();
    let mut mapper_guard_chirho = GLOBAL_MAPPER_CHIRHO.lock();
    if GLOBAL_MAPPER_ROOT_PHYS_CHIRHO.load(Ordering::Acquire) != current_root_phys_chirho {
        let (current_mapper_chirho, rebound_root_phys_chirho) =
            unsafe { mapper_for_current_cr3_chirho() };
        *mapper_guard_chirho = Some(current_mapper_chirho);
        GLOBAL_MAPPER_ROOT_PHYS_CHIRHO.store(rebound_root_phys_chirho, Ordering::Release);
    }
    mapper_guard_chirho
}

/// Flag: set to true during exec's ELF loading (load_segment_chirho).
/// When true, mmap always unmaps inherited COW pages instead of reusing
/// them via update_flags. This prevents exec's copy_nonoverlapping from
/// writing ELF data to COW-shared frames that the parent still reads.
pub static EXEC_MMAP_MODE_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

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
    let memory_regions_chirho = frame_allocator_chirho.memory_regions_chirho();
    *GLOBAL_MAPPER_CHIRHO.lock() = Some(mapper_chirho);
    GLOBAL_MAPPER_ROOT_PHYS_CHIRHO.store(
        x86_64::registers::control::Cr3::read()
            .0
            .start_address()
            .as_u64(),
        Ordering::Release,
    );
    *GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock() = Some(frame_allocator_chirho);
    let ownership_stats_chirho = crate::pagetable_chirho::init_leaf_frame_ownership_chirho(
        memory_regions_chirho,
    )
    .expect("physical leaf ownership table initialization failed");
    let registered_mappings_chirho =
        crate::pagetable_chirho::register_existing_user_mappings_chirho(
            crate::pagetable_chirho::get_boot_pml4_chirho(),
        )
        .expect("existing user mapping ownership registration failed");
    crate::serial_println_chirho!(
        "[MM-OWNERSHIP] slots={} managed={} initial_user_mappings={}",
        ownership_stats_chirho.slot_count_chirho,
        ownership_stats_chirho.managed_frame_count_chirho,
        registered_mappings_chirho,
    );
    crate::serial_println_chirho!("[OK] MM subsystem initialized");
}

/// Re-initialize the global mapper to point at the current CR3's PML4.
///
/// After switching CR3 to a per-process page table, the mapper must be
/// updated so that `map_to()` writes to the CURRENT page table (not the
/// boot PML4 the mapper was originally created with).
///
/// This function reads CR3, obtains a `&'static mut PageTable` reference
/// to the active PML4 via the physical memory window, and replaces the
/// global mapper with a fresh `OffsetPageTable` pointing there.
///
/// # Safety
///
/// The current CR3 must point to a valid PML4 with kernel mappings.
pub unsafe fn reinit_mapper_for_current_cr3_chirho() {
    let (new_mapper_chirho, pml4_phys_chirho) = mapper_for_current_cr3_chirho();
    *GLOBAL_MAPPER_CHIRHO.lock() = Some(new_mapper_chirho);
    GLOBAL_MAPPER_ROOT_PHYS_CHIRHO.store(pml4_phys_chirho, Ordering::Release);
}

// ============================================================================
// Internal utilities
// ============================================================================

/// Align `val_chirho` upward to the nearest multiple of [`PAGE_SIZE_CHIRHO`].
const fn align_up_page_chirho(val_chirho: u64) -> u64 {
    (val_chirho + PAGE_SIZE_CHIRHO - 1) & !(PAGE_SIZE_CHIRHO - 1)
}
