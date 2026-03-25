// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![allow(invalid_reference_casting)]
//! tmpfs — RAM-backed filesystem for Lineluya.
//!
//! This is the in-memory filesystem equivalent to Linux's `mm/shmem.c`.
//! Files are backed by `Vec<u8>` buffers; directories hold a list of
//! `(name, inode)` pairs.  Everything lives in kernel heap memory and
//! is lost on reboot.
//!
//! Interior mutability is achieved by wrapping `TmpfsDataChirho` in a
//! `spin::Mutex`, stored inside `inode.fs_data_chirho`.  This avoids
//! the need for `&mut InodeChirho` in `InodeOpsChirho` methods.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::vfs_chirho::{
    DentryChirho, FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho,
    S_IFDIR_CHIRHO, S_IFLNK_CHIRHO, S_IFREG_CHIRHO, SEEK_CUR_CHIRHO, SEEK_END_CHIRHO, SEEK_SET_CHIRHO,
    StatfsChirho, SuperOpsChirho, SuperblockChirho,
};
use crate::syscall_chirho::{
    EINVAL_CHIRHO, EISDIR_CHIRHO, ENOENT_CHIRHO, ENOSYS_CHIRHO, ENOTDIR_CHIRHO,
    EEXIST_CHIRHO, ENOTEMPTY_CHIRHO,
};

// ---------------------------------------------------------------------------
// Inode counter
// ---------------------------------------------------------------------------

/// Global inode counter shared across all tmpfs instances.
static NEXT_INO_CHIRHO: AtomicU64 = AtomicU64::new(0x10_0000);

