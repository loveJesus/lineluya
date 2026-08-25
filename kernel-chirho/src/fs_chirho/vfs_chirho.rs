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
use core::sync::atomic::{AtomicU32, Ordering};
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
// Inode mode type-check helpers (A2-AUDIT-014)
// ---------------------------------------------------------------------------

/// Returns true if `mode_chirho` indicates a symbolic link (S_IFLNK).
#[inline]
pub fn is_symlink_chirho(mode_chirho: u32) -> bool { mode_chirho & 0xF000 == 0xA000 }

/// Returns true if `mode_chirho` indicates a directory (S_IFDIR).
#[inline]
pub fn is_dir_chirho(mode_chirho: u32) -> bool { mode_chirho & 0xF000 == 0x4000 }

/// Returns true if `mode_chirho` indicates a regular file (S_IFREG).
#[inline]
pub fn is_regular_chirho(mode_chirho: u32) -> bool { mode_chirho & 0xF000 == 0x8000 }

/// Returns true if `mode_chirho` indicates a character device (S_IFCHR).
#[inline]
pub fn is_chardev_chirho(mode_chirho: u32) -> bool { mode_chirho & 0xF000 == 0x2000 }

/// Returns true if `mode_chirho` indicates a block device (S_IFBLK).
#[inline]
pub fn is_blkdev_chirho(mode_chirho: u32) -> bool { mode_chirho & 0xF000 == 0x6000 }

// ---------------------------------------------------------------------------
// Open flags (O_*)
// ---------------------------------------------------------------------------

pub const O_RDONLY_CHIRHO: u32 = 0;
pub const O_WRONLY_CHIRHO: u32 = 1;
pub const O_RDWR_CHIRHO: u32 = 2;
pub const O_CREAT_CHIRHO: u32 = 0o100;
pub const O_TRUNC_CHIRHO: u32 = 0o1000;
pub const O_APPEND_CHIRHO: u32 = 0o2000;
pub const O_NONBLOCK_CHIRHO: u32 = 0o4000;
pub const O_CLOEXEC_CHIRHO: u32 = 0o2000000;
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

    /// Create a symbolic link in `parent_chirho` with name `name_chirho`
    /// pointing to `target_chirho`.
    fn symlink_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
        target_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        let _ = (parent_chirho, name_chirho, target_chirho);
        Err(-38) // ENOSYS
    }
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
    /// Path associated with each fd (for openat dirfd resolution).
    pub paths_chirho: Vec<Option<alloc::string::String>>,
    /// Per-fd close-on-exec state (`FD_CLOEXEC`).
    pub cloexec_chirho: Vec<bool>,
}

/// Linux errno: bad file descriptor.
const EBADF_CHIRHO: i64 = -9;
/// Linux errno: too many open files.
const EMFILE_CHIRHO: i64 = -24;

/// A pipe endpoint can contribute a reader reference, a writer reference, or
/// both for an `O_RDWR` FIFO description.
#[derive(Clone, Copy)]
struct PipeEndpointReferenceChirho {
    reads_chirho: bool,
    writes_chirho: bool,
}

/// Keep impossible accounting diagnostics bounded. These messages expose a
/// residual descriptor bug; they never repair one by reopening a closed end.
static PIPE_REFERENCE_INVARIANT_COUNT_CHIRHO: AtomicU32 = AtomicU32::new(0);
const PIPE_REFERENCE_INVARIANT_LIMIT_CHIRHO: u32 = 16;

/// A nonzero value means an fd table reached `Drop` with live descriptors.
/// Drop is deliberately lock-free; normal ownership paths must explicitly
/// retire descriptors before releasing the table.
static UNRETIRED_FD_TABLE_DROP_COUNT_CHIRHO: AtomicU32 = AtomicU32::new(0);

fn saturating_atomic_increment_chirho(counter_chirho: &AtomicU32) -> u32 {
    counter_chirho
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value_chirho| {
            Some(value_chirho.saturating_add(1))
        })
        .unwrap_or_else(|value_chirho| value_chirho)
}

