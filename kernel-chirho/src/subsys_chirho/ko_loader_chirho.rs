// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux `.ko` kernel module loader for the Lineluya kernel.
//!
//! Parses ELF relocatable objects (`ET_REL`) produced by the Linux kernel build
//! system, resolves symbols against the kernel symbol table, and manages the
//! lifecycle of loaded modules (init / cleanup).
//!
//! ## Phase A2 milestones implemented
//!
//! - **A2-003**: x86_64 ELF relocation engine — handles `R_X86_64_64`,
//!   `R_X86_64_PC32`, `R_X86_64_32`, `R_X86_64_32S`, `R_X86_64_PLT32`, and
//!   `R_X86_64_GOTPCREL` relocations.  Parses `SHT_RELA` sections and applies
//!   patches to loaded module memory.
//! - **A2-004**: Kernel symbol export table — a runtime registry of kernel
//!   symbols that modules can resolve against.  Includes the
//!   [`EXPORT_SYMBOL_CHIRHO!`] macro and is pre-populated with key kernel
//!   functions (`printk` / `serial_println`, `kmalloc` / `kfree`,
//!   `register_chrdev`, `schedule`, `mutex_lock`, `mutex_unlock`).
//! - **A2-005**: Module init / cleanup execution — after relocation, locates
//!   `init_module` and `cleanup_module` symbols and calls them via function
//!   pointers.
//! - **A2-006**: C ABI shim: `printk` — fully C-callable `printk` that parses
//!   Linux log-level prefixes (`<0>`..`<7>`), forwards messages to serial, and
//!   stores them in a 256-entry kernel log ring buffer
//!   ([`KLOG_RING_CHIRHO`]).  Also provides `vprintk` and `printk_emit`.
//! - **A2-007**: C ABI shim: `kmalloc` / `kfree` — C-callable `kmalloc(size,
//!   gfp_flags)` and `kfree(ptr)` with internal allocation tracking.  Also
//!   provides `kzalloc`, `krealloc`, and `ksize`.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;
use spin::Mutex;

use crate::elf_chirho::{
    ELF_MAGIC_CHIRHO, ELFCLASS64_CHIRHO, ELFDATA2LSB_CHIRHO, EM_X86_64_CHIRHO,
    Elf64HeaderChirho,
};
use crate::syscall_chirho::{
    EBUSY_CHIRHO, EFAULT_CHIRHO, EINVAL_CHIRHO, ENOENT_CHIRHO, ENOEXEC_CHIRHO, ENOMEM_CHIRHO,
    ERANGE_CHIRHO,
};
use crate::uaccess_chirho;

// ---------------------------------------------------------------------------
// Helper: read a NUL-terminated string from a string table slice
// ---------------------------------------------------------------------------

fn read_str_chirho<'a>(strtab_chirho: &'a [u8], offset_chirho: usize) -> Result<&'a str, KoErrorChirho> {
    if offset_chirho >= strtab_chirho.len() {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }
    let start_chirho = offset_chirho;
    let mut end_chirho = start_chirho;
    while end_chirho < strtab_chirho.len() && strtab_chirho[end_chirho] != 0 {
        end_chirho += 1;
    }
    core::str::from_utf8(&strtab_chirho[start_chirho..end_chirho])
        .map_err(|_| KoErrorChirho::InvalidStrtabChirho)
}

// ---------------------------------------------------------------------------
// ELF constants specific to relocatable objects (.ko files)
// ---------------------------------------------------------------------------

/// ELF type: relocatable object.
const ET_REL_CHIRHO: u16 = 1;

// -- Section header types ---------------------------------------------------

/// Inactive section header.
#[allow(dead_code)]
const SHT_NULL_CHIRHO: u32 = 0;

/// Program data.
#[allow(dead_code)]
const SHT_PROGBITS_CHIRHO: u32 = 1;

/// Symbol table.
const SHT_SYMTAB_CHIRHO: u32 = 2;

/// String table.
#[allow(dead_code)]
const SHT_STRTAB_CHIRHO: u32 = 3;

/// Relocation entries with explicit addends.
const SHT_RELA_CHIRHO: u32 = 4;

/// Section contains no data (BSS).
#[allow(dead_code)]
const SHT_NOBITS_CHIRHO: u32 = 8;

// -- Section header flags ---------------------------------------------------

/// Section contains writable data.
#[allow(dead_code)]
const SHF_WRITE_CHIRHO: u64 = 0x1;

/// Section occupies memory during execution.
#[allow(dead_code)]
const SHF_ALLOC_CHIRHO: u64 = 0x2;

/// Section contains executable instructions.
#[allow(dead_code)]
const SHF_EXECINSTR_CHIRHO: u64 = 0x4;

// -- Symbol binding / type --------------------------------------------------

/// Extract binding from st_info.
const fn elf64_st_bind_chirho(info_chirho: u8) -> u8 {
    info_chirho >> 4
}

/// Extract type from st_info.
const fn elf64_st_type_chirho(info_chirho: u8) -> u8 {
    info_chirho & 0xf
}

/// Global symbol binding.
const STB_GLOBAL_CHIRHO: u8 = 1;

/// Weak symbol binding.
#[allow(dead_code)]
const STB_WEAK_CHIRHO: u8 = 2;

/// Symbol type: function (code).
const STT_FUNC_CHIRHO: u8 = 2;

/// Symbol type: data object.
#[allow(dead_code)]
const STT_OBJECT_CHIRHO: u8 = 1;

/// Symbol type: no type specified.
#[allow(dead_code)]
const STT_NOTYPE_CHIRHO: u8 = 0;

/// Undefined section index.
const SHN_UNDEF_CHIRHO: u16 = 0;

// ===========================================================================
// A2-003: x86_64 relocation type constants
// ===========================================================================

/// `R_X86_64_64` — Direct 64-bit absolute address.
const R_X86_64_64_CHIRHO: u32 = 1;

/// `R_X86_64_PC32` — PC-relative 32-bit signed offset.
const R_X86_64_PC32_CHIRHO: u32 = 2;

/// `R_X86_64_32` — Direct 32-bit zero-extended address.
const R_X86_64_32_CHIRHO: u32 = 10;

/// `R_X86_64_32S` — Direct 32-bit sign-extended address.
const R_X86_64_32S_CHIRHO: u32 = 11;

/// `R_X86_64_PLT32` — 32-bit PLT relative (treated like PC32 for static link).
const R_X86_64_PLT32_CHIRHO: u32 = 4;

/// `R_X86_64_GOTPCREL` — 32-bit GOT-relative PC offset.
/// For kernel modules we resolve this to a direct PC-relative reference
/// to the symbol (no GOT indirection needed in a monolithic kernel image).
const R_X86_64_GOTPCREL_CHIRHO: u32 = 9;

/// `R_X86_64_REX_GOTPCRELX` — relaxed GOT-relative (GCC 7+, KASLR).
const R_X86_64_REX_GOTPCRELX_CHIRHO: u32 = 42;

/// `R_X86_64_GOTPCRELX` — relaxed GOT-relative (GCC 7+, KASLR).
const R_X86_64_GOTPCRELX_CHIRHO: u32 = 41;

/// `R_X86_64_NONE` — no relocation.
#[allow(dead_code)]
const R_X86_64_NONE_CHIRHO: u32 = 0;

// ---------------------------------------------------------------------------
// ELF Section Header (64-bit)
// ---------------------------------------------------------------------------

/// 64-bit ELF section header, corresponding to `Elf64_Shdr`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64ShdrChirho {
    /// Offset into the section header string table for this section's name.
    sh_name_chirho: u32,
    /// Section type (SHT_PROGBITS, SHT_SYMTAB, etc.).
    sh_type_chirho: u32,
    /// Section attribute flags.
    sh_flags_chirho: u64,
    /// Virtual address of the section in memory (0 for relocatable objects).
    sh_addr_chirho: u64,
    /// Offset of the section data in the file.
    sh_offset_chirho: u64,
    /// Size of the section data in bytes.
    sh_size_chirho: u64,
    /// Section header table index link (interpretation depends on type).
    sh_link_chirho: u32,
    /// Extra information (interpretation depends on type).
    sh_info_chirho: u32,
    /// Address alignment constraint.
    sh_addralign_chirho: u64,
    /// Size of each entry, for sections that contain fixed-size entries.
    sh_entsize_chirho: u64,
}

// ---------------------------------------------------------------------------
// ELF Symbol Table Entry (64-bit)
// ---------------------------------------------------------------------------

/// 64-bit ELF symbol table entry, corresponding to `Elf64_Sym`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64SymChirho {
    /// Offset into the string table for this symbol's name.
    st_name_chirho: u32,
    /// Symbol type and binding attributes.
    st_info_chirho: u8,
    /// Symbol visibility.
    st_other_chirho: u8,
    /// Section header table index this symbol is defined in.
    st_shndx_chirho: u16,
    /// Symbol value (address or offset).
    st_value_chirho: u64,
    /// Symbol size in bytes.
    st_size_chirho: u64,
}

// ===========================================================================
// A2-003: ELF Rela entry (64-bit)
// ===========================================================================

/// 64-bit ELF relocation entry with explicit addend, `Elf64_Rela`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64RelaChirho {
    /// Offset within the section where the relocation applies.
    r_offset_chirho: u64,
    /// Relocation type + symbol index packed together.
    r_info_chirho: u64,
    /// Explicit addend.
    r_addend_chirho: i64,
}

impl Elf64RelaChirho {
    /// Extract the symbol table index from `r_info`.
    const fn sym_chirho(&self) -> u32 {
        (self.r_info_chirho >> 32) as u32
    }

    /// Extract the relocation type from `r_info`.
    const fn type_chirho(&self) -> u32 {
        (self.r_info_chirho & 0xffff_ffff) as u32
    }
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------

/// State of a loaded kernel module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleStateChirho {
    /// Module is loaded and initialised.
    LoadedChirho,
    /// Module has been unloaded / cleaned up.
    UnloadedChirho,
}

// ---------------------------------------------------------------------------
// KoModuleChirho — represents a loaded .ko module
// ---------------------------------------------------------------------------

/// A loaded Linux kernel module (.ko).
#[derive(Debug)]
pub struct KoModuleChirho {
    /// Human-readable module name.
    pub name_chirho: String,
    /// Address of the module's `init_module` function, if found.
    pub init_fn_chirho: Option<u64>,
    /// Address of the module's `cleanup_module` function, if found.
    pub cleanup_fn_chirho: Option<u64>,
    /// Current lifecycle state.
    pub state_chirho: ModuleStateChirho,
    /// Number of symbols exported by this module.
    pub symbol_count_chirho: usize,
    /// Parsed section info for later relocation / unload.
    pub sections_chirho: Vec<KoSectionInfoChirho>,
    /// Base address of the allocated module memory region (for deallocation).
    pub module_mem_base_chirho: u64,
    /// Total size of the allocated module memory region.
    pub module_mem_size_chirho: usize,
    /// Module-image arena slot index when the module lives in the static
    /// kernel-range loader arena instead of a heap allocation.
    pub module_mem_arena_slot_chirho: Option<usize>,
    /// A2-016: Reference count — incremented when other modules depend on us.
    pub refcount_chirho: u32,
    /// A2-016: Names of modules that this module depends on (imported symbols from).
    pub depends_on_chirho: Vec<String>,
    /// A2-017: Module parameters parsed from insmod command line.
    pub params_chirho: Vec<ModuleParamChirho>,
}

// ---------------------------------------------------------------------------
// A2-017: Module parameter (parsed from "key=value" pairs on insmod cmdline)
// ---------------------------------------------------------------------------

/// A parsed module parameter (key=value from insmod command line).
#[derive(Debug, Clone)]
pub struct ModuleParamChirho {
    /// Parameter name.
    pub key_chirho: String,
    /// Parameter value (string representation).
    pub value_chirho: String,
}

/// Metadata for a single section inside the .ko.
#[derive(Debug, Clone)]
pub struct KoSectionInfoChirho {
    /// Section name (e.g. ".text", ".data").
    pub name_chirho: String,
    /// Section type.
    pub type_chirho: u32,
    /// Section flags.
    pub flags_chirho: u64,
    /// Offset within the .ko file image.
    pub offset_chirho: u64,
    /// Size of the section data.
    pub size_chirho: u64,
}

/// A parsed symbol from the module's symbol table.
#[derive(Debug, Clone)]
pub struct KoSymbolChirho {
    /// Symbol name.
    pub name_chirho: String,
    /// Symbol value (offset within section).
    pub value_chirho: u64,
    /// Section index this symbol belongs to.
    pub section_index_chirho: u16,
    /// Binding (STB_GLOBAL, STB_WEAK, etc.).
    pub binding_chirho: u8,
    /// Type (STT_FUNC, STT_OBJECT, etc.).
    pub sym_type_chirho: u8,
}

// ---------------------------------------------------------------------------
// KoErrorChirho
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing or loading a .ko module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KoErrorChirho {
    /// Data too short to contain an ELF header.
    TooShortChirho,
    /// ELF magic bytes do not match.
    InvalidMagicChirho,
    /// Not a 64-bit ELF.
    UnsupportedClassChirho,
    /// Not little-endian.
    UnsupportedEndianChirho,
    /// Not x86_64.
    UnsupportedMachineChirho,
    /// ELF type is not ET_REL (relocatable).
    NotRelocatableChirho,
    /// Section header table is invalid or missing.
    InvalidSectionHeadersChirho,
    /// Symbol table not found.
    NoSymtabChirho,
    /// String table not found or out of bounds.
    InvalidStrtabChirho,
    /// init_module symbol not found.
    NoInitSymbolChirho,
    /// Out-of-memory during parsing.
    OutOfMemoryChirho,
    /// Section data extends beyond file image.
    SectionOutOfBoundsChirho,
    /// Relocation references an undefined symbol that cannot be resolved.
    UnresolvedSymbolChirho,
    /// Unsupported relocation type encountered.
    UnsupportedRelocationChirho,
    /// A relocation value overflows the target field width.
    RelocationOverflowChirho,
}

/// Per-slot state for the static kernel-range module image arena.
#[derive(Clone, Copy)]
struct ModuleImageArenaSlotStateChirho {
    in_use_chirho: bool,
    used_bytes_chirho: usize,
}

const MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO: usize = 256 * 1024;
const MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO: usize = 8;

#[repr(align(4096))]
struct ModuleImageArenaStorageChirho {
    bytes_chirho:
        [[u8; MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO]; MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO],
}

static MODULE_IMAGE_ARENA_STATE_CHIRHO: Mutex<
    [ModuleImageArenaSlotStateChirho; MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO],
> = Mutex::new(
    [ModuleImageArenaSlotStateChirho {
        in_use_chirho: false,
        used_bytes_chirho: 0,
    }; MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO],
);

static mut MODULE_IMAGE_ARENA_STORAGE_CHIRHO: ModuleImageArenaStorageChirho =
    ModuleImageArenaStorageChirho {
        bytes_chirho: [[0u8; MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO];
            MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO],
    };

// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/// High-canonical base for module arena. Uses PDPT[510] PD[1] (not PD[0]
/// which is used by the bootloader's kernel text). The truncated 32-bit
/// value 0x80200000 sign-extends correctly for R_X86_64_32S relocations.
/// High-canonical virtual address for module arena. 0xFFFFFFFFC0100000
/// is in the Linux kernel module range. The low 32 bits (0xC0100000)
/// sign-extend correctly: 0xC0100000 as i32 = -0x3FF00000, which
/// sign-extends to 0xFFFFFFFFC0100000. This allows R_X86_64_32S
/// relocations to work (they truncate to 32 bits and sign-extend back).
const MODULE_ARENA_HIGH_BASE_CHIRHO: u64 = 0xFFFF_FFFF_C010_0000;

static MODULE_ARENA_HIGH_READY_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Next available offset in the thunk page (first page of module arena).
/// Each thunk is 12 bytes: movabs rax, <addr>; jmp rax.
static THUNK_NEXT_OFFSET_CHIRHO: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Thunk cache: maps kernel address → thunk address (avoid duplicates).
static THUNK_CACHE_CHIRHO: Mutex<alloc::vec::Vec<(u64, u64)>> =
    Mutex::new(alloc::vec::Vec::new());

/// Get or create a thunk at high-canonical address for a kernel symbol.
/// The thunk is: movabs rax, <kernel_addr>; jmp rax (12 bytes).
fn get_or_create_thunk_chirho(kernel_addr_chirho: u64) -> u64 {
    // Check cache first
    {
        let cache_chirho = THUNK_CACHE_CHIRHO.lock();
        for (ka_chirho, ta_chirho) in cache_chirho.iter() {
            if *ka_chirho == kernel_addr_chirho {
                return *ta_chirho;
            }
        }
    }

    // Allocate 12 bytes in the thunk page
    let offset_chirho = THUNK_NEXT_OFFSET_CHIRHO.fetch_add(12, core::sync::atomic::Ordering::SeqCst);
    if offset_chirho >= 4096 {
        // Thunk page full — return kernel addr as fallback
        return kernel_addr_chirho;
    }

    let thunk_virt_chirho = MODULE_ARENA_HIGH_BASE_CHIRHO + offset_chirho;
    // The thunk page is mapped to the same physical frame as BSS arena start.
    // Write the thunk via the BSS address (which is the same physical memory).
    let bss_base_chirho = unsafe {
        core::ptr::addr_of_mut!(MODULE_IMAGE_ARENA_STORAGE_CHIRHO.bytes_chirho) as *mut u8
    };
    let thunk_ptr_chirho = unsafe { bss_base_chirho.add(offset_chirho as usize) };

    // movabs rax, <kernel_addr>  = 0x48 0xB8 <8 bytes LE>
    // jmp rax                    = 0xFF 0xE0
    unsafe {
        *thunk_ptr_chirho.add(0) = 0x48; // REX.W
        *thunk_ptr_chirho.add(1) = 0xB8; // MOV RAX, imm64
        core::ptr::copy_nonoverlapping(
            &kernel_addr_chirho as *const u64 as *const u8,
            thunk_ptr_chirho.add(2),
            8,
        );
        *thunk_ptr_chirho.add(10) = 0xFF; // JMP
        *thunk_ptr_chirho.add(11) = 0xE0; // RAX
    }

    // Cache it
    {
        let mut cache_chirho = THUNK_CACHE_CHIRHO.lock();
        cache_chirho.push((kernel_addr_chirho, thunk_virt_chirho));
    }

    thunk_virt_chirho
}

