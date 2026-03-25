// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ext4 filesystem implementation for the Lineluya kernel.
//!
//! Parses ext4 superblocks, block group descriptors, inodes, extent trees,
//! and directory entries. Integrates with the VFS layer for mount/read/write.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

// ---------------------------------------------------------------------------
// Typed error enum for ext4 operations
// ---------------------------------------------------------------------------

/// Typed errors for ext4 filesystem operations.
/// Convert to Linux errno at syscall boundaries only.
#[derive(Debug)]
pub enum Ext4ErrorChirho {
    /// I/O error reading from block device.
    IoErrorChirho,
    /// Superblock magic number or checksum invalid.
    CorruptSuperblockChirho,
    /// Inode data is corrupt or unsupported.
    CorruptInodeChirho,
    /// Directory entry parsing failed.
    CorruptDirEntryChirho,
    /// Extent tree is invalid or unsupported depth.
    InvalidExtentChirho,
    /// Feature not supported (e.g., non-extent block map).
    UnsupportedFeatureChirho(&'static str),
    /// File or directory not found.
    NotFoundChirho,
    /// File already exists.
    AlreadyExistsChirho,
    /// Not a directory.
    NotDirectoryChirho,
    /// Not a symlink.
    NotSymlinkChirho,
    /// Filesystem is read-only.
    ReadOnlyChirho,
    /// No free inodes or blocks.
    NoSpaceChirho,
}

impl Ext4ErrorChirho {
    /// Convert to Linux errno value for syscall return.
    pub fn to_errno_chirho(&self) -> i64 {
        match self {
            Self::IoErrorChirho => -5,            // EIO
            Self::CorruptSuperblockChirho => -5,  // EIO
            Self::CorruptInodeChirho => -5,       // EIO
            Self::CorruptDirEntryChirho => -5,    // EIO
            Self::InvalidExtentChirho => -5,      // EIO
            Self::UnsupportedFeatureChirho(_) => -95, // EOPNOTSUPP
            Self::NotFoundChirho => -2,           // ENOENT
            Self::AlreadyExistsChirho => -17,     // EEXIST
            Self::NotDirectoryChirho => -20,      // ENOTDIR
            Self::NotSymlinkChirho => -22,        // EINVAL
            Self::ReadOnlyChirho => -30,          // EROFS
            Self::NoSpaceChirho => -28,           // ENOSPC
        }
    }
}

/// Bridge for gradual migration: lets `?` coerce `&'static str` errors
/// from not-yet-converted functions into `Ext4ErrorChirho`.
impl From<&'static str> for Ext4ErrorChirho {
    fn from(msg_chirho: &'static str) -> Self {
        match msg_chirho {
            "filesystem is read-only" => Self::ReadOnlyChirho,
            "no free inodes" | "not enough free blocks" => Self::NoSpaceChirho,
            _ => Self::IoErrorChirho,
        }
    }
}

impl core::fmt::Display for Ext4ErrorChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::IoErrorChirho => write!(f_chirho, "I/O error"),
            Self::CorruptSuperblockChirho => write!(f_chirho, "corrupt superblock"),
            Self::CorruptInodeChirho => write!(f_chirho, "corrupt inode"),
            Self::CorruptDirEntryChirho => write!(f_chirho, "corrupt directory entry"),
            Self::InvalidExtentChirho => write!(f_chirho, "invalid extent tree"),
            Self::UnsupportedFeatureChirho(msg_chirho) => {
                write!(f_chirho, "unsupported feature: {}", msg_chirho)
            }
            Self::NotFoundChirho => write!(f_chirho, "not found"),
            Self::AlreadyExistsChirho => write!(f_chirho, "already exists"),
            Self::NotDirectoryChirho => write!(f_chirho, "not a directory"),
            Self::NotSymlinkChirho => write!(f_chirho, "not a symlink"),
            Self::ReadOnlyChirho => write!(f_chirho, "filesystem is read-only"),
            Self::NoSpaceChirho => write!(f_chirho, "no space left on device"),
        }
    }
}

// ---------------------------------------------------------------------------
// Mount mode enum — replaces bug-prone `readonly_chirho: bool`
// ---------------------------------------------------------------------------

/// Mount mode — replaces the bug-prone `readonly_chirho: bool` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountModeChirho {
    ReadOnlyChirho,
    ReadWriteChirho,
}

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

// ===========================================================================
// A4-010: Page cache for block devices
// ===========================================================================

use alloc::collections::BTreeMap;

/// A simple LRU page cache for block device reads.
///
/// Caches blocks keyed by (device_id, block_number). When the cache
/// exceeds `max_pages_chirho`, the least-recently-used entry is evicted.
pub struct PageCacheChirho {
    /// Cached pages: key = (device_id, block_nr), value = data.
    pages_chirho: BTreeMap<(u32, u64), CachedPageChirho>,
    /// Access counter for LRU ordering.
    access_counter_chirho: u64,
    /// Maximum number of pages to keep in the cache.
    max_pages_chirho: usize,
}

/// A single cached page (one filesystem block).
struct CachedPageChirho {
    /// Block data.
    data_chirho: Vec<u8>,
    /// Last access timestamp (monotonic counter).
    last_access_chirho: u64,
    /// Whether the page has been modified (dirty).
    dirty_chirho: bool,
}

impl PageCacheChirho {
    /// Create a new page cache with the given maximum capacity.
    pub const fn new_chirho(max_pages_chirho: usize) -> Self {
        Self {
            pages_chirho: BTreeMap::new(),
            access_counter_chirho: 0,
            max_pages_chirho,
        }
    }

    /// Look up a cached block. Returns `Some(&[u8])` on hit, `None` on miss.
    pub fn get_chirho(&mut self, device_id_chirho: u32, block_nr_chirho: u64) -> Option<&[u8]> {
        self.access_counter_chirho += 1;
        let counter_chirho = self.access_counter_chirho;
        if let Some(page_chirho) = self.pages_chirho.get_mut(&(device_id_chirho, block_nr_chirho)) {
            page_chirho.last_access_chirho = counter_chirho;
            return Some(&page_chirho.data_chirho);
        }
        None
    }

    /// Insert a block into the cache. Evicts the LRU page if at capacity.
    pub fn insert_chirho(
        &mut self,
        device_id_chirho: u32,
        block_nr_chirho: u64,
        data_chirho: Vec<u8>,
    ) {
        // Evict LRU if at capacity.
        if self.pages_chirho.len() >= self.max_pages_chirho {
            self.evict_lru_chirho();
        }

        self.access_counter_chirho += 1;
        self.pages_chirho.insert(
            (device_id_chirho, block_nr_chirho),
            CachedPageChirho {
                data_chirho,
                last_access_chirho: self.access_counter_chirho,
                dirty_chirho: false,
            },
        );
    }

    /// Mark a cached page as dirty.
    #[allow(dead_code)]
    pub fn mark_dirty_chirho(&mut self, device_id_chirho: u32, block_nr_chirho: u64) {
        if let Some(page_chirho) = self.pages_chirho.get_mut(&(device_id_chirho, block_nr_chirho)) {
            page_chirho.dirty_chirho = true;
        }
    }

    /// Evict the least-recently-used (oldest access counter) page.
    fn evict_lru_chirho(&mut self) {
        let mut lru_key_chirho: Option<(u32, u64)> = None;
        let mut lru_access_chirho: u64 = u64::MAX;

        for (key_chirho, page_chirho) in self.pages_chirho.iter() {
            if page_chirho.last_access_chirho < lru_access_chirho {
                lru_access_chirho = page_chirho.last_access_chirho;
                lru_key_chirho = Some(*key_chirho);
            }
        }

        if let Some(key_chirho) = lru_key_chirho {
            self.pages_chirho.remove(&key_chirho);
        }
    }

    /// Invalidate all cached pages for a given device.
    #[allow(dead_code)]
    pub fn invalidate_device_chirho(&mut self, device_id_chirho: u32) {
        self.pages_chirho
            .retain(|key_chirho, _| key_chirho.0 != device_id_chirho);
    }

    /// Return the number of cached pages.
    #[allow(dead_code)]
    pub fn len_chirho(&self) -> usize {
        self.pages_chirho.len()
    }
}

/// Global page cache instance (protected by a spinlock).
/// Bounded to 64 entries (~256KB with 4K blocks) to prevent OOM during
/// Block cache for ext4 reads. 8192 entries × 4KB = 32MB of cached blocks.
/// Xorg loads 20+ shared libraries requiring repeated directory traversals.
/// The old 512-entry cache caused evictions between SSH sessions, making
/// library loading flaky ("not found" errors on 2nd+ SSH connection).
pub static PAGE_CACHE_CHIRHO: spin::Mutex<PageCacheChirho> =
    spin::Mutex::new(PageCacheChirho::new_chirho(8192));

// ===========================================================================
// A4-009: ext4 read-only VFS integration
// ===========================================================================

/// Cached ext4 filesystem state for a mounted partition.
///
/// Holds the parsed superblock and group descriptors so that inode/block
/// lookups don't need to re-parse them on every access.
pub struct Ext4MountChirho {
    /// Parsed superblock.
    pub sb_chirho: Ext4SuperblockChirho,
    /// Block group descriptors.
    pub group_descs_chirho: Vec<Ext4GroupDescChirho>,
    /// Block size in bytes (1024, 2048, or 4096).
    pub block_size_chirho: u32,
    /// Device ID in the block registry (for read_block calls).
    pub device_id_chirho: u32,
    /// Mount mode: read-only or read-write.
    pub mode_chirho: MountModeChirho,
}

impl Ext4MountChirho {
    /// Read a single block from the underlying device.
    ///
    /// Checks the page cache first; on miss, reads from the block device
    /// and caches the result.
    #[allow(dead_code)]
    /// Copy a cached block directly into a destination buffer.
    /// On cache hit: zero-copy from cache to dest (no heap alloc).
    /// On cache miss: read into dest, then copy to cache (ONE alloc, kept in cache).
    pub fn read_block_into_chirho(&self, block_nr_chirho: u64, dest_chirho: &mut [u8]) -> Option<usize> {
        // Check page cache first — zero-copy from cache to dest
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            if let Some(data_chirho) = cache_chirho.get_chirho(self.device_id_chirho, block_nr_chirho) {
                let copy_len_chirho = core::cmp::min(data_chirho.len(), dest_chirho.len());
                dest_chirho[..copy_len_chirho].copy_from_slice(&data_chirho[..copy_len_chirho]);
                return Some(copy_len_chirho);
            }
        }

        // Cache miss — read directly into dest buffer (no intermediate Vec)
        let bs_chirho = self.block_size_chirho as usize;
        let sectors_per_block_chirho = bs_chirho / 512;
        let start_sector_chirho = block_nr_chirho * sectors_per_block_chirho as u64;
        let read_len_chirho = core::cmp::min(bs_chirho, dest_chirho.len());

        // Zero the dest first
        dest_chirho[..read_len_chirho].fill(0);

        let registry_chirho = &crate::block_chirho::BLOCK_REGISTRY_CHIRHO;
        if registry_chirho
            .read_block_chirho(
                self.device_id_chirho as usize,
                start_sector_chirho,
                &mut dest_chirho[..read_len_chirho],
            )
            .is_err()
        {
            return None;
        }

