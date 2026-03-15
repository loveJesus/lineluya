// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ext4 filesystem implementation for the Lineluya kernel.
//!
//! Parses ext4 superblocks, block group descriptors, inodes, extent trees,
//! and directory entries. Integrates with the VFS layer for mount/read/write.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// ext4 constants
// ---------------------------------------------------------------------------

/// ext4 magic number (same as ext2/ext3)
pub const EXT4_MAGIC_CHIRHO: u16 = 0xEF53;

/// Superblock is always at byte offset 1024 from partition start
pub const SUPERBLOCK_OFFSET_CHIRHO: u64 = 1024;

/// Default block size shift base
const BLOCK_SIZE_BASE_CHIRHO: u32 = 1024;

// Feature flags
pub const COMPAT_HAS_JOURNAL_CHIRHO: u32 = 0x0004;
pub const COMPAT_EXT_ATTR_CHIRHO: u32 = 0x0008;
pub const COMPAT_DIR_INDEX_CHIRHO: u32 = 0x0020;

pub const INCOMPAT_FILETYPE_CHIRHO: u32 = 0x0002;
pub const INCOMPAT_EXTENTS_CHIRHO: u32 = 0x0040;
pub const INCOMPAT_64BIT_CHIRHO: u32 = 0x0080;
pub const INCOMPAT_FLEX_BG_CHIRHO: u32 = 0x0200;

pub const RO_COMPAT_SPARSE_SUPER_CHIRHO: u32 = 0x0001;
pub const RO_COMPAT_LARGE_FILE_CHIRHO: u32 = 0x0002;
pub const RO_COMPAT_HUGE_FILE_CHIRHO: u32 = 0x0008;
pub const RO_COMPAT_GDT_CSUM_CHIRHO: u32 = 0x0010;
pub const RO_COMPAT_METADATA_CSUM_CHIRHO: u32 = 0x0400;

// Inode constants
pub const EXT4_ROOT_INO_CHIRHO: u32 = 2;
pub const EXT4_GOOD_OLD_INODE_SIZE_CHIRHO: u16 = 128;

// File type constants (from directory entries)
pub const FT_UNKNOWN_CHIRHO: u8 = 0;
pub const FT_REG_FILE_CHIRHO: u8 = 1;
pub const FT_DIR_CHIRHO: u8 = 2;
pub const FT_CHRDEV_CHIRHO: u8 = 3;
pub const FT_BLKDEV_CHIRHO: u8 = 4;
pub const FT_FIFO_CHIRHO: u8 = 5;
pub const FT_SOCK_CHIRHO: u8 = 6;
pub const FT_SYMLINK_CHIRHO: u8 = 7;

// Inode mode flags
pub const S_IFMT_CHIRHO: u16 = 0xF000;
pub const S_IFREG_CHIRHO: u16 = 0x8000;
pub const S_IFDIR_CHIRHO: u16 = 0x4000;
pub const S_IFLNK_CHIRHO: u16 = 0xA000;
pub const S_IFBLK_CHIRHO: u16 = 0x6000;
pub const S_IFCHR_CHIRHO: u16 = 0x2000;
pub const S_IFIFO_CHIRHO: u16 = 0x1000;
pub const S_IFSOCK_CHIRHO: u16 = 0xC000;

// Extent magic
pub const EXT4_EXT_MAGIC_CHIRHO: u16 = 0xF30A;

// ---------------------------------------------------------------------------
// Superblock (A4-004)
// ---------------------------------------------------------------------------