fn alloc_ino_chirho() -> u64 {
    NEXT_INO_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// TmpfsDataChirho — per-inode filesystem-private data
// ---------------------------------------------------------------------------

/// Filesystem-private data stored in `inode.fs_data_chirho` for tmpfs inodes.
///
/// Wrapped in `Mutex<TmpfsDataChirho>` when placed inside the inode so that
/// `InodeOpsChirho` methods (which receive `&InodeChirho`) can mutate it.
pub enum TmpfsDataChirho {
    /// Regular file: content buffer.
    FileChirho(Vec<u8>),
    /// Directory: list of `(name, child_inode)` pairs.
    DirChirho(Vec<(String, Arc<Mutex<InodeChirho>>)>),
    /// Symbolic link: target path string.
    SymlinkChirho(String),
}

/// Helper: downcast `fs_data_chirho` to `&Mutex<TmpfsDataChirho>`.
fn get_tmpfs_data_chirho(inode_chirho: &InodeChirho) -> Result<&Mutex<TmpfsDataChirho>, i64> {
    inode_chirho
        .fs_data_chirho
        .as_ref()
        .and_then(|d_chirho| d_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>())
        .ok_or(-ENOTDIR_CHIRHO)
}

/// Convenience: create a new `Box<Mutex<TmpfsDataChirho>>` suitable for
/// storing in `fs_data_chirho`.
fn new_tmpfs_fs_data_chirho(data_chirho: TmpfsDataChirho) -> Option<Box<dyn core::any::Any + Send>> {
    Some(Box::new(Mutex::new(data_chirho)))
}

// ---------------------------------------------------------------------------
// TmpfsInodeOpsChirho
// ---------------------------------------------------------------------------

/// Inode operations for tmpfs directories (lookup, create, mkdir, etc.).
pub struct TmpfsInodeOpsChirho;

impl InodeOpsChirho for TmpfsInodeOpsChirho {
    fn lookup_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let data_chirho = data_lock_chirho.lock();

        match &*data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                for (entry_name_chirho, inode_chirho) in entries_chirho {
                    if entry_name_chirho == name_chirho {
                        let locked_chirho = inode_chirho.lock();
                        // Take the fs_data out temporarily, wrap in new Arc.
                        // We can't clone Box<dyn Any>, so we re-wrap the
                        // same data pointer via a shared Arc<Mutex<TmpfsData>>.
                        // For directories, we construct a new fs_data pointing
                        // to the same underlying TmpfsData.
                        let fs_data_new_chirho: Option<Box<dyn core::any::Any + Send>> =
                            if let Some(ref data_chirho) = locked_chirho.fs_data_chirho {
                                if let Some(tmpfs_mutex_chirho) = data_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>() {
                                    // Re-read the data and create a new copy
                                    let inner_chirho = tmpfs_mutex_chirho.lock();
                                    let cloned_data_chirho = match &*inner_chirho {
                                        TmpfsDataChirho::DirChirho(entries_inner_chirho) => {
                                            TmpfsDataChirho::DirChirho(entries_inner_chirho.clone())
                                        }
                                        TmpfsDataChirho::FileChirho(content_chirho) => {
                                            TmpfsDataChirho::FileChirho(content_chirho.clone())
                                        }
                                        TmpfsDataChirho::SymlinkChirho(target_chirho) => {
                                            TmpfsDataChirho::SymlinkChirho(target_chirho.clone())
                                        }
                                    };
                                    drop(inner_chirho);
                                    Some(Box::new(Mutex::new(cloned_data_chirho)) as Box<dyn core::any::Any + Send>)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                        let cloned_chirho = InodeChirho {
                            ino_chirho: locked_chirho.ino_chirho,
                            mode_chirho: locked_chirho.mode_chirho,
                            uid_chirho: locked_chirho.uid_chirho,
                            gid_chirho: locked_chirho.gid_chirho,
                            size_chirho: locked_chirho.size_chirho,
                            nlink_chirho: locked_chirho.nlink_chirho,
                            atime_chirho: locked_chirho.atime_chirho,
                            mtime_chirho: locked_chirho.mtime_chirho,
                            ctime_chirho: locked_chirho.ctime_chirho,
                            ops_chirho: locked_chirho.ops_chirho,
                            fs_data_chirho: fs_data_new_chirho,
                        };
                        return Ok(Arc::new(cloned_chirho));
                    }
                }
                Err(-ENOENT_CHIRHO)
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }

    fn create_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let mut data_chirho = data_lock_chirho.lock();

        match &mut *data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                // Check for duplicate
                for (n_chirho, _) in entries_chirho.iter() {
                    if n_chirho == name_chirho {
                        return Err(-EEXIST_CHIRHO);
                    }
                }

                let ino_chirho = alloc_ino_chirho();
                let new_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFREG_CHIRHO | (mode_chirho & 0o7777),
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: new_tmpfs_fs_data_chirho(TmpfsDataChirho::FileChirho(Vec::new())),
                }));

                entries_chirho.push((String::from(name_chirho), new_inode_chirho));
                if entries_chirho.len() > 200 {
                    crate::serial_debug_chirho!(
                        "[TMPFS] dir has {} entries after adding '{}'",
                        entries_chirho.len(), name_chirho,
                    );
                }

                Ok(Arc::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFREG_CHIRHO | (mode_chirho & 0o7777),
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: None,
                }))
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }

    fn mkdir_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let mut data_chirho = data_lock_chirho.lock();

        match &mut *data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                for (n_chirho, _) in entries_chirho.iter() {
                    if n_chirho == name_chirho {
                        return Err(-EEXIST_CHIRHO);
                    }
                }

                let ino_chirho = alloc_ino_chirho();
                let new_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFDIR_CHIRHO | (mode_chirho & 0o7777),
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 2,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: new_tmpfs_fs_data_chirho(TmpfsDataChirho::DirChirho(Vec::new())),
                }));

                entries_chirho.push((String::from(name_chirho), new_inode_chirho));
                if entries_chirho.len() > 200 {
                    crate::serial_debug_chirho!(
                        "[TMPFS] dir has {} entries after adding '{}'",
                        entries_chirho.len(), name_chirho,
                    );
                }

                Ok(Arc::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFDIR_CHIRHO | (mode_chirho & 0o7777),
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 2,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: None,
                }))
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }

    fn unlink_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<(), i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let mut data_chirho = data_lock_chirho.lock();

        match &mut *data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                let pos_chirho = entries_chirho
                    .iter()
                    .position(|(n_chirho, _)| n_chirho == name_chirho)
                    .ok_or(-ENOENT_CHIRHO)?;

                // Check it is not a directory
                let child_chirho = entries_chirho[pos_chirho].1.lock();
                if child_chirho.mode_chirho & S_IFDIR_CHIRHO == S_IFDIR_CHIRHO {
                    return Err(-EISDIR_CHIRHO);
                }
                drop(child_chirho);

                entries_chirho.remove(pos_chirho);
                Ok(())
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }

    fn rmdir_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<(), i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let mut data_chirho = data_lock_chirho.lock();

        match &mut *data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                let pos_chirho = entries_chirho
                    .iter()
                    .position(|(n_chirho, _)| n_chirho == name_chirho)
                    .ok_or(-ENOENT_CHIRHO)?;

                // Check it is a directory and is empty
                let child_chirho = entries_chirho[pos_chirho].1.lock();
                if child_chirho.mode_chirho & S_IFDIR_CHIRHO != S_IFDIR_CHIRHO {
                    return Err(-ENOTDIR_CHIRHO);
                }
                if let Some(child_fs_data_chirho) = child_chirho.fs_data_chirho.as_ref() {
                    if let Some(child_data_lock_chirho) =
                        child_fs_data_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>()
                    {
                        let child_data_chirho = child_data_lock_chirho.lock();
                        if let TmpfsDataChirho::DirChirho(child_entries_chirho) = &*child_data_chirho {
                            if !child_entries_chirho.is_empty() {
                                return Err(-ENOTEMPTY_CHIRHO);
                            }
                        }
                    }
                }
                drop(child_chirho);

                entries_chirho.remove(pos_chirho);
                Ok(())
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }

    fn readlink_chirho(
        &self,
        inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        // Check S_IFLNK
        if inode_chirho.mode_chirho & 0xF000 != 0xA000 {
            return Err(-EINVAL_CHIRHO);
        }
        let data_lock_chirho = get_tmpfs_data_chirho(inode_chirho)?;
        let data_chirho = data_lock_chirho.lock();
        match &*data_chirho {
            TmpfsDataChirho::SymlinkChirho(target_chirho) => {
                Ok(target_chirho.clone())
            }
            _ => Err(-EINVAL_CHIRHO),
        }
    }

    fn symlink_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        target_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        let data_lock_chirho = get_tmpfs_data_chirho(parent_chirho)?;
        let mut data_chirho = data_lock_chirho.lock();

        match &mut *data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                // Check for duplicate
                for (n_chirho, _) in entries_chirho.iter() {
                    if n_chirho == name_chirho {
                        return Err(-EEXIST_CHIRHO);
                    }
                }

                let ino_chirho = alloc_ino_chirho();
                let new_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFLNK_CHIRHO | 0o777, // symlinks are always rwxrwxrwx
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: target_chirho.len() as u64,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: new_tmpfs_fs_data_chirho(
                        TmpfsDataChirho::SymlinkChirho(String::from(target_chirho)),
                    ),
                }));

                entries_chirho.push((String::from(name_chirho), new_inode_chirho.clone()));
                if entries_chirho.len() > 200 {
                    crate::serial_debug_chirho!(
                        "[TMPFS] dir has {} entries after adding '{}'",
                        entries_chirho.len(), name_chirho,
                    );
                }

                Ok(Arc::new(InodeChirho {
                    ino_chirho,
                    mode_chirho: S_IFLNK_CHIRHO | 0o777,
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: target_chirho.len() as u64,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: None,
                }))
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }
}

