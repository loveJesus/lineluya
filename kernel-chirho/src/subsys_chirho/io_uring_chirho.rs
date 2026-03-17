// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! io_uring implementation for the Lineluya kernel (A6-005).
//!
//! Implements the Linux io_uring async I/O interface with support for:
//! - `io_uring_setup(2)` — create an io_uring instance, return an fd
//! - `io_uring_enter(2)` — submit SQEs and optionally wait for CQEs
//! - `io_uring_register(2)` — register buffers and files
//!
//! Supported operations:
//! - `IORING_OP_NOP` — no-op (always succeeds with result 0)
//! - `IORING_OP_READ` — read from fd into buffer
//! - `IORING_OP_WRITE` — write from buffer to fd
//! - `IORING_OP_READV` / `IORING_OP_WRITEV` — scatter/gather I/O (stub)

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::syscall_chirho::{
    EBADF_CHIRHO, EINVAL_CHIRHO, EMFILE_CHIRHO, ENOMEM_CHIRHO,
};

// ============================================================================
// io_uring operation codes (matching Linux uapi)
// ============================================================================

/// No-op — always completes successfully.
pub const IORING_OP_NOP_CHIRHO: u8 = 0;
/// vectored read.
#[allow(dead_code)]
pub const IORING_OP_READV_CHIRHO: u8 = 1;
/// vectored write.
#[allow(dead_code)]
pub const IORING_OP_WRITEV_CHIRHO: u8 = 2;
/// fsync.
#[allow(dead_code)]
pub const IORING_OP_FSYNC_CHIRHO: u8 = 3;
/// read from fd at offset into fixed buffer.
pub const IORING_OP_READ_CHIRHO: u8 = 22;
/// write from fixed buffer to fd at offset.
pub const IORING_OP_WRITE_CHIRHO: u8 = 23;

// ============================================================================
// io_uring setup flags
// ============================================================================

/// IORING_SETUP_SQPOLL — kernel polls the SQ.
#[allow(dead_code)]
pub const IORING_SETUP_SQPOLL_CHIRHO: u32 = 1 << 1;

// ============================================================================
// io_uring enter flags
// ============================================================================

/// Wait for completions (IORING_ENTER_GETEVENTS).
pub const IORING_ENTER_GETEVENTS_CHIRHO: u32 = 1 << 0;
/// Wakeup SQ poll thread (IORING_ENTER_SQ_WAKEUP).
#[allow(dead_code)]
pub const IORING_ENTER_SQ_WAKEUP_CHIRHO: u32 = 1 << 1;

// ============================================================================
// Submission Queue Entry (SQE) — 64 bytes
// ============================================================================

/// io_uring Submission Queue Entry.
///
/// This is a simplified version of the Linux `io_uring_sqe` struct.
/// In a real implementation this would be 64 bytes mapped in shared memory.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IoUringSqeChirho {
    /// Operation code (IORING_OP_*).
    pub opcode_chirho: u8,
    /// Flags for this SQE.
    pub flags_chirho: u8,
    /// I/O priority.
    pub ioprio_chirho: u16,
    /// File descriptor.
    pub fd_chirho: i32,
    /// Offset for the operation.
    pub off_chirho: u64,
    /// User buffer address.
    pub addr_chirho: u64,
    /// Length of the operation.
    pub len_chirho: u32,
    /// Operation-specific flags.
    pub op_flags_chirho: u32,
    /// User data — returned in the CQE for correlation.
    pub user_data_chirho: u64,
}

// ============================================================================
// Completion Queue Entry (CQE) — 16 bytes
// ============================================================================

/// io_uring Completion Queue Entry.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IoUringCqeChirho {
    /// User data from the corresponding SQE.
    pub user_data_chirho: u64,
    /// Result of the operation (bytes transferred or -errno).
    pub res_chirho: i32,
    /// Flags.
    pub flags_chirho: u32,
}

// ============================================================================
// IoUringInstanceChirho — per-ring state
// ============================================================================