/// ext4 superblock — first 256 bytes of the most important fields.
/// Located at byte offset 1024 from partition/device start.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4SuperblockChirho {
    pub s_inodes_count_chirho: u32,          // Total inode count
    pub s_blocks_count_lo_chirho: u32,       // Total block count (low 32 bits)
    pub s_r_blocks_count_lo_chirho: u32,     // Reserved block count (low)
    pub s_free_blocks_count_lo_chirho: u32,  // Free block count (low)
    pub s_free_inodes_count_chirho: u32,     // Free inode count
    pub s_first_data_block_chirho: u32,      // First data block (0 for 4K blocks, 1 for 1K)
    pub s_log_block_size_chirho: u32,        // Block size = 1024 << this
    pub s_log_cluster_size_chirho: u32,      // Cluster size
    pub s_blocks_per_group_chirho: u32,      // Blocks per group
    pub s_clusters_per_group_chirho: u32,    // Clusters per group
    pub s_inodes_per_group_chirho: u32,      // Inodes per group
    pub s_mtime_chirho: u32,                 // Mount time
    pub s_wtime_chirho: u32,                 // Write time
    pub s_mnt_count_chirho: u16,             // Mount count
    pub s_max_mnt_count_chirho: u16,         // Max mount count
    pub s_magic_chirho: u16,                 // Magic number (0xEF53)
    pub s_state_chirho: u16,                 // Filesystem state
    pub s_errors_chirho: u16,                // Error behavior
    pub s_minor_rev_level_chirho: u16,       // Minor revision level
    pub s_lastcheck_chirho: u32,             // Last check time
    pub s_checkinterval_chirho: u32,         // Check interval
    pub s_creator_os_chirho: u32,            // Creator OS
    pub s_rev_level_chirho: u32,             // Revision level
    pub s_def_resuid_chirho: u16,            // Default uid for reserved blocks
    pub s_def_resgid_chirho: u16,            // Default gid for reserved blocks
    // -- EXT4_DYNAMIC_REV fields --
    pub s_first_ino_chirho: u32,             // First non-reserved inode
    pub s_inode_size_chirho: u16,            // Inode size
    pub s_block_group_nr_chirho: u16,        // Block group of this superblock
    pub s_feature_compat_chirho: u32,        // Compatible features
    pub s_feature_incompat_chirho: u32,      // Incompatible features
    pub s_feature_ro_compat_chirho: u32,     // Read-only compatible features
    pub s_uuid_chirho: [u8; 16],             // 128-bit filesystem UUID
    pub s_volume_name_chirho: [u8; 16],      // Volume name
    pub s_last_mounted_chirho: [u8; 64],     // Last mount point
    pub s_algorithm_usage_bitmap_chirho: u32,// Compression
    // Preallocation
    pub s_prealloc_blocks_chirho: u8,
    pub s_prealloc_dir_blocks_chirho: u8,
    pub s_reserved_gdt_blocks_chirho: u16,
    // Journal
    pub s_journal_uuid_chirho: [u8; 16],
    pub s_journal_inum_chirho: u32,
    pub s_journal_dev_chirho: u32,
    pub s_last_orphan_chirho: u32,
    pub s_hash_seed_chirho: [u32; 4],
    pub s_def_hash_version_chirho: u8,
    pub s_jnl_backup_type_chirho: u8,
    pub s_desc_size_chirho: u16,             // Group descriptor size
    pub s_default_mount_opts_chirho: u32,
    pub s_first_meta_bg_chirho: u32,
    pub s_mkfs_time_chirho: u32,
    pub s_jnl_blocks_chirho: [u32; 17],
    // 64-bit support
    pub s_blocks_count_hi_chirho: u32,
    pub s_r_blocks_count_hi_chirho: u32,
    pub s_free_blocks_count_hi_chirho: u32,
    pub s_min_extra_isize_chirho: u16,
    pub s_want_extra_isize_chirho: u16,
    pub s_flags_chirho: u32,
}

impl Ext4SuperblockChirho {
    /// Validate the superblock magic number.
    pub fn is_valid_chirho(&self) -> bool {
        self.s_magic_chirho == EXT4_MAGIC_CHIRHO
    }

    /// Get the block size in bytes.
    pub fn block_size_chirho(&self) -> u32 {
        BLOCK_SIZE_BASE_CHIRHO << self.s_log_block_size_chirho
    }