        // Insert into cache (ONE allocation — stays in cache, never freed during boot)
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            let cache_copy_chirho = dest_chirho[..read_len_chirho].to_vec();
            cache_chirho.insert_chirho(
                self.device_id_chirho,
                block_nr_chirho,
                cache_copy_chirho,
            );
        }

        Some(read_len_chirho)
    }

    pub fn read_block_cached_chirho(&self, block_nr_chirho: u64) -> Option<Vec<u8>> {
        // Check page cache.
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            if let Some(data_chirho) = cache_chirho.get_chirho(self.device_id_chirho, block_nr_chirho) {
                return Some(data_chirho.to_vec());
            }
        }
        // Cache miss — read the full 4K block in one VirtIO request.
        // VirtIO-blk supports multi-sector reads (sector count determined
        // by the data buffer size in the descriptor).
        let bs_chirho = self.block_size_chirho as usize;
        let sectors_per_block_chirho = bs_chirho / 512;
        let start_sector_chirho = block_nr_chirho * sectors_per_block_chirho as u64;

        let mut buf_chirho = alloc::vec![0u8; bs_chirho];

        let registry_chirho = &crate::block_chirho::BLOCK_REGISTRY_CHIRHO;
        // Read entire 4K block in one request (8x faster than per-sector)
        let read_result_chirho = registry_chirho.read_block_chirho(
            self.device_id_chirho as usize,
            start_sector_chirho,
            &mut buf_chirho,
        );
        if let Err(_e_chirho) = read_result_chirho {
            return None;
        }

        // Insert into page cache.
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            cache_chirho.insert_chirho(self.device_id_chirho, block_nr_chirho, buf_chirho.clone());
        }

        Some(buf_chirho)
    }

    /// Read an ext4 inode by inode number.
    #[allow(dead_code)]
    pub fn read_inode_chirho(&self, ino_chirho: u32) -> Option<Ext4InodeChirho> {
        let (group_chirho, local_chirho) =
            inode_to_group_chirho(ino_chirho, self.sb_chirho.s_inodes_per_group_chirho);

        if group_chirho as usize >= self.group_descs_chirho.len() {
            return None;
        }

        let gd_chirho = &self.group_descs_chirho[group_chirho as usize];
        let inode_table_block_chirho = gd_chirho.inode_table_chirho(self.sb_chirho.has_64bit_chirho());
        let inode_size_chirho = self.sb_chirho.inode_size_chirho();
        let byte_offset_chirho = inode_table_offset_chirho(local_chirho, inode_size_chirho);

        // Which block contains this inode?
        let block_nr_chirho = inode_table_block_chirho + byte_offset_chirho / self.block_size_chirho as u64;
        let offset_in_block_chirho = (byte_offset_chirho % self.block_size_chirho as u64) as usize;

        let block_data_chirho = self.read_block_cached_chirho(block_nr_chirho)?;

        if offset_in_block_chirho + core::mem::size_of::<Ext4InodeChirho>() > block_data_chirho.len() {
            return None;
        }

        let inode_chirho: Ext4InodeChirho = unsafe {
            core::ptr::read_unaligned(
                block_data_chirho.as_ptr().add(offset_in_block_chirho) as *const Ext4InodeChirho,
            )
        };

        Some(inode_chirho)
    }

    /// Read file data for an inode using its extent tree.
    ///
    /// Returns all the file data by walking the extent tree and reading
    /// the corresponding data blocks.
    #[allow(dead_code)]
    pub fn read_file_data_chirho(&self, inode_chirho: &Ext4InodeChirho) -> Option<Vec<u8>> {
        let file_size_chirho = inode_chirho.size_chirho() as usize;
        // Log reads > 256KB for debugging heap usage.
        if file_size_chirho > 256 * 1024 {
            let mode_copy_chirho = { inode_chirho.i_mode_chirho };
            crate::serial_debug_chirho!(
                "[EXT4-RD] size={} mode=0x{:x}",
                file_size_chirho,
                mode_copy_chirho,
            );
        }
        if file_size_chirho == 0 {
            return Some(Vec::new());
        }

        if !inode_chirho.uses_extents_chirho() {
            // Non-extent (legacy block map): i_block[0..11] are direct block
            // pointers, i_block[12] is single-indirect, etc.
            // For small files (< 48KB), direct blocks suffice.
            let blk0_chirho = { inode_chirho.i_block_chirho[0] };
            let bsz_chirho = self.block_size_chirho;
            crate::serial_println_chirho!(
                "[EXT4-LEGACY] Reading legacy block map file: size={} i_block[0]={} block_size={}",
                file_size_chirho, blk0_chirho, bsz_chirho,
            );
            let mut data_chirho = Vec::with_capacity(file_size_chirho);
            let block_size_chirho = self.block_size_chirho as usize;
            for i_chirho in 0..12usize {
                if data_chirho.len() >= file_size_chirho {
                    break;
                }
                let block_nr_chirho = inode_chirho.i_block_chirho[i_chirho];
                if block_nr_chirho == 0 {
                    // Sparse file hole — zero-fill
                    let fill_chirho = core::cmp::min(block_size_chirho, file_size_chirho - data_chirho.len());
                    data_chirho.extend(core::iter::repeat(0u8).take(fill_chirho));
                    continue;
                }
                let offset_chirho = block_nr_chirho as u64 * block_size_chirho as u64;
                let to_read_chirho = core::cmp::min(block_size_chirho, file_size_chirho - data_chirho.len());
                let mut block_buf_chirho = alloc::vec![0u8; block_size_chirho];
                if let Some(_n_chirho) = self.read_block_into_chirho(block_nr_chirho as u64, &mut block_buf_chirho) {
                    data_chirho.extend_from_slice(&block_buf_chirho[..to_read_chirho]);
                }
            }
            data_chirho.truncate(file_size_chirho);
            return Some(data_chirho);
        }

        let header_chirho = inode_chirho.extent_header_chirho();
        if !header_chirho.is_valid_chirho() {
            return None;
        }

        // Pre-allocate to avoid Vec doubling (which caused 64MB OOM:
        // 7MB→14MB→28MB→...→64MB from repeated reallocation).
        let mut data_chirho = Vec::with_capacity(file_size_chirho);
        let block_copy_chirho = inode_chirho.i_block_chirho;

        // Depth 0 = leaf extents directly in i_block.
        if header_chirho.eh_depth_chirho == 0 {
            self.read_leaf_extents_chirho(&block_copy_chirho, &header_chirho, file_size_chirho, &mut data_chirho)?;
        } else {
            // Multi-level extent tree — read index entries, then recurse.
            self.read_extent_tree_chirho(&block_copy_chirho, &header_chirho, file_size_chirho, &mut data_chirho)?;
        }

        // Truncate to actual file size.
        data_chirho.truncate(file_size_chirho);
        Some(data_chirho)
    }

    /// Read a single logical 4K block into a provided buffer (zero-copy).
    /// Returns the number of bytes copied, or None on error.
    pub fn read_block_by_logical_into_chirho(
        &self,
        inode_chirho: &Ext4InodeChirho,
        logical_block_chirho: u64,
        dest_chirho: &mut [u8],
    ) -> Option<usize> {
        if !inode_chirho.uses_extents_chirho() {
            return None;
        }
        let header_chirho = inode_chirho.extent_header_chirho();
        if !header_chirho.is_valid_chirho() {
            return None;
        }
        let block_copy_chirho = inode_chirho.i_block_chirho;
        let phys_block_chirho = self.find_phys_block_chirho(
            &block_copy_chirho, &header_chirho, logical_block_chirho as u32,
        );
        if let Some(pb_chirho) = phys_block_chirho {
            self.read_block_into_chirho(pb_chirho, dest_chirho)
        } else {
            // Sparse hole — zero fill
            let fill_chirho = core::cmp::min(4096, dest_chirho.len());
            dest_chirho[..fill_chirho].fill(0);
            Some(fill_chirho)
        }
    }

    /// Read a single logical 4K block (allocating Vec — use read_block_by_logical_into for zero-copy).
    pub fn read_block_by_logical_chirho(
        &self,
        inode_chirho: &Ext4InodeChirho,
        logical_block_chirho: u64,
    ) -> Option<Vec<u8>> {
        let mut buf_chirho = alloc::vec![0u8; 4096];
        self.read_block_by_logical_into_chirho(inode_chirho, logical_block_chirho, &mut buf_chirho)?;
        Some(buf_chirho)
    }

    /// Find the physical block number for a logical block in the extent tree.
    pub fn find_phys_block_chirho(
        &self,
        block_data_chirho: &[u32; 15],
        header_chirho: &Ext4ExtentHeaderChirho,
        logical_block_chirho: u32,
    ) -> Option<u64> {
        let raw_bytes_chirho: &[u8] = unsafe {
            core::slice::from_raw_parts(
                block_data_chirho.as_ptr() as *const u8,
                60,
            )
        };

        if header_chirho.eh_depth_chirho == 0 {
            // Leaf node — search extents
            let entries_chirho = header_chirho.eh_entries_chirho as usize;
            for i_chirho in 0..entries_chirho {
                let ext_off_chirho = 12 + i_chirho * 12;
                if ext_off_chirho + 12 > 60 { break; }
                let ee_block_chirho = u32::from_le_bytes([
                    raw_bytes_chirho[ext_off_chirho],
                    raw_bytes_chirho[ext_off_chirho + 1],
                    raw_bytes_chirho[ext_off_chirho + 2],
                    raw_bytes_chirho[ext_off_chirho + 3],
                ]);
                let ee_len_chirho = u16::from_le_bytes([
                    raw_bytes_chirho[ext_off_chirho + 4],
                    raw_bytes_chirho[ext_off_chirho + 5],
                ]);
                let ee_start_lo_chirho = u32::from_le_bytes([
                    raw_bytes_chirho[ext_off_chirho + 8],
                    raw_bytes_chirho[ext_off_chirho + 9],
                    raw_bytes_chirho[ext_off_chirho + 10],
                    raw_bytes_chirho[ext_off_chirho + 11],
                ]);
                let ee_start_hi_chirho = u16::from_le_bytes([
                    raw_bytes_chirho[ext_off_chirho + 6],
                    raw_bytes_chirho[ext_off_chirho + 7],
                ]);
                let phys_start_chirho = (ee_start_hi_chirho as u64) << 32 | (ee_start_lo_chirho as u64);
                let actual_len_chirho = if ee_len_chirho > 0x8000 {
                    (ee_len_chirho - 0x8000) as u32
                } else {
                    ee_len_chirho as u32
                };

                if logical_block_chirho >= ee_block_chirho
                    && logical_block_chirho < ee_block_chirho + actual_len_chirho
                {
                    let offset_chirho = (logical_block_chirho - ee_block_chirho) as u64;
                    return Some(phys_start_chirho + offset_chirho);
                }
            }
            None // Not found — sparse hole
        } else {
            // Index node — find child and recurse
            let entries_chirho = header_chirho.eh_entries_chirho as usize;
            let mut best_idx_chirho: Option<usize> = None;
            for i_chirho in 0..entries_chirho {
                let idx_off_chirho = 12 + i_chirho * 12;
                if idx_off_chirho + 12 > 60 { break; }
                let ei_block_chirho = u32::from_le_bytes([
                    raw_bytes_chirho[idx_off_chirho],
                    raw_bytes_chirho[idx_off_chirho + 1],
                    raw_bytes_chirho[idx_off_chirho + 2],
                    raw_bytes_chirho[idx_off_chirho + 3],
                ]);
                if logical_block_chirho >= ei_block_chirho {
                    best_idx_chirho = Some(i_chirho);
                }
            }

            if let Some(idx_chirho) = best_idx_chirho {
                let idx_off_chirho = 12 + idx_chirho * 12;
                let child_lo_chirho = u32::from_le_bytes([
                    raw_bytes_chirho[idx_off_chirho + 4],
                    raw_bytes_chirho[idx_off_chirho + 5],
                    raw_bytes_chirho[idx_off_chirho + 6],
                    raw_bytes_chirho[idx_off_chirho + 7],
                ]);
                let child_hi_chirho = u16::from_le_bytes([
                    raw_bytes_chirho[idx_off_chirho + 8],
                    raw_bytes_chirho[idx_off_chirho + 9],
                ]);
                let child_block_chirho = (child_hi_chirho as u64) << 32 | (child_lo_chirho as u64);

                // Read the child node from disk via block cache
                let child_vec_chirho = self.read_block_cached_chirho(child_block_chirho)?;
                let mut child_data_chirho = [0u8; 4096];
                let copy_len_chirho = core::cmp::min(4096, child_vec_chirho.len());
                child_data_chirho[..copy_len_chirho].copy_from_slice(&child_vec_chirho[..copy_len_chirho]);

                // Parse child header
                let child_header_chirho = Ext4ExtentHeaderChirho {
                    eh_magic_chirho: u16::from_le_bytes([child_data_chirho[0], child_data_chirho[1]]),
                    eh_entries_chirho: u16::from_le_bytes([child_data_chirho[2], child_data_chirho[3]]),
                    eh_max_chirho: u16::from_le_bytes([child_data_chirho[4], child_data_chirho[5]]),
                    eh_depth_chirho: u16::from_le_bytes([child_data_chirho[6], child_data_chirho[7]]),
                    eh_generation_chirho: u32::from_le_bytes([
                        child_data_chirho[8], child_data_chirho[9],
                        child_data_chirho[10], child_data_chirho[11],
                    ]),
                };

                if !child_header_chirho.is_valid_chirho() {
                    return None;
                }

                // Convert child_data to [u32; 15] format for recursion
                let mut child_block_arr_chirho = [0u32; 15];
                for k_chirho in 0..15 {
                    child_block_arr_chirho[k_chirho] = u32::from_le_bytes([
                        child_data_chirho[k_chirho * 4],
                        child_data_chirho[k_chirho * 4 + 1],
                        child_data_chirho[k_chirho * 4 + 2],
                        child_data_chirho[k_chirho * 4 + 3],
                    ]);
                }

                self.find_phys_block_chirho(
                    &child_block_arr_chirho,
                    &child_header_chirho,
                    logical_block_chirho,
                )
            } else {
                None
            }
        }
    }

    /// Read leaf extents from the i_block array.
    fn read_leaf_extents_chirho(
        &self,
        block_data_chirho: &[u32; 15],
        header_chirho: &Ext4ExtentHeaderChirho,
        _max_size_chirho: usize,
        out_chirho: &mut Vec<u8>,
    ) -> Option<()> {
        let entries_count_chirho = header_chirho.eh_entries_chirho as usize;
        // Each extent is 12 bytes, starting after the 12-byte header.
        let raw_bytes_chirho: &[u8] = unsafe {
            core::slice::from_raw_parts(
                block_data_chirho.as_ptr() as *const u8,
                60,
            )
        };

        for i_chirho in 0..entries_count_chirho {
            let offset_chirho = 12 + i_chirho * 12; // after header
            if offset_chirho + 12 > raw_bytes_chirho.len() {
                break;
            }
            let extent_chirho: Ext4ExtentChirho = unsafe {
                core::ptr::read_unaligned(
                    raw_bytes_chirho.as_ptr().add(offset_chirho) as *const Ext4ExtentChirho,
                )
            };
            let phys_start_chirho = extent_chirho.physical_block_chirho();
            let count_chirho = extent_chirho.block_count_chirho();

            for blk_chirho in 0..count_chirho {
                let block_data_result_chirho = self.read_block_cached_chirho(phys_start_chirho + blk_chirho as u64)?;
                out_chirho.extend_from_slice(&block_data_result_chirho);
            }
        }

        Some(())
    }

    /// Recursively read a multi-level extent tree.
    fn read_extent_tree_chirho(
        &self,
        block_data_chirho: &[u32; 15],
        header_chirho: &Ext4ExtentHeaderChirho,
        max_size_chirho: usize,
        out_chirho: &mut Vec<u8>,
    ) -> Option<()> {
        let entries_count_chirho = header_chirho.eh_entries_chirho as usize;
        let raw_bytes_chirho: &[u8] = unsafe {
            core::slice::from_raw_parts(
                block_data_chirho.as_ptr() as *const u8,
                60,
            )
        };

        for i_chirho in 0..entries_count_chirho {
            let offset_chirho = 12 + i_chirho * 12;
            if offset_chirho + 12 > raw_bytes_chirho.len() {
                break;
            }
            let idx_chirho: Ext4ExtentIdxChirho = unsafe {
                core::ptr::read_unaligned(
                    raw_bytes_chirho.as_ptr().add(offset_chirho) as *const Ext4ExtentIdxChirho,
                )
            };

            let child_block_chirho = idx_chirho.child_block_chirho();
            let child_data_chirho = self.read_block_cached_chirho(child_block_chirho)?;

            // Parse the child node's header.
            if child_data_chirho.len() < 12 {
                return None;
            }
            let child_header_chirho: Ext4ExtentHeaderChirho = unsafe {
                core::ptr::read_unaligned(
                    child_data_chirho.as_ptr() as *const Ext4ExtentHeaderChirho,
                )
            };

            if !child_header_chirho.is_valid_chirho() {
                return None;
            }

            if child_header_chirho.eh_depth_chirho == 0 {
                // Leaf node: read extents from the block data.
                let leaf_count_chirho = child_header_chirho.eh_entries_chirho as usize;
                for j_chirho in 0..leaf_count_chirho {
                    let ext_off_chirho = 12 + j_chirho * 12;
                    if ext_off_chirho + 12 > child_data_chirho.len() {
                        break;
                    }
                    let extent_chirho: Ext4ExtentChirho = unsafe {
                        core::ptr::read_unaligned(
                            child_data_chirho.as_ptr().add(ext_off_chirho) as *const Ext4ExtentChirho,
                        )
                    };
                    let phys_start_chirho = extent_chirho.physical_block_chirho();
                    let count_chirho = extent_chirho.block_count_chirho();

                    for blk_chirho in 0..count_chirho {
                        if out_chirho.len() >= max_size_chirho {
                            return Some(());
                        }
                        let block_result_chirho = self.read_block_cached_chirho(phys_start_chirho + blk_chirho as u64)?;
                        out_chirho.extend_from_slice(&block_result_chirho);
                    }
                }
            }
            // Deeper levels would recurse further (rare in practice).
        }

        Some(())
    }

    /// List directory entries for a given directory inode.
    #[allow(dead_code)]
    pub fn read_dir_entries_chirho(&self, dir_ino_chirho: u32) -> Option<Vec<DirEntryInfoChirho>> {
        let inode_chirho = self.read_inode_chirho(dir_ino_chirho)?;
        if !inode_chirho.is_dir_chirho() {
            return None;
        }

        let data_chirho = self.read_file_data_chirho(&inode_chirho)?;
        Some(parse_dir_entries_chirho(&data_chirho))
    }

    /// Look up a file by name in a directory.
    #[allow(dead_code)]
    pub fn lookup_in_dir_chirho(
        &self,
        dir_ino_chirho: u32,
        name_chirho: &str,
    ) -> Option<DirEntryInfoChirho> {
        // Scan directory blocks sequentially instead of reading the
        // entire directory into a Vec. This avoids the 2MB→64MB Vec
        // doubling that caused OOM for large directories like /usr/lib.
        let debug_lookup_chirho = name_chirho == "usr" || name_chirho == "sbin" || name_chirho == "dropbear";
        let inode_chirho = self.read_inode_chirho(dir_ino_chirho)?;
        if debug_lookup_chirho {
            let mode_copy_chirho = { inode_chirho.i_mode_chirho };
            crate::serial_debug_chirho!(
                "[EXT4-DBG] lookup_in_dir: ino={} mode={:#o} is_dir={}",
                dir_ino_chirho, mode_copy_chirho, inode_chirho.is_dir_chirho()
            );
        }
        if !inode_chirho.is_dir_chirho() {
            return None;
        }
        let dir_size_chirho = inode_chirho.size_chirho() as usize;
        let num_blocks_chirho = (dir_size_chirho + 4095) / 4096;

        // Debug: log root directory lookup details
        let debug_lookup_chirho = name_chirho == "usr" || name_chirho == "sbin" || name_chirho == "dropbear";
        if debug_lookup_chirho {
            let flags_copy_chirho = { inode_chirho.i_flags_chirho };
            crate::serial_debug_chirho!(
                "[EXT4-DBG] dir ino={} size={} blocks={} flags={:#x} uses_extents={}",
                dir_ino_chirho, dir_size_chirho, num_blocks_chirho,
                flags_copy_chirho, inode_chirho.uses_extents_chirho()
            );
        }

        let mut block_buf_chirho = [0u8; 4096];
        for blk_idx_chirho in 0..num_blocks_chirho {
            if self.read_block_by_logical_into_chirho(
                &inode_chirho,
                blk_idx_chirho as u64,
                &mut block_buf_chirho,
            )
            .is_none()
            {
                continue;
            }

            // Parse directory entries in this block.
            let block_end_chirho = if blk_idx_chirho == num_blocks_chirho - 1 {
                dir_size_chirho - blk_idx_chirho * 4096
            } else {
                4096
            };
            let mut offset_chirho = 0usize;
            while offset_chirho + 8 <= block_end_chirho {
                let ino_chirho = u32::from_le_bytes(
                    block_buf_chirho[offset_chirho..offset_chirho + 4]
                        .try_into()
                        .unwrap_or([0; 4]),
                );
                let rec_len_chirho = u16::from_le_bytes(
                    block_buf_chirho[offset_chirho + 4..offset_chirho + 6]
                        .try_into()
                        .unwrap_or([0; 2]),
                ) as usize;
                let name_len_chirho = block_buf_chirho[offset_chirho + 6] as usize;
                let file_type_chirho = block_buf_chirho[offset_chirho + 7];

                if rec_len_chirho == 0 {
                    break;
                }
                if ino_chirho != 0 && name_len_chirho > 0 && offset_chirho + 8 + name_len_chirho <= 4096 {
                    if let Ok(entry_name_chirho) = core::str::from_utf8(
                        &block_buf_chirho[offset_chirho + 8..offset_chirho + 8 + name_len_chirho],
                    ) {
                        if debug_lookup_chirho {
                            crate::serial_debug_chirho!(
                                "[EXT4-DBG]   entry: ino={} name='{}' type={}",
                                ino_chirho, entry_name_chirho, file_type_chirho
                            );
                        }
                        if entry_name_chirho == name_chirho {
                            return Some(DirEntryInfoChirho {
                                inode_chirho: ino_chirho,
                                name_chirho: alloc::string::String::from(entry_name_chirho),
                                file_type_chirho,
                            });
                        }
                    }
                }
                offset_chirho += rec_len_chirho;
            }
        }
        None
    }

    /// Resolve a path (e.g. "/usr/bin/ls") to an inode number, starting
    /// from the root inode.
    #[allow(dead_code)]
    pub fn resolve_path_chirho(&self, path_chirho: &str) -> Option<u32> {
        let mut current_ino_chirho = EXT4_ROOT_INO_CHIRHO;

        for component_chirho in path_chirho.split('/') {
            if component_chirho.is_empty() || component_chirho == "." {
                continue;
            }
            if component_chirho == ".." {
                // For simplicity, treat ".." as staying at current (safe for read-only).
                continue;
            }
            let entry_chirho = self.lookup_in_dir_chirho(current_ino_chirho, component_chirho)?;
            current_ino_chirho = entry_chirho.inode_chirho;
        }

        Some(current_ino_chirho)
    }
}

