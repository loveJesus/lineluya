// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! GPT (GUID Partition Table) parser for the Lineluya kernel.
//!
//! Parses GPT headers and partition entries from block devices,
//! enabling the kernel to locate ext4 and other partitions on disk.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::mem;

// ---------------------------------------------------------------------------
// GPT constants
// ---------------------------------------------------------------------------

/// GPT signature: "EFI PART"
const GPT_SIGNATURE_CHIRHO: u64 = 0x5452_4150_2049_4645;

/// LBA of the primary GPT header (always LBA 1)
pub const GPT_HEADER_LBA_CHIRHO: u64 = 1;

/// Standard sector size
const SECTOR_SIZE_CHIRHO: usize = 512;

/// Standard GPT partition entry size
const GPT_ENTRY_SIZE_CHIRHO: usize = 128;

// ---------------------------------------------------------------------------
// Well-known partition type GUIDs
// ---------------------------------------------------------------------------

/// Linux filesystem: 0FC63DAF-8483-4772-8E79-3D69D8477DE4
pub const LINUX_FS_GUID_CHIRHO: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47,
    0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];

/// EFI System Partition: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
pub const EFI_SYSTEM_GUID_CHIRHO: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
    0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];

/// Linux swap: 0657FD6D-A4AB-43C4-84E5-0933C84B4F4F
pub const LINUX_SWAP_GUID_CHIRHO: [u8; 16] = [
    0x6D, 0xFD, 0x57, 0x06, 0xAB, 0xA4, 0xC4, 0x43,
    0x84, 0xE5, 0x09, 0x33, 0xC8, 0x4B, 0x4F, 0x4F,
];

/// Empty/unused partition entry
const EMPTY_GUID_CHIRHO: [u8; 16] = [0u8; 16];

// ---------------------------------------------------------------------------
// GPT Header structure
// ---------------------------------------------------------------------------

/// GPT Header (LBA 1 for primary, last LBA for backup).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GptHeaderChirho {
    pub signature_chirho: u64,
    pub revision_chirho: u32,
    pub header_size_chirho: u32,
    pub header_crc32_chirho: u32,
    pub reserved_chirho: u32,
    pub my_lba_chirho: u64,
    pub alternate_lba_chirho: u64,
    pub first_usable_lba_chirho: u64,
    pub last_usable_lba_chirho: u64,
    pub disk_guid_chirho: [u8; 16],
    pub partition_entry_lba_chirho: u64,
    pub num_partition_entries_chirho: u32,
    pub partition_entry_size_chirho: u32,
    pub partition_entries_crc32_chirho: u32,
}

impl GptHeaderChirho {
    /// Validate the GPT header signature and basic fields.
    pub fn is_valid_chirho(&self) -> bool {
        self.signature_chirho == GPT_SIGNATURE_CHIRHO
            && self.header_size_chirho >= 92
            && self.partition_entry_size_chirho >= GPT_ENTRY_SIZE_CHIRHO as u32
            && self.num_partition_entries_chirho > 0
    }
}

// ---------------------------------------------------------------------------
// GPT Partition Entry
// ---------------------------------------------------------------------------

/// A single GPT partition entry (128 bytes minimum).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GptEntryChirho {
    pub type_guid_chirho: [u8; 16],
    pub unique_guid_chirho: [u8; 16],
    pub first_lba_chirho: u64,
    pub last_lba_chirho: u64,
    pub attributes_chirho: u64,
    pub name_chirho: [u16; 36], // UTF-16LE name
}

impl GptEntryChirho {
    /// Check if this entry is unused (empty type GUID).
    pub fn is_empty_chirho(&self) -> bool {
        self.type_guid_chirho == EMPTY_GUID_CHIRHO
    }

    /// Check if this is a Linux filesystem partition.
    pub fn is_linux_fs_chirho(&self) -> bool {
        self.type_guid_chirho == LINUX_FS_GUID_CHIRHO
    }

    /// Check if this is an EFI System Partition.
    pub fn is_efi_system_chirho(&self) -> bool {
        self.type_guid_chirho == EFI_SYSTEM_GUID_CHIRHO
    }

