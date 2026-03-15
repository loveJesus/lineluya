// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Dynamic ELF loader for the Lineluya kernel (A6-001).
//!
//! Provides support for loading dynamically linked ELF binaries:
//! - Parse `ET_DYN` (shared object / PIE) ELF binaries
//! - Extract `.dynamic` section entries (`DT_NEEDED`, `DT_STRTAB`, `DT_SYMTAB`,
//!   `DT_RELA`, `DT_JMPREL`, etc.)
//! - PLT/GOT lazy binding support structures
//! - Load the ELF interpreter specified by `PT_INTERP`
//! - Populate `AT_BASE`, `AT_ENTRY`, `AT_PHDR` auxiliary vector entries for the
//!   dynamic linker
//!
//! This module works alongside [`crate::elf_chirho`] (ELF parsing) and
//! [`crate::exec_chirho`] (userspace entry) to enable running dynamically linked
//! programs such as those built against glibc or musl with shared libraries.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::mem;

use crate::elf_chirho::{
    self, Elf64HeaderChirho, Elf64PhdrChirho, ElfInfoChirho, ElfSegmentChirho,
    ET_DYN_CHIRHO, PT_INTERP_CHIRHO, PT_LOAD_CHIRHO, PT_PHDR_CHIRHO,
    PF_R_CHIRHO, PF_W_CHIRHO, PF_X_CHIRHO,
};
use crate::exec_chirho::LoadedElfChirho;
use crate::mm_chirho::{
    self, MmChirho, MAP_ANONYMOUS_CHIRHO, MAP_FIXED_CHIRHO, MAP_PRIVATE_CHIRHO,
    PROT_EXEC_CHIRHO, PROT_READ_CHIRHO, PROT_WRITE_CHIRHO,
};
use crate::serial_println_chirho;

// ============================================================================
// Constants
// ============================================================================

/// Page size (4 KiB).
const PAGE_SIZE_CHIRHO: u64 = 4096;

/// Default base address for loading PIE executables and the dynamic linker.
/// This sits below the typical mmap region to avoid collisions.
const PIE_LOAD_BASE_CHIRHO: u64 = 0x5555_5555_0000;

/// Default base address for loading the dynamic linker (interpreter).
/// Placed above the PIE base but well below the stack.
const INTERP_LOAD_BASE_CHIRHO: u64 = 0x7F00_0010_0000;

// ---------------------------------------------------------------------------
// Program header segment type: PT_DYNAMIC
// ---------------------------------------------------------------------------

/// Dynamic linking information segment.
pub const PT_DYNAMIC_CHIRHO: u32 = 2;

// ---------------------------------------------------------------------------
// Dynamic section tag constants (from ELF spec / elf.h)
// ---------------------------------------------------------------------------

/// Marks the end of the `_DYNAMIC` array.
pub const DT_NULL_CHIRHO: u64 = 0;

/// String table offset of a needed shared library name.
pub const DT_NEEDED_CHIRHO: u64 = 1;

/// Size in bytes of each PLT relocation entry.
pub const DT_PLTRELSZ_CHIRHO: u64 = 2;

/// Address of the PLT and/or GOT.
pub const DT_PLTGOT_CHIRHO: u64 = 3;

/// Address of the symbol hash table.
pub const DT_HASH_CHIRHO: u64 = 4;

/// Address of the string table.
pub const DT_STRTAB_CHIRHO: u64 = 5;

/// Address of the symbol table.
pub const DT_SYMTAB_CHIRHO: u64 = 6;

/// Address of the `Rela` relocation table.
pub const DT_RELA_CHIRHO: u64 = 7;

/// Total size in bytes of the `DT_RELA` relocation table.
pub const DT_RELASZ_CHIRHO: u64 = 8;

/// Size of each `Rela` entry.
pub const DT_RELAENT_CHIRHO: u64 = 9;

/// Total size of the string table.
pub const DT_STRSZ_CHIRHO: u64 = 10;

/// Size of each symbol table entry.
pub const DT_SYMENT_CHIRHO: u64 = 11;

/// Address of the init function.
pub const DT_INIT_CHIRHO: u64 = 12;

/// Address of the fini function.
pub const DT_FINI_CHIRHO: u64 = 13;

/// Address of the PLT relocation table.
pub const DT_JMPREL_CHIRHO: u64 = 23;

/// Type of PLT relocations (DT_RELA or DT_REL).
pub const DT_PLTREL_CHIRHO: u64 = 20;

/// GNU hash table address.
pub const DT_GNU_HASH_CHIRHO: u64 = 0x6ffffef5;

// ---------------------------------------------------------------------------
// Relocation type constants (x86_64)
// ---------------------------------------------------------------------------

/// R_X86_64_RELATIVE — Base + addend.
pub const R_X86_64_RELATIVE_CHIRHO: u32 = 8;

/// R_X86_64_JUMP_SLOT — PLT entry relocation.
pub const R_X86_64_JUMP_SLOT_CHIRHO: u32 = 7;