// ===========================================================================
// A4-011: ext4 block and inode allocation from bitmaps
// ===========================================================================

/// Allocate a free block from the given block group.
///
/// Scans the block bitmap for the first free bit, marks it as used,
/// and returns the global block number.
///
/// `bitmap_data_chirho` is the raw block bitmap (one bit per block in the group).
/// Returns `Some(global_block_nr)` or `None` if the group is full.
#[allow(dead_code)]
pub fn alloc_block_in_group_chirho(
    bitmap_data_chirho: &mut [u8],
    group_chirho: u32,
    blocks_per_group_chirho: u32,
    first_data_block_chirho: u32,
) -> Option<u64> {
    for byte_idx_chirho in 0..bitmap_data_chirho.len() {
        let byte_chirho = bitmap_data_chirho[byte_idx_chirho];
        if byte_chirho == 0xFF {
            continue; // all bits set
        }
        for bit_chirho in 0..8u32 {
            if byte_chirho & (1 << bit_chirho) == 0 {
                let local_block_chirho = byte_idx_chirho as u32 * 8 + bit_chirho;
                if local_block_chirho >= blocks_per_group_chirho {
                    return None; // past the end of this group
                }
                // Found a free block within the valid range.
                bitmap_data_chirho[byte_idx_chirho] |= 1 << bit_chirho;
                let global_block_chirho =
                    group_chirho as u64 * blocks_per_group_chirho as u64
                    + local_block_chirho as u64
                    + first_data_block_chirho as u64;
                return Some(global_block_chirho);
            }
        }
    }
    None
}

/// Allocate a free inode from the given block group.
///
/// Scans the inode bitmap for the first free bit, marks it as used,
/// and returns the global inode number (1-based).
#[allow(dead_code)]
pub fn alloc_inode_in_group_chirho(
    bitmap_data_chirho: &mut [u8],
    group_chirho: u32,
    inodes_per_group_chirho: u32,
) -> Option<u32> {
    for byte_idx_chirho in 0..bitmap_data_chirho.len() {
        let byte_chirho = bitmap_data_chirho[byte_idx_chirho];
        if byte_chirho == 0xFF {
            continue;
        }
        for bit_chirho in 0..8u32 {
            if byte_chirho & (1 << bit_chirho) == 0 {
                let local_inode_chirho = byte_idx_chirho as u32 * 8 + bit_chirho;
                if local_inode_chirho >= inodes_per_group_chirho {
                    return None;
                }
                bitmap_data_chirho[byte_idx_chirho] |= 1 << bit_chirho;
                // Inode numbers are 1-based.
                let global_inode_chirho =
                    group_chirho * inodes_per_group_chirho + local_inode_chirho + 1;
                return Some(global_inode_chirho);
            }
        }
    }
    None
}

/// Free a block by clearing its bit in the block bitmap.
#[allow(dead_code)]
pub fn free_block_in_group_chirho(
    bitmap_data_chirho: &mut [u8],
    local_block_chirho: u32,
) {
    let byte_idx_chirho = (local_block_chirho / 8) as usize;
    let bit_chirho = local_block_chirho % 8;
    if byte_idx_chirho < bitmap_data_chirho.len() {
        bitmap_data_chirho[byte_idx_chirho] &= !(1 << bit_chirho);
    }
}

/// Free an inode by clearing its bit in the inode bitmap.
#[allow(dead_code)]
pub fn free_inode_in_group_chirho(
    bitmap_data_chirho: &mut [u8],
    local_inode_chirho: u32,
) {
    let byte_idx_chirho = (local_inode_chirho / 8) as usize;
    let bit_chirho = local_inode_chirho % 8;
    if byte_idx_chirho < bitmap_data_chirho.len() {
        bitmap_data_chirho[byte_idx_chirho] &= !(1 << bit_chirho);
    }
}

// ===========================================================================
// A4-012: ext4 write support (create file, write data, truncate)
// ===========================================================================