/// Represents a single io_uring instance.
#[allow(dead_code)]
pub struct IoUringInstanceChirho {
    /// Submission queue (pending SQEs to process).
    pub sq_chirho: Vec<IoUringSqeChirho>,
    /// Completion queue (completed CQEs for userspace to consume).
    pub cq_chirho: Vec<IoUringCqeChirho>,
    /// Maximum SQ entries.
    pub sq_size_chirho: u32,
    /// Maximum CQ entries (typically 2x SQ size).
    pub cq_size_chirho: u32,
    /// SQ head (consumer index — kernel consumes SQEs).
    pub sq_head_chirho: u32,
    /// SQ tail (producer index — userspace produces SQEs).
    pub sq_tail_chirho: u32,
    /// CQ head (consumer index — userspace consumes CQEs).
    pub cq_head_chirho: u32,
    /// CQ tail (producer index — kernel produces CQEs).
    pub cq_tail_chirho: u32,
    /// Setup flags.
    pub flags_chirho: u32,
    /// Whether this instance is active.
    pub active_chirho: bool,
}

impl IoUringInstanceChirho {
    /// Create a new io_uring instance with the given number of SQ entries.
    pub fn new_chirho(entries_chirho: u32, flags_chirho: u32) -> Self {
        let sq_size_chirho = entries_chirho.next_power_of_two();
        let cq_size_chirho = sq_size_chirho * 2;
        Self {
            sq_chirho: Vec::with_capacity(sq_size_chirho as usize),
            cq_chirho: Vec::with_capacity(cq_size_chirho as usize),
            sq_size_chirho,
            cq_size_chirho,
            sq_head_chirho: 0,
            sq_tail_chirho: 0,
            cq_head_chirho: 0,
            cq_tail_chirho: 0,
            flags_chirho,
            active_chirho: true,
        }
    }

    /// Submit an SQE to the ring.
    #[allow(dead_code)]
    pub fn submit_chirho(&mut self, sqe_chirho: IoUringSqeChirho) -> Result<(), i64> {
        if self.sq_chirho.len() >= self.sq_size_chirho as usize {
            return Err(-ENOMEM_CHIRHO);
        }
        self.sq_chirho.push(sqe_chirho);
        self.sq_tail_chirho = self.sq_tail_chirho.wrapping_add(1);
        Ok(())
    }

    /// Process all pending SQEs and generate CQEs.
    pub fn process_submissions_chirho(&mut self) -> u32 {
        let mut completed_chirho: u32 = 0;

        while let Some(sqe_chirho) = self.sq_chirho.pop() {
            self.sq_head_chirho = self.sq_head_chirho.wrapping_add(1);

            let cqe_chirho = self.execute_sqe_chirho(&sqe_chirho);
            if self.cq_chirho.len() < self.cq_size_chirho as usize {
                self.cq_chirho.push(cqe_chirho);
                self.cq_tail_chirho = self.cq_tail_chirho.wrapping_add(1);
            }
            completed_chirho += 1;
        }

        completed_chirho
    }

    /// Execute a single SQE and return the corresponding CQE.
    fn execute_sqe_chirho(&self, sqe_chirho: &IoUringSqeChirho) -> IoUringCqeChirho {
        let res_chirho = match sqe_chirho.opcode_chirho {
            IORING_OP_NOP_CHIRHO => {
                crate::serial_println_chirho!("[IO_URING] NOP completed");
                0i32
            }
            IORING_OP_READ_CHIRHO => {
                self.do_read_chirho(sqe_chirho)
            }
            IORING_OP_WRITE_CHIRHO => {
                self.do_write_chirho(sqe_chirho)
            }
            _ => {
                crate::serial_println_chirho!(
                    "[IO_URING] Unsupported opcode {}",
                    sqe_chirho.opcode_chirho
                );
                -(EINVAL_CHIRHO as i32)
            }
        };

        IoUringCqeChirho {
            user_data_chirho: sqe_chirho.user_data_chirho,
            res_chirho,
            flags_chirho: 0,
        }
    }

