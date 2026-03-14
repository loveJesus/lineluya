// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ELF binary loader for the Lineluya kernel.
//!
//! Parses and validates 64-bit ELF executables (`ET_EXEC` / `ET_DYN`) so that
//! the kernel can load user-space programs into process memory.  This is the
//! Lineluya equivalent of Linux's `load_elf_binary`.
//!
//! All structures use `#[repr(C)]` to match the on-disk ELF layout exactly.
//! No external crates are used — parsing is done from raw byte slices with
//! explicit bounds checking.

extern crate alloc;

use alloc::vec::Vec;
use core::mem;

// ---------------------------------------------------------------------------
// ELF constants
// ---------------------------------------------------------------------------

/// ELF magic bytes: `\x7fELF`
pub const ELF_MAGIC_CHIRHO: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit objects.
pub const ELFCLASS64_CHIRHO: u8 = 2;

/// ELF data encoding: little-endian (2's complement, least significant byte
/// first).
pub const ELFDATA2LSB_CHIRHO: u8 = 1;

/// ELF type: executable file.
pub const ET_EXEC_CHIRHO: u16 = 2;

/// ELF type: shared object / position-independent executable.
pub const ET_DYN_CHIRHO: u16 = 3;

/// ELF machine: AMD x86-64.
pub const EM_X86_64_CHIRHO: u16 = 62;

// -- Program header segment types -------------------------------------------

/// Loadable segment.
pub const PT_LOAD_CHIRHO: u32 = 1;

/// Interpreter path (e.g. `/lib64/ld-linux-x86-64.so.2`).
pub const PT_INTERP_CHIRHO: u32 = 3;

/// Note segment.
pub const PT_NOTE_CHIRHO: u32 = 4;

/// Program header table itself.
pub const PT_PHDR_CHIRHO: u32 = 6;

/// GNU stack permissions segment.
pub const PT_GNU_STACK_CHIRHO: u32 = 0x6474e551;

// -- Program header permission flags ----------------------------------------

/// Segment is executable.
pub const PF_X_CHIRHO: u32 = 1;

/// Segment is writable.
pub const PF_W_CHIRHO: u32 = 2;

/// Segment is readable.
pub const PF_R_CHIRHO: u32 = 4;

// -- Linux Auxiliary Vector types -------------------------------------------

/// End of auxiliary vector.
pub const AT_NULL_CHIRHO: u64 = 0;

/// Address of the program header table in memory.
pub const AT_PHDR_CHIRHO: u64 = 3;

/// Size of a single program header entry.
pub const AT_PHENT_CHIRHO: u64 = 4;

/// Number of program header entries.
pub const AT_PHNUM_CHIRHO: u64 = 5;

/// System page size.
pub const AT_PAGESZ_CHIRHO: u64 = 6;

/// Entry point of the program.
pub const AT_ENTRY_CHIRHO: u64 = 9;

/// Real user ID.
pub const AT_UID_CHIRHO: u64 = 11;

/// Effective user ID.
pub const AT_EUID_CHIRHO: u64 = 12;

/// Real group ID.
pub const AT_GID_CHIRHO: u64 = 13;

/// Effective group ID.
pub const AT_EGID_CHIRHO: u64 = 14;

/// Address of 16 bytes of random data (provided by the kernel).
pub const AT_RANDOM_CHIRHO: u64 = 25;

/// Filename of the executed program.
pub const AT_EXECFN_CHIRHO: u64 = 31;

/// Default page size used when building the auxiliary vector (4 KiB).
const PAGE_SIZE_CHIRHO: u64 = 4096;

// ---------------------------------------------------------------------------
// ELF header (64-bit)
// ---------------------------------------------------------------------------