/// R_X86_64_GLOB_DAT — GOT entry for a global symbol.
pub const R_X86_64_GLOB_DAT_CHIRHO: u32 = 6;

/// R_X86_64_64 — Direct 64-bit relocation.
pub const R_X86_64_64_CHIRHO: u32 = 1;

// ---------------------------------------------------------------------------
// Auxiliary vector types not yet in elf_chirho.rs
// ---------------------------------------------------------------------------

/// Base address at which the interpreter (dynamic linker) was loaded.
pub const AT_BASE_CHIRHO: u64 = 7;

// ============================================================================
// ELF64 dynamic entry (Elf64_Dyn)
// ============================================================================

/// 64-bit ELF dynamic section entry, corresponding to `Elf64_Dyn`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64DynChirho {
    /// Dynamic entry tag (`DT_NEEDED`, `DT_STRTAB`, etc.).
    pub d_tag_chirho: i64,
    /// Value or address associated with this tag.
    pub d_val_chirho: u64,
}

// ============================================================================
// ELF64 symbol table entry (Elf64_Sym)
// ============================================================================

/// 64-bit ELF symbol table entry, corresponding to `Elf64_Sym`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64SymChirho {
    /// Index into the string table for the symbol name.
    pub st_name_chirho: u32,
    /// Symbol type and binding attributes.
    pub st_info_chirho: u8,
    /// Symbol visibility.
    pub st_other_chirho: u8,
    /// Section header index this symbol is associated with.
    pub st_shndx_chirho: u16,
    /// Symbol value (address).
    pub st_value_chirho: u64,
    /// Symbol size in bytes.
    pub st_size_chirho: u64,
}

// ============================================================================
// ELF64 relocation entry with addend (Elf64_Rela)
// ============================================================================

/// 64-bit ELF relocation entry with explicit addend, corresponding to
/// `Elf64_Rela`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64RelaChirho {
    /// Address at which to apply the relocation.
    pub r_offset_chirho: u64,
    /// Relocation type and symbol index packed together.
    /// Type = low 32 bits, symbol index = high 32 bits.
    pub r_info_chirho: u64,
    /// Addend for computing the relocation value.
    pub r_addend_chirho: i64,
}

impl Elf64RelaChirho {
    /// Extract the relocation type (low 32 bits of `r_info`).
    pub fn rela_type_chirho(&self) -> u32 {
        (self.r_info_chirho & 0xFFFF_FFFF) as u32
    }

    /// Extract the symbol table index (high 32 bits of `r_info`).
    pub fn rela_sym_chirho(&self) -> u32 {
        (self.r_info_chirho >> 32) as u32
    }
}

// ============================================================================
// DynamicInfoChirho — parsed .dynamic section
// ============================================================================

/// Parsed information from the ELF `.dynamic` section.
///
/// Contains resolved addresses for the string table, symbol table,
/// relocation tables, and the list of needed shared libraries.
#[derive(Debug, Clone)]
pub struct DynamicInfoChirho {
    /// Address of the string table in memory.
    pub strtab_addr_chirho: u64,
    /// Size of the string table.
    pub strtab_size_chirho: u64,
    /// Address of the symbol table in memory.
    pub symtab_addr_chirho: u64,
    /// Size of each symbol table entry.
    pub syment_size_chirho: u64,
    /// Address of the RELA relocation table.
    pub rela_addr_chirho: u64,
    /// Total size of the RELA table in bytes.
    pub rela_size_chirho: u64,
    /// Size of each RELA entry.
    pub relaent_size_chirho: u64,
    /// Address of the PLT relocation table (JMPREL).
    pub jmprel_addr_chirho: u64,
    /// Size of the PLT relocation table.
    pub pltrelsz_chirho: u64,
    /// Type of PLT relocations (7 = RELA, 17 = REL).
    pub pltrel_type_chirho: u64,
    /// Address of the PLT/GOT.
    pub pltgot_addr_chirho: u64,
    /// Address of the hash table.
    pub hash_addr_chirho: u64,
    /// Address of the GNU hash table.
    pub gnu_hash_addr_chirho: u64,
    /// Address of the init function.
    pub init_addr_chirho: u64,
    /// Address of the fini function.
    pub fini_addr_chirho: u64,
    /// List of needed library name offsets into the string table.
    pub needed_offsets_chirho: Vec<u64>,
}

impl DynamicInfoChirho {
    /// Create a new empty `DynamicInfoChirho`.
    pub fn new_chirho() -> Self {
        Self {
            strtab_addr_chirho: 0,
            strtab_size_chirho: 0,
            symtab_addr_chirho: 0,
            syment_size_chirho: 0,
            rela_addr_chirho: 0,
            rela_size_chirho: 0,
            relaent_size_chirho: 0,
            jmprel_addr_chirho: 0,
            pltrelsz_chirho: 0,
            pltrel_type_chirho: 0,
            pltgot_addr_chirho: 0,
            hash_addr_chirho: 0,
            gnu_hash_addr_chirho: 0,
            init_addr_chirho: 0,
            fini_addr_chirho: 0,
            needed_offsets_chirho: Vec::new(),
        }
    }
}

