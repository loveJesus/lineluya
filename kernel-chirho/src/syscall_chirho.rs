// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux-compatible syscall dispatch module for the Lineluya kernel (x86_64).
//!
//! Implements the Linux x86_64 syscall ABI:
//!
//! | Register | Purpose                         |
//! |----------|---------------------------------|
//! | `rax`    | Syscall number (in) / return (out) |
//! | `rdi`    | Argument 0                      |
//! | `rsi`    | Argument 1                      |
//! | `rdx`    | Argument 2                      |
//! | `r10`    | Argument 3                      |
//! | `r8`     | Argument 4                      |
//! | `r9`     | Argument 5                      |
//! | `rcx`    | Return address (saved by SYSCALL) |
//! | `r11`    | Saved RFLAGS                    |
//!
//! Return value is placed in `rax`; negative values encode `-errno`.
//!
//! This module provides:
//! - [`SyscallFrameChirho`] -- saved register state on syscall entry
//! - Linux syscall number constants (x86_64)
//! - Linux errno constants
//! - [`syscall_dispatch_chirho`] -- main dispatch function
//! - Stub implementations for ~20 critical syscalls
//! - [`init_syscalls_chirho`] -- MSR setup for the SYSCALL instruction
//! - [`UtsNameChirho`] -- Linux `struct utsname` equivalent

use core::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// Syscall frame (saved register state)
// ============================================================================

/// Saved register state captured at syscall entry.
///
/// Layout matches the order in which the assembly entry stub pushes registers.
/// The `SYSCALL` instruction stores the return address in `rcx` and the caller's
/// RFLAGS in `r11`; the entry stub additionally saves `rsp` (user stack pointer).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrameChirho {
    /// Syscall number on entry; overwritten with the return value before SYSRET.
    pub rax_chirho: u64,
    /// Argument 0 (first parameter).
    pub rdi_chirho: u64,
    /// Argument 1.
    pub rsi_chirho: u64,
    /// Argument 2.
    pub rdx_chirho: u64,
    /// Argument 3 (note: the kernel ABI uses `r10`, not `rcx`, for arg3).
    pub r10_chirho: u64,
    /// Argument 4.
    pub r8_chirho: u64,
    /// Argument 5.
    pub r9_chirho: u64,
    /// User-space return address (saved by SYSCALL into RCX).
    pub rcx_chirho: u64,
    /// Saved RFLAGS (saved by SYSCALL into R11).
    pub r11_chirho: u64,
    /// User-space stack pointer (saved by the entry stub).
    pub rsp_chirho: u64,
}

impl SyscallFrameChirho {
    /// Create a zeroed syscall frame (useful for testing).
    pub const fn zeroed_chirho() -> Self {
        Self {
            rax_chirho: 0,
            rdi_chirho: 0,
            rsi_chirho: 0,
            rdx_chirho: 0,
            r10_chirho: 0,
            r8_chirho: 0,
            r9_chirho: 0,
            rcx_chirho: 0,
            r11_chirho: 0,
            rsp_chirho: 0,
        }
    }
}

// ============================================================================
// Linux x86_64 syscall numbers
// ============================================================================
//
// These match the upstream Linux kernel (arch/x86/entry/syscalls/syscall_64.tbl)
// exactly.  Only the most critical ~70 are defined here; the rest will be added
// as they are implemented.