/// Map BSS module arena to high-canonical addresses using raw page table ops.
pub fn init_module_arena_mapping_chirho() {
    let bss_base_chirho = unsafe {
        core::ptr::addr_of_mut!(MODULE_IMAGE_ARENA_STORAGE_CHIRHO.bytes_chirho) as u64
    };
    let total_chirho =
        MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO * MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO;
    let phys_off_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    // Use CURRENT CR3 via raw assembly (Cr3::read may panic on non-standard values)
    let pml4_phys_chirho: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) pml4_phys_chirho, options(nostack));
    }
    let pml4_phys_chirho = pml4_phys_chirho & 0x000F_FFFF_FFFF_F000; // mask flags

    // Convert BSS virtual address to physical using page table walk
    // (can't use simple offset subtraction — BSS is in kernel PML4[1],
    // not in the physical memory window).
    let bss_phys_base_chirho = {
        let (cr3_chirho, _) = x86_64::registers::control::Cr3::read();
        if let Some(pte_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
            cr3_chirho.start_address(),
            x86_64::VirtAddr::new(bss_base_chirho),
        ) {
            unsafe { (*pte_chirho).addr().as_u64() }
        } else {
            crate::serial_println_chirho!("[KO] FATAL: can't resolve BSS physical address");
            return;
        }
    };
    crate::serial_println_chirho!(
        "[KO] BSS virt={:#x} phys={:#x}", bss_base_chirho, bss_phys_base_chirho
    );

    let mut ok_chirho = 0u64;
    for off_chirho in (0..total_chirho).step_by(4096) {
        // Get physical address for each BSS page via PT walk
        let page_virt_chirho = bss_base_chirho + off_chirho as u64;
        let phys_chirho = {
            let (cr3_chirho, _) = x86_64::registers::control::Cr3::read();
            if let Some(pte_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
                cr3_chirho.start_address(),
                x86_64::VirtAddr::new(page_virt_chirho),
            ) {
                unsafe { (*pte_chirho).addr().as_u64() }
            } else {
                continue; // Page not mapped
            }
        };
        let virt_chirho = MODULE_ARENA_HIGH_BASE_CHIRHO + off_chirho as u64;
        if crate::pagetable_chirho::map_page_raw_chirho(pml4_phys_chirho, virt_chirho, phys_chirho).is_ok() {
            ok_chirho += 1;
        }
    }
    // Direct serial byte to prove we reach this point
    unsafe {
        while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
        x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b'V');
        while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
        x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b'\n');
    }
    if ok_chirho > 0 {
        let first_phys_chirho = bss_base_chirho - phys_off_chirho;
        let first_virt_chirho = MODULE_ARENA_HIGH_BASE_CHIRHO;
        let i4 = ((first_virt_chirho >> 39) & 0x1FF) as usize;
        let i3 = ((first_virt_chirho >> 30) & 0x1FF) as usize;
        let i2 = ((first_virt_chirho >> 21) & 0x1FF) as usize;
        let i1 = ((first_virt_chirho >> 12) & 0x1FF) as usize;
        let rd = |phys: u64, idx: usize| -> u64 {
            unsafe { *((phys + phys_off_chirho) as *const u64).add(idx) }
        };
        let ae = |e: u64| -> u64 { e & 0x000F_FFFF_FFFF_F000 };
        let pml4e = rd(pml4_phys_chirho, i4);
        let pdpte = rd(ae(pml4e), i3);
        let pde = rd(ae(pdpte), i2);
        let pte = rd(ae(pde), i1);
        // Use raw serial to avoid format macro issues during early boot
        let verify_msg_chirho = if pte == (first_phys_chirho | 0x03) {
            "[KO] arena PTE CORRECT\r\n"
        } else {
            "[KO] arena PTE WRONG\r\n"
        };
        for &b_chirho in verify_msg_chirho.as_bytes() {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        // Store the PDPT phys address for insmod PML4[511] fixup
        // pml4e = current PML4[511] entry, extract phys addr from bits 12-51
        let pdpt_phys_from_pml4_chirho = ae(pml4e);
        // Store full PML4[511] entry value (phys + flags) for direct write
        crate::pagetable_chirho::set_module_arena_pml4_entry_chirho(pml4e);
        MODULE_ARENA_HIGH_READY_CHIRHO.store(true, core::sync::atomic::Ordering::Release);
        crate::serial_println_chirho!(
            "[KO] Stored PML4[511] = {:#x} for per-process PT propagation",
            pml4e,
        );
    }
    crate::serial_println_chirho!(
        "[KO] Module arena: {:#x} ({} pages)", MODULE_ARENA_HIGH_BASE_CHIRHO, ok_chirho,
    );
}

/// Offset to skip the first page (reserved for kernel symbol thunks).
const THUNK_PAGE_SIZE_CHIRHO: usize = 4096;

unsafe fn module_image_arena_slot_ptr_chirho(slot_index_chirho: usize) -> *mut u8 {
    // If high-canonical mapping is ready, return the high address.
    // Skip first page (reserved for kernel symbol thunks).
    if MODULE_ARENA_HIGH_READY_CHIRHO.load(core::sync::atomic::Ordering::Acquire) {
        let high_addr_chirho = MODULE_ARENA_HIGH_BASE_CHIRHO
            + THUNK_PAGE_SIZE_CHIRHO as u64
            + (slot_index_chirho * MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO) as u64;
        return high_addr_chirho as *mut u8;
    }
    // Fallback: BSS pointer (skip thunk page for consistency)
    let base_chirho =
        core::ptr::addr_of_mut!(MODULE_IMAGE_ARENA_STORAGE_CHIRHO.bytes_chirho) as *mut u8;
    unsafe { base_chirho.add(THUNK_PAGE_SIZE_CHIRHO + slot_index_chirho * MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO) }
}

fn allocate_module_image_slot_chirho(
    requested_bytes_chirho: usize,
) -> Result<(usize, *mut u8), KoErrorChirho> {
    if requested_bytes_chirho == 0
        || requested_bytes_chirho > MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO
    {
        crate::serial_println_chirho!(
            "[KO] Module image request {} exceeds slot size {}",
            requested_bytes_chirho,
            MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO
        );
        return Err(KoErrorChirho::OutOfMemoryChirho);
    }

    let mut arena_state_chirho = MODULE_IMAGE_ARENA_STATE_CHIRHO.lock();
    let Some(slot_index_chirho) = arena_state_chirho
        .iter()
        .position(|slot_state_chirho| !slot_state_chirho.in_use_chirho)
    else {
        crate::serial_println_chirho!(
            "[KO] Module arena exhausted: {} slots busy",
            MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO
        );
        return Err(KoErrorChirho::OutOfMemoryChirho);
    };

    arena_state_chirho[slot_index_chirho].in_use_chirho = true;
    arena_state_chirho[slot_index_chirho].used_bytes_chirho = requested_bytes_chirho;
    drop(arena_state_chirho);

    let slot_ptr_chirho = unsafe { module_image_arena_slot_ptr_chirho(slot_index_chirho) };

    // Debug: dump page table entries if high-canonical (disabled — using BSS)
    if false {
        let va_chirho = slot_ptr_chirho as u64;
        let (cr3_chirho, _) = x86_64::registers::control::Cr3::read();
        let pml4p_chirho = cr3_chirho.start_address().as_u64();
        let off_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let i4 = ((va_chirho >> 39) & 0x1FF) as usize;
        let i3 = ((va_chirho >> 30) & 0x1FF) as usize;
        let i2 = ((va_chirho >> 21) & 0x1FF) as usize;
        let i1 = ((va_chirho >> 12) & 0x1FF) as usize;
        let rd = |phys: u64, idx: usize| -> u64 {
            unsafe { *((phys + off_chirho) as *const u64).add(idx) }
        };
        let pml4e = rd(pml4p_chirho, i4);
        let pdpte = if pml4e & 1 != 0 { rd(pml4e & 0xFFFFFFFFF000, i3) } else { 0 };
        let pde = if pdpte & 1 != 0 { rd(pdpte & 0xFFFFFFFFF000, i2) } else { 0 };
        let pte = if pde & 1 != 0 { rd(pde & 0xFFFFFFFFF000, i1) } else { 0 };
        crate::serial_println_chirho!(
            "[PT-DUMP] va={:#x} CR3={:#x} PML4[{}]={:#x} PDPT[{}]={:#x} PD[{}]={:#x} PT[{}]={:#x}",
            va_chirho, pml4p_chirho, i4, pml4e, i3, pdpte, i2, pde, i1, pte,
        );
    }

    unsafe {
        core::ptr::write_bytes(slot_ptr_chirho, 0, requested_bytes_chirho);
    }

    crate::serial_println_chirho!(
        "[KO] Reserved module arena slot {} at {:#x} for {} bytes",
        slot_index_chirho,
        slot_ptr_chirho as usize,
        requested_bytes_chirho
    );

    Ok((slot_index_chirho, slot_ptr_chirho))
}

fn release_module_image_slot_chirho(slot_index_chirho: usize) {
    if slot_index_chirho >= MODULE_IMAGE_ARENA_SLOT_COUNT_CHIRHO {
        return;
    }

    let mut arena_state_chirho = MODULE_IMAGE_ARENA_STATE_CHIRHO.lock();
    let used_bytes_chirho = arena_state_chirho[slot_index_chirho].used_bytes_chirho;
    arena_state_chirho[slot_index_chirho].in_use_chirho = false;
    arena_state_chirho[slot_index_chirho].used_bytes_chirho = 0;
    drop(arena_state_chirho);

    let slot_ptr_chirho = unsafe { module_image_arena_slot_ptr_chirho(slot_index_chirho) };
    unsafe {
        core::ptr::write_bytes(slot_ptr_chirho, 0, MODULE_IMAGE_ARENA_SLOT_BYTES_CHIRHO);
    }

    crate::serial_println_chirho!(
        "[KO] Released module arena slot {} at {:#x} ({} bytes used)",
        slot_index_chirho,
        slot_ptr_chirho as usize,
        used_bytes_chirho
    );
}

struct ModuleImageAllocationGuardChirho {
    slot_index_chirho: usize,
    base_ptr_chirho: *mut u8,
    used_bytes_chirho: usize,
    armed_chirho: bool,
}

impl ModuleImageAllocationGuardChirho {
    fn allocate_chirho(total_bytes_chirho: usize) -> Result<Self, KoErrorChirho> {
        let (slot_index_chirho, base_ptr_chirho) =
            allocate_module_image_slot_chirho(total_bytes_chirho)?;
        Ok(Self {
            slot_index_chirho,
            base_ptr_chirho,
            used_bytes_chirho: total_bytes_chirho,
            armed_chirho: true,
        })
    }

    fn as_mut_slice_chirho(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.base_ptr_chirho, self.used_bytes_chirho) }
    }

    fn base_addr_chirho(&self) -> u64 {
        self.base_ptr_chirho as u64
    }

    fn into_loaded_parts_chirho(mut self) -> (u64, usize, usize) {
        self.armed_chirho = false;
        (
            self.base_ptr_chirho as u64,
            self.used_bytes_chirho,
            self.slot_index_chirho,
        )
    }
}

impl Drop for ModuleImageAllocationGuardChirho {
    fn drop(&mut self) {
        if self.armed_chirho {
            release_module_image_slot_chirho(self.slot_index_chirho);
        }
    }
}

impl KoErrorChirho {
    /// Map to a Linux errno value for syscall return.
    pub fn to_errno_chirho(self) -> i64 {
        match self {
            KoErrorChirho::TooShortChirho
            | KoErrorChirho::InvalidMagicChirho
            | KoErrorChirho::UnsupportedClassChirho
            | KoErrorChirho::UnsupportedEndianChirho
            | KoErrorChirho::UnsupportedMachineChirho
            | KoErrorChirho::NotRelocatableChirho
            | KoErrorChirho::InvalidSectionHeadersChirho
            | KoErrorChirho::NoSymtabChirho
            | KoErrorChirho::InvalidStrtabChirho
            | KoErrorChirho::SectionOutOfBoundsChirho
            | KoErrorChirho::UnsupportedRelocationChirho
            | KoErrorChirho::RelocationOverflowChirho => -ENOEXEC_CHIRHO,
            KoErrorChirho::NoInitSymbolChirho
            | KoErrorChirho::UnresolvedSymbolChirho => -EINVAL_CHIRHO,
            KoErrorChirho::OutOfMemoryChirho => -ENOMEM_CHIRHO,
        }
    }
}

// ===========================================================================
// A2-004: Kernel Symbol Export Table
// ===========================================================================

/// A single exported kernel symbol.
struct KernelSymbolEntryChirho {
    /// C-compatible name (e.g. "printk").
    name_chirho: &'static str,
    /// Virtual address of the kernel function / object.
    addr_chirho: u64,
}

/// Macro to export a kernel symbol so that `.ko` modules can resolve it.
///
/// Usage:
/// ```ignore
/// EXPORT_SYMBOL_CHIRHO!(serial_println_stub_chirho, "printk");
/// ```
///
/// This registers the Rust function `serial_println_stub_chirho` under the C
/// name `"printk"` in the dynamic kernel symbol table.
#[macro_export]
macro_rules! EXPORT_SYMBOL_CHIRHO {
    ($func_chirho:ident, $cname_chirho:expr) => {
        $crate::ko_loader_chirho::KernelSymbolTableChirho::register_symbol_chirho(
            alloc::string::String::from($cname_chirho),
            $func_chirho as u64,
        )
    };
}

/// Stub functions that kernel modules can call via the symbol table.  These are
/// thin wrappers around actual kernel routines, presented with C-compatible
/// signatures so that module code compiled by GCC / Clang can invoke them.

// ===========================================================================
// A2-006: C ABI shim — printk (kernel log ring buffer)
// ===========================================================================

/// Maximum number of entries in the kernel log ring buffer.
const KLOG_RING_SIZE_CHIRHO: usize = 256;

/// Maximum length of a single kernel log message.
const KLOG_MSG_MAX_LEN_CHIRHO: usize = 256;

/// Single entry in the kernel log ring buffer.
#[derive(Clone)]
pub struct KlogEntryChirho {
    /// Log level (0-7, matching Linux KERN_EMERG..KERN_DEBUG).
    pub level_chirho: u8,
    /// Monotonic timestamp (tick counter at log time).
    pub timestamp_chirho: u64,
    /// The log message bytes (UTF-8, NUL-terminated C string origin).
    pub message_chirho: [u8; KLOG_MSG_MAX_LEN_CHIRHO],
    /// Actual length of the message (excluding NUL).
    pub len_chirho: usize,
}

impl KlogEntryChirho {
    /// Create a zeroed log entry.
    const fn zeroed_chirho() -> Self {
        Self {
            level_chirho: 7, // KERN_DEBUG
            timestamp_chirho: 0,
            message_chirho: [0u8; KLOG_MSG_MAX_LEN_CHIRHO],
            len_chirho: 0,
        }
    }
}

/// Kernel log ring buffer — stores the last `KLOG_RING_SIZE_CHIRHO` messages.
pub struct KlogRingChirho {
    /// Ring buffer storage.
    entries_chirho: [KlogEntryChirho; KLOG_RING_SIZE_CHIRHO],
    /// Write index (wraps around).
    write_idx_chirho: usize,
    /// Total number of messages logged (never wraps).
    total_count_chirho: u64,
}

impl KlogRingChirho {
    /// Create a new empty ring buffer.
    const fn new_chirho() -> Self {
        const EMPTY_ENTRY_CHIRHO: KlogEntryChirho = KlogEntryChirho::zeroed_chirho();
        Self {
            entries_chirho: [EMPTY_ENTRY_CHIRHO; KLOG_RING_SIZE_CHIRHO],
            write_idx_chirho: 0,
            total_count_chirho: 0,
        }
    }

    /// Append a log message to the ring buffer.
    fn log_chirho(&mut self, level_chirho: u8, msg_chirho: &[u8]) {
        let entry_chirho = &mut self.entries_chirho[self.write_idx_chirho];
        entry_chirho.level_chirho = level_chirho;
        entry_chirho.timestamp_chirho = self.total_count_chirho;
        let copy_len_chirho = core::cmp::min(msg_chirho.len(), KLOG_MSG_MAX_LEN_CHIRHO);
        entry_chirho.message_chirho[..copy_len_chirho]
            .copy_from_slice(&msg_chirho[..copy_len_chirho]);
        entry_chirho.len_chirho = copy_len_chirho;
        self.write_idx_chirho = (self.write_idx_chirho + 1) % KLOG_RING_SIZE_CHIRHO;
        self.total_count_chirho += 1;
    }

    /// Return the total number of messages ever logged.
    pub fn total_count_chirho(&self) -> u64 {
        self.total_count_chirho
    }

    /// Iterate over the most recent messages (oldest to newest).
    pub fn recent_entries_chirho(&self) -> impl Iterator<Item = &KlogEntryChirho> {
        let count_chirho = core::cmp::min(
            self.total_count_chirho as usize,
            KLOG_RING_SIZE_CHIRHO,
        );
        let start_chirho = if self.total_count_chirho as usize >= KLOG_RING_SIZE_CHIRHO {
            self.write_idx_chirho
        } else {
            0
        };
        (0..count_chirho).map(move |i_chirho| {
            &self.entries_chirho[(start_chirho + i_chirho) % KLOG_RING_SIZE_CHIRHO]
        })
    }
}

/// Global kernel log ring buffer, protected by a spin mutex.
pub static KLOG_RING_CHIRHO: Mutex<KlogRingChirho> =
    Mutex::new(KlogRingChirho::new_chirho());

/// Parse a Linux-style log level prefix like `<N>` from the start of a message.
/// Returns `(level, offset_past_prefix)`.  If no prefix is found, returns
/// `(KERN_DEFAULT=4, 0)`.
fn parse_klog_level_chirho(msg_chirho: &[u8]) -> (u8, usize) {
    if msg_chirho.len() >= 3
        && msg_chirho[0] == b'<'
        && msg_chirho[2] == b'>'
        && msg_chirho[1].is_ascii_digit()
    {
        (msg_chirho[1] - b'0', 3)
    } else {
        (4, 0) // KERN_WARNING default
    }
}

/// `printk` C ABI shim — writes a NUL-terminated string to the serial console
/// and the kernel log ring buffer.
///
/// Supports Linux log-level prefixes: `<0>` through `<7>`.
/// This is the A2-006 implementation: C module calling printk produces kernel
/// log output that is stored in the ring buffer and forwarded to serial.
///
/// # Safety
///
/// `msg_ptr_chirho` must point to a valid, NUL-terminated C string in kernel
/// address space.
#[no_mangle]
pub unsafe extern "C" fn printk_stub_chirho(msg_ptr_chirho: *const u8) -> i32 {
    if msg_ptr_chirho.is_null() {
        return -1;
    }
    // Walk the string to find length.
    let mut len_chirho: usize = 0;
    unsafe {
        while *msg_ptr_chirho.add(len_chirho) != 0 {
            len_chirho += 1;
            if len_chirho > 4096 {
                break; // safety cap
            }
        }
        let slice_chirho = core::slice::from_raw_parts(msg_ptr_chirho, len_chirho);

        // Parse optional log-level prefix.
        let (level_chirho, offset_chirho) = parse_klog_level_chirho(slice_chirho);
        let body_chirho = &slice_chirho[offset_chirho..];

        // Write to kernel log ring buffer.
        {
            let mut ring_chirho = KLOG_RING_CHIRHO.lock();
            ring_chirho.log_chirho(level_chirho, body_chirho);
        }

        // Forward to serial console.
        if let Ok(s_chirho) = core::str::from_utf8(body_chirho) {
            crate::serial_println_chirho!("[printk/{}] {}", level_chirho, s_chirho);
        }
    }
    0
}

/// `vprintk` C ABI shim — same as printk but can be used as a redirect for
/// variadic printk calls where va_list is pre-formatted.
#[no_mangle]
pub unsafe extern "C" fn vprintk_stub_chirho(msg_ptr_chirho: *const u8) -> i32 {
    unsafe { printk_stub_chirho(msg_ptr_chirho) }
}

/// `printk_emit` C ABI shim — emits to ring buffer with explicit facility/level.
#[no_mangle]
#[allow(dead_code)]
pub unsafe extern "C" fn printk_emit_stub_chirho(
    _facility_chirho: i32,
    level_chirho: i32,
    msg_ptr_chirho: *const u8,
    _len_chirho: usize,
) -> i32 {
    if msg_ptr_chirho.is_null() {
        return -1;
    }
    let mut msg_len_chirho: usize = 0;
    unsafe {
        while *msg_ptr_chirho.add(msg_len_chirho) != 0 {
            msg_len_chirho += 1;
            if msg_len_chirho > 4096 {
                break;
            }
        }
        let slice_chirho = core::slice::from_raw_parts(msg_ptr_chirho, msg_len_chirho);
        let lvl_chirho = core::cmp::min(level_chirho as u8, 7);
        {
            let mut ring_chirho = KLOG_RING_CHIRHO.lock();
            ring_chirho.log_chirho(lvl_chirho, slice_chirho);
        }
        if let Ok(s_chirho) = core::str::from_utf8(slice_chirho) {
            crate::serial_println_chirho!("[printk/{}] {}", lvl_chirho, s_chirho);
        }
    }
    0
}

// ===========================================================================
// A2-007: C ABI shim — kmalloc / kfree
// ===========================================================================

/// GFP flags (Linux kernel memory allocation flags).
/// We accept them but ignore the specifics — all allocations come from the
/// kernel heap.
#[allow(dead_code)]
pub const GFP_KERNEL_CHIRHO: u32 = 0xCC0;
#[allow(dead_code)]
pub const GFP_ATOMIC_CHIRHO: u32 = 0xA20;

/// Internal tracking for kmalloc allocations so kfree does not need the size.
///
/// We store (ptr, layout) pairs. This is a simple approach; a real kernel
/// would use slab caches.
static KMALLOC_TRACKER_CHIRHO: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Fake `gendisk` allocation size for Linux block-device module stubs.
///
/// The loop module writes a fair amount of state into the returned `gendisk`
/// before `add_disk()`. Give it more than one page so offset mismatches do not
/// immediately walk off the end of the fake object.
const FAKE_GENDISK_BYTES_CHIRHO: usize = 16 * 1024;

/// Fake `request_queue` allocation size for loop.ko block-layer stubs.
const FAKE_REQUEST_QUEUE_BYTES_CHIRHO: usize = 4096;

/// Number of `u64` slots at the front of the fake `gendisk` that mirror the
/// fake request-queue pointer. Linux 6.x `struct gendisk` is not laid out near
/// the start of the object, so a tiny handful of offsets is too optimistic.
const FAKE_GENDISK_QUEUE_MIRROR_SLOTS_CHIRHO: usize = 64;