fn pipe_endpoint_reference_chirho(
    file_arc_chirho: &Arc<Mutex<FileChirho>>,
) -> Option<(
    Arc<Mutex<crate::pipe_chirho::PipeChirho>>,
    PipeEndpointReferenceChirho,
)> {
    let file_guard_chirho = file_arc_chirho.lock();
    let access_mode_chirho = file_guard_chirho.flags_chirho & 0b11;
    let endpoint_chirho = match access_mode_chirho {
        O_RDONLY_CHIRHO => PipeEndpointReferenceChirho {
            reads_chirho: true,
            writes_chirho: false,
        },
        O_WRONLY_CHIRHO => PipeEndpointReferenceChirho {
            reads_chirho: false,
            writes_chirho: true,
        },
        O_RDWR_CHIRHO => PipeEndpointReferenceChirho {
            reads_chirho: true,
            writes_chirho: true,
        },
        _ => return None,
    };
    let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
    if (inode_guard_chirho.mode_chirho & 0o170000) != 0o010000 {
        return None;
    }
    let pipe_arc_chirho = inode_guard_chirho
        .fs_data_chirho
        .as_ref()?
        .downcast_ref::<Arc<Mutex<crate::pipe_chirho::PipeChirho>>>()?
        .clone();
    Some((pipe_arc_chirho, endpoint_chirho))
}

fn report_pipe_reference_invariant_chirho(
    operation_chirho: &str,
    readers_chirho: u32,
    writers_chirho: u32,
    closed_read_chirho: bool,
    closed_write_chirho: bool,
) {
    let report_index_chirho =
        saturating_atomic_increment_chirho(&PIPE_REFERENCE_INVARIANT_COUNT_CHIRHO);
    if report_index_chirho < PIPE_REFERENCE_INVARIANT_LIMIT_CHIRHO {
        crate::serial_println_chirho!(
            "[PIPE-REF-INVARIANT] #{} op={} readers={} writers={} closed_read={} closed_write={}",
            report_index_chirho,
            operation_chirho,
            readers_chirho,
            writers_chirho,
            closed_read_chirho,
            closed_write_chirho,
        );
    }
}

/// Account one additional descriptor that refers to an existing pipe open-file
/// description. A legitimate duplicate is created while the source descriptor
/// is still live, so its endpoint count must be nonzero and not closed.
fn increment_pipe_descriptor_reference_chirho(
    file_arc_chirho: &Arc<Mutex<FileChirho>>,
    operation_chirho: &str,
) {
    let Some((pipe_arc_chirho, endpoint_chirho)) =
        pipe_endpoint_reference_chirho(file_arc_chirho)
    else {
        return;
    };
    let mut pipe_guard_chirho = pipe_arc_chirho.lock();
    let mut invariant_broken_chirho =
        (endpoint_chirho.reads_chirho
            && (pipe_guard_chirho.readers_chirho == 0
                || pipe_guard_chirho.closed_read_chirho))
            || (endpoint_chirho.writes_chirho
                && (pipe_guard_chirho.writers_chirho == 0
                    || pipe_guard_chirho.closed_write_chirho));

    if endpoint_chirho.reads_chirho {
        if pipe_guard_chirho.readers_chirho == u32::MAX {
            invariant_broken_chirho = true;
        } else {
            pipe_guard_chirho.readers_chirho += 1;
        }
    }
    if endpoint_chirho.writes_chirho {
        if pipe_guard_chirho.writers_chirho == u32::MAX {
            invariant_broken_chirho = true;
        } else {
            pipe_guard_chirho.writers_chirho += 1;
        }
    }
    let snapshot_chirho = (
        pipe_guard_chirho.readers_chirho,
        pipe_guard_chirho.writers_chirho,
        pipe_guard_chirho.closed_read_chirho,
        pipe_guard_chirho.closed_write_chirho,
    );
    drop(pipe_guard_chirho);

    if invariant_broken_chirho {
        report_pipe_reference_invariant_chirho(
            operation_chirho,
            snapshot_chirho.0,
            snapshot_chirho.1,
            snapshot_chirho.2,
            snapshot_chirho.3,
        );
    }
}

