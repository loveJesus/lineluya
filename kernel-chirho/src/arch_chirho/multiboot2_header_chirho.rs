// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Multiboot2 header for GRUB booting the Lineluya kernel (E1-001).
//!
//! When the kernel ELF is loaded by a Multiboot2-compliant bootloader
//! (e.g. GRUB2), the bootloader scans the first 32 KiB of the binary for
//! a header with the magic value `0xE85250D6`. This module places such a
//! header in a dedicated `.multiboot2_header` section so the linker script
//! can arrange it at the very start of the binary.
//!
//! Reference: <https://www.gnu.org/software/grub/manual/multiboot2/multiboot.html>

// ============================================================================
// Multiboot2 constants
// ============================================================================

/// Multiboot2 header magic value.
pub const MULTIBOOT2_HEADER_MAGIC_CHIRHO: u32 = 0xE852_50D6;

/// Architecture: i386 (protected mode, 32-bit).
pub const MULTIBOOT2_ARCHITECTURE_I386_CHIRHO: u32 = 0;

/// Multiboot2 header length (we use a minimal header: 24 bytes).
/// Header (16 bytes) + end tag (8 bytes).
pub const MULTIBOOT2_HEADER_LENGTH_CHIRHO: u32 = 24;

/// Checksum: -(magic + architecture + header_length) mod 2^32.
pub const MULTIBOOT2_HEADER_CHECKSUM_CHIRHO: u32 =
    (0u32.wrapping_sub(
        MULTIBOOT2_HEADER_MAGIC_CHIRHO
            .wrapping_add(MULTIBOOT2_ARCHITECTURE_I386_CHIRHO)
            .wrapping_add(MULTIBOOT2_HEADER_LENGTH_CHIRHO),
    ));

/// Multiboot2 bootloader magic value placed in EAX on entry.
#[allow(dead_code)]
pub const MULTIBOOT2_BOOTLOADER_MAGIC_CHIRHO: u32 = 0x36D7_6289;

/// Tag type for end tag.
pub const MULTIBOOT2_TAG_TYPE_END_CHIRHO: u16 = 0;

// ============================================================================
// Multiboot2 header structure
// ============================================================================

/// The Multiboot2 header placed at the start of the kernel image.
///
/// This struct is `repr(C)` and placed in a `.multiboot2_header` section.
/// GRUB scans the first 32 KiB of the loaded ELF for this magic.
#[repr(C, align(8))]
pub struct Multiboot2HeaderChirho {
    /// Must be `MULTIBOOT2_HEADER_MAGIC_CHIRHO` (0xE85250D6).
    pub magic_chirho: u32,
    /// Architecture (0 = i386 protected mode).
    pub architecture_chirho: u32,
    /// Total header length in bytes (including tags).
    pub header_length_chirho: u32,
    /// Checksum: all header fields must sum to zero (mod 2^32).
    pub checksum_chirho: u32,
    /// End tag: type=0, flags=0, size=8.
    pub end_tag_type_chirho: u16,
    pub end_tag_flags_chirho: u16,
    pub end_tag_size_chirho: u32,
}

// ============================================================================
// Static header instance (placed in .multiboot2_header section)
// ============================================================================

/// The actual Multiboot2 header. Linked into `.multiboot2_header` section.
///
/// The linker script must place this section within the first 32 KiB of the
/// final binary for GRUB to discover it.
#[used]
#[link_section = ".multiboot2_header"]
pub static MULTIBOOT2_HEADER_CHIRHO: Multiboot2HeaderChirho = Multiboot2HeaderChirho {
    magic_chirho: MULTIBOOT2_HEADER_MAGIC_CHIRHO,
    architecture_chirho: MULTIBOOT2_ARCHITECTURE_I386_CHIRHO,
    header_length_chirho: MULTIBOOT2_HEADER_LENGTH_CHIRHO,
    checksum_chirho: MULTIBOOT2_HEADER_CHECKSUM_CHIRHO,
    end_tag_type_chirho: MULTIBOOT2_TAG_TYPE_END_CHIRHO,
    end_tag_flags_chirho: 0,
    end_tag_size_chirho: 8,
};

// ============================================================================
// Multiboot2 boot information parsing
// ============================================================================

/// Multiboot2 tag header — common prefix for all tags in the boot info.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct Multiboot2TagChirho {
    pub type_chirho: u32,
    pub size_chirho: u32,
}