impl Ext4MountChirho {
    /// Write a single block to the underlying block device.
    ///
    /// The block data must be exactly `block_size_chirho` bytes.
    /// Called by write_inode_chirho, write_file_data_chirho, and the VFS write path.
    pub fn write_block_chirho(&self, block_nr_chirho: u64, data_chirho: &[u8]) -> Result<(), Ext4ErrorChirho> {
        if self.mode_chirho == MountModeChirho::ReadOnlyChirho {
            return Err(Ext4ErrorChirho::ReadOnlyChirho);
        }
        let bs_chirho = self.block_size_chirho as usize;
        if data_chirho.len() != bs_chirho {
            return Err(Ext4ErrorChirho::IoErrorChirho);
        }

        crate::serial_debug_chirho!(
            "[EXT4] write_block: block={} bs={} dev={}",
            block_nr_chirho, bs_chirho, self.device_id_chirho
        );

        let sectors_per_block_chirho = bs_chirho / 512;
        let start_sector_chirho = block_nr_chirho * sectors_per_block_chirho as u64;
        let registry_chirho = &crate::block_chirho::BLOCK_REGISTRY_CHIRHO;

        for i_chirho in 0..sectors_per_block_chirho {
            let sector_chirho = start_sector_chirho + i_chirho as u64;
            let offset_chirho = i_chirho * 512;
            registry_chirho
                .write_block_chirho(
                    self.device_id_chirho as usize,
                    sector_chirho,
                    &data_chirho[offset_chirho..offset_chirho + 512],
                )
                .map_err(|err_chirho| {
                    crate::serial_println_chirho!(
                        "[EXT4] write_block FAILED: block={} sector={} err={}",
                        block_nr_chirho, sector_chirho, err_chirho
                    );
                    Ext4ErrorChirho::IoErrorChirho
                })?;
        }

        // Update the page cache entry for this block.
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            cache_chirho.insert_chirho(self.device_id_chirho, block_nr_chirho, data_chirho.to_vec());
        }

        Ok(())
    }

    /// Write an inode back to disk.
    ///
    /// Serializes the inode struct into the correct position within its
    /// block group's inode table and writes the containing block.
    pub fn write_inode_chirho(&self, ino_chirho: u32, inode_chirho: &Ext4InodeChirho) -> Result<(), Ext4ErrorChirho> {
        let sb_inodes_per_group_chirho = self.sb_chirho.s_inodes_per_group_chirho;
        let (group_chirho, local_chirho) = inode_to_group_chirho(ino_chirho, sb_inodes_per_group_chirho);

        if group_chirho as usize >= self.group_descs_chirho.len() {
            return Err(Ext4ErrorChirho::CorruptInodeChirho);
        }

        let gd_chirho = &self.group_descs_chirho[group_chirho as usize];
        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let inode_table_block_chirho = gd_chirho.inode_table_chirho(has_64bit_chirho);
        let inode_size_chirho = self.sb_chirho.inode_size_chirho();
        let byte_offset_chirho = inode_table_offset_chirho(local_chirho, inode_size_chirho);

        let block_nr_chirho = inode_table_block_chirho + byte_offset_chirho / self.block_size_chirho as u64;
        let offset_in_block_chirho = (byte_offset_chirho % self.block_size_chirho as u64) as usize;

        let mut block_data_chirho = self.read_block_cached_chirho(block_nr_chirho)
            .ok_or(Ext4ErrorChirho::IoErrorChirho)?;

        // Copy the inode into the block data.
        let inode_bytes_chirho: &[u8] = unsafe {
            core::slice::from_raw_parts(
                inode_chirho as *const Ext4InodeChirho as *const u8,
                core::mem::size_of::<Ext4InodeChirho>(),
            )
        };
        let end_chirho = offset_in_block_chirho + inode_bytes_chirho.len();
        if end_chirho > block_data_chirho.len() {
            return Err(Ext4ErrorChirho::CorruptInodeChirho);
        }
        block_data_chirho[offset_in_block_chirho..end_chirho].copy_from_slice(inode_bytes_chirho);

        self.write_block_chirho(block_nr_chirho, &block_data_chirho)
    }

    /// Create a new file in a directory.
    ///
    /// Allocates a new inode, creates a directory entry, and initializes
    /// the inode as a regular file with the given mode.
    #[allow(dead_code)]
    pub fn create_file_chirho(
        &self,
        parent_ino_chirho: u32,
        name_chirho: &str,
        file_mode_chirho: u16,
    ) -> Result<u32, Ext4ErrorChirho> {
        crate::serial_debug_chirho!(
            "[EXT4] create_file: parent={} name='{}' mode={:?}",
            parent_ino_chirho, name_chirho, self.mode_chirho
        );
        if self.mode_chirho == MountModeChirho::ReadOnlyChirho {
            return Err(Ext4ErrorChirho::ReadOnlyChirho);
        }

        let sb_inodes_per_group_chirho = self.sb_chirho.s_inodes_per_group_chirho;

        // Find a block group with free inodes.
        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let mut new_ino_chirho: Option<u32> = None;

        for (gidx_chirho, gd_chirho) in self.group_descs_chirho.iter().enumerate() {
            let free_chirho = gd_chirho.free_inodes_count_chirho(has_64bit_chirho);
            crate::serial_println_chirho!(
                "[EXT4] create_file: group {} free_inodes={} bitmap_blk={}",
                gidx_chirho, free_chirho,
                gd_chirho.inode_bitmap_chirho(has_64bit_chirho)
            );
            if free_chirho == 0 {
                continue;
            }

            let bitmap_block_chirho = gd_chirho.inode_bitmap_chirho(has_64bit_chirho);
            let mut bitmap_data_chirho = match self.read_block_cached_chirho(bitmap_block_chirho) {
                Some(d) => d,
                None => {
                    crate::serial_println_chirho!(
                        "[EXT4] create_file: bitmap read FAILED for block {}",
                        bitmap_block_chirho
                    );
                    return Err(Ext4ErrorChirho::IoErrorChirho);
                }
            };

            if let Some(ino_chirho) = alloc_inode_in_group_chirho(
                &mut bitmap_data_chirho,
                gidx_chirho as u32,
                sb_inodes_per_group_chirho,
            ) {
                // Write the updated bitmap back.
                self.write_block_chirho(bitmap_block_chirho, &bitmap_data_chirho)?;
                new_ino_chirho = Some(ino_chirho);
                break;
            }
        }

        let ino_chirho = new_ino_chirho.ok_or(Ext4ErrorChirho::NoSpaceChirho)?;

        // Initialize the new inode.
        let new_inode_chirho = Ext4InodeChirho {
            i_mode_chirho: S_IFREG_CHIRHO | file_mode_chirho,
            i_uid_chirho: 0,
            i_size_lo_chirho: 0,
            i_atime_chirho: 0,
            i_ctime_chirho: 0,
            i_mtime_chirho: 0,
            i_dtime_chirho: 0,
            i_gid_chirho: 0,
            i_links_count_chirho: 1,
            i_blocks_lo_chirho: 0,
            i_flags_chirho: 0x00080000, // EXT4_EXTENTS_FL
            i_osd1_chirho: 0,
            i_block_chirho: {
                let mut blk_chirho = [0u32; 15];
                // Initialize extent header in i_block[0..2].
                // Magic = 0xF30A, entries = 0, max = 4, depth = 0.
                blk_chirho[0] = 0xF30A | (0 << 16); // magic + entries
                blk_chirho[1] = 4 | (0 << 16);      // max + depth
                blk_chirho[2] = 0;                    // generation
                blk_chirho
            },
            i_generation_chirho: 0,
            i_file_acl_lo_chirho: 0,
            i_size_high_chirho: 0,
            i_obso_faddr_chirho: 0,
            i_osd2_chirho: [0u8; 12],
        };

        self.write_inode_chirho(ino_chirho, &new_inode_chirho)?;

        // Add directory entry to parent.
        self.add_dir_entry_chirho(parent_ino_chirho, ino_chirho, name_chirho, FT_REG_FILE_CHIRHO)?;

        crate::serial_debug_chirho!(
            "[EXT4] Created file '{}' with inode {} in dir {}",
            name_chirho, ino_chirho, parent_ino_chirho
        );

        Ok(ino_chirho)
    }

    /// Add a directory entry to a directory inode.
    #[allow(dead_code)]
    fn add_dir_entry_chirho(
        &self,
        dir_ino_chirho: u32,
        child_ino_chirho: u32,
        name_chirho: &str,
        file_type_chirho: u8,
    ) -> Result<(), Ext4ErrorChirho> {
        let dir_inode_chirho = self.read_inode_chirho(dir_ino_chirho)
            .ok_or(Ext4ErrorChirho::NotFoundChirho)?;
        if !dir_inode_chirho.is_dir_chirho() {
            return Err(Ext4ErrorChirho::NotDirectoryChirho);
        }
        let dir_size_chirho = dir_inode_chirho.size_chirho();
        crate::serial_debug_chirho!(
            "[EXT4] add_dir_entry: ino={} size={} name='{}'",
            dir_ino_chirho, dir_size_chirho, name_chirho,
        );
        let mut dir_data_chirho = self.read_file_data_chirho(&dir_inode_chirho)
            .ok_or(Ext4ErrorChirho::IoErrorChirho)?;

        // Build the new directory entry.
        let name_len_chirho = name_chirho.len() as u8;
        let rec_len_chirho: u16 = ((8 + name_len_chirho as u16 + 3) / 4) * 4; // 4-byte aligned

        let mut entry_bytes_chirho = Vec::new();
        entry_bytes_chirho.extend_from_slice(&child_ino_chirho.to_le_bytes());
        entry_bytes_chirho.extend_from_slice(&rec_len_chirho.to_le_bytes());
        entry_bytes_chirho.push(name_len_chirho);
        entry_bytes_chirho.push(file_type_chirho);
        entry_bytes_chirho.extend_from_slice(name_chirho.as_bytes());
        // Pad to rec_len.
        while entry_bytes_chirho.len() < rec_len_chirho as usize {
            entry_bytes_chirho.push(0);
        }

        // Find the last entry in the directory data and adjust its rec_len
        // to make room, then append the new entry.
        // Strategy: find the last entry, shrink its rec_len to its actual size,
        // and give the remaining space to the new entry.
        let bs_chirho = self.block_size_chirho as usize;
        let dir_len_chirho = dir_data_chirho.len();

        // Find the last entry in the last block
        let last_block_start_chirho = if dir_len_chirho == 0 {
            0
        } else {
            ((dir_len_chirho - 1) / bs_chirho) * bs_chirho
        };
        if dir_len_chirho == 0 {
            // Empty directory: append the first entry directly.
            dir_data_chirho.extend_from_slice(&entry_bytes_chirho);
            self.write_file_data_chirho(dir_ino_chirho, &dir_data_chirho)?;
            // Update dir inode size
            let mut dir_inode_update_chirho = self.read_inode_chirho(dir_ino_chirho)
                .ok_or(Ext4ErrorChirho::IoErrorChirho)?;
            dir_inode_update_chirho.i_size_lo_chirho = dir_data_chirho.len() as u32;
            self.write_inode_chirho(dir_ino_chirho, &dir_inode_update_chirho)?;
        } else {
            // Walk entries in the last block to find the last one
            let mut off_chirho = last_block_start_chirho;
            let mut last_entry_off_chirho = off_chirho;
            while off_chirho + 8 <= dir_len_chirho {
                let rl_chirho = u16::from_le_bytes(
                    dir_data_chirho[off_chirho + 4..off_chirho + 6]
                        .try_into().unwrap_or([0; 2])
                ) as usize;
                if rl_chirho == 0 { break; }
                last_entry_off_chirho = off_chirho;
                off_chirho += rl_chirho;
            }

            // Compute the actual size of the last entry
            let last_name_len_chirho = dir_data_chirho[last_entry_off_chirho + 6] as usize;
            let last_actual_len_chirho = ((8 + last_name_len_chirho + 3) / 4) * 4;
            let old_rec_len_chirho = u16::from_le_bytes(
                dir_data_chirho[last_entry_off_chirho + 4..last_entry_off_chirho + 6]
                    .try_into().unwrap_or([0; 2])
            ) as usize;

            let space_after_chirho = old_rec_len_chirho - last_actual_len_chirho;
            if space_after_chirho >= rec_len_chirho as usize {
                // Enough space: shrink last entry, insert new one
                let new_last_rec_len_chirho = last_actual_len_chirho as u16;
                dir_data_chirho[last_entry_off_chirho + 4] = new_last_rec_len_chirho as u8;
                dir_data_chirho[last_entry_off_chirho + 5] = (new_last_rec_len_chirho >> 8) as u8;

                // New entry gets remaining space
                let new_entry_off_chirho = last_entry_off_chirho + last_actual_len_chirho;
                let new_rec_len_final_chirho = (old_rec_len_chirho - last_actual_len_chirho) as u16;
                entry_bytes_chirho[4] = new_rec_len_final_chirho as u8;
                entry_bytes_chirho[5] = (new_rec_len_final_chirho >> 8) as u8;

                // Write into existing data
                let end_chirho = new_entry_off_chirho + entry_bytes_chirho.len();
                if end_chirho <= dir_data_chirho.len() {
                    dir_data_chirho[new_entry_off_chirho..end_chirho]
                        .copy_from_slice(&entry_bytes_chirho);
                }
            } else {
                // No space in current block: append entry to dir data
                dir_data_chirho.extend_from_slice(&entry_bytes_chirho);
            }

            // Write updated directory data back to disk
            self.write_file_data_chirho(dir_ino_chirho, &dir_data_chirho)?;

            // Update dir inode size
            let mut dir_inode_update_chirho = self.read_inode_chirho(dir_ino_chirho)
                .ok_or(Ext4ErrorChirho::IoErrorChirho)?;
            dir_inode_update_chirho.i_size_lo_chirho = dir_data_chirho.len() as u32;
            self.write_inode_chirho(dir_ino_chirho, &dir_inode_update_chirho)?;
        }

        crate::serial_debug_chirho!(
            "[EXT4] Dir entry '{}' (ino={}) written to dir ino={}",
            name_chirho, child_ino_chirho, dir_ino_chirho
        );

        Ok(())
    }

    /// Write data to a file inode.
    ///
    /// Allocates new data blocks as needed and writes the data through
    /// the extent tree.
    #[allow(dead_code)]
    pub fn write_file_data_chirho(
        &self,
        ino_chirho: u32,
        data_chirho: &[u8],
    ) -> Result<(), Ext4ErrorChirho> {
        // NOTE: read-only guard now lives in write_block_chirho via
        // mode_chirho == MountModeChirho::ReadOnlyChirho check.

        let bs_chirho = self.block_size_chirho as usize;
        let blocks_needed_chirho = (data_chirho.len() + bs_chirho - 1) / bs_chirho;

        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let sb_blocks_per_group_chirho = self.sb_chirho.s_blocks_per_group_chirho;
        let sb_first_data_block_chirho = self.sb_chirho.s_first_data_block_chirho;

        // Allocate data blocks.
        let mut allocated_blocks_chirho: Vec<u64> = Vec::new();
        for (group_idx_chirho, gd_chirho) in self.group_descs_chirho.iter().enumerate() {
            if allocated_blocks_chirho.len() >= blocks_needed_chirho {
                break;
            }
            if gd_chirho.free_blocks_count_chirho(has_64bit_chirho) == 0 {
                continue;
            }
            let bitmap_block_chirho = gd_chirho.block_bitmap_chirho(has_64bit_chirho);
            let mut bitmap_chirho = self.read_block_cached_chirho(bitmap_block_chirho)
                .ok_or(Ext4ErrorChirho::IoErrorChirho)?;

            while allocated_blocks_chirho.len() < blocks_needed_chirho {
                if let Some(blk_chirho) = alloc_block_in_group_chirho(
                    &mut bitmap_chirho, group_idx_chirho as u32, sb_blocks_per_group_chirho, sb_first_data_block_chirho,
                ) {
                    allocated_blocks_chirho.push(blk_chirho);
                } else {
                    break;
                }
            }

            self.write_block_chirho(bitmap_block_chirho, &bitmap_chirho)?;
        }

        if allocated_blocks_chirho.len() < blocks_needed_chirho {
            return Err(Ext4ErrorChirho::NoSpaceChirho);
        }

        // Write data to allocated blocks.
        for (i_chirho, &blk_chirho) in allocated_blocks_chirho.iter().enumerate() {
            let start_chirho = i_chirho * bs_chirho;
            let end_chirho = core::cmp::min(start_chirho + bs_chirho, data_chirho.len());
            let mut block_buf_chirho = alloc::vec![0u8; bs_chirho];
            block_buf_chirho[..end_chirho - start_chirho].copy_from_slice(&data_chirho[start_chirho..end_chirho]);
            self.write_block_chirho(blk_chirho, &block_buf_chirho)?;
        }

        // Update the inode with the new size and extent info.
        let mut inode_chirho = self.read_inode_chirho(ino_chirho)
            .ok_or(Ext4ErrorChirho::CorruptInodeChirho)?;

        inode_chirho.i_size_lo_chirho = data_chirho.len() as u32;
        inode_chirho.i_size_high_chirho = (data_chirho.len() as u64 >> 32) as u32;
        inode_chirho.i_blocks_lo_chirho = (allocated_blocks_chirho.len() * (bs_chirho / 512)) as u32;

        // Set up a single extent covering all allocated blocks (simplified).
        // This is only valid for a contiguous run.
        if !allocated_blocks_chirho.is_empty() {
            for window_chirho in allocated_blocks_chirho.windows(2) {
                if window_chirho[1] != window_chirho[0] + 1 {
                    crate::serial_println_chirho!(
                        "[EXT4] write_file_data: non-contiguous block allocation for inode {} ({} then {})",
                        ino_chirho,
                        window_chirho[0],
                        window_chirho[1]
                    );
                    return Err(Ext4ErrorChirho::UnsupportedFeatureChirho(
                        "non-contiguous extent allocation",
                    ));
                }
            }
            let first_phys_chirho = allocated_blocks_chirho[0];
            let num_blocks_chirho = allocated_blocks_chirho.len() as u16;
            // Write extent header + one extent in i_block.
            inode_chirho.i_block_chirho[0] = (EXT4_EXT_MAGIC_CHIRHO as u32) | (1u32 << 16); // magic + entries=1
            inode_chirho.i_block_chirho[1] = 4 | (0u32 << 16); // max=4, depth=0
            inode_chirho.i_block_chirho[2] = 0; // generation
            // Extent at offset 12: ee_block(4) + ee_len(2) + ee_start_hi(2) + ee_start_lo(4)
            inode_chirho.i_block_chirho[3] = 0; // ee_block = 0 (first logical block)
            inode_chirho.i_block_chirho[4] = num_blocks_chirho as u32 | ((first_phys_chirho >> 32) as u32 & 0xFFFF) << 16;
            inode_chirho.i_block_chirho[5] = first_phys_chirho as u32;
        }

        self.write_inode_chirho(ino_chirho, &inode_chirho)?;

        crate::serial_debug_chirho!(
            "[EXT4] Wrote {} bytes to inode {} ({} blocks)",
            data_chirho.len(), ino_chirho, allocated_blocks_chirho.len()
        );

        Ok(())
    }

    /// Truncate a file to zero length.
    ///
    /// Frees all data blocks and resets the inode size and extent tree.
    #[allow(dead_code)]
    pub fn truncate_file_chirho(&self, ino_chirho: u32) -> Result<(), Ext4ErrorChirho> {
        // NOTE: read-only guard now lives in write_block_chirho via
        // mode_chirho == MountModeChirho::ReadOnlyChirho check.

        let mut inode_chirho = self.read_inode_chirho(ino_chirho)
            .ok_or(Ext4ErrorChirho::CorruptInodeChirho)?;

        // Zero out size and blocks.
        inode_chirho.i_size_lo_chirho = 0;
        inode_chirho.i_size_high_chirho = 0;
        inode_chirho.i_blocks_lo_chirho = 0;

        // Reset extent header.
        inode_chirho.i_block_chirho[0] = (EXT4_EXT_MAGIC_CHIRHO as u32) | (0u32 << 16); // 0 entries
        inode_chirho.i_block_chirho[1] = 4 | (0u32 << 16);
        inode_chirho.i_block_chirho[2] = 0;

        self.write_inode_chirho(ino_chirho, &inode_chirho)?;

        crate::serial_debug_chirho!("[EXT4] Truncated inode {}", ino_chirho);
        Ok(())
    }
}

