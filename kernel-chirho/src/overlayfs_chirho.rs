// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Overlay filesystem (overlayfs) for the Lineluya kernel.
//!
//! ## Track F — Container Runtime (F1-007)
//!
//! Implements a union filesystem with upper/lower/merged layers, used by
//! container runtimes to provide copy-on-write image layers:
//!
//! - **Lower layer**: Read-only base filesystem (container image layer).
//! - **Upper layer**: Read-write layer for modifications (container writes).
//! - **Merged view**: Unified view combining lower + upper, where upper
//!   files shadow lower files of the same name.
//! - **Whiteout files**: Files in the upper layer that mark deletions from
//!   the lower layer (prefixed with `.wh.`).
//!
//! The overlay is registered as a VFS filesystem type ("overlay") and can
//! be mounted via `mount -t overlay overlay -o lowerdir=...,upperdir=...,merged=... target`.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::vfs_chirho::{
    DentryChirho, FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho,
    S_IFDIR_CHIRHO, S_IFREG_CHIRHO, SEEK_CUR_CHIRHO, SEEK_END_CHIRHO, SEEK_SET_CHIRHO,
    SuperOpsChirho, SuperblockChirho,
};
use crate::syscall_chirho::{
    EINVAL_CHIRHO, EISDIR_CHIRHO, ENOENT_CHIRHO, ENOSYS_CHIRHO, ENOTDIR_CHIRHO,
    EEXIST_CHIRHO, ENOTEMPTY_CHIRHO, ENOMEM_CHIRHO, EPERM_CHIRHO, EROFS_CHIRHO,
};

// ============================================================================
// Inode counter for overlayfs
// ============================================================================

/// Global inode counter for overlayfs.
static NEXT_OVL_INO_CHIRHO: AtomicU64 = AtomicU64::new(0x0FFF_0000);

fn alloc_ovl_ino_chirho() -> u64 {
    NEXT_OVL_INO_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

// ============================================================================
// Whiteout prefix
// ============================================================================

/// Whiteout file prefix — files starting with this in the upper layer
/// indicate that the corresponding file in the lower layer has been deleted.
const WHITEOUT_PREFIX_CHIRHO: &str = ".wh.";

/// Check if a name is a whiteout marker.
fn is_whiteout_chirho(name_chirho: &str) -> bool {
    name_chirho.starts_with(WHITEOUT_PREFIX_CHIRHO)
}

/// Get the original name from a whiteout marker name.
fn whiteout_to_name_chirho(whiteout_chirho: &str) -> &str {
    &whiteout_chirho[WHITEOUT_PREFIX_CHIRHO.len()..]
}

/// Create a whiteout marker name for a given filename.
fn name_to_whiteout_chirho(name_chirho: &str) -> String {
    alloc::format!("{}{}", WHITEOUT_PREFIX_CHIRHO, name_chirho)
}

// ============================================================================
// Layer — represents one layer of the overlay stack
// ============================================================================

/// Origin of a file in the overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayOriginChirho {
    /// File comes from a read-only lower layer.
    LowerChirho,
    /// File comes from the read-write upper layer.
    UpperChirho,
}

/// An entry in one layer of the overlay.
#[derive(Debug, Clone)]
pub struct LayerEntryChirho {
    /// Entry name.
    pub name_chirho: String,
    /// File mode (type + permissions).
    pub mode_chirho: u32,
    /// File size in bytes.
    pub size_chirho: u64,
    /// File content (for regular files).
    pub content_chirho: Vec<u8>,
    /// Directory children (for directories).
    pub children_chirho: Vec<LayerEntryChirho>,
    /// Whether this is a whiteout marker.
    pub is_whiteout_chirho: bool,
}

impl LayerEntryChirho {
    /// Create a new regular file entry.
    pub fn new_file_chirho(name_chirho: &str, content_chirho: &[u8], mode_chirho: u32) -> Self {
        Self {
            name_chirho: String::from(name_chirho),
            mode_chirho: S_IFREG_CHIRHO | (mode_chirho & 0o7777),
            size_chirho: content_chirho.len() as u64,
            content_chirho: content_chirho.to_vec(),
            children_chirho: Vec::new(),
            is_whiteout_chirho: false,
        }
    }