/// Release exactly one descriptor reference. EOF/EPIPE state changes only when
/// the final descriptor for the corresponding endpoint is retired.
fn decrement_pipe_descriptor_reference_chirho(
    file_arc_chirho: &Arc<Mutex<FileChirho>>,
    operation_chirho: &str,
) {
    let Some((pipe_arc_chirho, endpoint_chirho)) =
        pipe_endpoint_reference_chirho(file_arc_chirho)
    else {
        return;
    };
    let mut pipe_guard_chirho = pipe_arc_chirho.lock();
    let mut invariant_broken_chirho = false;

    if endpoint_chirho.reads_chirho {
        if pipe_guard_chirho.readers_chirho == 0 {
            invariant_broken_chirho = true;
        } else {
            if pipe_guard_chirho.closed_read_chirho {
                invariant_broken_chirho = true;
            }
            pipe_guard_chirho.readers_chirho -= 1;
            if pipe_guard_chirho.readers_chirho == 0 {
                pipe_guard_chirho.closed_read_chirho = true;
            }
        }
    }
    if endpoint_chirho.writes_chirho {
        if pipe_guard_chirho.writers_chirho == 0 {
            invariant_broken_chirho = true;
        } else {
            if pipe_guard_chirho.closed_write_chirho {
                invariant_broken_chirho = true;
            }
            pipe_guard_chirho.writers_chirho -= 1;
            if pipe_guard_chirho.writers_chirho == 0 {
                pipe_guard_chirho.closed_write_chirho = true;
            }
        }
    }
    let snapshot_chirho = (
        pipe_guard_chirho.readers_chirho,
        pipe_guard_chirho.writers_chirho,
        pipe_guard_chirho.closed_read_chirho,
        pipe_guard_chirho.closed_write_chirho,
    );
    drop(pipe_guard_chirho);

    if invariant_broken_chirho {
        report_pipe_reference_invariant_chirho(
            operation_chirho,
            snapshot_chirho.0,
            snapshot_chirho.1,
            snapshot_chirho.2,
            snapshot_chirho.3,
        );
    }
}

impl FdTableChirho {
    /// Create a new, empty file-descriptor table with room for
    /// `capacity_chirho` descriptors.
    pub fn new_chirho(capacity_chirho: usize) -> Self {
        let mut fds_chirho = Vec::with_capacity(capacity_chirho);
        fds_chirho.resize_with(capacity_chirho, || None);
        let mut paths_chirho = Vec::with_capacity(capacity_chirho);
        paths_chirho.resize_with(capacity_chirho, || None);
        let mut cloexec_chirho = Vec::with_capacity(capacity_chirho);
        cloexec_chirho.resize(capacity_chirho, false);
        Self { fds_chirho, paths_chirho, cloexec_chirho }
    }