/// `read(2)` -- read from a file descriptor.
pub const SYS_READ_CHIRHO: u64 = 0;
/// `write(2)` -- write to a file descriptor.
pub const SYS_WRITE_CHIRHO: u64 = 1;
/// `open(2)` -- open a file.
pub const SYS_OPEN_CHIRHO: u64 = 2;
/// `close(2)` -- close a file descriptor.
pub const SYS_CLOSE_CHIRHO: u64 = 3;
/// `stat(2)` -- get file status.
pub const SYS_STAT_CHIRHO: u64 = 4;
/// `fstat(2)` -- get file status by fd.
pub const SYS_FSTAT_CHIRHO: u64 = 5;
/// `lstat(2)` -- get file status (no dereference).
pub const SYS_LSTAT_CHIRHO: u64 = 6;
/// `poll(2)` -- wait for events on file descriptors.
pub const SYS_POLL_CHIRHO: u64 = 7;
/// `lseek(2)` -- reposition read/write offset.
pub const SYS_LSEEK_CHIRHO: u64 = 8;
/// `mmap(2)` -- map files or devices into memory.
pub const SYS_MMAP_CHIRHO: u64 = 9;
/// `mprotect(2)` -- set protection on a region of memory.
pub const SYS_MPROTECT_CHIRHO: u64 = 10;
/// `munmap(2)` -- unmap files from memory.
pub const SYS_MUNMAP_CHIRHO: u64 = 11;
/// `brk(2)` -- change data segment size.
pub const SYS_BRK_CHIRHO: u64 = 12;
/// `rt_sigaction(2)` -- examine and change a signal action.
pub const SYS_RT_SIGACTION_CHIRHO: u64 = 13;
/// `rt_sigprocmask(2)` -- examine and change blocked signals.
pub const SYS_RT_SIGPROCMASK_CHIRHO: u64 = 14;
/// `rt_sigreturn(2)` -- return from signal handler.
pub const SYS_RT_SIGRETURN_CHIRHO: u64 = 15;
/// `ioctl(2)` -- control device.
pub const SYS_IOCTL_CHIRHO: u64 = 16;
/// `pread64(2)` -- read at a given offset.
pub const SYS_PREAD64_CHIRHO: u64 = 17;
/// `pwrite64(2)` -- write at a given offset.
pub const SYS_PWRITE64_CHIRHO: u64 = 18;
/// `readv(2)` -- read data into multiple buffers.
pub const SYS_READV_CHIRHO: u64 = 19;
/// `writev(2)` -- write data from multiple buffers.
pub const SYS_WRITEV_CHIRHO: u64 = 20;
/// `access(2)` -- check user permissions for a file.
pub const SYS_ACCESS_CHIRHO: u64 = 21;
/// `pipe(2)` -- create a pipe.
pub const SYS_PIPE_CHIRHO: u64 = 22;
/// `select(2)` -- synchronous I/O multiplexing.
pub const SYS_SELECT_CHIRHO: u64 = 23;
/// `sched_yield(2)` -- yield the processor.
pub const SYS_SCHED_YIELD_CHIRHO: u64 = 24;
/// `mremap(2)` -- remap a virtual memory address.
pub const SYS_MREMAP_CHIRHO: u64 = 25;
/// `msync(2)` -- synchronize a file with a memory map.
pub const SYS_MSYNC_CHIRHO: u64 = 26;
/// `mincore(2)` -- determine whether pages are resident in memory.
pub const SYS_MINCORE_CHIRHO: u64 = 27;
/// `madvise(2)` -- give advice about use of memory.
pub const SYS_MADVISE_CHIRHO: u64 = 28;
/// `dup(2)` -- duplicate a file descriptor.
pub const SYS_DUP_CHIRHO: u64 = 32;
/// `dup2(2)` -- duplicate a file descriptor.
pub const SYS_DUP2_CHIRHO: u64 = 33;
/// `pause(2)` -- wait for signal.
pub const SYS_PAUSE_CHIRHO: u64 = 34;
/// `nanosleep(2)` -- high-resolution sleep.
pub const SYS_NANOSLEEP_CHIRHO: u64 = 35;
/// `getitimer(2)` -- get value of an interval timer.
pub const SYS_GETITIMER_CHIRHO: u64 = 36;
/// `alarm(2)` -- set an alarm clock for delivery of a signal.
pub const SYS_ALARM_CHIRHO: u64 = 37;
/// `setitimer(2)` -- set value of an interval timer.
pub const SYS_SETITIMER_CHIRHO: u64 = 38;
/// `getpid(2)` -- get process identification.
pub const SYS_GETPID_CHIRHO: u64 = 39;
/// `socket(2)` -- create an endpoint for communication.
pub const SYS_SOCKET_CHIRHO: u64 = 41;
/// `connect(2)` -- initiate a connection on a socket.
pub const SYS_CONNECT_CHIRHO: u64 = 42;
/// `accept(2)` -- accept a connection on a socket.
pub const SYS_ACCEPT_CHIRHO: u64 = 43;
/// `sendto(2)` -- send a message on a socket.
pub const SYS_SENDTO_CHIRHO: u64 = 44;
/// `recvfrom(2)` -- receive a message from a socket.
pub const SYS_RECVFROM_CHIRHO: u64 = 45;
/// `shutdown(2)` -- shut down part of a full-duplex connection.
pub const SYS_SHUTDOWN_CHIRHO: u64 = 48;
/// `bind(2)` -- bind a name to a socket.
pub const SYS_BIND_CHIRHO: u64 = 49;
/// `listen(2)` -- listen for connections on a socket.
pub const SYS_LISTEN_CHIRHO: u64 = 50;
/// `clone(2)` -- create a child process.
pub const SYS_CLONE_CHIRHO: u64 = 56;
/// `fork(2)` -- create a child process.
pub const SYS_FORK_CHIRHO: u64 = 57;
/// `vfork(2)` -- create a child process and block parent.
pub const SYS_VFORK_CHIRHO: u64 = 58;
/// `execve(2)` -- execute program.
pub const SYS_EXECVE_CHIRHO: u64 = 59;
/// `exit(2)` -- terminate the calling process.
pub const SYS_EXIT_CHIRHO: u64 = 60;
/// `wait4(2)` -- wait for process to change state.
pub const SYS_WAIT4_CHIRHO: u64 = 61;
/// `kill(2)` -- send signal to a process.
pub const SYS_KILL_CHIRHO: u64 = 62;
/// `uname(2)` -- get name and information about current kernel.
pub const SYS_UNAME_CHIRHO: u64 = 63;
/// `fcntl(2)` -- file control.
pub const SYS_FCNTL_CHIRHO: u64 = 72;
/// `flock(2)` -- apply or remove an advisory lock.
pub const SYS_FLOCK_CHIRHO: u64 = 73;
/// `fsync(2)` -- synchronize file state.
pub const SYS_FSYNC_CHIRHO: u64 = 74;
/// `truncate(2)` -- truncate a file to a specified length.
pub const SYS_TRUNCATE_CHIRHO: u64 = 76;
/// `ftruncate(2)` -- truncate a file to a specified length (by fd).
pub const SYS_FTRUNCATE_CHIRHO: u64 = 77;
/// `getdents(2)` -- get directory entries.
pub const SYS_GETDENTS_CHIRHO: u64 = 78;
/// `getcwd(2)` -- get current working directory.
pub const SYS_GETCWD_CHIRHO: u64 = 79;
/// `chdir(2)` -- change working directory.
pub const SYS_CHDIR_CHIRHO: u64 = 80;
/// `rename(2)` -- change the name or location of a file.
pub const SYS_RENAME_CHIRHO: u64 = 82;
/// `mkdir(2)` -- create a directory.
pub const SYS_MKDIR_CHIRHO: u64 = 83;
/// `rmdir(2)` -- remove a directory.
pub const SYS_RMDIR_CHIRHO: u64 = 84;
/// `creat(2)` -- create a file.
pub const SYS_CREAT_CHIRHO: u64 = 85;
/// `link(2)` -- make a new name for a file.
pub const SYS_LINK_CHIRHO: u64 = 86;
/// `unlink(2)` -- delete a name and possibly the file it refers to.
pub const SYS_UNLINK_CHIRHO: u64 = 87;
/// `readlink(2)` -- read value of a symbolic link.
pub const SYS_READLINK_CHIRHO: u64 = 89;
/// `chmod(2)` -- change file mode.
pub const SYS_CHMOD_CHIRHO: u64 = 90;
/// `chown(2)` -- change file owner and group.
pub const SYS_CHOWN_CHIRHO: u64 = 92;
/// `getuid(2)` -- get user identity.
pub const SYS_GETUID_CHIRHO: u64 = 102;
/// `getgid(2)` -- get group identity.
pub const SYS_GETGID_CHIRHO: u64 = 104;
/// `geteuid(2)` -- get effective user identity.
pub const SYS_GETEUID_CHIRHO: u64 = 107;
/// `getegid(2)` -- get effective group identity.
pub const SYS_GETEGID_CHIRHO: u64 = 108;
/// `getppid(2)` -- get parent process identification.
pub const SYS_GETPPID_CHIRHO: u64 = 110;
/// `getpgrp(2)` -- get process group.
pub const SYS_GETPGRP_CHIRHO: u64 = 111;
/// `setsid(2)` -- creates a session and sets the process group ID.
pub const SYS_SETSID_CHIRHO: u64 = 112;
/// `sigaltstack(2)` -- set/get signal stack context.
pub const SYS_SIGALTSTACK_CHIRHO: u64 = 131;
/// `arch_prctl(2)` -- set architecture-specific thread state.
pub const SYS_ARCH_PRCTL_CHIRHO: u64 = 158;
/// `gettid(2)` -- get thread identification.
pub const SYS_GETTID_CHIRHO: u64 = 186;
/// `futex(2)` -- fast user-space locking.
pub const SYS_FUTEX_CHIRHO: u64 = 202;
/// `set_tid_address(2)` -- set pointer to thread ID.
pub const SYS_SET_TID_ADDRESS_CHIRHO: u64 = 218;
/// `clock_gettime(2)` -- retrieve the time of the specified clock.
pub const SYS_CLOCK_GETTIME_CHIRHO: u64 = 228;
/// `exit_group(2)` -- exit all threads in a process.
pub const SYS_EXIT_GROUP_CHIRHO: u64 = 231;
/// `openat(2)` -- open file relative to directory fd.
pub const SYS_OPENAT_CHIRHO: u64 = 257;
/// `newfstatat(2)` -- get file status relative to directory fd.
pub const SYS_NEWFSTATAT_CHIRHO: u64 = 262;
/// `set_robust_list(2)` -- set list of robust futexes.
pub const SYS_SET_ROBUST_LIST_CHIRHO: u64 = 273;
/// `get_robust_list(2)` -- get list of robust futexes.
pub const SYS_GET_ROBUST_LIST_CHIRHO: u64 = 274;
/// `prlimit64(2)` -- get/set resource limits.
pub const SYS_PRLIMIT64_CHIRHO: u64 = 302;
/// `getrandom(2)` -- obtain a series of random bytes.
pub const SYS_GETRANDOM_CHIRHO: u64 = 318;
/// `rseq(2)` -- restartable sequences.
pub const SYS_RSEQ_CHIRHO: u64 = 334;