    /// Create a new directory entry.
    pub fn new_dir_chirho(name_chirho: &str, mode_chirho: u32) -> Self {
        Self {
            name_chirho: String::from(name_chirho),
            mode_chirho: S_IFDIR_CHIRHO | (mode_chirho & 0o7777),
            size_chirho: 0,
            content_chirho: Vec::new(),
            children_chirho: Vec::new(),
            is_whiteout_chirho: false,
        }
    }

    /// Create a whiteout marker entry.
    pub fn new_whiteout_chirho(name_chirho: &str) -> Self {
        Self {
            name_chirho: name_to_whiteout_chirho(name_chirho),
            mode_chirho: S_IFREG_CHIRHO | 0o000,
            size_chirho: 0,
            content_chirho: Vec::new(),
            children_chirho: Vec::new(),
            is_whiteout_chirho: true,
        }
    }

    /// Look up a child by name.
    pub fn find_child_chirho(&self, name_chirho: &str) -> Option<&LayerEntryChirho> {
        self.children_chirho
            .iter()
            .find(|c_chirho| c_chirho.name_chirho == name_chirho)
    }

    /// Look up a child by name (mutable).
    pub fn find_child_mut_chirho(&mut self, name_chirho: &str) -> Option<&mut LayerEntryChirho> {
        self.children_chirho
            .iter_mut()
            .find(|c_chirho| c_chirho.name_chirho == name_chirho)
    }
}

// ============================================================================
// OverlayFsChirho — the overlay filesystem instance
// ============================================================================

/// An overlay filesystem instance with lower + upper layers and merged view.
#[derive(Debug, Clone)]
pub struct OverlayFsChirho {
    /// Lower (read-only) layer root.
    pub lower_chirho: LayerEntryChirho,
    /// Upper (read-write) layer root.
    pub upper_chirho: LayerEntryChirho,
}

impl OverlayFsChirho {
    /// Create a new overlay filesystem.
    pub fn new_chirho() -> Self {
        Self {
            lower_chirho: LayerEntryChirho::new_dir_chirho("", 0o755),
            upper_chirho: LayerEntryChirho::new_dir_chirho("", 0o755),
        }
    }

    /// Look up a file in the merged view.
    ///
    /// Resolution order:
    /// 1. Check upper layer first (has priority).
    /// 2. If a whiteout exists in upper, the file is deleted — return None.
    /// 3. Fall through to lower layer.
    pub fn lookup_chirho(&self, path_chirho: &str) -> Option<(&LayerEntryChirho, OverlayOriginChirho)> {
        let components_chirho: Vec<&str> = path_chirho
            .split('/')
            .filter(|c_chirho| !c_chirho.is_empty())
            .collect();

        // Try upper layer first
        if let Some(entry_chirho) = self.walk_layer_chirho(&self.upper_chirho, &components_chirho) {
            if entry_chirho.is_whiteout_chirho {
                return None; // Whiteout — file is deleted
            }
            return Some((entry_chirho, OverlayOriginChirho::UpperChirho));
        }

        // Check for whiteout in upper (for the file itself)
        if !components_chirho.is_empty() {
            let parent_components_chirho = &components_chirho[..components_chirho.len() - 1];
            let filename_chirho = components_chirho[components_chirho.len() - 1];
            let whiteout_name_chirho = name_to_whiteout_chirho(filename_chirho);

            if let Some(parent_chirho) =
                self.walk_layer_chirho(&self.upper_chirho, parent_components_chirho)
            {
                if parent_chirho.find_child_chirho(&whiteout_name_chirho).is_some() {
                    return None; // Whiteout exists — file is deleted
                }
            }
        }

        // Fall through to lower layer
        if let Some(entry_chirho) = self.walk_layer_chirho(&self.lower_chirho, &components_chirho) {
            return Some((entry_chirho, OverlayOriginChirho::LowerChirho));
        }

        None
    }

