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
                // Take the Arc out of the slot so we can check strong_count.
                let file_arc_chirho = slot_chirho.take().unwrap();

                // Pipe close: decrement the reader/writer counter. Only set
                // closed_write/closed_read when the counter reaches 0.
                // The old Arc::strong_count check was unreliable because
                // multiple per-process fd tables clone the Arc independently,
                // and the global table isolation changed the refcount behavior.
                {
                    let file_guard_chirho = file_arc_chirho.lock();
                    let is_fifo_chirho = (file_guard_chirho.inode_chirho.lock().mode_chirho & 0o170000) == 0o010000;
                    if is_fifo_chirho {
                        let flags_chirho = file_guard_chirho.flags_chirho;
                        if let Some(ref fs_data_chirho) = file_guard_chirho.inode_chirho.lock().fs_data_chirho {
                            if let Some(pipe_arc_chirho) = fs_data_chirho
                                .downcast_ref::<alloc::sync::Arc<spin::Mutex<crate::pipe_chirho::PipeChirho>>>()
                            {
                                let mut pipe_chirho = pipe_arc_chirho.lock();
                                if flags_chirho & O_WRONLY_CHIRHO != 0 || flags_chirho & O_RDWR_CHIRHO != 0 {
                                    if pipe_chirho.writers_chirho > 0 {
                                        pipe_chirho.writers_chirho -= 1;
                                    }
                                    if pipe_chirho.writers_chirho == 0 {
                                        pipe_chirho.closed_write_chirho = true;
                                    }
                                }
                                if flags_chirho == O_RDONLY_CHIRHO {
                                    if pipe_chirho.readers_chirho > 0 {
                                        pipe_chirho.readers_chirho -= 1;
                                    }
                                    if pipe_chirho.readers_chirho == 0 {
                                        pipe_chirho.closed_read_chirho = true;
                                    }
                                }
                            }
                        }
                    }
                    drop(file_guard_chirho);
                }
                // Arc drops here (file_arc_chirho goes out of scope)
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
        self.fds_chirho[new_fd_chirho] = Some(file_chirho);
        if new_fd_chirho < self.paths_chirho.len() && old_fd_chirho < self.paths_chirho.len() {
            self.paths_chirho[new_fd_chirho] = self.paths_chirho[old_fd_chirho].clone();
        }
        if new_fd_chirho < self.cloexec_chirho.len() {
            self.cloexec_chirho[new_fd_chirho] = false;
        }
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

        // Increment pipe reader/writer counters for each cloned pipe fd.
        // Without this, close_chirho's decrement reaches 0 too early,
        // setting closed_write/closed_read while the other process still
        // has the pipe open — causing spurious EOF on select().
        for slot_chirho in &cloned_fds_chirho {
            if let Some(ref file_arc_chirho) = slot_chirho {
                let file_guard_chirho = file_arc_chirho.lock();
                let is_fifo_chirho = (file_guard_chirho.inode_chirho.lock().mode_chirho & 0o170000) == 0o010000;
                if is_fifo_chirho {
                    let flags_chirho = file_guard_chirho.flags_chirho;
                    if let Some(ref fs_data_chirho) = file_guard_chirho.inode_chirho.lock().fs_data_chirho {
                        if let Some(pipe_arc_chirho) = fs_data_chirho
                            .downcast_ref::<alloc::sync::Arc<spin::Mutex<crate::pipe_chirho::PipeChirho>>>()
                        {
                            let mut pipe_chirho = pipe_arc_chirho.lock();
                            if flags_chirho & O_WRONLY_CHIRHO != 0 || flags_chirho & O_RDWR_CHIRHO != 0 {
                                pipe_chirho.writers_chirho += 1;
                            }
                            if flags_chirho == O_RDONLY_CHIRHO {
                                pipe_chirho.readers_chirho += 1;
                            }
                        }
                    }
                }
            }
        }

        Self {
            fds_chirho: cloned_fds_chirho,
            paths_chirho: self.paths_chirho.clone(),
            cloexec_chirho: self.cloexec_chirho.clone(),
        }
    }

    /// Decrement pipe reader/writer counters for all pipe fds in this table.
    /// Called when this table is about to be REPLACED (not closed fd-by-fd).
    /// The Arcs will be dropped without going through close_chirho, so the
    /// pipe refcounts would be left inflated. This compensates.
    pub fn dec_pipe_refcounts_chirho(&self) {
        for slot_chirho in &self.fds_chirho {
            if let Some(ref file_arc_chirho) = slot_chirho {
                let file_guard_chirho = file_arc_chirho.lock();
                let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
                let is_fifo_chirho = (inode_guard_chirho.mode_chirho & 0o170000) == 0o010000;
                if is_fifo_chirho {
                    let flags_chirho = file_guard_chirho.flags_chirho;
                    if let Some(ref fs_data_chirho) = inode_guard_chirho.fs_data_chirho {
                        if let Some(pipe_arc_chirho) = fs_data_chirho
                            .downcast_ref::<alloc::sync::Arc<spin::Mutex<crate::pipe_chirho::PipeChirho>>>()
                        {
                            let mut pipe_chirho = pipe_arc_chirho.lock();
                            if flags_chirho & O_WRONLY_CHIRHO != 0 || flags_chirho & O_RDWR_CHIRHO != 0 {
                                if pipe_chirho.writers_chirho > 0 {
                                    pipe_chirho.writers_chirho -= 1;
                                }
                                if pipe_chirho.writers_chirho == 0 {
                                    pipe_chirho.closed_write_chirho = true;
                                }
                            }
                            if flags_chirho == O_RDONLY_CHIRHO {
                                if pipe_chirho.readers_chirho > 0 {
                                    pipe_chirho.readers_chirho -= 1;
                                }
                                if pipe_chirho.readers_chirho == 0 {
                                    pipe_chirho.closed_read_chirho = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
