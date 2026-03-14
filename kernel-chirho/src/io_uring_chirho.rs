// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! io_uring stub for the Lineluya kernel.
//!
//! Provides placeholder syscall handlers for `io_uring_setup`, `io_uring_enter`,
//! and `io_uring_register`.  All return `-ENOSYS` until real async I/O is
//! implemented.

use crate::syscall_chirho::ENOSYS_CHIRHO;

// ============================================================================
// io_uring structures
// ============================================================================

/// Placeholder io_uring instance descriptor.
#[allow(dead_code)]
pub struct IoUringChirho {
    /// Number of submission queue entries.
    pub sq_entries_chirho: u32,
    /// Number of completion queue entries.
    pub cq_entries_chirho: u32,
    /// Setup flags (e.g. IORING_SETUP_SQPOLL).
    pub flags_chirho: u32,
}

// ============================================================================
// Syscall stubs
// ============================================================================

/// `io_uring_setup(2)` stub.
///
/// # Arguments
/// * `_entries_chirho` — requested SQ size
/// * `_params_ptr_chirho` — pointer to user `io_uring_params`
///
/// # Returns
/// Always `-ENOSYS`; io_uring is not yet implemented.
pub fn sys_io_uring_setup_chirho(_entries_chirho: u64, _params_ptr_chirho: u64) -> i64 {
    crate::serial_println_chirho!(
        "[IO_URING] io_uring_setup() -- io_uring not yet implemented"
    );
    -ENOSYS_CHIRHO
}

/// `io_uring_enter(2)` stub.
///
/// # Arguments
/// * `_fd_chirho` — io_uring file descriptor
/// * `_to_submit_chirho` — number of SQEs to submit
/// * `_min_complete_chirho` — minimum completions to wait for
/// * `_flags_chirho` — IORING_ENTER_* flags
/// * `_sig_chirho` — pointer to sigset
///
/// # Returns
/// Always `-ENOSYS`.
pub fn sys_io_uring_enter_chirho(
    _fd_chirho: u64,
    _to_submit_chirho: u64,
    _min_complete_chirho: u64,
    _flags_chirho: u64,
    _sig_chirho: u64,
) -> i64 {
    -ENOSYS_CHIRHO
}

/// `io_uring_register(2)` stub.
///
/// # Arguments
/// * `_fd_chirho` — io_uring file descriptor
/// * `_opcode_chirho` — registration operation
/// * `_arg_chirho` — pointer to operation arguments
/// * `_nr_args_chirho` — number of arguments
///
/// # Returns
/// Always `-ENOSYS`.
pub fn sys_io_uring_register_chirho(
    _fd_chirho: u64,
    _opcode_chirho: u64,
    _arg_chirho: u64,
    _nr_args_chirho: u64,
) -> i64 {
    -ENOSYS_CHIRHO
}