    /// Get total block count (64-bit).
    pub fn total_blocks_chirho(&self) -> u64 {
        self.s_blocks_count_lo_chirho as u64
            | ((self.s_blocks_count_hi_chirho as u64) << 32)
    }

    /// Get free block count (64-bit).
    pub fn free_blocks_chirho(&self) -> u64 {
        self.s_free_blocks_count_lo_chirho as u64
            | ((self.s_free_blocks_count_hi_chirho as u64) << 32)
    }

    /// Get the number of block groups.
    pub fn block_group_count_chirho(&self) -> u32 {
        let total_chirho = self.total_blocks_chirho();
        let per_group_chirho = self.s_blocks_per_group_chirho as u64;
        ((total_chirho + per_group_chirho - 1) / per_group_chirho) as u32
    }

    /// Get the group descriptor size (32 or 64 bytes).
    pub fn group_desc_size_chirho(&self) -> u32 {
        if self.s_feature_incompat_chirho & INCOMPAT_64BIT_CHIRHO != 0 && self.s_desc_size_chirho >= 64 {
            self.s_desc_size_chirho as u32
        } else {
            32
        }
    }

    /// Get the inode size.
    pub fn inode_size_chirho(&self) -> u32 {
        if self.s_rev_level_chirho > 0 && self.s_inode_size_chirho > 0 {
            self.s_inode_size_chirho as u32
        } else {
            EXT4_GOOD_OLD_INODE_SIZE_CHIRHO as u32
        }
    }

    /// Check if extents feature is enabled.
    pub fn has_extents_chirho(&self) -> bool {
        self.s_feature_incompat_chirho & INCOMPAT_EXTENTS_CHIRHO != 0
    }

    /// Check if 64-bit feature is enabled.
    pub fn has_64bit_chirho(&self) -> bool {
        self.s_feature_incompat_chirho & INCOMPAT_64BIT_CHIRHO != 0
    }

    /// Check if journal feature is enabled.
    pub fn has_journal_chirho(&self) -> bool {
        self.s_feature_compat_chirho & COMPAT_HAS_JOURNAL_CHIRHO != 0
    }

    /// Get filesystem size in bytes.
    pub fn fs_size_bytes_chirho(&self) -> u64 {
        self.total_blocks_chirho() * self.block_size_chirho() as u64
    }

    /// Get the volume name as a string.
    pub fn volume_name_chirho(&self) -> String {
        let mut name_chirho = String::new();
        for &b_chirho in &self.s_volume_name_chirho {
            if b_chirho == 0 {
                break;
            }
            name_chirho.push(b_chirho as char);
        }
        name_chirho
    }
}

// ---------------------------------------------------------------------------
// Block Group Descriptor (A4-005)
// ---------------------------------------------------------------------------

/// ext4 block group descriptor (32 bytes, or 64 bytes with 64-bit feature).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4GroupDescChirho {
    pub bg_block_bitmap_lo_chirho: u32,
    pub bg_inode_bitmap_lo_chirho: u32,
    pub bg_inode_table_lo_chirho: u32,
    pub bg_free_blocks_count_lo_chirho: u16,
    pub bg_free_inodes_count_lo_chirho: u16,
    pub bg_used_dirs_count_lo_chirho: u16,
    pub bg_flags_chirho: u16,
    pub bg_exclude_bitmap_lo_chirho: u32,
    pub bg_block_bitmap_csum_lo_chirho: u16,
    pub bg_inode_bitmap_csum_lo_chirho: u16,
    pub bg_itable_unused_lo_chirho: u16,
    pub bg_checksum_chirho: u16,
    // 64-bit extensions (only present when desc_size >= 64)
    pub bg_block_bitmap_hi_chirho: u32,
    pub bg_inode_bitmap_hi_chirho: u32,
    pub bg_inode_table_hi_chirho: u32,
    pub bg_free_blocks_count_hi_chirho: u16,
    pub bg_free_inodes_count_hi_chirho: u16,
    pub bg_used_dirs_count_hi_chirho: u16,
    pub bg_itable_unused_hi_chirho: u16,
    pub bg_exclude_bitmap_hi_chirho: u32,
    pub bg_block_bitmap_csum_hi_chirho: u16,
    pub bg_inode_bitmap_csum_hi_chirho: u16,
    pub bg_reserved_chirho: u32,
}