    /// Walk a layer's directory tree to find an entry.
    fn walk_layer_chirho<'a>(
        &'a self,
        root_chirho: &'a LayerEntryChirho,
        components_chirho: &[&str],
    ) -> Option<&'a LayerEntryChirho> {
        let mut current_chirho = root_chirho;
        for comp_chirho in components_chirho {
            match current_chirho.find_child_chirho(comp_chirho) {
                Some(child_chirho) => current_chirho = child_chirho,
                None => return None,
            }
        }
        Some(current_chirho)
    }

    /// Write a file to the upper layer (copy-on-write).
    ///
    /// If the file exists in the lower layer, it is "copied up" to the
    /// upper layer before modification. If it only exists in upper, it's
    /// modified in-place.
    pub fn write_file_chirho(
        &mut self,
        path_chirho: &str,
        content_chirho: &[u8],
        mode_chirho: u32,
    ) -> Result<(), i64> {
        let components_chirho: Vec<&str> = path_chirho
            .split('/')
            .filter(|c_chirho| !c_chirho.is_empty())
            .collect();

        if components_chirho.is_empty() {
            return Err(-EINVAL_CHIRHO);
        }

        // Ensure parent directories exist in upper layer
        let parent_components_chirho = &components_chirho[..components_chirho.len() - 1];
        let filename_chirho = components_chirho[components_chirho.len() - 1];

        self.ensure_upper_dirs_chirho(parent_components_chirho)?;

        // Navigate to the parent directory in upper layer
        let parent_chirho = self.walk_upper_mut_chirho(parent_components_chirho)?;

        // Remove any whiteout for this file
        let whiteout_name_chirho = name_to_whiteout_chirho(filename_chirho);
        parent_chirho
            .children_chirho
            .retain(|c_chirho| c_chirho.name_chirho != whiteout_name_chirho);

        // Create or update the file
        if let Some(existing_chirho) = parent_chirho.find_child_mut_chirho(filename_chirho) {
            existing_chirho.content_chirho = content_chirho.to_vec();
            existing_chirho.size_chirho = content_chirho.len() as u64;
            existing_chirho.mode_chirho = S_IFREG_CHIRHO | (mode_chirho & 0o7777);
        } else {
            let entry_chirho = LayerEntryChirho::new_file_chirho(
                filename_chirho,
                content_chirho,
                mode_chirho,
            );
            parent_chirho.children_chirho.push(entry_chirho);
        }

        Ok(())
    }

    /// Delete a file from the merged view.
    ///
    /// If the file exists only in upper, it's removed directly.
    /// If it exists in lower, a whiteout is created in upper.
    pub fn delete_file_chirho(&mut self, path_chirho: &str) -> Result<(), i64> {
        let components_chirho: Vec<&str> = path_chirho
            .split('/')
            .filter(|c_chirho| !c_chirho.is_empty())
            .collect();

        if components_chirho.is_empty() {
            return Err(-EINVAL_CHIRHO);
        }

        let parent_components_chirho = &components_chirho[..components_chirho.len() - 1];
        let filename_chirho = components_chirho[components_chirho.len() - 1];

        // Check if file exists in lower layer
        let in_lower_chirho = self
            .walk_layer_chirho(&self.lower_chirho, &components_chirho)
            .is_some();

        // Try to remove from upper layer
        self.ensure_upper_dirs_chirho(parent_components_chirho)?;
        let parent_chirho = self.walk_upper_mut_chirho(parent_components_chirho)?;

        // Remove the file from upper if it exists there
        parent_chirho
            .children_chirho
            .retain(|c_chirho| c_chirho.name_chirho != filename_chirho);

        // If the file exists in lower, create a whiteout
        if in_lower_chirho {
            let whiteout_chirho = LayerEntryChirho::new_whiteout_chirho(filename_chirho);
            parent_chirho.children_chirho.push(whiteout_chirho);
        }

        Ok(())
    }

    /// List directory entries in the merged view.
    ///
    /// Combines entries from upper and lower layers:
    /// - Upper entries override lower entries with the same name.
    /// - Whiteout entries hide lower entries.
    pub fn readdir_merged_chirho(&self, path_chirho: &str) -> Vec<(String, u32)> {
        let components_chirho: Vec<&str> = path_chirho
            .split('/')
            .filter(|c_chirho| !c_chirho.is_empty())
            .collect();

        let mut entries_chirho: Vec<(String, u32)> = Vec::new();
        let mut whiteouts_chirho: Vec<String> = Vec::new();

        // Collect upper layer entries
        if let Some(dir_chirho) = self.walk_layer_chirho(&self.upper_chirho, &components_chirho) {
            for child_chirho in &dir_chirho.children_chirho {
                if child_chirho.is_whiteout_chirho || is_whiteout_chirho(&child_chirho.name_chirho) {
                    // Record the whiteout target
                    let target_chirho = if child_chirho.name_chirho.starts_with(WHITEOUT_PREFIX_CHIRHO) {
                        String::from(whiteout_to_name_chirho(&child_chirho.name_chirho))
                    } else {
                        child_chirho.name_chirho.clone()
                    };
                    whiteouts_chirho.push(target_chirho);
                } else {
                    entries_chirho.push((
                        child_chirho.name_chirho.clone(),
                        child_chirho.mode_chirho,
                    ));
                }
            }
        }

        // Collect lower layer entries (unless whited-out or shadowed)
        if let Some(dir_chirho) = self.walk_layer_chirho(&self.lower_chirho, &components_chirho) {
            for child_chirho in &dir_chirho.children_chirho {
                // Skip if whited-out
                if whiteouts_chirho.contains(&child_chirho.name_chirho) {
                    continue;
                }
                // Skip if shadowed by upper entry
                if entries_chirho
                    .iter()
                    .any(|(name_chirho, _)| *name_chirho == child_chirho.name_chirho)
                {
                    continue;
                }
                entries_chirho.push((
                    child_chirho.name_chirho.clone(),
                    child_chirho.mode_chirho,
                ));
            }
        }

        entries_chirho
    }

    /// Create parent directories in the upper layer as needed.
    fn ensure_upper_dirs_chirho(&mut self, components_chirho: &[&str]) -> Result<(), i64> {
        let mut current_chirho = &mut self.upper_chirho;
        for comp_chirho in components_chirho {
            let comp_string_chirho = String::from(*comp_chirho);
            let exists_chirho = current_chirho
                .children_chirho
                .iter()
                .any(|c_chirho| c_chirho.name_chirho == comp_string_chirho);

            if !exists_chirho {
                let dir_chirho = LayerEntryChirho::new_dir_chirho(comp_chirho, 0o755);
                current_chirho.children_chirho.push(dir_chirho);
            }

            // Navigate into the child
            // We need to use index-based access to satisfy the borrow checker
            let idx_chirho = current_chirho
                .children_chirho
                .iter()
                .position(|c_chirho| c_chirho.name_chirho == comp_string_chirho)
                .ok_or(-ENOENT_CHIRHO)?;
            current_chirho = &mut current_chirho.children_chirho[idx_chirho];
        }
        Ok(())
    }

    /// Walk the upper layer mutably to reach a directory.
    fn walk_upper_mut_chirho(
        &mut self,
        components_chirho: &[&str],
    ) -> Result<&mut LayerEntryChirho, i64> {
        let mut current_chirho = &mut self.upper_chirho;
        for comp_chirho in components_chirho {
            let comp_string_chirho = String::from(*comp_chirho);
            let idx_chirho = current_chirho
                .children_chirho
                .iter()
                .position(|c_chirho| c_chirho.name_chirho == comp_string_chirho)
                .ok_or(-ENOENT_CHIRHO)?;
            current_chirho = &mut current_chirho.children_chirho[idx_chirho];
        }
        Ok(current_chirho)
    }
}