/// Static instance of tmpfs inode operations.
pub static TMPFS_INODE_OPS_CHIRHO: TmpfsInodeOpsChirho = TmpfsInodeOpsChirho;

// ---------------------------------------------------------------------------
// TmpfsFileOpsChirho
// ---------------------------------------------------------------------------

/// File operations for tmpfs regular files and directories.
pub struct TmpfsFileOpsChirho;

impl FileOpsChirho for TmpfsFileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        let inode_chirho = file_chirho.inode_chirho.lock();
        let data_lock_chirho = get_tmpfs_data_chirho(&inode_chirho)?;
        let data_chirho = data_lock_chirho.lock();

        match &*data_chirho {
            TmpfsDataChirho::FileChirho(content_chirho) => {
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
            TmpfsDataChirho::DirChirho(_) => Err(-EISDIR_CHIRHO),
            TmpfsDataChirho::SymlinkChirho(target_chirho) => {
                // Reading a symlink returns its target content.
                let pos_chirho = file_chirho.pos_chirho as usize;
                let target_bytes_chirho = target_chirho.as_bytes();
                if pos_chirho >= target_bytes_chirho.len() {
                    return Ok(0);
                }
                let available_chirho = target_bytes_chirho.len() - pos_chirho;
                let to_read_chirho = buf_chirho.len().min(available_chirho);
                buf_chirho[..to_read_chirho]
                    .copy_from_slice(&target_bytes_chirho[pos_chirho..pos_chirho + to_read_chirho]);
                file_chirho.pos_chirho += to_read_chirho as u64;
                Ok(to_read_chirho)
            }
        }
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // Get tmpfs data and write content.
        let new_size_chirho = {
            let inode_chirho = file_chirho.inode_chirho.lock();
            let data_lock_chirho = get_tmpfs_data_chirho(&inode_chirho)?;
            let mut data_chirho = data_lock_chirho.lock();

            match &mut *data_chirho {
                TmpfsDataChirho::FileChirho(content_chirho) => {
                    let pos_chirho = file_chirho.pos_chirho as usize;

                    // Extend if writing beyond current length
                    if pos_chirho + buf_chirho.len() > content_chirho.len() {
                        content_chirho.resize(pos_chirho + buf_chirho.len(), 0);
                    }

                    content_chirho[pos_chirho..pos_chirho + buf_chirho.len()]
                        .copy_from_slice(buf_chirho);
                    file_chirho.pos_chirho += buf_chirho.len() as u64;

                    Ok(content_chirho.len() as u64) // Return new content size
                }
                TmpfsDataChirho::DirChirho(_) => Err(-EISDIR_CHIRHO),
                TmpfsDataChirho::SymlinkChirho(_) => Err(-EINVAL_CHIRHO),
            }
            // inode_chirho lock dropped here
        }?;

        // Update inode size (lock released above, safe to re-lock)
        {
            let mut inode_chirho = file_chirho.inode_chirho.lock();
            inode_chirho.size_chirho = new_size_chirho;
        }

        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        let inode_chirho = file_chirho.inode_chirho.lock();
        let size_chirho = inode_chirho.size_chirho;
        drop(inode_chirho);

        let new_pos_chirho: i64 = match whence_chirho {
            SEEK_SET_CHIRHO => offset_chirho,
            SEEK_CUR_CHIRHO => file_chirho.pos_chirho as i64 + offset_chirho,
            SEEK_END_CHIRHO => size_chirho as i64 + offset_chirho,
            _ => return Err(-EINVAL_CHIRHO),
        };

        if new_pos_chirho < 0 {
            return Err(-EINVAL_CHIRHO);
        }

        file_chirho.pos_chirho = new_pos_chirho as u64;
        Ok(file_chirho.pos_chirho)
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
        let ino_chirho = inode_chirho.ino_chirho;
        let data_lock_chirho = get_tmpfs_data_chirho(&inode_chirho)?;
        let data_chirho = data_lock_chirho.lock();

        match &*data_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                let mut count_chirho: usize = 0;
                let start_chirho = file_chirho.pos_chirho as usize;

                // Emit "." and ".." as virtual entries
                if start_chirho == 0 {
                    if !callback_chirho(".", ino_chirho, 4 /* DT_DIR */) {
                        return Ok(count_chirho);
                    }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }
                if (file_chirho.pos_chirho as usize) == 1 {
                    if !callback_chirho("..", ino_chirho, 4) {
                        return Ok(count_chirho);
                    }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }

                // Emit real entries (offset by 2 for . and ..)
                let entry_start_chirho = if start_chirho > 2 { start_chirho - 2 } else { 0 };
                for (name_chirho, child_chirho) in entries_chirho.iter().skip(entry_start_chirho) {
                    let child_locked_chirho = child_chirho.lock();
                    let file_type_chirho: u8 =
                        if child_locked_chirho.mode_chirho & S_IFDIR_CHIRHO == S_IFDIR_CHIRHO {
                            4 // DT_DIR
                        } else {
                            8 // DT_REG
                        };
                    if !callback_chirho(name_chirho, child_locked_chirho.ino_chirho, file_type_chirho) {
                        return Ok(count_chirho);
                    }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }
                Ok(count_chirho)
            }
            _ => Err(-ENOTDIR_CHIRHO),
        }
    }
}