// ============================================================================
// Linux errno constants
// ============================================================================
//
// Values match <asm-generic/errno-base.h> and <asm-generic/errno.h>.

/// Operation not permitted.
pub const EPERM_CHIRHO: i64 = 1;
/// No such file or directory.
pub const ENOENT_CHIRHO: i64 = 2;
/// No such process.
pub const ESRCH_CHIRHO: i64 = 3;
/// Interrupted system call.
pub const EINTR_CHIRHO: i64 = 4;
/// I/O error.
pub const EIO_CHIRHO: i64 = 5;
/// No such device or address.
pub const ENXIO_CHIRHO: i64 = 6;
/// Argument list too long.
pub const E2BIG_CHIRHO: i64 = 7;
/// Exec format error.
pub const ENOEXEC_CHIRHO: i64 = 8;
/// Bad file descriptor.
pub const EBADF_CHIRHO: i64 = 9;
/// No child processes.
pub const ECHILD_CHIRHO: i64 = 10;
/// Try again / resource temporarily unavailable.
pub const EAGAIN_CHIRHO: i64 = 11;
/// Out of memory.
pub const ENOMEM_CHIRHO: i64 = 12;
/// Permission denied.
pub const EACCES_CHIRHO: i64 = 13;
/// Bad address.
pub const EFAULT_CHIRHO: i64 = 14;
/// Device or resource busy.
pub const EBUSY_CHIRHO: i64 = 16;
/// File exists.
pub const EEXIST_CHIRHO: i64 = 17;
/// Invalid cross-device link.
pub const EXDEV_CHIRHO: i64 = 18;
/// No such device.
pub const ENODEV_CHIRHO: i64 = 19;
/// Not a directory.
pub const ENOTDIR_CHIRHO: i64 = 20;
/// Is a directory.
pub const EISDIR_CHIRHO: i64 = 21;
/// Invalid argument.
pub const EINVAL_CHIRHO: i64 = 22;
/// File table overflow.
pub const ENFILE_CHIRHO: i64 = 23;
/// Too many open files.
pub const EMFILE_CHIRHO: i64 = 24;
/// Not a typewriter (inappropriate ioctl for device).
pub const ENOTTY_CHIRHO: i64 = 25;
/// File too large.
pub const EFBIG_CHIRHO: i64 = 27;
/// No space left on device.
pub const ENOSPC_CHIRHO: i64 = 28;
/// Illegal seek.
pub const ESPIPE_CHIRHO: i64 = 29;
/// Read-only file system.
pub const EROFS_CHIRHO: i64 = 30;
/// Too many links.
pub const EMLINK_CHIRHO: i64 = 31;
/// Broken pipe.
pub const EPIPE_CHIRHO: i64 = 32;
/// Math argument out of domain of func.
pub const EDOM_CHIRHO: i64 = 33;
/// Math result not representable.
pub const ERANGE_CHIRHO: i64 = 34;
/// Resource deadlock would occur.
pub const EDEADLK_CHIRHO: i64 = 35;
/// File name too long.
pub const ENAMETOOLONG_CHIRHO: i64 = 36;
/// No record locks available.
pub const ENOLCK_CHIRHO: i64 = 37;
/// Function not implemented.
pub const ENOSYS_CHIRHO: i64 = 38;
/// Directory not empty.
pub const ENOTEMPTY_CHIRHO: i64 = 39;
/// Too many levels of symbolic links.
pub const ELOOP_CHIRHO: i64 = 40;
/// No message of desired type.
pub const ENOMSG_CHIRHO: i64 = 42;
/// Operation not supported.
pub const ENOTSUP_CHIRHO: i64 = 95;
/// Address already in use.
pub const EADDRINUSE_CHIRHO: i64 = 98;
/// Connection refused.
pub const ECONNREFUSED_CHIRHO: i64 = 111;

// ============================================================================
// arch_prctl sub-command constants
// ============================================================================

/// Set the 64-bit base address for the FS register (TLS).
const ARCH_SET_FS_CHIRHO: u64 = 0x1002;
/// Get the 64-bit base address for the FS register.
const ARCH_GET_FS_CHIRHO: u64 = 0x1003;
/// Set the 64-bit base address for the GS register.
const ARCH_SET_GS_CHIRHO: u64 = 0x1001;
/// Get the 64-bit base address for the GS register.
const ARCH_GET_GS_CHIRHO: u64 = 0x1004;

// ============================================================================
// UtsNameChirho -- Linux utsname structure
// ============================================================================

/// Size of each field in `struct utsname` (including NUL terminator).
/// Linux defines this as 65 bytes in <sys/utsname.h>.
const UTS_LEN_CHIRHO: usize = 65;

/// Linux `struct utsname` equivalent.
///
/// Each field is a NUL-terminated byte array of [`UTS_LEN_CHIRHO`] bytes.
/// The layout matches the kernel's definition so it can be copied directly to
/// user-space memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UtsNameChirho {
    /// Operating system name (e.g., "Lineluya").
    pub sysname_chirho: [u8; UTS_LEN_CHIRHO],
    /// Network node hostname.
    pub nodename_chirho: [u8; UTS_LEN_CHIRHO],
    /// Operating system release (e.g., "0.1.0").
    pub release_chirho: [u8; UTS_LEN_CHIRHO],
    /// Operating system version string.
    pub version_chirho: [u8; UTS_LEN_CHIRHO],
    /// Hardware identifier (e.g., "x86_64").
    pub machine_chirho: [u8; UTS_LEN_CHIRHO],
    /// NIS or YP domain name.
    pub domainname_chirho: [u8; UTS_LEN_CHIRHO],
}

impl UtsNameChirho {
    /// Build the default `UtsNameChirho` for Lineluya.
    const fn default_chirho() -> Self {
        Self {
            sysname_chirho: Self::field_from_str_chirho(b"Lineluya"),
            nodename_chirho: Self::field_from_str_chirho(b"lineluya"),
            release_chirho: Self::field_from_str_chirho(b"0.1.0"),
            version_chirho: Self::field_from_str_chirho(
                b"#1 SMP Lineluya 0.1.0",
            ),
            machine_chirho: Self::field_from_str_chirho(b"x86_64"),
            domainname_chirho: Self::field_from_str_chirho(b"(none)"),
        }
    }

