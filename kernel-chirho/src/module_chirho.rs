// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel module loading syscall handlers for the Lineluya kernel.
//!
//! Routes `init_module`, `finit_module`, and `delete_module` syscalls to the
//! `.ko` ELF loader in [`crate::ko_loader_chirho`].

use crate::syscall_chirho::ENOSYS_CHIRHO;

// ============================================================================
// Syscall handlers
// ============================================================================

/// `init_module(2)` — load a kernel module from a memory image.
///
/// Delegates to [`crate::ko_loader_chirho::sys_init_module_impl_chirho`].
pub fn sys_init_module_chirho(
    img_ptr_chirho: u64,
    len_chirho: u64,
    params_ptr_chirho: u64,
) -> i64 {
    crate::ko_loader_chirho::sys_init_module_impl_chirho(
        img_ptr_chirho,
        len_chirho,
        params_ptr_chirho,
    )
}

/// `finit_module(2)` stub — load a kernel module from a file descriptor.
///
/// Not yet implemented; returns `-ENOSYS`.
pub fn sys_finit_module_chirho(
    _fd_chirho: u64,
    _params_ptr_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[MODULE] finit_module() -- not yet implemented (use init_module)"
    );
    -ENOSYS_CHIRHO
}

/// `delete_module(2)` — unload a kernel module by name.
///
/// Delegates to [`crate::ko_loader_chirho::sys_delete_module_impl_chirho`].
pub fn sys_delete_module_chirho(name_ptr_chirho: u64, flags_chirho: u64) -> i64 {
    crate::ko_loader_chirho::sys_delete_module_impl_chirho(name_ptr_chirho, flags_chirho)
}
