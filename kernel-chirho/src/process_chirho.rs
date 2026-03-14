// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Process creation syscall stubs for the Lineluya kernel.
//!
//! Provides stub implementations for `fork`, `clone`, `execve`, and `wait4`.
//! Full copy-on-write (COW) fork is deferred to P3-012; these stubs return
//! appropriate error codes so that userspace gets a clear signal that the
//! functionality is not yet available.

use crate::syscall_chirho::{ENOSYS_CHIRHO, ECHILD_CHIRHO};

// ---------------------------------------------------------------------------
// Clone flag constants (from Linux <linux/sched.h>)
// ---------------------------------------------------------------------------

/// Share the virtual memory space with the parent.
pub const CLONE_VM_CHIRHO: u64 = 0x0000_0100;
/// Share the filesystem information (cwd, root, umask).
pub const CLONE_FS_CHIRHO: u64 = 0x0000_0200;
/// Share the file descriptor table.
pub const CLONE_FILES_CHIRHO: u64 = 0x0000_0400;
/// Share signal handlers.
pub const CLONE_SIGHAND_CHIRHO: u64 = 0x0000_0800;
/// Create a new thread (same thread group).
pub const CLONE_THREAD_CHIRHO: u64 = 0x0001_0000;
/// Create a new mount namespace.
pub const CLONE_NEWNS_CHIRHO: u64 = 0x0002_0000;

// ---------------------------------------------------------------------------
// Syscall stubs
// ---------------------------------------------------------------------------

/// `fork()` — create a child process.
///
/// **Stub**: returns `-ENOSYS`.  Full COW fork is planned for P3-012.
pub fn sys_fork_chirho() -> i64 {
    crate::serial_println_chirho!(
        "[PROCESS] sys_fork called — stub, returning -ENOSYS (not yet implemented, see P3-012)"
    );
    -ENOSYS_CHIRHO
}

/// `clone(flags, stack, parent_tid, child_tid, tls)` — create a child
/// process/thread with fine-grained sharing control.
///
/// **Stub**: returns `-ENOSYS`.
pub fn sys_clone_chirho(
    flags_chirho: u64,
    stack_chirho: u64,
    parent_tid_chirho: u64,
    child_tid_chirho: u64,
    tls_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[PROCESS] sys_clone called — stub (flags={:#x}, stack={:#x}, ptid={:#x}, ctid={:#x}, tls={:#x})",
        flags_chirho,
        stack_chirho,
        parent_tid_chirho,
        child_tid_chirho,
        tls_chirho,
    );
    -ENOSYS_CHIRHO
}

/// `execve(filename, argv, envp)` — execute a new program.
///
/// **Stub**: returns `-ENOSYS`.
pub fn sys_execve_chirho(
    filename_chirho: u64,
    argv_chirho: u64,
    envp_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[PROCESS] sys_execve called — stub (filename={:#x}, argv={:#x}, envp={:#x})",
        filename_chirho,
        argv_chirho,
        envp_chirho,
    );
    -ENOSYS_CHIRHO
}

/// `wait4(pid, wstatus, options, rusage)` — wait for a child process.
///
/// **Stub**: returns `-ECHILD` (no child processes).
pub fn sys_wait4_chirho(
    pid_chirho: i64,
    wstatus_chirho: u64,
    options_chirho: u32,
    rusage_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[PROCESS] sys_wait4 called — stub (pid={}, wstatus={:#x}, options={:#x}, rusage={:#x})",
        pid_chirho,
        wstatus_chirho,
        options_chirho,
        rusage_chirho,
    );
    -ECHILD_CHIRHO
}