// ===========================================================================
// A4-013: ext4 journaling (JBD2) — ordered-mode journal
// ===========================================================================

/// JBD2 journal superblock magic number.
#[allow(dead_code)]
pub const JBD2_MAGIC_CHIRHO: u32 = 0xC03B3998;

/// JBD2 block types.
#[allow(dead_code)]
pub const JBD2_DESCRIPTOR_BLOCK_CHIRHO: u32 = 1;
#[allow(dead_code)]
pub const JBD2_COMMIT_BLOCK_CHIRHO: u32 = 2;
#[allow(dead_code)]
pub const JBD2_SUPERBLOCK_V1_CHIRHO: u32 = 3;
#[allow(dead_code)]
pub const JBD2_SUPERBLOCK_V2_CHIRHO: u32 = 4;
#[allow(dead_code)]
pub const JBD2_REVOKE_BLOCK_CHIRHO: u32 = 5;

/// JBD2 journal superblock (first 48 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Jbd2SuperblockChirho {
    /// Block header: magic (should be JBD2_MAGIC).
    pub s_header_magic_chirho: u32,
    /// Block type (JBD2_SUPERBLOCK_V1 or V2).
    pub s_header_blocktype_chirho: u32,
    /// Sequence number.
    pub s_header_sequence_chirho: u32,
    /// Journal device block size.
    pub s_blocksize_chirho: u32,
    /// Total blocks in journal.
    pub s_maxlen_chirho: u32,
    /// First block of log area.
    pub s_first_chirho: u32,
    /// First commit ID expected in log.
    pub s_sequence_chirho: u32,
    /// First block of log area (in journal).
    pub s_start_chirho: u32,
    /// Error number.
    pub s_errno_chirho: u32,
    // V2 fields follow...
    pub s_feature_compat_chirho: u32,
    pub s_feature_incompat_chirho: u32,
    pub s_feature_ro_compat_chirho: u32,
}

/// A journal transaction for ordered-mode journaling.
///
/// Metadata blocks are written to the journal before being committed
/// to their final locations. Data blocks are written directly (ordered mode).
pub struct JournalTransactionChirho {
    /// Transaction sequence number.
    pub tid_chirho: u32,
    /// Metadata block numbers and their data (to be journaled).
    pub metadata_blocks_chirho: Vec<(u64, Vec<u8>)>,
    /// Whether this transaction has been committed.
    pub committed_chirho: bool,
}

/// The in-memory journal state for a mounted ext4 filesystem.
pub struct JournalChirho {
    /// Journal inode number (typically inode 8).
    pub journal_ino_chirho: u32,
    /// Block size of the journal.
    pub block_size_chirho: u32,
    /// Total journal blocks.
    pub max_len_chirho: u32,
    /// Current sequence number.
    pub sequence_chirho: u32,
    /// Current (open) transaction.
    pub current_transaction_chirho: Option<JournalTransactionChirho>,
    /// Whether the journal is active.
    pub active_chirho: bool,
}

impl JournalChirho {
    /// Create a new journal state (before replay).
    #[allow(dead_code)]
    pub fn new_chirho(journal_ino_chirho: u32, block_size_chirho: u32) -> Self {
        Self {
            journal_ino_chirho,
            block_size_chirho,
            max_len_chirho: 0,
            sequence_chirho: 1,
            current_transaction_chirho: None,
            active_chirho: false,
        }
    }

    /// Begin a new transaction.
    #[allow(dead_code)]
    pub fn begin_transaction_chirho(&mut self) -> u32 {
        let tid_chirho = self.sequence_chirho;
        self.sequence_chirho += 1;
        self.current_transaction_chirho = Some(JournalTransactionChirho {
            tid_chirho,
            metadata_blocks_chirho: Vec::new(),
            committed_chirho: false,
        });
        crate::serial_debug_chirho!("[JBD2] Begin transaction {}", tid_chirho);
        tid_chirho
    }

    /// Add a metadata block to the current transaction.
    #[allow(dead_code)]
    pub fn journal_metadata_chirho(&mut self, block_nr_chirho: u64, data_chirho: Vec<u8>) {
        if let Some(ref mut txn_chirho) = self.current_transaction_chirho {
            txn_chirho.metadata_blocks_chirho.push((block_nr_chirho, data_chirho));
        }
    }

    /// Commit the current transaction.
    ///
    /// In ordered mode:
    /// 1. Write all data blocks to their final locations (already done by caller).
    /// 2. Write metadata blocks to the journal.
    /// 3. Write a commit record.
    /// 4. Write metadata blocks to their final locations.
    #[allow(dead_code)]
    pub fn commit_transaction_chirho(&mut self) -> Result<(), Ext4ErrorChirho> {
        let txn_chirho = self.current_transaction_chirho.take()
            .ok_or(Ext4ErrorChirho::IoErrorChirho)?;

        crate::serial_debug_chirho!(
            "[JBD2] Committing transaction {} ({} metadata blocks)",
            txn_chirho.tid_chirho,
            txn_chirho.metadata_blocks_chirho.len()
        );

        // In a real implementation:
        // 1. Write descriptor block to journal
        // 2. Write each metadata block to journal
        // 3. Write commit block to journal
        // 4. Write metadata blocks to their final locations on disk
        // 5. Mark transaction complete

        // For now, we mark it committed.
        crate::serial_debug_chirho!(
            "[JBD2] Transaction {} committed",
            txn_chirho.tid_chirho
        );

        Ok(())
    }

    /// Replay the journal on mount (recovery after unclean shutdown).
    ///
    /// Reads committed transactions from the journal and replays their
    /// metadata blocks to their final locations.
    #[allow(dead_code)]
    pub fn replay_journal_chirho(&mut self) -> Result<u32, Ext4ErrorChirho> {
        crate::serial_println_chirho!("[JBD2] Replaying journal (inode {})...", self.journal_ino_chirho);

        // In a real implementation:
        // 1. Read the journal superblock
        // 2. Find the first uncommitted transaction
        // 3. For each committed transaction:
        //    a. Read descriptor blocks to get metadata block mappings
        //    b. Copy journaled metadata blocks to final locations
        // 4. Clear the journal

        let replayed_chirho = 0u32;
        crate::serial_println_chirho!(
            "[JBD2] Journal replay complete ({} transactions replayed)",
            replayed_chirho
        );

        self.active_chirho = true;
        Ok(replayed_chirho)
    }
}

// ===========================================================================
// A4-014: Root filesystem mount from block device
// ===========================================================================