    /// Execute an IORING_OP_READ: read from fd into user buffer.
    fn do_read_chirho(&self, sqe_chirho: &IoUringSqeChirho) -> i32 {
        let fd_chirho = sqe_chirho.fd_chirho;
        let buf_addr_chirho = sqe_chirho.addr_chirho;
        let len_chirho = sqe_chirho.len_chirho;

        if buf_addr_chirho == 0 || len_chirho == 0 {
            return -(EINVAL_CHIRHO as i32);
        }

        crate::serial_println_chirho!(
            "[IO_URING] READ fd={} buf={:#x} len={}",
            fd_chirho, buf_addr_chirho, len_chirho,
        );

        // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
        let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64) {
            Some(f_chirho) => f_chirho,
            None => return -(EBADF_CHIRHO as i32),
        };

        let read_len_chirho = core::cmp::min(len_chirho as usize, 4096);
        let mut tmp_buf_chirho = alloc::vec![0u8; read_len_chirho];

        let mut file_guard_chirho = file_arc_chirho.lock();
        match file_guard_chirho.ops_chirho.read_chirho(&mut file_guard_chirho, &mut tmp_buf_chirho) {
            Ok(n_chirho) => {
                // Copy to user buffer
                let dst_ptr_chirho = buf_addr_chirho as *mut u8;
                for i_chirho in 0..n_chirho {
                    unsafe { core::ptr::write_volatile(dst_ptr_chirho.add(i_chirho), tmp_buf_chirho[i_chirho]) };
                }
                n_chirho as i32
            }
            Err(e_chirho) => e_chirho as i32,
        }
    }

    /// Execute an IORING_OP_WRITE: write from user buffer to fd.
    fn do_write_chirho(&self, sqe_chirho: &IoUringSqeChirho) -> i32 {
        let fd_chirho = sqe_chirho.fd_chirho;
        let buf_addr_chirho = sqe_chirho.addr_chirho;
        let len_chirho = sqe_chirho.len_chirho;

        if buf_addr_chirho == 0 || len_chirho == 0 {
            return -(EINVAL_CHIRHO as i32);
        }

        crate::serial_println_chirho!(
            "[IO_URING] WRITE fd={} buf={:#x} len={}",
            fd_chirho, buf_addr_chirho, len_chirho,
        );

        // Read data from user buffer
        let write_len_chirho = core::cmp::min(len_chirho as usize, 4096);
        let mut tmp_buf_chirho = alloc::vec![0u8; write_len_chirho];
        let src_ptr_chirho = buf_addr_chirho as *const u8;
        for i_chirho in 0..write_len_chirho {
            tmp_buf_chirho[i_chirho] = unsafe { core::ptr::read_volatile(src_ptr_chirho.add(i_chirho)) };
        }

        // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
        let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64) {
            Some(f_chirho) => f_chirho,
            None => return -(EBADF_CHIRHO as i32),
        };

        let mut file_guard_chirho = file_arc_chirho.lock();
        match file_guard_chirho.ops_chirho.write_chirho(&mut file_guard_chirho, &tmp_buf_chirho) {
            Ok(n_chirho) => n_chirho as i32,
            Err(e_chirho) => e_chirho as i32,
        }
    }

    /// Consume a CQE from the completion queue.
    #[allow(dead_code)]
    pub fn consume_cqe_chirho(&mut self) -> Option<IoUringCqeChirho> {
        if self.cq_chirho.is_empty() {
            return None;
        }
        let cqe_chirho = self.cq_chirho.remove(0);
        self.cq_head_chirho = self.cq_head_chirho.wrapping_add(1);
        Some(cqe_chirho)
    }
}

// ============================================================================
// Global io_uring instance table
// ============================================================================

/// Maximum number of concurrent io_uring instances.
const MAX_IO_URING_INSTANCES_CHIRHO: usize = 16;