/// Number of `u64` slots at the front of the fake request-queue that mirror
/// `queuedata`. This helps simple `queue->queuedata` dereferences survive.
const FAKE_REQUEST_QUEUE_QUEUEDATA_SLOTS_CHIRHO: usize = 16;

/// Number of `u64` slots after the queuedata area that mirror the owning disk
/// pointer. This gives module code a plausible `queue->disk` style backlink.
const FAKE_REQUEST_QUEUE_DISK_MIRROR_SLOTS_CHIRHO: usize = 16;

/// Maximum C string length we will read from module-provided pointers.
const KO_C_STRING_MAX_BYTES_CHIRHO: usize = 128;

/// Number of bytes to scan when trying to discover a `gendisk.disk_name`.
const GENDISK_NAME_SCAN_BYTES_CHIRHO: usize = 512;

/// Longest disk name we will log from a fake `gendisk`.
const GENDISK_NAME_MAX_BYTES_CHIRHO: usize = 32;

/// Minimal Linux `miscdevice` prefix used by `misc_register`.
#[repr(C)]
struct MiscDeviceStubLayoutChirho {
    minor_chirho: i32,
    _padding_chirho: i32,
    name_ptr_chirho: *const u8,
    fops_ptr_chirho: *const u8,
}

/// Read a best-effort NUL-terminated kernel C string.
unsafe fn read_kernel_c_string_chirho(
    ptr_chirho: *const u8,
    max_len_chirho: usize,
) -> Option<String> {
    if ptr_chirho.is_null() || max_len_chirho == 0 {
        return None;
    }

    let mut len_chirho = 0usize;
    while len_chirho < max_len_chirho {
        let byte_chirho = unsafe { *ptr_chirho.add(len_chirho) };
        if byte_chirho == 0 {
            break;
        }
        len_chirho += 1;
    }

    let bytes_chirho = unsafe { core::slice::from_raw_parts(ptr_chirho, len_chirho) };
    Some(String::from_utf8_lossy(bytes_chirho).into_owned())
}

/// Scan a fake `gendisk` allocation for a plausible disk name string.
unsafe fn sniff_gendisk_name_chirho(disk_ptr_chirho: *const u8) -> Option<String> {
    if disk_ptr_chirho.is_null() {
        return None;
    }

    let bytes_chirho = unsafe {
        core::slice::from_raw_parts(disk_ptr_chirho, GENDISK_NAME_SCAN_BYTES_CHIRHO)
    };

    let mut offset_chirho = 0usize;
    while offset_chirho < bytes_chirho.len() {
        let first_byte_chirho = bytes_chirho[offset_chirho];
        if !(first_byte_chirho.is_ascii_alphanumeric() || first_byte_chirho == b'_' || first_byte_chirho == b'-') {
            offset_chirho += 1;
            continue;
        }

        let mut end_chirho = offset_chirho;
        while end_chirho < bytes_chirho.len() && end_chirho - offset_chirho < GENDISK_NAME_MAX_BYTES_CHIRHO {
            let byte_chirho = bytes_chirho[end_chirho];
            if byte_chirho == 0 {
                break;
            }
            if !(byte_chirho.is_ascii_alphanumeric() || byte_chirho == b'_' || byte_chirho == b'-') {
                break;
            }
            end_chirho += 1;
        }

        if end_chirho > offset_chirho {
            let candidate_bytes_chirho = &bytes_chirho[offset_chirho..end_chirho];
            let candidate_chirho = String::from_utf8_lossy(candidate_bytes_chirho).into_owned();
            if candidate_chirho.starts_with("loop") || candidate_chirho.starts_with("sd") || candidate_chirho.starts_with("vd") {
                return Some(candidate_chirho);
            }
        }

        offset_chirho = end_chirho.saturating_add(1);
    }

    None
}

/// `kmalloc` C ABI shim — allocate `size_chirho` bytes from the kernel heap.
///
/// Matches the Linux signature: `void *kmalloc(size_t size, gfp_t flags)`.
/// Returns a pointer to allocated memory, or null on failure.
///
/// The allocation is tracked internally so that `kfree` can free it without
/// needing the size.
///
/// # Safety
///
/// Caller must ensure the returned pointer is eventually freed with
/// `kfree_stub_chirho`.
#[no_mangle]
pub unsafe extern "C" fn kmalloc_stub_chirho(
    size_chirho: usize,
    _flags_chirho: u32,
) -> *mut u8 {
    use alloc::alloc::{alloc, Layout};
    if size_chirho == 0 {
        return core::ptr::null_mut();
    }
    // Round up to power-of-two alignment (min 8).
    let align_chirho = 8usize;
    let layout_chirho = match Layout::from_size_align(size_chirho, align_chirho) {
        Ok(l_chirho) => l_chirho,
        Err(_) => return core::ptr::null_mut(),
    };
    let ptr_chirho = unsafe { alloc(layout_chirho) };
    if !ptr_chirho.is_null() {
        let mut tracker_chirho = KMALLOC_TRACKER_CHIRHO.lock();
        tracker_chirho.push((ptr_chirho as usize, size_chirho));
    }
    ptr_chirho
}

/// `kzalloc` C ABI shim — like kmalloc but zeroes the memory.
#[no_mangle]
pub unsafe extern "C" fn kzalloc_stub_chirho(
    size_chirho: usize,
    flags_chirho: u32,
) -> *mut u8 {
    let ptr_chirho = unsafe { kmalloc_stub_chirho(size_chirho, flags_chirho) };
    if !ptr_chirho.is_null() && size_chirho > 0 {
        unsafe {
            core::ptr::write_bytes(ptr_chirho, 0, size_chirho);
        }
    }
    ptr_chirho
}

/// Linux internal kmalloc entry point used by modern modules.
#[allow(dead_code)]
pub unsafe extern "C" fn kmalloc_noprof_stub_chirho(
    size_chirho: usize,
    flags_chirho: u32,
) -> *mut u8 {
    unsafe { kmalloc_stub_chirho(size_chirho, flags_chirho) }
}

/// Linux internal cache-based kmalloc entry point used by modern modules.
#[allow(dead_code)]
pub unsafe extern "C" fn kmalloc_cache_noprof_stub_chirho(
    _cache_ptr_chirho: *mut u8,
    flags_chirho: u32,
    size_chirho: usize,
) -> *mut u8 {
    unsafe { kmalloc_stub_chirho(size_chirho, flags_chirho) }
}

/// `krealloc` C ABI shim — resize a previous kmalloc allocation.
#[no_mangle]
pub unsafe extern "C" fn krealloc_stub_chirho(
    old_ptr_chirho: *mut u8,
    new_size_chirho: usize,
    flags_chirho: u32,
) -> *mut u8 {
    if old_ptr_chirho.is_null() {
        return unsafe { kmalloc_stub_chirho(new_size_chirho, flags_chirho) };
    }
    if new_size_chirho == 0 {
        unsafe { kfree_stub_chirho(old_ptr_chirho) };
        return core::ptr::null_mut();
    }

    // Find old size from tracker.
    let old_size_chirho = {
        let tracker_chirho = KMALLOC_TRACKER_CHIRHO.lock();
        tracker_chirho
            .iter()
            .find(|(addr_chirho, _)| *addr_chirho == old_ptr_chirho as usize)
            .map(|(_, sz_chirho)| *sz_chirho)
            .unwrap_or(0)
    };

    let new_ptr_chirho = unsafe { kmalloc_stub_chirho(new_size_chirho, flags_chirho) };
    if !new_ptr_chirho.is_null() && old_size_chirho > 0 {
        let copy_len_chirho = core::cmp::min(old_size_chirho, new_size_chirho);
        unsafe {
            core::ptr::copy_nonoverlapping(old_ptr_chirho, new_ptr_chirho, copy_len_chirho);
        }
        unsafe { kfree_stub_chirho(old_ptr_chirho) };
    }
    new_ptr_chirho
}

/// `kfree` C ABI shim — free memory previously allocated by `kmalloc_stub_chirho`.
///
/// Matches the Linux signature: `void kfree(const void *ptr)`.
/// Looks up the allocation size from the internal tracker.
///
/// # Safety
///
/// `ptr_chirho` must have been returned by `kmalloc_stub_chirho`.
/// Double-free is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn kfree_stub_chirho(ptr_chirho: *mut u8) {
    use alloc::alloc::{dealloc, Layout};
    if ptr_chirho.is_null() {
        return;
    }

    // Find and remove from tracker.
    let size_chirho = {
        let mut tracker_chirho = KMALLOC_TRACKER_CHIRHO.lock();
        let pos_chirho = tracker_chirho
            .iter()
            .position(|(addr_chirho, _)| *addr_chirho == ptr_chirho as usize);
        match pos_chirho {
            Some(idx_chirho) => {
                let (_, sz_chirho) = tracker_chirho.remove(idx_chirho);
                sz_chirho
            }
            None => {
                crate::serial_println_chirho!(
                    "[KO] kfree: unknown pointer {:#x}, ignoring",
                    ptr_chirho as usize
                );
                return;
            }
        }
    };

    let layout_chirho = match Layout::from_size_align(size_chirho, 8) {
        Ok(l_chirho) => l_chirho,
        Err(_) => return,
    };
    unsafe { dealloc(ptr_chirho, layout_chirho) };
}

/// `ksize` C ABI shim — return the usable size of a kmalloc allocation.
#[no_mangle]
#[allow(dead_code)]
pub unsafe extern "C" fn ksize_stub_chirho(ptr_chirho: *const u8) -> usize {
    if ptr_chirho.is_null() {
        return 0;
    }
    let tracker_chirho = KMALLOC_TRACKER_CHIRHO.lock();
    tracker_chirho
        .iter()
        .find(|(addr_chirho, _)| *addr_chirho == ptr_chirho as usize)
        .map(|(_, sz_chirho)| *sz_chirho)
        .unwrap_or(0)
}

/// `schedule` stub — yield the current task.
///
/// In the current kernel this is a no-op placeholder; a future scheduler
/// integration will make it preempt properly.
#[allow(dead_code)]
pub extern "C" fn schedule_stub_chirho() {
    // Intentional no-op for now.
}

/// `mutex_lock` stub — placeholder for kernel mutex acquisition.
#[allow(dead_code)]
pub extern "C" fn mutex_lock_stub_chirho(_lock_ptr_chirho: u64) {
    // No-op: spinlocks are used internally; this satisfies the ABI.
}

/// `mutex_unlock` stub — placeholder for kernel mutex release.
#[allow(dead_code)]
pub extern "C" fn mutex_unlock_stub_chirho(_lock_ptr_chirho: u64) {
    // No-op placeholder.
}

/// `register_chrdev` stub — placeholder for character device registration.
///
/// Returns 0 (success) unconditionally; actual device registration will be
/// wired up in a later phase.
#[allow(dead_code)]
pub extern "C" fn register_chrdev_stub_chirho(
    _major_chirho: u32,
    _name_ptr_chirho: u64,
    _fops_ptr_chirho: u64,
) -> i32 {
    crate::serial_println_chirho!("[KO] register_chrdev stub called");
    0
}

/// `unregister_chrdev` stub.
#[allow(dead_code)]
pub extern "C" fn unregister_chrdev_stub_chirho(_major_chirho: u32, _name_ptr_chirho: u64) {
    crate::serial_println_chirho!("[KO] unregister_chrdev stub called");
}

// ===========================================================================
// A2-011: C ABI shim — copy_to_user / copy_from_user
// ===========================================================================

/// `copy_to_user(to, from, n)` — C-callable wrapper over
/// [`crate::uaccess_chirho::copy_to_user_chirho`].
///
/// Returns the number of bytes that could **not** be copied (0 on success,
/// `n` on failure), matching the Linux kernel convention.
#[allow(dead_code)]
pub unsafe extern "C" fn copy_to_user_stub_chirho(
    to_chirho: u64,
    from_chirho: *const u8,
    n_chirho: usize,
) -> usize {
    if from_chirho.is_null() || n_chirho == 0 {
        return n_chirho;
    }
    let src_slice_chirho = unsafe { core::slice::from_raw_parts(from_chirho, n_chirho) };
    match uaccess_chirho::copy_to_user_chirho(to_chirho, src_slice_chirho, n_chirho) {
        Ok(()) => 0,
        Err(_) => n_chirho,
    }
}

/// `copy_from_user(to, from, n)` — C-callable wrapper over
/// [`crate::uaccess_chirho::copy_from_user_chirho`].
///
/// Returns the number of bytes that could **not** be copied (0 on success).
#[allow(dead_code)]
pub unsafe extern "C" fn copy_from_user_stub_chirho(
    to_chirho: *mut u8,
    from_chirho: u64,
    n_chirho: usize,
) -> usize {
    if to_chirho.is_null() || n_chirho == 0 {
        return n_chirho;
    }
    let dst_slice_chirho = unsafe { core::slice::from_raw_parts_mut(to_chirho, n_chirho) };
    match uaccess_chirho::copy_from_user_chirho(dst_slice_chirho, from_chirho, n_chirho) {
        Ok(()) => 0,
        Err(_) => n_chirho,
    }
}

/// `put_user(x, ptr)` emulation — write a u64 to user space.
/// Returns 0 on success, -EFAULT on failure.
#[allow(dead_code)]
pub unsafe extern "C" fn put_user_stub_chirho(
    val_chirho: u64,
    user_ptr_chirho: u64,
) -> i64 {
    match uaccess_chirho::write_user_u64_chirho(user_ptr_chirho, val_chirho) {
        Ok(()) => 0,
        Err(_) => -14, // EFAULT
    }
}

/// `get_user(ptr)` emulation — read a u64 from user space.
/// Returns the value on success, 0 on failure (Linux convention is via pointer).
#[allow(dead_code)]
pub unsafe extern "C" fn get_user_stub_chirho(
    user_ptr_chirho: u64,
) -> u64 {
    match uaccess_chirho::read_user_u64_chirho(user_ptr_chirho) {
        Ok(val_chirho) => val_chirho,
        Err(_) => 0,
    }
}

// ===========================================================================
// A2-010: C ABI shim — pci_register_driver and PCI helpers
// ===========================================================================

/// `__pci_register_driver(drv, owner, mod_name)` — register a PCI driver.
///
/// In a full kernel this would scan the PCI bus and call probe() for matching
/// devices. Here we log the registration and return 0 (success).
#[allow(dead_code)]
pub extern "C" fn pci_register_driver_stub_chirho(
    _drv_ptr_chirho: u64,
    _owner_ptr_chirho: u64,
    _mod_name_ptr_chirho: u64,
) -> i32 {
    crate::serial_println_chirho!("[KO] __pci_register_driver stub called");
    0 // success
}

/// `pci_unregister_driver(drv)` — unregister a PCI driver.
#[allow(dead_code)]
pub extern "C" fn pci_unregister_driver_stub_chirho(_drv_ptr_chirho: u64) {
    crate::serial_println_chirho!("[KO] pci_unregister_driver stub called");
}

/// `pci_enable_device(dev)` — enable a PCI device.
#[allow(dead_code)]
pub extern "C" fn pci_enable_device_stub_chirho(_dev_ptr_chirho: u64) -> i32 {
    crate::serial_println_chirho!("[KO] pci_enable_device stub called");
    0
}

/// `pci_disable_device(dev)` — disable a PCI device.
#[allow(dead_code)]
pub extern "C" fn pci_disable_device_stub_chirho(_dev_ptr_chirho: u64) {
    crate::serial_println_chirho!("[KO] pci_disable_device stub called");
}

/// `pci_set_master(dev)` — enable bus mastering for a PCI device.
#[allow(dead_code)]
pub extern "C" fn pci_set_master_stub_chirho(_dev_ptr_chirho: u64) {
    crate::serial_println_chirho!("[KO] pci_set_master stub called");
}

/// `pci_resource_start(dev, bar)` — get BAR start address.
#[allow(dead_code)]
pub extern "C" fn pci_resource_start_stub_chirho(_dev_ptr_chirho: u64, _bar_chirho: u32) -> u64 {
    0 // no real BAR
}

/// `pci_resource_len(dev, bar)` — get BAR length.
#[allow(dead_code)]
pub extern "C" fn pci_resource_len_stub_chirho(_dev_ptr_chirho: u64, _bar_chirho: u32) -> u64 {
    0
}

// ===========================================================================
// A2-012: C ABI shim — spinlock (raw_spin_lock / raw_spin_unlock)
// ===========================================================================

/// `_raw_spin_lock(lock)` — acquire a raw spinlock.
///
/// In our single-CPU kernel this disables interrupts (conceptually) and
/// returns. In future SMP builds this will do a real CAS loop.
#[allow(dead_code)]
pub extern "C" fn spin_lock_stub_chirho(_lock_ptr_chirho: u64) {
    // Single-CPU: disable preemption (no-op for cooperative scheduler).
}

/// `_raw_spin_unlock(lock)` — release a raw spinlock.
#[allow(dead_code)]
pub extern "C" fn spin_unlock_stub_chirho(_lock_ptr_chirho: u64) {
    // Single-CPU: re-enable preemption.
}

/// `_raw_spin_lock_irqsave(lock)` — acquire spinlock and save IRQ flags.
/// Returns the previous interrupt state (flags).
#[allow(dead_code)]
pub extern "C" fn spin_lock_irqsave_stub_chirho(_lock_ptr_chirho: u64) -> u64 {
    // Return a dummy flags value; in future we'd return RFLAGS.
    let flags_chirho: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) flags_chirho);
    }
    // Disable interrupts.
    unsafe { core::arch::asm!("cli"); }
    flags_chirho
}

/// `_raw_spin_unlock_irqrestore(lock, flags)` — release spinlock and restore IRQ flags.
#[allow(dead_code)]
pub extern "C" fn spin_unlock_irqrestore_stub_chirho(_lock_ptr_chirho: u64, flags_chirho: u64) {
    // Restore interrupt state.
    if flags_chirho & 0x200 != 0 {
        // IF was set — re-enable interrupts.
        unsafe { core::arch::asm!("sti"); }
    }
}

/// `mutex_init(mutex)` — initialize a mutex structure.
#[allow(dead_code)]
pub extern "C" fn mutex_init_stub_chirho(_mutex_ptr_chirho: u64) {
    // No-op: mutexes are conceptually always unlocked at init.
}

// ===========================================================================
// A2-013: C ABI shim — workqueue (schedule_work / create_workqueue)
// ===========================================================================

/// `schedule_work(work)` — schedule deferred work on the system workqueue.
///
/// In a full kernel this queues a `work_struct` for execution by a kernel
/// worker thread. Here we call the work function immediately (synchronous).
#[allow(dead_code)]
pub extern "C" fn schedule_work_stub_chirho(work_ptr_chirho: u64) -> i32 {
    crate::serial_println_chirho!("[KO] schedule_work stub: work_struct at {:#x}", work_ptr_chirho);
    // A real implementation would queue the work. For now we invoke it inline
    // if we can find the function pointer. The `work_struct` layout has the
    // function pointer at offset 32 on x86_64 Linux. Since we don't have
    // the exact struct, we just log it.
    1 // return 1 = work was queued
}