// ============================================================================
// DynLoadResultChirho — result of loading a dynamic ELF
// ============================================================================

/// Result of loading a dynamically linked ELF binary and its interpreter.
#[derive(Debug)]
pub struct DynLoadResultChirho {
    /// Information about the main executable.
    pub exe_loaded_chirho: LoadedElfChirho,
    /// The actual entry point to jump to (interpreter entry if present,
    /// otherwise the executable's own entry point).
    pub start_addr_chirho: u64,
    /// Base address at which the interpreter was loaded (0 if no interpreter).
    pub interp_base_chirho: u64,
    /// Parsed dynamic section information from the main executable.
    pub dynamic_info_chirho: Option<DynamicInfoChirho>,
    /// Path of the interpreter (from PT_INTERP), if any.
    pub interp_path_chirho: Option<String>,
}

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur during dynamic ELF loading.
#[derive(Debug)]
pub enum DynlinkErrorChirho {
    /// ELF parsing failed.
    ElfParseErrorChirho(&'static str),
    /// Memory mapping failed.
    MmapErrorChirho(i64),
    /// The PT_INTERP path could not be read.
    InterpPathErrorChirho,
    /// The .dynamic section could not be parsed.
    DynamicParseErrorChirho(&'static str),
    /// No PT_LOAD segments found.
    NoSegmentsChirho,
    /// Interpreter binary not found or could not be loaded.
    InterpLoadErrorChirho(&'static str),
}

// ============================================================================
// Core functions
// ============================================================================

/// Extract the interpreter path from a `PT_INTERP` segment.
///
/// Reads the NUL-terminated path string from the ELF binary data at the
/// offset and size specified by the PT_INTERP program header.
///
/// Returns `None` if the path cannot be extracted or is not valid UTF-8.
pub fn extract_interp_path_chirho(
    elf_data_chirho: &[u8],
    phdr_chirho: &Elf64PhdrChirho,
) -> Option<String> {
    if phdr_chirho.p_type_chirho != PT_INTERP_CHIRHO {
        return None;
    }

    let offset_chirho = phdr_chirho.p_offset_chirho as usize;
    let size_chirho = phdr_chirho.p_filesz_chirho as usize;

    if offset_chirho + size_chirho > elf_data_chirho.len() {
        return None;
    }

    let interp_bytes_chirho = &elf_data_chirho[offset_chirho..offset_chirho + size_chirho];

    // Strip trailing NUL byte(s).
    let trimmed_chirho = if let Some(nul_pos_chirho) = interp_bytes_chirho.iter().position(|&b_chirho| b_chirho == 0) {
        &interp_bytes_chirho[..nul_pos_chirho]
    } else {
        interp_bytes_chirho
    };

    core::str::from_utf8(trimmed_chirho).ok().map(String::from)
}

/// Parse the `.dynamic` section from a loaded ELF binary.
///
/// The dynamic section is located via the `PT_DYNAMIC` program header.
/// All addresses in the returned [`DynamicInfoChirho`] are the raw virtual
/// addresses from the ELF, which should be adjusted by `load_bias_chirho`
/// for PIE/ET_DYN binaries.
///
/// # Arguments
///
/// * `elf_data_chirho` — Raw ELF binary bytes.
/// * `load_bias_chirho` — Offset added to all ELF vaddrs when loaded into
///   memory (0 for ET_EXEC, base address for ET_DYN/PIE).
pub fn parse_dynamic_section_chirho(
    elf_data_chirho: &[u8],
    load_bias_chirho: u64,
) -> Result<DynamicInfoChirho, DynlinkErrorChirho> {
    let header_chirho = read_header_unaligned_chirho(elf_data_chirho)
        .ok_or(DynlinkErrorChirho::ElfParseErrorChirho("header too small"))?;

    let phoff_chirho = header_chirho.e_phoff_chirho as usize;
    let phentsize_chirho = header_chirho.e_phentsize_chirho as usize;
    let phnum_chirho = header_chirho.e_phnum_chirho as usize;

    // Find PT_DYNAMIC segment.
    let mut dyn_offset_chirho: Option<usize> = None;
    let mut dyn_size_chirho: u64 = 0;

    for idx_chirho in 0..phnum_chirho {
        let off_chirho = phoff_chirho + idx_chirho * phentsize_chirho;
        if off_chirho + mem::size_of::<Elf64PhdrChirho>() > elf_data_chirho.len() {
            continue;
        }
        let phdr_chirho = unsafe {
            core::ptr::read_unaligned(
                elf_data_chirho.as_ptr().add(off_chirho) as *const Elf64PhdrChirho,
            )
        };
        if phdr_chirho.p_type_chirho == PT_DYNAMIC_CHIRHO {
            dyn_offset_chirho = Some(phdr_chirho.p_offset_chirho as usize);
            dyn_size_chirho = phdr_chirho.p_filesz_chirho;
            break;
        }
    }

    let dyn_off_chirho = dyn_offset_chirho
        .ok_or(DynlinkErrorChirho::DynamicParseErrorChirho("no PT_DYNAMIC"))?;

    let mut info_chirho = DynamicInfoChirho::new_chirho();
    let dyn_entry_size_chirho = mem::size_of::<Elf64DynChirho>();
    let num_entries_chirho = dyn_size_chirho as usize / dyn_entry_size_chirho;

    for idx_chirho in 0..num_entries_chirho {
        let entry_off_chirho = dyn_off_chirho + idx_chirho * dyn_entry_size_chirho;
        if entry_off_chirho + dyn_entry_size_chirho > elf_data_chirho.len() {
            break;
        }

        let dyn_entry_chirho = unsafe {
            core::ptr::read_unaligned(
                elf_data_chirho.as_ptr().add(entry_off_chirho) as *const Elf64DynChirho,
            )
        };

        let tag_chirho = dyn_entry_chirho.d_tag_chirho as u64;
        let val_chirho = dyn_entry_chirho.d_val_chirho;

        match tag_chirho {
            DT_NULL_CHIRHO => break, // sentinel
            DT_NEEDED_CHIRHO => {
                info_chirho.needed_offsets_chirho.push(val_chirho);
            }
            DT_STRTAB_CHIRHO => {
                info_chirho.strtab_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_STRSZ_CHIRHO => {
                info_chirho.strtab_size_chirho = val_chirho;
            }
            DT_SYMTAB_CHIRHO => {
                info_chirho.symtab_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_SYMENT_CHIRHO => {
                info_chirho.syment_size_chirho = val_chirho;
            }
            DT_RELA_CHIRHO => {
                info_chirho.rela_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_RELASZ_CHIRHO => {
                info_chirho.rela_size_chirho = val_chirho;
            }
            DT_RELAENT_CHIRHO => {
                info_chirho.relaent_size_chirho = val_chirho;
            }
            DT_JMPREL_CHIRHO => {
                info_chirho.jmprel_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_PLTRELSZ_CHIRHO => {
                info_chirho.pltrelsz_chirho = val_chirho;
            }
            DT_PLTREL_CHIRHO => {
                info_chirho.pltrel_type_chirho = val_chirho;
            }
            DT_PLTGOT_CHIRHO => {
                info_chirho.pltgot_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_HASH_CHIRHO => {
                info_chirho.hash_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_GNU_HASH_CHIRHO => {
                info_chirho.gnu_hash_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_INIT_CHIRHO => {
                info_chirho.init_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            DT_FINI_CHIRHO => {
                info_chirho.fini_addr_chirho = val_chirho.wrapping_add(load_bias_chirho);
            }
            _ => {
                // Ignore unknown tags.
            }
        }
    }

    Ok(info_chirho)
}

/// Scan program headers and extract the PT_INTERP path, if present.
pub fn find_interp_in_phdrs_chirho(elf_data_chirho: &[u8]) -> Option<String> {
    let header_chirho = read_header_unaligned_chirho(elf_data_chirho)?;
    let phoff_chirho = header_chirho.e_phoff_chirho as usize;
    let phentsize_chirho = header_chirho.e_phentsize_chirho as usize;
    let phnum_chirho = header_chirho.e_phnum_chirho as usize;

    for idx_chirho in 0..phnum_chirho {
        let off_chirho = phoff_chirho + idx_chirho * phentsize_chirho;
        if off_chirho + mem::size_of::<Elf64PhdrChirho>() > elf_data_chirho.len() {
            continue;
        }
        let phdr_chirho = unsafe {
            core::ptr::read_unaligned(
                elf_data_chirho.as_ptr().add(off_chirho) as *const Elf64PhdrChirho,
            )
        };
        if phdr_chirho.p_type_chirho == PT_INTERP_CHIRHO {
            return extract_interp_path_chirho(elf_data_chirho, &phdr_chirho);
        }
    }

    None
}

/// Load an `ET_DYN` (PIE / shared object) ELF binary at a given base address.
///
/// This maps all `PT_LOAD` segments with the appropriate bias so that an
/// `ET_DYN` binary (which has vaddrs starting at 0) is placed at
/// `base_addr_chirho`.
///
/// Returns a [`LoadedElfChirho`] with adjusted addresses.
pub fn load_elf_at_base_chirho(
    elf_data_chirho: &[u8],
    base_addr_chirho: u64,
) -> Result<LoadedElfChirho, DynlinkErrorChirho> {
    let elf_info_chirho = elf_chirho::parse_elf_chirho(elf_data_chirho)
        .map_err(|_err_chirho| DynlinkErrorChirho::ElfParseErrorChirho("ELF parse failed"))?;

    if elf_info_chirho.segments_chirho.is_empty() {
        return Err(DynlinkErrorChirho::NoSegmentsChirho);
    }

    // For ET_DYN, determine the load bias.  If the first segment's vaddr is 0
    // (typical for PIE / shared objects), the bias is base_addr_chirho.
    // If vaddr is non-zero (rare for ET_DYN), bias = base_addr - first_vaddr.
    let first_vaddr_chirho = elf_info_chirho.segments_chirho[0].vaddr_chirho;
    let load_bias_chirho = base_addr_chirho.wrapping_sub(first_vaddr_chirho);

    serial_println_chirho!(
        "[DYNLINK] Loading ET_DYN ELF: base={:#x}, bias={:#x}, entry={:#x}",
        base_addr_chirho,
        load_bias_chirho,
        elf_info_chirho.entry_point_chirho.wrapping_add(load_bias_chirho)
    );

    let mm_lock_chirho = mm_chirho::get_or_init_mm_chirho();
    let mut brk_addr_chirho: u64 = 0;

    for seg_chirho in &elf_info_chirho.segments_chirho {
        // Apply the load bias to the segment's vaddr.
        let biased_vaddr_chirho = seg_chirho.vaddr_chirho.wrapping_add(load_bias_chirho);

        let page_start_chirho = align_down_chirho(biased_vaddr_chirho, PAGE_SIZE_CHIRHO);
        let page_end_chirho = align_up_chirho(
            biased_vaddr_chirho + seg_chirho.memsz_chirho,
            PAGE_SIZE_CHIRHO,
        );
        let map_len_chirho = page_end_chirho - page_start_chirho;

        let _prot_chirho = elf_flags_to_prot_chirho(seg_chirho.flags_chirho);

        serial_println_chirho!(
            "[DYNLINK]   Segment: vaddr={:#x} -> {:#x}, memsz={:#x}, filesz={:#x}",
            seg_chirho.vaddr_chirho,
            biased_vaddr_chirho,
            seg_chirho.memsz_chirho,
            seg_chirho.filesz_chirho
        );

        // Map with RWX initially so we can copy data in.
        let alloc_prot_chirho = PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | PROT_EXEC_CHIRHO;
        {
            let mut mm_guard_chirho = mm_lock_chirho.lock();
            let mm_ref_chirho = mm_guard_chirho
                .as_mut()
                .expect("MM not initialised for dynlink");
            mm_ref_chirho
                .mmap_chirho(
                    page_start_chirho,
                    map_len_chirho,
                    alloc_prot_chirho,
                    MAP_ANONYMOUS_CHIRHO | MAP_PRIVATE_CHIRHO | MAP_FIXED_CHIRHO,
                    -1,
                    0,
                )
                .map_err(|err_chirho| DynlinkErrorChirho::MmapErrorChirho(err_chirho))?;
        }

        // Copy initialised data.
        if seg_chirho.filesz_chirho > 0 {
            if let Some(data_chirho) = elf_chirho::segment_data_chirho(elf_data_chirho, seg_chirho)
            {
                let dest_ptr_chirho = biased_vaddr_chirho as *mut u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        data_chirho.as_ptr(),
                        dest_ptr_chirho,
                        data_chirho.len(),
                    );
                }
                // Verify: check bytes at offset 0x38bc0 in the code segment
                if seg_chirho.offset_chirho <= 0x38bc0 && seg_chirho.offset_chirho + seg_chirho.filesz_chirho > 0x38bc0 {
                    let check_off_chirho = (0x38bc0 - seg_chirho.offset_chirho) as usize;
                    if check_off_chirho + 8 <= data_chirho.len() {
                        serial_println_chirho!(
                            "[DYNLINK]   VERIFY file[0x38bc0]: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                            data_chirho[check_off_chirho], data_chirho[check_off_chirho+1],
                            data_chirho[check_off_chirho+2], data_chirho[check_off_chirho+3],
                            data_chirho[check_off_chirho+4], data_chirho[check_off_chirho+5],
                            data_chirho[check_off_chirho+6], data_chirho[check_off_chirho+7]
                        );
                        // Also verify what ended up in memory
                        let mem_ptr_chirho = (biased_vaddr_chirho + check_off_chirho as u64) as *const u8;
                        let (m0, m1, m2, m3) = unsafe { (
                            core::ptr::read_volatile(mem_ptr_chirho),
                            core::ptr::read_volatile(mem_ptr_chirho.add(1)),
                            core::ptr::read_volatile(mem_ptr_chirho.add(2)),
                            core::ptr::read_volatile(mem_ptr_chirho.add(3)),
                        ) };
                        serial_println_chirho!(
                            "[DYNLINK]   VERIFY mem[{:#x}]: {:02x} {:02x} {:02x} {:02x}",
                            biased_vaddr_chirho + check_off_chirho as u64, m0, m1, m2, m3
                        );
                    }
                }
            }
        }

        // Zero BSS.
        let bss_start_chirho = biased_vaddr_chirho + seg_chirho.filesz_chirho;
        let bss_len_chirho = seg_chirho.memsz_chirho - seg_chirho.filesz_chirho;
        if bss_len_chirho > 0 {
            unsafe {
                core::ptr::write_bytes(bss_start_chirho as *mut u8, 0, bss_len_chirho as usize);
            }
        }

        // Track highest address for brk.
        let seg_end_chirho = biased_vaddr_chirho + seg_chirho.memsz_chirho;
        if seg_end_chirho > brk_addr_chirho {
            brk_addr_chirho = seg_end_chirho;
        }
    }

    brk_addr_chirho = align_up_chirho(brk_addr_chirho, PAGE_SIZE_CHIRHO);

    // Compute biased phdr_addr.
    let phdr_addr_chirho = if elf_info_chirho.phdr_addr_chirho != 0 {
        elf_info_chirho.phdr_addr_chirho.wrapping_add(load_bias_chirho)
    } else {
        0
    };

    Ok(LoadedElfChirho {
        entry_point_chirho: elf_info_chirho.entry_point_chirho.wrapping_add(load_bias_chirho),
        phdr_addr_chirho,
        phdr_num_chirho: elf_info_chirho.phdr_num_chirho,
        phdr_size_chirho: elf_info_chirho.phdr_size_chirho,
        brk_addr_chirho,
    })
}

/// Apply `R_X86_64_RELATIVE` relocations from the RELA table.
///
/// These relocations are of the form `*(base + offset) = base + addend`,
/// used by PIE executables and shared libraries for position-independent
/// data references.
///
/// # Safety
///
/// The caller must ensure that the relocation addresses are within mapped,
/// writable memory.
pub unsafe fn apply_relative_relocs_chirho(
    rela_addr_chirho: u64,
    rela_size_chirho: u64,
    relaent_size_chirho: u64,
    load_bias_chirho: u64,
) {
    if rela_addr_chirho == 0 || rela_size_chirho == 0 || relaent_size_chirho == 0 {
        return;
    }

    let entry_size_chirho = relaent_size_chirho.max(mem::size_of::<Elf64RelaChirho>() as u64);
    let num_entries_chirho = rela_size_chirho / entry_size_chirho;

    serial_println_chirho!(
        "[DYNLINK] Applying {} RELA relocations (bias={:#x})",
        num_entries_chirho,
        load_bias_chirho
    );

    for idx_chirho in 0..num_entries_chirho {
        let entry_addr_chirho = rela_addr_chirho + idx_chirho * entry_size_chirho;
        let rela_chirho = core::ptr::read_unaligned(entry_addr_chirho as *const Elf64RelaChirho);

        let rela_type_chirho = rela_chirho.rela_type_chirho();

        match rela_type_chirho {
            R_X86_64_RELATIVE_CHIRHO => {
                // *(base + offset) = base + addend
                let target_addr_chirho = rela_chirho.r_offset_chirho.wrapping_add(load_bias_chirho);
                let value_chirho = (load_bias_chirho as i64)
                    .wrapping_add(rela_chirho.r_addend_chirho) as u64;
                core::ptr::write(target_addr_chirho as *mut u64, value_chirho);
            }
            R_X86_64_JUMP_SLOT_CHIRHO | R_X86_64_GLOB_DAT_CHIRHO => {
                // These require symbol resolution. For now, write 0 as a
                // placeholder — the dynamic linker (ld.so) will resolve them
                // during its own initialization.
                // If we are loading the linker itself, these should be
                // self-referencing and resolved via DT_SYMTAB.
            }
            _ => {
                // Unknown relocation type — skip.
                serial_println_chirho!(
                    "[DYNLINK] Skipping relocation type {} at offset {:#x}",
                    rela_type_chirho,
                    rela_chirho.r_offset_chirho
                );
            }
        }
    }
}

/// Apply PLT/GOT relocations from the JMPREL table for lazy binding setup.
///
/// For eager binding (no lazy binding), this resolves all PLT entries
/// immediately. For lazy binding, it sets up the GOT entries to point
/// back to the PLT stub (which will trigger the dynamic linker on first
/// call).
///
/// # Safety
///
/// The caller must ensure that the relocation addresses are within mapped,
/// writable memory.
pub unsafe fn setup_plt_got_chirho(
    jmprel_addr_chirho: u64,
    pltrelsz_chirho: u64,
    pltgot_addr_chirho: u64,
    load_bias_chirho: u64,
) {
    if jmprel_addr_chirho == 0 || pltrelsz_chirho == 0 {
        return;
    }

    let entry_size_chirho = mem::size_of::<Elf64RelaChirho>() as u64;
    let num_entries_chirho = pltrelsz_chirho / entry_size_chirho;

    serial_println_chirho!(
        "[DYNLINK] Setting up PLT/GOT: {} entries, pltgot={:#x}",
        num_entries_chirho,
        pltgot_addr_chirho
    );

    // Set up GOT[1] and GOT[2] for lazy binding:
    //   GOT[0] = address of _DYNAMIC (set by linker)
    //   GOT[1] = link_map pointer (set by dynamic linker)
    //   GOT[2] = address of _dl_runtime_resolve (set by dynamic linker)
    //
    // Since we're the kernel loading the dynamic linker itself, we leave
    // GOT[1] and GOT[2] as 0 — ld.so will fill them in during its own
    // self-bootstrap.

    // Process each JMPREL entry. For eager binding, resolve R_X86_64_JUMP_SLOT
    // with the bias-adjusted symbol value. For lazy binding, the GOT entry
    // should already point to the PLT stub (push index; jmp GOT[2]).
    for idx_chirho in 0..num_entries_chirho {
        let entry_addr_chirho = jmprel_addr_chirho + idx_chirho * entry_size_chirho;
        let rela_chirho = core::ptr::read_unaligned(entry_addr_chirho as *const Elf64RelaChirho);

        let rela_type_chirho = rela_chirho.rela_type_chirho();

        if rela_type_chirho == R_X86_64_JUMP_SLOT_CHIRHO {
            // For lazy binding: the initial GOT value points to the PLT stub.
            // We just need to add the load bias to it so it works at the
            // actual load address.
            let got_entry_addr_chirho =
                rela_chirho.r_offset_chirho.wrapping_add(load_bias_chirho);
            let current_val_chirho = core::ptr::read(got_entry_addr_chirho as *const u64);
            if current_val_chirho != 0 {
                // Add bias to the PLT stub address already stored in GOT.
                let biased_val_chirho = current_val_chirho.wrapping_add(load_bias_chirho);
                core::ptr::write(got_entry_addr_chirho as *mut u64, biased_val_chirho);
            }
        }
    }
}

/// Build an extended auxiliary vector for a dynamically linked binary.
///
/// Includes the standard entries from [`crate::elf_chirho::build_auxv_chirho`]
/// plus `AT_BASE` (interpreter base address) which the dynamic linker needs
/// to know where it was loaded.
///
/// # Arguments
///
/// * `exe_info_chirho` — Loaded main executable info.
/// * `interp_base_chirho` — Base address where the interpreter was loaded (0
///   if no interpreter).
/// * `original_entry_chirho` — The main executable's original entry point
///   (before any interpreter override). Passed as `AT_ENTRY`.
pub fn build_dynlink_auxv_chirho(
    exe_info_chirho: &LoadedElfChirho,
    interp_base_chirho: u64,
    original_entry_chirho: u64,
) -> Vec<(u64, u64)> {
    use crate::elf_chirho::{
        AT_ENTRY_CHIRHO, AT_EUID_CHIRHO, AT_GID_CHIRHO, AT_EGID_CHIRHO,
        AT_NULL_CHIRHO, AT_PAGESZ_CHIRHO, AT_PHDR_CHIRHO, AT_PHENT_CHIRHO,
        AT_PHNUM_CHIRHO, AT_RANDOM_CHIRHO, AT_UID_CHIRHO, AT_EXECFN_CHIRHO,
    };

    let mut auxv_chirho: Vec<(u64, u64)> = Vec::new();

    // Program header table of the main executable.
    auxv_chirho.push((AT_PHDR_CHIRHO, exe_info_chirho.phdr_addr_chirho));
    auxv_chirho.push((AT_PHENT_CHIRHO, exe_info_chirho.phdr_size_chirho as u64));
    auxv_chirho.push((AT_PHNUM_CHIRHO, exe_info_chirho.phdr_num_chirho as u64));

    // System page size.
    auxv_chirho.push((AT_PAGESZ_CHIRHO, PAGE_SIZE_CHIRHO));

    // Entry point of the main executable (not the interpreter).
    auxv_chirho.push((AT_ENTRY_CHIRHO, original_entry_chirho));

    // Base address of the interpreter.
    if interp_base_chirho != 0 {
        auxv_chirho.push((AT_BASE_CHIRHO, interp_base_chirho));
    } else {
        auxv_chirho.push((AT_BASE_CHIRHO, 0));
    }

    // Process credentials.
    auxv_chirho.push((AT_UID_CHIRHO, 0));
    auxv_chirho.push((AT_EUID_CHIRHO, 0));
    auxv_chirho.push((AT_GID_CHIRHO, 0));
    auxv_chirho.push((AT_EGID_CHIRHO, 0));

    // Random bytes placeholder.
    auxv_chirho.push((AT_RANDOM_CHIRHO, original_entry_chirho));

    // Executable filename placeholder.
    auxv_chirho.push((AT_EXECFN_CHIRHO, 0));

    // Terminator.
    auxv_chirho.push((AT_NULL_CHIRHO, 0));

    auxv_chirho
}

/// Check whether an ELF binary is dynamically linked (has a PT_INTERP
/// segment).
pub fn is_dynamically_linked_chirho(elf_data_chirho: &[u8]) -> bool {
    find_interp_in_phdrs_chirho(elf_data_chirho).is_some()
}

/// Determine the appropriate load base for a PIE executable.
///
/// Returns [`PIE_LOAD_BASE_CHIRHO`] for `ET_DYN` binaries whose first
/// `PT_LOAD` segment starts at vaddr 0, or 0 for `ET_EXEC` binaries
/// (which have fixed addresses).
pub fn compute_load_base_chirho(elf_data_chirho: &[u8]) -> u64 {
    let header_chirho = match read_header_unaligned_chirho(elf_data_chirho) {
        Some(h_chirho) => h_chirho,
        None => return 0,
    };

    if header_chirho.e_type_chirho != ET_DYN_CHIRHO {
        return 0;
    }

    // If the first PT_LOAD vaddr is 0, it's a typical PIE; apply a base.
    let phoff_chirho = header_chirho.e_phoff_chirho as usize;
    let phentsize_chirho = header_chirho.e_phentsize_chirho as usize;
    let phnum_chirho = header_chirho.e_phnum_chirho as usize;

    for idx_chirho in 0..phnum_chirho {
        let off_chirho = phoff_chirho + idx_chirho * phentsize_chirho;
        if off_chirho + mem::size_of::<Elf64PhdrChirho>() > elf_data_chirho.len() {
            continue;
        }
        let phdr_chirho = unsafe {
            core::ptr::read_unaligned(
                elf_data_chirho.as_ptr().add(off_chirho) as *const Elf64PhdrChirho,
            )
        };
        if phdr_chirho.p_type_chirho == PT_LOAD_CHIRHO {
            if phdr_chirho.p_vaddr_chirho == 0 {
                return PIE_LOAD_BASE_CHIRHO;
            }
            // Non-zero first PT_LOAD — already has a fixed base.
            return 0;
        }
    }

    0
}

/// Get the default interpreter load base address.
pub fn interp_load_base_chirho() -> u64 {
    INTERP_LOAD_BASE_CHIRHO
}

// ============================================================================
// Resolve a needed library name from the string table.
// ============================================================================

/// Read a NUL-terminated string from the in-memory string table at the given
/// offset.
///
/// # Safety
///
/// `strtab_addr_chirho` must point to mapped, readable memory containing the
/// ELF string table, and `offset_chirho` must be within bounds.
pub unsafe fn read_strtab_entry_chirho(
    strtab_addr_chirho: u64,
    offset_chirho: u64,
) -> Option<String> {
    let ptr_chirho = (strtab_addr_chirho + offset_chirho) as *const u8;

    // Walk until NUL or a safety limit (256 bytes).
    let mut len_chirho: usize = 0;
    let max_len_chirho: usize = 256;
    while len_chirho < max_len_chirho {
        let byte_chirho = core::ptr::read(ptr_chirho.add(len_chirho));
        if byte_chirho == 0 {
            break;
        }
        len_chirho += 1;
    }

    if len_chirho == 0 {
        return None;
    }

    let slice_chirho = core::slice::from_raw_parts(ptr_chirho, len_chirho);
    core::str::from_utf8(slice_chirho).ok().map(String::from)
}

/// Resolve all `DT_NEEDED` library names from the dynamic info.
///
/// # Safety
///
/// The string table must be in mapped memory.
pub unsafe fn resolve_needed_names_chirho(
    info_chirho: &DynamicInfoChirho,
) -> Vec<String> {
    let mut names_chirho: Vec<String> = Vec::new();

    for &offset_chirho in &info_chirho.needed_offsets_chirho {
        if let Some(name_chirho) =
            read_strtab_entry_chirho(info_chirho.strtab_addr_chirho, offset_chirho)
        {
            names_chirho.push(name_chirho);
        }
    }

    names_chirho
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Read the ELF header from raw bytes using an unaligned read.
fn read_header_unaligned_chirho(data_chirho: &[u8]) -> Option<Elf64HeaderChirho> {
    if data_chirho.len() < mem::size_of::<Elf64HeaderChirho>() {
        return None;
    }
    Some(unsafe {
        core::ptr::read_unaligned(data_chirho.as_ptr() as *const Elf64HeaderChirho)
    })
}

/// Align `val_chirho` upward to the next multiple of `align_chirho`.
const fn align_up_chirho(val_chirho: u64, align_chirho: u64) -> u64 {
    (val_chirho + align_chirho - 1) & !(align_chirho - 1)
}

/// Align `val_chirho` downward to the nearest multiple of `align_chirho`.
const fn align_down_chirho(val_chirho: u64, align_chirho: u64) -> u64 {
    val_chirho & !(align_chirho - 1)
}

/// Convert ELF segment flags to Linux PROT_* flags.
fn elf_flags_to_prot_chirho(flags_chirho: u32) -> u32 {
    let mut prot_chirho: u32 = PROT_READ_CHIRHO;
    if flags_chirho & PF_W_CHIRHO != 0 {
        prot_chirho |= PROT_WRITE_CHIRHO;
    }
    if flags_chirho & PF_X_CHIRHO != 0 {
        prot_chirho |= PROT_EXEC_CHIRHO;
    }
    prot_chirho
}