/// Static instance of tmpfs file operations.
pub static TMPFS_FILE_OPS_CHIRHO: TmpfsFileOpsChirho = TmpfsFileOpsChirho;

// ---------------------------------------------------------------------------
// TmpfsSuperOpsChirho
// ---------------------------------------------------------------------------

/// Superblock operations for tmpfs.
pub struct TmpfsSuperOpsChirho;

/// tmpfs magic number (same as Linux TMPFS_MAGIC).
const TMPFS_MAGIC_CHIRHO: u64 = 0x01021994;

impl SuperOpsChirho for TmpfsSuperOpsChirho {
    fn alloc_inode_chirho(&self) -> Arc<InodeChirho> {
        Arc::new(InodeChirho {
            ino_chirho: alloc_ino_chirho(),
            mode_chirho: S_IFREG_CHIRHO | 0o644,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
            fs_data_chirho: new_tmpfs_fs_data_chirho(TmpfsDataChirho::FileChirho(Vec::new())),
        })
    }

    fn statfs_chirho(&self) -> Result<StatfsChirho, i64> {
        Ok(StatfsChirho {
            f_type_chirho: TMPFS_MAGIC_CHIRHO,
            f_bsize_chirho: 4096,
            f_blocks_chirho: 0, // unlimited (bounded by heap)
            f_bfree_chirho: 0,
            f_bavail_chirho: 0,
            f_files_chirho: 0,
            f_ffree_chirho: 0,
            f_namelen_chirho: 255,
        })
    }
}