impl Ext4GroupDescChirho {
    /// Get block bitmap block number (64-bit).
    pub fn block_bitmap_chirho(&self, has_64bit_chirho: bool) -> u64 {
        if has_64bit_chirho {
            self.bg_block_bitmap_lo_chirho as u64
                | ((self.bg_block_bitmap_hi_chirho as u64) << 32)
        } else {
            self.bg_block_bitmap_lo_chirho as u64
        }
    }

    /// Get inode bitmap block number (64-bit).
    pub fn inode_bitmap_chirho(&self, has_64bit_chirho: bool) -> u64 {
        if has_64bit_chirho {
            self.bg_inode_bitmap_lo_chirho as u64
                | ((self.bg_inode_bitmap_hi_chirho as u64) << 32)
        } else {
            self.bg_inode_bitmap_lo_chirho as u64
        }
    }

    /// Get inode table start block (64-bit).
    pub fn inode_table_chirho(&self, has_64bit_chirho: bool) -> u64 {
        if has_64bit_chirho {
            self.bg_inode_table_lo_chirho as u64
                | ((self.bg_inode_table_hi_chirho as u64) << 32)
        } else {
            self.bg_inode_table_lo_chirho as u64
        }
    }

    /// Get free blocks count (32-bit, 64-bit aware).
    pub fn free_blocks_count_chirho(&self, has_64bit_chirho: bool) -> u32 {
        if has_64bit_chirho {
            self.bg_free_blocks_count_lo_chirho as u32
                | ((self.bg_free_blocks_count_hi_chirho as u32) << 16)
        } else {
            self.bg_free_blocks_count_lo_chirho as u32
        }
    }

    /// Get free inodes count.
    pub fn free_inodes_count_chirho(&self, has_64bit_chirho: bool) -> u32 {
        if has_64bit_chirho {
            self.bg_free_inodes_count_lo_chirho as u32
                | ((self.bg_free_inodes_count_hi_chirho as u32) << 16)
        } else {
            self.bg_free_inodes_count_lo_chirho as u32
        }
    }
}

// ---------------------------------------------------------------------------
// ext4 Inode (A4-006)
// ---------------------------------------------------------------------------

/// ext4 inode (128 bytes base + optional extra).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4InodeChirho {
    pub i_mode_chirho: u16,
    pub i_uid_chirho: u16,
    pub i_size_lo_chirho: u32,
    pub i_atime_chirho: u32,
    pub i_ctime_chirho: u32,
    pub i_mtime_chirho: u32,
    pub i_dtime_chirho: u32,
    pub i_gid_chirho: u16,
    pub i_links_count_chirho: u16,
    pub i_blocks_lo_chirho: u32,
    pub i_flags_chirho: u32,
    pub i_osd1_chirho: u32,
    pub i_block_chirho: [u32; 15],  // 60 bytes: block pointers or extent tree
    pub i_generation_chirho: u32,
    pub i_file_acl_lo_chirho: u32,
    pub i_size_high_chirho: u32,
    pub i_obso_faddr_chirho: u32,
    pub i_osd2_chirho: [u8; 12],
}

impl Ext4InodeChirho {
    /// Get file size (64-bit for large files).
    pub fn size_chirho(&self) -> u64 {
        self.i_size_lo_chirho as u64
            | ((self.i_size_high_chirho as u64) << 32)
    }