    /// Get partition size in sectors.
    pub fn size_sectors_chirho(&self) -> u64 {
        if self.last_lba_chirho >= self.first_lba_chirho {
            self.last_lba_chirho - self.first_lba_chirho + 1
        } else {
            0
        }
    }

    /// Get partition size in bytes (assuming 512-byte sectors).
    pub fn size_bytes_chirho(&self) -> u64 {
        self.size_sectors_chirho() * SECTOR_SIZE_CHIRHO as u64
    }

    /// Decode the UTF-16LE partition name to an ASCII string.
    pub fn name_string_chirho(&self) -> String {
        let mut s_chirho = String::new();
        let name_copy_chirho = self.name_chirho;
        for ch_chirho in name_copy_chirho {
            if ch_chirho == 0 {
                break;
            }
            if ch_chirho < 128 {
                s_chirho.push(ch_chirho as u8 as char);
            } else {
                s_chirho.push('?');
            }
        }
        s_chirho
    }
}

// ---------------------------------------------------------------------------
// Parsed partition info
// ---------------------------------------------------------------------------

/// A parsed partition entry with useful metadata.
#[derive(Debug, Clone)]
pub struct PartitionInfoChirho {
    pub index_chirho: u32,
    pub first_lba_chirho: u64,
    pub last_lba_chirho: u64,
    pub size_bytes_chirho: u64,
    pub name_chirho: String,
    pub type_guid_chirho: [u8; 16],
    pub is_linux_chirho: bool,
    pub is_efi_chirho: bool,
}

// ---------------------------------------------------------------------------
// GPT parsing functions
// ---------------------------------------------------------------------------

/// Parse GPT header from raw sector data (512 bytes of LBA 1).
pub fn parse_gpt_header_chirho(sector_data_chirho: &[u8]) -> Option<GptHeaderChirho> {
    if sector_data_chirho.len() < mem::size_of::<GptHeaderChirho>() {
        return None;
    }

    let header_chirho: GptHeaderChirho = unsafe {
        core::ptr::read_unaligned(sector_data_chirho.as_ptr() as *const GptHeaderChirho)
    };

    if header_chirho.is_valid_chirho() {
        Some(header_chirho)
    } else {
        None
    }
}

/// Parse all non-empty GPT partition entries from raw data.
///
/// `entries_data_chirho` should contain all partition entry sectors
/// (typically LBAs 2..33 for 128 entries).
pub fn parse_gpt_entries_chirho(
    entries_data_chirho: &[u8],
    num_entries_chirho: u32,
    entry_size_chirho: u32,
) -> Vec<PartitionInfoChirho> {
    let mut partitions_chirho = Vec::new();
    let entry_sz_chirho = entry_size_chirho as usize;

    for i_chirho in 0..num_entries_chirho as usize {
        let offset_chirho = i_chirho * entry_sz_chirho;
        if offset_chirho + mem::size_of::<GptEntryChirho>() > entries_data_chirho.len() {
            break;
        }

        let entry_chirho: GptEntryChirho = unsafe {
            core::ptr::read_unaligned(
                entries_data_chirho.as_ptr().add(offset_chirho) as *const GptEntryChirho,
            )
        };

        if entry_chirho.is_empty_chirho() {
            continue;
        }

        partitions_chirho.push(PartitionInfoChirho {
            index_chirho: i_chirho as u32,
            first_lba_chirho: entry_chirho.first_lba_chirho,
            last_lba_chirho: entry_chirho.last_lba_chirho,
            size_bytes_chirho: entry_chirho.size_bytes_chirho(),
            name_chirho: entry_chirho.name_string_chirho(),
            type_guid_chirho: entry_chirho.type_guid_chirho,
            is_linux_chirho: entry_chirho.is_linux_fs_chirho(),
            is_efi_chirho: entry_chirho.is_efi_system_chirho(),
        });
    }

    partitions_chirho
}

/// Find the first Linux filesystem partition in the partition table.
pub fn find_linux_root_chirho(partitions_chirho: &[PartitionInfoChirho]) -> Option<&PartitionInfoChirho> {
    partitions_chirho.iter().find(|p_chirho| p_chirho.is_linux_chirho)
}