/// Static instance of tmpfs superblock operations.
static TMPFS_SUPER_OPS_CHIRHO: TmpfsSuperOpsChirho = TmpfsSuperOpsChirho;

// ---------------------------------------------------------------------------
// Mount function
// ---------------------------------------------------------------------------

/// Create and return a mounted tmpfs instance.
///
/// The returned superblock has a root dentry backed by a directory inode
/// whose `fs_data_chirho` is an empty `TmpfsDataChirho::DirChirho`.
pub fn mount_tmpfs_chirho() -> Arc<Mutex<SuperblockChirho>> {
    let root_inode_chirho = Arc::new(Mutex::new(InodeChirho {
        ino_chirho: alloc_ino_chirho(),
        mode_chirho: S_IFDIR_CHIRHO | 0o755,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
        fs_data_chirho: new_tmpfs_fs_data_chirho(TmpfsDataChirho::DirChirho(Vec::new())),
    }));

    let root_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("/"),
        inode_chirho: Some(root_inode_chirho),
        parent_chirho: None,
        children_chirho: Vec::new(),
    }));

    Arc::new(Mutex::new(SuperblockChirho {
        fs_type_chirho: "tmpfs",
        root_chirho: root_dentry_chirho,
        flags_chirho: 0,
        ops_chirho: &TMPFS_SUPER_OPS_CHIRHO,
    }))
}

/// Write a file to tmpfs at the given path.
///
/// Creates the file if it doesn't exist on tmpfs. If the path resolves
/// to an ext4 file, this is a no-op (ext4 files can't be overwritten
/// via this helper).
///
/// Used by the kernel to create configuration files (e.g., `/etc/profile`)
/// before launching userspace.
pub fn write_tmpfs_file_chirho(path_chirho: &str, content_chirho: &[u8]) {
    // Split path into directory and filename
    let (dir_path_chirho, file_name_chirho) = match path_chirho.rfind('/') {
        Some(idx_chirho) if idx_chirho > 0 => (&path_chirho[..idx_chirho], &path_chirho[idx_chirho + 1..]),
        _ => ("/", &path_chirho[1..]),
    };

    // Resolve the parent directory
    let parent_chirho = match crate::fs_chirho::resolve_path_chirho(dir_path_chirho) {
        Ok((inode_chirho, _ops_chirho)) => inode_chirho,
        Err(_) => {
            crate::serial_println_chirho!(
                "[TMPFS] write_tmpfs_file: parent dir '{}' not found",
                dir_path_chirho,
            );
            return;
        }
    };

    // Lock the parent and get its tmpfs directory entries
    let parent_guard_chirho = parent_chirho.lock();
    let tmpfs_data_chirho = match get_tmpfs_data_chirho(&parent_guard_chirho) {
        Ok(d_chirho) => d_chirho,
        Err(_) => {
            crate::serial_println_chirho!(
                "[TMPFS] write_tmpfs_file: '{}' is not a tmpfs directory",
                dir_path_chirho,
            );
            return;
        }
    };

    let mut data_guard_chirho = tmpfs_data_chirho.lock();
    if let TmpfsDataChirho::DirChirho(ref mut entries_chirho) = *data_guard_chirho {
        // Check if file already exists
        for (name_chirho, inode_arc_chirho) in entries_chirho.iter() {
            if name_chirho == file_name_chirho {
                // Overwrite existing file content
                {
                    let inode_guard_chirho = inode_arc_chirho.lock();
                    if let Ok(file_data_chirho) = get_tmpfs_data_chirho(&inode_guard_chirho) {
                        let mut fd_chirho = file_data_chirho.lock();
                        if let TmpfsDataChirho::FileChirho(ref mut buf_chirho) = *fd_chirho {
                            buf_chirho.clear();
                            buf_chirho.extend_from_slice(content_chirho);
                        }
                    }
                }
                crate::serial_debug_chirho!(
                    "[TMPFS] Overwrote {} ({} bytes)",
                    path_chirho,
                    content_chirho.len(),
                );
                return;
            }
        }

        // Create new file
        let new_inode_chirho = Arc::new(Mutex::new(InodeChirho {
            ino_chirho: alloc_ino_chirho(),
            mode_chirho: S_IFREG_CHIRHO | 0o644,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: content_chirho.len() as u64,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
            fs_data_chirho: new_tmpfs_fs_data_chirho(
                TmpfsDataChirho::FileChirho(Vec::from(content_chirho)),
            ),
        }));
        entries_chirho.push((String::from(file_name_chirho), new_inode_chirho));
        crate::serial_debug_chirho!(
            "[TMPFS] Created {} ({} bytes)",
            path_chirho,
            content_chirho.len(),
        );
    }
}
