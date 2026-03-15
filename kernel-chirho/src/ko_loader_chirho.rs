// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux `.ko` kernel module loader for the Lineluya kernel.
//!
//! Parses ELF relocatable objects (`ET_REL`) produced by the Linux kernel build
//! system, resolves symbols against the kernel symbol table, and manages the
//! lifecycle of loaded modules (init / cleanup).
//!
//! This is the Phase A2 foundation — parsing, symbol lookup, and module
//! bookkeeping.  Actual relocation patching and code execution will land in a
//! follow-up phase.

extern crate alloc;

use alloc::string::String;
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
const SHT_PROGBITS_CHIRHO: u32 = 1;

/// Symbol table.
const SHT_SYMTAB_CHIRHO: u32 = 2;

/// String table.
const SHT_STRTAB_CHIRHO: u32 = 3;

/// Relocation entries with explicit addends.
#[allow(dead_code)]
const SHT_RELA_CHIRHO: u32 = 4;

/// Section contains no data (BSS).
const SHT_NOBITS_CHIRHO: u32 = 8;

// -- Section header flags ---------------------------------------------------

/// Section contains writable data.
const SHF_WRITE_CHIRHO: u64 = 0x1;

/// Section occupies memory during execution.
const SHF_ALLOC_CHIRHO: u64 = 0x2;

/// Section contains executable instructions.
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
const STT_NOTYPE_CHIRHO: u8 = 0;

/// Undefined section index.
const SHN_UNDEF_CHIRHO: u16 = 0;

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
            | KoErrorChirho::SectionOutOfBoundsChirho => -ENOEXEC_CHIRHO,
            KoErrorChirho::NoInitSymbolChirho => -EINVAL_CHIRHO,
            KoErrorChirho::OutOfMemoryChirho => -ENOMEM_CHIRHO,
        }
    }
}

// ---------------------------------------------------------------------------
// Kernel Symbol Table — maps C names to kernel addresses
// ---------------------------------------------------------------------------

/// A single exported kernel symbol.
struct KernelSymbolEntryChirho {
    /// C-compatible name (e.g. "printk").
    name_chirho: &'static str,
    /// Virtual address of the kernel function / object.
    addr_chirho: u64,
}

/// Placeholder addresses for kernel symbols.  In a real implementation these
/// would be filled in by the linker or at boot time.  For now they point to
/// the serial_println stub (printk) and the allocator wrappers.
///
/// We use function pointers cast to u64.
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
}

/// Dynamically registered kernel symbols (populated at runtime).
static DYNAMIC_SYMBOLS_CHIRHO: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Global loaded-module list
// ---------------------------------------------------------------------------

/// List of all currently loaded kernel modules.
pub static LOADED_MODULES_CHIRHO: Mutex<Vec<KoModuleChirho>> = Mutex::new(Vec::new());

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

    // Parse the ELF relocatable object.
    let module_chirho = match parse_ko_elf_chirho(&buf_chirho) {
        Ok(m_chirho) => m_chirho,
        Err(err_chirho) => {
            crate::serial_println_chirho!(
                "[KO] sys_init_module: parse failed: {:?}",
                err_chirho
            );
            return err_chirho.to_errno_chirho();
        }
    };

    crate::serial_println_chirho!(
        "[KO] Module '{}' parsed successfully ({} symbols)",
        module_chirho.name_chirho,
        module_chirho.symbol_count_chirho
    );

    // TODO (Phase A3): Perform relocations, allocate module memory, call init_fn.
    // For now, record the module as loaded.
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

    let mut module_chirho = module_chirho;
    module_chirho.state_chirho = ModuleStateChirho::LoadedChirho;
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

    // TODO (Phase A3): Call cleanup_fn if present, free module memory.
    // For now, just mark as unloaded.
    loaded_chirho[idx_chirho].state_chirho = ModuleStateChirho::UnloadedChirho;

    crate::serial_println_chirho!(
        "[KO] Module '{}' unloaded (cleanup_fn present: {})",
        name_chirho,
        loaded_chirho[idx_chirho].cleanup_fn_chirho.is_some()
    );

    0 // Success
}