    /// Copy a byte-string literal into a fixed-size array, NUL-padded.
    ///
    /// Operates at compile time (const fn) so the default utsname can be a
    /// static constant.
    const fn field_from_str_chirho(src_chirho: &[u8]) -> [u8; UTS_LEN_CHIRHO] {
        let mut buf_chirho = [0u8; UTS_LEN_CHIRHO];
        let len_chirho = if src_chirho.len() < UTS_LEN_CHIRHO - 1 {
            src_chirho.len()
        } else {
            UTS_LEN_CHIRHO - 1
        };
        let mut i_chirho: usize = 0;
        while i_chirho < len_chirho {
            buf_chirho[i_chirho] = src_chirho[i_chirho];
            i_chirho += 1;
        }
        // Remaining bytes (including at least the last) are already 0 (NUL).
        buf_chirho
    }
}

/// Global, static utsname for the running kernel.
static UTSNAME_CHIRHO: UtsNameChirho = UtsNameChirho::default_chirho();

// ============================================================================
// Program break tracking (for brk)
// ============================================================================

/// Current program break address.  Initialised to a reasonable default; a real
/// process loader would set this to the end of the BSS/data segment.
static CURRENT_BRK_CHIRHO: AtomicU64 = AtomicU64::new(0x0060_0000);

// ============================================================================
// MSR constants for SYSCALL/SYSRET setup
// ============================================================================

/// IA32_STAR -- Segment selectors for SYSCALL/SYSRET.
///   Bits 47:32 = kernel CS (used by SYSCALL)
///   Bits 63:48 = user CS base (used by SYSRET; CPU adds offsets)
const IA32_STAR_CHIRHO: u32 = 0xC000_0081;

/// IA32_LSTAR -- RIP loaded on SYSCALL (64-bit handler entry point).
const IA32_LSTAR_CHIRHO: u32 = 0xC000_0082;

/// IA32_CSTAR -- RIP loaded on SYSCALL in compatibility mode (unused by us).
#[allow(dead_code)]
const IA32_CSTAR_CHIRHO: u32 = 0xC000_0083;

/// IA32_FMASK -- RFLAGS bits to clear on SYSCALL.
const IA32_FMASK_CHIRHO: u32 = 0xC000_0084;

/// RFLAGS bits to mask on SYSCALL entry.
///   - IF (bit 9): disable interrupts
///   - DF (bit 10): clear direction flag
///   - TF (bit 8): clear trap flag (single-step)
///   - AC (bit 18): clear alignment check
const SYSCALL_FLAG_MASK_CHIRHO: u64 =
    (1 << 9) |  // IF
    (1 << 10) | // DF
    (1 << 8) |  // TF
    (1 << 18);  // AC

/// Kernel code segment selector index (GDT entry 1, ring 0).
/// The GDT in `gdt_chirho` puts the kernel code segment at index 1.
const KERNEL_CS_SELECTOR_CHIRHO: u64 = 0x08;

/// Kernel data segment selector (GDT entry 2, ring 0).
/// Not explicitly loaded on SYSCALL but implied by the STAR MSR layout.
#[allow(dead_code)]
const KERNEL_DS_SELECTOR_CHIRHO: u64 = 0x10;

/// User code segment selector base for SYSRET.
/// SYSRET loads CS = (STAR[63:48] + 16) | 3 and SS = (STAR[63:48] + 8) | 3.
/// With user base = 0x18:
///   user CS = 0x18 + 16 = 0x28 | 3 = 0x2B
///   user SS = 0x18 + 8  = 0x20 | 3 = 0x23
const USER_CS_BASE_SELECTOR_CHIRHO: u64 = 0x18;

// ============================================================================
// Initialisation
// ============================================================================

/// Placeholder for the assembly entry stub address.
///
/// The actual SYSCALL entry point is an assembly trampoline (defined in a
/// separate file) that saves all registers into a [`SyscallFrameChirho`] and
/// calls [`syscall_dispatch_chirho`].  This extern declaration lets us
/// reference its address for the IA32_LSTAR MSR.
///
/// Until the assembly stub is linked, we point LSTAR at
/// [`syscall_entry_stub_chirho`] which is a minimal Rust-side placeholder.
extern "C" {
    /// Symbol defined by the assembly syscall entry stub.
    /// Link-time resolved; declared here so `init_syscalls_chirho` can reference it.
    #[allow(dead_code)]
    fn syscall_entry_asm_chirho();
}

/// Minimal placeholder entry point for SYSCALL.
///
/// In a complete kernel the LSTAR MSR would point to an assembly stub that
/// performs the full register save/restore dance.  This Rust function serves
/// as a fallback during early development so that an accidental SYSCALL does
/// not triple-fault; it simply halts.
///
/// # Safety
///
/// This is called directly by the CPU on SYSCALL.  It must never be called
/// from Rust code.
#[no_mangle]
pub unsafe extern "C" fn syscall_entry_stub_chirho() {
    // We cannot safely do much here without the assembly trampoline.
    // Log a message and halt.
    crate::serial_println_chirho!(
        "[SYSCALL] Entry stub reached -- assembly trampoline not yet linked."
    );
    loop {
        x86_64::instructions::hlt();
    }
}

/// Set up the Model-Specific Registers required for the SYSCALL instruction.
///
/// After this function returns, executing `SYSCALL` in ring 3 will:
/// 1. Load the kernel CS/SS from IA32_STAR.
/// 2. Jump to the address in IA32_LSTAR (the syscall entry stub).
/// 3. Mask RFLAGS according to IA32_FMASK.
///
/// # Safety
///
/// Must be called exactly once during kernel initialisation, after the GDT has
/// been loaded.  The entry point referenced by LSTAR must be a valid code
/// address in the kernel's code segment.
pub unsafe fn init_syscalls_chirho() {
    use x86_64::registers::model_specific::Msr;

    // -- IA32_STAR --
    // Bits 31:0  = EIP for SYSCALL in 32-bit mode (unused, set to 0)
    // Bits 47:32 = Kernel CS selector
    // Bits 63:48 = User CS selector base (for SYSRET)
    let star_value_chirho: u64 =
        (KERNEL_CS_SELECTOR_CHIRHO << 32) | (USER_CS_BASE_SELECTOR_CHIRHO << 48);

    let mut star_msr_chirho = Msr::new(IA32_STAR_CHIRHO);
    star_msr_chirho.write(star_value_chirho);

    // -- IA32_LSTAR --
    // Point LSTAR at the Rust-side stub for now.  Once the assembly
    // entry trampoline is linked, change this to `syscall_entry_asm_chirho`.
    let lstar_addr_chirho = syscall_entry_stub_chirho as *const () as u64;
    let mut lstar_msr_chirho = Msr::new(IA32_LSTAR_CHIRHO);
    lstar_msr_chirho.write(lstar_addr_chirho);

    // -- IA32_FMASK --
    // Clear IF, DF, TF, AC on SYSCALL entry so the kernel starts with
    // interrupts disabled and a known flags state.
    let mut fmask_msr_chirho = Msr::new(IA32_FMASK_CHIRHO);
    fmask_msr_chirho.write(SYSCALL_FLAG_MASK_CHIRHO);

    // -- Enable SYSCALL/SYSRET via IA32_EFER.SCE (bit 0) --
    // The x86_64 crate's `EferFlags` type exposes this bit.
    use x86_64::registers::model_specific::Efer;
    let mut efer_flags_chirho = Efer::read();
    efer_flags_chirho |=
        x86_64::registers::model_specific::EferFlags::SYSTEM_CALL_EXTENSIONS;
    Efer::write(efer_flags_chirho);

    crate::serial_println_chirho!("[SYSCALL] MSRs configured (STAR, LSTAR, FMASK, EFER.SCE)");
}