/// Global io_uring table: maps ring fd -> instance.
static IO_URING_TABLE_CHIRHO: Mutex<[Option<IoUringInstanceChirho>; MAX_IO_URING_INSTANCES_CHIRHO]> = {
    const NONE_INSTANCE_CHIRHO: Option<IoUringInstanceChirho> = None;
    Mutex::new([NONE_INSTANCE_CHIRHO; MAX_IO_URING_INSTANCES_CHIRHO])
};

/// Atomic counter for io_uring fd allocation (start above normal fds).
static NEXT_URING_FD_CHIRHO: AtomicU64 = AtomicU64::new(100);

// ============================================================================
// io_uring_params — passed to/from userspace during setup
// ============================================================================

/// Simplified io_uring_params structure (matches Linux layout partially).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IoUringParamsChirho {
    /// SQ entries (in/out).
    pub sq_entries_chirho: u32,
    /// CQ entries (out).
    pub cq_entries_chirho: u32,
    /// Flags (in).
    pub flags_chirho: u32,
    /// SQ thread CPU (in, for SQPOLL).
    pub sq_thread_cpu_chirho: u32,
    /// SQ thread idle timeout in ms (in, for SQPOLL).
    pub sq_thread_idle_chirho: u32,
    /// Features supported by the kernel (out).
    pub features_chirho: u32,
}

// ============================================================================
// Syscall implementations
// ============================================================================