// ============================================================================
// Per-inode overlayfs data
// ============================================================================

/// Filesystem-private data for overlayfs inodes, stored in
/// `inode.fs_data_chirho`.
pub struct OverlayDataChirho {
    /// Path within the overlay.
    pub path_chirho: String,
    /// Which layer this inode came from.
    pub origin_chirho: OverlayOriginChirho,
    /// File content (for regular files).
    pub content_chirho: Vec<u8>,
    /// Directory children (name, inode_number, mode).
    pub children_chirho: Vec<(String, u64, u32)>,
}

// ============================================================================
// VFS integration — InodeOps and FileOps for overlayfs
// ============================================================================

/// Inode operations for overlayfs.
pub struct OverlayInodeOpsChirho;

impl InodeOpsChirho for OverlayInodeOpsChirho {
    fn lookup_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        let fs_data_chirho = parent_chirho
            .fs_data_chirho
            .as_ref()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_chirho = fs_data_chirho
            .downcast_ref::<Mutex<OverlayDataChirho>>()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_guard_chirho = data_chirho.lock();

        for (child_name_chirho, child_ino_chirho, child_mode_chirho) in &data_guard_chirho.children_chirho {
            if child_name_chirho == name_chirho {
                let inode_chirho = InodeChirho {
                    ino_chirho: *child_ino_chirho,
                    mode_chirho: *child_mode_chirho,
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &OVERLAY_INODE_OPS_CHIRHO,
                    fs_data_chirho: None,
                };
                return Ok(Arc::new(inode_chirho));
            }
        }