// ============================================================================
// Main dispatch function
// ============================================================================

/// Dispatch a syscall based on the number in `frame_chirho.rax_chirho`.
///
/// Called from the assembly entry stub after all registers have been saved into
/// the [`SyscallFrameChirho`].  The return value is placed back into
/// `frame_chirho.rax_chirho` and eventually into the real `rax` register before
/// SYSRET.
///
/// Returns negative `-errno` on error, or the syscall-specific result on
/// success.
pub fn syscall_dispatch_chirho(frame_chirho: &mut SyscallFrameChirho) -> i64 {
    let syscall_nr_chirho = frame_chirho.rax_chirho;
    let arg0_chirho = frame_chirho.rdi_chirho;
    let arg1_chirho = frame_chirho.rsi_chirho;
    let arg2_chirho = frame_chirho.rdx_chirho;
    let arg3_chirho = frame_chirho.r10_chirho;
    let arg4_chirho = frame_chirho.r8_chirho;
    let _arg5_chirho = frame_chirho.r9_chirho;

    let result_chirho: i64 = match syscall_nr_chirho {
        SYS_READ_CHIRHO => sys_read_chirho(
            arg0_chirho,
            arg1_chirho as *mut u8,
            arg2_chirho as usize,
        ),
        SYS_WRITE_CHIRHO => sys_write_chirho(
            arg0_chirho,
            arg1_chirho as *const u8,
            arg2_chirho as usize,
        ),
        SYS_OPEN_CHIRHO => -ENOENT_CHIRHO,     // stub: no filesystem yet
        SYS_CLOSE_CHIRHO => sys_close_chirho(arg0_chirho),
        SYS_STAT_CHIRHO | SYS_FSTAT_CHIRHO | SYS_LSTAT_CHIRHO => -ENOENT_CHIRHO,
        SYS_POLL_CHIRHO => -ENOSYS_CHIRHO,
        SYS_LSEEK_CHIRHO => -ESPIPE_CHIRHO,    // stdin/stdout not seekable
        SYS_MMAP_CHIRHO => sys_mmap_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho as u32,
            arg3_chirho as u32,
            arg4_chirho as i32,
            _arg5_chirho,
        ),
        SYS_MPROTECT_CHIRHO => 0,              // silently succeed for now
        SYS_MUNMAP_CHIRHO => 0,                 // silently succeed for now
        SYS_BRK_CHIRHO => sys_brk_chirho(arg0_chirho),
        SYS_RT_SIGACTION_CHIRHO => 0,           // silently succeed (no signals yet)
        SYS_RT_SIGPROCMASK_CHIRHO => 0,         // silently succeed
        SYS_RT_SIGRETURN_CHIRHO => -ENOSYS_CHIRHO,
        SYS_IOCTL_CHIRHO => sys_ioctl_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
        ),
        SYS_PREAD64_CHIRHO | SYS_PWRITE64_CHIRHO => -EBADF_CHIRHO,
        SYS_READV_CHIRHO => -ENOSYS_CHIRHO,
        SYS_WRITEV_CHIRHO => sys_writev_chirho(
            arg0_chirho,
            arg1_chirho as *const IoVecChirho,
            arg2_chirho as i32,
        ),
        SYS_ACCESS_CHIRHO => -ENOENT_CHIRHO,
        SYS_PIPE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_SELECT_CHIRHO => -ENOSYS_CHIRHO,
        SYS_SCHED_YIELD_CHIRHO => {
            // Yield: just return success (no scheduler yet).
            0
        }
        SYS_MREMAP_CHIRHO | SYS_MSYNC_CHIRHO | SYS_MINCORE_CHIRHO | SYS_MADVISE_CHIRHO => {
            -ENOSYS_CHIRHO
        }
        SYS_DUP_CHIRHO | SYS_DUP2_CHIRHO => -EBADF_CHIRHO,
        SYS_PAUSE_CHIRHO => -EINTR_CHIRHO,
        SYS_NANOSLEEP_CHIRHO => 0,             // instant return (no timer yet)
        SYS_GETITIMER_CHIRHO | SYS_SETITIMER_CHIRHO => -ENOSYS_CHIRHO,
        SYS_ALARM_CHIRHO => 0,                  // return 0 (no previous alarm)
        SYS_GETPID_CHIRHO => sys_getpid_chirho(),
        SYS_SOCKET_CHIRHO | SYS_CONNECT_CHIRHO | SYS_ACCEPT_CHIRHO
        | SYS_SENDTO_CHIRHO | SYS_RECVFROM_CHIRHO | SYS_SHUTDOWN_CHIRHO
        | SYS_BIND_CHIRHO | SYS_LISTEN_CHIRHO => -ENOSYS_CHIRHO,
        SYS_CLONE_CHIRHO | SYS_FORK_CHIRHO | SYS_VFORK_CHIRHO => -ENOSYS_CHIRHO,
        SYS_EXECVE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_EXIT_CHIRHO => sys_exit_chirho(arg0_chirho as i32),
        SYS_WAIT4_CHIRHO => -ECHILD_CHIRHO,
        SYS_KILL_CHIRHO => -ESRCH_CHIRHO,
        SYS_UNAME_CHIRHO => sys_uname_chirho(arg0_chirho as *mut UtsNameChirho),
        SYS_FCNTL_CHIRHO => -EBADF_CHIRHO,
        SYS_FLOCK_CHIRHO => -EBADF_CHIRHO,
        SYS_FSYNC_CHIRHO => -EBADF_CHIRHO,
        SYS_TRUNCATE_CHIRHO | SYS_FTRUNCATE_CHIRHO => -EBADF_CHIRHO,
        SYS_GETDENTS_CHIRHO => -EBADF_CHIRHO,
        SYS_GETCWD_CHIRHO => sys_getcwd_chirho(arg0_chirho as *mut u8, arg1_chirho as usize),
        SYS_CHDIR_CHIRHO => -ENOENT_CHIRHO,
        SYS_RENAME_CHIRHO | SYS_MKDIR_CHIRHO | SYS_RMDIR_CHIRHO
        | SYS_CREAT_CHIRHO | SYS_LINK_CHIRHO | SYS_UNLINK_CHIRHO => -ENOSYS_CHIRHO,
        SYS_READLINK_CHIRHO => -ENOENT_CHIRHO,
        SYS_CHMOD_CHIRHO | SYS_CHOWN_CHIRHO => -ENOENT_CHIRHO,
        SYS_GETUID_CHIRHO | SYS_GETEUID_CHIRHO => 0,  // root
        SYS_GETGID_CHIRHO | SYS_GETEGID_CHIRHO => 0,  // root
        SYS_GETPPID_CHIRHO => 0,                        // init has ppid 0
        SYS_GETPGRP_CHIRHO => 1,
        SYS_SETSID_CHIRHO => 1,
        SYS_SIGALTSTACK_CHIRHO => 0,            // silently succeed
        SYS_ARCH_PRCTL_CHIRHO => sys_arch_prctl_chirho(arg0_chirho, arg1_chirho),
        SYS_GETTID_CHIRHO => 1,                 // same as pid for now
        SYS_FUTEX_CHIRHO => -ENOSYS_CHIRHO,
        SYS_SET_TID_ADDRESS_CHIRHO => sys_set_tid_address_chirho(arg0_chirho as *mut i32),
        SYS_CLOCK_GETTIME_CHIRHO => -ENOSYS_CHIRHO,
        SYS_EXIT_GROUP_CHIRHO => sys_exit_group_chirho(arg0_chirho as i32),
        SYS_OPENAT_CHIRHO => -ENOENT_CHIRHO,
        SYS_NEWFSTATAT_CHIRHO => -ENOENT_CHIRHO,
        SYS_SET_ROBUST_LIST_CHIRHO => 0,        // silently succeed
        SYS_GET_ROBUST_LIST_CHIRHO => -ENOSYS_CHIRHO,
        SYS_PRLIMIT64_CHIRHO => -ENOSYS_CHIRHO,
        SYS_GETRANDOM_CHIRHO => -ENOSYS_CHIRHO,
        SYS_RSEQ_CHIRHO => -ENOSYS_CHIRHO,

        // Catch-all for unimplemented syscalls.
        unknown_chirho => {
            crate::serial_println_chirho!(
                "[SYSCALL] Unimplemented syscall {} (args: {:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x})",
                unknown_chirho,
                arg0_chirho,
                arg1_chirho,
                arg2_chirho,
                arg3_chirho,
                arg4_chirho,
                _arg5_chirho,
            );
            -ENOSYS_CHIRHO
        }
    };

    // Store the return value so the caller (assembly stub) can put it in rax.
    frame_chirho.rax_chirho = result_chirho as u64;
    result_chirho
}