    /// Allocate the lowest available file descriptor, returning its index.
    pub fn alloc_fd_chirho(&mut self) -> Result<usize, i64> {
        // POSIX requires open/dup to choose the lowest available descriptor.
        // Shell background jobs rely on this after closing stdin: opening
        // /dev/null must return fd 0, not an arbitrarily reserved fd >= 3.
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
                let file_arc_chirho = slot_chirho.take().unwrap();
                decrement_pipe_descriptor_reference_chirho(&file_arc_chirho, "close");
                drop(file_arc_chirho);
                if fd_chirho < self.paths_chirho.len() {
                    self.paths_chirho[fd_chirho] = None;
                }
                if fd_chirho < self.cloexec_chirho.len() {
                    self.cloexec_chirho[fd_chirho] = false;
                }
                Ok(())
            }
            _ => Err(EBADF_CHIRHO),
        }
    }

    /// Duplicate `old_fd_chirho` into the lowest available descriptor.
    pub fn dup_chirho(&mut self, old_fd_chirho: usize) -> Result<usize, i64> {
        let file_chirho = self.get_chirho(old_fd_chirho).ok_or(EBADF_CHIRHO)?;
        let new_fd_chirho = self.alloc_fd_chirho()?;
        let path_chirho = self.paths_chirho.get(old_fd_chirho).cloned().flatten();
        self.install_duplicate_at_chirho(
            new_fd_chirho,
            file_chirho,
            path_chirho,
            false,
            "dup",
        )
    }

    /// Duplicate `old_fd_chirho` onto `new_fd_chirho`, atomically retiring an
    /// occupied destination descriptor before installation.
    pub fn dup2_chirho(
        &mut self,
        old_fd_chirho: usize,
        new_fd_chirho: usize,
    ) -> Result<usize, i64> {
        let file_chirho = self.get_chirho(old_fd_chirho).ok_or(EBADF_CHIRHO)?;
        if old_fd_chirho == new_fd_chirho {
            return Ok(new_fd_chirho);
        }
        let path_chirho = self.paths_chirho.get(old_fd_chirho).cloned().flatten();
        self.install_duplicate_at_chirho(
            new_fd_chirho,
            file_chirho,
            path_chirho,
            false,
            "dup2",
        )
    }

    /// Install another descriptor reference to an existing open-file
    /// description at an exact slot. This is also used for the temporary
    /// global compatibility mirror, whose Arc must participate in pipe EOF and
    /// EPIPE accounting if it can later be closed as a descriptor.
    pub fn install_duplicate_at_chirho(
        &mut self,
        new_fd_chirho: usize,
        file_chirho: Arc<Mutex<FileChirho>>,
        path_chirho: Option<String>,
        cloexec_chirho: bool,
        operation_chirho: &str,
    ) -> Result<usize, i64> {
        if new_fd_chirho >= self.fds_chirho.len() {
            return Err(EBADF_CHIRHO);
        }

        // Increment first so dup2 over another descriptor for the same pipe
        // never transiently publishes EOF/EPIPE while the source is still live.
        increment_pipe_descriptor_reference_chirho(&file_chirho, operation_chirho);
        if self.fds_chirho[new_fd_chirho].is_some() {
            if let Err(error_chirho) = self.close_chirho(new_fd_chirho) {
                decrement_pipe_descriptor_reference_chirho(
                    &file_chirho,
                    "duplicate-rollback",
                );
                return Err(error_chirho);
            }
        }

        self.fds_chirho[new_fd_chirho] = Some(file_chirho);
        self.paths_chirho[new_fd_chirho] = path_chirho;
        self.cloexec_chirho[new_fd_chirho] = cloexec_chirho;
        Ok(new_fd_chirho)
    }

    /// Set or clear `FD_CLOEXEC` on an open file descriptor.
    pub fn set_cloexec_chirho(&mut self, fd_chirho: usize, enabled_chirho: bool) -> Result<(), i64> {
        if self.get_chirho(fd_chirho).is_none() {
            return Err(EBADF_CHIRHO);
        }
        if fd_chirho >= self.cloexec_chirho.len() {
            return Err(EBADF_CHIRHO);
        }
        self.cloexec_chirho[fd_chirho] = enabled_chirho;
        Ok(())
    }

    /// Return the `FD_CLOEXEC` state for an open file descriptor.
    pub fn get_cloexec_chirho(&self, fd_chirho: usize) -> Result<bool, i64> {
        if self.get_chirho(fd_chirho).is_none() {
            return Err(EBADF_CHIRHO);
        }
        self.cloexec_chirho
            .get(fd_chirho)
            .copied()
            .ok_or(EBADF_CHIRHO)
    }

    /// Drop all descriptors marked close-on-exec.
    pub fn close_cloexec_fds_chirho(&mut self) {
        for fd_chirho in 0..self.fds_chirho.len() {
            if self.fds_chirho[fd_chirho].is_some()
                && self.cloexec_chirho.get(fd_chirho).copied().unwrap_or(false)
            {
                // Use close_chirho() instead of just dropping the Arc.
                // close_chirho() properly decrements pipe writer/reader
                // counts and sets closed_write/closed_read when they
                // reach 0. Without this, inherited childpipe fds keep
                // writers_chirho > 0 after exec, preventing the parent's
                // select from detecting pipe EOF.
                let _ = self.close_chirho(fd_chirho);
            }
        }
    }

    /// Clear ALL O_CLOEXEC flags — every fd survives exec.
    /// Used for procfd exec (dropbear fexecve) where the connection fd
    /// must survive regardless of which fd number it's on.
    pub fn clear_all_cloexec_flags_chirho(&mut self) {
        for fd_chirho in 0..self.cloexec_chirho.len() {
            self.cloexec_chirho[fd_chirho] = false;
        }
    }

    /// Close all pipe fds > 2 that aren't stdin/stdout/stderr.
    /// Called during exec to release inherited parent pipe fds (e.g.,
    /// dropbear's childpipe) that don't have O_CLOEXEC set.
    pub fn close_non_stdio_pipes_chirho(&mut self) {
        // First pass: identify which fds are pipes
        let mut pipe_fds_chirho = alloc::vec::Vec::new();
        for fd_chirho in 3..self.fds_chirho.len() {
            if let Some(ref file_arc_chirho) = self.fds_chirho[fd_chirho] {
                let is_pipe_chirho = {
                    let fg_chirho = file_arc_chirho.lock();
                    let inode_guard_chirho = fg_chirho.inode_chirho.lock();
                    (inode_guard_chirho.mode_chirho & 0o170000) == 0o010000
                };
                if is_pipe_chirho {
                    pipe_fds_chirho.push(fd_chirho);
                }
            }
        }
        // Second pass: close them (modifies self)
        for fd_chirho in pipe_fds_chirho {
            let _ = self.close_chirho(fd_chirho);
        }
    }

    /// Return the lowest available descriptor slot.
    pub fn next_free_fd_chirho(&self) -> usize {
        self.fds_chirho
            .iter()
            .position(|slot_chirho| slot_chirho.is_none())
            .unwrap_or(self.fds_chirho.len())
    }

    /// Clone the entire file descriptor table.
    ///
    /// Each open file description (`Arc<Mutex<FileChirho>>`) is shared with the
    /// clone (matching POSIX fork semantics where parent and child share the
    /// underlying open file descriptions but have independent fd tables).
    pub fn clone_table_chirho(&self) -> Self {
        // Clone the fd vector (Arc refs are shared — POSIX fork semantics).
        let cloned_fds_chirho = self.fds_chirho.clone();

        // Every cloned slot is another live descriptor reference. The open
        // file description is shared, but pipe EOF/EPIPE lifetime is counted
        // per descriptor across both fd tables.
        for slot_chirho in &cloned_fds_chirho {
            if let Some(ref file_arc_chirho) = slot_chirho {
                increment_pipe_descriptor_reference_chirho(file_arc_chirho, "fork-clone");
            }
        }

        Self {
            fds_chirho: cloned_fds_chirho,
            paths_chirho: self.paths_chirho.clone(),
            cloexec_chirho: self.cloexec_chirho.clone(),
        }
    }

    /// Retire every live descriptor before releasing this table's ownership.
    ///
    /// This must run outside task-list, task, scheduler, and global-fd-table
    /// locks because closing pipe endpoints reaches File -> Inode -> Pipe.
    pub fn retire_all_descriptors_chirho(&mut self) {
        for fd_chirho in 0..self.fds_chirho.len() {
            if self.fds_chirho[fd_chirho].is_some() {
                let _ = self.close_chirho(fd_chirho);
            }
        }
    }
}