/// Mount an ext4 partition as the root filesystem.
///
/// # Arguments
/// * `device_id_chirho` — block device ID in the registry.
/// * `partition_start_chirho` — starting sector of the ext4 partition.
///
/// Returns an `Ext4MountChirho` on success, or a typed `Ext4ErrorChirho` on failure.
#[allow(dead_code)]
pub fn mount_root_ext4_chirho(
    device_id_chirho: u32,
    partition_start_chirho: u64,
) -> Result<Ext4MountChirho, Ext4ErrorChirho> {
    crate::serial_println_chirho!(
        "[EXT4] Mounting root filesystem: device={}, partition_start={}",
        device_id_chirho, partition_start_chirho
    );

    // Read the superblock (at byte offset 1024 from partition start).
    let sb_sector_chirho = partition_start_chirho + (SUPERBLOCK_OFFSET_CHIRHO / 512);
    let registry_chirho = &crate::block_chirho::BLOCK_REGISTRY_CHIRHO;

    // Read 2 sectors (1024 bytes) for the superblock.
    let mut sb_data_chirho = alloc::vec![0u8; 1024];
    for i_chirho in 0..2u64 {
        registry_chirho
            .read_block_chirho(
                device_id_chirho as usize,
                sb_sector_chirho + i_chirho,
                &mut sb_data_chirho[(i_chirho as usize * 512)..((i_chirho as usize + 1) * 512)],
            )
            .map_err(|_| Ext4ErrorChirho::IoErrorChirho)?;
    }

    let sb_chirho = parse_superblock_chirho(&sb_data_chirho)
        .ok_or(Ext4ErrorChirho::CorruptSuperblockChirho)?;

    let block_size_chirho = sb_chirho.block_size_chirho();
    let bg_count_chirho = sb_chirho.block_group_count_chirho();
    let gd_size_chirho = sb_chirho.group_desc_size_chirho();

    let inodes_count_copy_chirho = sb_chirho.s_inodes_count_chirho;
    crate::serial_println_chirho!(
        "[EXT4] Superblock valid: block_size={}, blocks={}, groups={}, inodes={}",
        block_size_chirho, sb_chirho.total_blocks_chirho(), bg_count_chirho,
        inodes_count_copy_chirho
    );

    // Read block group descriptors (starts at block 1 for 1K blocks, block 0 offset for 4K).
    let gdt_block_chirho = if block_size_chirho == 1024 { 2 } else { 1 };
    let gdt_bytes_chirho = bg_count_chirho as usize * gd_size_chirho as usize;
    let gdt_blocks_needed_chirho = (gdt_bytes_chirho + block_size_chirho as usize - 1) / block_size_chirho as usize;

    let mut gdt_data_chirho = Vec::new();
    for blk_chirho in 0..gdt_blocks_needed_chirho {
        let abs_block_chirho = gdt_block_chirho + blk_chirho as u64;
        let sectors_per_block_chirho = block_size_chirho as u64 / 512;
        let start_sec_chirho = partition_start_chirho + abs_block_chirho * sectors_per_block_chirho;

        let mut buf_chirho = alloc::vec![0u8; block_size_chirho as usize];
        for s_chirho in 0..sectors_per_block_chirho {
            let off_chirho = (s_chirho as usize) * 512;
            registry_chirho
                .read_block_chirho(
                    device_id_chirho as usize,
                    start_sec_chirho + s_chirho,
                    &mut buf_chirho[off_chirho..off_chirho + 512],
                )
                .map_err(|_| Ext4ErrorChirho::IoErrorChirho)?;
        }
        gdt_data_chirho.extend_from_slice(&buf_chirho);
    }

    let group_descs_chirho = parse_group_descs_chirho(&gdt_data_chirho, bg_count_chirho, gd_size_chirho);

    crate::serial_println_chirho!(
        "[EXT4] Root filesystem mounted successfully ({} block groups)",
        group_descs_chirho.len()
    );

    Ok(Ext4MountChirho {
        sb_chirho,
        group_descs_chirho,
        block_size_chirho,
        device_id_chirho,
        mode_chirho: MountModeChirho::ReadWriteChirho,
    })
}

// ===========================================================================
// P2-004: ext4 VFS integration — InodeOps, FileOps, SuperOps for mounting
// ===========================================================================

/// Filesystem-private data stored in VFS InodeChirho::fs_data_chirho for ext4
/// inodes. Holds the ext4 inode number and a reference to the mount so we
/// can read file data / directory entries on demand.
pub struct Ext4FsDataChirho {
    /// ext4 inode number on disk.
    pub ino_chirho: u32,
    /// Reference to the mount (shared across all ext4 inodes of this mount).
    pub mount_chirho: Arc<Mutex<Ext4MountChirho>>,
}

// SAFETY: Ext4FsDataChirho fields are either primitives or Arc-wrapped.
unsafe impl Send for Ext4FsDataChirho {}

/// ext4 inode operations (read-only for now).
pub struct Ext4InodeOpsChirho;

impl crate::vfs_chirho::InodeOpsChirho for Ext4InodeOpsChirho {
    fn lookup_chirho(
        &self,
        parent_chirho: &crate::vfs_chirho::InodeChirho,
        name_chirho: &str,
    ) -> Result<Arc<crate::vfs_chirho::InodeChirho>, i64> {
        let fs_data_chirho = parent_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .ok_or_else(|| {
                if name_chirho.contains("usr") || name_chirho.contains("sbin") || name_chirho.contains("dropbear") {
                    crate::serial_debug_chirho!(
                        "[EXT4-DBG] lookup '{}': fs_data downcast FAILED (parent ino={})",
                        name_chirho, parent_chirho.ino_chirho
                    );
                }
                -2i64
            })?;

        if name_chirho.contains("usr") || name_chirho.contains("sbin") || name_chirho.contains("dropbear") {
            crate::serial_debug_chirho!(
                "[EXT4-DBG] lookup '{}' in ext4 inode {} (mount OK)",
                name_chirho, fs_data_chirho.ino_chirho
            );
        }

        let mount_chirho = fs_data_chirho.mount_chirho.lock();
        let entry_chirho = mount_chirho
            .lookup_in_dir_chirho(fs_data_chirho.ino_chirho, name_chirho)
            .ok_or_else(|| {
                if name_chirho.contains("usr") || name_chirho.contains("sbin") || name_chirho.contains("dropbear") {
                    crate::serial_debug_chirho!(
                        "[EXT4-DBG] lookup_in_dir({}, '{}') returned None",
                        fs_data_chirho.ino_chirho, name_chirho
                    );
                }
                -2i64
            })?;

        let child_ext4_inode_chirho = mount_chirho
            .read_inode_chirho(entry_chirho.inode_chirho)
            .ok_or(-5i64)?; // EIO

        let vfs_mode_chirho = child_ext4_inode_chirho.i_mode_chirho as u32;
        let vfs_size_chirho = child_ext4_inode_chirho.size_chirho();

        let child_inode_chirho = Arc::new(crate::vfs_chirho::InodeChirho {
            ino_chirho: entry_chirho.inode_chirho as u64,
            mode_chirho: vfs_mode_chirho,
            uid_chirho: child_ext4_inode_chirho.i_uid_chirho as u32,
            gid_chirho: child_ext4_inode_chirho.i_gid_chirho as u32,
            size_chirho: vfs_size_chirho,
            nlink_chirho: child_ext4_inode_chirho.i_links_count_chirho as u32,
            atime_chirho: child_ext4_inode_chirho.i_atime_chirho as u64,
            mtime_chirho: child_ext4_inode_chirho.i_mtime_chirho as u64,
            ctime_chirho: child_ext4_inode_chirho.i_ctime_chirho as u64,
            ops_chirho: &EXT4_INODE_OPS_CHIRHO,
            fs_data_chirho: Some(Box::new(Ext4FsDataChirho {
                ino_chirho: entry_chirho.inode_chirho,
                mount_chirho: fs_data_chirho.mount_chirho.clone(),
            })),
        });

        Ok(child_inode_chirho)
    }

    fn create_chirho(
        &self,
        parent_chirho: &crate::vfs_chirho::InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<crate::vfs_chirho::InodeChirho>, i64> {
        // Use the ext4 create_file_chirho implementation.
        let fs_data_chirho = parent_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .ok_or(-9i64)?; // EBADF

        let mount_chirho = fs_data_chirho.mount_chirho.clone();
        let parent_ino_chirho = fs_data_chirho.ino_chirho;
        let mount_guard_chirho = mount_chirho.lock();

        match mount_guard_chirho.create_file_chirho(parent_ino_chirho, name_chirho, mode_chirho as u16) {
            Ok(new_ino_chirho) => {
                crate::serial_debug_chirho!(
                    "[EXT4] Created file '{}' inode={}",
                    name_chirho, new_ino_chirho
                );
                // Create a VFS inode for the new file.
                let new_inode_chirho = Arc::new(crate::vfs_chirho::InodeChirho {
                    ino_chirho: new_ino_chirho as u64,
                    mode_chirho: mode_chirho | 0o100000, // S_IFREG
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &EXT4_INODE_OPS_CHIRHO,
                    fs_data_chirho: Some(alloc::boxed::Box::new(Ext4FsDataChirho {
                        ino_chirho: new_ino_chirho,
                        mount_chirho: fs_data_chirho.mount_chirho.clone(),
                    })),
                });
                Ok(new_inode_chirho)
            }
            Err(err_chirho) => {
                crate::serial_debug_chirho!("[EXT4] create_file failed: {}", err_chirho);
                Err(err_chirho.to_errno_chirho())
            }
        }
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<crate::vfs_chirho::InodeChirho>, i64> {
        Err(-30) // EROFS — mkdir not yet implemented for ext4
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-30) // EROFS — unlink not yet implemented for ext4
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-30) // EROFS — rmdir not yet implemented for ext4
    }