/// `create_singlethread_workqueue(name)` — create a new workqueue.
///
/// Returns a non-NULL "handle" (just a unique ID for now).
#[allow(dead_code)]
pub extern "C" fn create_workqueue_stub_chirho(_name_ptr_chirho: u64) -> u64 {
    static NEXT_WQ_ID_CHIRHO: core::sync::atomic::AtomicU64 =
        core::sync::atomic::AtomicU64::new(0xAA00_0001);
    crate::serial_println_chirho!("[KO] create_workqueue stub called");
    NEXT_WQ_ID_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

/// `alloc_workqueue(name, flags, max_active, ...)` — variadic Linux helper.
///
/// We only care about returning a stable non-NULL handle so modules can keep
/// their workqueue state alive. The format/flags/max_active arguments are
/// ignored for now.
#[allow(dead_code)]
pub extern "C" fn alloc_workqueue_stub_chirho(
    name_ptr_chirho: u64,
    _flags_chirho: u32,
    _max_active_chirho: i32,
) -> u64 {
    create_workqueue_stub_chirho(name_ptr_chirho)
}

/// `destroy_workqueue(wq)` — destroy a workqueue.
#[allow(dead_code)]
pub extern "C" fn destroy_workqueue_stub_chirho(_wq_handle_chirho: u64) {
    crate::serial_println_chirho!("[KO] destroy_workqueue stub called");
}

/// `queue_work(wq, work)` — queue work on a specific workqueue.
#[allow(dead_code)]
pub extern "C" fn queue_work_stub_chirho(_wq_handle_chirho: u64, _work_ptr_chirho: u64) -> i32 {
    crate::serial_println_chirho!("[KO] queue_work stub called");
    1
}

/// `queue_work_on(cpu, wq, work)` — Linux helper variant that pins work to a CPU.
#[allow(dead_code)]
pub extern "C" fn queue_work_on_stub_chirho(
    _cpu_chirho: i32,
    wq_handle_chirho: u64,
    work_ptr_chirho: u64,
) -> i32 {
    queue_work_stub_chirho(wq_handle_chirho, work_ptr_chirho)
}

/// `flush_workqueue(wq)` — wait for all pending work to complete.
#[allow(dead_code)]
pub extern "C" fn flush_workqueue_stub_chirho(_wq_handle_chirho: u64) {
    crate::serial_println_chirho!("[KO] flush_workqueue stub called");
    // All work is synchronous in our stub, so nothing to flush.
}

/// `misc_register(misc)` — register a Linux miscdevice.
///
/// For `loop.ko` this is enough to make `/dev/loop-control` registration look
/// successful because the node already exists in devtmpfs.
pub unsafe extern "C" fn misc_register_stub_chirho(
    misc_ptr_chirho: *const MiscDeviceStubLayoutChirho,
) -> i32 {
    if misc_ptr_chirho.is_null() {
        crate::serial_println_chirho!("[KO] misc_register: null miscdevice pointer");
        return -(EINVAL_CHIRHO as i32);
    }

    let misc_ref_chirho = unsafe { &*misc_ptr_chirho };
    let name_chirho = unsafe {
        read_kernel_c_string_chirho(
            misc_ref_chirho.name_ptr_chirho,
            KO_C_STRING_MAX_BYTES_CHIRHO,
        )
    }
    .unwrap_or_else(|| String::from("<null>"));

    crate::serial_println_chirho!(
        "[KO] misc_register: minor={} name={}",
        misc_ref_chirho.minor_chirho,
        name_chirho
    );
    0
}

/// `kstrtoint(str, base, out)` — parse a kernel-space ASCII integer.
pub unsafe extern "C" fn kstrtoint_stub_chirho(
    string_ptr_chirho: *const u8,
    base_chirho: u32,
    out_ptr_chirho: *mut i32,
) -> i32 {
    if string_ptr_chirho.is_null() || out_ptr_chirho.is_null() {
        return -(EINVAL_CHIRHO as i32);
    }

    let input_chirho = unsafe {
        read_kernel_c_string_chirho(string_ptr_chirho, KO_C_STRING_MAX_BYTES_CHIRHO)
    }
    .unwrap_or_default();
    let trimmed_chirho = input_chirho.trim();
    let radix_chirho = if base_chirho == 0 { 10 } else { base_chirho };
    let negative_chirho = trimmed_chirho.starts_with('-');
    let digits_chirho = trimmed_chirho
        .strip_prefix('-')
        .or_else(|| trimmed_chirho.strip_prefix('+'))
        .unwrap_or(trimmed_chirho);

    let parsed_chirho = match i64::from_str_radix(digits_chirho, radix_chirho) {
        Ok(value_chirho) => {
            if negative_chirho {
                -value_chirho
            } else {
                value_chirho
            }
        }
        Err(_) => return -(EINVAL_CHIRHO as i32),
    };

    if parsed_chirho < i32::MIN as i64 || parsed_chirho > i32::MAX as i64 {
        return -(ERANGE_CHIRHO as i32);
    }

    unsafe {
        *out_ptr_chirho = parsed_chirho as i32;
    }
    0
}

/// `__register_blkdev(major, name)` — reserve a block-device major number.
pub unsafe extern "C" fn register_blkdev_stub_chirho(
    major_chirho: u32,
    name_ptr_chirho: *const u8,
    probe_ptr_chirho: *const u8,
) -> i32 {
    let name_chirho = unsafe {
        read_kernel_c_string_chirho(name_ptr_chirho, KO_C_STRING_MAX_BYTES_CHIRHO)
    }
    .unwrap_or_else(|| String::from("<null>"));

    let result_chirho = if major_chirho == 0 {
        crate::loop_device_chirho::LOOP_MAJOR_CHIRHO as i32
    } else {
        0
    };

    crate::serial_println_chirho!(
        "[KO] __register_blkdev: requested_major={} name={} probe={:#x} -> {}",
        major_chirho,
        name_chirho,
        probe_ptr_chirho as usize,
        result_chirho
    );
    result_chirho
}

unsafe fn allocate_fake_gendisk_chirho(
    queuedata_ptr_chirho: u64,
) -> *mut u8 {
    let disk_ptr_chirho =
        unsafe { kzalloc_stub_chirho(FAKE_GENDISK_BYTES_CHIRHO, GFP_KERNEL_CHIRHO) };
    let queue_ptr_chirho =
        unsafe { kzalloc_stub_chirho(FAKE_REQUEST_QUEUE_BYTES_CHIRHO, GFP_KERNEL_CHIRHO) };

    if !disk_ptr_chirho.is_null() && !queue_ptr_chirho.is_null() {
        let disk_u64_chirho = disk_ptr_chirho as *mut u64;
        // Mirror the queue pointer across the early fake gendisk words so
        // direct field reads like disk->queue survive even if our fake layout
        // does not match Linux exactly.
        for offset_chirho in 1..=FAKE_GENDISK_QUEUE_MIRROR_SLOTS_CHIRHO {
            unsafe {
                core::ptr::write(disk_u64_chirho.add(offset_chirho), queue_ptr_chirho as u64);
            }
        }

        let queue_u64_chirho = queue_ptr_chirho as *mut u64;
        for offset_chirho in 0..FAKE_REQUEST_QUEUE_QUEUEDATA_SLOTS_CHIRHO {
            unsafe {
                // Make the early queue words point back at the loop_device so
                // queue->queuedata style dereferences see something non-NULL.
                core::ptr::write(queue_u64_chirho.add(offset_chirho), queuedata_ptr_chirho);
            }
        }
        for offset_chirho in FAKE_REQUEST_QUEUE_QUEUEDATA_SLOTS_CHIRHO
            ..(FAKE_REQUEST_QUEUE_QUEUEDATA_SLOTS_CHIRHO + FAKE_REQUEST_QUEUE_DISK_MIRROR_SLOTS_CHIRHO)
        {
            unsafe {
                core::ptr::write(queue_u64_chirho.add(offset_chirho), disk_ptr_chirho as u64);
            }
        }
    }

    crate::serial_println_chirho!(
        "[KO] fake gendisk allocated: disk={:#x} queue={:#x} queuedata={:#x}",
        disk_ptr_chirho as usize,
        queue_ptr_chirho as usize,
        queuedata_ptr_chirho,
    );
    disk_ptr_chirho
}

/// `__blk_mq_alloc_disk(set, lkclass, queuedata)` — allocate a fake `gendisk`.
///
/// The returned gendisk must have a non-NULL `queue` pointer or loop.ko
/// will page fault when it tries to access `disk->queue->...`.
/// We allocate a fake request_queue and wire it into the gendisk.
pub unsafe extern "C" fn blk_mq_alloc_disk_stub_chirho(
    set_ptr_chirho: u64,
    lkclass_ptr_chirho: u64,
    queuedata_ptr_chirho: u64,
) -> *mut u8 {
    let disk_ptr_chirho = unsafe { allocate_fake_gendisk_chirho(queuedata_ptr_chirho) };

    crate::serial_println_chirho!(
        "[KO] __blk_mq_alloc_disk: set={:#x} lkclass={:#x} queuedata={:#x} -> disk={:#x}",
        set_ptr_chirho,
        lkclass_ptr_chirho,
        queuedata_ptr_chirho,
        disk_ptr_chirho as usize,
    );
    disk_ptr_chirho
}

/// `blk_mq_alloc_disk(set, lim, queuedata)` — newer helper used by loop.c.
pub unsafe extern "C" fn blk_mq_alloc_disk_alias_stub_chirho(
    set_ptr_chirho: u64,
    limits_ptr_chirho: u64,
    queuedata_ptr_chirho: u64,
) -> *mut u8 {
    let disk_ptr_chirho = unsafe { allocate_fake_gendisk_chirho(queuedata_ptr_chirho) };
    crate::serial_println_chirho!(
        "[KO] blk_mq_alloc_disk: set={:#x} limits={:#x} queuedata={:#x} -> disk={:#x}",
        set_ptr_chirho,
        limits_ptr_chirho,
        queuedata_ptr_chirho,
        disk_ptr_chirho as usize,
    );
    disk_ptr_chirho
}

/// `alloc_disk(minors)` — older block-disk allocator alias.
pub unsafe extern "C" fn alloc_disk_stub_chirho(
    minors_chirho: i32,
) -> *mut u8 {
    let disk_ptr_chirho = unsafe { allocate_fake_gendisk_chirho(0) };
    crate::serial_println_chirho!(
        "[KO] alloc_disk: minors={} -> disk={:#x}",
        minors_chirho,
        disk_ptr_chirho as usize,
    );
    disk_ptr_chirho
}

/// `device_add_disk(parent, disk, groups)` — publish a block disk device.
pub unsafe extern "C" fn device_add_disk_stub_chirho(
    parent_ptr_chirho: u64,
    disk_ptr_chirho: *const u8,
    groups_ptr_chirho: u64,
) -> i32 {
    let disk_name_chirho = unsafe { sniff_gendisk_name_chirho(disk_ptr_chirho) }
        .unwrap_or_else(|| String::from("<unknown>"));

    crate::serial_println_chirho!(
        "[KO] device_add_disk: parent={:#x} disk={:#x} groups={:#x} name={}",
        parent_ptr_chirho,
        disk_ptr_chirho as usize,
        groups_ptr_chirho,
        disk_name_chirho
    );
    0
}

/// `add_disk(disk)` — simple alias used by loop.c.
pub unsafe extern "C" fn add_disk_stub_chirho(
    disk_ptr_chirho: *const u8,
) -> i32 {
    let disk_name_chirho = unsafe { sniff_gendisk_name_chirho(disk_ptr_chirho) }
        .unwrap_or_else(|| String::from("<unknown>"));
    crate::serial_println_chirho!(
        "[KO] add_disk: disk={:#x} name={}",
        disk_ptr_chirho as usize,
        disk_name_chirho
    );
    0
}

/// `idr_alloc(idr, ptr, start, end, gfp)` — return a monotonically increasing ID.
pub extern "C" fn idr_alloc_stub_chirho(
    idr_ptr_chirho: u64,
    ptr_chirho: u64,
    start_chirho: i32,
    end_chirho: i32,
    _gfp_chirho: u32,
) -> i32 {
    static NEXT_IDR_ID_CHIRHO: core::sync::atomic::AtomicI32 =
        core::sync::atomic::AtomicI32::new(0);

    let id_chirho = NEXT_IDR_ID_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::serial_println_chirho!(
        "[KO] idr_alloc: idr={:#x} ptr={:#x} start={} end={} -> id={}",
        idr_ptr_chirho,
        ptr_chirho,
        start_chirho,
        end_chirho,
        id_chirho
    );
    id_chirho
}

// ===========================================================================
// A2-009: C ABI shim — request_irq / free_irq
// ===========================================================================

/// `request_irq(irq, handler, flags, name, dev)` — register an IRQ handler.
#[allow(dead_code)]
pub extern "C" fn request_irq_stub_chirho(
    irq_chirho: u32,
    _handler_ptr_chirho: u64,
    _flags_chirho: u64,
    _name_ptr_chirho: u64,
    _dev_ptr_chirho: u64,
) -> i32 {
    crate::serial_println_chirho!("[KO] request_irq stub: IRQ {}", irq_chirho);
    0 // success
}

/// `free_irq(irq, dev)` — unregister an IRQ handler.
#[allow(dead_code)]
pub extern "C" fn free_irq_stub_chirho(irq_chirho: u32, _dev_ptr_chirho: u64) {
    crate::serial_println_chirho!("[KO] free_irq stub: IRQ {}", irq_chirho);
}

// ===========================================================================
// A2-018: Rust native module API
// ===========================================================================

/// Trait for Rust kernel modules.
///
/// Provides a safe, idiomatic Rust alternative to the C ABI `init_module` /
/// `cleanup_module` convention. Modules implement this trait and register
/// via [`register_rust_module_chirho`].
pub trait RustModuleChirho: Send + Sync {
    /// Module name (displayed in `/proc/modules`).
    fn name_chirho(&self) -> &str;

    /// Called when the module is loaded. Return `Ok(())` on success or
    /// an error code on failure.
    fn init_chirho(&self) -> Result<(), i64>;

    /// Called when the module is unloaded. Should clean up all resources.
    fn exit_chirho(&self);
}

/// Register and initialize a Rust-native kernel module.
///
/// The module's `init_chirho` method is called immediately. If it succeeds
/// the module is added to the loaded modules list and will appear in
/// `/proc/modules`.
#[allow(dead_code)]
pub fn register_rust_module_chirho(module_chirho: &'static dyn RustModuleChirho) -> Result<(), i64> {
    crate::serial_println_chirho!(
        "[KO] Loading Rust module: {}",
        module_chirho.name_chirho()
    );

    module_chirho.init_chirho()?;

    // Register in the loaded modules list.
    let ko_chirho = KoModuleChirho {
        name_chirho: String::from(module_chirho.name_chirho()),
        init_fn_chirho: None,
        cleanup_fn_chirho: None,
        state_chirho: ModuleStateChirho::LoadedChirho,
        symbol_count_chirho: 0,
        sections_chirho: Vec::new(),
        module_mem_base_chirho: module_chirho as *const dyn RustModuleChirho as *const () as u64,
        module_mem_size_chirho: 0,
        module_mem_arena_slot_chirho: None,
        refcount_chirho: 0,
        depends_on_chirho: Vec::new(),
        params_chirho: Vec::new(),
    };

    let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();
    loaded_chirho.push(ko_chirho);

    crate::serial_println_chirho!(
        "[KO] Rust module '{}' loaded successfully",
        module_chirho.name_chirho()
    );

    Ok(())
}

// ===========================================================================
// A2-016: Module dependency tracking helpers
// ===========================================================================

/// Check whether a module can be safely unloaded (no dependents).
/// Returns `true` if the module's refcount is 0.
#[allow(dead_code)]
pub fn can_unload_module_chirho(name_chirho: &str) -> bool {
    let loaded_chirho = LOADED_MODULES_CHIRHO.lock();
    for module_chirho in loaded_chirho.iter() {
        if module_chirho.name_chirho == name_chirho {
            return module_chirho.refcount_chirho == 0;
        }
    }
    true // not found => no dependents
}

/// Increment refcount of a module by name (called when another module depends on it).
#[allow(dead_code)]
pub fn inc_module_refcount_chirho(name_chirho: &str) {
    let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();
    for module_chirho in loaded_chirho.iter_mut() {
        if module_chirho.name_chirho == name_chirho {
            module_chirho.refcount_chirho += 1;
            return;
        }
    }
}

/// Decrement refcount of a module by name (called when a dependent is unloaded).
#[allow(dead_code)]
pub fn dec_module_refcount_chirho(name_chirho: &str) {
    let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();
    for module_chirho in loaded_chirho.iter_mut() {
        if module_chirho.name_chirho == name_chirho {
            if module_chirho.refcount_chirho > 0 {
                module_chirho.refcount_chirho -= 1;
            }
            return;
        }
    }
}

// ===========================================================================
// A2-017: Module parameter parsing
// ===========================================================================

/// Parse module parameters from a "key1=val1 key2=val2" string.
///
/// Parameters are space-separated `key=value` pairs. Values may be quoted
/// (future extension). Returns a vector of [`ModuleParamChirho`].
#[allow(dead_code)]
pub fn parse_module_params_chirho(params_str_chirho: &str) -> Vec<ModuleParamChirho> {
    let mut result_chirho = Vec::new();
    for token_chirho in params_str_chirho.split_whitespace() {
        if let Some(eq_pos_chirho) = token_chirho.find('=') {
            let key_chirho = &token_chirho[..eq_pos_chirho];
            let value_chirho = &token_chirho[eq_pos_chirho + 1..];
            if !key_chirho.is_empty() {
                result_chirho.push(ModuleParamChirho {
                    key_chirho: String::from(key_chirho),
                    value_chirho: String::from(value_chirho),
                });
            }
        }
    }
    result_chirho
}

// ===========================================================================
// A2-015: /proc/modules content generator
// ===========================================================================

/// Generate the content for `/proc/modules`.
///
/// Format matches Linux: `name size refcount [dep1,dep2,] state address`
#[allow(dead_code)]
pub fn gen_proc_modules_chirho() -> String {
    use core::fmt::Write;
    let loaded_chirho = LOADED_MODULES_CHIRHO.lock();
    let mut output_chirho = String::new();
    for module_chirho in loaded_chirho.iter() {
        let state_str_chirho = match module_chirho.state_chirho {
            ModuleStateChirho::LoadedChirho => "Live",
            ModuleStateChirho::UnloadedChirho => "Unloaded",
        };
        let deps_str_chirho = if module_chirho.depends_on_chirho.is_empty() {
            String::from("-")
        } else {
            let mut d_chirho = String::new();
            for (i_chirho, dep_chirho) in module_chirho.depends_on_chirho.iter().enumerate() {
                if i_chirho > 0 {
                    d_chirho.push(',');
                }
                d_chirho.push_str(dep_chirho);
            }
            d_chirho.push(',');
            d_chirho
        };
        let _ = write!(
            output_chirho,
            "{} {} {} {} {} {:#x}\n",
            module_chirho.name_chirho,
            module_chirho.module_mem_size_chirho,
            module_chirho.refcount_chirho,
            deps_str_chirho,
            state_str_chirho,
            module_chirho.module_mem_base_chirho,
        );
    }
    output_chirho
}

/// Static table of built-in kernel symbol addresses.
///
/// Entries with `addr_chirho == 0` will be resolved lazily through the dynamic
/// table (populated by [`init_kernel_symbols_chirho`]).
static KERNEL_SYMBOLS_CHIRHO: &[KernelSymbolEntryChirho] = &[
    KernelSymbolEntryChirho {
        name_chirho: "printk",
        addr_chirho: 0, // Resolved at first lookup via lazy init
    },
    KernelSymbolEntryChirho {
        name_chirho: "kmalloc",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "kfree",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "schedule",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "mutex_lock",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "mutex_unlock",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "__register_chrdev",
        addr_chirho: 0,
    },
    KernelSymbolEntryChirho {
        name_chirho: "__unregister_chrdev",
        addr_chirho: 0,
    },
];

/// Kernel symbol table providing name -> address resolution.
pub struct KernelSymbolTableChirho;

impl KernelSymbolTableChirho {
    /// Look up a kernel symbol by its C name.
    ///
    /// Returns `Some(address)` if the symbol is found, `None` otherwise.
    /// Symbols with address 0 are treated as "not yet resolved" and return
    /// `None` — a future phase will populate real addresses at boot.
    pub fn lookup_symbol_chirho(name_chirho: &str) -> Option<u64> {
        let raw_addr_chirho = Self::lookup_raw_symbol_chirho(name_chirho)?;
        // If high-canonical module arena is active, create a thunk at
        // a high-canonical address that trampolines to the kernel stub.
        // This makes PLT32 relocations work (thunk is within ±2GB).
        if MODULE_ARENA_HIGH_READY_CHIRHO.load(core::sync::atomic::Ordering::Acquire) {
            return Some(get_or_create_thunk_chirho(raw_addr_chirho));
        }
        Some(raw_addr_chirho)
    }

    fn lookup_raw_symbol_chirho(name_chirho: &str) -> Option<u64> {
        for entry_chirho in KERNEL_SYMBOLS_CHIRHO.iter() {
            if entry_chirho.name_chirho == name_chirho {
                if entry_chirho.addr_chirho != 0 {
                    return Some(entry_chirho.addr_chirho);
                }
            }
        }

        let dynamic_chirho = DYNAMIC_SYMBOLS_CHIRHO.lock();
        for (sym_name_chirho, sym_addr_chirho) in dynamic_chirho.iter() {
            if sym_name_chirho.as_str() == name_chirho {
                return Some(*sym_addr_chirho);
            }
        }

        None
    }

    /// Register a kernel symbol at runtime (e.g. during subsystem init).
    pub fn register_symbol_chirho(name_chirho: String, addr_chirho: u64) {
        let mut dynamic_chirho = DYNAMIC_SYMBOLS_CHIRHO.lock();
        // Replace if already present.
        for entry_chirho in dynamic_chirho.iter_mut() {
            if entry_chirho.0 == name_chirho {
                entry_chirho.1 = addr_chirho;
                return;
            }
        }
        dynamic_chirho.push((name_chirho, addr_chirho));
    }

    /// Return the total number of exported symbols (static + dynamic).
    pub fn symbol_count_chirho() -> usize {
        let dynamic_chirho = DYNAMIC_SYMBOLS_CHIRHO.lock();
        KERNEL_SYMBOLS_CHIRHO.len() + dynamic_chirho.len()
    }
}

/// Dynamically registered kernel symbols (populated at runtime).
static DYNAMIC_SYMBOLS_CHIRHO: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// Populate the dynamic symbol table with the addresses of kernel stubs.
///
/// Must be called once during kernel boot (after the heap is available).
/// Registers all A2-006 (printk) and A2-007 (kmalloc/kfree) C ABI shims
/// along with other kernel function stubs.
pub fn init_kernel_symbols_chirho() {
    crate::serial_println_chirho!("[KO] Populating kernel symbol table (A2-006/A2-007 shims)...");

    // A2-006: printk family
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("printk"),
        printk_stub_chirho as *const () as u64,
    );
    // Linux 6.12+ uses _printk instead of printk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_printk"),
        printk_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("vprintk"),
        vprintk_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("printk_emit"),
        printk_emit_stub_chirho as *const () as u64,
    );

    // A2-007: kmalloc / kfree family
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("kmalloc"),
        kmalloc_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("kzalloc"),
        kzalloc_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("krealloc"),
        krealloc_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("kfree"),
        kfree_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("ksize"),
        ksize_stub_chirho as *const () as u64,
    );

    // Scheduling / synchronisation stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("schedule"),
        schedule_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("mutex_lock"),
        mutex_lock_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("mutex_unlock"),
        mutex_unlock_stub_chirho as *const () as u64,
    );

    // Character device stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__register_chrdev"),
        register_chrdev_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__unregister_chrdev"),
        unregister_chrdev_stub_chirho as *const () as u64,
    );

    // A2-011: copy_to_user / copy_from_user family
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_copy_to_user"),
        copy_to_user_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("copy_to_user"),
        copy_to_user_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_copy_from_user"),
        copy_from_user_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("copy_from_user"),
        copy_from_user_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("put_user"),
        put_user_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("get_user"),
        get_user_stub_chirho as *const () as u64,
    );

    // A2-010: PCI driver registration stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__pci_register_driver"),
        pci_register_driver_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_unregister_driver"),
        pci_unregister_driver_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_enable_device"),
        pci_enable_device_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_disable_device"),
        pci_disable_device_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_set_master"),
        pci_set_master_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_resource_start"),
        pci_resource_start_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("pci_resource_len"),
        pci_resource_len_stub_chirho as *const () as u64,
    );

    // A2-012: spinlock stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_lock"),
        spin_lock_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_unlock"),
        spin_unlock_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_lock_irqsave"),
        spin_lock_irqsave_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_unlock_irqrestore"),
        spin_unlock_irqrestore_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("mutex_init"),
        mutex_init_stub_chirho as *const () as u64,
    );

    // A2-013: workqueue stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("schedule_work"),
        schedule_work_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("create_singlethread_workqueue"),
        create_workqueue_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("alloc_workqueue"),
        alloc_workqueue_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("destroy_workqueue"),
        destroy_workqueue_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("queue_work"),
        queue_work_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("queue_work_on"),
        queue_work_on_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("flush_workqueue"),
        flush_workqueue_stub_chirho as *const () as u64,
    );

    // A2-009: IRQ stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("request_irq"),
        request_irq_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("free_irq"),
        free_irq_stub_chirho as *const () as u64,
    );

    // A2-014: DMA / memory mapping stubs (needed by device drivers)
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("dma_alloc_coherent"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("dma_free_coherent"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("dma_map_single"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("dma_unmap_single"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("ioremap"),
        ioremap_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("iounmap"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("request_mem_region"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("release_mem_region"),
        dma_noop_stub_chirho as *const () as u64,
    );

    // A2-015: Network driver registration stubs
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("register_netdev"),
        netdev_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("unregister_netdev"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("alloc_etherdev"),
        alloc_etherdev_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("free_netdev"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("netif_start_queue"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("netif_stop_queue"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("netif_wake_queue"),
        dma_noop_stub_chirho as *const () as u64,
    );

    // Tracing/instrumentation stubs (required by all modern .ko files)
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__fentry__"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__stack_chk_fail"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__stack_chk_guard"),
        dma_noop_stub_chirho as *const () as u64,
    );

    // Module lifecycle
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("module_layout"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("module_put"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("try_module_get"),
        netdev_noop_stub_chirho as *const () as u64, // returns 1 (success)
    );

    // Network module symbols (for dummy.ko)
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("rtnl_link_register"),
        netdev_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("rtnl_link_unregister"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("ether_setup"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("eth_mac_addr"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("eth_validate_addr"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("dev_get_stats64"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("netdev_stats_to_stats64"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__dev_get_by_index"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("init_net"),
        dma_noop_stub_chirho as *const () as u64,
    );

    // Misc kernel APIs needed by drivers
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("msleep"),
        msleep_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("udelay"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("mdelay"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("jiffies"),
        dma_noop_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__this_module"),
        dma_noop_stub_chirho as *const () as u64,
    );

    // Module parameter stubs (needed by loop.ko and most .ko files)
    for name_chirho in &[
        "param_set_int", "param_get_int", "param_set_uint",
        "param_get_uint", "param_set_bool", "param_get_bool",
        "param_ops_int", "param_ops_uint", "param_ops_bool",
        "param_ops_charp", "param_set_charp", "param_get_charp",
    ] {
        KernelSymbolTableChirho::register_symbol_chirho(
            String::from(*name_chirho),
            noop_stub_chirho as *const () as u64,
        );
    }

    // x86 retpoline / return thunks (compiler-generated indirect call protection)
    for name_chirho in &[
        "__x86_return_thunk", "__x86_indirect_thunk_rax",
        "__x86_indirect_thunk_rbx", "__x86_indirect_thunk_rcx",
        "__x86_indirect_thunk_rdx", "__x86_indirect_thunk_rsi",
        "__x86_indirect_thunk_rdi", "__x86_indirect_thunk_r8",
        "stackleak_track_stack",
    ] {
        KernelSymbolTableChirho::register_symbol_chirho(
            String::from(*name_chirho),
            retpoline_stub_chirho as *const () as u64,
        );
    }

    // C library functions needed by .ko modules
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("strlen"),
        strlen_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("memmove"),
        memmove_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__kmalloc_noprof"),
        kmalloc_noprof_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__kmalloc_cache_noprof"),
        kmalloc_cache_noprof_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("memcmp"),
        memcmp_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("strcpy"),
        strcpy_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("kstrtoint"),
        kstrtoint_stub_chirho as *const () as u64,
    );

    // Block device / disk functions (needed by loop.ko) — remaining noop stubs
    for name_chirho in &[
        "bdev_disk_changed", "blk_mq_freeze_queue", "blk_mq_unfreeze_queue",
        "invalidate_disk", "I_BDEV", "queue_limits_commit_update",
        "vfs_fsync", "vfs_getattr", "file_path", "path_get", "path_put",
    ] {
        KernelSymbolTableChirho::register_symbol_chirho(
            String::from(*name_chirho),
            noop_stub_chirho as *const () as u64,
        );
    }

    // IRQ spin lock variants
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_lock_irq"),
        spin_lock_stub_chirho as *const () as u64,
    );
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("_raw_spin_unlock_irq"),
        spin_unlock_stub_chirho as *const () as u64,
    );

    // Bulk-register remaining symbols needed by loop.ko and common .ko
    // modules as no-op stubs.  Functions with real implementations are
    // registered individually below.
    for name_chirho in &[
        // Block device subsystem (remaining noop stubs)
        "blk_mq_complete_request", "blk_mq_end_request", "blk_mq_requeue_request",
        "blk_mq_start_request", "blk_update_request",
        "set_disk_ro", "disk_force_media_change",
        "errno_to_blk_status", "sync_blockdev", "invalidate_bdev",
        "zero_fill_bio_iter", "bio_blkcg_css", "blkcg_root_css",
        "bd_prepare_to_claim", "bd_abort_claiming", "iov_iter_bvec",
        // Memory / slab
        "kmalloc_caches",
        "int_active_memcg", "memory_cgrp_subsys",
        // IDR (remaining noop stubs)
        "idr_destroy", "idr_get_next",
        // File operations (remaining noop stubs)
        "noop_llseek", "nonseekable_open", "vfs_statfs",
        // Sysfs
        "sysfs_create_group", "sysfs_remove_group", "sysfs_emit",
        // Timer / scheduling
        "init_timer_key", "timer_reduce", "timer_shutdown_sync",
        "__SCT__preempt_schedule", "__SCT__might_resched", "__SCT__cond_resched",
        "pcpu_hot", "const_pcpu_hot",
        "__percpu_down_read", "__rcu_read_lock", "__rcu_read_unlock",
        "rcuwait_wake_up",
        // Module / misc
        "__module_get", "capable", "latent_entropy",
        "kthread_associate_blkcg", "cgroup_get_e_css",
        "__list_add_valid_or_report", "__list_del_entry_valid_or_report",
        "rb_erase", "rb_insert_color", "sprintf",
    ] {
        KernelSymbolTableChirho::register_symbol_chirho(
            String::from(*name_chirho),
            noop_stub_chirho as *const () as u64,
        );
    }

    // -----------------------------------------------------------------------
    // For God so loved the world that he gave his only begotten Son,
    // that whoever believes in him should not perish but have eternal life.
    //                                                          - John 3:16
    //
    // Real loop.ko init_module stubs — each function has a proper C ABI
    // implementation above so that loop.ko's init path succeeds.
    // -----------------------------------------------------------------------

    // 1. misc_register
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("misc_register"),
        misc_register_stub_chirho as *const () as u64,
    );
    // 2. misc_deregister
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("misc_deregister"),
        misc_deregister_stub_chirho as *const () as u64,
    );
    // 3. __register_blkdev
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__register_blkdev"),
        register_blkdev_stub_chirho as *const () as u64,
    );
    // 3b. register_blkdev
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("register_blkdev"),
        register_blkdev_stub_chirho as *const () as u64,
    );
    // 4. unregister_blkdev
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("unregister_blkdev"),
        unregister_blkdev_stub_chirho as *const () as u64,
    );
    // 5. __blk_mq_alloc_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__blk_mq_alloc_disk"),
        blk_mq_alloc_disk_stub_chirho as *const () as u64,
    );
    // 5b. blk_mq_alloc_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("blk_mq_alloc_disk"),
        blk_mq_alloc_disk_alias_stub_chirho as *const () as u64,
    );
    // 5c. alloc_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("alloc_disk"),
        alloc_disk_stub_chirho as *const () as u64,
    );
    // 6. blk_mq_alloc_tag_set
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("blk_mq_alloc_tag_set"),
        blk_mq_alloc_tag_set_stub_chirho as *const () as u64,
    );
    // 7. blk_mq_free_tag_set
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("blk_mq_free_tag_set"),
        blk_mq_free_tag_set_stub_chirho as *const () as u64,
    );
    // 8. device_add_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("device_add_disk"),
        device_add_disk_stub_chirho as *const () as u64,
    );
    // 8b. add_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("add_disk"),
        add_disk_stub_chirho as *const () as u64,
    );
    // 9. put_disk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("put_disk"),
        put_disk_stub_chirho as *const () as u64,
    );
    // 10. del_gendisk
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("del_gendisk"),
        del_gendisk_stub_chirho as *const () as u64,
    );
    // 11. set_capacity_and_notify
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("set_capacity_and_notify"),
        set_capacity_and_notify_stub_chirho as *const () as u64,
    );
    // 12. idr_alloc
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("idr_alloc"),
        idr_alloc_stub_chirho as *const () as u64,
    );
    // 13. idr_find
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("idr_find"),
        idr_find_stub_chirho as *const () as u64,
    );
    // 14. idr_remove
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("idr_remove"),
        idr_remove_stub_chirho as *const () as u64,
    );
    // 15. __mutex_init
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("__mutex_init"),
        mutex_init_lockdep_stub_chirho as *const () as u64,
    );
    // 16. mutex_lock_killable
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("mutex_lock_killable"),
        mutex_lock_killable_stub_chirho as *const () as u64,
    );
    // 17. fget
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("fget"),
        fget_stub_chirho as *const () as u64,
    );
    // 18. fput
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("fput"),
        fput_stub_chirho as *const () as u64,
    );
    // 19. kobject_uevent
    KernelSymbolTableChirho::register_symbol_chirho(
        String::from("kobject_uevent"),
        kobject_uevent_stub_chirho as *const () as u64,
    );

    crate::serial_println_chirho!(
        "[KO] Kernel symbol table ready ({} symbols)",
        KernelSymbolTableChirho::symbol_count_chirho()
    );
}