impl Drop for FdTableChirho {
    fn drop(&mut self) {
        // Both kernel profiles use panic=abort, so this never runs during
        // unwinding. It also must never acquire VFS locks: final ownership may
        // be released beneath an unrelated lock. Record a missed explicit
        // retirement using only an atomic, and report it from normal context.
        if self.fds_chirho.iter().any(Option::is_some) {
            saturating_atomic_increment_chirho(&UNRETIRED_FD_TABLE_DROP_COUNT_CHIRHO);
        }
    }
}

/// Report fd tables that bypassed explicit descriptor retirement.
pub fn report_unretired_fd_table_drops_chirho() {
    let dropped_count_chirho =
        UNRETIRED_FD_TABLE_DROP_COUNT_CHIRHO.swap(0, Ordering::AcqRel);
    if dropped_count_chirho != 0 {
        report_pipe_reference_invariant_chirho(
            "unretired-table-drop",
            dropped_count_chirho,
            0,
            false,
            false,
        );
    }
}

#[cfg(test)]
fn run_pipe_descriptor_reference_regression_chirho() -> Result<(), &'static str> {
    let (read_file_chirho, write_file_chirho) = crate::pipe_chirho::create_pipe_chirho();
    let pipe_arc_chirho = pipe_endpoint_reference_chirho(&read_file_chirho)
        .map(|pipe_state_chirho| pipe_state_chirho.0)
        .ok_or("new pipe did not expose shared state")?;
    let mut fd_table_chirho = FdTableChirho::new_chirho(16);
    fd_table_chirho.fds_chirho[0] = Some(read_file_chirho);
    fd_table_chirho.fds_chirho[1] = Some(write_file_chirho);

    let duplicated_read_fd_chirho = fd_table_chirho
        .dup_chirho(0)
        .map_err(|_| "dup of read endpoint failed")?;
    if duplicated_read_fd_chirho != 2 {
        return Err("dup did not choose the lowest available descriptor");
    }
    {
        let pipe_guard_chirho = pipe_arc_chirho.lock();
        if pipe_guard_chirho.readers_chirho != 2 || pipe_guard_chirho.closed_read_chirho {
            return Err("dup did not add a live reader reference");
        }
    }

    fd_table_chirho
        .close_chirho(0)
        .map_err(|_| "close of original reader failed")?;
    {
        let pipe_guard_chirho = pipe_arc_chirho.lock();
        if pipe_guard_chirho.readers_chirho != 1 || pipe_guard_chirho.closed_read_chirho {
            return Err("closing the original invalidated its duplicate");
        }
    }

    let payload_chirho = b"pipe-reference-chirho";
    let write_result_chirho = {
        let write_arc_chirho = fd_table_chirho
            .get_chirho(1)
            .ok_or("writer disappeared before live-reader write")?;
        let mut write_guard_chirho = write_arc_chirho.lock();
        write_guard_chirho
            .ops_chirho
            .write_chirho(&mut write_guard_chirho, payload_chirho)
    };
    if write_result_chirho != Ok(payload_chirho.len()) {
        return Err("writer returned EPIPE while a duplicate reader was live");
    }

    let mut read_buffer_chirho = [0u8; 32];
    let read_result_chirho = {
        let read_arc_chirho = fd_table_chirho
            .get_chirho(duplicated_read_fd_chirho)
            .ok_or("duplicate reader disappeared")?;
        let mut read_guard_chirho = read_arc_chirho.lock();
        read_guard_chirho
            .ops_chirho
            .read_chirho(&mut read_guard_chirho, &mut read_buffer_chirho)
    };
    if read_result_chirho != Ok(payload_chirho.len())
        || &read_buffer_chirho[..payload_chirho.len()] != payload_chirho
    {
        return Err("duplicate reader did not receive the written payload");
    }

    fd_table_chirho
        .dup2_chirho(1, 3)
        .map_err(|_| "dup2 of write endpoint failed")?;
    fd_table_chirho
        .close_chirho(1)
        .map_err(|_| "close of original writer failed")?;
    {
        let pipe_guard_chirho = pipe_arc_chirho.lock();
        if pipe_guard_chirho.writers_chirho != 1 || pipe_guard_chirho.closed_write_chirho {
            return Err("closing the original invalidated its duplicate writer");
        }
    }
    fd_table_chirho
        .close_chirho(3)
        .map_err(|_| "close of final writer failed")?;
    let eof_result_chirho = {
        let read_arc_chirho = fd_table_chirho
            .get_chirho(duplicated_read_fd_chirho)
            .ok_or("reader disappeared before EOF check")?;
        let mut read_guard_chirho = read_arc_chirho.lock();
        read_guard_chirho
            .ops_chirho
            .read_chirho(&mut read_guard_chirho, &mut read_buffer_chirho)
    };
    if eof_result_chirho != Ok(0) {
        return Err("reader did not observe EOF after the final writer closed");
    }

    let (second_read_file_chirho, second_write_file_chirho) =
        crate::pipe_chirho::create_pipe_chirho();
    let second_pipe_arc_chirho = pipe_endpoint_reference_chirho(&second_read_file_chirho)
        .map(|pipe_state_chirho| pipe_state_chirho.0)
        .ok_or("second pipe did not expose shared state")?;
    let mut second_fd_table_chirho = FdTableChirho::new_chirho(16);
    second_fd_table_chirho.fds_chirho[0] = Some(second_read_file_chirho);
    second_fd_table_chirho.fds_chirho[1] = Some(second_write_file_chirho);
    second_fd_table_chirho
        .dup2_chirho(0, 2)
        .map_err(|_| "dup2 of second reader failed")?;
    second_fd_table_chirho
        .close_chirho(0)
        .map_err(|_| "close of second original reader failed")?;

    let live_reader_write_chirho = {
        let write_arc_chirho = second_fd_table_chirho
            .get_chirho(1)
            .ok_or("second writer disappeared")?;
        let mut write_guard_chirho = write_arc_chirho.lock();
        write_guard_chirho
            .ops_chirho
            .write_chirho(&mut write_guard_chirho, b"R")
    };
    if live_reader_write_chirho != Ok(1) {
        return Err("writer returned EPIPE before the final reader closed");
    }
    let mut one_byte_chirho = [0u8; 1];
    {
        let read_arc_chirho = second_fd_table_chirho
            .get_chirho(2)
            .ok_or("second duplicate reader disappeared")?;
        let mut read_guard_chirho = read_arc_chirho.lock();
        if read_guard_chirho
            .ops_chirho
            .read_chirho(&mut read_guard_chirho, &mut one_byte_chirho)
            != Ok(1)
        {
            return Err("second duplicate reader did not drain its payload");
        }
    }
    second_fd_table_chirho
        .close_chirho(2)
        .map_err(|_| "close of final reader failed")?;
    {
        let pipe_guard_chirho = second_pipe_arc_chirho.lock();
        if pipe_guard_chirho.readers_chirho != 0 || !pipe_guard_chirho.closed_read_chirho {
            return Err("final reader close did not publish broken-pipe state");
        }
    }
    let broken_pipe_write_chirho = {
        let write_arc_chirho = second_fd_table_chirho
            .get_chirho(1)
            .ok_or("second writer disappeared before EPIPE check")?;
        let mut write_guard_chirho = write_arc_chirho.lock();
        write_guard_chirho
            .ops_chirho
            .write_chirho(&mut write_guard_chirho, b"E")
    };
    if broken_pipe_write_chirho != Err(-crate::syscall_chirho::EPIPE_CHIRHO) {
        return Err("writer did not return EPIPE after the final reader closed");
    }

    let (replacement_read_chirho, replacement_write_chirho) =
        crate::pipe_chirho::create_pipe_chirho();
    let replacement_pipe_arc_chirho = pipe_endpoint_reference_chirho(&replacement_read_chirho)
        .map(|pipe_state_chirho| pipe_state_chirho.0)
        .ok_or("replacement pipe did not expose shared state")?;
    second_fd_table_chirho.fds_chirho[4] = Some(replacement_read_chirho);
    second_fd_table_chirho.fds_chirho[5] = Some(replacement_write_chirho);
    second_fd_table_chirho
        .dup2_chirho(1, 4)
        .map_err(|_| "dup2 over an occupied destination failed")?;
    {
        let pipe_guard_chirho = replacement_pipe_arc_chirho.lock();
        if pipe_guard_chirho.readers_chirho != 0 || !pipe_guard_chirho.closed_read_chirho {
            return Err("dup2 did not retire its occupied destination descriptor");
        }
    }

    let mut cloned_fd_table_chirho = second_fd_table_chirho.clone_table_chirho();
    cloned_fd_table_chirho.retire_all_descriptors_chirho();
    fd_table_chirho.retire_all_descriptors_chirho();
    second_fd_table_chirho.retire_all_descriptors_chirho();
    if PIPE_REFERENCE_INVARIANT_COUNT_CHIRHO.load(Ordering::Relaxed) != 0 {
        return Err("pipe accounting reported an invariant violation");
    }
    if UNRETIRED_FD_TABLE_DROP_COUNT_CHIRHO.load(Ordering::Relaxed) != 0 {
        return Err("fd table bypassed explicit descriptor retirement");
    }
    Ok(())
}

#[cfg(test)]
mod pipe_descriptor_reference_tests_chirho {
    use super::run_pipe_descriptor_reference_regression_chirho;

    #[test]
    fn duplicate_close_eof_and_epipe_lifetimes_chirho() {
        assert_eq!(run_pipe_descriptor_reference_regression_chirho(), Ok(()));
    }
}
