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
        for entry_chirho in KERNEL_SYMBOLS_CHIRHO.iter() {
            if entry_chirho.name_chirho == name_chirho {
                if entry_chirho.addr_chirho != 0 {
                    return Some(entry_chirho.addr_chirho);
                }
                // Address not yet resolved — fall through to dynamic table.
            }
        }

        // Check dynamically registered symbols.
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

    crate::serial_println_chirho!(
        "[KO] Kernel symbol table ready ({} symbols)",
        KernelSymbolTableChirho::symbol_count_chirho()
    );
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
                // For weak symbols, resolve to 0 instead of failing.
                let binding_chirho =
                    elf64_st_bind_chirho(sym_chirho.st_info_chirho);
                if binding_chirho == STB_WEAK_CHIRHO {
                    Ok(0)
                } else {
                    Err(KoErrorChirho::UnresolvedSymbolChirho)
                }
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
                    "[KO] R_X86_64_32 overflow: value={:#x}",
                    val_chirho
                );
                return Err(KoErrorChirho::RelocationOverflowChirho);
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
                crate::serial_println_chirho!(
                    "[KO] R_X86_64_32S overflow: value={:#x}",
                    val_chirho
                );
                return Err(KoErrorChirho::RelocationOverflowChirho);
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
        R_X86_64_GOTPCREL_CHIRHO => {
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
unsafe fn call_init_module_chirho(addr_chirho: u64) -> i32 {
    crate::serial_println_chirho!(
        "[KO] Calling init_module at {:#x}",
        addr_chirho
    );
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
        });
    }

    // Allocate a contiguous block from the kernel heap.
    let mut module_mem_chirho: Vec<u8> = Vec::new();
    module_mem_chirho
        .try_reserve(total_size_chirho)
        .map_err(|_| KoErrorChirho::OutOfMemoryChirho)?;
    module_mem_chirho.resize(total_size_chirho, 0u8);

    let mem_base_chirho = module_mem_chirho.as_ptr() as u64;

    crate::serial_println_chirho!(
        "[KO] Allocated {} bytes for module at {:#x}",
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
            module_mem_chirho[dest_off_chirho..dest_off_chirho + size_chirho]
                .copy_from_slice(&data_chirho[src_off_chirho..src_off_chirho + size_chirho]);
        }
        // SHT_NOBITS (.bss) is already zeroed.
    }

    // -----------------------------------------------------------------------
    // 4. Apply relocations (A2-003).
    // -----------------------------------------------------------------------
    apply_relocations_chirho(
        &mut module_mem_chirho,
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

    // Leak the Vec so that the module memory stays alive.  We store the
    // base + size so it can be reclaimed on unload.
    let mem_ptr_chirho = module_mem_chirho.as_mut_ptr();
    let mem_len_chirho = module_mem_chirho.len();
    let mem_cap_chirho = module_mem_chirho.capacity();
    mem::forget(module_mem_chirho);

    crate::serial_println_chirho!(
        "[KO] Module '{}' loaded: {} symbols, mem={:#x}+{:#x}",
        module_name_chirho,
        symbol_count_chirho,
        mem_ptr_chirho as u64,
        mem_len_chirho
    );

    Ok(KoModuleChirho {
        name_chirho: module_name_chirho,
        init_fn_chirho: init_addr_chirho,
        cleanup_fn_chirho: cleanup_addr_chirho,
        state_chirho: ModuleStateChirho::LoadedChirho,
        symbol_count_chirho,
        sections_chirho: sections_info_chirho,
        module_mem_base_chirho: mem_ptr_chirho as u64,
        module_mem_size_chirho: mem_cap_chirho,
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
    crate::serial_println_chirho!(
        "[KO] sys_init_module: img_ptr={:#x}, len={}",
        img_ptr_chirho,
        len_chirho
    );

    // Sanity checks.
    if len_chirho == 0 || len_chirho > 16 * 1024 * 1024 {
        // Reject zero-length or absurdly large images (> 16 MiB).
        return -EINVAL_CHIRHO;
    }

    // Copy the module image from user space into a kernel buffer.
    let len_usize_chirho = len_chirho as usize;
    let mut buf_chirho: Vec<u8> = Vec::with_capacity(len_usize_chirho);

    // SAFETY: We are about to copy user memory into this buffer.
    // The Vec has capacity but length 0; we set length after copy.
    unsafe {
        buf_chirho.set_len(len_usize_chirho);
    }

    // Use copy_from_user to safely read from user space.
    match uaccess_chirho::copy_from_user_chirho(
        &mut buf_chirho[..],
        img_ptr_chirho,
        len_usize_chirho,
    ) {
        Ok(()) => {}
        Err(_) => {
            crate::serial_println_chirho!(
                "[KO] sys_init_module: failed to copy {} bytes from user {:#x}",
                len_chirho,
                img_ptr_chirho
            );
            return -EFAULT_CHIRHO;
        }
    }

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

    // A2-005: Call cleanup_module if present.
    if let Some(cleanup_addr_chirho) = loaded_chirho[idx_chirho].cleanup_fn_chirho {
        unsafe {
            call_cleanup_module_chirho(cleanup_addr_chirho);
        }
    }

    // Reclaim module memory.
    let mem_base_chirho = loaded_chirho[idx_chirho].module_mem_base_chirho;
    let mem_size_chirho = loaded_chirho[idx_chirho].module_mem_size_chirho;
    if mem_base_chirho != 0 && mem_size_chirho > 0 {
        // SAFETY: We allocated this Vec in load_and_init_module_chirho and
        // called mem::forget on it.  Reconstruct and drop to free.
        unsafe {
            let _ = Vec::from_raw_parts(
                mem_base_chirho as *mut u8,
                mem_size_chirho,
                mem_size_chirho,
            );
        }
        crate::serial_println_chirho!(
            "[KO] Freed {} bytes of module memory at {:#x}",
            mem_size_chirho,
            mem_base_chirho
        );
    }

    loaded_chirho[idx_chirho].state_chirho = ModuleStateChirho::UnloadedChirho;

    crate::serial_println_chirho!(
        "[KO] Module '{}' unloaded",
        name_chirho
    );

    0 // Success
}
