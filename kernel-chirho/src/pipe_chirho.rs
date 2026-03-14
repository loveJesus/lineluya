// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Pipe implementation for the Lineluya kernel.
//!
//! Provides a unidirectional byte stream (equivalent to Linux's `fs/pipe.c`).
//! Each pipe has a read end and a write end, backed by a shared ring buffer.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;

use crate::vfs_chirho::{FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho};
use crate::syscall_chirho::{EBADF_CHIRHO, EPIPE_CHIRHO, ENOSYS_CHIRHO, EINVAL_CHIRHO, EFAULT_CHIRHO};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default pipe buffer capacity in bytes (matches Linux's default).
const PIPE_BUF_SIZE_CHIRHO: usize = 4096;

// ---------------------------------------------------------------------------
// PipeChirho — shared pipe state
// ---------------------------------------------------------------------------

/// Shared state for a pipe, protected by a mutex and reference-counted.
///
/// Both the read and write ends hold an `Arc` to the same `PipeChirho`.
pub struct PipeChirho {
    /// Ring buffer holding unread bytes.
    pub buffer_chirho: VecDeque<u8>,
    /// Number of open read-end file descriptors.
    pub readers_chirho: u32,
    /// Number of open write-end file descriptors.
    pub writers_chirho: u32,
    /// Whether the read end has been closed.
    pub closed_read_chirho: bool,
    /// Whether the write end has been closed.
    pub closed_write_chirho: bool,
}

impl PipeChirho {
    /// Create a new pipe with default buffer capacity.
    pub fn new_chirho() -> Self {
        Self {
            buffer_chirho: VecDeque::with_capacity(PIPE_BUF_SIZE_CHIRHO),
            readers_chirho: 1,
            writers_chirho: 1,
            closed_read_chirho: false,
            closed_write_chirho: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Dummy inode for pipe file descriptions
// ---------------------------------------------------------------------------

/// Minimal inode ops for pipe inodes (pipes don't support directory operations).
struct PipeInodeOpsChirho;

impl InodeOpsChirho for PipeInodeOpsChirho {
    fn lookup_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<alloc::string::String, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static inode ops instance for pipe inodes.
static PIPE_INODE_OPS_CHIRHO: PipeInodeOpsChirho = PipeInodeOpsChirho;

/// Create a dummy inode suitable for a pipe file description.
fn make_pipe_inode_chirho() -> Arc<Mutex<InodeChirho>> {
    use core::sync::atomic::{AtomicU64, Ordering};
    static PIPE_INO_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0x8000_0000);

    Arc::new(Mutex::new(InodeChirho {
        ino_chirho: PIPE_INO_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed),
        mode_chirho: 0o010600, // S_IFIFO | rw for owner
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 1,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &PIPE_INODE_OPS_CHIRHO,
        fs_data_chirho: None,
    }))
}

// ---------------------------------------------------------------------------
// PipeReadOpsChirho — file operations for the read end
// ---------------------------------------------------------------------------

/// File operations for the read end of a pipe.
pub struct PipeReadOpsChirho {
    /// Shared pipe state.
    pub pipe_chirho: Arc<Mutex<PipeChirho>>,
}

impl FileOpsChirho for PipeReadOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        let mut pipe_chirho = self.pipe_chirho.lock();

        if pipe_chirho.buffer_chirho.is_empty() {
            if pipe_chirho.closed_write_chirho {
                // Write end closed and buffer empty => EOF.
                return Ok(0);
            }
            // In a real kernel we would block here; for now return 0
            // (non-blocking / no data available).
            return Ok(0);
        }

        let to_read_chirho = buf_chirho.len().min(pipe_chirho.buffer_chirho.len());
        for i_chirho in 0..to_read_chirho {
            buf_chirho[i_chirho] = pipe_chirho.buffer_chirho.pop_front().unwrap();
        }
        Ok(to_read_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // Cannot write to the read end of a pipe.
        Err(-EBADF_CHIRHO)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-crate::syscall_chirho::ESPIPE_CHIRHO)
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
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-ENOSYS_CHIRHO)
    }
}

// ---------------------------------------------------------------------------
// PipeWriteOpsChirho — file operations for the write end
// ---------------------------------------------------------------------------

/// File operations for the write end of a pipe.
pub struct PipeWriteOpsChirho {
    /// Shared pipe state.
    pub pipe_chirho: Arc<Mutex<PipeChirho>>,
}