// ============================================================================
// iovec for writev
// ============================================================================

/// Linux `struct iovec` equivalent for scatter/gather I/O.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IoVecChirho {
    /// Pointer to the buffer.
    pub iov_base_chirho: *const u8,
    /// Length of the buffer in bytes.
    pub iov_len_chirho: usize,
}

// ============================================================================
// Syscall stub implementations
// ============================================================================

/// `read(2)` stub.
///
/// Currently returns 0 (EOF) for all file descriptors.  Once the VFS layer is
/// implemented, this will dispatch to the appropriate file operations.
fn sys_read_chirho(
    fd_chirho: u64,
    _buf_ptr_chirho: *mut u8,
    _count_chirho: usize,
) -> i64 {
    match fd_chirho {
        0 => {
            // stdin -- return 0 (EOF) until we have a terminal/input driver
            0
        }
        1 | 2 => {
            // stdout/stderr are write-only
            -EBADF_CHIRHO
        }
        _ => -EBADF_CHIRHO,
    }
}

/// `write(2)` implementation.
///
/// For fd 1 (stdout) and fd 2 (stderr), writes the buffer contents to the
/// serial console.  For all other fds, returns -EBADF.
///
/// # Safety
///
/// `buf_ptr_chirho` must point to a readable user-space buffer of at least
/// `count_chirho` bytes.  In a production kernel, this would go through a
/// `copy_from_user` mechanism with proper page-fault handling.
fn sys_write_chirho(
    fd_chirho: u64,
    buf_ptr_chirho: *const u8,
    count_chirho: usize,
) -> i64 {
    match fd_chirho {
        // stdout or stderr -> serial console
        1 | 2 => {
            if buf_ptr_chirho.is_null() {
                return -EFAULT_CHIRHO;
            }
            if count_chirho == 0 {
                return 0;
            }

            // SAFETY: The caller (user space via SYSCALL) guarantees the buffer
            // is readable.  In the future, `copy_from_user` will validate
            // page-table permissions before touching the memory.
            let slice_chirho = unsafe {
                core::slice::from_raw_parts(buf_ptr_chirho, count_chirho)
            };

            // Write each byte to the serial port.
            for &byte_chirho in slice_chirho {
                if byte_chirho == b'\n' {
                    crate::serial_print_chirho!("\n");
                } else if byte_chirho.is_ascii() {
                    crate::serial_print_chirho!("{}", byte_chirho as char);
                } else {
                    // Non-ASCII bytes: emit as hex escape.
                    crate::serial_print_chirho!("\\x{:02x}", byte_chirho);
                }
            }

            count_chirho as i64
        }
        _ => -EBADF_CHIRHO,
    }
}

/// `writev(2)` implementation.
///
/// Writes scatter/gather buffers to the serial console for stdout/stderr.
fn sys_writev_chirho(
    fd_chirho: u64,
    iov_chirho: *const IoVecChirho,
    iovcnt_chirho: i32,
) -> i64 {
    if fd_chirho != 1 && fd_chirho != 2 {
        return -EBADF_CHIRHO;
    }
    if iov_chirho.is_null() || iovcnt_chirho <= 0 {
        return -EINVAL_CHIRHO;
    }

    let mut total_written_chirho: i64 = 0;

    for i_chirho in 0..iovcnt_chirho as usize {
        // SAFETY: Caller guarantees the iovec array is valid.
        let vec_entry_chirho = unsafe { &*iov_chirho.add(i_chirho) };
        if vec_entry_chirho.iov_base_chirho.is_null() || vec_entry_chirho.iov_len_chirho == 0 {
            continue;
        }
        let result_chirho = sys_write_chirho(
            fd_chirho,
            vec_entry_chirho.iov_base_chirho,
            vec_entry_chirho.iov_len_chirho,
        );
        if result_chirho < 0 {
            if total_written_chirho > 0 {
                return total_written_chirho;
            }
            return result_chirho;
        }
        total_written_chirho += result_chirho;
    }

    total_written_chirho
}