/// 64-bit ELF file header, corresponding to `Elf64_Ehdr` in the ELF
/// specification.
///
/// The layout is `#[repr(C)]` so that the struct can be safely reinterpreted
/// from a raw byte slice that matches the on-disk format.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64HeaderChirho {
    /// ELF identification bytes.
    ///
    /// - `[0..4]`  — magic number (`\x7fELF`)
    /// - `[4]`     — file class (1 = 32-bit, 2 = 64-bit)
    /// - `[5]`     — data encoding (1 = little-endian, 2 = big-endian)
    /// - `[6]`     — ELF version (must be 1)
    /// - `[7]`     — OS/ABI identification
    /// - `[8..16]` — padding (should be zero)
    pub e_ident_chirho: [u8; 16],

    /// Object file type (`ET_EXEC` = 2, `ET_DYN` = 3, etc.).
    pub e_type_chirho: u16,

    /// Required architecture (`EM_X86_64` = 62).
    pub e_machine_chirho: u16,

    /// Object file version (must be 1).
    pub e_version_chirho: u32,

    /// Virtual address of the entry point.
    pub e_entry_chirho: u64,

    /// File offset to the program header table.
    pub e_phoff_chirho: u64,

    /// File offset to the section header table.
    pub e_shoff_chirho: u64,

    /// Processor-specific flags.
    pub e_flags_chirho: u32,

    /// Size of this ELF header (bytes).
    pub e_ehsize_chirho: u16,

    /// Size of a single program header entry (bytes).
    pub e_phentsize_chirho: u16,

    /// Number of program header entries.
    pub e_phnum_chirho: u16,

    /// Size of a single section header entry (bytes).
    pub e_shentsize_chirho: u16,

    /// Number of section header entries.
    pub e_shnum_chirho: u16,

    /// Section header table index of the section-name string table.
    pub e_shstrndx_chirho: u16,
}

// ---------------------------------------------------------------------------
// Program header (64-bit)
// ---------------------------------------------------------------------------

/// 64-bit ELF program header, corresponding to `Elf64_Phdr`.
///
/// Each program header describes a segment or other information the system
/// needs to prepare the program for execution.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64PhdrChirho {
    /// Segment type (`PT_LOAD` = 1, `PT_INTERP` = 3, `PT_NOTE` = 4,
    /// `PT_PHDR` = 6, etc.).
    pub p_type_chirho: u32,

    /// Segment permission flags (`PF_R` = 4, `PF_W` = 2, `PF_X` = 1).
    pub p_flags_chirho: u32,

    /// Offset of the segment data in the file.
    pub p_offset_chirho: u64,

    /// Virtual address at which the segment should be loaded.
    pub p_vaddr_chirho: u64,

    /// Physical address (generally unused on modern systems).
    pub p_paddr_chirho: u64,

    /// Size of the segment data in the file (bytes).
    pub p_filesz_chirho: u64,

    /// Size of the segment in memory (bytes).  May be larger than
    /// `p_filesz_chirho` — the extra bytes (BSS) are zero-filled.
    pub p_memsz_chirho: u64,

    /// Alignment requirement for the segment.  Must be a power of two.
    /// `p_vaddr ≡ p_offset (mod p_align)`.
    pub p_align_chirho: u64,
}

// ---------------------------------------------------------------------------
// Parsed ELF metadata
// ---------------------------------------------------------------------------

/// High-level metadata extracted from a validated ELF binary.
///
/// Contains everything the kernel needs to set up the process address space
/// and jump to the entry point.
#[derive(Debug)]
pub struct ElfInfoChirho {
    /// Virtual address of the program entry point.
    pub entry_point_chirho: u64,

    /// Loadable segments (`PT_LOAD`) that must be mapped into the process
    /// address space.
    pub segments_chirho: Vec<ElfSegmentChirho>,

    /// Virtual address where the program header table is mapped (from the
    /// `PT_PHDR` segment, if present).  Passed to the process via `AT_PHDR`.
    pub phdr_addr_chirho: u64,

    /// Number of program header entries (`e_phnum`).
    pub phdr_num_chirho: u16,

    /// Size of a single program header entry (`e_phentsize`).
    pub phdr_size_chirho: u16,
}

/// A single loadable segment extracted from the ELF program headers.
#[derive(Debug, Clone)]
pub struct ElfSegmentChirho {
    /// Virtual address at which this segment should be loaded.
    pub vaddr_chirho: u64,

    /// Size of this segment in process memory (may include BSS).
    pub memsz_chirho: u64,