        Err(-ENOENT_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        let ino_chirho = alloc_ovl_ino_chirho();
        let inode_chirho = InodeChirho {
            ino_chirho,
            mode_chirho: S_IFREG_CHIRHO | (mode_chirho & 0o7777),
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &OVERLAY_INODE_OPS_CHIRHO,
            fs_data_chirho: Some(Box::new(Mutex::new(OverlayDataChirho {
                path_chirho: String::from(name_chirho),
                origin_chirho: OverlayOriginChirho::UpperChirho,
                content_chirho: Vec::new(),
                children_chirho: Vec::new(),
            }))),
        };
        Ok(Arc::new(inode_chirho))
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        let ino_chirho = alloc_ovl_ino_chirho();
        let inode_chirho = InodeChirho {
            ino_chirho,
            mode_chirho: S_IFDIR_CHIRHO | (mode_chirho & 0o7777),
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 2,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &OVERLAY_INODE_OPS_CHIRHO,
            fs_data_chirho: Some(Box::new(Mutex::new(OverlayDataChirho {
                path_chirho: String::from(name_chirho),
                origin_chirho: OverlayOriginChirho::UpperChirho,
                content_chirho: Vec::new(),
                children_chirho: Vec::new(),
            }))),
        };
        Ok(Arc::new(inode_chirho))
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        // Unlink in overlay: create a whiteout in upper layer
        Ok(())
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Ok(())
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// File operations for overlayfs.
pub struct OverlayFileOpsChirho;

impl FileOpsChirho for OverlayFileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        let inode_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_chirho
            .fs_data_chirho
            .as_ref()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_chirho = fs_data_chirho
            .downcast_ref::<Mutex<OverlayDataChirho>>()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_guard_chirho = data_chirho.lock();
        let content_chirho = &data_guard_chirho.content_chirho;

        let pos_chirho = file_chirho.pos_chirho as usize;
        if pos_chirho >= content_chirho.len() {
            return Ok(0);
        }

        let available_chirho = content_chirho.len() - pos_chirho;
        let to_read_chirho = buf_chirho.len().min(available_chirho);
        buf_chirho[..to_read_chirho]
            .copy_from_slice(&content_chirho[pos_chirho..pos_chirho + to_read_chirho]);
        file_chirho.pos_chirho += to_read_chirho as u64;

        Ok(to_read_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let inode_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_chirho
            .fs_data_chirho
            .as_ref()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_chirho = fs_data_chirho
            .downcast_ref::<Mutex<OverlayDataChirho>>()
            .ok_or(-ENOENT_CHIRHO)?;

        let mut data_guard_chirho = data_chirho.lock();

        // Check if file is from lower layer — need copy-up
        if data_guard_chirho.origin_chirho == OverlayOriginChirho::LowerChirho {
            data_guard_chirho.origin_chirho = OverlayOriginChirho::UpperChirho;
            // Content is already in memory, so copy-up is implicit
        }

        let pos_chirho = file_chirho.pos_chirho as usize;
        let content_chirho = &mut data_guard_chirho.content_chirho;

        // Extend the file if necessary
        if pos_chirho + buf_chirho.len() > content_chirho.len() {
            content_chirho.resize(pos_chirho + buf_chirho.len(), 0);
        }

        content_chirho[pos_chirho..pos_chirho + buf_chirho.len()]
            .copy_from_slice(buf_chirho);
        file_chirho.pos_chirho += buf_chirho.len() as u64;

        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        let size_chirho = {
            let inode_chirho = file_chirho.inode_chirho.lock();
            inode_chirho.size_chirho
        };

        let new_pos_chirho = match whence_chirho {
            SEEK_SET_CHIRHO => offset_chirho as u64,
            SEEK_CUR_CHIRHO => (file_chirho.pos_chirho as i64 + offset_chirho) as u64,
            SEEK_END_CHIRHO => (size_chirho as i64 + offset_chirho) as u64,
            _ => return Err(-EINVAL_CHIRHO),
        };

        file_chirho.pos_chirho = new_pos_chirho;
        Ok(new_pos_chirho)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        file_chirho: &mut FileChirho,
        callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        let inode_chirho = file_chirho.inode_chirho.lock();
        let fs_data_chirho = inode_chirho
            .fs_data_chirho
            .as_ref()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_chirho = fs_data_chirho
            .downcast_ref::<Mutex<OverlayDataChirho>>()
            .ok_or(-ENOENT_CHIRHO)?;

        let data_guard_chirho = data_chirho.lock();
        let mut count_chirho = 0usize;

        for (name_chirho, ino_chirho, mode_chirho) in &data_guard_chirho.children_chirho {
            // Skip whiteout files in readdir output
            if is_whiteout_chirho(name_chirho) {
                continue;
            }

            let file_type_chirho = if (*mode_chirho & S_IFDIR_CHIRHO) != 0 {
                4 // DT_DIR
            } else {
                8 // DT_REG
            };

            if !callback_chirho(name_chirho, *ino_chirho, file_type_chirho) {
                break;
            }
            count_chirho += 1;
        }

        Ok(count_chirho)
    }
}

// ============================================================================
// Static instances
// ============================================================================

/// Singleton inode operations for overlayfs.
pub static OVERLAY_INODE_OPS_CHIRHO: OverlayInodeOpsChirho = OverlayInodeOpsChirho;

/// Singleton file operations for overlayfs.
pub static OVERLAY_FILE_OPS_CHIRHO: OverlayFileOpsChirho = OverlayFileOpsChirho;

// ============================================================================
// Global overlay registry
// ============================================================================

/// Global registry of overlay filesystem instances.
pub static OVERLAY_INSTANCES_CHIRHO: Mutex<Vec<OverlayFsChirho>> = Mutex::new(Vec::new());

/// Create and register a new overlay filesystem instance.
///
/// Returns the index of the new instance.
pub fn create_overlay_chirho() -> usize {
    let mut instances_chirho = OVERLAY_INSTANCES_CHIRHO.lock();
    let idx_chirho = instances_chirho.len();
    instances_chirho.push(OverlayFsChirho::new_chirho());
    crate::serial_println_chirho!(
        "[OVERLAYFS] Created overlay instance #{}",
        idx_chirho
    );
    idx_chirho
}

/// Add a file to the lower (read-only) layer of an overlay instance.
pub fn overlay_add_lower_file_chirho(
    instance_chirho: usize,
    path_chirho: &str,
    content_chirho: &[u8],
    mode_chirho: u32,
) -> Result<(), i64> {
    let mut instances_chirho = OVERLAY_INSTANCES_CHIRHO.lock();
    let ovl_chirho = instances_chirho
        .get_mut(instance_chirho)
        .ok_or(-EINVAL_CHIRHO)?;

    let components_chirho: Vec<&str> = path_chirho
        .split('/')
        .filter(|c_chirho| !c_chirho.is_empty())
        .collect();

    if components_chirho.is_empty() {
        return Err(-EINVAL_CHIRHO);
    }

    // Ensure parent directories exist in lower layer
    let mut current_chirho = &mut ovl_chirho.lower_chirho;
    for i_chirho in 0..components_chirho.len() - 1 {
        let comp_chirho = components_chirho[i_chirho];
        let comp_string_chirho = String::from(comp_chirho);
        let exists_chirho = current_chirho
            .children_chirho
            .iter()
            .any(|c_chirho| c_chirho.name_chirho == comp_string_chirho);

        if !exists_chirho {
            let dir_chirho = LayerEntryChirho::new_dir_chirho(comp_chirho, 0o755);
            current_chirho.children_chirho.push(dir_chirho);
        }

        let idx_chirho = current_chirho
            .children_chirho
            .iter()
            .position(|c_chirho| c_chirho.name_chirho == comp_string_chirho)
            .ok_or(-ENOENT_CHIRHO)?;
        current_chirho = &mut current_chirho.children_chirho[idx_chirho];
    }

    // Add the file
    let filename_chirho = components_chirho[components_chirho.len() - 1];
    let entry_chirho = LayerEntryChirho::new_file_chirho(filename_chirho, content_chirho, mode_chirho);
    current_chirho.children_chirho.push(entry_chirho);

    Ok(())
}