impl FileOpsChirho for PipeWriteOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // Cannot read from the write end of a pipe.
        Err(-EBADF_CHIRHO)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let mut pipe_chirho = self.pipe_chirho.lock();

        if pipe_chirho.closed_read_chirho {
            // Read end closed => broken pipe.
            return Err(-EPIPE_CHIRHO);
        }

        // Append data to the ring buffer (may grow beyond PIPE_BUF_SIZE_CHIRHO
        // in this simplified implementation; a production kernel would block
        // when the buffer is full).
        for &byte_chirho in buf_chirho {
            pipe_chirho.buffer_chirho.push_back(byte_chirho);
        }
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-crate::syscall_chirho::ESPIPE_CHIRHO)
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
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-ENOSYS_CHIRHO)
    }
}

// We need Send + Sync for the file ops since they contain Arc<Mutex<..>>
// which is already Send + Sync.
unsafe impl Send for PipeReadOpsChirho {}
unsafe impl Sync for PipeReadOpsChirho {}
unsafe impl Send for PipeWriteOpsChirho {}
unsafe impl Sync for PipeWriteOpsChirho {}

// ---------------------------------------------------------------------------
// create_pipe_chirho — factory function
// ---------------------------------------------------------------------------

/// Create a new pipe, returning (read_end, write_end) as open file descriptions.
///
/// The caller is responsible for installing these into a process's fd table.
pub fn create_pipe_chirho() -> (Arc<Mutex<FileChirho>>, Arc<Mutex<FileChirho>>) {
    let pipe_state_chirho = Arc::new(Mutex::new(PipeChirho::new_chirho()));
    let inode_chirho = make_pipe_inode_chirho();

    // Leak the file ops so they have 'static lifetime as required by FileChirho.
    let read_ops_chirho: &'static dyn FileOpsChirho = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        PipeReadOpsChirho {
            pipe_chirho: Arc::clone(&pipe_state_chirho),
        },
    ));

    let write_ops_chirho: &'static dyn FileOpsChirho = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        PipeWriteOpsChirho {
            pipe_chirho: Arc::clone(&pipe_state_chirho),
        },
    ));

    let read_file_chirho = Arc::new(Mutex::new(FileChirho {
        inode_chirho: Arc::clone(&inode_chirho),
        pos_chirho: 0,
        flags_chirho: crate::vfs_chirho::O_RDONLY_CHIRHO,
        ops_chirho: read_ops_chirho,
    }));

    let write_file_chirho = Arc::new(Mutex::new(FileChirho {
        inode_chirho,
        pos_chirho: 0,
        flags_chirho: crate::vfs_chirho::O_WRONLY_CHIRHO,
        ops_chirho: write_ops_chirho,
    }));

    (read_file_chirho, write_file_chirho)
}

// ---------------------------------------------------------------------------
// Syscall entry points
// ---------------------------------------------------------------------------

/// `pipe(int pipefd[2])` — create a pipe.
///
/// Writes the read-end fd to `pipefd[0]` and the write-end fd to `pipefd[1]`.
/// Returns 0 on success, negative errno on failure.
pub fn sys_pipe_chirho(fds_ptr_chirho: u64) -> i64 {
    if fds_ptr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    crate::serial_println_chirho!("[PIPE] sys_pipe called (fds_ptr={:#x})", fds_ptr_chirho);

    // In a full implementation we would:
    // 1. Create the pipe via create_pipe_chirho()
    // 2. Install both ends into the current task's fd table
    // 3. Write the fd numbers to the user-space array at fds_ptr_chirho
    //
    // For now, since we don't have a per-task fd table wired in yet,
    // return -ENOSYS as a stub.
    -ENOSYS_CHIRHO
}

/// `pipe2(int pipefd[2], int flags)` — create a pipe with flags.
///
/// Like `sys_pipe_chirho` but accepts `O_CLOEXEC`, `O_NONBLOCK`, etc.
pub fn sys_pipe2_chirho(fds_ptr_chirho: u64, flags_chirho: u32) -> i64 {
    if fds_ptr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[PIPE] sys_pipe2 called (fds_ptr={:#x}, flags={:#x})",
        fds_ptr_chirho,
        flags_chirho,
    );

    // Stub — same rationale as sys_pipe_chirho.
    -ENOSYS_CHIRHO
}