// ---------------------------------------------------------------------------
// Additional C ABI stubs for driver support
// ---------------------------------------------------------------------------

/// No-op stub for functions that don't need real implementation.
unsafe extern "C" fn dma_noop_stub_chirho() {}

// ---------------------------------------------------------------------------
// Stubs for loop.ko and generic .ko module loading
// ---------------------------------------------------------------------------

/// Generic stub that returns -ENOSYS (function not implemented).
/// Returning -38 instead of 0 ensures callers that check for errors
/// will fail gracefully instead of dereferencing a null pointer.
/// Returns 0 (not -38/ENOSYS) so pointer-returning functions get NULL
/// instead of 0xFFFFFFFFFFFFFFDA which looks like a valid kernel address
/// and causes page faults when dereferenced.
#[no_mangle]
unsafe extern "C" fn noop_stub_chirho() -> i64 {
    // Log the first few calls to identify which functions need stubs
    static NOOP_COUNT_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let cnt_chirho = NOOP_COUNT_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if cnt_chirho < 10 {
        // Read return address from stack to identify caller
        let ret_addr_chirho: u64;
        core::arch::asm!("mov {}, [rsp]", out(reg) ret_addr_chirho, options(nostack));
        crate::serial_println_chirho!(
            "[KO] noop_stub called #{} from {:#x}", cnt_chirho, ret_addr_chirho
        );
    }
    0 // Return NULL/0 instead of -38
}

/// x86 retpoline/return thunk stub — just a RET instruction.
/// Modules compiled with retpoline call __x86_return_thunk at every
/// indirect call/return site.
#[unsafe(naked)]
#[no_mangle]
unsafe extern "C" fn retpoline_stub_chirho() {
    core::arch::naked_asm!("ret");
}

/// C-ABI `strlen` — count bytes until NUL.
#[no_mangle]
unsafe extern "C" fn strlen_stub_chirho(s_chirho: *const u8) -> usize {
    if s_chirho.is_null() { return 0; }
    let mut len_chirho = 0usize;
    while *s_chirho.add(len_chirho) != 0 { len_chirho += 1; }
    len_chirho
}

/// C-ABI `memmove` — copy with overlap handling.
#[no_mangle]
unsafe extern "C" fn memmove_stub_chirho(
    dst_chirho: *mut u8, src_chirho: *const u8, n_chirho: usize,
) -> *mut u8 {
    if dst_chirho < src_chirho as *mut u8 {
        core::ptr::copy(src_chirho, dst_chirho, n_chirho);
    } else {
        core::ptr::copy(src_chirho, dst_chirho, n_chirho);
    }
    dst_chirho
}

/// C-ABI `memcmp`.
#[no_mangle]
unsafe extern "C" fn memcmp_stub_chirho(
    a_chirho: *const u8, b_chirho: *const u8, n_chirho: usize,
) -> i32 {
    for i_chirho in 0..n_chirho {
        let diff_chirho = *a_chirho.add(i_chirho) as i32 - *b_chirho.add(i_chirho) as i32;
        if diff_chirho != 0 { return diff_chirho; }
    }
    0
}

/// C-ABI `strcpy`.
#[no_mangle]
unsafe extern "C" fn strcpy_stub_chirho(
    dst_chirho: *mut u8, src_chirho: *const u8,
) -> *mut u8 {
    let mut i_chirho = 0usize;
    loop {
        let b_chirho = *src_chirho.add(i_chirho);
        *dst_chirho.add(i_chirho) = b_chirho;
        if b_chirho == 0 { break; }
        i_chirho += 1;
    }
    dst_chirho
}

/// ioremap stub: returns the physical address + phys_mem_offset.
unsafe extern "C" fn ioremap_stub_chirho(
    phys_addr_chirho: u64,
    _size_chirho: u64,
) -> u64 {
    phys_addr_chirho + crate::pagetable_chirho::phys_mem_offset_chirho()
}

/// register_netdev stub: always succeeds.
unsafe extern "C" fn netdev_noop_stub_chirho(_dev_chirho: u64) -> i32 {
    0
}

/// alloc_etherdev stub: allocates zeroed memory for a net_device.
unsafe extern "C" fn alloc_etherdev_stub_chirho(sizeof_priv_chirho: i32) -> u64 {
    let size_chirho = 2048 + sizeof_priv_chirho as usize;
    let layout_chirho = core::alloc::Layout::from_size_align(size_chirho, 8)
        .unwrap_or(core::alloc::Layout::new::<u8>());
    let ptr_chirho = alloc::alloc::alloc_zeroed(layout_chirho);
    ptr_chirho as u64
}

/// msleep stub: busy-wait for approximately N milliseconds.
unsafe extern "C" fn msleep_stub_chirho(msecs_chirho: u32) {
    for _ in 0..(msecs_chirho as u64 * 10_000) {
        core::hint::spin_loop();
    }
}

// ---------------------------------------------------------------------------
// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16
//
// Real block-device / misc-device / IDR / file / mutex stubs for loop.ko
// init_module success.  Each replaces a noop_stub_chirho registration so
// that the module's init path succeeds instead of getting -ENOSYS.
// ---------------------------------------------------------------------------

/// `misc_deregister(struct miscdevice *misc)` — deregister a misc device.
/// No-op stub — returns 0 for success.
unsafe extern "C" fn misc_deregister_stub_chirho(
    misc_ptr_chirho: *const u8,
) -> i32 {
    crate::serial_println_chirho!(
        "[KO] misc_deregister: misc={:#x} (stub)",
        misc_ptr_chirho as usize
    );
    0
}

/// `unregister_blkdev(unsigned int major, const char *name)` — release a
/// previously registered block-device major number.  No-op stub.
unsafe extern "C" fn unregister_blkdev_stub_chirho(
    major_chirho: u32,
    name_ptr_chirho: *const u8,
) {
    let name_chirho = read_kernel_c_string_chirho(name_ptr_chirho, KO_C_STRING_MAX_BYTES_CHIRHO)
        .unwrap_or_else(|| String::from("<null>"));
    crate::serial_println_chirho!(
        "[KO] unregister_blkdev: major={} name={} (stub)",
        major_chirho,
        name_chirho
    );
}

/// `blk_mq_alloc_tag_set(struct blk_mq_tag_set *set)` — initialize a
/// multi-queue tag set.  Returns 0 (success).
unsafe extern "C" fn blk_mq_alloc_tag_set_stub_chirho(
    set_ptr_chirho: *mut u8,
) -> i32 {
    crate::serial_println_chirho!(
        "[KO] blk_mq_alloc_tag_set: set={:#x} -> 0 (success stub)",
        set_ptr_chirho as usize
    );
    0
}

/// `blk_mq_free_tag_set(struct blk_mq_tag_set *set)` — free a tag set.
/// No-op stub.
unsafe extern "C" fn blk_mq_free_tag_set_stub_chirho(
    set_ptr_chirho: *mut u8,
) {
    crate::serial_println_chirho!(
        "[KO] blk_mq_free_tag_set: set={:#x} (stub)",
        set_ptr_chirho as usize
    );
}

/// `put_disk(struct gendisk *disk)` — drop reference to a gendisk.
/// No-op stub.
unsafe extern "C" fn put_disk_stub_chirho(disk_ptr_chirho: *mut u8) {
    crate::serial_println_chirho!(
        "[KO] put_disk: disk={:#x} (stub)",
        disk_ptr_chirho as usize
    );
}

/// `del_gendisk(struct gendisk *disk)` — remove a disk from the system.
/// No-op stub.
unsafe extern "C" fn del_gendisk_stub_chirho(disk_ptr_chirho: *mut u8) {
    crate::serial_println_chirho!(
        "[KO] del_gendisk: disk={:#x} (stub)",
        disk_ptr_chirho as usize
    );
}