    /// Check if this inode is a regular file.
    pub fn is_file_chirho(&self) -> bool {
        (self.i_mode_chirho & S_IFMT_CHIRHO) == S_IFREG_CHIRHO
    }

    /// Check if this inode is a directory.
    pub fn is_dir_chirho(&self) -> bool {
        (self.i_mode_chirho & S_IFMT_CHIRHO) == S_IFDIR_CHIRHO
    }

    /// Check if this inode is a symbolic link.
    pub fn is_symlink_chirho(&self) -> bool {
        (self.i_mode_chirho & S_IFMT_CHIRHO) == S_IFLNK_CHIRHO
    }

    /// Check if this inode uses extents (vs block map).
    pub fn uses_extents_chirho(&self) -> bool {
        self.i_flags_chirho & 0x00080000 != 0 // EXT4_EXTENTS_FL
    }

    /// Get the extent header from i_block (first 12 bytes).
    pub fn extent_header_chirho(&self) -> Ext4ExtentHeaderChirho {
        let block_copy_chirho = self.i_block_chirho;
        unsafe {
            core::ptr::read_unaligned(
                block_copy_chirho.as_ptr() as *const Ext4ExtentHeaderChirho,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Extent Tree (A4-007)
// ---------------------------------------------------------------------------

/// ext4 extent tree header (12 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4ExtentHeaderChirho {
    pub eh_magic_chirho: u16,
    pub eh_entries_chirho: u16,
    pub eh_max_chirho: u16,
    pub eh_depth_chirho: u16,
    pub eh_generation_chirho: u32,
}

impl Ext4ExtentHeaderChirho {
    pub fn is_valid_chirho(&self) -> bool {
        self.eh_magic_chirho == EXT4_EXT_MAGIC_CHIRHO
    }
}

/// ext4 extent (leaf node, 12 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4ExtentChirho {
    pub ee_block_chirho: u32,     // First file block covered
    pub ee_len_chirho: u16,       // Number of blocks covered
    pub ee_start_hi_chirho: u16,  // High 16 bits of physical block
    pub ee_start_lo_chirho: u32,  // Low 32 bits of physical block
}

impl Ext4ExtentChirho {
    /// Get the physical start block (48-bit).
    pub fn physical_block_chirho(&self) -> u64 {
        self.ee_start_lo_chirho as u64
            | ((self.ee_start_hi_chirho as u64) << 32)
    }

    /// Get the number of blocks (uninitialized extents have high bit set).
    pub fn block_count_chirho(&self) -> u32 {
        (self.ee_len_chirho & 0x7FFF) as u32
    }

    /// Check if this is an uninitialized extent.
    pub fn is_uninitialized_chirho(&self) -> bool {
        self.ee_len_chirho & 0x8000 != 0
    }
}

/// ext4 extent index (internal node, 12 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4ExtentIdxChirho {
    pub ei_block_chirho: u32,     // Logical block covered
    pub ei_leaf_lo_chirho: u32,   // Low 32 bits of child node block
    pub ei_leaf_hi_chirho: u16,   // High 16 bits of child node block
    pub ei_unused_chirho: u16,
}

impl Ext4ExtentIdxChirho {
    /// Get the physical block of the child node (48-bit).
    pub fn child_block_chirho(&self) -> u64 {
        self.ei_leaf_lo_chirho as u64
            | ((self.ei_leaf_hi_chirho as u64) << 32)
    }
}

// ---------------------------------------------------------------------------
// Directory Entry (A4-008)
// ---------------------------------------------------------------------------

/// ext4 directory entry (variable length).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4DirEntryChirho {
    pub inode_chirho: u32,
    pub rec_len_chirho: u16,
    pub name_len_chirho: u8,
    pub file_type_chirho: u8,
    // name bytes follow (name_len bytes, NOT null-terminated)
}