/// Tag type constants for Multiboot2 boot information.
#[allow(dead_code)]
pub const MB2_TAG_END_CHIRHO: u32 = 0;
#[allow(dead_code)]
pub const MB2_TAG_CMDLINE_CHIRHO: u32 = 1;
#[allow(dead_code)]
pub const MB2_TAG_BOOT_LOADER_NAME_CHIRHO: u32 = 2;
#[allow(dead_code)]
pub const MB2_TAG_MODULE_CHIRHO: u32 = 3;
#[allow(dead_code)]
pub const MB2_TAG_BASIC_MEMINFO_CHIRHO: u32 = 4;
#[allow(dead_code)]
pub const MB2_TAG_MMAP_CHIRHO: u32 = 6;
#[allow(dead_code)]
pub const MB2_TAG_FRAMEBUFFER_CHIRHO: u32 = 8;
#[allow(dead_code)]
pub const MB2_TAG_ACPI_OLD_CHIRHO: u32 = 14;
#[allow(dead_code)]
pub const MB2_TAG_ACPI_NEW_CHIRHO: u32 = 15;

/// Parse the Multiboot2 boot information structure and extract the
/// kernel command line string (tag type 1).
///
/// # Safety
/// `mbi_ptr_chirho` must point to a valid Multiboot2 boot information
/// structure in mapped memory. EBX holds this pointer on entry from GRUB.
#[allow(dead_code)]
pub unsafe fn parse_mb2_cmdline_chirho(mbi_ptr_chirho: *const u8) -> Option<&'static str> {
    // The first 8 bytes are total_size (u32) + reserved (u32).
    let total_size_chirho = unsafe { *(mbi_ptr_chirho as *const u32) } as usize;
    let mut offset_chirho: usize = 8; // skip total_size + reserved

    while offset_chirho < total_size_chirho {
        let tag_ptr_chirho = unsafe { mbi_ptr_chirho.add(offset_chirho) as *const Multiboot2TagChirho };
        let tag_chirho = unsafe { &*tag_ptr_chirho };

        if tag_chirho.type_chirho == MB2_TAG_END_CHIRHO {
            break;
        }

        if tag_chirho.type_chirho == MB2_TAG_CMDLINE_CHIRHO {
            // Command line string starts right after the 8-byte tag header.
            let str_ptr_chirho = unsafe { mbi_ptr_chirho.add(offset_chirho + 8) };
            let str_len_chirho = (tag_chirho.size_chirho as usize).saturating_sub(9); // minus header + NUL
            let bytes_chirho = unsafe { core::slice::from_raw_parts(str_ptr_chirho, str_len_chirho) };
            if let Ok(s_chirho) = core::str::from_utf8(bytes_chirho) {
                return Some(s_chirho);
            }
        }

        // Tags are 8-byte aligned.
        let tag_size_chirho = tag_chirho.size_chirho as usize;
        offset_chirho += (tag_size_chirho + 7) & !7;
    }

    None
}

/// Parse the Multiboot2 boot information and extract the bootloader name
/// (tag type 2).
///
/// # Safety
/// Same requirements as `parse_mb2_cmdline_chirho`.
#[allow(dead_code)]
pub unsafe fn parse_mb2_bootloader_name_chirho(mbi_ptr_chirho: *const u8) -> Option<&'static str> {
    let total_size_chirho = unsafe { *(mbi_ptr_chirho as *const u32) } as usize;
    let mut offset_chirho: usize = 8;

    while offset_chirho < total_size_chirho {
        let tag_ptr_chirho = unsafe { mbi_ptr_chirho.add(offset_chirho) as *const Multiboot2TagChirho };
        let tag_chirho = unsafe { &*tag_ptr_chirho };

        if tag_chirho.type_chirho == MB2_TAG_END_CHIRHO {
            break;
        }

        if tag_chirho.type_chirho == MB2_TAG_BOOT_LOADER_NAME_CHIRHO {
            let str_ptr_chirho = unsafe { mbi_ptr_chirho.add(offset_chirho + 8) };
            let str_len_chirho = (tag_chirho.size_chirho as usize).saturating_sub(9);
            let bytes_chirho = unsafe { core::slice::from_raw_parts(str_ptr_chirho, str_len_chirho) };
            if let Ok(s_chirho) = core::str::from_utf8(bytes_chirho) {
                return Some(s_chirho);
            }
        }

        let tag_size_chirho = tag_chirho.size_chirho as usize;
        offset_chirho += (tag_size_chirho + 7) & !7;
    }

    None
}
