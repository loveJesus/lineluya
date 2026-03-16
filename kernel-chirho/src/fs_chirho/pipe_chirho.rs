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
            // Spin-wait for data: drop the lock, yield to the scheduler,
            // then re-acquire and check again. This allows the writer
            // (in a separate task or after a context switch) to produce
            // data. Limited to 100K iterations to prevent infinite hang
            // in single-task vfork mode.
            drop(pipe_chirho);
            for _retry_chirho in 0..100_000u32 {
                core::hint::spin_loop();
                let pipe_recheck_chirho = self.pipe_chirho.lock();
                if !pipe_recheck_chirho.buffer_chirho.is_empty() {
                    // Data arrived — fall through to the copy-out path.
                    let to_read_chirho = buf_chirho.len().min(pipe_recheck_chirho.buffer_chirho.len());
                    // Need mutable access to drain the buffer.
                    drop(pipe_recheck_chirho);
                    let mut pipe_drain_chirho = self.pipe_chirho.lock();
                    let actual_chirho = buf_chirho.len().min(pipe_drain_chirho.buffer_chirho.len());
                    for i_chirho in 0..actual_chirho {
                        buf_chirho[i_chirho] = pipe_drain_chirho.buffer_chirho.pop_front().unwrap();
                    }
                    return Ok(actual_chirho);
                }
                if pipe_recheck_chirho.closed_write_chirho {
                    return Ok(0); // Writer closed while we were waiting.
                }
                drop(pipe_recheck_chirho);
            }
            return Ok(0); // Timeout — no data arrived.
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

/// Internal helper: create a pipe and install both ends into the global
/// fd table, writing the two fd numbers to the user-space `int[2]` array
/// at `fds_ptr_chirho`.
fn pipe_common_chirho(fds_ptr_chirho: u64) -> i64 {
    use crate::fs_chirho::GLOBAL_FD_TABLE_CHIRHO;

    // 1. Create the pipe (read_end, write_end).
    let (read_file_chirho, write_file_chirho) = create_pipe_chirho();

    // 2. Install into the global fd table.
    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
        Some(t_chirho) => t_chirho,
        None => return -EBADF_CHIRHO,
    };

    let read_fd_chirho = match fd_table_chirho.alloc_fd_chirho() {
        Ok(fd_chirho) => fd_chirho,
        Err(e_chirho) => return e_chirho,
    };
    fd_table_chirho.fds_chirho[read_fd_chirho] = Some(read_file_chirho);

    let write_fd_chirho = match fd_table_chirho.alloc_fd_chirho() {
        Ok(fd_chirho) => fd_chirho,
        Err(e_chirho) => {
            // Roll back the read fd.
            fd_table_chirho.fds_chirho[read_fd_chirho] = None;
            return e_chirho;
        }
    };
    fd_table_chirho.fds_chirho[write_fd_chirho] = Some(write_file_chirho);

    // Drop the lock before writing to userspace.
    drop(fd_table_guard_chirho);

    // 3. Write the two fd numbers to user space as int[2] (i32[2]).
    let fds_array_chirho: [i32; 2] = [read_fd_chirho as i32, write_fd_chirho as i32];
    let src_bytes_chirho = unsafe {
        core::slice::from_raw_parts(
            fds_array_chirho.as_ptr() as *const u8,
            core::mem::size_of::<[i32; 2]>(),
        )
    };
    if crate::uaccess_chirho::copy_to_user_chirho(
        fds_ptr_chirho,
        src_bytes_chirho,
        core::mem::size_of::<[i32; 2]>(),
    )
    .is_err()
    {
        // Roll back both fds on write failure.
        let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        if let Some(t_chirho) = fd_table_guard_chirho.as_mut() {
            t_chirho.fds_chirho[read_fd_chirho] = None;
            t_chirho.fds_chirho[write_fd_chirho] = None;
        }
        return -EFAULT_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[PIPE] pipe created: read_fd={}, write_fd={}",
        read_fd_chirho,
        write_fd_chirho,
    );

    0
}

/// `pipe(int pipefd[2])` — create a pipe.
///
/// Writes the read-end fd to `pipefd[0]` and the write-end fd to `pipefd[1]`.
/// Returns 0 on success, negative errno on failure.
pub fn sys_pipe_chirho(fds_ptr_chirho: u64) -> i64 {
    if fds_ptr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    crate::serial_println_chirho!("[PIPE] sys_pipe called (fds_ptr={:#x})", fds_ptr_chirho);
    pipe_common_chirho(fds_ptr_chirho)
}

/// `pipe2(int pipefd[2], int flags)` — create a pipe with flags.
///
/// Like `sys_pipe_chirho` but accepts `O_CLOEXEC`, `O_NONBLOCK`, etc.
/// Flags are accepted but not yet enforced (BusyBox just needs the pipe to exist).
pub fn sys_pipe2_chirho(fds_ptr_chirho: u64, flags_chirho: u32) -> i64 {
    if fds_ptr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[PIPE] sys_pipe2 called (fds_ptr={:#x}, flags={:#x})",
        fds_ptr_chirho,
        flags_chirho,
    );

    // Flags (O_CLOEXEC, O_NONBLOCK) are accepted but not enforced yet.
    let _ = flags_chirho;
    pipe_common_chirho(fds_ptr_chirho)
}
