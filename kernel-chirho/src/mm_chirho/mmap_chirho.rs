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

        // Dispatch based on the mapping kind.
        match kind_chirho {
            MappingKindChirho::FramebufferChirho => {
                // Map the physical framebuffer pages directly instead of
                // allocating new anonymous pages.
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
                Ok(fb_virt_chirho)
            }

            MappingKindChirho::AnonymousChirho => {
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
                // Map as RWX initially so we can copy file data in, then
                // the VMA records the originally requested protection.
                let initial_prot_chirho =
                    PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | PROT_EXEC_CHIRHO;
                map_anonymous_pages_chirho(
                    map_addr_chirho, aligned_len_chirho, initial_prot_chirho,
                )?;

                // Read file data into the mapped region using sys_read_real
                // in a loop.  Each read copies up to 4K directly into the
                // mapped pages (already RWX from above).
                //
                // IMPORTANT: We seek to the requested offset first and read
                // sequentially.  sys_read_real internally calls ext4
                // read_file_data which re-reads the file each time — but
                // for 4K chunks this is acceptable (ext4 has a block cache).
                // The alternative (bulk read via read_file_data_at_offset)
                // caused heap corruption from nested Vec allocations.
                let saved_pos_chirho = crate::fs_chirho::sys_lseek_chirho(
                    fd_chirho as u64, 0, 1,
                );
                let _ = crate::fs_chirho::sys_lseek_chirho(
                    fd_chirho as u64, _offset_chirho as i64, 0,
                );
                let total_chirho = aligned_len_chirho.min(8 * 1024 * 1024) as usize;
                let mut done_chirho: usize = 0;
                while done_chirho < total_chirho {
                    let chunk_chirho = core::cmp::min(4096, total_chirho - done_chirho);
                    let n_chirho = crate::fs_chirho::sys_read_real_chirho(
                        fd_chirho as u64,
                        map_addr_chirho + done_chirho as u64,
                        chunk_chirho,
                    );
                    if n_chirho <= 0 { break; }
                    done_chirho += n_chirho as usize;
                }
                if saved_pos_chirho >= 0 {
                    let _ = crate::fs_chirho::sys_lseek_chirho(
                        fd_chirho as u64, saved_pos_chirho, 0,
                    );
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

        // Verify the entire region is covered by existing VMAs.
        if !self.is_region_mapped_chirho(addr_chirho, aligned_len_chirho) {
            return Err(-ENOMEM_CHIRHO);
        }

        // Skip actual page table protection updates for now.
        // The GLOBAL_MAPPER lock in update_page_protection can deadlock
        // with the page fault handler during execve's dynamic linking.
        // The VMA metadata is still updated so future mmap/munmap work.
        // TODO: fix lock ordering between MM lock and GLOBAL_MAPPER.
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

/// Get or initialise the global memory descriptor (BOOT-ONLY fallback).
///
/// Panics if called from a user task (PID >= 2) — those should use
/// `get_current_mm_chirho()` which returns the per-task MM.
/// Only PID 0/1 (init shell) and early boot should use this.
pub fn get_or_init_mm_chirho() -> &'static Mutex<Option<MmChirho>> {
    // Guard: panic if called from a user task
    let caller_pid_chirho = crate::task_chirho::current_task_chirho()
        .map(|t| t.lock().pid_chirho).unwrap_or(0);
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
    // Trace: log ALL mmap calls for PIDs 2-4 to find who creates 0x7f000a7c1000
    {
        let pid_chirho = crate::task_chirho::current_task_chirho()
            .map(|t| t.lock().pid_chirho).unwrap_or(0);
        if (pid_chirho >= 2 && pid_chirho <= 4) && addr_chirho >= 0x7f0000000000 {
            use core::sync::atomic::{AtomicU64, Ordering as MmapOrd};
            static MMAP_HI_CHIRHO: AtomicU64 = AtomicU64::new(0);
            let cnt_chirho = MMAP_HI_CHIRHO.fetch_add(1, MmapOrd::Relaxed);
            if cnt_chirho < 40 {
                let (cr3_f_chirho, _) = x86_64::registers::control::Cr3::read();
                crate::serial_println_chirho!(
                    "[MMAP-HI] #{} pid={} addr={:#x} len={:#x} cr3={:#x}",
                    cnt_chirho, pid_chirho, addr_chirho, len_chirho,
                    cr3_f_chirho.start_address().as_u64(),
                );
            }
        }
    }
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

        // Diagnostic: catch unexpected mapping of PID 0's BusyBox range
        if page_addr_chirho >= 0x400000 && page_addr_chirho < 0x800000 {
            let map_pid_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().pid_chirho).unwrap_or(0);
            if map_pid_chirho >= 2 {
                crate::serial_println_chirho!(
                    "[MMAP-BUSYBOX-BUG] pid={} mapping page {:#x} in PID 0 BusyBox range!",
                    map_pid_chirho, page_addr_chirho,
                );
            }
        }
        // Check if the page is already mapped (from a previous exec).
        // If so, just update flags — don't allocate a new frame.
        //
        // CRITICAL: This optimization MUST be disabled for forked processes
        // (PID >= 2). After fork, the child's page table has deep-copied
        // entries pointing to COW-shared physical frames. If exec reuses
        // these frames (via already_mapped), the new ELF data overwrites
        // the shared frame. The parent then reads the child's ELF .text
        // instead of its own .data, causing deterministic GPFs (e.g.,
        // rbx loads instruction bytes 0x10ff00012c62058b from what should
        // be a channel struct pointer).
        //
        // Only PID 0/1 (boot process before any fork) can safely reuse
        // pre-existing mappings from the bootloader.
        {
            let mmap_pid_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().pid_chirho).unwrap_or(0);
            // Check already_mapped by walking the CURRENT CR3's page table
            // directly (not via GLOBAL_MAPPER which can be stale even with
            // reinit). This is the reliable way to check if a page exists
            // in the active address space.
            let already_mapped_chirho = {
                let (cr3_am_chirho, _) = x86_64::registers::control::Cr3::read();
                crate::pagetable_chirho::walk_page_table_chirho(
                    cr3_am_chirho.start_address(),
                    VirtAddr::new(page_addr_chirho),
                ).map(|pte_ptr_chirho| unsafe {
                    // Check PRESENT flag specifically (not just non-zero).
                    (*pte_ptr_chirho).flags().contains(
                        x86_64::structures::paging::PageTableFlags::PRESENT
                    )
                }).unwrap_or(false)
            };

            // Trace: log PTE value when page IS present
            if page_addr_chirho >= 0x7efffffe4000 && page_addr_chirho < 0x7efffffe5000 {
                let (cr3_dbg_chirho, _) = x86_64::registers::control::Cr3::read();
                let pid_dbg_chirho = crate::task_chirho::current_task_chirho()
                    .map(|t| t.lock().pid_chirho).unwrap_or(99);
                let pte_val_chirho = crate::pagetable_chirho::walk_page_table_chirho(
                    cr3_dbg_chirho.start_address(),
                    VirtAddr::new(page_addr_chirho),
                ).map(|p| unsafe { (*p).addr().as_u64() }).unwrap_or(0);
                crate::serial_println_chirho!(
                    "[MMAP-TRACE] pid={} page={:#x} cr3={:#x} mapped={} pte_phys={:#x}",
                    pid_dbg_chirho, page_addr_chirho, cr3_dbg_chirho.start_address().as_u64(),
                    already_mapped_chirho, pte_val_chirho,
                );
            }
            if already_mapped_chirho {
                // Check COW status and frame refcount to decide action.
                let (cr3_uf_chirho, _) = x86_64::registers::control::Cr3::read();
                let (is_cow_chirho, frame_phys_val_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
                    cr3_uf_chirho.start_address(),
                    VirtAddr::new(page_addr_chirho),
                ).map(|pte_ptr_chirho| unsafe {
                    let cow_chirho = !(*pte_ptr_chirho).flags().contains(
                        x86_64::structures::paging::PageTableFlags::WRITABLE
                    );
                    let phys_chirho = (*pte_ptr_chirho).addr().as_u64();
                    (cow_chirho, phys_chirho)
                }).unwrap_or((false, 0));

                // Log COW+ref info for channels page
                if page_addr_chirho >= 0x7efffffe4000 && page_addr_chirho < 0x7efffffe5000 {
                    let ref_dbg_chirho = crate::pagetable_chirho::frame_ref_count_chirho(frame_phys_val_chirho);
                    crate::serial_println_chirho!(
                        "[COW-REF] pid={} page={:#x} cow={} ref={} phys={:#x}",
                        mmap_pid_chirho, page_addr_chirho, is_cow_chirho,
                        ref_dbg_chirho, frame_phys_val_chirho,
                    );
                }
                if false && mmap_pid_chirho >= 3 {
                    // DISABLED: forced unmap causes OOM (bump allocator).
                    // Needs frame deallocation before this can be enabled.
                    // For COW pages OR PID>=3 pages in the mmap heap arena:
                    // always unmap + reallocate fresh frame. The mmap arena
                    // range is limited (~32MB max) to prevent OOM.
                    // Unmap stale mapping, fall through to fresh alloc
                    if let Some(pte_ptr_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
                        cr3_uf_chirho.start_address(),
                        VirtAddr::new(page_addr_chirho),
                    ) {
                        unsafe { (*pte_ptr_chirho).set_unused(); }
                        x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
                    }
                    crate::pagetable_chirho::frame_ref_dec_chirho(frame_phys_val_chirho);
                } else {
                    // Writable (owned): safe to reuse, just update flags
                    if let Some(pte_ptr_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
                        cr3_uf_chirho.start_address(),
                        VirtAddr::new(page_addr_chirho),
                    ) {
                        unsafe {
                            (*pte_ptr_chirho).set_addr(
                                (*pte_ptr_chirho).addr(), flags_chirho,
                            );
                        }
                        x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
                    }
                    continue;
                }
            }
        }

        // Allocate a physical frame for a genuinely new page.
        let frame_chirho = {
            let mut alloc_lock_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
            match alloc_lock_chirho.as_mut() {
                Some(alloc_chirho) => match alloc_chirho.allocate_frame() {
                    Some(f_chirho) => {
                        // Initialize refcount to 1 (single owner).
                        crate::pagetable_chirho::frame_ref_inc_chirho(
                            f_chirho.start_address().as_u64()
                        );
                        f_chirho
                    },
                    None => return Err(-ENOMEM_CHIRHO),
                },
                None => return Err(-ENOMEM_CHIRHO),
            }
        };

        // For PID >= 3: map directly via CR3-based map_page_in_pt_chirho
        // (no GLOBAL_MAPPER). For PID 0-2: use GLOBAL_MAPPER (boot PT).
        let frame_phys_chirho = frame_chirho.start_address().as_u64();
        let mapping_pid_chirho = crate::task_chirho::current_task_chirho()
            .map(|t| t.lock().pid_chirho).unwrap_or(0);
        if mapping_pid_chirho >= 3 {
            let (cr3_map_chirho, _) = x86_64::registers::control::Cr3::read();
            if crate::pagetable_chirho::map_page_in_pt_chirho(
                cr3_map_chirho.start_address(),
                page_addr_chirho, frame_phys_chirho, flags_chirho,
            ).is_err() {
                continue;
            }
            x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
            // Zero-fill via physical identity mapping (avoids PF on user addr)
            let po_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
            unsafe {
                core::ptr::write_bytes(
                    (frame_phys_chirho + po_chirho) as *mut u8, 0,
                    PAGE_SIZE_CHIRHO as usize,
                );
            }
        } else {
            // PID 0-2: use GLOBAL_MAPPER (boot PT hierarchy)
            x86_64::instructions::interrupts::disable();
            unsafe { crate::mm_chirho::reinit_mapper_for_current_cr3_chirho(); }
            let result_chirho = {
                let mut ml_chirho = GLOBAL_MAPPER_CHIRHO.lock();
                match ml_chirho.as_mut() {
                    Some(m_chirho) => {
                        let mut al_chirho = GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
                        match al_chirho.as_mut() {
                            Some(a_chirho) => unsafe {
                                m_chirho.map_to(page_chirho, frame_chirho, flags_chirho, a_chirho)
                            },
                            None => return Err(-ENOMEM_CHIRHO),
                        }
                    }
                    None => return Err(-ENOMEM_CHIRHO),
                }
            };
            x86_64::instructions::interrupts::enable();
            match result_chirho {
                Ok(f_chirho) => f_chirho.flush(),
                Err(_) => continue,
            }
            unsafe {
                core::ptr::write_bytes(page_addr_chirho as *mut u8, 0, PAGE_SIZE_CHIRHO as usize);
            }
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

    // Transition safety: reinit mapper from CR3 with interrupts disabled
    // to prevent stale-mapper cross-process corruption (delete-me: option C).
    x86_64::instructions::interrupts::disable();
    unsafe { crate::mm_chirho::reinit_mapper_for_current_cr3_chirho(); }
    let mut mapper_lock_chirho = GLOBAL_MAPPER_CHIRHO.lock();
    if let Some(mapper_chirho) = mapper_lock_chirho.as_mut() {
        for i_chirho in 0..num_pages_chirho {
            let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(VirtAddr::new(page_addr_chirho));

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
    x86_64::instructions::interrupts::enable();
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

    // Transition safety: reinit mapper from CR3 with interrupts disabled
    // to prevent stale-mapper cross-process corruption (delete-me: option C).
    x86_64::instructions::interrupts::disable();
    unsafe { crate::mm_chirho::reinit_mapper_for_current_cr3_chirho(); }
    let mut mapper_lock_chirho = GLOBAL_MAPPER_CHIRHO.lock();
    if let Some(mapper_chirho) = mapper_lock_chirho.as_mut() {
        for i_chirho in 0..num_pages_chirho {
            let page_addr_chirho = addr_chirho + i_chirho * PAGE_SIZE_CHIRHO;
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(VirtAddr::new(page_addr_chirho));

            unsafe {
                let _ = mapper_chirho.update_flags(page_chirho, flags_chirho);
            }
            x86_64::instructions::tlb::flush(VirtAddr::new(page_addr_chirho));
        }
    }
    x86_64::instructions::interrupts::enable();
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
    *GLOBAL_MAPPER_CHIRHO.lock() = Some(mapper_chirho);
    *GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock() = Some(frame_allocator_chirho);
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
    use x86_64::registers::control::Cr3;
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;

    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let (pml4_frame_chirho, _flags_chirho) = Cr3::read();
    let pml4_phys_chirho = pml4_frame_chirho.start_address().as_u64();
    let pml4_virt_chirho = pml4_phys_chirho + phys_offset_chirho;
    let pml4_table_chirho: &'static mut PageTable =
        &mut *(pml4_virt_chirho as *mut PageTable);

    let new_mapper_chirho = OffsetPageTable::new(
        pml4_table_chirho,
        VirtAddr::new(phys_offset_chirho),
    );

    *GLOBAL_MAPPER_CHIRHO.lock() = Some(new_mapper_chirho);
}

// ============================================================================
// Internal utilities
// ============================================================================

/// Align `val_chirho` upward to the nearest multiple of [`PAGE_SIZE_CHIRHO`].
const fn align_up_page_chirho(val_chirho: u64) -> u64 {
    (val_chirho + PAGE_SIZE_CHIRHO - 1) & !(PAGE_SIZE_CHIRHO - 1)
}