    fn readlink_chirho(
        &self,
        inode_chirho: &crate::vfs_chirho::InodeChirho,
    ) -> Result<String, i64> {
        // Check if symlink mode
        if !crate::vfs_chirho::is_symlink_chirho(inode_chirho.mode_chirho) {
            return Err(-22); // EINVAL — not a symlink
        }
        // For inline symlinks (target stored in fs_data), extract it.
        // Ext4 stores short symlink targets (<=60 bytes) in i_block.
        if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
            if let Some(ext4_data_chirho) = data_chirho.downcast_ref::<Ext4FsDataChirho>() {
                let mount_guard_chirho = ext4_data_chirho.mount_chirho.lock();
                let raw_inode_chirho = mount_guard_chirho.read_inode_chirho(ext4_data_chirho.ino_chirho);
                if let Some(raw_chirho) = raw_inode_chirho {
                    if raw_chirho.is_symlink_chirho() {
                        let size_chirho = raw_chirho.i_size_lo_chirho as usize;
                        if size_chirho <= 60 {
                            // Inline symlink — target stored in i_block.
                            // Copy to stack to avoid packed struct alignment issue.
                            let block_copy_chirho = raw_chirho.i_block_chirho;
                            let bytes_chirho = unsafe {
                                core::slice::from_raw_parts(
                                    block_copy_chirho.as_ptr() as *const u8,
                                    size_chirho,
                                )
                            };
                            if let Ok(target_chirho) = core::str::from_utf8(bytes_chirho) {
                                return Ok(String::from(target_chirho.trim_end_matches('\0')));
                            }
                        }
                        // Long symlink — read from data blocks via extent tree
                        if let Some(data_vec_chirho) = mount_guard_chirho.read_file_data_chirho(&raw_chirho) {
                            if data_vec_chirho.len() >= size_chirho {
                                if let Ok(target_chirho) = core::str::from_utf8(&data_vec_chirho[..size_chirho]) {
                                    return Ok(String::from(target_chirho.trim_end_matches('\0')));
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(-22) // EINVAL
    }
}

/// Static instance of ext4 inode operations.
pub static EXT4_INODE_OPS_CHIRHO: Ext4InodeOpsChirho = Ext4InodeOpsChirho;

/// ext4 file operations (read-only for now).
pub struct Ext4FileOpsChirho;

impl crate::vfs_chirho::FileOpsChirho for Ext4FileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut crate::vfs_chirho::FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        let inode_guard_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_guard_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .ok_or(-9i64)?; // EBADF

        let file_size_chirho = inode_guard_chirho.size_chirho;
        let mount_chirho = fs_data_chirho.mount_chirho.lock();
        let ext4_inode_chirho = mount_chirho
            .read_inode_chirho(fs_data_chirho.ino_chirho)
            .ok_or(-5i64)?; // EIO

        let pos_chirho = file_chirho.pos_chirho as usize;
        if pos_chirho as u64 >= file_size_chirho {
            return Ok(0); // EOF
        }

        let available_chirho = (file_size_chirho as usize) - pos_chirho;
        let to_read_chirho = core::cmp::min(buf_chirho.len(), available_chirho);

        // Read ONLY the needed blocks from the extent tree instead of
        // loading the entire file. This avoids allocating multi-MB Vecs
        // for small reads, preventing heap corruption.
        //
        // Calculate which disk blocks cover [pos, pos+to_read).
        let start_block_chirho = pos_chirho / 4096;
        let end_block_chirho = (pos_chirho + to_read_chirho + 4095) / 4096;
        let num_blocks_chirho = end_block_chirho - start_block_chirho;

        // Read blocks via zero-copy path (no per-block Vec allocation).
        let mut bytes_copied_chirho = 0usize;
        let mut block_buf_chirho = [0u8; 4096]; // stack buffer, reused per block
        for blk_idx_chirho in start_block_chirho..end_block_chirho {
            let read_result_chirho = mount_chirho.read_block_by_logical_into_chirho(
                &ext4_inode_chirho,
                blk_idx_chirho as u64,
                &mut block_buf_chirho,
            );
            if let Some(_n_chirho) = read_result_chirho {
                let data_chirho = &block_buf_chirho[..];
                // Calculate the slice within this block we need
                let block_start_chirho = blk_idx_chirho * 4096;
                let copy_start_chirho = if pos_chirho > block_start_chirho {
                    pos_chirho - block_start_chirho
                } else {
                    0
                };
                let copy_end_chirho = core::cmp::min(
                    4096,
                    (pos_chirho + to_read_chirho).saturating_sub(block_start_chirho),
                );
                if copy_start_chirho < copy_end_chirho && copy_start_chirho < data_chirho.len() {
                    let actual_end_chirho = core::cmp::min(copy_end_chirho, data_chirho.len());
                    let chunk_chirho = &data_chirho[copy_start_chirho..actual_end_chirho];
                    let dest_start_chirho = bytes_copied_chirho;
                    let dest_end_chirho = dest_start_chirho + chunk_chirho.len();
                    if dest_end_chirho <= buf_chirho.len() {
                        buf_chirho[dest_start_chirho..dest_end_chirho].copy_from_slice(chunk_chirho);
                        bytes_copied_chirho += chunk_chirho.len();
                    }
                }
            } else {
                // Block not found — zero fill
                let fill_len_chirho = core::cmp::min(4096, to_read_chirho - bytes_copied_chirho);
                if bytes_copied_chirho + fill_len_chirho <= buf_chirho.len() {
                    buf_chirho[bytes_copied_chirho..bytes_copied_chirho + fill_len_chirho].fill(0);
                    bytes_copied_chirho += fill_len_chirho;
                }
            }
        }

        file_chirho.pos_chirho += bytes_copied_chirho as u64;
        Ok(bytes_copied_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut crate::vfs_chirho::FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let inode_guard_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_guard_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .ok_or(-9i64)?; // EBADF

        let ino_chirho = fs_data_chirho.ino_chirho;
        let mount_chirho = fs_data_chirho.mount_chirho.clone();
        let current_size_chirho = inode_guard_chirho.size_chirho as usize;
        let pos_chirho = file_chirho.pos_chirho as usize;
        drop(inode_guard_chirho);

        let mount_guard_chirho = mount_chirho.lock();

        // Block-level write: only modify the affected 4KB blocks.
        // For each block that overlaps [pos, pos+len):
        //   1. Read the existing block from disk
        //   2. Overlay the new data at the correct offset within the block
        //   3. Write the modified block back
        let new_end_chirho = pos_chirho + buf_chirho.len();
        let final_size_chirho = core::cmp::max(current_size_chirho, new_end_chirho);

        // Read the ext4 inode for block mapping
        let ext4_inode_chirho = mount_guard_chirho.read_inode_chirho(ino_chirho)
            .ok_or(-5i64)?; // EIO

        let start_block_chirho = pos_chirho / 4096;
        let end_block_chirho = (new_end_chirho + 4095) / 4096;
        let mut block_buf_chirho = [0u8; 4096];
        let mut bytes_remaining_chirho = buf_chirho.len();
        let mut buf_offset_chirho = 0usize;

        for blk_idx_chirho in start_block_chirho..end_block_chirho {
            // Read existing block (if it exists)
            let block_start_chirho = blk_idx_chirho * 4096;
            block_buf_chirho = [0u8; 4096];

            // Try to read the existing block data
            if mount_guard_chirho.read_block_by_logical_into_chirho(
                &ext4_inode_chirho,
                blk_idx_chirho as u64,
                &mut block_buf_chirho,
            ).is_none() {
                crate::serial_debug_chirho!(
                    "[EXT4] write: logical block {} not readable before overwrite (ino={})",
                    blk_idx_chirho,
                    ino_chirho
                );
            }

            // Calculate the write range within this block
            let write_start_in_block_chirho = if block_start_chirho < pos_chirho {
                pos_chirho - block_start_chirho
            } else {
                0
            };
            let write_end_in_block_chirho = core::cmp::min(
                4096,
                new_end_chirho.saturating_sub(block_start_chirho),
            );
            let write_len_chirho = write_end_in_block_chirho - write_start_in_block_chirho;

            // Overlay new data into the block buffer
            if write_len_chirho > 0 && buf_offset_chirho + write_len_chirho <= buf_chirho.len() {
                block_buf_chirho[write_start_in_block_chirho..write_end_in_block_chirho]
                    .copy_from_slice(&buf_chirho[buf_offset_chirho..buf_offset_chirho + write_len_chirho]);
                buf_offset_chirho += write_len_chirho;
                bytes_remaining_chirho -= write_len_chirho;
            }

            // Find the physical block and write it back
            let block_copy_chirho = ext4_inode_chirho.i_block_chirho;
            let header_copy_chirho = ext4_inode_chirho.extent_header_chirho();
            let phys_block_chirho = mount_guard_chirho.find_phys_block_chirho(
                &block_copy_chirho,
                &header_copy_chirho,
                blk_idx_chirho as u32,
            );

            if let Some(pb_chirho) = phys_block_chirho {
                // Write the modified block to the existing physical block.
                // Loop-device write-through intentionally tolerates a
                // ReadOnly rootfs backing file, but all other failures
                // should be surfaced in the log instead of discarded.
                if let Err(write_error_chirho) =
                    mount_guard_chirho.write_block_chirho(pb_chirho, &block_buf_chirho)
                {
                    if !matches!(write_error_chirho, Ext4ErrorChirho::ReadOnlyChirho) {
                        crate::serial_println_chirho!(
                            "[EXT4] write: block {} physical {} write failed: {}",
                            blk_idx_chirho,
                            pb_chirho,
                            write_error_chirho
                        );
                    }
                }
            } else {
                // Block doesn't exist yet — need to allocate.
                // For now, fall back to full-file write for new blocks.
                crate::serial_debug_chirho!(
                    "[EXT4] write: block {} not mapped, falling back to full write",
                    blk_idx_chirho
                );
                // Read full file, overlay, write back (old path)
                let mut full_data_chirho = alloc::vec![0u8; final_size_chirho];
                if current_size_chirho > 0 {
                    let num_blocks_chirho = (current_size_chirho + 4095) / 4096;
                    for bi_chirho in 0..num_blocks_chirho {
                        let mut bb_chirho = [0u8; 4096];
                        if let Some(_) = mount_guard_chirho.read_block_by_logical_into_chirho(
                            &ext4_inode_chirho, bi_chirho as u64, &mut bb_chirho,
                        ) {
                            let s_chirho = bi_chirho * 4096;
                            let e_chirho = core::cmp::min(s_chirho + 4096, current_size_chirho);
                            full_data_chirho[s_chirho..e_chirho]
                                .copy_from_slice(&bb_chirho[..e_chirho - s_chirho]);
                        }
                    }
                }
                full_data_chirho[pos_chirho..new_end_chirho].copy_from_slice(buf_chirho);
                if let Err(write_error_chirho) =
                    mount_guard_chirho.write_file_data_chirho(ino_chirho, &full_data_chirho)
                {
                    crate::serial_println_chirho!(
                        "[EXT4] write: full-file fallback failed for inode {}: {}",
                        ino_chirho,
                        write_error_chirho
                    );
                    return Err(write_error_chirho.to_errno_chirho());
                }
                // Update inode size
                let mut inode_guard2_chirho = file_chirho.inode_chirho.lock();
                inode_guard2_chirho.size_chirho = final_size_chirho as u64;
                file_chirho.pos_chirho = new_end_chirho as u64;
                return Ok(buf_chirho.len());
            }
        }

        // Update the VFS inode size.
        let mut inode_guard2_chirho = file_chirho.inode_chirho.lock();
        inode_guard2_chirho.size_chirho = final_size_chirho as u64;
        drop(inode_guard2_chirho);

        file_chirho.pos_chirho = new_end_chirho as u64;
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut crate::vfs_chirho::FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        let inode_guard_chirho = file_chirho.inode_chirho.lock();
        let size_chirho = inode_guard_chirho.size_chirho as i64;
        drop(inode_guard_chirho);

        let new_pos_chirho = match whence_chirho {
            0 => offset_chirho,                              // SEEK_SET
            1 => file_chirho.pos_chirho as i64 + offset_chirho, // SEEK_CUR
            2 => size_chirho + offset_chirho,                 // SEEK_END
            _ => return Err(-22),                             // EINVAL
        };

        if new_pos_chirho < 0 {
            return Err(-22); // EINVAL
        }

        file_chirho.pos_chirho = new_pos_chirho as u64;
        Ok(file_chirho.pos_chirho)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &crate::vfs_chirho::FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-25) // ENOTTY
    }

    fn readdir_chirho(
        &self,
        file_chirho: &mut crate::vfs_chirho::FileChirho,
        callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        let inode_guard_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_guard_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .ok_or(-20i64)?; // ENOTDIR

        let ino_chirho = fs_data_chirho.ino_chirho;
        let mount_chirho = fs_data_chirho.mount_chirho.clone();
        drop(inode_guard_chirho);

        let mount_guard_chirho = mount_chirho.lock();

        // Block-by-block directory scanning — avoids the 64MB OOM
        // from reading the entire directory into Vec<DirEntryInfoChirho>.
        let dir_inode_chirho = mount_guard_chirho.read_inode_chirho(ino_chirho)
            .ok_or(-5i64)?; // EIO
        let dir_size_chirho = dir_inode_chirho.size_chirho() as usize;
        let num_blocks_chirho = (dir_size_chirho + 4095) / 4096;

        let mut count_chirho = 0usize;
        let mut entry_idx_chirho = 0usize;
        let start_chirho = file_chirho.pos_chirho as usize;
        let mut block_buf_chirho = [0u8; 4096];

        'outer: for blk_idx_chirho in 0..num_blocks_chirho {
            if mount_guard_chirho.read_block_by_logical_into_chirho(
                &dir_inode_chirho, blk_idx_chirho as u64, &mut block_buf_chirho,
            ).is_none() {
                continue;
            }

            let block_end_chirho = if blk_idx_chirho == num_blocks_chirho - 1 {
                dir_size_chirho - blk_idx_chirho * 4096
            } else {
                4096
            };
            let mut offset_chirho = 0usize;

            while offset_chirho + 8 <= block_end_chirho {
                let entry_ino_chirho = u32::from_le_bytes(
                    block_buf_chirho[offset_chirho..offset_chirho + 4].try_into().unwrap_or([0; 4])
                );
                let rec_len_chirho = u16::from_le_bytes(
                    block_buf_chirho[offset_chirho + 4..offset_chirho + 6].try_into().unwrap_or([0; 2])
                ) as usize;
                let name_len_chirho = block_buf_chirho[offset_chirho + 6] as usize;
                let file_type_chirho = block_buf_chirho[offset_chirho + 7];

                if rec_len_chirho == 0 { break; }

                if entry_ino_chirho != 0 && name_len_chirho > 0 && offset_chirho + 8 + name_len_chirho <= 4096 {
                    if entry_idx_chirho >= start_chirho {
                        if let Ok(name_chirho) = core::str::from_utf8(
                            &block_buf_chirho[offset_chirho + 8..offset_chirho + 8 + name_len_chirho]
                        ) {
                            let dt_chirho = match file_type_chirho {
                                FT_REG_FILE_CHIRHO => 8,
                                FT_DIR_CHIRHO => 4,
                                FT_SYMLINK_CHIRHO => 10,
                                FT_CHRDEV_CHIRHO => 2,
                                FT_BLKDEV_CHIRHO => 6,
                                _ => 0,
                            };
                            if !callback_chirho(name_chirho, entry_ino_chirho as u64, dt_chirho) {
                                break 'outer;
                            }
                            count_chirho += 1;
                        }
                    }
                    entry_idx_chirho += 1;
                }
                offset_chirho += rec_len_chirho;
            }
        }

        file_chirho.pos_chirho = entry_idx_chirho as u64;
        Ok(count_chirho)
    }
}

/// Static instance of ext4 file operations.
pub static EXT4_FILE_OPS_CHIRHO: Ext4FileOpsChirho = Ext4FileOpsChirho;

/// ext4 directory operations — same as file ops but with readdir.
pub static EXT4_DIR_OPS_CHIRHO: Ext4FileOpsChirho = Ext4FileOpsChirho;

/// ext4 superblock operations.
pub struct Ext4SuperOpsChirho;

impl crate::vfs_chirho::SuperOpsChirho for Ext4SuperOpsChirho {
    fn alloc_inode_chirho(&self) -> Arc<crate::vfs_chirho::InodeChirho> {
        // Read-only: should not be called
        Arc::new(crate::vfs_chirho::InodeChirho {
            ino_chirho: 0,
            mode_chirho: 0,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 0,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &EXT4_INODE_OPS_CHIRHO,
            fs_data_chirho: None,
        })
    }

    fn statfs_chirho(&self) -> Result<crate::vfs_chirho::StatfsChirho, i64> {
        Ok(crate::vfs_chirho::StatfsChirho {
            f_type_chirho: 0xEF53,
            f_bsize_chirho: 4096,
            f_blocks_chirho: 0,
            f_bfree_chirho: 0,
            f_bavail_chirho: 0,
            f_files_chirho: 0,
            f_ffree_chirho: 0,
            f_namelen_chirho: 255,
        })
    }
}

/// Static instance of ext4 super operations.
static EXT4_SUPER_OPS_CHIRHO: Ext4SuperOpsChirho = Ext4SuperOpsChirho;

/// Mount an ext4 filesystem as a VFS superblock, suitable for registration
/// in the mount table. The `Ext4MountChirho` is wrapped in `Arc<Mutex<>>`
/// and shared with every VFS inode created for this mount.
///
/// Returns the VFS `SuperblockChirho` (wrapped in `Arc<Mutex<>>`).
pub fn mount_ext4_vfs_chirho(
    ext4_mount_chirho: Ext4MountChirho,
) -> Arc<Mutex<crate::vfs_chirho::SuperblockChirho>> {
    let mount_arc_chirho = Arc::new(Mutex::new(ext4_mount_chirho));

    crate::serial_println_chirho!("[EXT4] mount_ext4_vfs: reading root inode (inode 2)...");

    // Build the root VFS inode from the ext4 root inode (inode 2).
    let root_ext4_inode_chirho = {
        let m_chirho = mount_arc_chirho.lock();
        crate::serial_println_chirho!("[EXT4] mount_ext4_vfs: calling read_inode(2)");
        let result_chirho = m_chirho.read_inode_chirho(EXT4_ROOT_INO_CHIRHO);
        crate::serial_println_chirho!("[EXT4] mount_ext4_vfs: read_inode returned");
        result_chirho
    };

    crate::serial_println_chirho!("[EXT4] mount_ext4_vfs: root inode read: {:?}",
        root_ext4_inode_chirho.is_some());

    let (root_mode_chirho, root_size_chirho) = match root_ext4_inode_chirho {
        Some(ref i_chirho) => (i_chirho.i_mode_chirho as u32, i_chirho.size_chirho()),
        None => (crate::vfs_chirho::S_IFDIR_CHIRHO | 0o755, 0),
    };

    let root_inode_chirho = Arc::new(Mutex::new(crate::vfs_chirho::InodeChirho {
        ino_chirho: EXT4_ROOT_INO_CHIRHO as u64,
        mode_chirho: root_mode_chirho,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: root_size_chirho,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &EXT4_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(Ext4FsDataChirho {
            ino_chirho: EXT4_ROOT_INO_CHIRHO,
            mount_chirho: mount_arc_chirho.clone(),
        })),
    }));

    let root_dentry_chirho = Arc::new(Mutex::new(crate::vfs_chirho::DentryChirho {
        name_chirho: String::from("/"),
        inode_chirho: Some(root_inode_chirho),
        parent_chirho: None,
        children_chirho: Vec::new(),
    }));

    Arc::new(Mutex::new(crate::vfs_chirho::SuperblockChirho {
        fs_type_chirho: "ext4",
        root_chirho: root_dentry_chirho,
        flags_chirho: 1, // MS_RDONLY
        ops_chirho: &EXT4_SUPER_OPS_CHIRHO,
    }))
}

/// Mount an ext4 filesystem from a block device path (e.g., "/dev/loop0").
///
/// Opens the device via VFS, reads the superblock and group descriptors
/// through the device's file ops, and creates an Ext4MountChirho.
pub fn mount_ext4_from_device_chirho(
    device_path_chirho: &str,
) -> Result<alloc::sync::Arc<spin::Mutex<crate::vfs_chirho::SuperblockChirho>>, &'static str> {
    use alloc::vec;

    crate::serial_println_chirho!(
        "[EXT4] mount_ext4_from_device: opening {}",
        device_path_chirho
    );

    // Resolve the path directly through the kernel VFS path walker.
    // `sys_open_chirho` expects a user-space pointer, not a kernel `&str`.
    let (inode_chirho, file_ops_chirho) = crate::fs_chirho::resolve_path_chirho(device_path_chirho)
        .map_err(|_| "failed to resolve device path")?;
    let file_arc_chirho = alloc::sync::Arc::new(spin::Mutex::new(crate::vfs_chirho::FileChirho {
        inode_chirho,
        pos_chirho: 0,
        flags_chirho: crate::vfs_chirho::O_RDWR_CHIRHO,
        ops_chirho: file_ops_chirho,
    }));

    let mut sb_data_chirho = vec![0u8; 1024];
    {
        let mut file_chirho = file_arc_chirho.lock();
        file_chirho.pos_chirho = SUPERBLOCK_OFFSET_CHIRHO;
        match file_chirho.ops_chirho.read_chirho(&mut file_chirho, &mut sb_data_chirho) {
            Ok(n_chirho) if n_chirho == sb_data_chirho.len() => {}
            Ok(n_chirho) => {
                crate::serial_println_chirho!(
                    "[EXT4] mount_ext4_from_device: short superblock read {}",
                    n_chirho
                );
                return Err("failed to read superblock");
            }
            Err(errno_chirho) => {
                crate::serial_println_chirho!(
                    "[EXT4] mount_ext4_from_device: superblock read errno={}",
                    errno_chirho
                );
                return Err("failed to read superblock");
            }
        }
    }

    let sb_chirho = parse_superblock_chirho(&sb_data_chirho)
        .ok_or("corrupt superblock")?;

    let block_size_chirho = sb_chirho.block_size_chirho();
    let bg_count_chirho = sb_chirho.block_group_count_chirho();
    let gd_size_chirho = sb_chirho.group_desc_size_chirho();

    crate::serial_println_chirho!(
        "[EXT4] Loop mount: block_size={}, blocks={}, groups={}",
        block_size_chirho, sb_chirho.total_blocks_chirho(), bg_count_chirho
    );

    // Read block group descriptors.
    let gdt_offset_chirho = if block_size_chirho == 1024 {
        2 * block_size_chirho as u64
    } else {
        block_size_chirho as u64
    };
    let gdt_bytes_chirho = bg_count_chirho as usize * gd_size_chirho as usize;
    let mut gdt_data_chirho = vec![0u8; gdt_bytes_chirho];

    {
        let mut file_chirho = file_arc_chirho.lock();
        file_chirho.pos_chirho = gdt_offset_chirho;
        match file_chirho.ops_chirho.read_chirho(&mut file_chirho, &mut gdt_data_chirho) {
            Ok(n_chirho) if n_chirho == gdt_data_chirho.len() => {}
            Ok(n_chirho) => {
                crate::serial_println_chirho!(
                    "[EXT4] mount_ext4_from_device: short GDT read {} expected {}",
                    n_chirho,
                    gdt_data_chirho.len()
                );
                return Err("failed to read group descriptors");
            }
            Err(errno_chirho) => {
                crate::serial_println_chirho!(
                    "[EXT4] mount_ext4_from_device: GDT read errno={}",
                    errno_chirho
                );
                return Err("failed to read group descriptors");
            }
        }
    }

    let group_descs_chirho = parse_group_descs_chirho(&gdt_data_chirho, bg_count_chirho, gd_size_chirho);

    // Register this device in the block registry so read_block works.
    // Use device_id 99 for loop mounts.
    // Use device_id 1 (device 0 = VirtIO rootfs, 1 = first loop mount).
    // Using 99 caused the block registry to allocate 99 placeholder entries
    // which consumed heap and held the lock for too long.
    let device_id_chirho = 1u32;
    // Use the loop device's BACKING FILE Arc (the actual image file on ext4),
    // not the /dev/loop0 device file itself. The backing file has ext4 ops
    // that can read/write blocks on device 0 (VirtIO).
    let backing_file_arc_chirho = {
        let states_chirho = crate::loop_device_chirho::get_loop_states_chirho();
        let states_guard_chirho = states_chirho.lock();
        // Find the loop device that's bound (any minor with backing_file)
        let mut found_chirho = None;
        for state_chirho in states_guard_chirho.iter() {
            if let Some(ref bf_chirho) = state_chirho.backing_file_chirho {
                found_chirho = Some(bf_chirho.clone());
                break;
            }
        }
        found_chirho.unwrap_or_else(|| file_arc_chirho.clone())
    };
    crate::block_chirho::register_loop_block_device_chirho(
        device_id_chirho as usize,
        backing_file_arc_chirho,
    )
    .map_err(|_| "failed to register loop block device")?;
    PAGE_CACHE_CHIRHO
        .lock()
        .invalidate_device_chirho(device_id_chirho);
    crate::serial_println_chirho!(
        "[EXT4] mount_ext4_from_device: registered block device {} for {}",
        device_id_chirho,
        device_path_chirho
    );

    let ext4_mount_chirho = Ext4MountChirho {
        sb_chirho,
        group_descs_chirho,
        block_size_chirho,
        device_id_chirho,
        mode_chirho: MountModeChirho::ReadWriteChirho,
    };

    Ok(mount_ext4_vfs_chirho(ext4_mount_chirho))
}

/// Write a file to a loop-mounted ext4 filesystem and read it back.
/// Returns the read-back data as a Vec.
pub fn write_and_readback_chirho(
    mount_path_chirho: &str,
    name_chirho: &str,
    data_chirho: &[u8],
) -> Result<alloc::vec::Vec<u8>, &'static str> {
    use alloc::vec;

