// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Virtual Filesystem Switch (VFS) layer for Lineluya.
//!
//! Provides the core abstractions that let different filesystem implementations
//! (tmpfs, procfs, ext4, etc.) coexist behind a uniform interface.  This is
//! equivalent to Linux's `fs/` layer.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use spin::Mutex;

// ---------------------------------------------------------------------------
// File-type constants (S_IF*)
// ---------------------------------------------------------------------------

pub const S_IFREG_CHIRHO: u32 = 0o100000;
pub const S_IFDIR_CHIRHO: u32 = 0o040000;
pub const S_IFLNK_CHIRHO: u32 = 0o120000;
pub const S_IFCHR_CHIRHO: u32 = 0o020000;
pub const S_IFBLK_CHIRHO: u32 = 0o060000;

// ---------------------------------------------------------------------------
// Open flags (O_*)
// ---------------------------------------------------------------------------

pub const O_RDONLY_CHIRHO: u32 = 0;
pub const O_WRONLY_CHIRHO: u32 = 1;
pub const O_RDWR_CHIRHO: u32 = 2;
pub const O_CREAT_CHIRHO: u32 = 0o100;
pub const O_TRUNC_CHIRHO: u32 = 0o1000;
pub const O_APPEND_CHIRHO: u32 = 0o2000;
pub const O_DIRECTORY_CHIRHO: u32 = 0o200000;

// ---------------------------------------------------------------------------
// Seek whence constants
// ---------------------------------------------------------------------------

pub const SEEK_SET_CHIRHO: u32 = 0;
pub const SEEK_CUR_CHIRHO: u32 = 1;
pub const SEEK_END_CHIRHO: u32 = 2;

// ---------------------------------------------------------------------------
// InodeChirho — in-memory filesystem object
// ---------------------------------------------------------------------------

/// Represents an in-memory inode, the kernel's view of a filesystem object
/// (file, directory, symlink, device node, etc.).
pub struct InodeChirho {
    /// Inode number (unique within a filesystem).
    pub ino_chirho: u64,
    /// File type and permission bits (e.g. `S_IFREG_CHIRHO | 0o644`).
    pub mode_chirho: u32,
    /// Owner user ID.
    pub uid_chirho: u32,
    /// Owner group ID.
    pub gid_chirho: u32,
    /// Size in bytes.
    pub size_chirho: u64,
    /// Hard-link count.
    pub nlink_chirho: u32,
    /// Last access time (seconds since epoch).
    pub atime_chirho: u64,
    /// Last modification time (seconds since epoch).
    pub mtime_chirho: u64,
    /// Last status-change time (seconds since epoch).
    pub ctime_chirho: u64,
    /// Vtable for inode operations (lookup, create, mkdir, etc.).
    pub ops_chirho: &'static dyn InodeOpsChirho,
    /// Filesystem-private data (e.g. tmpfs page cache, ext4 disk inode).
    pub fs_data_chirho: Option<Box<dyn Any + Send>>,
}

// SAFETY: InodeChirho is only accessed through Arc<Mutex<InodeChirho>> or via
// &-references whose lifetimes the caller controls.  The `ops_chirho` field is
// `'static` and `fs_data_chirho` is `Send`.
unsafe impl Send for InodeChirho {}
unsafe impl Sync for InodeChirho {}

// ---------------------------------------------------------------------------
// InodeOpsChirho — inode-level operations
// ---------------------------------------------------------------------------

/// Operations dispatched on a directory or special inode.  Each filesystem
/// supplies its own implementation of this trait.
pub trait InodeOpsChirho: Send + Sync {
    /// Look up a child by name inside `parent_chirho`.
    fn lookup_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64>;

    /// Create a regular file inside `parent_chirho`.
    fn create_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64>;

    /// Create a subdirectory inside `parent_chirho`.
    fn mkdir_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64>;

    /// Remove a non-directory entry from `parent_chirho`.
    fn unlink_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<(), i64>;

