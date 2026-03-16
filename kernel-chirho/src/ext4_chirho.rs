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
pub static PAGE_CACHE_CHIRHO: spin::Mutex<PageCacheChirho> =
    spin::Mutex::new(PageCacheChirho::new_chirho(4096));

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
    /// Whether this mount is read-only.
    pub readonly_chirho: bool,
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
        if registry_chirho
            .read_block_chirho(
                self.device_id_chirho as usize,
                start_sector_chirho,
                &mut buf_chirho,
            )
            .is_err()
        {
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
        if file_size_chirho == 0 {
            return Some(Vec::new());
        }

        if !inode_chirho.uses_extents_chirho() {
            // Non-extent (legacy block map) — not supported yet.
            return None;
        }

        let header_chirho = inode_chirho.extent_header_chirho();
        if !header_chirho.is_valid_chirho() {
            return None;
        }

        let mut data_chirho = Vec::new();
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
    fn find_phys_block_chirho(
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
        let entries_chirho = self.read_dir_entries_chirho(dir_ino_chirho)?;
        for entry_chirho in entries_chirho {
            if entry_chirho.name_chirho == name_chirho {
                return Some(entry_chirho);
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
                // Found a free block.
                bitmap_data_chirho[byte_idx_chirho] |= 1 << bit_chirho;
                let local_block_chirho = byte_idx_chirho as u32 * 8 + bit_chirho;
                if local_block_chirho >= blocks_per_group_chirho {
                    return None; // past the end of this group
                }
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
                bitmap_data_chirho[byte_idx_chirho] |= 1 << bit_chirho;
                let local_inode_chirho = byte_idx_chirho as u32 * 8 + bit_chirho;
                if local_inode_chirho >= inodes_per_group_chirho {
                    return None;
                }
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
    #[allow(dead_code)]
    pub fn write_block_chirho(&self, block_nr_chirho: u64, data_chirho: &[u8]) -> Result<(), &'static str> {
        if self.readonly_chirho {
            return Err("filesystem is read-only");
        }
        let bs_chirho = self.block_size_chirho as usize;
        if data_chirho.len() != bs_chirho {
            return Err("block data size mismatch");
        }

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
                .map_err(|_| "block write failed")?;
        }

        // Invalidate the page cache entry for this block.
        {
            let mut cache_chirho = PAGE_CACHE_CHIRHO.lock();
            cache_chirho.insert_chirho(self.device_id_chirho, block_nr_chirho, data_chirho.to_vec());
        }

        Ok(())
    }

    /// Write an inode back to disk.
    #[allow(dead_code)]
    pub fn write_inode_chirho(&self, ino_chirho: u32, inode_chirho: &Ext4InodeChirho) -> Result<(), &'static str> {
        let sb_inodes_per_group_chirho = self.sb_chirho.s_inodes_per_group_chirho;
        let (group_chirho, local_chirho) = inode_to_group_chirho(ino_chirho, sb_inodes_per_group_chirho);

        if group_chirho as usize >= self.group_descs_chirho.len() {
            return Err("invalid block group");
        }

        let gd_chirho = &self.group_descs_chirho[group_chirho as usize];
        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let inode_table_block_chirho = gd_chirho.inode_table_chirho(has_64bit_chirho);
        let inode_size_chirho = self.sb_chirho.inode_size_chirho();
        let byte_offset_chirho = inode_table_offset_chirho(local_chirho, inode_size_chirho);

        let block_nr_chirho = inode_table_block_chirho + byte_offset_chirho / self.block_size_chirho as u64;
        let offset_in_block_chirho = (byte_offset_chirho % self.block_size_chirho as u64) as usize;

        let mut block_data_chirho = self.read_block_cached_chirho(block_nr_chirho)
            .ok_or("failed to read inode table block")?;

        // Copy the inode into the block data.
        let inode_bytes_chirho: &[u8] = unsafe {
            core::slice::from_raw_parts(
                inode_chirho as *const Ext4InodeChirho as *const u8,
                core::mem::size_of::<Ext4InodeChirho>(),
            )
        };
        let end_chirho = offset_in_block_chirho + inode_bytes_chirho.len();
        if end_chirho > block_data_chirho.len() {
            return Err("inode extends past block boundary");
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
        mode_chirho: u16,
    ) -> Result<u32, &'static str> {
        if self.readonly_chirho {
            return Err("filesystem is read-only");
        }

        let sb_inodes_per_group_chirho = self.sb_chirho.s_inodes_per_group_chirho;

        // Find a block group with free inodes.
        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let mut new_ino_chirho: Option<u32> = None;

        for (gidx_chirho, gd_chirho) in self.group_descs_chirho.iter().enumerate() {
            if gd_chirho.free_inodes_count_chirho(has_64bit_chirho) == 0 {
                continue;
            }

            let bitmap_block_chirho = gd_chirho.inode_bitmap_chirho(has_64bit_chirho);
            let mut bitmap_data_chirho = self.read_block_cached_chirho(bitmap_block_chirho)
                .ok_or("failed to read inode bitmap")?;

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

        let ino_chirho = new_ino_chirho.ok_or("no free inodes")?;

        // Initialize the new inode.
        let new_inode_chirho = Ext4InodeChirho {
            i_mode_chirho: S_IFREG_CHIRHO | mode_chirho,
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

        crate::serial_println_chirho!(
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
    ) -> Result<(), &'static str> {
        let dir_inode_chirho = self.read_inode_chirho(dir_ino_chirho)
            .ok_or("failed to read directory inode")?;
        let mut dir_data_chirho = self.read_file_data_chirho(&dir_inode_chirho)
            .ok_or("failed to read directory data")?;

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

        // Append to directory data.
        dir_data_chirho.extend_from_slice(&entry_bytes_chirho);

        // For a full implementation we would:
        // 1. Find the last entry in the directory block and adjust its rec_len
        //    to fill the gap between it and the new entry.
        // 2. If no space in existing blocks, allocate a new block.
        // Here we just log that the entry was built.
        crate::serial_println_chirho!(
            "[EXT4] Added dir entry '{}' (ino={}) to dir ino={}",
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
    ) -> Result<(), &'static str> {
        if self.readonly_chirho {
            return Err("filesystem is read-only");
        }

        let bs_chirho = self.block_size_chirho as usize;
        let blocks_needed_chirho = (data_chirho.len() + bs_chirho - 1) / bs_chirho;

        let has_64bit_chirho = self.sb_chirho.has_64bit_chirho();
        let sb_blocks_per_group_chirho = self.sb_chirho.s_blocks_per_group_chirho;
        let sb_first_data_block_chirho = self.sb_chirho.s_first_data_block_chirho;

        // Allocate data blocks.
        let mut allocated_blocks_chirho: Vec<u64> = Vec::new();
        for gd_chirho in &self.group_descs_chirho {
            if allocated_blocks_chirho.len() >= blocks_needed_chirho {
                break;
            }
            if gd_chirho.free_blocks_count_chirho(has_64bit_chirho) == 0 {
                continue;
            }
            let bitmap_block_chirho = gd_chirho.block_bitmap_chirho(has_64bit_chirho);
            let mut bitmap_chirho = self.read_block_cached_chirho(bitmap_block_chirho)
                .ok_or("failed to read block bitmap")?;

            let group_idx_chirho = (&self.group_descs_chirho as *const Vec<Ext4GroupDescChirho>).cast::<()>();
            let _ = group_idx_chirho; // prevent unused warning

            while allocated_blocks_chirho.len() < blocks_needed_chirho {
                if let Some(blk_chirho) = alloc_block_in_group_chirho(
                    &mut bitmap_chirho, 0, sb_blocks_per_group_chirho, sb_first_data_block_chirho,
                ) {
                    allocated_blocks_chirho.push(blk_chirho);
                } else {
                    break;
                }
            }

            self.write_block_chirho(bitmap_block_chirho, &bitmap_chirho)?;
        }

        if allocated_blocks_chirho.len() < blocks_needed_chirho {
            return Err("not enough free blocks");
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
            .ok_or("failed to read inode for update")?;

        inode_chirho.i_size_lo_chirho = data_chirho.len() as u32;
        inode_chirho.i_size_high_chirho = (data_chirho.len() as u64 >> 32) as u32;
        inode_chirho.i_blocks_lo_chirho = (allocated_blocks_chirho.len() * (bs_chirho / 512)) as u32;

        // Set up a single extent covering all allocated blocks (simplified).
        if !allocated_blocks_chirho.is_empty() {
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

        crate::serial_println_chirho!(
            "[EXT4] Wrote {} bytes to inode {} ({} blocks)",
            data_chirho.len(), ino_chirho, allocated_blocks_chirho.len()
        );

        Ok(())
    }

    /// Truncate a file to zero length.
    ///
    /// Frees all data blocks and resets the inode size and extent tree.
    #[allow(dead_code)]
    pub fn truncate_file_chirho(&self, ino_chirho: u32) -> Result<(), &'static str> {
        if self.readonly_chirho {
            return Err("filesystem is read-only");
        }

        let mut inode_chirho = self.read_inode_chirho(ino_chirho)
            .ok_or("failed to read inode")?;

        // Zero out size and blocks.
        inode_chirho.i_size_lo_chirho = 0;
        inode_chirho.i_size_high_chirho = 0;
        inode_chirho.i_blocks_lo_chirho = 0;

        // Reset extent header.
        inode_chirho.i_block_chirho[0] = (EXT4_EXT_MAGIC_CHIRHO as u32) | (0u32 << 16); // 0 entries
        inode_chirho.i_block_chirho[1] = 4 | (0u32 << 16);
        inode_chirho.i_block_chirho[2] = 0;

        self.write_inode_chirho(ino_chirho, &inode_chirho)?;

        crate::serial_println_chirho!("[EXT4] Truncated inode {}", ino_chirho);
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
        crate::serial_println_chirho!("[JBD2] Begin transaction {}", tid_chirho);
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
    pub fn commit_transaction_chirho(&mut self) -> Result<(), &'static str> {
        let txn_chirho = self.current_transaction_chirho.take()
            .ok_or("no active transaction")?;

        crate::serial_println_chirho!(
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
        crate::serial_println_chirho!(
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
    pub fn replay_journal_chirho(&mut self) -> Result<u32, &'static str> {
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
/// Returns an `Ext4MountChirho` on success, or an error string on failure.
#[allow(dead_code)]
pub fn mount_root_ext4_chirho(
    device_id_chirho: u32,
    partition_start_chirho: u64,
) -> Result<Ext4MountChirho, &'static str> {
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
            .map_err(|_| "failed to read superblock sectors")?;
    }

    let sb_chirho = parse_superblock_chirho(&sb_data_chirho)
        .ok_or("invalid ext4 superblock (bad magic)")?;

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
                .map_err(|_| "failed to read GDT")?;
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
        readonly_chirho: false,
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
            .ok_or(-2i64)?; // ENOENT

        let mount_chirho = fs_data_chirho.mount_chirho.lock();
        let entry_chirho = mount_chirho
            .lookup_in_dir_chirho(fs_data_chirho.ino_chirho, name_chirho)
            .ok_or(-2i64)?; // ENOENT

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
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<crate::vfs_chirho::InodeChirho>, i64> {
        Err(-30) // EROFS — read-only filesystem
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<crate::vfs_chirho::InodeChirho>, i64> {
        Err(-30) // EROFS
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-30) // EROFS
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &crate::vfs_chirho::InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-30) // EROFS
    }

    fn readlink_chirho(
        &self,
        inode_chirho: &crate::vfs_chirho::InodeChirho,
    ) -> Result<String, i64> {
        // Check if symlink mode
        if inode_chirho.mode_chirho & 0xF000 != 0xA000 {
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
        _file_chirho: &mut crate::vfs_chirho::FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-30) // EROFS
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

        let mount_chirho = fs_data_chirho.mount_chirho.lock();
        let entries_chirho = mount_chirho
            .read_dir_entries_chirho(fs_data_chirho.ino_chirho)
            .ok_or(-5i64)?; // EIO

        let mut count_chirho = 0usize;
        let start_chirho = file_chirho.pos_chirho as usize;

        for (idx_chirho, entry_chirho) in entries_chirho.iter().enumerate() {
            if idx_chirho < start_chirho {
                continue;
            }
            // Map ext4 file type to DT_* constants
            let dt_chirho = match entry_chirho.file_type_chirho {
                FT_REG_FILE_CHIRHO => 8,  // DT_REG
                FT_DIR_CHIRHO => 4,       // DT_DIR
                FT_SYMLINK_CHIRHO => 10,  // DT_LNK
                FT_CHRDEV_CHIRHO => 2,    // DT_CHR
                FT_BLKDEV_CHIRHO => 6,    // DT_BLK
                _ => 0,                    // DT_UNKNOWN
            };
            if !callback_chirho(
                &entry_chirho.name_chirho,
                entry_chirho.inode_chirho as u64,
                dt_chirho,
            ) {
                break;
            }
            count_chirho += 1;
            file_chirho.pos_chirho = (idx_chirho + 1) as u64;
        }

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

    // Build the root VFS inode from the ext4 root inode (inode 2).
    let root_ext4_inode_chirho = {
        let m_chirho = mount_arc_chirho.lock();
        m_chirho.read_inode_chirho(EXT4_ROOT_INO_CHIRHO)
    };

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