/// A parsed directory entry with its name.
#[derive(Debug, Clone)]
pub struct DirEntryInfoChirho {
    pub inode_chirho: u32,
    pub name_chirho: String,
    pub file_type_chirho: u8,
}

// ---------------------------------------------------------------------------
// Superblock parsing function
// ---------------------------------------------------------------------------

/// Parse an ext4 superblock from raw data (at least 1024 bytes).
pub fn parse_superblock_chirho(data_chirho: &[u8]) -> Option<Ext4SuperblockChirho> {
    if data_chirho.len() < core::mem::size_of::<Ext4SuperblockChirho>() {
        return None;
    }

    let sb_chirho: Ext4SuperblockChirho = unsafe {
        core::ptr::read_unaligned(data_chirho.as_ptr() as *const Ext4SuperblockChirho)
    };

    if sb_chirho.is_valid_chirho() {
        Some(sb_chirho)
    } else {
        None
    }
}

/// Parse block group descriptors from raw data.
pub fn parse_group_descs_chirho(
    data_chirho: &[u8],
    count_chirho: u32,
    desc_size_chirho: u32,
) -> Vec<Ext4GroupDescChirho> {
    let mut descs_chirho = Vec::new();
    let sz_chirho = desc_size_chirho as usize;

    for i_chirho in 0..count_chirho as usize {
        let offset_chirho = i_chirho * sz_chirho;
        if offset_chirho + 32 > data_chirho.len() {
            break;
        }

        let desc_chirho: Ext4GroupDescChirho = unsafe {
            core::ptr::read_unaligned(
                data_chirho.as_ptr().add(offset_chirho) as *const Ext4GroupDescChirho,
            )
        };
        descs_chirho.push(desc_chirho);
    }

    descs_chirho
}

/// Parse directory entries from a directory block.
pub fn parse_dir_entries_chirho(block_data_chirho: &[u8]) -> Vec<DirEntryInfoChirho> {
    let mut entries_chirho = Vec::new();
    let mut offset_chirho = 0usize;

    while offset_chirho + 8 <= block_data_chirho.len() {
        let entry_chirho: Ext4DirEntryChirho = unsafe {
            core::ptr::read_unaligned(
                block_data_chirho.as_ptr().add(offset_chirho) as *const Ext4DirEntryChirho,
            )
        };

        if entry_chirho.rec_len_chirho == 0 {
            break;
        }

        if entry_chirho.inode_chirho != 0 && entry_chirho.name_len_chirho > 0 {
            let name_start_chirho = offset_chirho + 8;
            let name_end_chirho = name_start_chirho + entry_chirho.name_len_chirho as usize;
            if name_end_chirho <= block_data_chirho.len() {
                let name_bytes_chirho = &block_data_chirho[name_start_chirho..name_end_chirho];
                let name_chirho = core::str::from_utf8(name_bytes_chirho)
                    .map(|s_chirho| String::from(s_chirho))
                    .unwrap_or_else(|_| String::from("?"));

                entries_chirho.push(DirEntryInfoChirho {
                    inode_chirho: entry_chirho.inode_chirho,
                    name_chirho,
                    file_type_chirho: entry_chirho.file_type_chirho,
                });
            }
        }

        offset_chirho += entry_chirho.rec_len_chirho as usize;
    }

    entries_chirho
}

/// Calculate the block group and local inode index for a given inode number.
pub fn inode_to_group_chirho(
    ino_chirho: u32,
    inodes_per_group_chirho: u32,
) -> (u32, u32) {
    let group_chirho = (ino_chirho - 1) / inodes_per_group_chirho;
    let local_chirho = (ino_chirho - 1) % inodes_per_group_chirho;
    (group_chirho, local_chirho)
}

/// Calculate the byte offset of an inode within the inode table.
pub fn inode_table_offset_chirho(
    local_index_chirho: u32,
    inode_size_chirho: u32,
) -> u64 {
    local_index_chirho as u64 * inode_size_chirho as u64
}