    /// Remove a directory entry from `parent_chirho`.
    fn rmdir_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<(), i64>;

    /// Read the target of a symbolic link.
    fn readlink_chirho(
        &self,
        inode_chirho: &InodeChirho,
    ) -> Result<String, i64>;
}

// ---------------------------------------------------------------------------
// FileChirho — open file instance
// ---------------------------------------------------------------------------

/// An open file description.  Each `open()` call produces one of these,
/// holding a reference to the underlying inode, the current file position,
/// and the open flags.
pub struct FileChirho {
    /// The inode this file refers to.
    pub inode_chirho: Arc<Mutex<InodeChirho>>,
    /// Current read/write offset.
    pub pos_chirho: u64,
    /// Open flags (`O_RDONLY_CHIRHO`, `O_APPEND_CHIRHO`, etc.).
    pub flags_chirho: u32,
    /// Vtable for file operations (read, write, seek, etc.).
    pub ops_chirho: &'static dyn FileOpsChirho,
}

// SAFETY: FileChirho is only accessed behind Arc<Mutex<FileChirho>>.
unsafe impl Send for FileChirho {}
unsafe impl Sync for FileChirho {}

// ---------------------------------------------------------------------------
// FileOpsChirho — file-level operations
// ---------------------------------------------------------------------------

/// Operations dispatched on an open file.  Each filesystem (or file type)
/// supplies its own implementation.
pub trait FileOpsChirho: Send + Sync {
    /// Read up to `buf_chirho.len()` bytes from the file at its current
    /// position.  Returns the number of bytes read.
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64>;

    /// Write up to `buf_chirho.len()` bytes to the file at its current
    /// position.  Returns the number of bytes written.
    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64>;

    /// Reposition the file offset.  `whence_chirho` is one of
    /// `SEEK_SET_CHIRHO`, `SEEK_CUR_CHIRHO`, or `SEEK_END_CHIRHO`.
    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64>;

    /// Device control.
    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64>;

    /// Iterate directory entries.  `callback_chirho` receives
    /// `(name, inode_number, file_type)` and returns `true` to continue or
    /// `false` to stop.  Returns the number of entries emitted.
    fn readdir_chirho(
        &self,
        file_chirho: &mut FileChirho,
        callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64>;
}

// ---------------------------------------------------------------------------
// DentryChirho — directory-entry cache
// ---------------------------------------------------------------------------

/// A cached directory entry, linking a name to an inode and tracking the
/// tree structure of the namespace.
pub struct DentryChirho {
    /// Name component (e.g. `"bin"`, `"hello.txt"`).
    pub name_chirho: String,
    /// The inode this entry points to (`None` for negative dentries).
    pub inode_chirho: Option<Arc<Mutex<InodeChirho>>>,
    /// Parent directory entry.
    pub parent_chirho: Option<Arc<Mutex<DentryChirho>>>,
    /// Cached child entries.
    pub children_chirho: Vec<Arc<Mutex<DentryChirho>>>,
}

// ---------------------------------------------------------------------------
// StatfsChirho — filesystem statistics (returned by statfs/fstatfs)
// ---------------------------------------------------------------------------

/// Filesystem statistics, modeled after Linux's `struct statfs`.
pub struct StatfsChirho {
    pub f_type_chirho: u64,
    pub f_bsize_chirho: u64,
    pub f_blocks_chirho: u64,
    pub f_bfree_chirho: u64,
    pub f_bavail_chirho: u64,
    pub f_files_chirho: u64,
    pub f_ffree_chirho: u64,
    pub f_namelen_chirho: u64,
}

// ---------------------------------------------------------------------------
// SuperblockChirho — mounted filesystem instance
// ---------------------------------------------------------------------------

/// Represents a mounted filesystem instance.
pub struct SuperblockChirho {
    /// Filesystem type name (e.g. `"tmpfs"`, `"procfs"`).
    pub fs_type_chirho: &'static str,
    /// Root dentry of this mount.
    pub root_chirho: Arc<Mutex<DentryChirho>>,
    /// Mount flags.
    pub flags_chirho: u32,
    /// Vtable for superblock operations.
    pub ops_chirho: &'static dyn SuperOpsChirho,
}

// SAFETY: SuperblockChirho fields are either 'static or Arc-wrapped.
unsafe impl Send for SuperblockChirho {}
unsafe impl Sync for SuperblockChirho {}

// ---------------------------------------------------------------------------
// SuperOpsChirho — superblock-level operations
// ---------------------------------------------------------------------------

/// Operations on a mounted filesystem's superblock.
pub trait SuperOpsChirho: Send + Sync {
    /// Allocate a fresh inode for this filesystem.
    fn alloc_inode_chirho(&self) -> Arc<InodeChirho>;