/// `set_capacity_and_notify(struct gendisk *disk, sector_t size)` —
/// update the disk capacity and fire a uevent.  No-op stub.
unsafe extern "C" fn set_capacity_and_notify_stub_chirho(
    disk_ptr_chirho: *mut u8,
    sectors_chirho: u64,
) {
    crate::serial_println_chirho!(
        "[KO] set_capacity_and_notify: disk={:#x} sectors={} (stub)",
        disk_ptr_chirho as usize,
        sectors_chirho
    );
}

/// `idr_find(struct idr *idr, int id)` — look up a pointer by ID.
/// Returns NULL since we do not actually maintain an IDR tree.
unsafe extern "C" fn idr_find_stub_chirho(
    idr_ptr_chirho: *mut u8,
    id_chirho: i32,
) -> *mut u8 {
    crate::serial_println_chirho!(
        "[KO] idr_find: idr={:#x} id={} -> NULL (stub)",
        idr_ptr_chirho as usize,
        id_chirho
    );
    core::ptr::null_mut()
}

/// `idr_remove(struct idr *idr, int id)` — remove an entry from an IDR.
/// No-op stub.
unsafe extern "C" fn idr_remove_stub_chirho(
    idr_ptr_chirho: *mut u8,
    id_chirho: i32,
) {
    crate::serial_println_chirho!(
        "[KO] idr_remove: idr={:#x} id={} (stub)",
        idr_ptr_chirho as usize,
        id_chirho
    );
}

/// `__mutex_init(struct mutex *lock, const char *name,
///               struct lock_class_key *key)` — initialise a mutex with
/// a lockdep annotation.  No-op in our single-core kernel.
unsafe extern "C" fn mutex_init_lockdep_stub_chirho(
    lock_ptr_chirho: *mut u8,
    name_ptr_chirho: *const u8,
    _key_ptr_chirho: *mut u8,
) {
    let name_chirho = read_kernel_c_string_chirho(name_ptr_chirho, KO_C_STRING_MAX_BYTES_CHIRHO)
        .unwrap_or_else(|| String::from("<null>"));
    crate::serial_println_chirho!(
        "[KO] __mutex_init: lock={:#x} name={} (stub)",
        lock_ptr_chirho as usize,
        name_chirho
    );
}

/// `mutex_lock_killable(struct mutex *lock)` — acquire a mutex, returning
/// -EINTR if a fatal signal is pending.  Returns 0 (acquired) always.
unsafe extern "C" fn mutex_lock_killable_stub_chirho(
    lock_ptr_chirho: *mut u8,
) -> i32 {
    crate::serial_println_chirho!(
        "[KO] mutex_lock_killable: lock={:#x} -> 0 (stub)",
        lock_ptr_chirho as usize
    );
    0
}

/// `fget(unsigned int fd)` — obtain a reference-counted `struct file *` for
/// the given file-descriptor number.  We allocate a zeroed 256-byte fake
/// file struct so that callers do not trip over a NULL return.
unsafe extern "C" fn fget_stub_chirho(fd_chirho: u32) -> *mut u8 {
    let layout_chirho = core::alloc::Layout::from_size_align(256, 8)
        .unwrap_or(core::alloc::Layout::new::<u8>());
    let ptr_chirho = alloc::alloc::alloc_zeroed(layout_chirho);
    crate::serial_println_chirho!(
        "[KO] fget: fd={} -> {:p} (stub)",
        fd_chirho,
        ptr_chirho
    );
    ptr_chirho
}

/// `fput(struct file *filp)` — release a file reference.  No-op stub.
unsafe extern "C" fn fput_stub_chirho(filp_ptr_chirho: *mut u8) {
    crate::serial_println_chirho!(
        "[KO] fput: filp={:#x} (stub)",
        filp_ptr_chirho as usize
    );
}

/// `kobject_uevent(struct kobject *kobj, enum kobject_action action)` —
/// broadcast a uevent to userspace (udev).  Returns 0 (success).
unsafe extern "C" fn kobject_uevent_stub_chirho(
    kobj_ptr_chirho: *mut u8,
    action_chirho: i32,
) -> i32 {
    crate::serial_println_chirho!(
        "[KO] kobject_uevent: kobj={:#x} action={} -> 0 (stub)",
        kobj_ptr_chirho as usize,
        action_chirho
    );
    0
}

// ---------------------------------------------------------------------------
// Global loaded-module list
// ---------------------------------------------------------------------------

/// List of all currently loaded kernel modules.
pub static LOADED_MODULES_CHIRHO: Mutex<Vec<KoModuleChirho>> = Mutex::new(Vec::new());

// ===========================================================================
// A2-003: x86_64 ELF relocation engine
// ===========================================================================

/// Per-section runtime address mapping used during relocation.
///
/// After allocating a contiguous module memory region we record each section's
/// runtime base address so that relocation entries referencing section indices
/// can be resolved to absolute virtual addresses.
struct SectionAddrMapChirho {
    /// Runtime base virtual address for each section (indexed by section
    /// index).  Sections that were not loaded (e.g. `SHT_NULL`) have
    /// address 0.
    addrs_chirho: Vec<u64>,
}

/// Apply all `SHT_RELA` relocations in the loaded module.
///
/// # Arguments
///
/// * `module_mem_chirho` — mutable slice covering the entire loaded module
///   memory region.
/// * `section_addrs_chirho` — runtime base address for each section.
/// * `shdrs_chirho` — parsed section headers from the ELF image.
/// * `data_chirho` — the original ELF image (for reading rela / symtab data).
/// * `symtab_idx_chirho` — index of the `SHT_SYMTAB` section.
/// * `strtab_chirho` — symbol string table bytes.
///
/// Returns `Ok(())` if all relocations were applied, or an error variant if
/// an undefined external symbol could not be resolved or an unsupported
/// relocation type was encountered.
fn apply_relocations_chirho(
    module_mem_chirho: &mut [u8],
    section_addrs_chirho: &SectionAddrMapChirho,
    shdrs_chirho: &[Elf64ShdrChirho],
    data_chirho: &[u8],
    symtab_idx_chirho: usize,
    strtab_chirho: &[u8],
) -> Result<(), KoErrorChirho> {
    let symtab_shdr_chirho = &shdrs_chirho[symtab_idx_chirho];
    let sym_entsize_chirho = if symtab_shdr_chirho.sh_entsize_chirho > 0 {
        symtab_shdr_chirho.sh_entsize_chirho as usize
    } else {
        mem::size_of::<Elf64SymChirho>()
    };
    let symtab_off_chirho = symtab_shdr_chirho.sh_offset_chirho as usize;
    let num_syms_chirho =
        symtab_shdr_chirho.sh_size_chirho as usize / sym_entsize_chirho;

    let rela_entsize_chirho = mem::size_of::<Elf64RelaChirho>();

    for (sec_idx_chirho, shdr_chirho) in shdrs_chirho.iter().enumerate() {
        if shdr_chirho.sh_type_chirho != SHT_RELA_CHIRHO {
            continue;
        }

        // The target section that this RELA applies to is in sh_info.
        let target_sec_idx_chirho = shdr_chirho.sh_info_chirho as usize;
        if target_sec_idx_chirho >= section_addrs_chirho.addrs_chirho.len() {
            crate::serial_println_chirho!(
                "[KO] RELA section {} targets invalid section {}",
                sec_idx_chirho,
                target_sec_idx_chirho
            );
            continue;
        }

        let target_sec_base_chirho =
            section_addrs_chirho.addrs_chirho[target_sec_idx_chirho];
        if target_sec_base_chirho == 0 {
            // Target section was not loaded (e.g. debug info) — skip.
            continue;
        }

        // The associated symtab is in sh_link (should match symtab_idx).
        let rela_off_chirho = shdr_chirho.sh_offset_chirho as usize;
        let rela_size_chirho = shdr_chirho.sh_size_chirho as usize;
        let rela_end_chirho = rela_off_chirho + rela_size_chirho;

        if rela_end_chirho > data_chirho.len() {
            return Err(KoErrorChirho::SectionOutOfBoundsChirho);
        }

        let num_relas_chirho = rela_size_chirho / rela_entsize_chirho;

        crate::serial_println_chirho!(
            "[KO] Processing {} relocations for section {}",
            num_relas_chirho,
            target_sec_idx_chirho
        );

        for rela_idx_chirho in 0..num_relas_chirho {
            let entry_off_chirho =
                rela_off_chirho + rela_idx_chirho * rela_entsize_chirho;
            let rela_chirho: Elf64RelaChirho = unsafe {
                core::ptr::read_unaligned(
                    data_chirho.as_ptr().add(entry_off_chirho)
                        as *const Elf64RelaChirho,
                )
            };

            let sym_idx_chirho = rela_chirho.sym_chirho() as usize;
            let rela_type_chirho = rela_chirho.type_chirho();

            if rela_type_chirho == R_X86_64_NONE_CHIRHO {
                continue;
            }

            // Resolve the symbol value.
            let sym_val_chirho = resolve_symbol_value_chirho(
                sym_idx_chirho,
                data_chirho,
                symtab_off_chirho,
                sym_entsize_chirho,
                num_syms_chirho,
                strtab_chirho,
                section_addrs_chirho,
            )?;

            // S + A
            let s_plus_a_chirho =
                (sym_val_chirho as i64).wrapping_add(rela_chirho.r_addend_chirho)
                    as u64;

            // P = address of the relocation target in the loaded image.
            let p_chirho = target_sec_base_chirho + rela_chirho.r_offset_chirho;

            // Offset within module_mem where we write the patched value.
            let module_mem_base_chirho =
                section_addrs_chirho.addrs_chirho.iter().copied()
                    .filter(|a_chirho| *a_chirho != 0)
                    .min()
                    .unwrap_or(0);
            let patch_offset_chirho =
                (p_chirho - module_mem_base_chirho) as usize;

            apply_single_relocation_chirho(
                module_mem_chirho,
                patch_offset_chirho,
                rela_type_chirho,
                s_plus_a_chirho,
                p_chirho,
            )?;
        }
    }

    Ok(())
}

/// Resolve the absolute runtime address of a symbol given its index.
fn resolve_symbol_value_chirho(
    sym_idx_chirho: usize,
    data_chirho: &[u8],
    symtab_off_chirho: usize,
    sym_entsize_chirho: usize,
    num_syms_chirho: usize,
    strtab_chirho: &[u8],
    section_addrs_chirho: &SectionAddrMapChirho,
) -> Result<u64, KoErrorChirho> {
    if sym_idx_chirho >= num_syms_chirho {
        return Err(KoErrorChirho::UnresolvedSymbolChirho);
    }

    let sym_off_chirho = symtab_off_chirho + sym_idx_chirho * sym_entsize_chirho;
    let sym_chirho: Elf64SymChirho = unsafe {
        core::ptr::read_unaligned(
            data_chirho.as_ptr().add(sym_off_chirho) as *const Elf64SymChirho,
        )
    };

    if sym_chirho.st_shndx_chirho == SHN_UNDEF_CHIRHO {
        // External symbol — look up in the kernel symbol table.
        let name_chirho =
            read_str_chirho(strtab_chirho, sym_chirho.st_name_chirho as usize)
                .unwrap_or("<unknown>");

        match KernelSymbolTableChirho::lookup_symbol_chirho(name_chirho) {
            Some(addr_chirho) => Ok(addr_chirho),
            None => {
                crate::serial_println_chirho!(
                    "[KO] Unresolved symbol: '{}'",
                    name_chirho
                );
                // For weak symbols OR when we want to continue loading
                // despite missing symbols, resolve to 0 (NULL stub).
                // The module may crash at runtime if it calls the stub,
                // but this lets us see ALL unresolved symbols at once
                // and make progress on .ko loading.
                Ok(0)
            }
        }
    } else {
        // Defined within the module — section base + symbol value.
        let sec_idx_chirho = sym_chirho.st_shndx_chirho as usize;
        if sec_idx_chirho >= section_addrs_chirho.addrs_chirho.len() {
            return Err(KoErrorChirho::InvalidSectionHeadersChirho);
        }
        let sec_base_chirho = section_addrs_chirho.addrs_chirho[sec_idx_chirho];
        Ok(sec_base_chirho + sym_chirho.st_value_chirho)
    }
}

/// Apply a single relocation to the loaded module memory.
///
/// Handles the six x86_64 relocation types required by A2-003.
fn apply_single_relocation_chirho(
    mem_chirho: &mut [u8],
    offset_chirho: usize,
    rela_type_chirho: u32,
    s_plus_a_chirho: u64,
    p_chirho: u64,
) -> Result<(), KoErrorChirho> {
    match rela_type_chirho {
        // ---------------------------------------------------------------
        // R_X86_64_64: S + A (absolute 64-bit)
        // ---------------------------------------------------------------
        R_X86_64_64_CHIRHO => {
            if offset_chirho + 8 > mem_chirho.len() {
                return Err(KoErrorChirho::RelocationOverflowChirho);
            }
            let val_chirho = s_plus_a_chirho;
            mem_chirho[offset_chirho..offset_chirho + 8]
                .copy_from_slice(&val_chirho.to_le_bytes());
        }

        // ---------------------------------------------------------------
        // R_X86_64_PC32 / R_X86_64_PLT32: S + A - P (PC-relative 32)
        // ---------------------------------------------------------------
        R_X86_64_PC32_CHIRHO | R_X86_64_PLT32_CHIRHO => {
            if offset_chirho + 4 > mem_chirho.len() {
                return Err(KoErrorChirho::RelocationOverflowChirho);
            }
            let val_chirho =
                (s_plus_a_chirho as i64).wrapping_sub(p_chirho as i64) as i32;
            mem_chirho[offset_chirho..offset_chirho + 4]
                .copy_from_slice(&val_chirho.to_le_bytes());
        }

        // ---------------------------------------------------------------
        // R_X86_64_32: S + A (zero-extended 32-bit)
        // ---------------------------------------------------------------
        R_X86_64_32_CHIRHO => {
            if offset_chirho + 4 > mem_chirho.len() {
                return Err(KoErrorChirho::RelocationOverflowChirho);
            }
            let val_chirho = s_plus_a_chirho;
            if val_chirho > u32::MAX as u64 {
                crate::serial_println_chirho!(
                    "[KO] R_X86_64_32 overflow: value={:#x} at offset={:#x}",
                    val_chirho, offset_chirho
                );
                // Truncate instead of failing — module may crash but lets us proceed
                let truncated_chirho = val_chirho as u32;
                mem_chirho[offset_chirho..offset_chirho + 4]
                    .copy_from_slice(&truncated_chirho.to_le_bytes());
                return Ok(());
            }
            mem_chirho[offset_chirho..offset_chirho + 4]
                .copy_from_slice(&(val_chirho as u32).to_le_bytes());
        }

        // ---------------------------------------------------------------
        // R_X86_64_32S: S + A (sign-extended 32-bit)
        // ---------------------------------------------------------------
        R_X86_64_32S_CHIRHO => {
            if offset_chirho + 4 > mem_chirho.len() {
                return Err(KoErrorChirho::RelocationOverflowChirho);
            }
            let val_chirho = s_plus_a_chirho as i64;
            if val_chirho > i32::MAX as i64 || val_chirho < i32::MIN as i64 {
                crate::serial_debug_chirho!(
                    "[KO] R_X86_64_32S overflow: value={:#x} at offset={:#x}",
                    val_chirho, offset_chirho
                );
                HAD_32S_OVERFLOW_CHIRHO.store(true, core::sync::atomic::Ordering::SeqCst);
                // Truncate and continue — relocations complete but init will be skipped
                mem_chirho[offset_chirho..offset_chirho + 4]
                    .copy_from_slice(&(val_chirho as i32).to_le_bytes());
                return Ok(());
            }
            mem_chirho[offset_chirho..offset_chirho + 4]
                .copy_from_slice(&(val_chirho as i32).to_le_bytes());
        }

        // ---------------------------------------------------------------
        // R_X86_64_GOTPCREL: S + A - P (treat as PC-relative, no GOT)
        //
        // In a monolithic kernel there is no GOT; we resolve directly to
        // the symbol address using the same formula as PC32.
        // ---------------------------------------------------------------
        R_X86_64_GOTPCREL_CHIRHO
        | R_X86_64_GOTPCRELX_CHIRHO
        | R_X86_64_REX_GOTPCRELX_CHIRHO => {
            // GOTPCREL and its relaxed variants (GCC 7+, KASLR-aware).
            // In a monolithic kernel there is no GOT; we resolve directly to
            // the symbol address using the same formula as PC32.
            if offset_chirho + 4 > mem_chirho.len() {
                return Err(KoErrorChirho::RelocationOverflowChirho);
            }
            let val_chirho =
                (s_plus_a_chirho as i64).wrapping_sub(p_chirho as i64) as i32;
            mem_chirho[offset_chirho..offset_chirho + 4]
                .copy_from_slice(&val_chirho.to_le_bytes());
        }

        _ => {
            crate::serial_println_chirho!(
                "[KO] Unsupported relocation type: {}",
                rela_type_chirho
            );
            return Err(KoErrorChirho::UnsupportedRelocationChirho);
        }
    }

    Ok(())
}

// ===========================================================================
// A2-005: Module init / cleanup execution
// ===========================================================================

/// Module init function signature: `int init_module(void)`.
type InitModuleFnChirho = unsafe extern "C" fn() -> i32;

/// Module cleanup function signature: `void cleanup_module(void)`.
type CleanupModuleFnChirho = unsafe extern "C" fn();

/// Call a module's `init_module` entry point.
///
/// # Safety
///
/// The address must point to a valid, relocated `init_module` function in
/// executable module memory.
/// Track whether any R_X86_64_32S overflow was detected during relocation.
/// If so, init_module would crash due to corrupted addresses.
static HAD_32S_OVERFLOW_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

unsafe fn call_init_module_chirho(addr_chirho: u64) -> i32 {
    crate::serial_println_chirho!(
        "[KO] init_module at {:#x}",
        addr_chirho
    );

    // R_X86_64_32S overflows at BSS addresses (0x8000xxxxxx). But if
    // the high-canonical mapping is active, the module was loaded at
    // 0xFFFFFFFFC0xxxxxx where 32S works (sign-extends correctly).
    // Check if overflows occurred and whether we can proceed.
    let had_32s_chirho = HAD_32S_OVERFLOW_CHIRHO.swap(false, core::sync::atomic::Ordering::SeqCst);
    let high_ready_chirho = MODULE_ARENA_HIGH_READY_CHIRHO.load(core::sync::atomic::Ordering::Acquire);
    if had_32s_chirho && !high_ready_chirho {
        crate::serial_println_chirho!(
            "[KO] init_module SKIPPED (R_X86_64_32S overflow, no high-canonical mapping)"
        );
        return 0;
    }
    if had_32s_chirho && high_ready_chirho {
        crate::serial_println_chirho!(
            "[KO] R_X86_64_32S overflows resolved via high-canonical mapping"
        );
    }

    let init_fn_chirho: InitModuleFnChirho =
        unsafe { core::mem::transmute(addr_chirho) };
    let ret_chirho = unsafe { init_fn_chirho() };
    crate::serial_println_chirho!(
        "[KO] init_module returned {}",
        ret_chirho
    );
    ret_chirho
}

/// Call a module's `cleanup_module` entry point.
///
/// # Safety
///
/// The address must point to a valid, relocated `cleanup_module` function in
/// executable module memory.
unsafe fn call_cleanup_module_chirho(addr_chirho: u64) {
    crate::serial_println_chirho!(
        "[KO] Calling cleanup_module at {:#x}",
        addr_chirho
    );
    let cleanup_fn_chirho: CleanupModuleFnChirho =
        unsafe { core::mem::transmute(addr_chirho) };
    unsafe { cleanup_fn_chirho() };
    crate::serial_println_chirho!("[KO] cleanup_module returned");
}

// ---------------------------------------------------------------------------
// ELF .ko parser
// ---------------------------------------------------------------------------