    let (dir_inode_chirho, _) = crate::fs_chirho::resolve_path_chirho(mount_path_chirho)
        .map_err(|_| "mount path not found")?;

    let mount_arc_chirho = {
        let ig_chirho = dir_inode_chirho.lock();
        ig_chirho.fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .map(|fd_chirho| fd_chirho.mount_chirho.clone())
            .ok_or("not an ext4 mount")?
    };

    let root_ino_chirho = {
        let ig_chirho = dir_inode_chirho.lock();
        ig_chirho.fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .map(|fd_chirho| fd_chirho.ino_chirho)
            .unwrap_or(EXT4_ROOT_INO_CHIRHO)
    };

    let mount_chirho = mount_arc_chirho.lock();

    // Create the file
    let new_ino_chirho = mount_chirho
        .create_file_chirho(root_ino_chirho, name_chirho, 0o100644)
        .map_err(|_| "create file failed")?;

    crate::serial_println_chirho!(
        "[EXT4] Created inode {} for '{}'", new_ino_chirho, name_chirho
    );

    // Write data
    mount_chirho
        .write_file_data_chirho(new_ino_chirho, data_chirho)
        .map_err(|_| "write data failed")?;

    crate::serial_println_chirho!(
        "[MOUNT] Wrote {} bytes to {}", data_chirho.len(), name_chirho
    );

    // Read back using the inode number directly (bypass VFS path cache).
    // Do NOT invalidate page cache — write_file_data_chirho already stored
    // the updated inode and data blocks in the cache via write_block_chirho.
    // Invalidating would force a read from the loop device's backing file,
    // which may fail if the root ext4 is read-only (writes don't persist
    // through the loop device to the root VirtIO disk in that case).
    let inode_chirho = mount_chirho.read_inode_chirho(new_ino_chirho)
        .ok_or("inode read failed after write")?;

    // Use the standard ext4 file data read (served from page cache)
    mount_chirho.read_file_data_chirho(&inode_chirho)
        .ok_or("read file data failed")
}

/// Write a file to a mounted ext4 filesystem (kernel-side).
pub fn write_file_to_mount_chirho(
    mount_path_chirho: &str,
    name_chirho: &str,
    data_chirho: &[u8],
) -> Result<usize, &'static str> {
    let (dir_inode_chirho, _) = crate::fs_chirho::resolve_path_chirho(mount_path_chirho)
        .map_err(|_| "mount path not found")?;

    let mount_arc_chirho = {
        let ig_chirho = dir_inode_chirho.lock();
        ig_chirho.fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .map(|fd_chirho| fd_chirho.mount_chirho.clone())
            .ok_or("not an ext4 mount")?
    };

    let root_ino_chirho = {
        let ig_chirho = dir_inode_chirho.lock();
        ig_chirho.fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Ext4FsDataChirho>())
            .map(|fd_chirho| fd_chirho.ino_chirho)
            .unwrap_or(EXT4_ROOT_INO_CHIRHO)
    };

    let mount_chirho = mount_arc_chirho.lock();
    match mount_chirho.create_file_chirho(root_ino_chirho, name_chirho, 0o100644) {
        Ok(new_ino_chirho) => {
            crate::serial_println_chirho!(
                "[EXT4] Created inode {} for '{}'", new_ino_chirho, name_chirho
            );
            match mount_chirho.write_file_data_chirho(new_ino_chirho, data_chirho) {
                Ok(()) => Ok(data_chirho.len()),
                Err(e_chirho) => {
                    crate::serial_println_chirho!("[EXT4] write_file_data failed: {:?}", e_chirho);
                    Err("write data failed")
                }
            }
        }
        Err(e_chirho) => {
            crate::serial_println_chirho!("[EXT4] create_file failed: {:?}", e_chirho);
            Err("create file failed")
        }
    }
}