    /// Return filesystem statistics.
    fn statfs_chirho(&self) -> Result<StatfsChirho, i64>;
}

// ---------------------------------------------------------------------------
// FdTableChirho — per-process file descriptor table
// ---------------------------------------------------------------------------

/// Per-process table mapping small integer file descriptors to open file
/// descriptions.
pub struct FdTableChirho {
    pub fds_chirho: Vec<Option<Arc<Mutex<FileChirho>>>>,
}

/// Linux errno: bad file descriptor.
const EBADF_CHIRHO: i64 = -9;
/// Linux errno: too many open files.
const EMFILE_CHIRHO: i64 = -24;

impl FdTableChirho {
    /// Create a new, empty file-descriptor table with room for
    /// `capacity_chirho` descriptors.
    pub fn new_chirho(capacity_chirho: usize) -> Self {
        let mut fds_chirho = Vec::with_capacity(capacity_chirho);
        fds_chirho.resize_with(capacity_chirho, || None);
        Self { fds_chirho }
    }

    /// Allocate the lowest available file descriptor, returning its index.
    pub fn alloc_fd_chirho(&mut self) -> Result<usize, i64> {
        for (idx_chirho, slot_chirho) in self.fds_chirho.iter().enumerate() {
            if slot_chirho.is_none() {
                return Ok(idx_chirho);
            }
        }
        Err(EMFILE_CHIRHO)
    }

    /// Retrieve the open file for `fd_chirho`, if any.
    pub fn get_chirho(&self, fd_chirho: usize) -> Option<Arc<Mutex<FileChirho>>> {
        self.fds_chirho
            .get(fd_chirho)
            .and_then(|slot_chirho| slot_chirho.clone())
    }

    /// Close a file descriptor, returning an error if it was not open.
    pub fn close_chirho(&mut self, fd_chirho: usize) -> Result<(), i64> {
        match self.fds_chirho.get_mut(fd_chirho) {
            Some(slot_chirho @ Some(_)) => {
                *slot_chirho = None;
                Ok(())
            }
            _ => Err(EBADF_CHIRHO),
        }
    }

    /// Duplicate `old_fd_chirho` into the lowest available descriptor.
    pub fn dup_chirho(&mut self, old_fd_chirho: usize) -> Result<usize, i64> {
        let file_chirho = self.get_chirho(old_fd_chirho).ok_or(EBADF_CHIRHO)?;
        let new_fd_chirho = self.alloc_fd_chirho()?;
        self.fds_chirho[new_fd_chirho] = Some(file_chirho);
        Ok(new_fd_chirho)
    }

    /// Clone the entire file descriptor table.
    ///
    /// Each open file description (`Arc<Mutex<FileChirho>>`) is shared with the
    /// clone (matching POSIX fork semantics where parent and child share the
    /// underlying open file descriptions but have independent fd tables).
    pub fn clone_table_chirho(&self) -> Self {
        Self {
            fds_chirho: self.fds_chirho.clone(),
        }
    }
}