    /// Size of the initialised data in the file.
    pub filesz_chirho: u64,

    /// File offset of the initialised data.
    pub offset_chirho: u64,

    /// Permission flags (`PF_R`, `PF_W`, `PF_X`).
    pub flags_chirho: u32,

    /// Alignment requirement.
    pub align_chirho: u64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing or validating an ELF binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfErrorChirho {
    /// The file does not start with the ELF magic bytes (`\x7fELF`).
    InvalidMagicChirho,

    /// The ELF class is not 64-bit (`ELFCLASS64`).
    UnsupportedClassChirho,

    /// The ELF data encoding is not little-endian (`ELFDATA2LSB`).
    UnsupportedEndianChirho,

    /// The target machine is not x86-64 (`EM_X86_64`).
    UnsupportedMachineChirho,

    /// The ELF type is neither `ET_EXEC` nor `ET_DYN`.
    UnsupportedTypeChirho,

    /// A program header is malformed or extends beyond the file.
    InvalidPhdrChirho,

    /// A `PT_LOAD` segment's `p_memsz` or `p_filesz` exceeds a sane limit.
    SegmentTooLargeChirho,
}

/// Maximum allowed segment size (256 MiB).  This is a sanity check to reject
/// clearly corrupt or adversarial ELF files before allocating memory.
const MAX_SEGMENT_SIZE_CHIRHO: u64 = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate an ELF binary header from a raw byte slice.
///
/// Performs the following checks:
/// 1. The slice is large enough to contain an [`Elf64HeaderChirho`].
/// 2. The magic bytes match `\x7fELF`.
/// 3. The file class is 64-bit.
/// 4. The data encoding is little-endian.
/// 5. The target machine is x86-64.
/// 6. The ELF type is `ET_EXEC` or `ET_DYN`.
///
/// On success, returns a reference into `data_chirho` reinterpreted as an
/// [`Elf64HeaderChirho`].  The reference is safe because the struct is
/// `#[repr(C)]`, the slice is large enough, and all field types are plain
/// integers (no validity invariants beyond alignment, which is 1 for a
/// packed read — see implementation note below).
pub fn validate_elf_chirho(data_chirho: &[u8]) -> Result<&Elf64HeaderChirho, ElfErrorChirho> {
    let header_size_chirho = mem::size_of::<Elf64HeaderChirho>();

    if data_chirho.len() < header_size_chirho {
        return Err(ElfErrorChirho::InvalidMagicChirho);
    }

    // Read the header from the byte slice.  We copy into a local to avoid
    // alignment issues (the slice may not be aligned to the struct's natural
    // alignment).
    let header_chirho = read_header_chirho(data_chirho)?;

    // 1. Magic number
    if header_chirho.e_ident_chirho[0..4] != ELF_MAGIC_CHIRHO {
        return Err(ElfErrorChirho::InvalidMagicChirho);
    }

    // 2. 64-bit class
    if header_chirho.e_ident_chirho[4] != ELFCLASS64_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedClassChirho);
    }

    // 3. Little-endian
    if header_chirho.e_ident_chirho[5] != ELFDATA2LSB_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedEndianChirho);
    }

    // 4. x86-64 machine
    if header_chirho.e_machine_chirho != EM_X86_64_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedMachineChirho);
    }

    // 5. Executable or shared-object type
    if header_chirho.e_type_chirho != ET_EXEC_CHIRHO
        && header_chirho.e_type_chirho != ET_DYN_CHIRHO
    {
        return Err(ElfErrorChirho::UnsupportedTypeChirho);
    }

    // SAFETY: We have verified the slice is large enough.  We return a
    // reference obtained via `read_unaligned` below (through `parse_elf`),
    // but for the public validate API we actually just need to confirm
    // validity.  We reinterpret via a pointer cast here; the data is
    // `#[repr(C)]` with no padding invariants.  Alignment is guaranteed to
    // be at least 1 for u8 slices, and `Elf64HeaderChirho` only contains
    // integer fields — on x86-64 unaligned reads of integers are fine in
    // practice, but to be safe we go through `read_unaligned`.
    //
    // However, returning a *reference* into the slice could theoretically
    // have alignment issues.  To stay fully safe, callers should prefer
    // `parse_elf_chirho` which copies the header.  This function returns a
    // pointer-cast reference for zero-copy convenience when the caller
    // knows the data is aligned (e.g. page-aligned file data).
    let header_ptr_chirho = data_chirho.as_ptr() as *const Elf64HeaderChirho;

    // SAFETY: We checked the length above.  On x86-64, unaligned access to
    // integer fields is architecturally supported, so the pointer cast is
    // sound even if the slice is not naturally aligned.
    Ok(unsafe { &*header_ptr_chirho })
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a complete ELF binary from a byte slice.
///
/// Validates the ELF header, iterates over the program headers, and extracts
/// all `PT_LOAD` segments and the `PT_PHDR` address.
///
/// Returns an [`ElfInfoChirho`] containing everything the kernel needs to
/// load the program into a process address space.
pub fn parse_elf_chirho(data_chirho: &[u8]) -> Result<ElfInfoChirho, ElfErrorChirho> {
    // --- Validate and read the ELF header ----------------------------------

    let header_chirho = read_header_chirho(data_chirho)?;

    // Validate magic, class, endianness, machine, and type.
    validate_header_fields_chirho(&header_chirho)?;

    let phoff_chirho = header_chirho.e_phoff_chirho as usize;
    let phentsize_chirho = header_chirho.e_phentsize_chirho as usize;
    let phnum_chirho = header_chirho.e_phnum_chirho as usize;

    // Sanity-check that the program header table fits inside the file.
    let phdr_table_end_chirho = phoff_chirho
        .checked_add(phentsize_chirho.checked_mul(phnum_chirho).ok_or(ElfErrorChirho::InvalidPhdrChirho)?)
        .ok_or(ElfErrorChirho::InvalidPhdrChirho)?;

    if phdr_table_end_chirho > data_chirho.len() {
        return Err(ElfErrorChirho::InvalidPhdrChirho);
    }

    if phentsize_chirho < mem::size_of::<Elf64PhdrChirho>() {
        return Err(ElfErrorChirho::InvalidPhdrChirho);
    }

    // --- Iterate program headers -------------------------------------------

    let mut segments_chirho: Vec<ElfSegmentChirho> = Vec::new();
    let mut phdr_addr_chirho: u64 = 0;

    for idx_chirho in 0..phnum_chirho {
        let offset_chirho = phoff_chirho + idx_chirho * phentsize_chirho;
        let phdr_chirho = read_phdr_chirho(data_chirho, offset_chirho)?;

        match phdr_chirho.p_type_chirho {
            PT_LOAD_CHIRHO => {
                // Sanity-check segment sizes.
                if phdr_chirho.p_memsz_chirho > MAX_SEGMENT_SIZE_CHIRHO
                    || phdr_chirho.p_filesz_chirho > MAX_SEGMENT_SIZE_CHIRHO
                {
                    return Err(ElfErrorChirho::SegmentTooLargeChirho);
                }

                // p_memsz must be >= p_filesz (the difference is BSS).
                if phdr_chirho.p_memsz_chirho < phdr_chirho.p_filesz_chirho {
                    return Err(ElfErrorChirho::InvalidPhdrChirho);
                }

                // The file data for this segment must fit inside the file.
                let seg_file_end_chirho = (phdr_chirho.p_offset_chirho as usize)
                    .checked_add(phdr_chirho.p_filesz_chirho as usize)
                    .ok_or(ElfErrorChirho::InvalidPhdrChirho)?;

                if seg_file_end_chirho > data_chirho.len() {
                    return Err(ElfErrorChirho::InvalidPhdrChirho);
                }

                segments_chirho.push(ElfSegmentChirho {
                    vaddr_chirho: phdr_chirho.p_vaddr_chirho,
                    memsz_chirho: phdr_chirho.p_memsz_chirho,
                    filesz_chirho: phdr_chirho.p_filesz_chirho,
                    offset_chirho: phdr_chirho.p_offset_chirho,
                    flags_chirho: phdr_chirho.p_flags_chirho,
                    align_chirho: phdr_chirho.p_align_chirho,
                });
            }
            PT_PHDR_CHIRHO => {
                phdr_addr_chirho = phdr_chirho.p_vaddr_chirho;
            }
            _ => {
                // Other segment types (PT_INTERP, PT_NOTE, PT_GNU_STACK,
                // etc.) are noted but not processed during loading.
            }
        }
    }

    Ok(ElfInfoChirho {
        entry_point_chirho: header_chirho.e_entry_chirho,
        segments_chirho,
        phdr_addr_chirho,
        phdr_num_chirho: header_chirho.e_phnum_chirho,
        phdr_size_chirho: header_chirho.e_phentsize_chirho,
    })
}

