// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! vDSO (virtual Dynamic Shared Object) stub for the Lineluya kernel.
//!
//! The vDSO is a small shared library that the kernel maps into every user
//! process's address space.  It allows certain syscalls (e.g.
//! `clock_gettime`, `gettimeofday`) to execute entirely in userspace by
//! reading kernel-maintained shared memory, avoiding the cost of a real
//! syscall transition.
//!
//! This module provides placeholder constants and an init function.
//! The actual vDSO ELF blob and shared-page mapping are not yet implemented.

/// Base virtual address where the vDSO will be mapped in every user process.
///
/// This sits just below the top of the lower-half canonical address space,
/// a region commonly used by Linux for the vDSO on x86_64.
#[allow(dead_code)]
pub const VDSO_BASE_CHIRHO: u64 = 0x7FFE_0000_0000;

/// Size of the vDSO mapping (one 4 KiB page for now).
#[allow(dead_code)]
pub const VDSO_SIZE_CHIRHO: u64 = 0x1000;

/// Initialize the vDSO subsystem.
///
/// Currently a stub — logs a message and returns.  A real implementation
/// would:
/// 1. Build or embed a small ELF shared object containing
///    `__vdso_clock_gettime` and `__vdso_gettimeofday`.
/// 2. Allocate a physical page, copy the ELF into it, and set up a
///    shared data page (updated by the timer interrupt) so userspace
///    can read the current time without a syscall.
/// 3. Record the mapping so that `exec` maps it into every new process.
pub fn init_vdso_chirho() {
    crate::serial_println_chirho!("[VDSO] vDSO not yet mapped (stub)");
}
