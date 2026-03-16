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

/// `finit_module(2)` — load a kernel module from a file descriptor.
///
/// Reads the entire .ko ELF from the fd into a kernel buffer, then
/// delegates to init_module for the actual loading/relocation/init.
pub fn sys_finit_module_chirho(
    fd_chirho: u64,
    params_ptr_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    use alloc::vec;

    crate::serial_println_chirho!(
        "[MODULE] finit_module(fd={}, params={:#x})",
        fd_chirho, params_ptr_chirho
    );

    // Get the file size via fstat-like approach. Read up to 4MB max.
    const MAX_KO_SIZE_CHIRHO: usize = 4 * 1024 * 1024;
    let mut ko_buf_chirho = vec![0u8; MAX_KO_SIZE_CHIRHO];

    // Read the .ko file from the fd into our buffer.
    let bytes_read_chirho = crate::fs_chirho::sys_read_real_chirho(
        fd_chirho,
        ko_buf_chirho.as_mut_ptr() as u64,
        MAX_KO_SIZE_CHIRHO,
    );

    if bytes_read_chirho <= 0 {
        crate::serial_println_chirho!(
            "[MODULE] finit_module: failed to read from fd {}: {}",
            fd_chirho, bytes_read_chirho
        );
        return -ENOSYS_CHIRHO;
    }

    ko_buf_chirho.truncate(bytes_read_chirho as usize);

    crate::serial_println_chirho!(
        "[MODULE] finit_module: read {} bytes from fd {}",
        bytes_read_chirho, fd_chirho
    );

    // Delegate to init_module with the buffer pointer and length.
    crate::ko_loader_chirho::sys_init_module_impl_chirho(
        ko_buf_chirho.as_ptr() as u64,
        bytes_read_chirho as u64,
        params_ptr_chirho,
    )
}

/// `delete_module(2)` — unload a kernel module by name.
///
/// Delegates to [`crate::ko_loader_chirho::sys_delete_module_impl_chirho`].
pub fn sys_delete_module_chirho(name_ptr_chirho: u64, flags_chirho: u64) -> i64 {
    crate::ko_loader_chirho::sys_delete_module_impl_chirho(name_ptr_chirho, flags_chirho)
}