/// `io_uring_setup(2)` — create an io_uring instance.
///
/// # Arguments
/// * `entries_chirho` — requested SQ size (power of 2)
/// * `params_ptr_chirho` — pointer to user `io_uring_params`
///
/// # Returns
/// File descriptor for the io_uring instance, or negative errno.
pub fn sys_io_uring_setup_chirho(entries_chirho: u64, params_ptr_chirho: u64) -> i64 {
    if entries_chirho == 0 || entries_chirho > 4096 {
        return -EINVAL_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[IO_URING] io_uring_setup(entries={}, params={:#x})",
        entries_chirho, params_ptr_chirho,
    );

    // Read flags from params if pointer is valid
    let flags_chirho = if params_ptr_chirho != 0 {
        let ptr_chirho = params_ptr_chirho as *const IoUringParamsChirho;
        let params_chirho = unsafe { core::ptr::read_volatile(ptr_chirho) };
        params_chirho.flags_chirho
    } else {
        0
    };

    // Create the instance
    let instance_chirho = IoUringInstanceChirho::new_chirho(entries_chirho as u32, flags_chirho);
    let sq_size_chirho = instance_chirho.sq_size_chirho;
    let cq_size_chirho = instance_chirho.cq_size_chirho;

    // Allocate a slot in the global table
    let mut table_chirho = IO_URING_TABLE_CHIRHO.lock();
    let mut slot_idx_chirho: Option<usize> = None;
    for (idx_chirho, slot_chirho) in table_chirho.iter_mut().enumerate() {
        if slot_chirho.is_none() {
            *slot_chirho = Some(instance_chirho);
            slot_idx_chirho = Some(idx_chirho);
            break;
        }
    }

    let idx_chirho = match slot_idx_chirho {
        Some(i_chirho) => i_chirho,
        None => return -EMFILE_CHIRHO,
    };
    drop(table_chirho);

    // Write back params to userspace
    if params_ptr_chirho != 0 {
        let out_params_chirho = IoUringParamsChirho {
            sq_entries_chirho: sq_size_chirho,
            cq_entries_chirho: cq_size_chirho,
            flags_chirho,
            sq_thread_cpu_chirho: 0,
            sq_thread_idle_chirho: 0,
            features_chirho: 0, // No special features yet
        };
        let ptr_chirho = params_ptr_chirho as *mut IoUringParamsChirho;
        unsafe { core::ptr::write_volatile(ptr_chirho, out_params_chirho) };
    }

    // Return a pseudo-fd that maps to this slot
    let fd_chirho = NEXT_URING_FD_CHIRHO.fetch_add(1, Ordering::Relaxed) as i64;

    // Store the mapping: we use idx as part of the fd (low bits)
    // In a real kernel, we'd register this in the fd table properly.
    // For now, the fd encodes the slot index.
    let result_fd_chirho = (fd_chirho & !0xFF) | (idx_chirho as i64);

    crate::serial_println_chirho!(
        "[IO_URING] Created ring fd={} (slot={}, sq={}, cq={})",
        result_fd_chirho, idx_chirho, sq_size_chirho, cq_size_chirho,
    );

    result_fd_chirho
}

/// `io_uring_enter(2)` — submit SQEs and/or wait for CQEs.
///
/// # Arguments
/// * `fd_chirho` — io_uring file descriptor
/// * `to_submit_chirho` — number of SQEs to submit
/// * `min_complete_chirho` — minimum completions to wait for
/// * `flags_chirho` — IORING_ENTER_* flags
/// * `sig_chirho` — pointer to sigset (ignored)
///
/// # Returns
/// Number of SQEs submitted, or negative errno.
pub fn sys_io_uring_enter_chirho(
    fd_chirho: u64,
    to_submit_chirho: u64,
    min_complete_chirho: u64,
    flags_chirho: u64,
    _sig_chirho: u64,
) -> i64 {
    let slot_idx_chirho = (fd_chirho & 0xFF) as usize;

    if slot_idx_chirho >= MAX_IO_URING_INSTANCES_CHIRHO {
        return -EBADF_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[IO_URING] enter(fd={}, submit={}, min_complete={}, flags={:#x})",
        fd_chirho, to_submit_chirho, min_complete_chirho, flags_chirho,
    );

    let mut table_chirho = IO_URING_TABLE_CHIRHO.lock();
    let instance_chirho = match table_chirho[slot_idx_chirho].as_mut() {
        Some(i_chirho) => i_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Process any pending submissions
    let completed_chirho = instance_chirho.process_submissions_chirho();

    crate::serial_println_chirho!(
        "[IO_URING] Processed {} submissions, {} CQEs pending",
        completed_chirho, instance_chirho.cq_chirho.len(),
    );

    // If GETEVENTS flag is set, we would normally block until
    // min_complete CQEs are available. For now, return immediately.
    if (flags_chirho as u32 & IORING_ENTER_GETEVENTS_CHIRHO) != 0 {
        let available_chirho = instance_chirho.cq_chirho.len() as u64;
        if available_chirho < min_complete_chirho {
            crate::serial_println_chirho!(
                "[IO_URING] GETEVENTS: {} available, {} requested (non-blocking return)",
                available_chirho, min_complete_chirho,
            );
        }
    }

    completed_chirho as i64
}

/// `io_uring_register(2)` — register resources with an io_uring instance.
///
/// # Arguments
/// * `fd_chirho` — io_uring file descriptor
/// * `opcode_chirho` — registration operation
/// * `arg_chirho` — pointer to operation arguments
/// * `nr_args_chirho` — number of arguments
///
/// # Returns
/// 0 on success, or negative errno.
pub fn sys_io_uring_register_chirho(
    fd_chirho: u64,
    opcode_chirho: u64,
    _arg_chirho: u64,
    _nr_args_chirho: u64,
) -> i64 {
    let slot_idx_chirho = (fd_chirho & 0xFF) as usize;

    if slot_idx_chirho >= MAX_IO_URING_INSTANCES_CHIRHO {
        return -EBADF_CHIRHO;
    }

    crate::serial_println_chirho!(
        "[IO_URING] register(fd={}, opcode={})",
        fd_chirho, opcode_chirho,
    );

    let table_chirho = IO_URING_TABLE_CHIRHO.lock();
    if table_chirho[slot_idx_chirho].is_none() {
        return -EBADF_CHIRHO;
    }

    // Stub: accept all register operations
    0
}