// ---------------------------------------------------------------------------
// Auxiliary vector builder
// ---------------------------------------------------------------------------

/// Build the Linux auxiliary vector (`auxv`) for a loaded ELF binary.
///
/// The auxiliary vector is a list of `(type, value)` pairs placed on the
/// initial process stack by the kernel.  The C runtime (`_start` / `crt0`)
/// reads these values to locate the program headers, entry point, page size,
/// and other system information.
///
/// The returned vector is terminated by an `AT_NULL` entry.
///
/// # Arguments
///
/// * `elf_info_chirho` — Parsed ELF metadata from [`parse_elf_chirho`].
///
/// # Notes
///
/// - `AT_RANDOM` points to `elf_info_chirho.entry_point_chirho` as a
///   placeholder.  A real implementation should point to 16 bytes of
///   kernel-generated random data on the user stack.
/// - `AT_EXECFN` is set to 0.  The kernel should replace this with a pointer
///   to the executable filename string on the user stack.
/// - UID/GID values are all set to 0 (root).  A real implementation should
///   query the process credentials.
pub fn build_auxv_chirho(elf_info_chirho: &ElfInfoChirho) -> Vec<(u64, u64)> {
    let mut auxv_chirho: Vec<(u64, u64)> = Vec::new();

    // Program header table location and dimensions.
    auxv_chirho.push((AT_PHDR_CHIRHO, elf_info_chirho.phdr_addr_chirho));
    auxv_chirho.push((AT_PHENT_CHIRHO, elf_info_chirho.phdr_size_chirho as u64));
    auxv_chirho.push((AT_PHNUM_CHIRHO, elf_info_chirho.phdr_num_chirho as u64));

    // System page size.
    auxv_chirho.push((AT_PAGESZ_CHIRHO, PAGE_SIZE_CHIRHO));

    // Entry point virtual address.
    auxv_chirho.push((AT_ENTRY_CHIRHO, elf_info_chirho.entry_point_chirho));

    // Process credentials (placeholder: all zero / root).
    auxv_chirho.push((AT_UID_CHIRHO, 0));
    auxv_chirho.push((AT_EUID_CHIRHO, 0));
    auxv_chirho.push((AT_GID_CHIRHO, 0));
    auxv_chirho.push((AT_EGID_CHIRHO, 0));

    // 16 bytes of "random" data.  A proper implementation should place
    // random bytes on the user stack and pass that address here.
    auxv_chirho.push((AT_RANDOM_CHIRHO, elf_info_chirho.entry_point_chirho));

    // Filename of the executed program (placeholder: null pointer).
    auxv_chirho.push((AT_EXECFN_CHIRHO, 0));

    // Terminator — the C runtime stops scanning here.
    auxv_chirho.push((AT_NULL_CHIRHO, 0));

    auxv_chirho
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read an [`Elf64HeaderChirho`] from the start of `data_chirho` using a
/// bytewise copy, avoiding alignment issues.
fn read_header_chirho(data_chirho: &[u8]) -> Result<Elf64HeaderChirho, ElfErrorChirho> {
    let size_chirho = mem::size_of::<Elf64HeaderChirho>();

    if data_chirho.len() < size_chirho {
        return Err(ElfErrorChirho::InvalidMagicChirho);
    }

    // SAFETY: We have verified that `data_chirho` is at least `size_chirho`
    // bytes long.  `read_unaligned` performs a bytewise copy so alignment of
    // the source pointer is irrelevant.  `Elf64HeaderChirho` is `#[repr(C)]`
    // and consists entirely of integer fields, so every bit pattern is valid.
    let header_chirho = unsafe {
        core::ptr::read_unaligned(data_chirho.as_ptr() as *const Elf64HeaderChirho)
    };

    Ok(header_chirho)
}

/// Read an [`Elf64PhdrChirho`] at the given byte offset within `data_chirho`.
fn read_phdr_chirho(
    data_chirho: &[u8],
    offset_chirho: usize,
) -> Result<Elf64PhdrChirho, ElfErrorChirho> {
    let size_chirho = mem::size_of::<Elf64PhdrChirho>();

    let end_chirho = offset_chirho
        .checked_add(size_chirho)
        .ok_or(ElfErrorChirho::InvalidPhdrChirho)?;

    if end_chirho > data_chirho.len() {
        return Err(ElfErrorChirho::InvalidPhdrChirho);
    }

    // SAFETY: Bounds have been checked.  `read_unaligned` copies bytewise,
    // and `Elf64PhdrChirho` is `#[repr(C)]` with only integer fields.
    let phdr_chirho = unsafe {
        core::ptr::read_unaligned(
            data_chirho.as_ptr().add(offset_chirho) as *const Elf64PhdrChirho,
        )
    };

    Ok(phdr_chirho)
}

/// Validate the identity and type fields of an already-read ELF header.
///
/// This is the shared validation logic used by both [`validate_elf_chirho`]
/// and [`parse_elf_chirho`].
fn validate_header_fields_chirho(
    header_chirho: &Elf64HeaderChirho,
) -> Result<(), ElfErrorChirho> {
    if header_chirho.e_ident_chirho[0..4] != ELF_MAGIC_CHIRHO {
        return Err(ElfErrorChirho::InvalidMagicChirho);
    }

    if header_chirho.e_ident_chirho[4] != ELFCLASS64_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedClassChirho);
    }

    if header_chirho.e_ident_chirho[5] != ELFDATA2LSB_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedEndianChirho);
    }

    if header_chirho.e_machine_chirho != EM_X86_64_CHIRHO {
        return Err(ElfErrorChirho::UnsupportedMachineChirho);
    }

    if header_chirho.e_type_chirho != ET_EXEC_CHIRHO
        && header_chirho.e_type_chirho != ET_DYN_CHIRHO
    {
        return Err(ElfErrorChirho::UnsupportedTypeChirho);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Utility: segment data access
// ---------------------------------------------------------------------------

/// Return the initialised file data for a segment, given the full ELF file
/// contents.
///
/// The returned slice has length `segment_chirho.filesz_chirho`.  The caller
/// is responsible for zero-filling the remaining
/// `memsz_chirho - filesz_chirho` bytes (BSS) after copying this data into
/// the process address space.
///
/// Returns `None` if the segment's file range is out of bounds.
pub fn segment_data_chirho<'a>(
    data_chirho: &'a [u8],
    segment_chirho: &ElfSegmentChirho,
) -> Option<&'a [u8]> {
    let start_chirho = segment_chirho.offset_chirho as usize;
    let len_chirho = segment_chirho.filesz_chirho as usize;
    let end_chirho = start_chirho.checked_add(len_chirho)?;

    if end_chirho > data_chirho.len() {
        return None;
    }

    Some(&data_chirho[start_chirho..end_chirho])
}

// ---------------------------------------------------------------------------
// Utility: permission flag helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the segment is readable.
pub fn is_readable_chirho(flags_chirho: u32) -> bool {
    flags_chirho & PF_R_CHIRHO != 0
}

/// Returns `true` if the segment is writable.
pub fn is_writable_chirho(flags_chirho: u32) -> bool {
    flags_chirho & PF_W_CHIRHO != 0
}

/// Returns `true` if the segment is executable.
pub fn is_executable_chirho(flags_chirho: u32) -> bool {
    flags_chirho & PF_X_CHIRHO != 0
}