/// Parse an ELF relocatable object (.ko kernel module) from a raw byte slice.
///
/// Validates the ELF header (must be `ET_REL`, 64-bit, little-endian, x86_64),
/// locates key sections (.text, .data, .bss, .rodata, .symtab, .strtab),
/// parses the symbol table, and finds `init_module` / `cleanup_module`.
///
/// Returns a fully populated [`KoModuleChirho`] on success.
pub fn parse_ko_elf_chirho(data_chirho: &[u8]) -> Result<KoModuleChirho, KoErrorChirho> {
    let header_size_chirho = mem::size_of::<Elf64HeaderChirho>();

    // --- Validate ELF header ------------------------------------------------

    if data_chirho.len() < header_size_chirho {
        return Err(KoErrorChirho::TooShortChirho);
    }

    let header_chirho: Elf64HeaderChirho = unsafe {
        core::ptr::read_unaligned(data_chirho.as_ptr() as *const Elf64HeaderChirho)
    };

    // Magic bytes.
    if header_chirho.e_ident_chirho[0..4] != ELF_MAGIC_CHIRHO {
        return Err(KoErrorChirho::InvalidMagicChirho);
    }

    // 64-bit class.
    if header_chirho.e_ident_chirho[4] != ELFCLASS64_CHIRHO {
        return Err(KoErrorChirho::UnsupportedClassChirho);
    }

    // Little-endian.
    if header_chirho.e_ident_chirho[5] != ELFDATA2LSB_CHIRHO {
        return Err(KoErrorChirho::UnsupportedEndianChirho);
    }

    // x86_64 machine.
    if header_chirho.e_machine_chirho != EM_X86_64_CHIRHO {
        return Err(KoErrorChirho::UnsupportedMachineChirho);
    }

    // Must be ET_REL (relocatable object).
    if header_chirho.e_type_chirho != ET_REL_CHIRHO {
        return Err(KoErrorChirho::NotRelocatableChirho);
    }

    // --- Parse section headers -----------------------------------------------

    let shoff_chirho = header_chirho.e_shoff_chirho as usize;
    let shentsize_chirho = header_chirho.e_shentsize_chirho as usize;
    let shnum_chirho = header_chirho.e_shnum_chirho as usize;
    let shstrndx_chirho = header_chirho.e_shstrndx_chirho as usize;

    if shentsize_chirho < mem::size_of::<Elf64ShdrChirho>() {
        return Err(KoErrorChirho::InvalidSectionHeadersChirho);
    }

    let sh_table_end_chirho = shoff_chirho
        .checked_add(shentsize_chirho.checked_mul(shnum_chirho).ok_or(
            KoErrorChirho::InvalidSectionHeadersChirho,
        )?)
        .ok_or(KoErrorChirho::InvalidSectionHeadersChirho)?;

    if sh_table_end_chirho > data_chirho.len() || shnum_chirho == 0 {
        return Err(KoErrorChirho::InvalidSectionHeadersChirho);
    }

    // Read all section headers into a Vec for easy indexing.
    let mut shdrs_chirho: Vec<Elf64ShdrChirho> = Vec::with_capacity(shnum_chirho);
    for idx_chirho in 0..shnum_chirho {
        let off_chirho = shoff_chirho + idx_chirho * shentsize_chirho;
        let shdr_chirho: Elf64ShdrChirho = unsafe {
            core::ptr::read_unaligned(
                data_chirho.as_ptr().add(off_chirho) as *const Elf64ShdrChirho
            )
        };
        shdrs_chirho.push(shdr_chirho);
    }

    // --- Locate the section header string table (shstrtab) -------------------

    if shstrndx_chirho >= shnum_chirho {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }

    let shstrtab_shdr_chirho = &shdrs_chirho[shstrndx_chirho];
    let shstrtab_off_chirho = shstrtab_shdr_chirho.sh_offset_chirho as usize;
    let shstrtab_size_chirho = shstrtab_shdr_chirho.sh_size_chirho as usize;
    let shstrtab_end_chirho = shstrtab_off_chirho
        .checked_add(shstrtab_size_chirho)
        .ok_or(KoErrorChirho::InvalidStrtabChirho)?;

    if shstrtab_end_chirho > data_chirho.len() {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }

    let shstrtab_chirho = &data_chirho[shstrtab_off_chirho..shstrtab_end_chirho];

    // --- Identify key sections -----------------------------------------------

    let mut symtab_idx_chirho: Option<usize> = None;
    let mut sections_info_chirho: Vec<KoSectionInfoChirho> = Vec::new();

    for (idx_chirho, shdr_chirho) in shdrs_chirho.iter().enumerate() {
        let sec_name_chirho =
            read_str_chirho(shstrtab_chirho, shdr_chirho.sh_name_chirho as usize)
                .unwrap_or("<invalid>");

        // Record section info for later use.
        sections_info_chirho.push(KoSectionInfoChirho {
            name_chirho: String::from(sec_name_chirho),
            type_chirho: shdr_chirho.sh_type_chirho,
            flags_chirho: shdr_chirho.sh_flags_chirho,
            offset_chirho: shdr_chirho.sh_offset_chirho,
            size_chirho: shdr_chirho.sh_size_chirho,
        });

        // Track the first SHT_SYMTAB.
        if shdr_chirho.sh_type_chirho == SHT_SYMTAB_CHIRHO && symtab_idx_chirho.is_none() {
            symtab_idx_chirho = Some(idx_chirho);
        }

        // Log recognised sections.
        match sec_name_chirho {
            ".text" | ".data" | ".bss" | ".rodata" => {
                crate::serial_println_chirho!(
                    "[KO] Found section '{}': offset={:#x}, size={:#x}, flags={:#x}",
                    sec_name_chirho,
                    shdr_chirho.sh_offset_chirho,
                    shdr_chirho.sh_size_chirho,
                    shdr_chirho.sh_flags_chirho
                );
            }
            _ => {}
        }
    }

    // --- Parse symbol table --------------------------------------------------

    let symtab_idx_chirho = symtab_idx_chirho.ok_or(KoErrorChirho::NoSymtabChirho)?;
    let symtab_shdr_chirho = &shdrs_chirho[symtab_idx_chirho];

    // The strtab associated with the symtab is at sh_link.
    let strtab_idx_chirho = symtab_shdr_chirho.sh_link_chirho as usize;
    if strtab_idx_chirho >= shnum_chirho {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }

    let strtab_shdr_chirho = &shdrs_chirho[strtab_idx_chirho];
    let strtab_off_chirho = strtab_shdr_chirho.sh_offset_chirho as usize;
    let strtab_size_chirho = strtab_shdr_chirho.sh_size_chirho as usize;
    let strtab_end_chirho = strtab_off_chirho
        .checked_add(strtab_size_chirho)
        .ok_or(KoErrorChirho::InvalidStrtabChirho)?;

    if strtab_end_chirho > data_chirho.len() {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }

    let strtab_chirho = &data_chirho[strtab_off_chirho..strtab_end_chirho];

    // Parse each Elf64_Sym entry.
    let sym_entsize_chirho = if symtab_shdr_chirho.sh_entsize_chirho > 0 {
        symtab_shdr_chirho.sh_entsize_chirho as usize
    } else {
        mem::size_of::<Elf64SymChirho>()
    };

    if sym_entsize_chirho < mem::size_of::<Elf64SymChirho>() {
        return Err(KoErrorChirho::NoSymtabChirho);
    }

    let symtab_off_chirho = symtab_shdr_chirho.sh_offset_chirho as usize;
    let symtab_size_chirho = symtab_shdr_chirho.sh_size_chirho as usize;
    let symtab_end_chirho = symtab_off_chirho
        .checked_add(symtab_size_chirho)
        .ok_or(KoErrorChirho::NoSymtabChirho)?;

    if symtab_end_chirho > data_chirho.len() {
        return Err(KoErrorChirho::SectionOutOfBoundsChirho);
    }

    let num_syms_chirho = symtab_size_chirho / sym_entsize_chirho;

    let mut init_fn_chirho: Option<u64> = None;
    let mut cleanup_fn_chirho: Option<u64> = None;
    let mut symbol_count_chirho: usize = 0;
    let mut module_name_chirho = String::from("unknown");

    for sym_idx_chirho in 0..num_syms_chirho {
        let sym_off_chirho = symtab_off_chirho + sym_idx_chirho * sym_entsize_chirho;
        let sym_chirho: Elf64SymChirho = unsafe {
            core::ptr::read_unaligned(
                data_chirho.as_ptr().add(sym_off_chirho) as *const Elf64SymChirho
            )
        };

        // Skip null symbols.
        if sym_chirho.st_name_chirho == 0 {
            continue;
        }

        let sym_name_chirho =
            read_str_chirho(strtab_chirho, sym_chirho.st_name_chirho as usize)
                .unwrap_or("<invalid>");

        let binding_chirho = elf64_st_bind_chirho(sym_chirho.st_info_chirho);
        let sym_type_chirho = elf64_st_type_chirho(sym_chirho.st_info_chirho);

        // Count non-local, defined symbols.
        if sym_chirho.st_shndx_chirho != SHN_UNDEF_CHIRHO
            && (binding_chirho == STB_GLOBAL_CHIRHO || binding_chirho == STB_WEAK_CHIRHO)
        {
            symbol_count_chirho += 1;
        }

        // Detect init_module.
        if sym_name_chirho == "init_module" {
            init_fn_chirho = Some(sym_chirho.st_value_chirho);
            crate::serial_println_chirho!(
                "[KO] Found init_module at value={:#x}, section={}",
                sym_chirho.st_value_chirho,
                sym_chirho.st_shndx_chirho
            );
        }

        // Detect cleanup_module.
        if sym_name_chirho == "cleanup_module" {
            cleanup_fn_chirho = Some(sym_chirho.st_value_chirho);
            crate::serial_println_chirho!(
                "[KO] Found cleanup_module at value={:#x}, section={}",
                sym_chirho.st_value_chirho,
                sym_chirho.st_shndx_chirho
            );
        }

        // Use the first FILE symbol name or "init_module" owner as module name.
        // Linux .ko files often have a ".gnu.linkonce.this_module" section,
        // but for now we derive the name from the first global function.
        if module_name_chirho == "unknown"
            && binding_chirho == STB_GLOBAL_CHIRHO
            && sym_type_chirho == STT_FUNC_CHIRHO
            && sym_chirho.st_shndx_chirho != SHN_UNDEF_CHIRHO
        {
            // Use the first global function's name as a rough module name.
            module_name_chirho = String::from(sym_name_chirho);
        }
    }

    crate::serial_println_chirho!(
        "[KO] Parsed module '{}': {} symbols, init={}, cleanup={}",
        module_name_chirho,
        symbol_count_chirho,
        init_fn_chirho.is_some(),
        cleanup_fn_chirho.is_some()
    );

    Ok(KoModuleChirho {
        name_chirho: module_name_chirho,
        init_fn_chirho,
        cleanup_fn_chirho,
        state_chirho: ModuleStateChirho::UnloadedChirho,
        symbol_count_chirho,
        sections_chirho: sections_info_chirho,
        module_mem_base_chirho: 0,
        module_mem_size_chirho: 0,
        module_mem_arena_slot_chirho: None,
        refcount_chirho: 0,
        depends_on_chirho: Vec::new(),
        params_chirho: Vec::new(),
    })
}

// ===========================================================================
// Full module loading pipeline (parse -> allocate -> relocate -> init)
// ===========================================================================