/// `close(2)` stub.
///
/// Returns 0 for stdin/stdout/stderr (silently succeeds), -EBADF for
/// everything else.
fn sys_close_chirho(fd_chirho: u64) -> i64 {
    match fd_chirho {
        0 | 1 | 2 => 0,
        _ => -EBADF_CHIRHO,
    }
}

/// `exit(2)` implementation.
///
/// Prints an exit message to serial and halts the CPU.  In a multi-process
/// kernel this would only terminate the calling process/thread; for now
/// it halts the entire machine.
fn sys_exit_chirho(code_chirho: i32) -> i64 {
    crate::serial_println_chirho!(
        "[SYSCALL] exit({}) -- process terminated",
        code_chirho
    );
    // In a real multi-tasking kernel, we would mark the task as dead and
    // schedule another.  For now, halt.
    loop {
        x86_64::instructions::hlt();
    }
}

/// `exit_group(2)` implementation.
///
/// Terminates all threads in the current thread group.  Since Lineluya is
/// currently single-threaded, this is identical to [`sys_exit_chirho`].
fn sys_exit_group_chirho(code_chirho: i32) -> i64 {
    crate::serial_println_chirho!(
        "[SYSCALL] exit_group({}) -- all threads terminated",
        code_chirho
    );
    loop {
        x86_64::instructions::hlt();
    }
}

/// `brk(2)` implementation.
///
/// If `addr_chirho` is 0, returns the current break.  Otherwise, attempts to
/// set the break to `addr_chirho`.  Currently a stub that tracks the value
/// atomically but does not actually map or unmap any memory.
fn sys_brk_chirho(addr_chirho: u64) -> i64 {
    if addr_chirho == 0 {
        // Query: return the current break.
        return CURRENT_BRK_CHIRHO.load(Ordering::SeqCst) as i64;
    }

    // For now, accept any break value.  A real implementation would:
    //   1. Validate the range.
    //   2. Map/unmap pages as needed.
    //   3. Return the new break on success, or the old break on failure.
    CURRENT_BRK_CHIRHO.store(addr_chirho, Ordering::SeqCst);
    addr_chirho as i64
}

/// `arch_prctl(2)` implementation.
///
/// Supports:
/// - `ARCH_SET_FS` (0x1002): Write the FS base MSR.  Used by the C library to
///   set up Thread-Local Storage (TLS).
/// - `ARCH_GET_FS` (0x1003): Read the FS base MSR (stub).
/// - `ARCH_SET_GS` (0x1001): Write the GS base MSR.
/// - `ARCH_GET_GS` (0x1004): Read the GS base MSR (stub).
fn sys_arch_prctl_chirho(code_chirho: u64, addr_chirho: u64) -> i64 {
    use x86_64::registers::model_specific::Msr;

    /// IA32_FS_BASE MSR address.
    const IA32_FS_BASE_CHIRHO: u32 = 0xC000_0100;
    /// IA32_GS_BASE MSR address.
    const IA32_GS_BASE_CHIRHO: u32 = 0xC000_0101;

    match code_chirho {
        ARCH_SET_FS_CHIRHO => {
            // Write FS base for TLS.
            let mut msr_chirho = Msr::new(IA32_FS_BASE_CHIRHO);
            unsafe {
                msr_chirho.write(addr_chirho);
            }
            crate::serial_println_chirho!(
                "[SYSCALL] arch_prctl(ARCH_SET_FS, {:#x})",
                addr_chirho
            );
            0
        }
        ARCH_GET_FS_CHIRHO => {
            // Read FS base and write it to the user-supplied pointer.
            if addr_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let msr_chirho = Msr::new(IA32_FS_BASE_CHIRHO);
            let fs_base_chirho = unsafe { msr_chirho.read() };
            // SAFETY: Caller provides a valid user-space pointer.
            unsafe {
                *(addr_chirho as *mut u64) = fs_base_chirho;
            }
            0
        }
        ARCH_SET_GS_CHIRHO => {
            let mut msr_chirho = Msr::new(IA32_GS_BASE_CHIRHO);
            unsafe {
                msr_chirho.write(addr_chirho);
            }
            crate::serial_println_chirho!(
                "[SYSCALL] arch_prctl(ARCH_SET_GS, {:#x})",
                addr_chirho
            );
            0
        }
        ARCH_GET_GS_CHIRHO => {
            if addr_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let msr_chirho = Msr::new(IA32_GS_BASE_CHIRHO);
            let gs_base_chirho = unsafe { msr_chirho.read() };
            unsafe {
                *(addr_chirho as *mut u64) = gs_base_chirho;
            }
            0
        }
        _ => {
            crate::serial_println_chirho!(
                "[SYSCALL] arch_prctl: unknown code {:#x}",
                code_chirho
            );
            -EINVAL_CHIRHO
        }
    }
}

/// `uname(2)` implementation.
///
/// Copies the kernel's [`UtsNameChirho`] structure to user-space memory at
/// `buf_chirho`.
///
/// # Safety
///
/// `buf_chirho` must point to a writable user-space buffer of at least
/// `size_of::<UtsNameChirho>()` bytes.
fn sys_uname_chirho(buf_chirho: *mut UtsNameChirho) -> i64 {
    if buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // SAFETY: Caller (user space) guarantees the buffer is writable and
    // properly sized.  Future `copy_to_user` will add page-fault safety.
    unsafe {
        core::ptr::write(buf_chirho, UTSNAME_CHIRHO);
    }

    0
}

/// `getpid(2)` implementation.
///
/// Returns the PID of the calling process.  Since Lineluya currently runs a
/// single process (init), this always returns 1.
fn sys_getpid_chirho() -> i64 {
    1
}

/// `mmap(2)` stub.
///
/// Currently returns -ENOMEM for all requests.  A real implementation requires
/// a virtual memory area (VMA) subsystem, page-fault-driven lazy allocation,
/// and file-backed mapping support.
fn sys_mmap_chirho(
    _addr_chirho: u64,
    _length_chirho: u64,
    _prot_chirho: u32,
    _flags_chirho: u32,
    _fd_chirho: i32,
    _offset_chirho: u64,
) -> i64 {
    // TODO: Implement anonymous mmap (MAP_ANONYMOUS | MAP_PRIVATE) as the
    // first step, since that is what the C library uses for large allocations.
    crate::serial_println_chirho!(
        "[SYSCALL] mmap(addr={:#x}, len={:#x}, prot={:#x}, flags={:#x}, fd={}, off={:#x}) -> ENOMEM (stub)",
        _addr_chirho, _length_chirho, _prot_chirho, _flags_chirho, _fd_chirho, _offset_chirho,
    );
    -ENOMEM_CHIRHO
}

/// `set_tid_address(2)` implementation.
///
/// The kernel stores `tidptr_chirho` so it can perform a futex wake when the
/// thread exits.  For now we just record it and return the current TID (1).
fn sys_set_tid_address_chirho(_tidptr_chirho: *mut i32) -> i64 {
    // TODO: Store tidptr in the current task struct for clear_child_tid logic.
    1 // Current TID
}

