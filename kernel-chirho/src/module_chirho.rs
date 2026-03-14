// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel module loading stubs for the Lineluya kernel.
//!
//! Provides placeholder syscall handlers for `init_module`, `finit_module`,
//! and `delete_module`.  All return `-ENOSYS` until real loadable kernel
//! module support is implemented.

use crate::syscall_chirho::ENOSYS_CHIRHO;

// ============================================================================
// Syscall stubs
// ============================================================================

/// `init_module(2)` stub — load a kernel module from a memory image.
///
/// # Arguments
/// * `_img_ptr_chirho` — pointer to the module image in user memory
/// * `_len_chirho` — length of the module image in bytes
/// * `_params_ptr_chirho` — pointer to null-terminated parameter string
///
/// # Returns
/// Always `-ENOSYS`; module loading is not yet implemented.
pub fn sys_init_module_chirho(
    _img_ptr_chirho: u64,
    _len_chirho: u64,
    _params_ptr_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[MODULE] init_module() -- kernel modules not yet implemented"
    );
    -ENOSYS_CHIRHO
}

/// `finit_module(2)` stub — load a kernel module from a file descriptor.
///
/// # Arguments
/// * `_fd_chirho` — file descriptor referring to the module file
/// * `_params_ptr_chirho` — pointer to null-terminated parameter string
/// * `_flags_chirho` — flags (e.g. MODULE_INIT_IGNORE_MODVERSIONS)
///
/// # Returns
/// Always `-ENOSYS`.
pub fn sys_finit_module_chirho(
    _fd_chirho: u64,
    _params_ptr_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[MODULE] finit_module() -- kernel modules not yet implemented"
    );
    -ENOSYS_CHIRHO
}

/// `delete_module(2)` stub — unload a kernel module.
///
/// # Arguments
/// * `_name_ptr_chirho` — pointer to the module name string
/// * `_flags_chirho` — flags (e.g. O_NONBLOCK)
///
/// # Returns
/// Always `-ENOSYS`.
pub fn sys_delete_module_chirho(_name_ptr_chirho: u64, _flags_chirho: u64) -> i64 {
    crate::serial_println_chirho!(
        "[MODULE] delete_module() -- kernel modules not yet implemented"
    );
    -ENOSYS_CHIRHO
}