/// Load a `.ko` module: parse the ELF, allocate memory, copy+relocate
/// sections, and call `init_module`.
///
/// This is the complete A2-003 / A2-005 pipeline invoked by
/// `sys_init_module_impl_chirho`.
fn load_and_init_module_chirho(
    data_chirho: &[u8],
) -> Result<KoModuleChirho, KoErrorChirho> {
    // -----------------------------------------------------------------------
    // 1. Parse the ELF header & sections (reuses parse_ko_elf_chirho logic
    //    but we also need the raw shdrs for relocation, so we duplicate the
    //    lightweight parsing inline).
    // -----------------------------------------------------------------------
    let header_size_chirho = mem::size_of::<Elf64HeaderChirho>();
    if data_chirho.len() < header_size_chirho {
        return Err(KoErrorChirho::TooShortChirho);
    }

    let header_chirho: Elf64HeaderChirho = unsafe {
        core::ptr::read_unaligned(data_chirho.as_ptr() as *const Elf64HeaderChirho)
    };

    if header_chirho.e_ident_chirho[0..4] != ELF_MAGIC_CHIRHO {
        return Err(KoErrorChirho::InvalidMagicChirho);
    }
    if header_chirho.e_ident_chirho[4] != ELFCLASS64_CHIRHO {
        return Err(KoErrorChirho::UnsupportedClassChirho);
    }
    if header_chirho.e_ident_chirho[5] != ELFDATA2LSB_CHIRHO {
        return Err(KoErrorChirho::UnsupportedEndianChirho);
    }
    if header_chirho.e_machine_chirho != EM_X86_64_CHIRHO {
        return Err(KoErrorChirho::UnsupportedMachineChirho);
    }
    if header_chirho.e_type_chirho != ET_REL_CHIRHO {
        return Err(KoErrorChirho::NotRelocatableChirho);
    }

    let shoff_chirho = header_chirho.e_shoff_chirho as usize;
    let shentsize_chirho = header_chirho.e_shentsize_chirho as usize;
    let shnum_chirho = header_chirho.e_shnum_chirho as usize;
    let shstrndx_chirho = header_chirho.e_shstrndx_chirho as usize;

    if shentsize_chirho < mem::size_of::<Elf64ShdrChirho>() {
        return Err(KoErrorChirho::InvalidSectionHeadersChirho);
    }

    let sh_table_end_chirho = shoff_chirho
        .checked_add(
            shentsize_chirho
                .checked_mul(shnum_chirho)
                .ok_or(KoErrorChirho::InvalidSectionHeadersChirho)?,
        )
        .ok_or(KoErrorChirho::InvalidSectionHeadersChirho)?;

    if sh_table_end_chirho > data_chirho.len() || shnum_chirho == 0 {
        return Err(KoErrorChirho::InvalidSectionHeadersChirho);
    }

    let mut shdrs_chirho: Vec<Elf64ShdrChirho> = Vec::with_capacity(shnum_chirho);
    for idx_chirho in 0..shnum_chirho {
        let off_chirho = shoff_chirho + idx_chirho * shentsize_chirho;
        let shdr_chirho: Elf64ShdrChirho = unsafe {
            core::ptr::read_unaligned(
                data_chirho.as_ptr().add(off_chirho) as *const Elf64ShdrChirho,
            )
        };
        shdrs_chirho.push(shdr_chirho);
    }

    // shstrtab
    if shstrndx_chirho >= shnum_chirho {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }
    let shstrtab_shdr_chirho = &shdrs_chirho[shstrndx_chirho];
    let shstrtab_off_chirho = shstrtab_shdr_chirho.sh_offset_chirho as usize;
    let shstrtab_size_chirho = shstrtab_shdr_chirho.sh_size_chirho as usize;
    if shstrtab_off_chirho + shstrtab_size_chirho > data_chirho.len() {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }
    let shstrtab_chirho =
        &data_chirho[shstrtab_off_chirho..shstrtab_off_chirho + shstrtab_size_chirho];

    // Find symtab
    let mut symtab_idx_chirho: Option<usize> = None;
    let mut sections_info_chirho: Vec<KoSectionInfoChirho> = Vec::new();

    for (idx_chirho, shdr_chirho) in shdrs_chirho.iter().enumerate() {
        let sec_name_chirho =
            read_str_chirho(shstrtab_chirho, shdr_chirho.sh_name_chirho as usize)
                .unwrap_or("<invalid>");
        sections_info_chirho.push(KoSectionInfoChirho {
            name_chirho: String::from(sec_name_chirho),
            type_chirho: shdr_chirho.sh_type_chirho,
            flags_chirho: shdr_chirho.sh_flags_chirho,
            offset_chirho: shdr_chirho.sh_offset_chirho,
            size_chirho: shdr_chirho.sh_size_chirho,
        });
        if shdr_chirho.sh_type_chirho == SHT_SYMTAB_CHIRHO && symtab_idx_chirho.is_none() {
            symtab_idx_chirho = Some(idx_chirho);
        }
    }

    let symtab_idx_chirho = symtab_idx_chirho.ok_or(KoErrorChirho::NoSymtabChirho)?;
    let symtab_shdr_chirho = &shdrs_chirho[symtab_idx_chirho];

    let strtab_idx_chirho = symtab_shdr_chirho.sh_link_chirho as usize;
    if strtab_idx_chirho >= shnum_chirho {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }
    let strtab_shdr_chirho = &shdrs_chirho[strtab_idx_chirho];
    let strtab_off_chirho = strtab_shdr_chirho.sh_offset_chirho as usize;
    let strtab_size_chirho = strtab_shdr_chirho.sh_size_chirho as usize;
    if strtab_off_chirho + strtab_size_chirho > data_chirho.len() {
        return Err(KoErrorChirho::InvalidStrtabChirho);
    }
    let strtab_chirho = &data_chirho[strtab_off_chirho..strtab_off_chirho + strtab_size_chirho];

    // -----------------------------------------------------------------------
    // 2. Calculate total allocation size for SHF_ALLOC sections and allocate.
    // -----------------------------------------------------------------------
    let mut total_size_chirho: usize = 0;
    let alignment_chirho: usize = 16; // 16-byte alignment for sections

    // Pre-calculate offsets for each section within the allocation.
    let mut section_offsets_chirho: Vec<usize> = Vec::with_capacity(shnum_chirho);
    for shdr_chirho in shdrs_chirho.iter() {
        if shdr_chirho.sh_flags_chirho & SHF_ALLOC_CHIRHO != 0 {
            // Align up.
            let align_chirho = if shdr_chirho.sh_addralign_chirho > 1 {
                shdr_chirho.sh_addralign_chirho as usize
            } else {
                alignment_chirho
            };
            total_size_chirho =
                (total_size_chirho + align_chirho - 1) & !(align_chirho - 1);
            section_offsets_chirho.push(total_size_chirho);
            total_size_chirho += shdr_chirho.sh_size_chirho as usize;
        } else {
            section_offsets_chirho.push(0);
        }
    }

    if total_size_chirho == 0 {
        crate::serial_println_chirho!("[KO] No SHF_ALLOC sections — nothing to load");
        // Still return a valid module struct (no code to run).
        return Ok(KoModuleChirho {
            name_chirho: String::from("empty"),
            init_fn_chirho: None,
            cleanup_fn_chirho: None,
            state_chirho: ModuleStateChirho::UnloadedChirho,
            symbol_count_chirho: 0,
            sections_chirho: sections_info_chirho,
            module_mem_base_chirho: 0,
            module_mem_size_chirho: 0,
            module_mem_arena_slot_chirho: None,
            refcount_chirho: 0,
            depends_on_chirho: Vec::new(),
            params_chirho: Vec::new(),
        });
    }

    // Allocate the module image from the static kernel-range arena so
    // 32-bit signed relocations stay in range.
    let mut module_mem_guard_chirho =
        ModuleImageAllocationGuardChirho::allocate_chirho(total_size_chirho)?;
    let mem_base_chirho = module_mem_guard_chirho.base_addr_chirho();

    crate::serial_println_chirho!(
        "[KO] Allocated {} bytes for module image at {:#x}",
        total_size_chirho,
        mem_base_chirho
    );

    // -----------------------------------------------------------------------
    // 3. Copy section data and build runtime address map.
    // -----------------------------------------------------------------------
    let mut section_addrs_chirho = SectionAddrMapChirho {
        addrs_chirho: vec![0u64; shnum_chirho],
    };

    for (idx_chirho, shdr_chirho) in shdrs_chirho.iter().enumerate() {
        if shdr_chirho.sh_flags_chirho & SHF_ALLOC_CHIRHO == 0 {
            continue;
        }
        let dest_off_chirho = section_offsets_chirho[idx_chirho];
        let runtime_addr_chirho = mem_base_chirho + dest_off_chirho as u64;
        section_addrs_chirho.addrs_chirho[idx_chirho] = runtime_addr_chirho;

        if shdr_chirho.sh_type_chirho != SHT_NOBITS_CHIRHO {
            let src_off_chirho = shdr_chirho.sh_offset_chirho as usize;
            let size_chirho = shdr_chirho.sh_size_chirho as usize;
            if src_off_chirho + size_chirho > data_chirho.len() {
                return Err(KoErrorChirho::SectionOutOfBoundsChirho);
            }
            module_mem_guard_chirho.as_mut_slice_chirho()
                [dest_off_chirho..dest_off_chirho + size_chirho]
                .copy_from_slice(&data_chirho[src_off_chirho..src_off_chirho + size_chirho]);
        }
        // SHT_NOBITS (.bss) is already zeroed.
    }

    // -----------------------------------------------------------------------
    // 4. Apply relocations (A2-003).
    // -----------------------------------------------------------------------
    apply_relocations_chirho(
        module_mem_guard_chirho.as_mut_slice_chirho(),
        &section_addrs_chirho,
        &shdrs_chirho,
        data_chirho,
        symtab_idx_chirho,
        strtab_chirho,
    )?;

    crate::serial_println_chirho!("[KO] Relocations applied successfully");

    // -----------------------------------------------------------------------
    // 5. Find init_module / cleanup_module runtime addresses (A2-005).
    // -----------------------------------------------------------------------
    let sym_entsize_chirho = if symtab_shdr_chirho.sh_entsize_chirho > 0 {
        symtab_shdr_chirho.sh_entsize_chirho as usize
    } else {
        mem::size_of::<Elf64SymChirho>()
    };
    let symtab_off_chirho = symtab_shdr_chirho.sh_offset_chirho as usize;
    let num_syms_chirho =
        symtab_shdr_chirho.sh_size_chirho as usize / sym_entsize_chirho;

    let mut init_addr_chirho: Option<u64> = None;
    let mut cleanup_addr_chirho: Option<u64> = None;
    let mut symbol_count_chirho: usize = 0;
    let mut module_name_chirho = String::from("unknown");

    for sym_i_chirho in 0..num_syms_chirho {
        let off_chirho = symtab_off_chirho + sym_i_chirho * sym_entsize_chirho;
        let sym_chirho: Elf64SymChirho = unsafe {
            core::ptr::read_unaligned(
                data_chirho.as_ptr().add(off_chirho) as *const Elf64SymChirho,
            )
        };
        if sym_chirho.st_name_chirho == 0 {
            continue;
        }
        let name_chirho =
            read_str_chirho(strtab_chirho, sym_chirho.st_name_chirho as usize)
                .unwrap_or("<invalid>");

        let binding_chirho = elf64_st_bind_chirho(sym_chirho.st_info_chirho);
        let sym_type_chirho = elf64_st_type_chirho(sym_chirho.st_info_chirho);

        if sym_chirho.st_shndx_chirho != SHN_UNDEF_CHIRHO
            && (binding_chirho == STB_GLOBAL_CHIRHO || binding_chirho == STB_WEAK_CHIRHO)
        {
            symbol_count_chirho += 1;
        }

        // Compute runtime address for defined symbols.
        if sym_chirho.st_shndx_chirho != SHN_UNDEF_CHIRHO {
            let sec_idx_chirho = sym_chirho.st_shndx_chirho as usize;
            let runtime_val_chirho = if sec_idx_chirho < section_addrs_chirho.addrs_chirho.len() {
                section_addrs_chirho.addrs_chirho[sec_idx_chirho] + sym_chirho.st_value_chirho
            } else {
                sym_chirho.st_value_chirho
            };

            if name_chirho == "init_module" {
                init_addr_chirho = Some(runtime_val_chirho);
                crate::serial_println_chirho!(
                    "[KO] init_module runtime addr = {:#x}",
                    runtime_val_chirho
                );
            }
            if name_chirho == "cleanup_module" {
                cleanup_addr_chirho = Some(runtime_val_chirho);
                crate::serial_println_chirho!(
                    "[KO] cleanup_module runtime addr = {:#x}",
                    runtime_val_chirho
                );
            }

            if module_name_chirho == "unknown"
                && binding_chirho == STB_GLOBAL_CHIRHO
                && sym_type_chirho == STT_FUNC_CHIRHO
            {
                module_name_chirho = String::from(name_chirho);
            }
        }
    }

    // -----------------------------------------------------------------------
    // 6. Call init_module (A2-005).
    // -----------------------------------------------------------------------
    if let Some(init_chirho) = init_addr_chirho {
        let ret_chirho = unsafe { call_init_module_chirho(init_chirho) };
        if ret_chirho != 0 {
            crate::serial_println_chirho!(
                "[KO] init_module failed with code {}, module NOT loaded",
                ret_chirho
            );
            // Module init failed — do not add to loaded list.
            // Call cleanup if available.
            if let Some(cleanup_chirho) = cleanup_addr_chirho {
                unsafe { call_cleanup_module_chirho(cleanup_chirho) };
            }
            return Err(KoErrorChirho::NoInitSymbolChirho);
        }
    }

    let (mem_base_chirho, mem_size_chirho, arena_slot_chirho) =
        module_mem_guard_chirho.into_loaded_parts_chirho();

    crate::serial_println_chirho!(
        "[KO] Module '{}' loaded: {} symbols, mem={:#x}+{:#x}",
        module_name_chirho,
        symbol_count_chirho,
        mem_base_chirho,
        mem_size_chirho
    );

    Ok(KoModuleChirho {
        name_chirho: module_name_chirho,
        init_fn_chirho: init_addr_chirho,
        cleanup_fn_chirho: cleanup_addr_chirho,
        state_chirho: ModuleStateChirho::LoadedChirho,
        symbol_count_chirho,
        sections_chirho: sections_info_chirho,
        module_mem_base_chirho: mem_base_chirho,
        module_mem_size_chirho: mem_size_chirho,
        module_mem_arena_slot_chirho: Some(arena_slot_chirho),
        refcount_chirho: 0,
        depends_on_chirho: Vec::new(),
        params_chirho: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Syscall handlers — wire into sys_init_module / sys_delete_module
// ---------------------------------------------------------------------------

/// `init_module(2)` implementation — parse and load a .ko from user memory.
///
/// # Arguments
/// * `img_ptr_chirho`    — user-space pointer to the module ELF image.
/// * `len_chirho`        — length of the image in bytes.
/// * `_params_ptr_chirho` — pointer to parameter string (unused for now).
///
/// Returns 0 on success, negative errno on failure.
pub fn sys_init_module_impl_chirho(
    img_ptr_chirho: u64,
    len_chirho: u64,
    _params_ptr_chirho: u64,
) -> i64 {
    // Fix PML4[511] in the current per-process page table by reading the
    // boot PML4's entry (which has our fresh PDPT from arena init).
    // Use ONLY raw operations — no serial_println (mutex deadlock in syscall).
    {
        let off_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho().as_u64();
        let cr3_chirho: u64;
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3_chirho, options(nostack)); }
        let cur_pml4_chirho = cr3_chirho & 0x000F_FFFF_FFFF_F000;

        // Use the STORED PML4[511] from arena init (not boot PML4 which
        // might not have the arena mapping yet)
        let stored_511_chirho = crate::pagetable_chirho::module_arena_pml4_entry_chirho();
        if stored_511_chirho & 1 != 0 {
            unsafe {
                *((cur_pml4_chirho + off_chirho) as *mut u64).add(511) = stored_511_chirho;
                core::arch::asm!("mov rax, cr3; mov cr3, rax", out("rax") _, options(nostack));
                core::arch::asm!(
                    "invlpg [{}]",
                    in(reg) MODULE_ARENA_HIGH_BASE_CHIRHO,
                    options(nostack)
                );
            }
            // Verify the write and walk the page table chain
            let readback_chirho = unsafe {
                *((cur_pml4_chirho + off_chirho) as *const u64).add(511)
            };
            let pdpt_phys_chirho = readback_chirho & 0x000F_FFFF_FFFF_F000;
            // PDPT[511] for 0xFFFFFFFFC0000000 range
            let pdpte_chirho = unsafe {
                *((pdpt_phys_chirho + off_chirho) as *const u64).add(511)
            };
            let pd_phys_chirho = pdpte_chirho & 0x000F_FFFF_FFFF_F000;
            // Walk full chain: PD[0] and PT[256]
            let pde_chirho = if pd_phys_chirho != 0 {
                unsafe { *((pd_phys_chirho + off_chirho) as *const u64).add(0) }
            } else { 0 };
            let pt_phys_chirho = pde_chirho & 0x000F_FFFF_FFFF_F000;
            let pte_chirho = if pt_phys_chirho != 0 {
                unsafe { *((pt_phys_chirho + off_chirho) as *const u64).add(256) }
            } else { 0 };
            crate::serial_println_chirho!(
                "[KO] PT chain: PML4={:#x} PDPT[511]={:#x} PD[0]={:#x} PT[256]={:#x}",
                readback_chirho, pdpte_chirho, pde_chirho, pte_chirho,
            );
        }
    }

    crate::serial_println_chirho!(
        "[KO] sys_init_module: img_ptr={:#x}, len={}",
        img_ptr_chirho,
        len_chirho
    );

    // Sanity checks.
    if len_chirho == 0 || len_chirho > 16 * 1024 * 1024 {
        return -EINVAL_CHIRHO;
    }

    // Copy the module image into a kernel buffer.
    let len_usize_chirho = len_chirho as usize;
    let is_kernel_ptr_chirho = img_ptr_chirho >= 0x8000_0000_0000
        || (img_ptr_chirho >= 0x4444_0000_0000 && img_ptr_chirho < 0x5555_0000_0000);
    crate::serial_println_chirho!(
        "[KO] copying {} bytes, kernel_ptr={}",
        len_usize_chirho, is_kernel_ptr_chirho,
    );
    let buf_chirho: Vec<u8> = if is_kernel_ptr_chirho {
        // Called from finit_module — pointer is already in kernel heap.
        let src_chirho = unsafe {
            core::slice::from_raw_parts(img_ptr_chirho as *const u8, len_usize_chirho)
        };
        let v_chirho = src_chirho.to_vec();
        crate::serial_println_chirho!("[KO] kernel copy done, {} bytes", v_chirho.len());
        v_chirho
    } else {
        // Called from init_module — pointer is in user space.
        let mut user_buf_chirho = Vec::with_capacity(len_usize_chirho);
        unsafe { user_buf_chirho.set_len(len_usize_chirho); }
        match uaccess_chirho::copy_from_user_chirho(
            &mut user_buf_chirho[..],
            img_ptr_chirho,
            len_usize_chirho,
        ) {
            Ok(()) => user_buf_chirho,
            Err(_) => {
                crate::serial_println_chirho!(
                    "[KO] sys_init_module: failed to copy {} bytes from {:#x}",
                    len_chirho, img_ptr_chirho
                );
                return -EFAULT_CHIRHO;
            }
        }
    };

    // Full loading pipeline: parse -> allocate -> relocate -> init.
    let module_chirho = match load_and_init_module_chirho(&buf_chirho) {
        Ok(m_chirho) => m_chirho,
        Err(err_chirho) => {
            crate::serial_println_chirho!(
                "[KO] sys_init_module: load failed: {:?}",
                err_chirho
            );
            return err_chirho.to_errno_chirho();
        }
    };

    crate::serial_println_chirho!(
        "[KO] Module '{}' loaded successfully ({} symbols)",
        module_chirho.name_chirho,
        module_chirho.symbol_count_chirho
    );

    let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();

    // Check for duplicate module names.
    for existing_chirho in loaded_chirho.iter() {
        if existing_chirho.name_chirho == module_chirho.name_chirho
            && existing_chirho.state_chirho == ModuleStateChirho::LoadedChirho
        {
            crate::serial_println_chirho!(
                "[KO] Module '{}' already loaded",
                module_chirho.name_chirho
            );
            return -EBUSY_CHIRHO;
        }
    }

    loaded_chirho.push(module_chirho);

    crate::serial_println_chirho!(
        "[KO] Module loaded, total modules: {}",
        loaded_chirho.len()
    );

    0 // Success
}

/// `delete_module(2)` implementation — unload a kernel module by name.
///
/// # Arguments
/// * `name_ptr_chirho` — user-space pointer to the null-terminated module name.
/// * `_flags_chirho`   — flags (O_NONBLOCK etc., unused for now).
///
/// Returns 0 on success, negative errno on failure.
pub fn sys_delete_module_impl_chirho(name_ptr_chirho: u64, _flags_chirho: u64) -> i64 {
    // Read the module name from user space.
    let name_chirho = match uaccess_chirho::read_user_string_chirho(name_ptr_chirho, 256) {
        Ok(s_chirho) => s_chirho,
        Err(_) => {
            crate::serial_println_chirho!(
                "[KO] sys_delete_module: failed to read name from {:#x}",
                name_ptr_chirho
            );
            return -EFAULT_CHIRHO;
        }
    };

    crate::serial_println_chirho!("[KO] sys_delete_module: name='{}'", name_chirho);

    let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();

    // Find the module.
    let mut found_idx_chirho: Option<usize> = None;
    for (idx_chirho, module_chirho) in loaded_chirho.iter().enumerate() {
        if module_chirho.name_chirho == name_chirho
            && module_chirho.state_chirho == ModuleStateChirho::LoadedChirho
        {
            found_idx_chirho = Some(idx_chirho);
            break;
        }
    }

    let idx_chirho = match found_idx_chirho {
        Some(i_chirho) => i_chirho,
        None => {
            crate::serial_println_chirho!(
                "[KO] sys_delete_module: module '{}' not found",
                name_chirho
            );
            return -ENOENT_CHIRHO;
        }
    };

    // A2-016: Check refcount — cannot unload if other modules depend on us.
    if loaded_chirho[idx_chirho].refcount_chirho > 0 {
        crate::serial_println_chirho!(
            "[KO] sys_delete_module: module '{}' still has {} dependents",
            name_chirho,
            loaded_chirho[idx_chirho].refcount_chirho
        );
        return -EBUSY_CHIRHO;
    }

    // A2-016: Decrement refcounts of modules this one depends on.
    let deps_chirho = loaded_chirho[idx_chirho].depends_on_chirho.clone();
    for dep_name_chirho in &deps_chirho {
        for other_chirho in loaded_chirho.iter_mut() {
            if other_chirho.name_chirho == *dep_name_chirho && other_chirho.refcount_chirho > 0 {
                other_chirho.refcount_chirho -= 1;
            }
        }
    }

    // A2-005: Call cleanup_module if present.
    if let Some(cleanup_addr_chirho) = loaded_chirho[idx_chirho].cleanup_fn_chirho {
        unsafe {
            call_cleanup_module_chirho(cleanup_addr_chirho);
        }
    }

    // Reclaim module memory.
    let mem_base_chirho = loaded_chirho[idx_chirho].module_mem_base_chirho;
    let mem_size_chirho = loaded_chirho[idx_chirho].module_mem_size_chirho;
    let arena_slot_chirho = loaded_chirho[idx_chirho].module_mem_arena_slot_chirho;
    if mem_base_chirho != 0 && mem_size_chirho > 0 {
        if let Some(arena_slot_index_chirho) = arena_slot_chirho {
            release_module_image_slot_chirho(arena_slot_index_chirho);
        } else {
            // SAFETY: Legacy heap-backed module images were allocated as Vecs
            // and leaked. Reconstruct and drop to free them.
            unsafe {
                let _ = Vec::from_raw_parts(
                    mem_base_chirho as *mut u8,
                    mem_size_chirho,
                    mem_size_chirho,
                );
            }
        }
        crate::serial_println_chirho!(
            "[KO] Freed {} bytes of module memory at {:#x}",
            mem_size_chirho,
            mem_base_chirho
        );
    }

    loaded_chirho[idx_chirho].state_chirho = ModuleStateChirho::UnloadedChirho;
    loaded_chirho[idx_chirho].module_mem_base_chirho = 0;
    loaded_chirho[idx_chirho].module_mem_size_chirho = 0;
    loaded_chirho[idx_chirho].module_mem_arena_slot_chirho = None;

    crate::serial_println_chirho!(
        "[KO] Module '{}' unloaded",
        name_chirho
    );

    0 // Success
}

// ===========================================================================
// Module dependency resolution (modprobe-style)
// ===========================================================================

/// Module dependency entry: a module name and its required dependencies.
#[derive(Clone)]
pub struct ModDepEntryChirho {
    /// Module name (e.g., "virtio_blk").
    pub name_chirho: String,
    /// Filesystem path to the .ko file.
    pub path_chirho: String,
    /// List of dependency module names (must be loaded first).
    pub deps_chirho: Vec<String>,
}

/// In-memory module dependency database — equivalent to `modules.dep`.
/// Populated by scanning `/lib/modules/<version>/` or manually.
static MODULE_DEPS_CHIRHO: Mutex<Vec<ModDepEntryChirho>> = Mutex::new(Vec::new());

/// Register a module dependency entry (e.g., from parsing modules.dep).
pub fn register_moddep_chirho(entry_chirho: ModDepEntryChirho) {
    MODULE_DEPS_CHIRHO.lock().push(entry_chirho);
}

/// Resolve and load a module and all its dependencies (modprobe semantics).
///
/// Recursively loads dependencies before loading the requested module.
/// Returns 0 on success, negative errno on failure.
pub fn modprobe_chirho(name_chirho: &str) -> i64 {
    crate::serial_println_chirho!("[MODPROBE] Loading module '{}'...", name_chirho);

    // Check if already loaded.
    {
        let loaded_chirho = LOADED_MODULES_CHIRHO.lock();
        for m_chirho in loaded_chirho.iter() {
            if m_chirho.name_chirho == name_chirho
                && m_chirho.state_chirho == ModuleStateChirho::LoadedChirho
            {
                crate::serial_println_chirho!(
                    "[MODPROBE] '{}' already loaded",
                    name_chirho
                );
                return 0;
            }
        }
    }

    // Look up in dependency database.
    let entry_chirho = {
        let deps_chirho = MODULE_DEPS_CHIRHO.lock();
        deps_chirho
            .iter()
            .find(|e_chirho| e_chirho.name_chirho == name_chirho)
            .cloned()
    };

    if let Some(dep_entry_chirho) = entry_chirho {
        // Load dependencies first (recursive).
        for dep_name_chirho in &dep_entry_chirho.deps_chirho {
            let result_chirho = modprobe_chirho(dep_name_chirho);
            if result_chirho != 0 {
                crate::serial_println_chirho!(
                    "[MODPROBE] Failed to load dependency '{}' for '{}'",
                    dep_name_chirho,
                    name_chirho
                );
                return result_chirho;
            }
        }

        // Load the .ko file from the filesystem path.
        crate::serial_println_chirho!(
            "[MODPROBE] Loading '{}' from '{}'",
            name_chirho,
            dep_entry_chirho.path_chirho
        );

        match crate::process_chirho::try_read_file_pub_chirho(&dep_entry_chirho.path_chirho) {
            Some(ko_data_chirho) => {
                let result_chirho = load_and_init_module_chirho(&ko_data_chirho);
                match result_chirho {
                    Ok(module_chirho) => {
                        let mut loaded_chirho = LOADED_MODULES_CHIRHO.lock();
                        loaded_chirho.push(module_chirho);
                        crate::serial_println_chirho!(
                            "[MODPROBE] '{}' loaded successfully",
                            name_chirho
                        );
                        0
                    }
                    Err(err_chirho) => {
                        crate::serial_println_chirho!(
                            "[MODPROBE] Failed to load '{}': {:?}",
                            name_chirho,
                            err_chirho
                        );
                        err_chirho.to_errno_chirho()
                    }
                }
            }
            None => {
                crate::serial_println_chirho!(
                    "[MODPROBE] Could not read '{}' from filesystem",
                    dep_entry_chirho.path_chirho
                );
                -ENOENT_CHIRHO
            }
        }
    } else {
        crate::serial_println_chirho!(
            "[MODPROBE] Module '{}' not found in dependency database",
            name_chirho
        );
        -ENOENT_CHIRHO
    }
}

/// Parse a `modules.dep` file from the filesystem and populate the
/// dependency database.
///
/// Format: `path/to/module.ko: dep1.ko dep2.ko ...`
pub fn parse_modules_dep_chirho(content_chirho: &str) {
    for line_chirho in content_chirho.lines() {
        let line_chirho = line_chirho.trim();
        if line_chirho.is_empty() || line_chirho.starts_with('#') {
            continue;
        }
        if let Some(colon_idx_chirho) = line_chirho.find(':') {
            let path_chirho = line_chirho[..colon_idx_chirho].trim();
            let deps_str_chirho = line_chirho[colon_idx_chirho + 1..].trim();

            // Extract module name from path (e.g., "virtio_blk.ko" from path).
            let name_chirho = path_chirho
                .rsplit('/')
                .next()
                .unwrap_or(path_chirho)
                .trim_end_matches(".ko");

            let deps_chirho: Vec<String> = if deps_str_chirho.is_empty() {
                Vec::new()
            } else {
                deps_str_chirho
                    .split_whitespace()
                    .map(|d_chirho| {
                        String::from(
                            d_chirho
                                .rsplit('/')
                                .next()
                                .unwrap_or(d_chirho)
                                .trim_end_matches(".ko"),
                        )
                    })
                    .collect()
            };

            register_moddep_chirho(ModDepEntryChirho {
                name_chirho: String::from(name_chirho),
                path_chirho: String::from(path_chirho),
                deps_chirho,
            });
        }
    }
}