/// `ioctl(2)` stub.
///
/// Returns -ENOTTY for all requests.  Individual device drivers will override
/// this via their file operations once the VFS is in place.
fn sys_ioctl_chirho(
    fd_chirho: u64,
    request_chirho: u64,
    _arg_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[SYSCALL] ioctl(fd={}, request={:#x}) -> ENOTTY (stub)",
        fd_chirho,
        request_chirho,
    );
    -ENOTTY_CHIRHO
}

/// `getcwd(2)` stub.
///
/// Returns "/" as the current working directory.
fn sys_getcwd_chirho(buf_chirho: *mut u8, size_chirho: usize) -> i64 {
    if buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }
    if size_chirho < 2 {
        return -ERANGE_CHIRHO;
    }

    // SAFETY: Caller guarantees buf_chirho is writable for size_chirho bytes.
    unsafe {
        *buf_chirho = b'/';
        *buf_chirho.add(1) = 0; // NUL terminator
    }

    // Linux getcwd returns the buf pointer on success (cast to long).
    buf_chirho as i64
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a syscall number to its Linux name for debug logging.
///
/// Returns `"unknown"` for numbers not in the table.
#[allow(dead_code)]
pub fn syscall_name_chirho(nr_chirho: u64) -> &'static str {
    match nr_chirho {
        SYS_READ_CHIRHO => "read",
        SYS_WRITE_CHIRHO => "write",
        SYS_OPEN_CHIRHO => "open",
        SYS_CLOSE_CHIRHO => "close",
        SYS_STAT_CHIRHO => "stat",
        SYS_FSTAT_CHIRHO => "fstat",
        SYS_LSTAT_CHIRHO => "lstat",
        SYS_POLL_CHIRHO => "poll",
        SYS_LSEEK_CHIRHO => "lseek",
        SYS_MMAP_CHIRHO => "mmap",
        SYS_MPROTECT_CHIRHO => "mprotect",
        SYS_MUNMAP_CHIRHO => "munmap",
        SYS_BRK_CHIRHO => "brk",
        SYS_RT_SIGACTION_CHIRHO => "rt_sigaction",
        SYS_RT_SIGPROCMASK_CHIRHO => "rt_sigprocmask",
        SYS_RT_SIGRETURN_CHIRHO => "rt_sigreturn",
        SYS_IOCTL_CHIRHO => "ioctl",
        SYS_PREAD64_CHIRHO => "pread64",
        SYS_PWRITE64_CHIRHO => "pwrite64",
        SYS_READV_CHIRHO => "readv",
        SYS_WRITEV_CHIRHO => "writev",
        SYS_ACCESS_CHIRHO => "access",
        SYS_PIPE_CHIRHO => "pipe",
        SYS_SELECT_CHIRHO => "select",
        SYS_SCHED_YIELD_CHIRHO => "sched_yield",
        SYS_MREMAP_CHIRHO => "mremap",
        SYS_MSYNC_CHIRHO => "msync",
        SYS_MINCORE_CHIRHO => "mincore",
        SYS_MADVISE_CHIRHO => "madvise",
        SYS_DUP_CHIRHO => "dup",
        SYS_DUP2_CHIRHO => "dup2",
        SYS_PAUSE_CHIRHO => "pause",
        SYS_NANOSLEEP_CHIRHO => "nanosleep",
        SYS_GETITIMER_CHIRHO => "getitimer",
        SYS_ALARM_CHIRHO => "alarm",
        SYS_SETITIMER_CHIRHO => "setitimer",
        SYS_GETPID_CHIRHO => "getpid",
        SYS_SOCKET_CHIRHO => "socket",
        SYS_CONNECT_CHIRHO => "connect",
        SYS_ACCEPT_CHIRHO => "accept",
        SYS_SENDTO_CHIRHO => "sendto",
        SYS_RECVFROM_CHIRHO => "recvfrom",
        SYS_SHUTDOWN_CHIRHO => "shutdown",
        SYS_BIND_CHIRHO => "bind",
        SYS_LISTEN_CHIRHO => "listen",
        SYS_CLONE_CHIRHO => "clone",
        SYS_FORK_CHIRHO => "fork",
        SYS_VFORK_CHIRHO => "vfork",
        SYS_EXECVE_CHIRHO => "execve",
        SYS_EXIT_CHIRHO => "exit",
        SYS_WAIT4_CHIRHO => "wait4",
        SYS_KILL_CHIRHO => "kill",
        SYS_UNAME_CHIRHO => "uname",
        SYS_FCNTL_CHIRHO => "fcntl",
        SYS_FLOCK_CHIRHO => "flock",
        SYS_FSYNC_CHIRHO => "fsync",
        SYS_TRUNCATE_CHIRHO => "truncate",
        SYS_FTRUNCATE_CHIRHO => "ftruncate",
        SYS_GETDENTS_CHIRHO => "getdents",
        SYS_GETCWD_CHIRHO => "getcwd",
        SYS_CHDIR_CHIRHO => "chdir",
        SYS_RENAME_CHIRHO => "rename",
        SYS_MKDIR_CHIRHO => "mkdir",
        SYS_RMDIR_CHIRHO => "rmdir",
        SYS_CREAT_CHIRHO => "creat",
        SYS_LINK_CHIRHO => "link",
        SYS_UNLINK_CHIRHO => "unlink",
        SYS_READLINK_CHIRHO => "readlink",
        SYS_CHMOD_CHIRHO => "chmod",
        SYS_CHOWN_CHIRHO => "chown",
        SYS_GETUID_CHIRHO => "getuid",
        SYS_GETGID_CHIRHO => "getgid",
        SYS_GETEUID_CHIRHO => "geteuid",
        SYS_GETEGID_CHIRHO => "getegid",
        SYS_GETPPID_CHIRHO => "getppid",
        SYS_GETPGRP_CHIRHO => "getpgrp",
        SYS_SETSID_CHIRHO => "setsid",
        SYS_SIGALTSTACK_CHIRHO => "sigaltstack",
        SYS_ARCH_PRCTL_CHIRHO => "arch_prctl",
        SYS_GETTID_CHIRHO => "gettid",
        SYS_FUTEX_CHIRHO => "futex",
        SYS_SET_TID_ADDRESS_CHIRHO => "set_tid_address",
        SYS_CLOCK_GETTIME_CHIRHO => "clock_gettime",
        SYS_EXIT_GROUP_CHIRHO => "exit_group",
        SYS_OPENAT_CHIRHO => "openat",
        SYS_NEWFSTATAT_CHIRHO => "newfstatat",
        SYS_SET_ROBUST_LIST_CHIRHO => "set_robust_list",
        SYS_GET_ROBUST_LIST_CHIRHO => "get_robust_list",
        SYS_PRLIMIT64_CHIRHO => "prlimit64",
        SYS_GETRANDOM_CHIRHO => "getrandom",
        SYS_RSEQ_CHIRHO => "rseq",
        _ => "unknown",
    }
}
