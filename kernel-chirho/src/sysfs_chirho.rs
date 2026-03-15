// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! sysfs — Skeleton `/sys` filesystem for Lineluya.
//!
//! Provides a minimal directory structure under `/sys` that programs can
//! stat and readdir on:
//!
//! ```text
//! /sys/
//! ├── class/
//! ├── devices/
//! └── kernel/
//! ```
//!
//! No dynamic content is served yet; this module just sets up the directory
//! tree so that mount("sysfs", "/sys", ...) returns a valid superblock.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::vfs_chirho::{
    DentryChirho, FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho,
    S_IFDIR_CHIRHO,
    StatfsChirho, SuperOpsChirho, SuperblockChirho,
};
use crate::syscall_chirho::{
    EISDIR_CHIRHO, ENOENT_CHIRHO, ENOSYS_CHIRHO,
};

// ---------------------------------------------------------------------------
// Inode counter
// ---------------------------------------------------------------------------

/// Inode counter for sysfs (starts high to avoid collisions with tmpfs/procfs).
static SYSFS_NEXT_INO_CHIRHO: AtomicU64 = AtomicU64::new(90_000);

fn alloc_ino_chirho() -> u64 {
    SYSFS_NEXT_INO_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// SysfsInodeOpsChirho
// ---------------------------------------------------------------------------

/// Inode operations for sysfs directories (read-only skeleton).
struct SysfsInodeOpsChirho;

impl InodeOpsChirho for SysfsInodeOpsChirho {
    fn lookup_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(ENOENT_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(ENOSYS_CHIRHO) // sysfs is read-only
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(ENOSYS_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(ENOSYS_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(ENOSYS_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        Err(EINVAL_CHIRHO)
    }
}

use crate::syscall_chirho::EINVAL_CHIRHO;

/// Singleton instance of the sysfs inode operations.
static SYSFS_INODE_OPS_CHIRHO: SysfsInodeOpsChirho = SysfsInodeOpsChirho;

// ---------------------------------------------------------------------------
// SysfsFileOpsChirho
// ---------------------------------------------------------------------------

/// File operations for sysfs directories (read-only skeleton).
struct SysfsFileOpsChirho;

impl FileOpsChirho for SysfsFileOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Err(EISDIR_CHIRHO)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(ENOSYS_CHIRHO)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(ENOSYS_CHIRHO)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(EINVAL_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        // No dynamic entries yet
        Ok(0)
    }
}

/// Singleton instance of the sysfs file operations.
static SYSFS_FILE_OPS_CHIRHO: SysfsFileOpsChirho = SysfsFileOpsChirho;

// ---------------------------------------------------------------------------
// SysfsSuperOpsChirho
// ---------------------------------------------------------------------------

/// Superblock operations for sysfs.
struct SysfsSuperOpsChirho;

impl SuperOpsChirho for SysfsSuperOpsChirho {
    fn alloc_inode_chirho(&self) -> Arc<InodeChirho> {
        Arc::new(InodeChirho {
            ino_chirho: alloc_ino_chirho(),
            mode_chirho: S_IFDIR_CHIRHO | 0o555,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 2,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &SYSFS_INODE_OPS_CHIRHO,
            fs_data_chirho: None,
        })
    }

    fn statfs_chirho(&self) -> Result<StatfsChirho, i64> {
        Ok(StatfsChirho {
            f_type_chirho: 0x6273_7973, // "sysb" magic
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

/// Singleton instance of sysfs superblock operations.
static SYSFS_SUPER_OPS_CHIRHO: SysfsSuperOpsChirho = SysfsSuperOpsChirho;

// ---------------------------------------------------------------------------
// Helper: create a directory inode + dentry
// ---------------------------------------------------------------------------

fn make_dir_dentry_chirho(
    name_chirho: &str,
    parent_chirho: Option<Arc<Mutex<DentryChirho>>>,
) -> Arc<Mutex<DentryChirho>> {
    let inode_chirho = Arc::new(Mutex::new(InodeChirho {
        ino_chirho: alloc_ino_chirho(),
        mode_chirho: S_IFDIR_CHIRHO | 0o555,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &SYSFS_INODE_OPS_CHIRHO,
        fs_data_chirho: None,
    }));

    Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from(name_chirho),
        inode_chirho: Some(inode_chirho),
        parent_chirho,
        children_chirho: Vec::new(),
    }))
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

/// Mount sysfs, creating the `/sys` directory skeleton.
///
/// Returns a [`SuperblockChirho`] whose root dentry contains:
/// - `class/`
/// - `devices/`
/// - `kernel/`
pub fn mount_sysfs_chirho() -> Arc<Mutex<SuperblockChirho>> {
    // Create root dentry for /sys
    let root_dentry_chirho = make_dir_dentry_chirho("/", None);

    // Create child directories
    let class_dentry_chirho = make_dir_dentry_chirho("class", Some(Arc::clone(&root_dentry_chirho)));
    let devices_dentry_chirho = make_dir_dentry_chirho("devices", Some(Arc::clone(&root_dentry_chirho)));
    let kernel_dentry_chirho = make_dir_dentry_chirho("kernel", Some(Arc::clone(&root_dentry_chirho)));
    // A2: /sys/module directory — lists loaded kernel modules.
    let module_dentry_chirho = make_dir_dentry_chirho("module", Some(Arc::clone(&root_dentry_chirho)));
    // A2: /sys/bus directory — bus subsystem.
    let bus_dentry_chirho = make_dir_dentry_chirho("bus", Some(Arc::clone(&root_dentry_chirho)));
    // /sys/fs directory — filesystem parameters.
    let fs_dentry_chirho = make_dir_dentry_chirho("fs", Some(Arc::clone(&root_dentry_chirho)));

    // Attach children to root
    {
        let mut root_guard_chirho = root_dentry_chirho.lock();
        root_guard_chirho.children_chirho.push(class_dentry_chirho);
        root_guard_chirho.children_chirho.push(devices_dentry_chirho);
        root_guard_chirho.children_chirho.push(kernel_dentry_chirho);
        root_guard_chirho.children_chirho.push(module_dentry_chirho);
        root_guard_chirho.children_chirho.push(bus_dentry_chirho);
        root_guard_chirho.children_chirho.push(fs_dentry_chirho);
    }

    crate::serial_println_chirho!("[SYSFS] Mounted with /sys/class, /sys/devices, /sys/kernel, /sys/module, /sys/bus, /sys/fs");

    Arc::new(Mutex::new(SuperblockChirho {
        fs_type_chirho: "sysfs",
        root_chirho: root_dentry_chirho,
        flags_chirho: 0,
        ops_chirho: &SYSFS_SUPER_OPS_CHIRHO,
    }))
}
