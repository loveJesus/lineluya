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
    // --- Callee-saved registers (needed for fork child return) ---
    /// Callee-saved register rbx.
    pub rbx_chirho: u64,
    /// Callee-saved register rbp (frame pointer).
    pub rbp_chirho: u64,
    /// Callee-saved register r12.
    pub r12_chirho: u64,
    /// Callee-saved register r13.
    pub r13_chirho: u64,
    /// Callee-saved register r14.
    pub r14_chirho: u64,
    /// Callee-saved register r15.
    pub r15_chirho: u64,
}

// Compile-time layout assertions tying the Rust struct to assembly offsets.
// If these fail, the assembly trampoline in syscall_entry_chirho.rs is broken.
const _: () = {
    assert!(core::mem::offset_of!(SyscallFrameChirho, rax_chirho) == 0x00);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rdi_chirho) == 0x08);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rsi_chirho) == 0x10);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rdx_chirho) == 0x18);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r10_chirho) == 0x20);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r8_chirho)  == 0x28);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r9_chirho)  == 0x30);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rcx_chirho) == 0x38);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r11_chirho) == 0x40);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rsp_chirho) == 0x48);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rbx_chirho) == 0x50);
    assert!(core::mem::offset_of!(SyscallFrameChirho, rbp_chirho) == 0x58);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r12_chirho) == 0x60);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r13_chirho) == 0x68);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r14_chirho) == 0x70);
    assert!(core::mem::offset_of!(SyscallFrameChirho, r15_chirho) == 0x78);
    assert!(core::mem::size_of::<SyscallFrameChirho>() == 0x80); // 128 bytes = 16 * 8
};

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
            rbx_chirho: 0,
            rbp_chirho: 0,
            r12_chirho: 0,
            r13_chirho: 0,
            r14_chirho: 0,
            r15_chirho: 0,
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
/// `symlink(2)` -- create a symbolic link.
pub const SYS_SYMLINK_CHIRHO: u64 = 88;
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
/// `clock_getres(2)` -- get clock resolution.
pub const SYS_CLOCK_GETRES_CHIRHO: u64 = 229;
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
/// `readlinkat(2)` -- read value of a symbolic link relative to directory fd.
pub const SYS_READLINKAT_CHIRHO: u64 = 267;
/// `faccessat(2)` -- check user permissions for a file relative to directory fd.
pub const SYS_FACCESSAT_CHIRHO: u64 = 269;
/// `getdents64(2)` -- get directory entries (64-bit).
pub const SYS_GETDENTS64_CHIRHO: u64 = 217;
/// `statx(2)` -- get file status (extended).
pub const SYS_STATX_CHIRHO: u64 = 332;
/// `mkdirat(2)` -- create a directory relative to directory fd.
pub const SYS_MKDIRAT_CHIRHO: u64 = 258;
/// `unlinkat(2)` -- remove a directory entry relative to directory fd.
pub const SYS_UNLINKAT_CHIRHO: u64 = 263;
/// `pipe2(2)` -- create pipe with flags.
pub const SYS_PIPE2_CHIRHO: u64 = 293;
/// `renameat2(2)` -- rename a file with flags.
pub const SYS_RENAMEAT2_CHIRHO: u64 = 316;
/// `mount(2)` -- mount filesystem.
pub const SYS_MOUNT_CHIRHO: u64 = 165;
/// `umount2(2)` -- unmount filesystem.
pub const SYS_UMOUNT2_CHIRHO: u64 = 166;
/// `epoll_wait(2)` -- wait for events on an epoll fd.
pub const SYS_EPOLL_WAIT_CHIRHO: u64 = 232;
/// `epoll_ctl(2)` -- control interface for an epoll fd.
pub const SYS_EPOLL_CTL_CHIRHO: u64 = 233;
/// `pselect6(2)` -- synchronous I/O multiplexing (with sigmask).
pub const SYS_PSELECT6_CHIRHO: u64 = 270;
/// `ppoll(2)` -- wait for events on file descriptors (with sigmask).
pub const SYS_PPOLL_CHIRHO: u64 = 271;
/// `epoll_pwait(2)` -- wait for events on an epoll fd (with sigmask).
pub const SYS_EPOLL_PWAIT_CHIRHO: u64 = 281;
/// `epoll_create1(2)` -- open an epoll file descriptor.
pub const SYS_EPOLL_CREATE1_CHIRHO: u64 = 291;
/// `sysinfo(2)` -- return system information.
pub const SYS_SYSINFO_CHIRHO: u64 = 99;
/// `mknod(2)` -- create a special or ordinary file.
pub const SYS_MKNOD_CHIRHO: u64 = 133;
/// `personality(2)` -- set the process execution domain.
pub const SYS_PERSONALITY_CHIRHO: u64 = 135;
/// `prctl(2)` -- operations on a process.
pub const SYS_PRCTL_CHIRHO: u64 = 157;
/// `sched_setaffinity(2)` -- set a thread's CPU affinity mask.
pub const SYS_SCHED_SETAFFINITY_CHIRHO: u64 = 203;
/// `sched_getaffinity(2)` -- get a thread's CPU affinity mask.
pub const SYS_SCHED_GETAFFINITY_CHIRHO: u64 = 204;
/// `clock_nanosleep(2)` -- high-resolution sleep with specifiable clock.
pub const SYS_CLOCK_NANOSLEEP_CHIRHO: u64 = 230;
/// `mknodat(2)` -- create a special or ordinary file relative to directory fd.
pub const SYS_MKNODAT_CHIRHO: u64 = 259;
/// `timerfd_create(2)` -- create a timer that delivers expiration via fd.
pub const SYS_TIMERFD_CREATE_CHIRHO: u64 = 283;
/// `signalfd4(2)` -- create a file descriptor for accepting signals.
pub const SYS_SIGNALFD4_CHIRHO: u64 = 289;
/// `eventfd2(2)` -- create a file descriptor for event notification.
pub const SYS_EVENTFD2_CHIRHO: u64 = 290;
/// `dup3(2)` -- duplicate a file descriptor with flags.
pub const SYS_DUP3_CHIRHO: u64 = 292;
/// `memfd_create(2)` -- create an anonymous file.
pub const SYS_MEMFD_CREATE_CHIRHO: u64 = 319;
/// `rseq(2)` -- restartable sequences.
pub const SYS_RSEQ_CHIRHO: u64 = 334;


// --- Phase 4 syscall number additions ---

/// `setuid(2)` -- set user identity.
pub const SYS_SETUID_CHIRHO: u64 = 105;
/// `setgid(2)` -- set group identity.
pub const SYS_SETGID_CHIRHO: u64 = 106;
/// `setpgid(2)` -- set process group ID.
pub const SYS_SETPGID_CHIRHO: u64 = 109;
/// `setreuid(2)` -- set real and effective user IDs.
pub const SYS_SETREUID_CHIRHO: u64 = 113;
/// `setregid(2)` -- set real and effective group IDs.
pub const SYS_SETREGID_CHIRHO: u64 = 114;
/// `getgroups(2)` -- get supplementary group IDs.
pub const SYS_GETGROUPS_CHIRHO: u64 = 115;
/// `setgroups(2)` -- set supplementary group IDs.
pub const SYS_SETGROUPS_CHIRHO: u64 = 116;
/// `setresuid(2)` -- set real, effective and saved user IDs.
pub const SYS_SETRESUID_CHIRHO: u64 = 117;
/// `getresuid(2)` -- get real, effective and saved user IDs.
pub const SYS_GETRESUID_CHIRHO: u64 = 118;
/// `setresgid(2)` -- set real, effective and saved group IDs.
pub const SYS_SETRESGID_CHIRHO: u64 = 119;
/// `getresgid(2)` -- get real, effective and saved group IDs.
pub const SYS_GETRESGID_CHIRHO: u64 = 120;
/// `getpgid(2)` -- get process group ID.
pub const SYS_GETPGID_CHIRHO: u64 = 121;
/// `getsid(2)` -- get session ID.
pub const SYS_GETSID_CHIRHO: u64 = 124;
/// `rt_sigpending(2)` -- examine pending signals.
pub const SYS_RT_SIGPENDING_CHIRHO: u64 = 127;
/// `rt_sigsuspend(2)` -- wait for a signal.
pub const SYS_RT_SIGSUSPEND_CHIRHO: u64 = 130;
/// `tkill(2)` -- send a signal to a thread.
pub const SYS_TKILL_CHIRHO: u64 = 200;
/// `tgkill(2)` -- send a signal to a thread in a thread group.
pub const SYS_TGKILL_CHIRHO: u64 = 234;
/// `timerfd_settime(2)` -- arm/disarm a timer fd.
pub const SYS_TIMERFD_SETTIME_CHIRHO: u64 = 286;
/// `timerfd_gettime(2)` -- get timer fd expiration.
pub const SYS_TIMERFD_GETTIME_CHIRHO: u64 = 287;
/// `eventfd(2)` -- create a file descriptor for event notification.
pub const SYS_EVENTFD_CHIRHO: u64 = 284;
// --- Phase 8+9 syscall number additions ---

/// `sendfile(2)` -- transfer data between file descriptors.
pub const SYS_SENDFILE_CHIRHO: u64 = 40;
/// `fdatasync(2)` -- synchronize file data.
pub const SYS_FDATASYNC_CHIRHO: u64 = 75;
/// `gettimeofday(2)` -- get time.
pub const SYS_GETTIMEOFDAY_CHIRHO: u64 = 96;
/// `getrusage(2)` -- get resource usage.
pub const SYS_GETRUSAGE_CHIRHO: u64 = 98;
/// `times(2)` -- get process times.
pub const SYS_TIMES_CHIRHO: u64 = 100;
/// `ptrace(2)` -- process trace.
pub const SYS_PTRACE_CHIRHO: u64 = 101;
/// `syslog(2)` -- read/clear kernel message ring buffer.
pub const SYS_SYSLOG_CHIRHO: u64 = 103;
/// `getpriority(2)` -- get program scheduling priority.
pub const SYS_GETPRIORITY_CHIRHO: u64 = 140;
/// `setpriority(2)` -- set program scheduling priority.
pub const SYS_SETPRIORITY_CHIRHO: u64 = 141;
/// `sched_setparam(2)` -- set scheduling parameters.
pub const SYS_SCHED_SETPARAM_CHIRHO: u64 = 142;
/// `sched_getparam(2)` -- get scheduling parameters.
pub const SYS_SCHED_GETPARAM_CHIRHO: u64 = 143;
/// `sched_setscheduler(2)` -- set scheduling policy/parameters.
pub const SYS_SCHED_SETSCHEDULER_CHIRHO: u64 = 144;
/// `sched_getscheduler(2)` -- get scheduling policy.
pub const SYS_SCHED_GETSCHEDULER_CHIRHO: u64 = 145;
/// `sched_get_priority_max(2)` -- get static priority range (max).
pub const SYS_SCHED_GET_PRIORITY_MAX_CHIRHO: u64 = 146;
/// `sched_get_priority_min(2)` -- get static priority range (min).
pub const SYS_SCHED_GET_PRIORITY_MIN_CHIRHO: u64 = 147;
/// `mlock(2)` -- lock memory.
pub const SYS_MLOCK_CHIRHO: u64 = 149;
/// `munlock(2)` -- unlock memory.
pub const SYS_MUNLOCK_CHIRHO: u64 = 150;
/// `mlockall(2)` -- lock all memory.
pub const SYS_MLOCKALL_CHIRHO: u64 = 151;
/// `munlockall(2)` -- unlock all memory.
pub const SYS_MUNLOCKALL_CHIRHO: u64 = 152;
/// `sync(2)` -- commit buffer cache to disk.
pub const SYS_SYNC_CHIRHO: u64 = 162;
/// `settimeofday(2)` -- set time.
pub const SYS_SETTIMEOFDAY_CHIRHO: u64 = 164;
/// `reboot(2)` -- reboot or disable Ctrl-Alt-Del.
pub const SYS_REBOOT_CHIRHO: u64 = 169;
/// `sethostname(2)` -- set hostname.
pub const SYS_SETHOSTNAME_CHIRHO: u64 = 170;
/// `init_module(2)` -- load a kernel module image.
pub const SYS_INIT_MODULE_CHIRHO: u64 = 175;
/// `delete_module(2)` -- unload a kernel module.
pub const SYS_DELETE_MODULE_CHIRHO: u64 = 176;
/// `setxattr(2)` -- set an extended attribute value.
pub const SYS_SETXATTR_CHIRHO: u64 = 188;
/// `getxattr(2)` -- get an extended attribute value.
pub const SYS_GETXATTR_CHIRHO: u64 = 191;
/// `listxattr(2)` -- list extended attribute names.
pub const SYS_LISTXATTR_CHIRHO: u64 = 194;
/// `removexattr(2)` -- remove an extended attribute.
pub const SYS_REMOVEXATTR_CHIRHO: u64 = 197;
/// `fadvise64(2)` -- predeclare an access pattern for file data.
pub const SYS_FADVISE64_CHIRHO: u64 = 221;
/// `timer_create(2)` -- create a POSIX per-process timer.
pub const SYS_TIMER_CREATE_CHIRHO: u64 = 222;
/// `timer_settime(2)` -- arm/disarm a POSIX per-process timer.
pub const SYS_TIMER_SETTIME_CHIRHO: u64 = 223;
/// `timer_gettime(2)` -- fetch state of a POSIX per-process timer.
pub const SYS_TIMER_GETTIME_CHIRHO: u64 = 224;
/// `timer_delete(2)` -- delete a POSIX per-process timer.
pub const SYS_TIMER_DELETE_CHIRHO: u64 = 226;
/// `waitid(2)` -- wait for a child process to change state.
pub const SYS_WAITID_CHIRHO: u64 = 247;
/// `splice(2)` -- splice data to/from a pipe.
pub const SYS_SPLICE_CHIRHO: u64 = 275;
/// `tee(2)` -- duplicating pipe content.
pub const SYS_TEE_CHIRHO: u64 = 276;
/// `vmsplice(2)` -- splice user pages into a pipe.
pub const SYS_VMSPLICE_CHIRHO: u64 = 278;
/// `fallocate(2)` -- manipulate file space.
pub const SYS_FALLOCATE_CHIRHO: u64 = 285;
/// `execveat(2)` -- execute program relative to a directory file descriptor.
pub const SYS_EXECVEAT_CHIRHO: u64 = 322;
/// `mlock2(2)` -- lock memory (with flags).
pub const SYS_MLOCK2_CHIRHO: u64 = 325;
/// `copy_file_range(2)` -- copy a range of data between two files.
pub const SYS_COPY_FILE_RANGE_CHIRHO: u64 = 326;
/// `io_uring_setup(2)` -- set up io_uring submission/completion queues.
pub const SYS_IO_URING_SETUP_CHIRHO: u64 = 425;
/// `io_uring_enter(2)` -- initiate and/or complete io_uring I/O.
pub const SYS_IO_URING_ENTER_CHIRHO: u64 = 426;
/// `io_uring_register(2)` -- register files/buffers for io_uring.
pub const SYS_IO_URING_REGISTER_CHIRHO: u64 = 427;
/// `clone3(2)` -- create a child process (extended).
pub const SYS_CLONE3_CHIRHO: u64 = 435;

// --- Phase 10: Massive syscall coverage additions ---

/// `statfs(2)` -- get filesystem statistics.
pub const SYS_STATFS_CHIRHO: u64 = 137;
/// `fstatfs(2)` -- get filesystem statistics by fd.
pub const SYS_FSTATFS_CHIRHO: u64 = 138;
/// `fchmod(2)` -- change file mode by fd.
pub const SYS_FCHMOD_CHIRHO: u64 = 91;
/// `fchown(2)` -- change file owner by fd.
pub const SYS_FCHOWN_CHIRHO: u64 = 93;
/// `lchown(2)` -- change file owner (no dereference).
pub const SYS_LCHOWN_CHIRHO: u64 = 94;
/// `umask(2)` -- set file mode creation mask.
pub const SYS_UMASK_CHIRHO: u64 = 95;
/// `getrlimit(2)` -- get resource limits.
pub const SYS_GETRLIMIT_CHIRHO: u64 = 97;
/// `lsetxattr(2)` -- set extended attribute (no dereference).
pub const SYS_LSETXATTR_CHIRHO: u64 = 189;
/// `fsetxattr(2)` -- set extended attribute by fd.
pub const SYS_FSETXATTR_CHIRHO: u64 = 190;
/// `lgetxattr(2)` -- get extended attribute (no dereference).
pub const SYS_LGETXATTR_CHIRHO: u64 = 192;
/// `fgetxattr(2)` -- get extended attribute by fd.
pub const SYS_FGETXATTR_CHIRHO: u64 = 193;
/// `llistxattr(2)` -- list extended attributes (no dereference).
pub const SYS_LLISTXATTR_CHIRHO: u64 = 195;
/// `flistxattr(2)` -- list extended attributes by fd.
pub const SYS_FLISTXATTR_CHIRHO: u64 = 196;
/// `lremovexattr(2)` -- remove extended attribute (no dereference).
pub const SYS_LREMOVEXATTR_CHIRHO: u64 = 198;
/// `fremovexattr(2)` -- remove extended attribute by fd.
pub const SYS_FREMOVEXATTR_CHIRHO: u64 = 199;
/// `ioprio_set(2)` -- set I/O scheduling class and priority.
pub const SYS_IOPRIO_SET_CHIRHO: u64 = 251;
/// `ioprio_get(2)` -- get I/O scheduling class and priority.
pub const SYS_IOPRIO_GET_CHIRHO: u64 = 252;
/// `inotify_add_watch(2)` -- add a watch to an inotify instance.
pub const SYS_INOTIFY_ADD_WATCH_CHIRHO: u64 = 254;
/// `inotify_rm_watch(2)` -- remove a watch from an inotify instance.
pub const SYS_INOTIFY_RM_WATCH_CHIRHO: u64 = 255;
/// `fchownat(2)` -- change file owner relative to directory fd.
pub const SYS_FCHOWNAT_CHIRHO: u64 = 260;
/// `linkat(2)` -- create a hard link relative to directory fds.
pub const SYS_LINKAT_CHIRHO: u64 = 265;
/// `symlinkat(2)` -- create a symbolic link relative to directory fd.
pub const SYS_SYMLINKAT_CHIRHO: u64 = 266;
/// `fchmodat(2)` -- change file mode relative to directory fd.
pub const SYS_FCHMODAT_CHIRHO: u64 = 268;
/// `sync_file_range(2)` -- sync a file segment with disk.
pub const SYS_SYNC_FILE_RANGE_CHIRHO: u64 = 277;
/// `utimensat(2)` -- change file timestamps with nanosecond precision.
pub const SYS_UTIMENSAT_CHIRHO: u64 = 280;
/// `inotify_init1(2)` -- initialize an inotify instance.
pub const SYS_INOTIFY_INIT1_CHIRHO: u64 = 294;
/// `perf_event_open(2)` -- set up performance monitoring.
pub const SYS_PERF_EVENT_OPEN_CHIRHO: u64 = 298;
/// `fanotify_init(2)` -- create and initialize a fanotify group.
pub const SYS_FANOTIFY_INIT_CHIRHO: u64 = 300;
/// `fanotify_mark(2)` -- add, remove, or modify a fanotify mark.
pub const SYS_FANOTIFY_MARK_CHIRHO: u64 = 301;
/// `name_to_handle_at(2)` -- obtain handle for a pathname.
pub const SYS_NAME_TO_HANDLE_AT_CHIRHO: u64 = 303;
/// `open_by_handle_at(2)` -- open file via a handle.
pub const SYS_OPEN_BY_HANDLE_AT_CHIRHO: u64 = 304;
/// `finit_module(2)` -- load a kernel module by file descriptor.
pub const SYS_FINIT_MODULE_CHIRHO: u64 = 313;

// --- Phase 5+6+7 syscall number additions ---

/// `sendmsg(2)` -- send a message on a socket.
pub const SYS_SENDMSG_CHIRHO: u64 = 46;
/// `recvmsg(2)` -- receive a message from a socket.
pub const SYS_RECVMSG_CHIRHO: u64 = 47;
/// `getsockname(2)` -- get socket name.
pub const SYS_GETSOCKNAME_CHIRHO: u64 = 51;
/// `getpeername(2)` -- get name of connected peer socket.
pub const SYS_GETPEERNAME_CHIRHO: u64 = 52;
/// `socketpair(2)` -- create a pair of connected sockets.
pub const SYS_SOCKETPAIR_CHIRHO: u64 = 53;
/// `setsockopt(2)` -- set options on sockets.
pub const SYS_SETSOCKOPT_CHIRHO: u64 = 54;
/// `getsockopt(2)` -- get options on sockets.
pub const SYS_GETSOCKOPT_CHIRHO: u64 = 55;
/// `capget(2)` -- get process capabilities.
pub const SYS_CAPGET_CHIRHO: u64 = 125;
/// `capset(2)` -- set process capabilities.
pub const SYS_CAPSET_CHIRHO: u64 = 126;
/// `unshare(2)` -- disassociate parts of the process execution context.
pub const SYS_UNSHARE_CHIRHO: u64 = 272;
/// `accept4(2)` -- accept a connection on a socket (with flags).
pub const SYS_ACCEPT4_CHIRHO: u64 = 288;
/// `setns(2)` -- reassociate thread with a namespace.
pub const SYS_SETNS_CHIRHO: u64 = 308;
/// `seccomp(2)` -- operate on Secure Computing state.
pub const SYS_SECCOMP_CHIRHO: u64 = 317;
/// `bpf(2)` -- perform a command on an extended BPF map or program.
pub const SYS_BPF_CHIRHO: u64 = 321;
/// `landlock_create_ruleset(2)` -- create a new Landlock ruleset.
pub const SYS_LANDLOCK_CREATE_RULESET_CHIRHO: u64 = 444;

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
/// Not a socket.
pub const ENOTSOCK_CHIRHO: i64 = 88;
/// Connection refused.
pub const ECONNREFUSED_CHIRHO: i64 = 111;
/// Address family not supported.
pub const EAFNOSUPPORT_CHIRHO: i64 = 97;
/// Operation not supported on socket.
pub const EOPNOTSUPP_CHIRHO: i64 = 95;
/// Transport endpoint is not connected.
pub const ENOTCONN_CHIRHO: i64 = 107;
/// Transport endpoint is already connected.
pub const EISCONN_CHIRHO: i64 = 106;
/// Operation now in progress.
pub const EINPROGRESS_CHIRHO: i64 = 115;
/// Connection reset by peer.
pub const ECONNRESET_CHIRHO: i64 = 104;
/// Software caused connection abort.
pub const ECONNABORTED_CHIRHO: i64 = 103;

/// `ENETUNREACH` — network unreachable.
pub const ENETUNREACH_CHIRHO: i64 = 101;

/// `EHOSTUNREACH` — no route to host.
pub const EHOSTUNREACH_CHIRHO: i64 = 113;

/// `EMSGSIZE` — message too long.
pub const EMSGSIZE_CHIRHO: i64 = 90;

/// `EPROTONOSUPPORT` — protocol not supported.
pub const EPROTONOSUPPORT_CHIRHO: i64 = 93;

/// `EDESTADDRREQ` — destination address required.
pub const EDESTADDRREQ_CHIRHO: i64 = 89;

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
// prctl(2) sub-command constants
// ============================================================================

/// PR_SET_NAME -- set the name of the calling thread.
const PR_SET_NAME_CHIRHO: u64 = 15;
/// PR_GET_NAME -- get the name of the calling thread.
const PR_GET_NAME_CHIRHO: u64 = 16;

// ============================================================================
// SysinfoChirho -- Linux sysinfo structure
// ============================================================================

/// Linux `struct sysinfo` equivalent for sysinfo(2).
///
/// Layout matches the kernel's `struct sysinfo` on x86_64.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SysinfoChirho {
    /// Seconds since boot.
    pub uptime_chirho: i64,
    /// 1, 5, and 15 minute load averages.
    pub loads_chirho: [u64; 3],
    /// Total usable main memory size.
    pub totalram_chirho: u64,
    /// Available memory size.
    pub freeram_chirho: u64,
    /// Amount of shared memory.
    pub sharedram_chirho: u64,
    /// Memory used by buffers.
    pub bufferram_chirho: u64,
    /// Total swap space size.
    pub totalswap_chirho: u64,
    /// Swap space still available.
    pub freeswap_chirho: u64,
    /// Number of current processes.
    pub procs_chirho: u16,
    /// Padding.
    pub _pad_chirho: [u8; 6],
    /// Total high memory size.
    pub totalhigh_chirho: u64,
    /// Available high memory size.
    pub freehigh_chirho: u64,
    /// Memory unit size in bytes.
    pub mem_unit_chirho: u32,
    /// Padding to 64 bytes.
    pub _padding_chirho: [u8; 4],
}

// ============================================================================
// StatfsChirho -- Linux statfs structure (for statfs/fstatfs)
// ============================================================================

/// Linux `struct statfs` equivalent for statfs(2)/fstatfs(2).
///
/// Layout matches the kernel's `struct statfs` on x86_64 (120 bytes).
/// sqlite3 uses this to detect filesystem capabilities (e.g. POSIX locks).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StatfsChirho {
    /// Type of filesystem (magic number).
    pub f_type_chirho: i64,
    /// Optimal transfer block size.
    pub f_bsize_chirho: i64,
    /// Total data blocks in filesystem.
    pub f_blocks_chirho: u64,
    /// Free blocks in filesystem.
    pub f_bfree_chirho: u64,
    /// Free blocks available to unprivileged user.
    pub f_bavail_chirho: u64,
    /// Total file nodes in filesystem.
    pub f_files_chirho: u64,
    /// Free file nodes in filesystem.
    pub f_ffree_chirho: u64,
    /// Filesystem ID.
    pub f_fsid_chirho: [i32; 2],
    /// Maximum length of filenames.
    pub f_namelen_chirho: i64,
    /// Fragment size.
    pub f_frsize_chirho: i64,
    /// Mount flags.
    pub f_flags_chirho: i64,
    /// Padding.
    pub f_spare_chirho: [i64; 4],
}

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
// TimespecChirho -- Linux timespec structure
// ============================================================================

/// Linux `struct timespec` equivalent for clock_gettime(2).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TimespecChirho {
    /// Seconds.
    pub tv_sec_chirho: i64,
    /// Nanoseconds.
    pub tv_nsec_chirho: i64,
}

// ============================================================================
// TimevalChirho -- Linux timeval structure (Phase 8+9)
// ============================================================================

/// Linux `struct timeval` equivalent for gettimeofday(2).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TimevalChirho {
    /// Seconds.
    pub tv_sec_chirho: i64,
    /// Microseconds.
    pub tv_usec_chirho: i64,
}

// ============================================================================
// RusageChirho -- Linux rusage structure (Phase 8+9)
// ============================================================================

/// Linux `struct rusage` equivalent for getrusage(2).
///
/// Contains 18 fields (all i64/TimevalChirho). We use a flat layout matching
/// the kernel's x86_64 struct rusage.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RusageChirho {
    /// User CPU time used.
    pub ru_utime_chirho: TimevalChirho,
    /// System CPU time used.
    pub ru_stime_chirho: TimevalChirho,
    /// Maximum resident set size.
    pub ru_maxrss_chirho: i64,
    /// Integral shared memory size.
    pub ru_ixrss_chirho: i64,
    /// Integral unshared data size.
    pub ru_idrss_chirho: i64,
    /// Integral unshared stack size.
    pub ru_isrss_chirho: i64,
    /// Page reclaims (soft page faults).
    pub ru_minflt_chirho: i64,
    /// Page faults (hard page faults).
    pub ru_majflt_chirho: i64,
    /// Swaps.
    pub ru_nswap_chirho: i64,
    /// Block input operations.
    pub ru_inblock_chirho: i64,
    /// Block output operations.
    pub ru_oublock_chirho: i64,
    /// IPC messages sent.
    pub ru_msgsnd_chirho: i64,
    /// IPC messages received.
    pub ru_msgrcv_chirho: i64,
    /// Signals received.
    pub ru_nsignals_chirho: i64,
    /// Voluntary context switches.
    pub ru_nvcsw_chirho: i64,
    /// Involuntary context switches.
    pub ru_nivcsw_chirho: i64,
}

// ============================================================================
// Rlimit64Chirho -- Linux rlimit64 structure
// ============================================================================

/// Linux `struct rlimit64` equivalent for prlimit64(2).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Rlimit64Chirho {
    /// Soft limit.
    pub rlim_cur_chirho: u64,
    /// Hard limit.
    pub rlim_max_chirho: u64,
}

/// Infinity value for resource limits.
const RLIM_INFINITY_CHIRHO: u64 = u64::MAX;

/// Resource limit constants (Linux).
const RLIMIT_STACK_CHIRHO: u64 = 3;
const RLIMIT_NOFILE_CHIRHO: u64 = 7;

// ============================================================================
// StatChirho -- Linux stat structure (x86_64)
// ============================================================================

/// Linux `struct stat` equivalent for fstat(2) on x86_64.
///
/// Layout matches the kernel's `struct stat` for x86_64.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StatChirho {
    pub st_dev_chirho: u64,
    pub st_ino_chirho: u64,
    pub st_nlink_chirho: u64,
    pub st_mode_chirho: u32,
    pub st_uid_chirho: u32,
    pub st_gid_chirho: u32,
    pub _pad0_chirho: u32,
    pub st_rdev_chirho: u64,
    pub st_size_chirho: i64,
    pub st_blksize_chirho: i64,
    pub st_blocks_chirho: i64,
    pub st_atime_chirho: u64,
    pub st_atime_nsec_chirho: u64,
    pub st_mtime_chirho: u64,
    pub st_mtime_nsec_chirho: u64,
    pub st_ctime_chirho: u64,
    pub st_ctime_nsec_chirho: u64,
    pub _unused_chirho: [i64; 3],
}

impl StatChirho {
    const fn zeroed_chirho() -> Self {
        Self {
            st_dev_chirho: 0,
            st_ino_chirho: 0,
            st_nlink_chirho: 0,
            st_mode_chirho: 0,
            st_uid_chirho: 0,
            st_gid_chirho: 0,
            _pad0_chirho: 0,
            st_rdev_chirho: 0,
            st_size_chirho: 0,
            st_blksize_chirho: 0,
            st_blocks_chirho: 0,
            st_atime_chirho: 0,
            st_atime_nsec_chirho: 0,
            st_mtime_chirho: 0,
            st_mtime_nsec_chirho: 0,
            st_ctime_chirho: 0,
            st_ctime_nsec_chirho: 0,
            _unused_chirho: [0; 3],
        }
    }
}

/// S_IFCHR -- character device mode flag.
const S_IFCHR_CHIRHO: u32 = 0o020000;
/// S_IFREG -- regular file mode flag.
const S_IFREG_CHIRHO: u32 = 0o100000;
/// Default permissions for character devices (rw-rw-rw-).
const S_IRUSR_CHIRHO: u32 = 0o400;
const S_IWUSR_CHIRHO: u32 = 0o200;
const S_IRGRP_CHIRHO: u32 = 0o040;
const S_IWGRP_CHIRHO: u32 = 0o020;
const S_IROTH_CHIRHO: u32 = 0o004;
const S_IWOTH_CHIRHO: u32 = 0o002;

// ============================================================================
// fcntl(2) command constants
// ============================================================================

/// Duplicate file descriptor (lowest >= arg).
const F_DUPFD_CHIRHO: u64 = 0;
/// Get file descriptor flags.
const F_GETFD_CHIRHO: u64 = 1;
/// Set file descriptor flags.
const F_SETFD_CHIRHO: u64 = 2;
/// `FD_CLOEXEC` bit used by `F_GETFD` / `F_SETFD`.
const FD_CLOEXEC_CHIRHO: u64 = 1;
/// Get file status flags.
const F_GETFL_CHIRHO: u64 = 3;
/// Set file status flags.
const F_SETFL_CHIRHO: u64 = 4;
/// Get advisory record lock.
const F_GETLK_CHIRHO: u64 = 5;
/// Set advisory record lock (blocking).
const F_SETLK_CHIRHO: u64 = 6;
/// Set advisory record lock (wait).
const F_SETLKW_CHIRHO: u64 = 7;
/// Duplicate fd with close-on-exec.
const F_DUPFD_CLOEXEC_CHIRHO: u64 = 1030;

// ============================================================================
// ioctl(2) command constants
// ============================================================================

/// TCGETS -- get terminal attributes.
const TCGETS_CHIRHO: u64 = 0x5401;
/// TCSETS -- set terminal attributes.
const TCSETS_CHIRHO: u64 = 0x5402;
/// TCSETSW -- set terminal attributes, drain output first.
const TCSETSW_CHIRHO: u64 = 0x5403;
/// TCSETSF -- set terminal attributes, drain + flush.
const TCSETSF_CHIRHO: u64 = 0x5404;
/// TIOCNOTTY -- give up controlling terminal.
const TIOCNOTTY_CHIRHO: u64 = 0x5422;
/// TIOCSCTTY -- become controlling terminal.
const TIOCSCTTY_CHIRHO: u64 = 0x540E;
/// TIOCGWINSZ -- get window size.
const TIOCGWINSZ_CHIRHO: u64 = 0x5413;
/// FIONREAD -- bytes available to read.
const FIONREAD_CHIRHO: u64 = 0x541B;
/// FIONBIO -- set/clear non-blocking I/O.
const FIONBIO_CHIRHO: u64 = 0x5421;
/// FIOCLEX -- set close-on-exec flag.
const FIOCLEX_CHIRHO: u64 = 0x5451;
/// TIOCGPGRP -- get foreground process group ID.
const TIOCGPGRP_CHIRHO: u64 = 0x540F;
/// TIOCSPGRP -- set foreground process group ID.
const TIOCSPGRP_CHIRHO: u64 = 0x5410;

/// Linux `struct winsize` equivalent for TIOCGWINSZ.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinsizeChirho {
    pub ws_row_chirho: u16,
    pub ws_col_chirho: u16,
    pub ws_xpixel_chirho: u16,
    pub ws_ypixel_chirho: u16,
}

// ============================================================================
// poll(2) / select(2) constants and structures
// ============================================================================

/// POLLIN -- there is data to read.
const POLLIN_CHIRHO: i16 = 1;
/// POLLOUT -- writing now will not block.
const POLLOUT_CHIRHO: i16 = 4;
/// POLLERR -- error condition.
const POLLERR_CHIRHO: i16 = 8;
/// POLLHUP -- hang up.
const POLLHUP_CHIRHO: i16 = 16;
/// POLLNVAL -- invalid request: fd not open.
const POLLNVAL_CHIRHO: i16 = 32;

/// Linux `struct pollfd` equivalent.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PollfdChirho {
    pub fd_chirho: i32,
    pub events_chirho: i16,
    pub revents_chirho: i16,
}

// ============================================================================
// clock_gettime(2) clock ID constants
// ============================================================================

/// CLOCK_REALTIME -- system-wide real-time clock.
const CLOCK_REALTIME_CHIRHO: u64 = 0;
/// CLOCK_MONOTONIC -- monotonic clock since some unspecified point.
const CLOCK_MONOTONIC_CHIRHO: u64 = 1;

// ============================================================================
// PRNG state for getrandom(2)
// ============================================================================

/// Global xorshift64 PRNG state, seeded from TSC on first use.
pub static PRNG_STATE_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// PID of the process that called listen() (0 = no daemon yet).
/// The shell never calls listen(); dropbear does. Storing the PID lets us
/// distinguish the two even though both have ppid=0 from exit_group re-exec.
static DAEMON_LISTENER_PID_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Called from sys_listen to record which PID is the daemon.
pub fn mark_daemon_listener_chirho() {
    let pid_chirho = crate::task_chirho::current_task_chirho()
        .map(|t_chirho| t_chirho.lock().pid_chirho)
        .unwrap_or(0);
    if pid_chirho != 0 {
        DAEMON_LISTENER_PID_CHIRHO.store(pid_chirho, Ordering::Relaxed);
        crate::serial_println_chirho!("[DAEMON] PID {} called listen — marked as daemon", pid_chirho);
    }
}

fn is_interactive_shell_chirho() -> bool {
    crate::task_chirho::current_task_chirho()
        .map(|t_chirho| {
            let task_chirho = t_chirho.lock();
            // Forked children (ppid != 0) are never the main shell.
            if task_chirho.ppid_chirho != 0 {
                return false;
            }
            // ppid=0: check if this PID is the daemon that called listen().
            let daemon_pid_chirho = DAEMON_LISTENER_PID_CHIRHO.load(Ordering::Relaxed);
            if daemon_pid_chirho != 0 && task_chirho.pid_chirho == daemon_pid_chirho {
                return false; // this is the daemon, not the shell
            }
            true
        })
        .unwrap_or(true)
}

/// Global tick counter for clock_gettime monotonic approximation.
static TICK_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Approximate boot timestamp (seconds since Unix epoch).
/// March 17, 2026 00:00:00 UTC = 1773878400.
/// CLOCK_REALTIME = BOOT_EPOCH_CHIRHO + monotonic offset from tick counter.
const BOOT_EPOCH_CHIRHO: i64 = 1773878400;

/// Tick period in nanoseconds.  The timer IRQ fires every ~10 ms.
const TICK_PERIOD_NS_CHIRHO: i64 = 10_000_000; // 10 ms

// ============================================================================
// Program break tracking (for brk)
// ============================================================================

/// Current program break address.  Initialised to a reasonable default; a real
/// process loader would set this to the end of the BSS/data segment.
static CURRENT_BRK_CHIRHO: AtomicU64 = AtomicU64::new(0x0060_0000);

/// Current executable path (for /proc/self/exe readlink).
/// Updated by execve when a new binary is loaded.
static CURRENT_EXE_PATH_CHIRHO: spin::Mutex<[u8; 256]> = spin::Mutex::new([0u8; 256]);
static CURRENT_EXE_PATH_LEN_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Set the current executable path (called from execve).
pub fn set_current_exe_path_chirho(path_chirho: &[u8]) {
    let mut buf_chirho = CURRENT_EXE_PATH_CHIRHO.lock();
    let len_chirho = core::cmp::min(path_chirho.len(), 255);
    buf_chirho[..len_chirho].copy_from_slice(&path_chirho[..len_chirho]);
    buf_chirho[len_chirho] = 0;
    CURRENT_EXE_PATH_LEN_CHIRHO.store(len_chirho as u64, Ordering::Relaxed);
}

/// Set the initial program break (called by exec after loading ELF).
/// Updates both the global fallback AND the current task's per-process brk.
/// Last syscall number for post-mortem debugging.
pub static LAST_SYSCALL_NR_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn set_brk_chirho(addr_chirho: u64) {
    // Always keep the global as a fallback for PID 0 / init context
    CURRENT_BRK_CHIRHO.store(addr_chirho, core::sync::atomic::Ordering::SeqCst);

    // Also set the per-process brk on the current task (A2-AUDIT-003)
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let mut task_chirho = task_arc_chirho.lock();
        task_chirho.brk_chirho = addr_chirho;
        // If brk_start was never initialised, set it to the same value
        if task_chirho.brk_start_chirho == 0 {
            task_chirho.brk_start_chirho = addr_chirho;
        }
    }
}

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

// Segment selectors and STAR MSR value are defined centrally in gdt_chirho.
// Re-use them here to stay DRY and avoid selector layout mismatches.
use crate::gdt_chirho::STAR_MSR_VALUE_CHIRHO;

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
    // Bits 47:32 = Kernel CS selector (0x08)
    // Bits 63:48 = User CS base for SYSRET (0x18)
    // Value is pre-computed in gdt_chirho::STAR_MSR_VALUE_CHIRHO.
    let mut star_msr_chirho = Msr::new(IA32_STAR_CHIRHO);
    star_msr_chirho.write(STAR_MSR_VALUE_CHIRHO);

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

    crate::serial_debug_chirho!("[SYSCALL] MSRs configured (STAR, LSTAR, FMASK, EFER.SCE)");
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

    // Syscall trace (disabled — enable for debugging)
    // crate::serial_println_chirho!("[SC] nr={} a0={:#x}", syscall_nr_chirho, arg0_chirho);
    let arg4_chirho = frame_chirho.r8_chirho;
    let _arg5_chirho = frame_chirho.r9_chirho;

    // Track last syscall for post-mortem debugging
    LAST_SYSCALL_NR_CHIRHO.store(syscall_nr_chirho, core::sync::atomic::Ordering::Relaxed);

    // Log all syscalls from PID >= 2 (dropbear + children) to trace fork flow
    static POST_ACCEPT_ARMED_CHIRHO: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    if syscall_nr_chirho == 43 || syscall_nr_chirho == 288 {
        POST_ACCEPT_ARMED_CHIRHO.store(true, core::sync::atomic::Ordering::SeqCst);
    }
    if POST_ACCEPT_ARMED_CHIRHO.load(core::sync::atomic::Ordering::SeqCst) {
        let pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(0);
        if pid_chirho >= 2 {
            crate::serial_debug_chirho!(
                "[SC] pid={} nr={}({})", pid_chirho, syscall_nr_chirho,
                syscall_name_chirho(syscall_nr_chirho),
            );
        }
    }
    // Temporary: log syscalls after accept (nr > 40 range)
    static AFTER_ACCEPT_CHIRHO: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    if syscall_nr_chirho == 43 || syscall_nr_chirho == 288 { // accept/accept4
        AFTER_ACCEPT_CHIRHO.store(true, core::sync::atomic::Ordering::SeqCst);
    }
    if AFTER_ACCEPT_CHIRHO.load(core::sync::atomic::Ordering::SeqCst) {
        let name_chirho = syscall_name_chirho(syscall_nr_chirho);
        crate::serial_debug_chirho!("[POST-ACCEPT] nr={}({})", syscall_nr_chirho, name_chirho);
    }

    let result_chirho: i64 = match syscall_nr_chirho {
        SYS_READ_CHIRHO => {
            if arg0_chirho == 0 {
                // stdin: check if fd=0 has been redirected (dup2/pipe).
                // For the init shell (PID 0), use direct serial poll.
                // For fork children (PID 3+), use VFS which handles pipes.
                // Use direct serial poll for stdin (fd=0) when the fd
                // has not been redirected via dup2/pipe.  The re-exec'd
                // shell inherits fd=0 pointing to the serial console,
                // but VFS read returns EOF since the serial console file
                // has no data buffered.  Check if fd=0 has a real VFS
                // entry with pipe/PTY ops before falling through.
                let has_vfs_stdin_chirho = crate::task_chirho::current_task_chirho()
                    .and_then(|t| {
                        let task_chirho = t.lock();
                        task_chirho.fd_table_chirho.as_ref()
                            .and_then(|fdt| fdt.get_chirho(0))
                    })
                    .is_some();
                if has_vfs_stdin_chirho {
                    crate::fs_chirho::sys_read_real_chirho(arg0_chirho, arg1_chirho, arg2_chirho as usize)
                } else {
                    // For the main shell (PID 0 or re-exec'd shell), block on
                    // serial input. For daemon children (dropbear PID 3+),
                    // return EAGAIN so they don't spin on empty stdin reads.
                    
                    if is_interactive_shell_chirho() {
                        sys_read_stdin_chirho(arg1_chirho, arg2_chirho as usize)
                    } else {
                        // Check serial port — if no data ready, return EAGAIN
                        let lsr_chirho: u8 = unsafe {
                            x86_64::instructions::port::Port::<u8>::new(0x3FD).read()
                        };
                        if lsr_chirho & 1 != 0 {
                            sys_read_stdin_chirho(arg1_chirho, arg2_chirho as usize)
                        } else {
                            -11 // EAGAIN
                        }
                    }
                }
            } else if crate::net_chirho::is_socket_fd_chirho(arg0_chirho) {
                // Socket fd → recvfrom
                crate::net_chirho::sys_recvfrom_chirho(
                    arg0_chirho, arg1_chirho, arg2_chirho, 0, 0, 0,
                )
            } else {
                crate::fs_chirho::sys_read_real_chirho(arg0_chirho, arg1_chirho, arg2_chirho as usize)
            }
        },
        SYS_WRITE_CHIRHO => {
            // SSH redirect: daemon writing to fd=1/2 (console) when fd=0
            // is a TCP socket → redirect to fd=0 so SSH data goes over TCP.
            // SSH redirect: if this is a daemon (not shell) writing to
            // stdout/stderr, and there's an established TCP connection on
            // port 2222, send the data directly via TCP instead of serial.
            // This handles dropbear which expects stdin/stdout to be the
            // TCP socket but our kernel doesn't dup2 them automatically.
            let write_fd_chirho = if (arg0_chirho == 1 || arg0_chirho == 2)
                && !is_interactive_shell_chirho()
                && (
                    crate::net_chirho::has_tcp_data_for_port_chirho(2222)
                    || crate::net_chirho::has_established_tcp_chirho(2222)
                )
            {
                // Send directly via TCP, bypass fd table
                let data_count_chirho = core::cmp::min(arg2_chirho as usize, 65536);
                let mut data_chirho = alloc::vec![0u8; data_count_chirho];
                for i_chirho in 0..data_count_chirho {
                    data_chirho[i_chirho] = unsafe {
                        core::ptr::read_volatile((arg1_chirho as *const u8).add(i_chirho))
                    };
                }
                crate::net_chirho::relay_to_tcp_2222_chirho(&data_chirho);
                return data_count_chirho as i64;
            } else {
                arg0_chirho
            };
            if fd_uses_console_stdio_chirho(write_fd_chirho) {
                sys_write_chirho(write_fd_chirho, arg1_chirho as *const u8, arg2_chirho as usize)
            } else {
                if arg0_chirho == 1 || arg0_chirho == 2 {
                    crate::serial_debug_chirho!(
                        "[WRITE] fd={} redirected away from console (pid={}, {} bytes)",
                        arg0_chirho,
                        crate::task_chirho::current_task_chirho()
                            .map(|t| t.lock().pid_chirho).unwrap_or(999),
                        arg2_chirho
                    )
                }
                sys_write_fd_dispatch_chirho(arg0_chirho, arg1_chirho, arg2_chirho as usize)
            }
        },
        SYS_OPEN_CHIRHO => crate::fs_chirho::sys_open_chirho(
            arg0_chirho,  // pathname
            arg1_chirho as u32, // flags
            arg2_chirho as u32, // mode
        ),
        SYS_CLOSE_CHIRHO => {
            let pid_dbg_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().pid_chirho).unwrap_or(0);
            if pid_dbg_chirho >= 3 {
                let fd0_exists_chirho = crate::fs_chirho::lookup_fd_chirho(0).is_some();
                crate::serial_debug_chirho!(
                    "[CLOSE] pid={} close({}) fd0_exists={}",
                    pid_dbg_chirho, arg0_chirho, fd0_exists_chirho
                );
            }
            crate::fs_chirho::sys_close_real_chirho(arg0_chirho)
        },
        SYS_FSTAT_CHIRHO => sys_fstat_chirho(arg0_chirho, arg1_chirho as *mut StatChirho),
        SYS_STAT_CHIRHO => sys_stat_chirho(
            arg0_chirho as *const u8,
            arg1_chirho as *mut StatChirho,
        ),
        SYS_LSTAT_CHIRHO => sys_lstat_chirho(
            arg0_chirho as *const u8,
            arg1_chirho as *mut StatChirho,
        ),
        SYS_POLL_CHIRHO => sys_poll_chirho(arg0_chirho, arg1_chirho as u32, arg2_chirho as i32),
        SYS_LSEEK_CHIRHO => crate::fs_chirho::sys_lseek_chirho(
            arg0_chirho,
            arg1_chirho as i64,
            arg2_chirho as u32,
        ),
        SYS_MMAP_CHIRHO => sys_mmap_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho as u32,
            arg3_chirho as u32,
            arg4_chirho as i32,
            _arg5_chirho,
        ),
        SYS_MPROTECT_CHIRHO => sys_mprotect_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho as u32,
        ),
        SYS_MUNMAP_CHIRHO => sys_munmap_chirho(
            arg0_chirho,
            arg1_chirho,
        ),
        SYS_BRK_CHIRHO => sys_brk_chirho(arg0_chirho),
        SYS_RT_SIGACTION_CHIRHO => crate::signal_chirho::sys_rt_sigaction_chirho(
            arg0_chirho as u32,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
        ),
        SYS_RT_SIGPROCMASK_CHIRHO => crate::signal_chirho::sys_rt_sigprocmask_chirho(
            arg0_chirho as u32,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
        ),
        SYS_RT_SIGRETURN_CHIRHO => {
            // Stub: signal frame restoration not yet implemented.
            // Return 0 so BusyBox doesn't crash on signal handler return.
            crate::serial_debug_chirho!("[SYSCALL] rt_sigreturn (stub, returning 0)");
            0
        },
        SYS_IOCTL_CHIRHO => sys_ioctl_real_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
        ),
        SYS_PREAD64_CHIRHO => sys_pread64_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho as usize,
            arg3_chirho as i64,
        ),
        SYS_PWRITE64_CHIRHO => sys_pwrite64_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho as usize,
            arg3_chirho as i64,
        ),
        SYS_READV_CHIRHO => sys_readv_chirho(
            arg0_chirho,
            arg1_chirho as *const IoVecChirho,
            arg2_chirho as i32,
        ),
        SYS_WRITEV_CHIRHO => {
            // SSH redirect: daemon writev to stdout/stderr with active
            // TCP connection on port 2222 → send directly via TCP
            if (arg0_chirho == 1 || arg0_chirho == 2)
                && !is_interactive_shell_chirho()
                && crate::net_chirho::has_established_tcp_chirho(2222)
            {
                // Gather iovec data and send via TCP relay
                let iovecs_chirho = arg1_chirho as *const IoVecChirho;
                let iovcnt_chirho = arg2_chirho as usize;
                let mut total_chirho: usize = 0;
                for i_chirho in 0..core::cmp::min(iovcnt_chirho, 16) {
                    let iov_chirho = unsafe { &*iovecs_chirho.add(i_chirho) };
                    if iov_chirho.iov_len_chirho > 0 && !iov_chirho.iov_base_chirho.is_null() {
                        let len_chirho = core::cmp::min(iov_chirho.iov_len_chirho, 4096);
                        let mut buf_chirho = alloc::vec![0u8; len_chirho];
                        for j_chirho in 0..len_chirho {
                            buf_chirho[j_chirho] = unsafe {
                                core::ptr::read_volatile(iov_chirho.iov_base_chirho.add(j_chirho))
                            };
                        }
                        crate::net_chirho::relay_to_tcp_2222_chirho(&buf_chirho);
                        total_chirho += len_chirho;
                    }
                }
                total_chirho as i64
            } else {
                sys_writev_chirho(
                    arg0_chirho,
                    arg1_chirho as *const IoVecChirho,
                    arg2_chirho as i32,
                )
            }
        },
        SYS_ACCESS_CHIRHO => sys_faccessat_real_chirho(-100, arg0_chirho, arg1_chirho as u32, 0),
        SYS_PIPE_CHIRHO => crate::pipe_chirho::sys_pipe_chirho(arg0_chirho),
        SYS_SELECT_CHIRHO => sys_select_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho),
        SYS_SCHED_YIELD_CHIRHO => {
            crate::scheduler_chirho::yield_current_chirho();
            0
        }
        SYS_MREMAP_CHIRHO => -ENOSYS_CHIRHO,
        SYS_MSYNC_CHIRHO => 0,   // stub: silently succeed
        SYS_MINCORE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_MADVISE_CHIRHO => 0, // advisory, silently ignore
        SYS_DUP_CHIRHO => crate::fs_chirho::sys_dup_chirho(arg0_chirho),
        SYS_DUP2_CHIRHO => {
            crate::serial_debug_chirho!("[DUP2] dup2({}, {})", arg0_chirho, arg1_chirho);
            crate::fs_chirho::sys_dup2_chirho(arg0_chirho, arg1_chirho)
        },
        SYS_PAUSE_CHIRHO => -EINTR_CHIRHO,
        SYS_NANOSLEEP_CHIRHO => sys_clock_nanosleep_chirho(
            1, // CLOCK_MONOTONIC
            0, // relative
            arg0_chirho,
            arg1_chirho,
        ),
        SYS_GETITIMER_CHIRHO | SYS_SETITIMER_CHIRHO => -ENOSYS_CHIRHO,
        SYS_ALARM_CHIRHO => 0,                  // return 0 (no previous alarm)
        SYS_GETPID_CHIRHO => sys_getpid_chirho(),
        // --- Phase 6: Network socket stubs ---
        SYS_SOCKET_CHIRHO => crate::net_chirho::sys_socket_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_CONNECT_CHIRHO => crate::net_chirho::sys_connect_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_ACCEPT_CHIRHO => crate::net_chirho::sys_accept_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_SENDTO_CHIRHO => crate::net_chirho::sys_sendto_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
            arg3_chirho, arg4_chirho, _arg5_chirho,
        ),
        SYS_RECVFROM_CHIRHO => crate::net_chirho::sys_recvfrom_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
            arg3_chirho, arg4_chirho, _arg5_chirho,
        ),
        SYS_SENDMSG_CHIRHO => crate::net_chirho::sys_sendmsg_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_RECVMSG_CHIRHO => crate::net_chirho::sys_recvmsg_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_SHUTDOWN_CHIRHO => crate::net_chirho::sys_shutdown_chirho(
            arg0_chirho, arg1_chirho,
        ),
        SYS_BIND_CHIRHO => crate::net_chirho::sys_bind_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_LISTEN_CHIRHO => crate::net_chirho::sys_listen_chirho(
            arg0_chirho, arg1_chirho,
        ),
        SYS_GETSOCKNAME_CHIRHO => crate::net_chirho::sys_getsockname_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_GETPEERNAME_CHIRHO => crate::net_chirho::sys_getpeername_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_SOCKETPAIR_CHIRHO => crate::net_chirho::sys_socketpair_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho,
        ),
        SYS_SETSOCKOPT_CHIRHO => crate::net_chirho::sys_setsockopt_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho,
        ),
        SYS_GETSOCKOPT_CHIRHO => crate::net_chirho::sys_getsockopt_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho,
        ),
        SYS_ACCEPT4_CHIRHO => crate::net_chirho::sys_accept4_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho,
        ),

        // --- Phase 7 partial: Security stubs ---
        SYS_CAPGET_CHIRHO | SYS_CAPSET_CHIRHO => 0,
        SYS_SECCOMP_CHIRHO => 0,
        SYS_BPF_CHIRHO => crate::bpf_chirho::sys_bpf_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_UNSHARE_CHIRHO => 0,
        SYS_SETNS_CHIRHO => -ENOSYS_CHIRHO,
        SYS_LANDLOCK_CREATE_RULESET_CHIRHO => -ENOSYS_CHIRHO,

        SYS_CLONE_CHIRHO => crate::process_chirho::sys_clone_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
            arg4_chirho,
            frame_chirho,
        ),
        SYS_FORK_CHIRHO | SYS_VFORK_CHIRHO => crate::process_chirho::sys_fork_chirho(frame_chirho),
        SYS_EXECVE_CHIRHO => crate::process_chirho::sys_execve_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
        ),
        SYS_EXIT_CHIRHO => sys_exit_chirho(arg0_chirho as i32),
        SYS_WAIT4_CHIRHO => crate::process_chirho::sys_wait4_chirho(
            arg0_chirho as i64,
            arg1_chirho,
            arg2_chirho as u32,
            arg3_chirho,
        ),
        SYS_KILL_CHIRHO => crate::signal_chirho::sys_kill_chirho(
            arg0_chirho,
            arg1_chirho as u32,
        ),
        SYS_UNAME_CHIRHO => sys_uname_chirho(arg0_chirho as *mut UtsNameChirho),
        SYS_FCNTL_CHIRHO => sys_fcntl_chirho(arg0_chirho, arg1_chirho, arg2_chirho),
        SYS_FLOCK_CHIRHO => 0,     // stub: silently succeed
        SYS_FSYNC_CHIRHO => 0,     // stub: silently succeed
        SYS_FDATASYNC_CHIRHO => 0, // stub: silently succeed
        SYS_TRUNCATE_CHIRHO => 0,  // stub: silently succeed
        SYS_FTRUNCATE_CHIRHO => {
            // ftruncate(fd, length): set file size. SQLite needs this.
            // A2-PROC-003: Use lookup_fd_chirho (per-process first).
            crate::serial_debug_chirho!("[SYSCALL] ftruncate(fd={}, len={})", arg0_chirho, arg1_chirho);
            let file_arc_chirho = crate::fs_chirho::lookup_fd_chirho(arg0_chirho);
            match file_arc_chirho {
                Some(fa_chirho) => {
                    let fg_chirho = fa_chirho.lock();
                    let mut ig_chirho = fg_chirho.inode_chirho.lock();
                    ig_chirho.size_chirho = arg1_chirho;
                    0
                }
                None => -EBADF_CHIRHO,
            }
        },
        SYS_GETDENTS_CHIRHO => sys_getdents_chirho(
            arg0_chirho,
            arg1_chirho as *mut u8,
            arg2_chirho as usize,
        ),
        SYS_GETCWD_CHIRHO => sys_getcwd_chirho(arg0_chirho as *mut u8, arg1_chirho as usize),
        SYS_CHDIR_CHIRHO => sys_chdir_chirho(arg0_chirho as *const u8),
        SYS_RENAME_CHIRHO => sys_rename_chirho(
            arg0_chirho as *const u8,
            arg1_chirho as *const u8,
        ),
        SYS_MKDIR_CHIRHO => sys_mkdir_chirho(arg0_chirho as *const u8, arg1_chirho as u32),
        SYS_RMDIR_CHIRHO => sys_rmdir_chirho(arg0_chirho as *const u8),
        SYS_CREAT_CHIRHO | SYS_LINK_CHIRHO => -ENOSYS_CHIRHO,
        SYS_UNLINK_CHIRHO => sys_unlink_chirho(arg0_chirho as *const u8),
        SYS_SYMLINK_CHIRHO => sys_symlinkat_chirho(
            arg0_chirho, // target
            -100,        // AT_FDCWD
            arg1_chirho, // linkpath
        ),
        SYS_READLINK_CHIRHO => sys_readlink_chirho(
            arg0_chirho as *const u8,
            arg1_chirho as *mut u8,
            arg2_chirho as usize,
        ),
        SYS_CHMOD_CHIRHO | SYS_CHOWN_CHIRHO => -ENOENT_CHIRHO,
        SYS_GETUID_CHIRHO => sys_getuid_chirho(),
        // Phase 4: Credentials -- setuid/setgid family (stubs, always root)
        SYS_SETUID_CHIRHO => 0,
        SYS_SETGID_CHIRHO => 0,
        SYS_GETEUID_CHIRHO => sys_geteuid_chirho(),
        SYS_GETGID_CHIRHO => sys_getgid_chirho(),
        SYS_GETEGID_CHIRHO => sys_getegid_chirho(),
        SYS_GETPPID_CHIRHO => sys_getppid_chirho(),
        // Phase 5: Real process group/session infrastructure
        SYS_SETPGID_CHIRHO => sys_setpgid_chirho(arg0_chirho, arg1_chirho),
        SYS_GETPGRP_CHIRHO => sys_getpgrp_chirho(),
        SYS_SETSID_CHIRHO => sys_setsid_chirho(),
        SYS_SETREUID_CHIRHO => 0,
        SYS_SETREGID_CHIRHO => 0,
        SYS_GETGROUPS_CHIRHO => 0,
        SYS_SETGROUPS_CHIRHO => 0,
        SYS_SETRESUID_CHIRHO => 0,
        SYS_GETRESUID_CHIRHO => {
            // Write 0 (root) to ruid, euid, suid pointers
            if arg0_chirho != 0 { unsafe { *(arg0_chirho as *mut u32) = 0; } }
            if arg1_chirho != 0 { unsafe { *(arg1_chirho as *mut u32) = 0; } }
            if arg2_chirho != 0 { unsafe { *(arg2_chirho as *mut u32) = 0; } }
            0
        },
        SYS_SETRESGID_CHIRHO => 0,
        SYS_GETRESGID_CHIRHO => {
            // Write 0 (root) to rgid, egid, sgid pointers
            if arg0_chirho != 0 { unsafe { *(arg0_chirho as *mut u32) = 0; } }
            if arg1_chirho != 0 { unsafe { *(arg1_chirho as *mut u32) = 0; } }
            if arg2_chirho != 0 { unsafe { *(arg2_chirho as *mut u32) = 0; } }
            0
        },
        SYS_GETPGID_CHIRHO => sys_getpgid_chirho(arg0_chirho),
        SYS_GETSID_CHIRHO => sys_getsid_chirho(arg0_chirho),
        // Phase 4: Additional signal syscalls
        SYS_RT_SIGPENDING_CHIRHO => crate::signal_chirho::sys_rt_sigpending_chirho(arg0_chirho),
        SYS_RT_SIGSUSPEND_CHIRHO => crate::signal_chirho::sys_rt_sigsuspend_chirho(arg0_chirho),
        SYS_SIGALTSTACK_CHIRHO => crate::signal_chirho::sys_sigaltstack_chirho(arg0_chirho, arg1_chirho),
        SYS_TKILL_CHIRHO => crate::signal_chirho::sys_tkill_chirho(arg0_chirho, arg1_chirho as u32),
        SYS_TGKILL_CHIRHO => crate::signal_chirho::sys_tgkill_chirho(arg0_chirho, arg1_chirho, arg2_chirho as u32),
        SYS_ARCH_PRCTL_CHIRHO => sys_arch_prctl_chirho(arg0_chirho, arg1_chirho),
        SYS_GETTID_CHIRHO => sys_gettid_chirho(),
        SYS_FUTEX_CHIRHO => crate::futex_chirho::sys_futex_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho, _arg5_chirho,
        ),
        SYS_SET_TID_ADDRESS_CHIRHO => sys_set_tid_address_chirho(arg0_chirho as *mut i32),
        SYS_CLOCK_GETTIME_CHIRHO => sys_clock_gettime_chirho(
            arg0_chirho,
            arg1_chirho as *mut TimespecChirho,
        ),
        // clock_getres(2): musl calls this during __libc_start_main
        // A2-AUDIT-002: report real 10ms tick resolution
        SYS_CLOCK_GETRES_CHIRHO => {
            if arg1_chirho != 0 {
                let res_chirho = TimespecChirho {
                    tv_sec_chirho: 0,
                    tv_nsec_chirho: TICK_PERIOD_NS_CHIRHO, // 10ms real resolution
                };
                unsafe { core::ptr::write(arg1_chirho as *mut TimespecChirho, res_chirho); }
            }
            0
        },
        SYS_EXIT_GROUP_CHIRHO => sys_exit_group_chirho(arg0_chirho as i32),
        SYS_OPENAT_CHIRHO => crate::fs_chirho::sys_openat_chirho(
            arg0_chirho as i64,  // dirfd
            arg1_chirho,         // pathname
            arg2_chirho as u32,  // flags
            arg3_chirho as u32,  // mode
        ),
        SYS_MKDIRAT_CHIRHO => sys_mkdirat_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as u32,
        ),
        SYS_NEWFSTATAT_CHIRHO => sys_fstatat_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as *mut StatChirho,
            arg3_chirho as u32,
        ),
        SYS_UNLINKAT_CHIRHO => sys_unlinkat_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as u32,
        ),
        SYS_SET_ROBUST_LIST_CHIRHO => 0,        // silently succeed
        SYS_GET_ROBUST_LIST_CHIRHO => -ENOSYS_CHIRHO,
        SYS_FACCESSAT_CHIRHO => sys_faccessat_real_chirho(
            arg0_chirho as i32,
            arg1_chirho,
            arg2_chirho as u32,
            arg3_chirho as u32,
        ),
        SYS_READLINKAT_CHIRHO => sys_readlinkat_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as *mut u8,
            arg3_chirho as usize,
        ),
        SYS_PRLIMIT64_CHIRHO => sys_prlimit64_chirho(
            arg0_chirho as u32,
            arg1_chirho,
            arg2_chirho as *const Rlimit64Chirho,
            arg3_chirho as *mut Rlimit64Chirho,
        ),
        SYS_GETRANDOM_CHIRHO => sys_getrandom_chirho(
            arg0_chirho as *mut u8,
            arg1_chirho as usize,
            arg2_chirho as u32,
        ),
        SYS_PIPE2_CHIRHO => crate::pipe_chirho::sys_pipe2_chirho(
            arg0_chirho,
            arg1_chirho as u32,
        ),
        SYS_GETDENTS64_CHIRHO => sys_getdents64_chirho(
            arg0_chirho,
            arg1_chirho as *mut u8,
            arg2_chirho as usize,
        ),
        // renameat (264) — same as renameat2 with flags=0
        264 => sys_renameat2_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as i32,
            arg3_chirho as *const u8,
            0,
        ),
        SYS_RENAMEAT2_CHIRHO => sys_renameat2_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as i32,
            arg3_chirho as *const u8,
            arg4_chirho as u32,
        ),
        SYS_STATX_CHIRHO => sys_statx_chirho(
            arg0_chirho as i32,
            arg1_chirho as *const u8,
            arg2_chirho as u32,
            arg3_chirho as u32,
            arg4_chirho as *mut u8,
        ),
        SYS_STATFS_CHIRHO => sys_statfs_chirho(arg1_chirho as *mut StatfsChirho),
        SYS_FSTATFS_CHIRHO => sys_statfs_chirho(arg1_chirho as *mut StatfsChirho),
        SYS_SYSINFO_CHIRHO => sys_sysinfo_chirho(arg0_chirho as *mut SysinfoChirho),
        SYS_MKNOD_CHIRHO => 0,   // stub: silently succeed
        SYS_PERSONALITY_CHIRHO => sys_personality_chirho(arg0_chirho),
        SYS_PRCTL_CHIRHO => sys_prctl_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
            arg4_chirho,
        ),
        SYS_SCHED_SETAFFINITY_CHIRHO => 0, // stub: accept
        SYS_SCHED_GETAFFINITY_CHIRHO => sys_sched_getaffinity_chirho(
            arg0_chirho,
            arg1_chirho as u32,
            arg2_chirho as *mut u8,
        ),
        SYS_CLOCK_NANOSLEEP_CHIRHO => sys_clock_nanosleep_chirho(
            arg0_chirho as u32,
            arg1_chirho as u32,
            arg2_chirho,
            arg3_chirho,
        ),
        SYS_MKNODAT_CHIRHO => 0,  // stub: silently succeed
        SYS_TIMERFD_CREATE_CHIRHO => sys_fake_fd_chirho(),
        SYS_SIGNALFD4_CHIRHO => sys_fake_fd_chirho(),
        SYS_EVENTFD2_CHIRHO => sys_fake_fd_chirho(),
        SYS_DUP3_CHIRHO => crate::fs_chirho::sys_dup3_chirho(arg0_chirho, arg1_chirho, arg2_chirho as u32),
        SYS_MEMFD_CREATE_CHIRHO => sys_fake_fd_chirho(),
        SYS_RSEQ_CHIRHO => -ENOSYS_CHIRHO,

        // Phase 4: Timer/event syscalls
        SYS_TIMERFD_SETTIME_CHIRHO => 0,  // stub: succeed
        SYS_TIMERFD_GETTIME_CHIRHO => 0,  // stub: succeed
        SYS_EVENTFD_CHIRHO => sys_fake_fd_chirho(), // stub: return fake fd

        // mount / umount
        SYS_MOUNT_CHIRHO => sys_mount_chirho(
            arg0_chirho,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
            arg4_chirho,
        ),
        SYS_UMOUNT2_CHIRHO => sys_umount2_chirho(arg0_chirho, arg1_chirho as u32),

        // epoll family
        213 => sys_epoll_create1_chirho(0), // epoll_create(size) — ignore size, same as create1(0)
        SYS_EPOLL_CREATE1_CHIRHO => sys_epoll_create1_chirho(arg0_chirho as u32),
        SYS_EPOLL_CTL_CHIRHO => sys_epoll_ctl_chirho(
            arg0_chirho as i32,
            arg1_chirho as i32,
            arg2_chirho as i32,
            arg3_chirho,
        ),
        SYS_EPOLL_WAIT_CHIRHO => sys_epoll_wait_chirho(
            arg0_chirho as i32,
            arg1_chirho,
            arg2_chirho as i32,
            arg3_chirho as i32,
        ),
        SYS_EPOLL_PWAIT_CHIRHO => sys_epoll_wait_chirho(
            arg0_chirho as i32,
            arg1_chirho,
            arg2_chirho as i32,
            arg3_chirho as i32,
        ),

        // pselect6 / ppoll
        SYS_PSELECT6_CHIRHO => sys_select_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho),
        SYS_PPOLL_CHIRHO => sys_ppoll_chirho(arg0_chirho, arg1_chirho as u32, arg2_chirho, arg3_chirho, arg4_chirho),

        // --- Phase 8+9: sendfile, splice, tee, vmsplice, copy_file_range ---
        SYS_SENDFILE_CHIRHO => sys_sendfile_chirho(
            arg0_chirho,       // out_fd
            arg1_chirho,       // in_fd
            arg2_chirho,       // offset ptr (or NULL)
            arg3_chirho as usize, // count
        ),
        SYS_SPLICE_CHIRHO | SYS_TEE_CHIRHO | SYS_VMSPLICE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_COPY_FILE_RANGE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_FALLOCATE_CHIRHO => 0,    // stub: silently succeed
        SYS_FADVISE64_CHIRHO => 0,    // advisory, silently ignore
        SYS_SYNC_CHIRHO => 0,         // stub: silently succeed

        // --- Phase 8+9: memory locking ---
        SYS_MLOCK_CHIRHO | SYS_MUNLOCK_CHIRHO => 0,
        SYS_MLOCK2_CHIRHO => 0,
        SYS_MLOCKALL_CHIRHO | SYS_MUNLOCKALL_CHIRHO => 0,

        // --- Phase 8+9: process/thread/scheduling ---
        SYS_CLONE3_CHIRHO => -ENOSYS_CHIRHO,
        SYS_EXECVEAT_CHIRHO => sys_execveat_real_chirho(
            arg0_chirho as i32,
            arg1_chirho,
            arg2_chirho,
            arg3_chirho,
            arg4_chirho as u32,
        ),
        SYS_WAITID_CHIRHO => -ENOSYS_CHIRHO,
        SYS_PTRACE_CHIRHO => -EPERM_CHIRHO,
        SYS_SCHED_GETSCHEDULER_CHIRHO => 0,   // SCHED_NORMAL
        SYS_SCHED_SETSCHEDULER_CHIRHO => 0,
        SYS_SCHED_GETPARAM_CHIRHO => {
            // Write a zeroed sched_param (priority=0) to user buf
            if arg1_chirho != 0 {
                unsafe { core::ptr::write_bytes(arg1_chirho as *mut u8, 0, 4); }
            }
            0
        },
        SYS_SCHED_SETPARAM_CHIRHO => 0,
        SYS_SCHED_GET_PRIORITY_MAX_CHIRHO => 99,
        SYS_SCHED_GET_PRIORITY_MIN_CHIRHO => 0,
        SYS_GETPRIORITY_CHIRHO => 20,  // nice 0
        SYS_SETPRIORITY_CHIRHO => 0,

        // --- Phase 8+9: time ---
        SYS_GETTIMEOFDAY_CHIRHO => {
            // A2-AUDIT-002: real time from tick counter + boot epoch
            if arg0_chirho != 0 {
                let (mono_sec_chirho, mono_nsec_chirho) = monotonic_from_ticks_chirho();
                let tv_chirho = TimevalChirho {
                    tv_sec_chirho: BOOT_EPOCH_CHIRHO + mono_sec_chirho,
                    tv_usec_chirho: mono_nsec_chirho / 1_000, // ns to us
                };
                unsafe { core::ptr::write(arg0_chirho as *mut TimevalChirho, tv_chirho); }
            }
            0
        },
        SYS_SETTIMEOFDAY_CHIRHO => -EPERM_CHIRHO,
        SYS_TIMER_CREATE_CHIRHO | SYS_TIMER_SETTIME_CHIRHO
        | SYS_TIMER_GETTIME_CHIRHO | SYS_TIMER_DELETE_CHIRHO => -ENOSYS_CHIRHO,
        SYS_TIMES_CHIRHO => 0,

        // --- Phase 8+9: system ---
        SYS_SYSLOG_CHIRHO => -EPERM_CHIRHO,
        SYS_REBOOT_CHIRHO => {
            crate::power_chirho::sys_reboot_real_chirho(
                arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho,
            );
            // sys_reboot_real_chirho is diverging (-> !), unreachable
        },
        SYS_GETRUSAGE_CHIRHO => {
            // Write a zeroed rusage struct
            if arg1_chirho != 0 {
                unsafe { core::ptr::write_bytes(arg1_chirho as *mut u8, 0, core::mem::size_of::<RusageChirho>()); }
            }
            0
        },
        SYS_SETHOSTNAME_CHIRHO => 0,

        // --- Phase 8+9: extended attributes ---
        SYS_SETXATTR_CHIRHO | SYS_GETXATTR_CHIRHO
        | SYS_LISTXATTR_CHIRHO | SYS_REMOVEXATTR_CHIRHO => -ENOTSUP_CHIRHO,

        // --- Phase 9: io_uring ---
        SYS_IO_URING_SETUP_CHIRHO => crate::io_uring_chirho::sys_io_uring_setup_chirho(
            arg0_chirho, arg1_chirho,
        ),
        SYS_IO_URING_ENTER_CHIRHO => crate::io_uring_chirho::sys_io_uring_enter_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho,
        ),
        SYS_IO_URING_REGISTER_CHIRHO => crate::io_uring_chirho::sys_io_uring_register_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho,
        ),

        // --- Phase 10: Massive syscall coverage ---

        // Permission-related: we're root, silently succeed
        SYS_FCHMOD_CHIRHO => 0,
        SYS_FCHMODAT_CHIRHO => sys_fchmodat_chirho(
            arg0_chirho as i32,
            arg1_chirho,
            arg2_chirho as u32,
            arg3_chirho as u32,
        ),
        SYS_FCHOWN_CHIRHO | SYS_FCHOWNAT_CHIRHO | SYS_LCHOWN_CHIRHO => 0,
        SYS_UMASK_CHIRHO => {
            // umask returns the previous mask; stub always returns 0o022
            0o022
        },

        // Resource limits
        SYS_GETRLIMIT_CHIRHO => {
            // Write a permissive rlimit struct
            if arg1_chirho != 0 {
                let rlim_chirho = Rlimit64Chirho {
                    rlim_cur_chirho: RLIM_INFINITY_CHIRHO,
                    rlim_max_chirho: RLIM_INFINITY_CHIRHO,
                };
                unsafe { core::ptr::write(arg1_chirho as *mut Rlimit64Chirho, rlim_chirho); }
            }
            0
        },

        // Extended attributes (l*/f* variants): not supported
        SYS_LSETXATTR_CHIRHO | SYS_FSETXATTR_CHIRHO => -ENOTSUP_CHIRHO,
        SYS_LGETXATTR_CHIRHO | SYS_FGETXATTR_CHIRHO => -ENOTSUP_CHIRHO,
        SYS_LLISTXATTR_CHIRHO | SYS_FLISTXATTR_CHIRHO => -ENOTSUP_CHIRHO,
        SYS_LREMOVEXATTR_CHIRHO | SYS_FREMOVEXATTR_CHIRHO => -ENOTSUP_CHIRHO,

        // I/O priority: stub (return default class)
        SYS_IOPRIO_SET_CHIRHO => 0,
        SYS_IOPRIO_GET_CHIRHO => 0, // IOPRIO_CLASS_NONE

        // inotify: return -ENOSYS (not implemented)
        SYS_INOTIFY_INIT1_CHIRHO => -ENOSYS_CHIRHO,
        SYS_INOTIFY_ADD_WATCH_CHIRHO => -ENOSYS_CHIRHO,
        SYS_INOTIFY_RM_WATCH_CHIRHO => -ENOSYS_CHIRHO,

        // fanotify: return -ENOSYS (not implemented)
        SYS_FANOTIFY_INIT_CHIRHO => -ENOSYS_CHIRHO,
        SYS_FANOTIFY_MARK_CHIRHO => -ENOSYS_CHIRHO,

        // Handle-based open: return -ENOSYS
        SYS_NAME_TO_HANDLE_AT_CHIRHO => -ENOSYS_CHIRHO,
        SYS_OPEN_BY_HANDLE_AT_CHIRHO => -ENOSYS_CHIRHO,

        // sync_file_range: advisory, silently succeed
        SYS_SYNC_FILE_RANGE_CHIRHO => 0,

        // utimensat: stub, silently succeed
        SYS_UTIMENSAT_CHIRHO => 0,

        SYS_LINKAT_CHIRHO => sys_linkat_chirho(
            arg0_chirho as i64,
            arg1_chirho,
            arg2_chirho as i64,
            arg3_chirho,
            arg4_chirho as u32,
        ),
        SYS_SYMLINKAT_CHIRHO => sys_symlinkat_chirho(
            arg0_chirho,
            arg1_chirho as i64,
            arg2_chirho,
        ),

        // --- Phase 9: Kernel module loading ---
        SYS_INIT_MODULE_CHIRHO => crate::module_chirho::sys_init_module_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),
        SYS_DELETE_MODULE_CHIRHO => crate::module_chirho::sys_delete_module_chirho(
            arg0_chirho, arg1_chirho,
        ),
        SYS_FINIT_MODULE_CHIRHO => crate::module_chirho::sys_finit_module_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho,
        ),

        // --- Phase 9: Tracing / perf ---
        SYS_PERF_EVENT_OPEN_CHIRHO => crate::trace_chirho::sys_perf_event_open_chirho(
            arg0_chirho, arg1_chirho, arg2_chirho, arg3_chirho, arg4_chirho,
        ),

        // Catch-all for unimplemented syscalls.
        unknown_chirho => {
            let name_chirho = syscall_name_chirho(unknown_chirho);
            crate::serial_debug_chirho!(
                "[SYSCALL] Unimplemented: {} ({}) args=({:#x},{:#x},{:#x})",
                name_chirho,
                unknown_chirho,
                arg0_chirho,
                arg1_chirho,
                arg2_chirho,
            );
            -ENOSYS_CHIRHO
        }
    };

    // Store the return value so the caller (assembly stub) can put it in rax.
    frame_chirho.rax_chirho = result_chirho as u64;

    let is_blocking_chirho = matches!(
        syscall_nr_chirho,
        SYS_SELECT_CHIRHO | SYS_POLL_CHIRHO | SYS_PPOLL_CHIRHO
        | SYS_EPOLL_WAIT_CHIRHO | SYS_EPOLL_PWAIT_CHIRHO
        | SYS_NANOSLEEP_CHIRHO | SYS_CLOCK_NANOSLEEP_CHIRHO
    );
    // Skip rescheduling after blocking and lifecycle syscalls.
    // Blocking syscalls (select/poll) consume the time slice in HLT loops.
    // schedule_chirho() at return boundary crashes (#UD) — the context
    // switch doesn't handle being called from the syscall dispatch path.
    // Instead, the fork child gets CPU time from schedule calls inside
    // the HLT loops (added back below).
    // Yield ONLY on wait4 with WNOHANG returning 0 (child not yet exited).
    // This gives fork children CPU time without disrupting the SSH handshake.
    // The SSH handshake (select/read/write on socket) runs without preemption
    // for maximum speed. Once dropbear forks PID 4 and calls wait4, it
    // yields to let PID 4 run the command.
    if syscall_nr_chirho == SYS_WAIT4_CHIRHO && result_chirho == 0 {
        crate::scheduler_chirho::schedule_chirho();
    }

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

            // Copy user buffer to kernel stack first, THEN write to serial.
            // This uses copy_from_user which validates the address range.
            // We process in 256-byte chunks to avoid huge stack allocations.
            let mut written_chirho: usize = 0;
            while written_chirho < count_chirho {
                let chunk_chirho = core::cmp::min(256, count_chirho - written_chirho);
                let mut kbuf_chirho = [0u8; 256];
                let src_addr_chirho = buf_ptr_chirho as u64 + written_chirho as u64;
                if crate::uaccess_chirho::copy_from_user_chirho(
                    &mut kbuf_chirho[..chunk_chirho], src_addr_chirho, chunk_chirho,
                ).is_err() {
                    return if written_chirho > 0 { written_chirho as i64 } else { -EFAULT_CHIRHO };
                }
                // Write the chunk to serial port AND framebuffer console
                for j_chirho in 0..chunk_chirho {
                    let byte_chirho = kbuf_chirho[j_chirho];
                    unsafe {
                        while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                        x86_64::instructions::port::Port::<u8>::new(0x3F8).write(byte_chirho);
                    }
                    // Mirror to framebuffer console
                    if let Some(mut fb_chirho) = crate::fbconsole_chirho::FB_CONSOLE_CHIRHO.try_lock() {
                        fb_chirho.write_byte_chirho(byte_chirho);
                    }
                }
                written_chirho += chunk_chirho;
            }

            written_chirho as i64
        }
        _ => -EBADF_CHIRHO,
    }
}

fn fd_uses_console_stdio_chirho(fd_chirho: u64) -> bool {
    if fd_chirho != 1 && fd_chirho != 2 {
        return false;
    }

    let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(fd_chirho) else {
        return true;
    };

    let file_guard_chirho = file_arc_chirho.lock();
    let ops_ptr_chirho =
        file_guard_chirho.ops_chirho as *const dyn crate::vfs_chirho::FileOpsChirho as *const u8;
    let console_ptr_chirho =
        &crate::devtmpfs_chirho::DEV_CONSOLE_OPS_CHIRHO
            as *const dyn crate::vfs_chirho::FileOpsChirho as *const u8;
    ops_ptr_chirho == console_ptr_chirho
}

fn sys_write_fd_dispatch_chirho(
    fd_chirho: u64,
    buf_addr_chirho: u64,
    count_chirho: usize,
) -> i64 {
    if crate::net_chirho::is_socket_fd_chirho(fd_chirho) {
        crate::net_chirho::sys_sendto_chirho(
            fd_chirho,
            buf_addr_chirho,
            count_chirho as u64,
            0,
            0,
            0,
        )
    } else {
        crate::fs_chirho::sys_write_real_chirho(fd_chirho, buf_addr_chirho, count_chirho)
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
    if iov_chirho.is_null() || iovcnt_chirho <= 0 {
        return -EINVAL_CHIRHO;
    }
    let is_sock_chirho = crate::net_chirho::is_socket_fd_chirho(fd_chirho);
    crate::serial_debug_chirho!(
        "[WRITEV] fd={} iovcnt={} is_socket={}",
        fd_chirho, iovcnt_chirho, is_sock_chirho,
    );

    let mut total_written_chirho: i64 = 0;

    for i_chirho in 0..iovcnt_chirho as usize {
        // Read the iovec entry from user memory safely via copy_from_user.
        let mut iov_buf_chirho = [0u8; 16]; // IoVecChirho is 16 bytes (ptr + len)
        let iov_addr_chirho = iov_chirho as u64 + (i_chirho * 16) as u64;
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut iov_buf_chirho, iov_addr_chirho, 16,
        ).is_err() {
            return if total_written_chirho > 0 { total_written_chirho } else { -EFAULT_CHIRHO };
        }
        // SAFETY: Slicing [0..8] and [8..16] from a [u8; 16] always yields
        // exactly 8 bytes, so try_into cannot fail.  We use unwrap_or(0) to
        // avoid a panic path in the generated code nonetheless.
        let iov_base_chirho = u64::from_ne_bytes(
            iov_buf_chirho[0..8].try_into().unwrap_or([0u8; 8])
        );
        let iov_len_chirho = u64::from_ne_bytes(
            iov_buf_chirho[8..16].try_into().unwrap_or([0u8; 8])
        ) as usize;
        if iov_base_chirho == 0 || iov_len_chirho == 0 {
            continue;
        }
        let result_chirho = if fd_uses_console_stdio_chirho(fd_chirho) {
            sys_write_chirho(
                fd_chirho,
                iov_base_chirho as *const u8,
                iov_len_chirho,
            )
        } else {
            sys_write_fd_dispatch_chirho(fd_chirho, iov_base_chirho, iov_len_chirho)
        };
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

/// `pread64(2)` — read at a given offset without changing file position.
///
/// Like `read(2)` but reads from position `offset_chirho` in the file
/// without modifying the file's current offset (pos_chirho). This is the
/// #1 missing syscall needed by musl libc and Alpine programs.
fn sys_pread64_chirho(
    fd_chirho: u64,
    buf_addr_chirho: u64,
    count_chirho: usize,
    offset_chirho: i64,
) -> i64 {
    if count_chirho == 0 {
        return 0;
    }
    if offset_chirho < 0 {
        return -EINVAL_CHIRHO;
    }

    // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Read at the specified offset without changing the file position.
    // Support reads larger than 4KB (sqlite3 needs multi-page reads).
    // Cap at 16MB to prevent OOM from untrusted userspace count (oom-004).
    let capped_count_chirho = core::cmp::min(count_chirho, 16 * 1024 * 1024);
    let mut kernel_buf_chirho = alloc::vec![0u8; capped_count_chirho];

    let bytes_read_chirho = {
        let mut file_guard_chirho = file_arc_chirho.lock();
        let saved_pos_chirho = file_guard_chirho.pos_chirho;
        file_guard_chirho.pos_chirho = offset_chirho as u64;
        let result_chirho = file_guard_chirho.ops_chirho.read_chirho(
            &mut file_guard_chirho,
            &mut kernel_buf_chirho,
        );
        // Restore position regardless of result
        file_guard_chirho.pos_chirho = saved_pos_chirho;
        match result_chirho {
            Ok(n_chirho) => n_chirho,
            Err(errno_chirho) => return errno_chirho,
        }
    };

    // Copy to user space
    if bytes_read_chirho > 0 {
        if crate::uaccess_chirho::copy_to_user_chirho(
            buf_addr_chirho,
            &kernel_buf_chirho[..bytes_read_chirho],
            bytes_read_chirho,
        ).is_err() {
            return -EFAULT_CHIRHO;
        }
    }

    bytes_read_chirho as i64
}

/// `pwrite64(2)` — write at a given offset without changing file position.
///
/// Like `write(2)` but writes at position `offset_chirho` in the file
/// without modifying the file's current offset (pos_chirho).
fn sys_pwrite64_chirho(
    fd_chirho: u64,
    buf_addr_chirho: u64,
    count_chirho: usize,
    offset_chirho: i64,
) -> i64 {
    if count_chirho == 0 {
        return 0;
    }
    if offset_chirho < 0 {
        return -EINVAL_CHIRHO;
    }

    // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Copy from user space into kernel buffer.
    // Support writes larger than 4KB (sqlite3 needs multi-page writes).
    // Cap at 16MB to prevent OOM from untrusted userspace count (oom-004).
    let capped_count_chirho = core::cmp::min(count_chirho, 16 * 1024 * 1024);
    let mut kernel_buf_chirho = alloc::vec![0u8; capped_count_chirho];
    if crate::uaccess_chirho::copy_from_user_chirho(
        &mut kernel_buf_chirho,
        buf_addr_chirho,
        capped_count_chirho,
    ).is_err() {
        return -EFAULT_CHIRHO;
    }

    // Write at the specified offset without changing the file position.
    let bytes_written_chirho = {
        let mut file_guard_chirho = file_arc_chirho.lock();
        let saved_pos_chirho = file_guard_chirho.pos_chirho;
        file_guard_chirho.pos_chirho = offset_chirho as u64;
        let result_chirho = file_guard_chirho.ops_chirho.write_chirho(
            &mut file_guard_chirho,
            &kernel_buf_chirho,
        );
        // Restore position regardless of result
        file_guard_chirho.pos_chirho = saved_pos_chirho;
        match result_chirho {
            Ok(n_chirho) => n_chirho,
            Err(errno_chirho) => return errno_chirho,
        }
    };

    bytes_written_chirho as i64
}

/// `readv(2)` — scatter read into multiple buffers.
///
/// Reads data from file descriptor `fd_chirho` into the iovec array.
/// Handles all fds through the VFS (stdin via serial poll for fd 0).
fn sys_readv_chirho(
    fd_chirho: u64,
    iov_chirho: *const IoVecChirho,
    iovcnt_chirho: i32,
) -> i64 {
    if iov_chirho.is_null() || iovcnt_chirho <= 0 {
        return -EINVAL_CHIRHO;
    }

    let mut total_read_chirho: i64 = 0;

    for i_chirho in 0..iovcnt_chirho as usize {
        // Read the iovec entry from user memory
        let mut iov_buf_chirho = [0u8; 16]; // IoVecChirho is 16 bytes (ptr + len)
        let iov_addr_chirho = iov_chirho as u64 + (i_chirho * 16) as u64;
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut iov_buf_chirho, iov_addr_chirho, 16,
        ).is_err() {
            return if total_read_chirho > 0 { total_read_chirho } else { -EFAULT_CHIRHO };
        }
        // SAFETY: Slicing [0..8] and [8..16] from a [u8; 16] always yields
        // exactly 8 bytes, so try_into cannot fail.  We use unwrap_or(0) to
        // avoid a panic path in the generated code nonetheless.
        let iov_base_chirho = u64::from_ne_bytes(
            iov_buf_chirho[0..8].try_into().unwrap_or([0u8; 8])
        );
        let iov_len_chirho = u64::from_ne_bytes(
            iov_buf_chirho[8..16].try_into().unwrap_or([0u8; 8])
        ) as usize;
        if iov_base_chirho == 0 || iov_len_chirho == 0 {
            continue;
        }

        // Route read through VFS (fd 0 stdin handled by dispatch)
        let result_chirho = if fd_chirho == 0 {
            sys_read_stdin_chirho(iov_base_chirho, iov_len_chirho)
        } else {
            crate::fs_chirho::sys_read_real_chirho(fd_chirho, iov_base_chirho, iov_len_chirho)
        };
        if result_chirho < 0 {
            if total_read_chirho > 0 {
                return total_read_chirho;
            }
            return result_chirho;
        }
        total_read_chirho += result_chirho;
        // Short read: don't continue to next iovec
        if (result_chirho as usize) < iov_len_chirho {
            break;
        }
    }

    total_read_chirho
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
/// Marks the current task as a zombie (so `wait4` can reap it), removes it
/// from the scheduler run queue, and context-switches to the next task.
/// If there is no current task (should not happen), falls back to halt.
fn sys_exit_chirho(code_chirho: i32) -> i64 {
    crate::serial_debug_chirho!(
        "[SYSCALL] exit({}) -- process terminating",
        code_chirho
    );

    // Mark the current task as zombie with the given exit code.
    let pid_chirho = {
        let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
            Some(t_chirho) => t_chirho,
            None => {
                // No current task — fallback to halt (should not happen).
                loop { x86_64::instructions::hlt(); }
            }
        };
        let mut task_chirho = task_arc_chirho.lock();
        task_chirho.exit_chirho(code_chirho);
        let pid_chirho = task_chirho.pid_chirho;
        crate::serial_debug_chirho!(
            "[SYSCALL] exit: PID={} -> zombie (exit_code={})",
            pid_chirho,
            code_chirho
        );
        pid_chirho
    };

    // Remove from scheduler run queue.
    crate::scheduler_chirho::remove_task_chirho(pid_chirho);

    // Deliver SIGCHLD to the parent process (A2-PROC-005).
    // Use the centralized signal delivery helper which also enqueues
    // SignalInfoChirho and wakes blocked parents.
    {
        let ppid_chirho = crate::task_chirho::find_task_by_pid_chirho(pid_chirho)
            .map(|t_chirho| t_chirho.lock().ppid_chirho)
            .unwrap_or(0);
        crate::signal_chirho::deliver_sigchld_chirho(ppid_chirho, pid_chirho);
    }

    // Wake any parent sleeping in wait4 on the child-exit wait queue.
    // This unblocks the parent so it can reap this zombie child immediately
    // instead of polling.  (A2-PROC-001: WaitQueueChirho replaces poll loop.)
    crate::process_chirho::wake_child_exit_waitqueue_chirho();

    // Yield to parent (real fork) or re-launch shell (fallback).
    crate::serial_debug_chirho!("[SYSCALL] exit: PID={} zombie, yielding", pid_chirho);
    crate::scheduler_chirho::yield_current_chirho();
    crate::serial_println_chirho!("[SYSCALL] exit: no parent, re-launching shell");

    // Re-load BusyBox as ash shell
    let shell_argv_chirho = [
        alloc::string::String::from("sh"),
    ];
    let shell_envp_chirho = [
        alloc::string::String::from("HOME=/root"),
        alloc::string::String::from("PATH=/bin:/sbin:/usr/bin:/usr/sbin"),
        alloc::string::String::from("TERM=linux"),
        alloc::string::String::from("PS1=lineluya# "),
        alloc::string::String::from("LD_LIBRARY_PATH=/lib:/usr/lib"),
        alloc::string::String::from("SHELL=/bin/sh"),
        alloc::string::String::from("PYTHONDONTWRITEBYTECODE=1"),
        alloc::string::String::from("PYTHONHOME=/usr"),
        alloc::string::String::from("PYTHONPATH=/usr/lib/python3.12"),
    ];
    let loaded_chirho = match crate::exec_chirho::load_elf_into_memory_chirho(
        crate::exec_chirho::BUSYBOX_ELF_CHIRHO
    ) {
        Ok(l_chirho) => l_chirho,
        Err(_e_chirho) => {
            crate::serial_println_chirho!(
                "[SYSCALL] exit: failed to reload shell ELF — halting"
            );
            loop { x86_64::instructions::hlt(); }
        }
    };

    crate::syscall_chirho::set_brk_chirho(loaded_chirho.brk_addr_chirho);

    let user_rsp_chirho = crate::exec_chirho::setup_user_stack_with_args_chirho(
        &loaded_chirho,
        &shell_argv_chirho,
        &shell_envp_chirho,
    );

    crate::exec_chirho::jump_to_userspace_chirho(
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho,
    );
}

/// `exit_group(2)` implementation (A2-PROC-006).
///
/// Terminates **all** threads in the current thread group (same `tgid_chirho`).
/// For each thread: sets it to `ZombieChirho`, records the exit code, and
/// removes it from the scheduler run queue.  SIGCHLD is delivered to the
/// parent of each terminated thread.  The current workaround then re-execs
/// the shell in the exiting task's context instead of returning to `wait4`.
fn sys_exit_group_chirho(code_chirho: i32) -> i64 {
    crate::serial_debug_chirho!(
        "[SYSCALL] exit_group({}) -- terminating thread group (A2-PROC-006)",
        code_chirho
    );

    // Determine the thread-group ID of the caller.
    let (caller_tgid_chirho, caller_pid_chirho) = {
        let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
            Some(t_chirho) => t_chirho,
            None => {
                // No current task — should never happen; halt.
                loop { x86_64::instructions::hlt(); }
            }
        };
        let task_chirho = task_arc_chirho.lock();
        (task_chirho.tgid_chirho, task_chirho.pid_chirho)
    };

    crate::serial_debug_chirho!(
        "[SYSCALL] exit_group: caller PID={} tgid={}",
        caller_pid_chirho,
        caller_tgid_chirho
    );

    // Collect PIDs and parent PIDs of all threads in the same thread group.
    let threads_chirho: alloc::vec::Vec<(u64, u64)> = {
        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        list_chirho
            .iter()
            .filter_map(|t_arc_chirho| {
                let t_chirho = t_arc_chirho.lock();
                if t_chirho.tgid_chirho == caller_tgid_chirho
                    && !t_chirho.is_exited_chirho()
                {
                    Some((t_chirho.pid_chirho, t_chirho.ppid_chirho))
                } else {
                    None
                }
            })
            .collect()
    };

    crate::serial_debug_chirho!(
        "[SYSCALL] exit_group: found {} thread(s) in tgid={}",
        threads_chirho.len(),
        caller_tgid_chirho
    );

    // Terminate each thread: set ZombieChirho + exit code, remove from
    // scheduler, deliver SIGCHLD to parent.
    for &(pid_chirho, ppid_chirho) in &threads_chirho {
        // Mark zombie with exit code.
        if let Some(t_arc_chirho) = crate::task_chirho::find_task_by_pid_chirho(pid_chirho) {
            let mut t_chirho = t_arc_chirho.lock();
            t_chirho.exit_chirho(code_chirho);
            crate::serial_debug_chirho!(
                "[SYSCALL] exit_group: PID={} -> zombie (exit_code={})",
                pid_chirho,
                code_chirho
            );
        }

        // Remove from scheduler run queue.
        crate::scheduler_chirho::remove_task_chirho(pid_chirho);

        // Deliver SIGCHLD to the parent (A2-PROC-005).
        crate::signal_chirho::deliver_sigchld_chirho(ppid_chirho, pid_chirho);
    }

    // Shell re-exec workaround: kill the parent shell and re-exec a fresh
    // shell in the exiting task's context. Context switch back to parent
    // still has issues (3rd generation fork child hangs in userspace).
    let parent_pid_chirho = threads_chirho.first().map(|&(_, pp)| pp).unwrap_or(0);
    crate::scheduler_chirho::remove_task_chirho(parent_pid_chirho);
    if let Some(t_chirho) = crate::task_chirho::find_task_by_pid_chirho(parent_pid_chirho) {
        t_chirho.lock().exit_chirho(0);
    }
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let mut task_chirho = task_arc_chirho.lock();
        let pid_chirho = task_chirho.pid_chirho;
        task_chirho.sid_chirho = pid_chirho;
        task_chirho.pgid_chirho = pid_chirho;
        task_chirho.ppid_chirho = 0;
        task_chirho.state_chirho = crate::task_chirho::TaskStateChirho::RunningChirho;
        task_chirho.exit_code_chirho = 0;
        // CRITICAL: Clear per-process page table so the re-exec'd shell
        // uses the boot PML4. exec_init maps BusyBox into the boot PML4,
        // not the per-process PT. If we keep the old PT, the next fork
        // clones stale mappings instead of getting fresh ones from boot.
        task_chirho.page_table_root_chirho = None;
    }
    // Switch back to boot PML4 before exec_init
    let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
    if boot_pml4_chirho.as_u64() != 0 {
        unsafe { crate::pagetable_chirho::switch_page_table_chirho(boot_pml4_chirho); }
    }
    crate::exec_chirho::exec_init_chirho();

    // Unreachable — exec_init jumps to userspace. Fallback below is
    // for the case where exec_init returns (shouldn't happen).
    let shell_argv_chirho = [
        alloc::string::String::from("sh"),
    ];
    let shell_envp_chirho = [
        alloc::string::String::from("HOME=/root"),
        alloc::string::String::from("PATH=/bin:/sbin:/usr/bin:/usr/sbin"),
        alloc::string::String::from("TERM=linux"),
        alloc::string::String::from("PS1=lineluya# "),
        alloc::string::String::from("LD_LIBRARY_PATH=/lib:/usr/lib"),
        alloc::string::String::from("SHELL=/bin/sh"),
        alloc::string::String::from("PYTHONDONTWRITEBYTECODE=1"),
        alloc::string::String::from("PYTHONHOME=/usr"),
        alloc::string::String::from("PYTHONPATH=/usr/lib/python3.12"),
    ];
    let loaded_chirho = match crate::exec_chirho::load_elf_into_memory_chirho(
        crate::exec_chirho::BUSYBOX_ELF_CHIRHO
    ) {
        Ok(l_chirho) => l_chirho,
        Err(_e_chirho) => {
            crate::serial_println_chirho!(
                "[SYSCALL] exit_group: failed to reload shell ELF — halting"
            );
            loop { x86_64::instructions::hlt(); }
        }
    };

    crate::syscall_chirho::set_brk_chirho(loaded_chirho.brk_addr_chirho);

    let user_rsp_chirho = crate::exec_chirho::setup_user_stack_with_args_chirho(
        &loaded_chirho,
        &shell_argv_chirho,
        &shell_envp_chirho,
    );

    crate::exec_chirho::jump_to_userspace_chirho(
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho,
    );
}

/// `brk(2)` implementation — per-process program break (A2-AUDIT-003).
///
/// If `addr_chirho` is 0, returns the current break.  Otherwise, attempts to
/// set the break to `addr_chirho`.  When expanding, new pages are mapped via
/// the memory manager.
///
/// Uses the current task's `brk_chirho` field for per-process isolation.
/// Falls back to the global `CURRENT_BRK_CHIRHO` for PID 0 (init/idle tasks
/// that run before any user process exists).
fn sys_brk_chirho(addr_chirho: u64) -> i64 {
    // Try to read the per-process brk from the current task
    let (old_brk_chirho, is_per_process_chirho) =
        if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
            let task_chirho = task_arc_chirho.lock();
            let pid_chirho = task_chirho.pid_chirho;
            let brk_val_chirho = task_chirho.brk_chirho;
            drop(task_chirho);
            if pid_chirho == 0 || brk_val_chirho == 0 {
                // PID 0 (idle) or uninitialised brk — use global fallback
                (CURRENT_BRK_CHIRHO.load(Ordering::SeqCst), false)
            } else {
                (brk_val_chirho, true)
            }
        } else {
            // No current task — very early boot, use global
            (CURRENT_BRK_CHIRHO.load(Ordering::SeqCst), false)
        };

    if addr_chirho == 0 {
        return old_brk_chirho as i64;
    }

    // If expanding, map new pages
    if addr_chirho > old_brk_chirho {
        let old_page_chirho = (old_brk_chirho + 0xFFF) & !0xFFF; // round up
        let new_page_chirho = (addr_chirho + 0xFFF) & !0xFFF;
        if new_page_chirho > old_page_chirho {
            let size_chirho = new_page_chirho - old_page_chirho;
            let mm_lock_chirho = crate::mm_chirho::get_or_init_mm_chirho();
            let mut guard_chirho = mm_lock_chirho.lock();
            if let Some(mm_chirho) = guard_chirho.as_mut() {
                let result_chirho = mm_chirho.mmap_chirho(
                    old_page_chirho,
                    size_chirho,
                    crate::mm_chirho::PROT_READ_CHIRHO | crate::mm_chirho::PROT_WRITE_CHIRHO,
                    crate::mm_chirho::MAP_PRIVATE_CHIRHO
                        | crate::mm_chirho::MAP_ANONYMOUS_CHIRHO
                        | crate::mm_chirho::MAP_FIXED_CHIRHO,
                    -1i32,
                    0,
                );
                if result_chirho.is_err() {
                    return old_brk_chirho as i64; // return old brk on failure
                }
            }
        }
    }

    // Update per-process brk on the current task
    if is_per_process_chirho {
        if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
            task_arc_chirho.lock().brk_chirho = addr_chirho;
        }
    }

    // Always keep the global in sync as a fallback
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
    /// IA32_KERNEL_GS_BASE MSR address.
    const IA32_KERNEL_GS_BASE_CHIRHO: u32 = 0xC000_0102;

    match code_chirho {
        ARCH_SET_FS_CHIRHO => {
            // Write FS base for TLS.
            let mut msr_chirho = Msr::new(IA32_FS_BASE_CHIRHO);
            unsafe {
                msr_chirho.write(addr_chirho);
            }
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                task_arc_chirho.lock().fs_base_chirho = addr_chirho;
            }
            crate::serial_debug_chirho!(
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
            // While in kernel mode after SWAPGS, the user GS value lives in
            // IA32_KERNEL_GS_BASE and will be restored on the next SWAPGS back
            // to userspace.
            let mut msr_chirho = Msr::new(IA32_KERNEL_GS_BASE_CHIRHO);
            unsafe {
                msr_chirho.write(addr_chirho);
            }
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                task_arc_chirho.lock().gs_base_chirho = addr_chirho;
            }
            crate::serial_debug_chirho!(
                "[SYSCALL] arch_prctl(ARCH_SET_GS, {:#x})",
                addr_chirho
            );
            0
        }
        ARCH_GET_GS_CHIRHO => {
            if addr_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let msr_chirho = Msr::new(IA32_KERNEL_GS_BASE_CHIRHO);
            let gs_base_chirho = unsafe { msr_chirho.read() };
            unsafe {
                *(addr_chirho as *mut u64) = gs_base_chirho;
            }
            0
        }
        _ => {
            crate::serial_debug_chirho!(
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
    match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho.lock().pid_chirho as i64,
        None => 1, // fallback for early boot
    }
}

/// `mmap(2)` implementation.
///
/// Delegates to [`crate::mm_chirho::MmChirho::mmap_chirho`] for anonymous
/// `MAP_PRIVATE` mappings.  Lazily initialises the global memory descriptor
/// on first call.
fn sys_mmap_chirho(
    addr_chirho: u64,
    length_chirho: u64,
    prot_chirho: u32,
    flags_chirho: u32,
    fd_chirho: i32,
    offset_chirho: u64,
) -> i64 {
    let mm_lock_chirho = crate::mm_chirho::get_or_init_mm_chirho();
    let mut guard_chirho = mm_lock_chirho.lock();
    match guard_chirho.as_mut() {
        Some(mm_chirho) => match mm_chirho.mmap_chirho(
            addr_chirho,
            length_chirho,
            prot_chirho,
            flags_chirho,
            fd_chirho,
            offset_chirho,
        ) {
            Ok(mapped_addr_chirho) => mapped_addr_chirho as i64,
            Err(errno_chirho) => errno_chirho,
        },
        None => -ENOMEM_CHIRHO,
    }
}

/// `mprotect(2)` implementation.
///
/// Delegates to [`crate::mm_chirho::MmChirho::mprotect_chirho`].
fn sys_mprotect_chirho(
    addr_chirho: u64,
    len_chirho: u64,
    prot_chirho: u32,
) -> i64 {
    let mm_lock_chirho = crate::mm_chirho::get_or_init_mm_chirho();
    let mut guard_chirho = mm_lock_chirho.lock();
    match guard_chirho.as_mut() {
        Some(mm_chirho) => match mm_chirho.mprotect_chirho(addr_chirho, len_chirho, prot_chirho) {
            Ok(()) => 0,
            Err(errno_chirho) => errno_chirho,
        },
        None => -ENOMEM_CHIRHO,
    }
}

/// `munmap(2)` implementation.
///
/// Delegates to [`crate::mm_chirho::MmChirho::munmap_chirho`].
fn sys_munmap_chirho(
    addr_chirho: u64,
    len_chirho: u64,
) -> i64 {
    let mm_lock_chirho = crate::mm_chirho::get_or_init_mm_chirho();
    let mut guard_chirho = mm_lock_chirho.lock();
    match guard_chirho.as_mut() {
        Some(mm_chirho) => match mm_chirho.munmap_chirho(addr_chirho, len_chirho) {
            Ok(()) => 0,
            Err(errno_chirho) => errno_chirho,
        },
        None => -ENOMEM_CHIRHO,
    }
}

/// `set_tid_address(2)` implementation.
///
/// The kernel stores `tidptr_chirho` so it can perform a futex wake when the
/// thread exits.  Returns the caller's TID (= PID for single-threaded).
/// CRITICAL: must return a unique value per process — musl uses the TID
/// to own internal locks. If parent and child share the same TID after fork,
/// musl detects a false recursive lock and calls a_crash() (HLT → GPF).
fn sys_set_tid_address_chirho(_tidptr_chirho: *mut i32) -> i64 {
    // Return a unique TID per process. TID must be >= 1 because musl uses
    // CAS(lock, 0, tid) for locking — tid=0 makes a_cas a no-op.
    // The tid_ptr write is deferred — musl handles it in __init_tp.
    let pid_chirho = sys_getpid_chirho();
    if pid_chirho < 1 { 1 } else { pid_chirho }
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
    crate::serial_debug_chirho!(
        "[SYSCALL] ioctl(fd={}, request={:#x}) -> ENOTTY (stub)",
        fd_chirho,
        request_chirho,
    );
    -ENOTTY_CHIRHO
}

const F_SETFL_MUTABLE_FLAGS_CHIRHO: u32 =
    crate::vfs_chirho::O_APPEND_CHIRHO | crate::vfs_chirho::O_NONBLOCK_CHIRHO;

fn update_file_status_flags_chirho(
    fd_chirho: u64,
    requested_flags_chirho: u32,
) -> i64 {
    if let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        let mut file_chirho = file_arc_chirho.lock();
        file_chirho.flags_chirho =
            (file_chirho.flags_chirho & !F_SETFL_MUTABLE_FLAGS_CHIRHO)
            | (requested_flags_chirho & F_SETFL_MUTABLE_FLAGS_CHIRHO);
        return 0;
    }

    -EBADF_CHIRHO
}

/// `ioctl(2)` real implementation (P3-016).
///
/// Dispatches to VFS FileOps::ioctl where possible. Handles common terminal
/// and file ioctls directly for fds that have no VFS backing.
fn sys_ioctl_real_chirho(
    fd_chirho: u64,
    cmd_chirho: u64,
    arg_chirho: u64,
) -> i64 {
    // NOTE: TIOCGPGRP / TIOCSPGRP are no longer intercepted here before VFS
    // dispatch.  PTY fds have their own handlers in pty_ioctl_chirho which
    // store/retrieve the real foreground pgrp.  The fallback for non-PTY fds
    // (stdin/stdout/stderr, BusyBox dup'd fds) is handled after VFS dispatch
    // below.

    // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    {
        if let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
            let file_guard_chirho = file_arc_chirho.lock();
            let result_chirho = file_guard_chirho.ops_chirho.ioctl_chirho(
                &file_guard_chirho,
                cmd_chirho,
                arg_chirho,
            );
            match result_chirho {
                Ok(val_chirho) => return val_chirho,
                Err(e_chirho) if e_chirho != ENOSYS_CHIRHO && e_chirho != ENOTTY_CHIRHO => {
                    // Device ioctls return Err(-errno) or Err(errno).
                    // Normalize: if already negative, return as-is; if positive, negate.
                    return if e_chirho < 0 { e_chirho } else { -e_chirho };
                }
                _ => {} // Fall through to common handler
            }
        }
    }

    // Fallback: handle common terminal ioctls for any fd
    // (BusyBox dup's the TTY to fd 4+ and calls ioctl on it)
    match cmd_chirho {
        TCGETS_CHIRHO => {
            // For stdin/stdout/stderr (fds 0-2), return a basic termios
            // so isatty() returns true. Programs use this to decide whether
            // to use line-buffered output, readline, etc.
            if fd_chirho <= 2 && arg_chirho != 0 {
                // Return a minimal cooked-mode termios
                unsafe {
                    core::ptr::write_bytes(arg_chirho as *mut u8, 0, 60);
                    // c_iflag: ICRNL | IXON
                    core::ptr::write((arg_chirho) as *mut u32, 0x0500);
                    // c_oflag: OPOST | ONLCR
                    core::ptr::write((arg_chirho + 4) as *mut u32, 0x0005);
                    // c_cflag: B38400 | CS8 | CREAD | HUPCL
                    core::ptr::write((arg_chirho + 8) as *mut u32, 0x00BF);
                    // c_lflag: ECHO | ECHOE | ECHOK | ISIG | ICANON | IEXTEN | ECHOCTL | ECHOKE
                    core::ptr::write((arg_chirho + 12) as *mut u32, 0x8A3B);
                }
                return 0;
            }
            -ENOTTY_CHIRHO
        }
        TCSETS_CHIRHO | TCSETSW_CHIRHO | TCSETSF_CHIRHO => {
            // Accept termios changes for fds 0-2
            if fd_chirho <= 2 { return 0; }
            -ENOTTY_CHIRHO
        }
        TIOCNOTTY_CHIRHO => 0,    // give up controlling TTY: succeed
        TIOCSCTTY_CHIRHO => 0,    // become controlling TTY: succeed
        TIOCGWINSZ_CHIRHO => {
            // Return a default 80x24 window size
            if arg_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let winsize_chirho = WinsizeChirho {
                ws_row_chirho: 24,
                ws_col_chirho: 80,
                ws_xpixel_chirho: 0,
                ws_ypixel_chirho: 0,
            };
            let src_bytes_chirho = unsafe {
                core::slice::from_raw_parts(
                    &winsize_chirho as *const WinsizeChirho as *const u8,
                    core::mem::size_of::<WinsizeChirho>(),
                )
            };
            match crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                src_bytes_chirho,
                core::mem::size_of::<WinsizeChirho>(),
            ) {
                Ok(()) => 0,
                Err(_) => -EFAULT_CHIRHO,
            }
        }
        FIONREAD_CHIRHO => {
            // Report 0 bytes available
            if arg_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let zero_val_chirho: i32 = 0;
            let src_bytes_chirho = unsafe {
                core::slice::from_raw_parts(
                    &zero_val_chirho as *const i32 as *const u8,
                    core::mem::size_of::<i32>(),
                )
            };
            match crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                src_bytes_chirho,
                core::mem::size_of::<i32>(),
            ) {
                Ok(()) => 0,
                Err(_) => -EFAULT_CHIRHO,
            }
        }
        FIOCLEX_CHIRHO => {
            crate::fs_chirho::set_fd_cloexec_chirho(fd_chirho, true)
        }
        FIONBIO_CHIRHO => {
            if arg_chirho == 0 {
                return -EFAULT_CHIRHO;
            }

            let mut nonblock_bytes_chirho = [0u8; core::mem::size_of::<i32>()];
            let nonblock_len_chirho = nonblock_bytes_chirho.len();
            if crate::uaccess_chirho::copy_from_user_chirho(
                &mut nonblock_bytes_chirho,
                arg_chirho,
                nonblock_len_chirho,
            ).is_err() {
                return -EFAULT_CHIRHO;
            }

            let enable_nonblock_chirho =
                i32::from_ne_bytes(nonblock_bytes_chirho) != 0;
            let requested_flags_chirho = if enable_nonblock_chirho {
                crate::vfs_chirho::O_NONBLOCK_CHIRHO
            } else {
                0
            };

            update_file_status_flags_chirho(fd_chirho, requested_flags_chirho)
        }
        TIOCGPGRP_CHIRHO => {
            // Return the foreground process group ID (= our PID, since we
            // are the only process group for now).
            if arg_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            let pgrp_chirho: i32 = sys_getpid_chirho() as i32;
            let src_bytes_chirho = unsafe {
                core::slice::from_raw_parts(
                    &pgrp_chirho as *const i32 as *const u8,
                    core::mem::size_of::<i32>(),
                )
            };
            match crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                src_bytes_chirho,
                core::mem::size_of::<i32>(),
            ) {
                Ok(()) => 0,
                Err(_) => -EFAULT_CHIRHO,
            }
        }
        TIOCSPGRP_CHIRHO => {
            // Set the foreground process group ID — accept silently.
            // BusyBox ash calls this to set its own pgrp.
            0
        }
        _ => {
            crate::serial_debug_chirho!(
                "[SYSCALL] ioctl(fd={}, cmd={:#x}) -> ENOTTY (unrecognised)",
                fd_chirho,
                cmd_chirho,
            );
            -ENOTTY_CHIRHO
        }
    }
}

// ============================================================================
// poll / select / epoll implementations (P3-031)
// ============================================================================

/// `poll(2)` simplified implementation.
///
/// Marks all valid fds as ready immediately (non-blocking stub).
fn sys_poll_chirho(
    fds_ptr_chirho: u64,
    nfds_chirho: u32,
    _timeout_chirho: i32,
) -> i64 {
    if fds_ptr_chirho == 0 || nfds_chirho == 0 {
        return 0;
    }

    let entry_size_chirho = core::mem::size_of::<PollfdChirho>();
    let total_size_chirho = entry_size_chirho * nfds_chirho as usize;

    // Read pollfd array from user space into an aligned buffer
    #[repr(C, align(8))]
    struct AlignedBufChirho([u8; 2048]);
    let mut buf_chirho = AlignedBufChirho([0u8; 2048]);
    if total_size_chirho > buf_chirho.0.len() {
        return -EINVAL_CHIRHO;
    }
    if crate::uaccess_chirho::copy_from_user_chirho(
        &mut buf_chirho.0[..total_size_chirho],
        fds_ptr_chirho,
        total_size_chirho,
    ).is_err() {
        return -EFAULT_CHIRHO;
    }

    let mut ready_count_chirho: i64 = 0;
    let pollfds_chirho = unsafe {
        core::slice::from_raw_parts_mut(
            buf_chirho.0.as_mut_ptr() as *mut PollfdChirho,
            nfds_chirho as usize,
        )
    };

    // Poll network for incoming packets before checking fds.
    crate::net_chirho::poll_network_chirho();

    for pfd_chirho in pollfds_chirho.iter_mut() {
        if pfd_chirho.fd_chirho < 0 {
            pfd_chirho.revents_chirho = 0;
            continue;
        }

        let fd_val_chirho = pfd_chirho.fd_chirho as u64;
        let mut revents_chirho: i16 = 0;

        if crate::net_chirho::is_socket_fd_chirho(fd_val_chirho) {
            // Socket fd: only report POLLIN if data/connection pending.
            if pfd_chirho.events_chirho & POLLIN_CHIRHO != 0
                && crate::net_chirho::socket_has_data_chirho(fd_val_chirho)
            {
                revents_chirho |= POLLIN_CHIRHO;
            }
            // POLLOUT: Don't report unconditionally — it causes dropbear
            // to spin in its event loop (22K+ syscalls/sec) instead of
            // processing received crypto data. Only set POLLOUT when the
            // caller ONLY asked for POLLOUT (not POLLIN|POLLOUT together).
            // When both are requested, let POLLIN drive the wake.
            if pfd_chirho.events_chirho == POLLOUT_CHIRHO {
                revents_chirho |= POLLOUT_CHIRHO;
            }
        } else {
            // Regular file/pipe
            if pfd_chirho.events_chirho & POLLOUT_CHIRHO != 0 {
                revents_chirho |= POLLOUT_CHIRHO;
            }
            if pfd_chirho.events_chirho & POLLIN_CHIRHO != 0 {
                if fd_val_chirho == 0 {
                    // stdin: for non-shell PIDs (daemons like dropbear),
                    // only report POLLIN if serial actually has data.
                    // The shell (PID 0/re-exec'd) needs unconditional
                    // POLLIN so its blocking read loop works.
                    
                    if is_interactive_shell_chirho() {
                        revents_chirho |= POLLIN_CHIRHO; // shell: always
                    } else {
                        // Daemon (dropbear): check serial AND TCP port 2222.
                        // Dropbear reads SSH data from fd=0 (pipe). TCP data
                        // on port 2222 is relayed to the pipe during read().
                        // Report POLLIN if either serial or TCP has data.
                        let lsr_chirho: u8 = unsafe {
                            x86_64::instructions::port::Port::<u8>::new(0x3FD).read()
                        };
                        if lsr_chirho & 1 != 0 {
                            revents_chirho |= POLLIN_CHIRHO;
                        }
                        // Also check TCP port 2222 for SSH data
                        if crate::net_chirho::has_tcp_data_for_port_chirho(2222) {
                            revents_chirho |= POLLIN_CHIRHO;
                        }
                    }
                } else {
                    revents_chirho |= POLLIN_CHIRHO;
                }
            }
        }

        pfd_chirho.revents_chirho = revents_chirho;
        if revents_chirho != 0 {
            ready_count_chirho += 1;
        }
    }

    // If nothing is ready, block until something arrives.
    if ready_count_chirho == 0 {
        for _attempt_chirho in 0..1000u32 {
            x86_64::instructions::interrupts::enable_and_hlt();
            crate::net_chirho::poll_network_chirho();
            // Yield to other runnable tasks (fork children).
            // Yield to fork children during blocking wait.
            if crate::scheduler_chirho::has_runnable_tasks_chirho() {
                crate::scheduler_chirho::schedule_chirho();
                crate::scheduler_chirho::reset_time_slice_chirho();
            }
            // Re-check pollfds
            for pfd_chirho in pollfds_chirho.iter() {
                if pfd_chirho.fd_chirho >= 0 {
                    let fd_val_chirho = pfd_chirho.fd_chirho as u64;
                    if crate::net_chirho::is_socket_fd_chirho(fd_val_chirho)
                        && crate::net_chirho::socket_has_data_chirho(fd_val_chirho)
                    {
                        ready_count_chirho = 1;
                        break;
                    }
                }
            }
            if ready_count_chirho > 0 { break; }
        }
    }

    // Write pollfd array back to user space
    if crate::uaccess_chirho::copy_to_user_chirho(
        fds_ptr_chirho,
        &buf_chirho.0[..total_size_chirho],
        total_size_chirho,
    ).is_err() {
        return -EFAULT_CHIRHO;
    }

    // One-shot debug: log poll result for PID >= 3
    {
        let pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(0);
        if pid_chirho >= 3 {
            static POLL_LOG_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
            let cnt_chirho = POLL_LOG_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            if cnt_chirho < 5 {
                let mut fds_str_chirho = alloc::string::String::new();
                for pfd_chirho in pollfds_chirho.iter() {
                    use core::fmt::Write;
                    let _ = write!(fds_str_chirho, " fd={}(ev={:#x},rev={:#x})",
                        pfd_chirho.fd_chirho, pfd_chirho.events_chirho, pfd_chirho.revents_chirho);
                }
                crate::serial_println_chirho!(
                    "[POLL-DBG] pid={} nfds={} timeout={} ready={} fds:{}",
                    pid_chirho, nfds_chirho, _timeout_chirho, ready_count_chirho, fds_str_chirho
                );
            }
        }
    }

    ready_count_chirho
}

/// `select(2)` implementation.
///
/// Checks which file descriptors are ready for I/O. For socket fds
/// with pending connections/data, reports POLLIN. For regular files
/// and pipes, always reports ready. For listening sockets with no
/// pending connections, blocks (yields CPU) until timeout.
fn sys_select_chirho(
    nfds_chirho: i32,
    readfds_ptr_chirho: u64,
    _writefds_chirho: u64,
    _exceptfds_chirho: u64,
    timeout_ptr_chirho: u64,
) -> i64 {
    if nfds_chirho < 0 {
        return -EINVAL_CHIRHO;
    }

    // Check if any socket has pending data by polling the network.
    crate::net_chirho::poll_network_chirho();

    // One-shot debug for select from dropbear child
    {
        static SELECT_DBG_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        if !is_interactive_shell_chirho() {
            let cnt_chirho = SELECT_DBG_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            if cnt_chirho < 3 {
                crate::serial_println_chirho!(
                    "[SELECT-DBG] nfds={} readfds_ptr={:#x} shell=false",
                    nfds_chirho, readfds_ptr_chirho,
                );
                // Log which fds are set and their data status
                if readfds_ptr_chirho != 0 && nfds_chirho > 0 {
                    let sz_chirho = core::cmp::min(16, ((nfds_chirho as usize + 7) / 8));
                    let mut tmp_chirho = [0u8; 16];
                    let _ = crate::uaccess_chirho::copy_from_user_chirho(&mut tmp_chirho[..sz_chirho], readfds_ptr_chirho, sz_chirho);
                    for fd_chirho in 0..core::cmp::min(nfds_chirho as usize, 16) {
                        if tmp_chirho[fd_chirho / 8] & (1 << (fd_chirho % 8)) != 0 {
                            let is_sock_chirho = crate::net_chirho::is_socket_fd_chirho(fd_chirho as u64);
                            let has_data_chirho = crate::net_chirho::socket_has_data_chirho(fd_chirho as u64);
                            let has_fd_chirho = crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64).is_some();
                            let inode_mode_chirho = crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64)
                                .map(|f| f.lock().inode_chirho.lock().mode_chirho)
                                .unwrap_or(0);
                            crate::serial_println_chirho!(
                                "[SELECT-DBG] fd={} sock={} data={} vfs={} mode={:#o}",
                                fd_chirho, is_sock_chirho, has_data_chirho, has_fd_chirho, inode_mode_chirho
                            );
                        }
                    }
                }
            }
        }
    }

    // Read the fd_set once from userspace for both initial check and re-check loop.
    let mut fds_buf_chirho = [0u8; 128];
    let set_size_chirho = if readfds_ptr_chirho != 0 && nfds_chirho > 0 {
        let sz_chirho = core::cmp::min(128, ((nfds_chirho as usize + 7) / 8));
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut fds_buf_chirho[..sz_chirho], readfds_ptr_chirho, sz_chirho,
        ).is_err() {
            return -EFAULT_CHIRHO;
        }
        sz_chirho
    } else {
        0
    };

    // Check if any of the readfds have actual data available.
    let mut has_ready_chirho = false;
    if set_size_chirho > 0 {
        {
            // Check each fd in the set
            for fd_chirho in 0..nfds_chirho as usize {
                let byte_idx_chirho = fd_chirho / 8;
                let bit_idx_chirho = fd_chirho % 8;
                if byte_idx_chirho < set_size_chirho && fds_buf_chirho[byte_idx_chirho] & (1 << bit_idx_chirho) != 0 {
                    // Check if this fd is a socket with pending data
                    if crate::net_chirho::socket_has_data_chirho(fd_chirho as u64) {
                        has_ready_chirho = true;
                        break;
                    }
                    // Regular files are always "ready" — except fd=0
                    // (stdin) for daemon processes (PID >= 2), which should
                    // only be ready when serial has actual data. Without this,
                    // dropbear spins in its select loop on empty stdin.
                    if !crate::net_chirho::is_socket_fd_chirho(fd_chirho as u64) {
                        if fd_chirho == 0 {
                            
                            if is_interactive_shell_chirho() {
                                has_ready_chirho = true;
                                break;
                            }
                            // Daemon: check serial LSR AND TCP port 2222
                            let lsr_chirho: u8 = unsafe {
                                x86_64::instructions::port::Port::<u8>::new(0x3FD).read()
                            };
                            if lsr_chirho & 1 != 0
                                || crate::net_chirho::has_tcp_data_for_port_chirho(2222)
                            {
                                has_ready_chirho = true;
                                break;
                            }
                        } else if crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64).is_some() {
                            has_ready_chirho = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    // Helper: scan fds_buf for ready sockets, build output fd_set, return count.
    let write_ready_fds_chirho = |fds_buf_chirho: &[u8; 128], set_size_chirho: usize,
                                   nfds_chirho: i32, readfds_ptr_chirho: u64| -> i64 {
        let mut out_fds_chirho = [0u8; 128];
        let mut count_chirho: i64 = 0;
        for fd_chirho in 0..nfds_chirho as usize {
            let byte_idx_chirho = fd_chirho / 8;
            let bit_idx_chirho = fd_chirho % 8;
            if byte_idx_chirho < set_size_chirho
                && fds_buf_chirho[byte_idx_chirho] & (1 << bit_idx_chirho) != 0
            {
                let ready_chirho = if crate::net_chirho::is_socket_fd_chirho(fd_chirho as u64) {
                    crate::net_chirho::socket_has_data_chirho(fd_chirho as u64)
                } else if fd_chirho == 0 {
                    // stdin: daemon PIDs check serial LSR + TCP port 2222
                    if is_interactive_shell_chirho() { true } else {
                        let lsr_chirho: u8 = unsafe {
                            x86_64::instructions::port::Port::<u8>::new(0x3FD).read()
                        };
                        lsr_chirho & 1 != 0
                            || crate::net_chirho::has_tcp_data_for_port_chirho(2222)
                    }
                } else {
                    crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64).is_some()
                };
                if ready_chirho {
                    out_fds_chirho[byte_idx_chirho] |= 1 << bit_idx_chirho;
                    count_chirho += 1;
                }
            }
        }
        if count_chirho > 0 && readfds_ptr_chirho != 0 {
            // Write the modified fd_set back to userspace.
            if crate::uaccess_chirho::copy_to_user_chirho(
                readfds_ptr_chirho, &out_fds_chirho[..set_size_chirho], set_size_chirho,
            ).is_err() {
                return -EFAULT_CHIRHO;
            }
        }
        count_chirho
    };

    if has_ready_chirho {
        write_ready_fds_chirho(&fds_buf_chirho, set_size_chirho, nfds_chirho, readfds_ptr_chirho)
    } else {
        // Block: yield to fork children, then HLT loop waiting for data.
        // Remove self from scheduler so fork children run uninterrupted.
        // This prevents the context switch #UD when switching back.
        let my_pid_chirho = crate::task_chirho::current_task_chirho()
            .map(|t| t.lock().pid_chirho).unwrap_or(0);
        if crate::scheduler_chirho::has_runnable_tasks_chirho() {
                // Yield once so fork children get CPU time
            crate::scheduler_chirho::yield_current_chirho();
        }

        // Wait long enough for TCP data to arrive. 200 was too short —
        // SSH client timed out before dropbear's select woke up.
        let max_attempts_chirho = 50_000u32;
        for _attempt_chirho in 0..max_attempts_chirho {
            x86_64::instructions::interrupts::enable_and_hlt();
            crate::net_chirho::poll_network_chirho();

            let count_chirho = write_ready_fds_chirho(
                &fds_buf_chirho, set_size_chirho, nfds_chirho, readfds_ptr_chirho,
            );
            if count_chirho > 0 {
                crate::serial_debug_chirho!("[SELECT] woke: {} fds ready", count_chirho);
                return count_chirho;
            }
        }
        0
    }
}

/// Simplified epoll state: maps monitored fds to their event data.
/// Each entry is (fd, events_mask, data_u64).
static EPOLL_ENTRIES_CHIRHO: spin::Mutex<alloc::vec::Vec<(i32, u32, u64)>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// `epoll_create1(2)` — create an epoll instance.
fn sys_epoll_create1_chirho(_flags_chirho: u32) -> i64 {
    static NEXT_EPOLL_FD_CHIRHO: AtomicU64 = AtomicU64::new(100);
    let fd_chirho = NEXT_EPOLL_FD_CHIRHO.fetch_add(1, Ordering::SeqCst);
    fd_chirho as i64
}

/// `epoll_ctl(2)` — add/modify/delete a fd in the epoll interest list.
fn sys_epoll_ctl_chirho(
    _epfd_chirho: i32,
    op_chirho: i32,
    fd_chirho: i32,
    event_ptr_chirho: u64,
) -> i64 {
    const EPOLL_CTL_ADD_CHIRHO: i32 = 1;
    const EPOLL_CTL_DEL_CHIRHO: i32 = 2;
    const EPOLL_CTL_MOD_CHIRHO: i32 = 3;

    // Read epoll_event struct from userspace: { u32 events, u64 data } = 12 bytes packed
    let (events_chirho, data_chirho) = if event_ptr_chirho != 0 {
        let ev_chirho = unsafe { core::ptr::read(event_ptr_chirho as *const u32) };
        let dt_chirho = unsafe { core::ptr::read((event_ptr_chirho + 4) as *const u64) };
        (ev_chirho, dt_chirho)
    } else {
        (0, 0)
    };

    let mut entries_chirho = EPOLL_ENTRIES_CHIRHO.lock();
    match op_chirho {
        EPOLL_CTL_ADD_CHIRHO => {
            entries_chirho.push((fd_chirho, events_chirho, data_chirho));
        }
        EPOLL_CTL_DEL_CHIRHO => {
            entries_chirho.retain(|(f_chirho, _, _)| *f_chirho != fd_chirho);
        }
        EPOLL_CTL_MOD_CHIRHO => {
            for entry_chirho in entries_chirho.iter_mut() {
                if entry_chirho.0 == fd_chirho {
                    entry_chirho.1 = events_chirho;
                    entry_chirho.2 = data_chirho;
                }
            }
        }
        _ => return -EINVAL_CHIRHO,
    }
    0
}

/// `epoll_wait(2)` / `epoll_pwait(2)` — wait for events on sockets.
///
/// Simplified: polls all sockets for data/connections, blocks via HLT
/// if nothing ready. Writes epoll_event structs to userspace when ready.
fn sys_epoll_wait_chirho(
    _epfd_chirho: i32,
    events_ptr_chirho: u64,
    maxevents_chirho: i32,
    timeout_chirho: i32,
) -> i64 {
    if events_ptr_chirho == 0 || maxevents_chirho <= 0 {
        return -EINVAL_CHIRHO;
    }

    crate::serial_debug_chirho!(
        "[EPOLL] wait: maxev={} timeout={} entries={}",
        maxevents_chirho, timeout_chirho,
        EPOLL_ENTRIES_CHIRHO.lock().len(),
    );

    // Try up to 1000 HLT cycles (~10s). If timeout is 0, don't block.
    let max_attempts_chirho = if timeout_chirho == 0 { 1u32 }
        else if timeout_chirho < 0 { 1000 } // infinite → cap at 10s
        else { core::cmp::min((timeout_chirho as u32) / 10, 1000) };

    for _attempt_chirho in 0..max_attempts_chirho {
        crate::net_chirho::poll_network_chirho();

        // Scan registered epoll entries for ready fds.
        let mut count_chirho: i32 = 0;
        let entries_chirho = EPOLL_ENTRIES_CHIRHO.lock();
        for &(fd_chirho, mask_chirho, data_chirho) in entries_chirho.iter() {
            if count_chirho >= maxevents_chirho { break; }

            let mut ready_events_chirho: u32 = 0;

            if crate::net_chirho::is_socket_fd_chirho(fd_chirho as u64) {
                if crate::net_chirho::socket_has_data_chirho(fd_chirho as u64) {
                    ready_events_chirho |= 0x001; // EPOLLIN
                }
                // Connected sockets: always writable.
                // Check state via socket table.
                let table_chirho = crate::net_chirho::SOCKET_TABLE_CHIRHO.lock();
                if let Ok(idx_chirho) = crate::net_chirho::socket_idx_from_fd_pub_chirho(fd_chirho as u64) {
                    if let Some(Some(ref sock_chirho)) = table_chirho.get(idx_chirho) {
                        if sock_chirho.state_chirho == crate::net_chirho::SocketStateChirho::ConnectedChirho {
                            ready_events_chirho |= 0x004; // EPOLLOUT
                        }
                    }
                }
            } else {
                // Regular file: always ready.
                ready_events_chirho |= mask_chirho & (0x001 | 0x004);
            }

            if ready_events_chirho != 0 {
                // Write epoll_event: { u32 events, u64 data } = 12 bytes packed
                let offset_chirho = (count_chirho as u64) * 12;
                let event_addr_chirho = events_ptr_chirho + offset_chirho;
                unsafe {
                    core::ptr::write(event_addr_chirho as *mut u32, ready_events_chirho);
                    core::ptr::write((event_addr_chirho + 4) as *mut u64, data_chirho);
                }
                count_chirho += 1;
            }
        }
        drop(entries_chirho);

        if count_chirho > 0 {
            crate::serial_debug_chirho!(
                "[EPOLL] returning {} events", count_chirho,
            );
            return count_chirho as i64;
        }

        // Nothing ready — yield CPU, wait for interrupt.
        if timeout_chirho != 0 {
            x86_64::instructions::interrupts::enable_and_hlt();
            crate::net_chirho::poll_network_chirho();
            // Yield to fork children during blocking wait.
            if crate::scheduler_chirho::has_runnable_tasks_chirho() {
                crate::scheduler_chirho::schedule_chirho();
                crate::scheduler_chirho::reset_time_slice_chirho();
            }
        } else {
            return 0; // Non-blocking: return immediately.
        }
    }
    0 // Timed out.
}

// ============================================================================
// mount / umount implementations (P3-035)
// ============================================================================

/// `mount(2)` implementation.
///
/// Reads the fstype string from user space and dispatches to the
/// appropriate filesystem mount function, then adds to the mount table.
fn sys_mount_chirho(
    _source_chirho: u64,
    target_chirho: u64,
    fstype_chirho: u64,
    _flags_chirho: u64,
    _data_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[MOUNT-DBG] sys_mount called: source={:#x} target={:#x} fstype={:#x} flags={:#x}",
        _source_chirho, target_chirho, fstype_chirho, _flags_chirho
    );
    // Read target path
    let target_path_chirho = match crate::uaccess_chirho::read_user_string_chirho(target_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Read fstype string
    let fstype_str_chirho = match crate::uaccess_chirho::read_user_string_chirho(fstype_chirho, 256) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    crate::serial_debug_chirho!(
        "[SYSCALL] mount(target={}, fstype={})",
        target_path_chirho,
        fstype_str_chirho,
    );

    // Dispatch to the appropriate filesystem mount function
    let sb_chirho = match fstype_str_chirho.as_str() {
        "tmpfs" => crate::tmpfs_chirho::mount_tmpfs_chirho(),
        "proc" | "procfs" => crate::procfs_chirho::mount_procfs_chirho(),
        "devtmpfs" => crate::devtmpfs_chirho::mount_devtmpfs_chirho(),
        "sysfs" => crate::sysfs_chirho::mount_sysfs_chirho(),
        "ext4" | "ext2" | "ext3" | "" => {
            // Mount ext4 filesystem from a block device (e.g., /dev/loop0).
            // Read the source device path.
            let source_path_chirho = match crate::uaccess_chirho::read_user_string_chirho(
                _source_chirho, 256
            ) {
                Ok(s_chirho) => s_chirho,
                Err(_) => return -EFAULT_CHIRHO,
            };
            crate::serial_println_chirho!(
                "[MOUNT] ext4 mount: source={} target={}",
                source_path_chirho, target_path_chirho,
            );
            // Read the ext4 superblock from the source device via our VFS.
            // Open the device, read 4096 bytes starting at offset 0.
            match crate::ext4_chirho::mount_ext4_from_device_chirho(
                &source_path_chirho,
            ) {
                Ok(sb_chirho) => sb_chirho,
                Err(e_chirho) => {
                    crate::serial_println_chirho!(
                        "[MOUNT] ext4 mount failed: {}",
                        e_chirho
                    );
                    return -EINVAL_CHIRHO;
                }
            }
        }
        _ => {
            crate::serial_println_chirho!(
                "[SYSCALL] mount: unsupported fstype '{}'",
                fstype_str_chirho,
            );
            return -ENODEV_CHIRHO;
        }
    };

    // Add to the mount table
    {
        use alloc::string::String;
        let mut mounts_chirho = crate::fs_chirho::MOUNT_TABLE_CHIRHO.lock();
        mounts_chirho.push(crate::fs_chirho::MountPointChirho {
            path_chirho: String::from(target_path_chirho.as_str()),
            superblock_chirho: sb_chirho,
        });
    }

    // After a successful ext4 loop mount, verify by trying to read a file.
    if target_path_chirho == "/mnt" {
        crate::serial_println_chirho!("[MOUNT] Verifying /mnt mount...");
        match crate::fs_chirho::resolve_path_chirho("/mnt/matthew712_chirho.txt") {
            Ok((_inode_chirho, _ops_chirho)) => {
                crate::serial_println_chirho!("[MOUNT] /mnt/matthew712_chirho.txt FOUND!");
                // Try reading the file content
                let mut buf_chirho = alloc::vec![0u8; 256];
                let file_arc_chirho = alloc::sync::Arc::new(spin::Mutex::new(
                    crate::vfs_chirho::FileChirho {
                        inode_chirho: _inode_chirho,
                        pos_chirho: 0,
                        flags_chirho: 0,
                        ops_chirho: _ops_chirho,
                    },
                ));
                let mut file_chirho = file_arc_chirho.lock();
                match file_chirho.ops_chirho.read_chirho(&mut file_chirho, &mut buf_chirho) {
                    Ok(n_chirho) => {
                        let text_chirho = core::str::from_utf8(&buf_chirho[..n_chirho])
                            .unwrap_or("<binary>");
                        crate::serial_println_chirho!(
                            "[MOUNT] matthew712_chirho.txt ({} bytes):", n_chirho
                        );
                        // Print the file content directly to serial — this IS
                        // the demo proof that the loop mount ext4 read works.
                        crate::serial_println_chirho!("{}", text_chirho);
                    }
                    Err(e_chirho) => {
                        crate::serial_println_chirho!(
                            "[MOUNT] matthew712_chirho.txt read error: {}",
                            e_chirho
                        );
                    }
                }

                // Demo: write "Aleluya" to a new file on the loop mount,
                // then read it back to verify the full write pipeline.
                crate::serial_println_chirho!("[MOUNT] Writing aleluya_chirho.txt...");
                let write_data_chirho = b"Aleluya! Hallelujah! Glory to God in Jesus name!\n";
                match crate::ext4_chirho::write_and_readback_chirho(
                    "/mnt", "aleluya_chirho.txt", write_data_chirho
                ) {
                    Ok(readback_chirho) => {
                        crate::serial_println_chirho!(
                            "[MOUNT] Wrote {} bytes, read back: {}",
                            write_data_chirho.len(),
                            core::str::from_utf8(&readback_chirho).unwrap_or("<bin>")
                        );
                    }
                    Err(e_chirho) => {
                        crate::serial_println_chirho!("[MOUNT] Write/readback: {}", e_chirho);
                    }
                }
            }
            Err(e_chirho) => {
                crate::serial_println_chirho!(
                    "[MOUNT] /mnt/matthew712_chirho.txt not found: {}",
                    e_chirho
                );
                // Try listing /mnt/ entries
                match crate::fs_chirho::resolve_path_chirho("/mnt") {
                    Ok((dir_inode_chirho, dir_ops_chirho)) => {
                        let mut entries_chirho = alloc::vec::Vec::new();
                        let mut file_chirho = crate::vfs_chirho::FileChirho {
                            inode_chirho: dir_inode_chirho,
                            pos_chirho: 0,
                            flags_chirho: 0,
                            ops_chirho: dir_ops_chirho,
                        };
                        if let Err(readdir_error_chirho) = file_chirho.ops_chirho.readdir_chirho(
                            &mut file_chirho,
                            &mut |name_chirho, _ino, _type| {
                                entries_chirho.push(alloc::string::String::from(name_chirho));
                                true
                            },
                        ) {
                            crate::serial_println_chirho!(
                                "[MOUNT] /mnt readdir failed during debug listing: {}",
                                readdir_error_chirho
                            );
                        }
                        crate::serial_println_chirho!(
                            "[MOUNT] /mnt/ listing: {:?}",
                            entries_chirho
                        );
                    }
                    Err(_) => {}
                }
            }
        }
    }

    0
}

/// `umount2(2)` stub -- return 0.
fn sys_umount2_chirho(_target_chirho: u64, _flags_chirho: u32) -> i64 {
    crate::serial_debug_chirho!("[SYSCALL] umount2 (stub) -> 0");
    0
}

/// `getcwd(2)` stub.
///
/// Returns "/" as the current working directory.
fn sys_getcwd_chirho(buf_chirho: *mut u8, size_chirho: usize) -> i64 {
    if buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    let cwd_chirho = get_task_cwd_chirho();
    let cwd_bytes_chirho = cwd_chirho.as_bytes();
    let needed_chirho = cwd_bytes_chirho.len() + 1; // +1 for NUL

    if size_chirho < needed_chirho {
        return -ERANGE_CHIRHO;
    }

    // Copy CWD to user buffer.
    unsafe {
        core::ptr::copy_nonoverlapping(
            cwd_bytes_chirho.as_ptr(),
            buf_chirho,
            cwd_bytes_chirho.len(),
        );
        *buf_chirho.add(cwd_bytes_chirho.len()) = 0; // NUL terminator
    }

    // Linux getcwd returns the buf pointer on success (cast to long).
    buf_chirho as i64
}

// ============================================================================
// Phase 3 batch 1 syscall implementations
// ============================================================================

/// `getppid(2)` -- return parent PID.
///
/// Returns the current task's actual parent PID.
fn sys_getppid_chirho() -> i64 {
    match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => {
            let ppid_chirho = t_chirho.lock().ppid_chirho;
            if ppid_chirho == 0 { 1 } else { ppid_chirho as i64 }
        }
        None => 1, // fallback
    }
}

// ============================================================================
// Process group / session syscall implementations
// ============================================================================

/// `setsid(2)` -- create a new session and set the process group ID.
///
/// The calling process becomes the leader of a new session and the leader of
/// a new process group.  The controlling terminal is detached.
///
/// Returns the new session ID (= caller's PID) on success, or `-EPERM` if the
/// caller is already a process group leader (pgid == pid).
fn sys_setsid_chirho() -> i64 {
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return -EPERM_CHIRHO,
    };
    let mut task_chirho = task_arc_chirho.lock();
    let pid_chirho = task_chirho.pid_chirho;

    // POSIX: setsid fails if caller is already a process group leader.
    if task_chirho.pgid_chirho == pid_chirho && task_chirho.sid_chirho == pid_chirho {
        crate::serial_debug_chirho!(
            "[SYSCALL] setsid() EPERM: PID {} already session leader",
            pid_chirho
        );
        return -EPERM_CHIRHO;
    }

    // Create new session: sid = pid, pgid = pid, detach controlling tty.
    task_chirho.sid_chirho = pid_chirho;
    task_chirho.pgid_chirho = pid_chirho;
    task_chirho.controlling_tty_chirho = None;

    crate::serial_debug_chirho!(
        "[SYSCALL] setsid() PID {} -> new session {}",
        pid_chirho,
        pid_chirho
    );
    pid_chirho as i64
}

/// `setpgid(2)` -- set process group ID for a process.
///
/// `setpgid(0, 0)` sets the caller's pgid to its own pid (creates a new
/// process group).  `setpgid(pid, pgid)` sets the target's pgid.
///
/// Returns 0 on success, `-ESRCH` if the target PID is not found, `-EPERM`
/// on permission errors.
fn sys_setpgid_chirho(pid_arg_chirho: u64, pgid_arg_chirho: u64) -> i64 {
    let current_pid_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho.lock().pid_chirho,
        None => return -ESRCH_CHIRHO,
    };

    // If pid == 0, target is the current process.
    let target_pid_chirho = if pid_arg_chirho == 0 {
        current_pid_chirho
    } else {
        pid_arg_chirho
    };

    // If pgid == 0, set pgid to the target's PID.
    let new_pgid_chirho = if pgid_arg_chirho == 0 {
        target_pid_chirho
    } else {
        pgid_arg_chirho
    };

    // Find the target task and update its pgid.
    let target_arc_chirho = match crate::task_chirho::find_task_by_pid_chirho(target_pid_chirho) {
        Some(t_chirho) => t_chirho,
        None => {
            crate::serial_debug_chirho!(
                "[SYSCALL] setpgid({}, {}) ESRCH: no such process",
                target_pid_chirho,
                new_pgid_chirho
            );
            return -ESRCH_CHIRHO;
        }
    };

    {
        let mut target_chirho = target_arc_chirho.lock();

        // POSIX: cannot change pgid of a session leader.
        if target_chirho.sid_chirho == target_chirho.pid_chirho
            && target_chirho.pgid_chirho == target_chirho.pid_chirho
            && new_pgid_chirho != target_chirho.pid_chirho
        {
            crate::serial_debug_chirho!(
                "[SYSCALL] setpgid({}, {}) EPERM: target is session leader",
                target_pid_chirho,
                new_pgid_chirho
            );
            return -EPERM_CHIRHO;
        }

        target_chirho.pgid_chirho = new_pgid_chirho;
    }

    crate::serial_debug_chirho!(
        "[SYSCALL] setpgid({}, {}) -> 0",
        target_pid_chirho,
        new_pgid_chirho
    );
    0
}

/// `getpgrp(2)` -- get the calling process's process group ID.
///
/// Equivalent to `getpgid(0)`.
fn sys_getpgrp_chirho() -> i64 {
    match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho.lock().pgid_chirho as i64,
        None => 1, // fallback
    }
}

/// `getpgid(2)` -- get process group ID for a process.
///
/// If `pid_arg_chirho` is 0, returns the caller's pgid.  Otherwise returns
/// the pgid of the specified process.
fn sys_getpgid_chirho(pid_arg_chirho: u64) -> i64 {
    if pid_arg_chirho == 0 {
        return sys_getpgrp_chirho();
    }
    match crate::task_chirho::find_task_by_pid_chirho(pid_arg_chirho) {
        Some(t_chirho) => t_chirho.lock().pgid_chirho as i64,
        None => -ESRCH_CHIRHO,
    }
}

/// `getsid(2)` -- get session ID for a process.
///
/// If `pid_arg_chirho` is 0, returns the caller's session ID.  Otherwise
/// returns the session ID of the specified process.
fn sys_getsid_chirho(pid_arg_chirho: u64) -> i64 {
    if pid_arg_chirho == 0 {
        return match crate::task_chirho::current_task_chirho() {
            Some(t_chirho) => t_chirho.lock().sid_chirho as i64,
            None => 1, // fallback
        };
    }
    match crate::task_chirho::find_task_by_pid_chirho(pid_arg_chirho) {
        Some(t_chirho) => t_chirho.lock().sid_chirho as i64,
        None => -ESRCH_CHIRHO,
    }
}

/// `getuid(2)` -- return user ID (root = 0).
fn sys_getuid_chirho() -> i64 {
    0
}

/// `geteuid(2)` -- return effective user ID (root = 0).
fn sys_geteuid_chirho() -> i64 {
    0
}

/// `getgid(2)` -- return group ID (root = 0).
fn sys_getgid_chirho() -> i64 {
    0
}

/// `getegid(2)` -- return effective group ID (root = 0).
fn sys_getegid_chirho() -> i64 {
    0
}

/// `getresuid(2)` -- write real, effective, saved UIDs to user buffers.
///
/// Stub: writes 0 (root) to all three pointers.
fn sys_getresuid_chirho(ruid_ptr_chirho: u64, euid_ptr_chirho: u64, suid_ptr_chirho: u64) -> i64 {
    let zero_bytes_chirho = 0u32.to_ne_bytes();
    for ptr_chirho in [ruid_ptr_chirho, euid_ptr_chirho, suid_ptr_chirho] {
        if ptr_chirho != 0 {
            if crate::uaccess_chirho::copy_to_user_chirho(ptr_chirho, &zero_bytes_chirho, 4).is_err()
            {
                return -14; // EFAULT
            }
        }
    }
    0
}

/// `getresgid(2)` -- write real, effective, saved GIDs to user buffers.
///
/// Stub: writes 0 (root) to all three pointers.
fn sys_getresgid_chirho(rgid_ptr_chirho: u64, egid_ptr_chirho: u64, sgid_ptr_chirho: u64) -> i64 {
    let zero_bytes_chirho = 0u32.to_ne_bytes();
    for ptr_chirho in [rgid_ptr_chirho, egid_ptr_chirho, sgid_ptr_chirho] {
        if ptr_chirho != 0 {
            if crate::uaccess_chirho::copy_to_user_chirho(ptr_chirho, &zero_bytes_chirho, 4).is_err()
            {
                return -14; // EFAULT
            }
        }
    }
    0
}

/// `gettid(2)` -- return thread ID (= PID for single-threaded).
/// TID must be >= 1 (musl uses 0 as "unlocked" sentinel).
fn sys_gettid_chirho() -> i64 {
    let pid_chirho = sys_getpid_chirho();
    if pid_chirho < 1 { 1 } else { pid_chirho }
}

/// Compute monotonic seconds and nanoseconds from the tick counter.
///
/// Reads (without incrementing) the global tick counter and converts to
/// `(seconds, nanoseconds)` using `TICK_PERIOD_NS_CHIRHO`.
#[inline]
fn monotonic_from_ticks_chirho() -> (i64, i64) {
    let ticks_chirho = TICK_COUNTER_CHIRHO.load(Ordering::Relaxed);
    let mono_ns_chirho = ticks_chirho as i64 * TICK_PERIOD_NS_CHIRHO;
    let mono_sec_chirho = mono_ns_chirho / 1_000_000_000;
    let mono_nsec_chirho = mono_ns_chirho % 1_000_000_000;
    (mono_sec_chirho, mono_nsec_chirho)
}

/// Compute a `TimespecChirho` for the given `clock_id_chirho`.
///
/// - `CLOCK_REALTIME` (0): `BOOT_EPOCH_CHIRHO` + monotonic offset.
/// - `CLOCK_MONOTONIC` (1) and all others: pure monotonic time.
#[inline]
fn clock_gettime_value_chirho(clock_id_chirho: u64) -> TimespecChirho {
    let (mono_sec_chirho, mono_nsec_chirho) = monotonic_from_ticks_chirho();

    match clock_id_chirho {
        CLOCK_REALTIME_CHIRHO => TimespecChirho {
            tv_sec_chirho: BOOT_EPOCH_CHIRHO + mono_sec_chirho,
            tv_nsec_chirho: mono_nsec_chirho,
        },
        // CLOCK_MONOTONIC (1), CLOCK_PROCESS_CPUTIME_ID (2),
        // CLOCK_THREAD_CPUTIME_ID (3), CLOCK_MONOTONIC_RAW (4),
        // CLOCK_MONOTONIC_COARSE (6), CLOCK_BOOTTIME (7), and any
        // unknown clock ID -- all treated as monotonic.
        _ => TimespecChirho {
            tv_sec_chirho: mono_sec_chirho,
            tv_nsec_chirho: mono_nsec_chirho,
        },
    }
}

/// `clock_gettime(2)` implementation (A2-AUDIT-002).
///
/// For CLOCK_MONOTONIC, uses the tick counter * 10ms per tick.
/// For CLOCK_REALTIME, returns BOOT_EPOCH_CHIRHO + monotonic offset.
fn sys_clock_gettime_chirho(
    clock_id_chirho: u64,
    tp_chirho: *mut TimespecChirho,
) -> i64 {
    if tp_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    let ts_chirho = clock_gettime_value_chirho(clock_id_chirho);

    // SAFETY: Caller guarantees tp_chirho is a valid writable user-space pointer.
    unsafe {
        core::ptr::write(tp_chirho, ts_chirho);
    }
    0
}

/// Read the x86 Time Stamp Counter for PRNG seeding.
#[inline]
fn rdtsc_chirho() -> u64 {
    let lo_chirho: u32;
    let hi_chirho: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo_chirho,
            out("edx") hi_chirho,
            options(nomem, nostack, preserves_flags),
        );
    }
    ((hi_chirho as u64) << 32) | (lo_chirho as u64)
}

/// Xorshift64 PRNG step.
pub fn xorshift64_chirho(state_chirho: u64) -> u64 {
    let mut x_chirho = state_chirho;
    x_chirho ^= x_chirho << 13;
    x_chirho ^= x_chirho >> 7;
    x_chirho ^= x_chirho << 17;
    x_chirho
}

/// `getrandom(2)` implementation.
///
/// Fills the user buffer with pseudo-random bytes using a xorshift64 PRNG
/// seeded from the TSC (rdtsc instruction).
fn sys_getrandom_chirho(
    buf_chirho: *mut u8,
    buflen_chirho: usize,
    _flags_chirho: u32,
) -> i64 {
    if buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }
    if buflen_chirho == 0 {
        return 0;
    }

    // Seed PRNG from TSC on first use.
    let mut state_chirho = PRNG_STATE_CHIRHO.load(Ordering::Relaxed);
    if state_chirho == 0 {
        state_chirho = rdtsc_chirho();
        if state_chirho == 0 {
            state_chirho = 0xDEAD_BEEF_CAFE_BABE; // fallback seed
        }
        PRNG_STATE_CHIRHO.store(state_chirho, Ordering::Relaxed);
    }

    let mut offset_chirho: usize = 0;
    while offset_chirho < buflen_chirho {
        state_chirho = xorshift64_chirho(state_chirho);
        let bytes_chirho = state_chirho.to_ne_bytes();
        let remaining_chirho = buflen_chirho - offset_chirho;
        let chunk_chirho = if remaining_chirho < 8 { remaining_chirho } else { 8 };
        // SAFETY: Caller guarantees buf_chirho is writable for buflen_chirho bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes_chirho.as_ptr(),
                buf_chirho.add(offset_chirho),
                chunk_chirho,
            );
        }
        offset_chirho += chunk_chirho;
    }

    PRNG_STATE_CHIRHO.store(state_chirho, Ordering::Relaxed);
    buflen_chirho as i64
}

/// `prlimit64(2)` implementation.
///
/// Returns reasonable defaults for common resource limits.
fn sys_prlimit64_chirho(
    _pid_chirho: u32,
    resource_chirho: u64,
    _new_limit_chirho: *const Rlimit64Chirho,
    old_limit_chirho: *mut Rlimit64Chirho,
) -> i64 {
    // If old_limit is requested, fill in defaults.
    if !old_limit_chirho.is_null() {
        let limit_chirho = match resource_chirho {
            RLIMIT_STACK_CHIRHO => Rlimit64Chirho {
                rlim_cur_chirho: 8 * 1024 * 1024,  // 8 MB
                rlim_max_chirho: 8 * 1024 * 1024,
            },
            RLIMIT_NOFILE_CHIRHO => Rlimit64Chirho {
                rlim_cur_chirho: 1024,
                rlim_max_chirho: 1024,
            },
            _ => Rlimit64Chirho {
                rlim_cur_chirho: RLIM_INFINITY_CHIRHO,
                rlim_max_chirho: RLIM_INFINITY_CHIRHO,
            },
        };
        // SAFETY: Caller guarantees old_limit_chirho is writable.
        unsafe {
            core::ptr::write(old_limit_chirho, limit_chirho);
        }
    }
    // Silently accept new_limit settings (ignore them).
    0
}

/// `access(2)` stub -- pretend the file exists.
fn sys_access_chirho() -> i64 {
    0
}

/// `faccessat(2)` stub -- pretend the file exists.
fn sys_faccessat_chirho() -> i64 {
    0
}

/// Helper: check if a user-space NUL-terminated string matches `target_chirho`.
///
/// # Safety
///
/// `user_str_chirho` must point to a valid NUL-terminated string in user memory.
unsafe fn user_str_eq_chirho(user_str_chirho: *const u8, target_chirho: &[u8]) -> bool {
    for (i_chirho, &byte_chirho) in target_chirho.iter().enumerate() {
        let user_byte_chirho = unsafe { *user_str_chirho.add(i_chirho) };
        if user_byte_chirho != byte_chirho {
            return false;
        }
    }
    // Check NUL terminator in user string.
    let next_chirho = unsafe { *user_str_chirho.add(target_chirho.len()) };
    next_chirho == 0
}

/// Fallback path for /proc/self/exe readlink.
const PROC_SELF_EXE_FALLBACK_CHIRHO: &[u8] = b"/bin/busybox";

/// `readlink(2)` implementation.
///
/// For "/proc/self/exe", returns "/hello-chirho". For all others, returns -ENOENT.
fn sys_readlink_chirho(
    path_chirho: *const u8,
    buf_chirho: *mut u8,
    bufsiz_chirho: usize,
) -> i64 {
    if path_chirho.is_null() || buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // Check if path is "/proc/self/exe".
    let is_proc_self_exe_chirho = unsafe {
        user_str_eq_chirho(path_chirho, b"/proc/self/exe")
    };

    if is_proc_self_exe_chirho {
        // Use the stored executable path from execve, or fallback.
        let exe_len_chirho = CURRENT_EXE_PATH_LEN_CHIRHO.load(Ordering::Relaxed) as usize;
        if exe_len_chirho > 0 {
            let exe_path_chirho = CURRENT_EXE_PATH_CHIRHO.lock();
            let copy_len_chirho = core::cmp::min(bufsiz_chirho, exe_len_chirho);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    exe_path_chirho.as_ptr(),
                    buf_chirho,
                    copy_len_chirho,
                );
            }
            return copy_len_chirho as i64;
        }
        // Fallback
        let copy_len_chirho = core::cmp::min(bufsiz_chirho, PROC_SELF_EXE_FALLBACK_CHIRHO.len());
        unsafe {
            core::ptr::copy_nonoverlapping(
                PROC_SELF_EXE_FALLBACK_CHIRHO.as_ptr(),
                buf_chirho,
                copy_len_chirho,
            );
        }
        return copy_len_chirho as i64;
    }

    -ENOENT_CHIRHO
}

/// `readlinkat(2)` implementation.
///
/// Handles AT_FDCWD (-100) properly: for absolute paths or when dirfd is
/// AT_FDCWD, delegates to readlink. For "/proc/self/exe", returns "/hello-chirho".
fn sys_readlinkat_chirho(
    dirfd_chirho: i32,
    pathname_chirho: *const u8,
    buf_chirho: *mut u8,
    bufsiz_chirho: usize,
) -> i64 {
    if pathname_chirho.is_null() || buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // AT_FDCWD = -100: use current working directory (which is "/")
    // For absolute paths (starting with '/'), dirfd is ignored per POSIX.
    // For relative paths with a real dirfd, we don't support that yet.
    let first_byte_chirho = unsafe { *pathname_chirho };
    if first_byte_chirho == b'/' || dirfd_chirho == -100 {
        // Absolute path or AT_FDCWD: delegate to readlink
        return sys_readlink_chirho(pathname_chirho, buf_chirho, bufsiz_chirho);
    }

    // Relative path with a real dirfd -- not yet supported
    -ENOENT_CHIRHO
}

/// `execveat(2)` implementation.
///
/// Supports the Linux cases Dropbear actually uses:
/// - absolute paths
/// - relative paths resolved against `dirfd_chirho`
/// - `AT_EMPTY_PATH` executing the file referenced by `dirfd_chirho`
fn sys_execveat_real_chirho(
    dirfd_chirho: i32,
    pathname_addr_chirho: u64,
    argv_chirho: u64,
    envp_chirho: u64,
    flags_chirho: u32,
) -> i64 {
    const AT_EMPTY_PATH_CHIRHO: u32 = 0x1000;
    const AT_FDCWD_EXECVEAT_CHIRHO: i32 = -100;

    let raw_path_chirho = if pathname_addr_chirho == 0 {
        if flags_chirho & AT_EMPTY_PATH_CHIRHO != 0 {
            alloc::string::String::new()
        } else {
            return -EFAULT_CHIRHO;
        }
    } else {
        match crate::uaccess_chirho::read_user_string_chirho(pathname_addr_chirho, 4096) {
            Ok(path_chirho) => path_chirho,
            Err(_) => return -EFAULT_CHIRHO,
        }
    };

    let resolved_path_chirho = if raw_path_chirho.is_empty() {
        if flags_chirho & AT_EMPTY_PATH_CHIRHO == 0 {
            return -ENOENT_CHIRHO;
        }
        alloc::format!("/proc/self/fd/{}", dirfd_chirho)
    } else if raw_path_chirho.starts_with('/') {
        raw_path_chirho
    } else if dirfd_chirho == AT_FDCWD_EXECVEAT_CHIRHO {
        let mut cwd_path_chirho = crate::task_chirho::current_task_chirho()
            .map(|task_arc_chirho| {
                let task_guard_chirho = task_arc_chirho.lock();
                if task_guard_chirho.cwd_chirho.is_empty() {
                    alloc::string::String::from("/")
                } else {
                    task_guard_chirho.cwd_chirho.clone()
                }
            })
            .unwrap_or_else(|| alloc::string::String::from("/"));
        if !cwd_path_chirho.ends_with('/') {
            cwd_path_chirho.push('/');
        }
        cwd_path_chirho.push_str(&raw_path_chirho);
        cwd_path_chirho
    } else {
        match crate::fs_chirho::get_fd_path_chirho(dirfd_chirho as u64) {
            Some(mut dir_path_chirho) => {
                if !dir_path_chirho.ends_with('/') {
                    dir_path_chirho.push('/');
                }
                dir_path_chirho.push_str(&raw_path_chirho);
                dir_path_chirho
            }
            None => {
                let mut fallback_path_chirho = alloc::string::String::from("/");
                fallback_path_chirho.push_str(&raw_path_chirho);
                fallback_path_chirho
            }
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] execveat: dirfd={} flags={:#x} resolved path=\"{}\"",
        dirfd_chirho,
        flags_chirho,
        resolved_path_chirho,
    );

    crate::process_chirho::sys_execve_with_filename_chirho(
        resolved_path_chirho,
        argv_chirho,
        envp_chirho,
    )
}

/// `fcntl(2)` implementation.
///
/// Handles F_GETFD, F_SETFD, F_GETFL, F_SETFL for fd 0, 1, 2.
fn sys_fcntl_chirho(
    fd_chirho: u64,
    cmd_chirho: u64,
    arg_chirho: u64,
) -> i64 {
    match cmd_chirho {
        F_DUPFD_CHIRHO => {
            // TODO(exec-fd-compat-001): honor the minimum-fd arg precisely.
            crate::fs_chirho::sys_dup_chirho(fd_chirho)
        }
        F_DUPFD_CLOEXEC_CHIRHO => {
            let duplicated_fd_chirho = crate::fs_chirho::sys_dup_chirho(fd_chirho);
            if duplicated_fd_chirho < 0 {
                return duplicated_fd_chirho;
            }
            let cloexec_result_chirho =
                crate::fs_chirho::set_fd_cloexec_chirho(duplicated_fd_chirho as u64, true);
            if cloexec_result_chirho < 0 {
                return cloexec_result_chirho;
            }
            duplicated_fd_chirho
        }
        F_GETFD_CHIRHO => {
            match crate::fs_chirho::get_fd_cloexec_chirho(fd_chirho) {
                Ok(true) => FD_CLOEXEC_CHIRHO as i64,
                Ok(false) => 0,
                Err(errno_chirho) => errno_chirho,
            }
        }
        F_SETFD_CHIRHO => {
            let enable_cloexec_chirho = (arg_chirho & FD_CLOEXEC_CHIRHO) != 0;
            crate::fs_chirho::set_fd_cloexec_chirho(fd_chirho, enable_cloexec_chirho)
        }
        F_GETFL_CHIRHO => {
            // Return the file's open flags from the VFS file table.
            // A2-PROC-003: Use lookup_fd_chirho (per-process first).
            if let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
                let file_chirho = file_arc_chirho.lock();
                return file_chirho.flags_chirho as i64;
            }
            // Fallback for stdin/stdout/stderr
            match fd_chirho {
                0 => 0,     // O_RDONLY
                1 | 2 => 1, // O_WRONLY
                _ => 0x8000, // O_LARGEFILE default
            }
        }
        F_SETFL_CHIRHO => update_file_status_flags_chirho(fd_chirho, arg_chirho as u32),
        F_GETLK_CHIRHO => {
            // Advisory file locking: report "no lock held" by setting
            // l_type to F_UNLCK (2). sqlite3 uses this to probe locking.
            if arg_chirho != 0 {
                // struct flock: l_type is the first i16 field
                unsafe {
                    core::ptr::write(arg_chirho as *mut i16, 2); // F_UNLCK
                }
            }
            0
        }
        F_SETLK_CHIRHO | F_SETLKW_CHIRHO => {
            // Advisory file locking: silently succeed (single-process kernel).
            // sqlite3 requires this to succeed for database access.
            0
        }
        _ => {
            // Silently succeed for unknown commands instead of failing —
            // many programs probe fcntl capabilities and don't check errors.
            0
        }
    }
}

/// Helper: fill a `StatChirho` from an `InodeChirho`.
fn fill_stat_from_inode_chirho(
    st_chirho: &mut StatChirho,
    inode_chirho: &crate::vfs_chirho::InodeChirho,
) {
    st_chirho.st_dev_chirho = 0x0801; // major 8, minor 1 (sda1 equivalent)
    st_chirho.st_ino_chirho = inode_chirho.ino_chirho;
    st_chirho.st_mode_chirho = inode_chirho.mode_chirho;
    st_chirho.st_nlink_chirho = inode_chirho.nlink_chirho as u64;
    st_chirho.st_uid_chirho = inode_chirho.uid_chirho;
    st_chirho.st_gid_chirho = inode_chirho.gid_chirho;
    st_chirho.st_size_chirho = inode_chirho.size_chirho as i64;
    st_chirho.st_blksize_chirho = 4096;
    st_chirho.st_blocks_chirho = ((inode_chirho.size_chirho + 511) / 512) as i64;
    st_chirho.st_atime_chirho = inode_chirho.atime_chirho;
    st_chirho.st_mtime_chirho = inode_chirho.mtime_chirho;
    st_chirho.st_ctime_chirho = inode_chirho.ctime_chirho;
}

/// `fstat(2)` implementation.
///
/// Gets the file from the FD table and fills stat from its inode fields.
fn sys_fstat_chirho(
    fd_chirho: u64,
    statbuf_chirho: *mut StatChirho,
) -> i64 {
    if statbuf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    let mut st_chirho = StatChirho::zeroed_chirho();

    {
        let file_guard_chirho = file_arc_chirho.lock();
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        fill_stat_from_inode_chirho(&mut st_chirho, &inode_guard_chirho);
    }

    // SAFETY: Caller guarantees statbuf_chirho is writable user-space pointer.
    unsafe {
        core::ptr::write(statbuf_chirho, st_chirho);
    }
    0
}

// ============================================================================
// Stat family syscall implementations (wired to VFS)
// ============================================================================

/// `stat(2)` implementation.
///
/// Resolves the pathname via VFS path resolution and fills `statbuf_chirho`
/// with inode metadata.

/// Check if a name is a known BusyBox applet.
/// Delegates to the centralized registry in `busybox_chirho` module.
fn is_busybox_applet_chirho(name_chirho: &str) -> bool {
    crate::busybox_chirho::is_busybox_applet_chirho(name_chirho)
}

fn sys_stat_chirho(
    pathname_chirho: *const u8,
    statbuf_chirho: *mut StatChirho,
) -> i64 {
    if pathname_chirho.is_null() || statbuf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // Read pathname from user space
    let raw_path_chirho = match crate::uaccess_chirho::read_user_string_chirho(
        pathname_chirho as u64,
        4096,
    ) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Handle relative paths by prepending "/" (CWD is always "/")
    let path_str_chirho = if !raw_path_chirho.starts_with('/') {
        let mut full_chirho = alloc::string::String::from("/");
        full_chirho.push_str(&raw_path_chirho);
        full_chirho
    } else {
        raw_path_chirho
    };

    // Resolve through VFS
    let (inode_arc_chirho, _file_ops_chirho) = match crate::fs_chirho::resolve_path_chirho(&path_str_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => {
            // Intercept: if path is a BusyBox applet in /bin or /sbin,
            // return a fake stat result so ash finds the command.
            let basename_chirho = path_str_chirho.rsplit('/').next().unwrap_or("");
            if (path_str_chirho.starts_with("/bin/") || path_str_chirho.starts_with("/sbin/")
                || path_str_chirho.starts_with("/usr/bin/"))
                && is_busybox_applet_chirho(basename_chirho)
            {
                // Return a fake stat: regular file, executable
                let mut st_chirho = StatChirho::zeroed_chirho();
                st_chirho.st_mode_chirho = 0o100755; // S_IFREG | 0755
                st_chirho.st_size_chirho = 1131168;  // BusyBox size
                st_chirho.st_nlink_chirho = 1;
                st_chirho.st_blksize_chirho = 4096;
                st_chirho.st_blocks_chirho = (1131168 + 511) / 512;
                unsafe { core::ptr::write(statbuf_chirho, st_chirho); }
                return 0;
            }
            return errno_chirho;
        }
    };

    let mut st_chirho = StatChirho::zeroed_chirho();
    {
        let inode_guard_chirho = inode_arc_chirho.lock();
        fill_stat_from_inode_chirho(&mut st_chirho, &inode_guard_chirho);
    }

    // SAFETY: Caller guarantees statbuf_chirho is writable.
    unsafe {
        core::ptr::write(statbuf_chirho, st_chirho);
    }
    0
}

/// `lstat(2)` implementation.
///
/// Like stat but does not follow symlinks.  Currently identical to stat
/// since Lineluya does not yet have symlink-following path resolution.
fn sys_lstat_chirho(
    pathname_chirho: *const u8,
    statbuf_chirho: *mut StatChirho,
) -> i64 {
    // lstat is identical to stat until we add symlink following
    sys_stat_chirho(pathname_chirho, statbuf_chirho)
}

/// `fstatat(2)` / `newfstatat(2)` implementation (syscall 262).
///
/// Gets file status relative to a directory fd.  Resolves the pathname
/// through VFS (ignoring dirfd for absolute paths) and fills stat from
/// the resolved inode.
fn sys_fstatat_chirho(
    dirfd_chirho: i32,
    pathname_chirho: *const u8,
    statbuf_chirho: *mut StatChirho,
    flags_chirho: u32,
) -> i64 {
    if statbuf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // AT_EMPTY_PATH (0x1000): if pathname is empty, operate on dirfd itself
    const AT_EMPTY_PATH_CHIRHO: u32 = 0x1000;

    if pathname_chirho.is_null() {
        // NULL pathname with AT_EMPTY_PATH => fstat on dirfd
        if flags_chirho & AT_EMPTY_PATH_CHIRHO != 0 {
            return sys_fstat_chirho(dirfd_chirho as u64, statbuf_chirho);
        }
        return -EFAULT_CHIRHO;
    }

    // Read pathname from user space
    let path_str_chirho = match crate::uaccess_chirho::read_user_string_chirho(
        pathname_chirho as u64,
        4096,
    ) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // If empty string with AT_EMPTY_PATH, fstat the dirfd
    if path_str_chirho.is_empty() && (flags_chirho & AT_EMPTY_PATH_CHIRHO != 0) {
        return sys_fstat_chirho(dirfd_chirho as u64, statbuf_chirho);
    }

    // Handle relative paths: resolve against dirfd
    let resolved_path_chirho = if !path_str_chirho.starts_with('/') {
        if dirfd_chirho == -100 { // AT_FDCWD
            let mut full_chirho = alloc::string::String::from("/");
            full_chirho.push_str(&path_str_chirho);
            full_chirho
        } else {
            // Resolve relative to dirfd's path
            match crate::fs_chirho::get_fd_path_chirho(dirfd_chirho as u64) {
                Some(dp_chirho) => {
                    let mut p_chirho = dp_chirho;
                    if !p_chirho.ends_with('/') { p_chirho.push('/'); }
                    p_chirho.push_str(&path_str_chirho);
                    p_chirho
                }
                None => path_str_chirho,
            }
        }
    } else {
        path_str_chirho
    };

    let (inode_arc_chirho, _file_ops_chirho) = match crate::fs_chirho::resolve_path_chirho(&resolved_path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let mut st_chirho = StatChirho::zeroed_chirho();
    {
        let inode_guard_chirho = inode_arc_chirho.lock();
        fill_stat_from_inode_chirho(&mut st_chirho, &inode_guard_chirho);
    }

    // SAFETY: Caller guarantees statbuf_chirho is writable.
    unsafe {
        core::ptr::write(statbuf_chirho, st_chirho);
    }
    0
}

/// `statx(2)` implementation (syscall 332).
///
/// Extended stat interface. Returns -ENOSYS for now.
fn sys_statx_chirho(
    _dirfd_chirho: i32,
    _pathname_chirho: *const u8,
    _flags_chirho: u32,
    _mask_chirho: u32,
    _statx_buf_chirho: *mut u8,
) -> i64 {
    crate::serial_debug_chirho!("[SYSCALL] statx() -> ENOSYS (not yet implemented)");
    -ENOSYS_CHIRHO
}

// ============================================================================
// Directory syscall implementations (P3-015)
// ============================================================================

/// `mkdir(2)` implementation (syscall 83).
///
/// Resolves the parent directory in the live tmpfs tree and creates a new
/// directory entry via `InodeOps::mkdir_chirho`.
fn sys_mkdir_chirho(
    pathname_chirho: *const u8,
    mode_chirho: u32,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let raw_path_chirho = match read_user_string_chirho(pathname_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Make path absolute
    let path_chirho = if !raw_path_chirho.starts_with('/') {
        let mut full_chirho = alloc::string::String::from("/");
        full_chirho.push_str(&raw_path_chirho);
        full_chirho
    } else {
        raw_path_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] mkdir({}, {:#o})", path_chirho, mode_chirho);

    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    match parent_guard_chirho.ops_chirho.mkdir_chirho(&parent_guard_chirho, &name_chirho, mode_chirho) {
        Ok(_) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `mkdirat(2)` implementation (syscall 258).
///
/// Like mkdir but with a dirfd argument. Currently only handles absolute
/// paths and AT_FDCWD for relative paths.
fn sys_mkdirat_chirho(
    dirfd_chirho: i32,
    pathname_chirho: *const u8,
    mode_chirho: u32,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let raw_path_chirho = match read_user_string_chirho(pathname_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Make path absolute (handle AT_FDCWD)
    let path_chirho = if !raw_path_chirho.starts_with('/') {
        if dirfd_chirho == -100 {
            // AT_FDCWD
            let mut full_chirho = alloc::string::String::from("/");
            full_chirho.push_str(&raw_path_chirho);
            full_chirho
        } else {
            raw_path_chirho
        }
    } else {
        raw_path_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] mkdirat({}, {}, {:#o})", dirfd_chirho, path_chirho, mode_chirho);

    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    match parent_guard_chirho.ops_chirho.mkdir_chirho(&parent_guard_chirho, &name_chirho, mode_chirho) {
        Ok(_) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `rmdir(2)` implementation (syscall 84).
///
/// Resolves the parent directory in the live tmpfs tree and removes the
/// named directory entry via `InodeOps::rmdir_chirho`.
fn sys_rmdir_chirho(
    pathname_chirho: *const u8,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let raw_path_chirho = match read_user_string_chirho(pathname_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    let path_chirho = if !raw_path_chirho.starts_with('/') {
        let mut full_chirho = alloc::string::String::from("/");
        full_chirho.push_str(&raw_path_chirho);
        full_chirho
    } else {
        raw_path_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] rmdir({})", path_chirho);

    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    match parent_guard_chirho.ops_chirho.rmdir_chirho(&parent_guard_chirho, &name_chirho) {
        Ok(()) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `unlink(2)` implementation (syscall 87).
///
/// Resolves the parent directory in the live tmpfs tree and removes the
/// named file entry via `InodeOps::unlink_chirho`.
fn sys_unlink_chirho(
    pathname_chirho: *const u8,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let raw_path_chirho = match read_user_string_chirho(pathname_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    let path_chirho = if !raw_path_chirho.starts_with('/') {
        let mut full_chirho = alloc::string::String::from("/");
        full_chirho.push_str(&raw_path_chirho);
        full_chirho
    } else {
        raw_path_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] unlink({})", path_chirho);

    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    match parent_guard_chirho.ops_chirho.unlink_chirho(&parent_guard_chirho, &name_chirho) {
        Ok(()) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `unlinkat(2)` implementation (syscall 263).
///
/// Combines unlink and rmdir based on the AT_REMOVEDIR flag (0x200).
fn sys_unlinkat_chirho(
    dirfd_chirho: i32,
    pathname_chirho: *const u8,
    flags_chirho: u32,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    const AT_REMOVEDIR_CHIRHO: u32 = 0x200;

    let raw_path_chirho = match read_user_string_chirho(pathname_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    let path_chirho = if !raw_path_chirho.starts_with('/') {
        if dirfd_chirho == -100 {
            let mut full_chirho = alloc::string::String::from("/");
            full_chirho.push_str(&raw_path_chirho);
            full_chirho
        } else {
            raw_path_chirho
        }
    } else {
        raw_path_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] unlinkat({}, {}, {:#x})", dirfd_chirho, path_chirho, flags_chirho);

    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    if flags_chirho & AT_REMOVEDIR_CHIRHO != 0 {
        match parent_guard_chirho.ops_chirho.rmdir_chirho(&parent_guard_chirho, &name_chirho) {
            Ok(()) => 0,
            Err(errno_chirho) => errno_chirho,
        }
    } else {
        match parent_guard_chirho.ops_chirho.unlink_chirho(&parent_guard_chirho, &name_chirho) {
            Ok(()) => 0,
            Err(errno_chirho) => errno_chirho,
        }
    }
}

/// `symlinkat(2)` — create a symbolic link.
///
/// Creates a symlink at `linkpath` pointing to `target`.
/// On tmpfs, creates an inode with S_IFLNK mode storing the target path.
fn sys_symlinkat_chirho(
    target_ptr_chirho: u64,
    newdirfd_chirho: i64,
    linkpath_ptr_chirho: u64,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let target_chirho = match read_user_string_chirho(target_ptr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };
    let raw_linkpath_chirho = match read_user_string_chirho(linkpath_ptr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Resolve linkpath relative to newdirfd.
    let linkpath_chirho = if !raw_linkpath_chirho.starts_with('/') {
        if newdirfd_chirho == -100 {
            let cwd_chirho = get_task_cwd_chirho();
            let mut full_chirho = cwd_chirho;
            if !full_chirho.ends_with('/') { full_chirho.push('/'); }
            full_chirho.push_str(&raw_linkpath_chirho);
            full_chirho
        } else {
            raw_linkpath_chirho
        }
    } else {
        raw_linkpath_chirho
    };

    crate::serial_debug_chirho!("[SYSCALL] symlinkat('{}', '{}') ", &target_chirho, &linkpath_chirho);

    // Create the symlink via VFS: resolve parent, create symlink inode.
    let (parent_inode_chirho, name_chirho) = match crate::fs_chirho::resolve_parent_live_chirho(&linkpath_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    let parent_guard_chirho = parent_inode_chirho.lock();
    match parent_guard_chirho.ops_chirho.symlink_chirho(&parent_guard_chirho, &name_chirho, &target_chirho) {
        Ok(_) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `linkat(2)` — create a hard link.
///
/// On tmpfs, creates a new directory entry pointing to the same inode.
/// On ext4, this is not yet supported (returns EXDEV).
fn sys_linkat_chirho(
    _olddirfd_chirho: i64,
    oldpath_ptr_chirho: u64,
    _newdirfd_chirho: i64,
    newpath_ptr_chirho: u64,
    _flags_chirho: u32,
) -> i64 {
    use crate::uaccess_chirho::read_user_string_chirho;

    let _oldpath_chirho = match read_user_string_chirho(oldpath_ptr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };
    let _newpath_chirho = match read_user_string_chirho(newpath_ptr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    crate::serial_debug_chirho!("[SYSCALL] linkat('{}' -> '{}')", &_oldpath_chirho, &_newpath_chirho);

    // Hard links on tmpfs: stub success for now.
    // Real implementation needs the VFS to support multiple directory
    // entries pointing to the same inode (nlink > 1).
    0
}

/// `rename(2)` implementation (syscall 82).
///
/// Stub: logs and returns 0 (success).
fn sys_rename_chirho(
    _oldpath_chirho: *const u8,
    _newpath_chirho: *const u8,
) -> i64 {
    crate::serial_debug_chirho!("[SYSCALL] rename(oldpath, newpath) -> 0 (stub)");
    0
}

/// `renameat2(2)` implementation (syscall 316).
///
/// Stub: logs and returns 0 (success).
fn sys_renameat2_chirho(
    _olddirfd_chirho: i32,
    _oldpath_chirho: *const u8,
    _newdirfd_chirho: i32,
    _newpath_chirho: *const u8,
    _flags_chirho: u32,
) -> i64 {
    crate::serial_debug_chirho!("[SYSCALL] renameat2(olddirfd, oldpath, newdirfd, newpath, flags) -> 0 (stub)");
    0
}

/// `chdir(2)` implementation (syscall 80).
///
/// Changes the current working directory of the calling process.
/// The path must exist and be a directory.
fn sys_chdir_chirho(
    path_ptr_chirho: *const u8,
) -> i64 {
    let path_chirho = match crate::uaccess_chirho::read_user_string_chirho(path_ptr_chirho as u64, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Make the path absolute if relative.
    let abs_path_chirho = if path_chirho.starts_with('/') {
        path_chirho
    } else {
        // Prepend CWD.
        let cwd_chirho = get_task_cwd_chirho();
        let mut full_chirho = cwd_chirho;
        if !full_chirho.ends_with('/') {
            full_chirho.push('/');
        }
        full_chirho.push_str(&path_chirho);
        full_chirho
    };

    // Verify the path exists and is a directory.
    match crate::fs_chirho::resolve_path_chirho(&abs_path_chirho) {
        Ok((inode_chirho, _ops_chirho)) => {
            let mode_chirho = inode_chirho.lock().mode_chirho;
            if mode_chirho & 0o170000 != 0o040000 {
                // Not a directory.
                return -ENOTDIR_CHIRHO;
            }
        }
        Err(errno_chirho) => return errno_chirho,
    }

    // Update the task's CWD.
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        task_arc_chirho.lock().cwd_chirho = abs_path_chirho.clone();
    }

    crate::serial_debug_chirho!("[SYSCALL] chdir('{}') -> 0", &abs_path_chirho);
    0
}

/// Get the current task's working directory.
fn get_task_cwd_chirho() -> alloc::string::String {
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let task_chirho = task_arc_chirho.lock();
        if !task_chirho.cwd_chirho.is_empty() {
            return task_chirho.cwd_chirho.clone();
        }
    }
    alloc::string::String::from("/")
}

/// `getdents64(2)` implementation (syscall 217).
///
/// Reads directory entries from the file descriptor into a user-space buffer,
/// producing `LinuxDirent64Chirho` records.  Delegates to `FileOps::readdir`
/// for the actual directory iteration.
fn sys_getdents64_chirho(
    fd_chirho: u64,
    dirp_chirho: *mut u8,
    count_chirho: usize,
) -> i64 {
    use crate::uaccess_chirho::copy_to_user_chirho;

    if dirp_chirho.is_null() || count_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    // 1. A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // 2. Collect directory entries via readdir callback into a kernel buffer.
    // Cap at 64KB to prevent userspace from causing huge kernel allocations.
    let capped_count_chirho = core::cmp::min(count_chirho, 64 * 1024);
    let mut kernel_buf_chirho = alloc::vec![0u8; capped_count_chirho];
    let mut bytes_written_chirho: usize = 0;
    let mut error_chirho: Option<i64> = None;

    {
        let mut file_guard_chirho = file_arc_chirho.lock();
        let readdir_result_chirho = file_guard_chirho.ops_chirho.readdir_chirho(
            &mut file_guard_chirho,
            &mut |name_chirho: &str, ino_chirho: u64, d_type_chirho: u8| -> bool {
                // LinuxDirent64 layout:
                //   u64  d_ino     (8 bytes, offset 0)
                //   i64  d_off     (8 bytes, offset 8)
                //   u16  d_reclen  (2 bytes, offset 16)
                //   u8   d_type    (1 byte,  offset 18)
                //   char d_name[]  (variable, offset 19, NUL-terminated)
                // d_reclen must be 8-byte aligned
                let name_bytes_chirho = name_chirho.as_bytes();
                let name_len_chirho = name_bytes_chirho.len() + 1; // +1 for NUL
                let reclen_unaligned_chirho: usize = 8 + 8 + 2 + 1 + name_len_chirho; // 19 + name_len
                let reclen_chirho = (reclen_unaligned_chirho + 7) & !7; // align to 8

                if bytes_written_chirho + reclen_chirho > capped_count_chirho {
                    return false; // buffer full
                }

                let offset_chirho = bytes_written_chirho;
                let d_off_chirho = (bytes_written_chirho + reclen_chirho) as i64;

                // Write d_ino (u64)
                kernel_buf_chirho[offset_chirho..offset_chirho + 8]
                    .copy_from_slice(&ino_chirho.to_ne_bytes());
                // Write d_off (i64)
                kernel_buf_chirho[offset_chirho + 8..offset_chirho + 16]
                    .copy_from_slice(&d_off_chirho.to_ne_bytes());
                // Write d_reclen (u16)
                kernel_buf_chirho[offset_chirho + 16..offset_chirho + 18]
                    .copy_from_slice(&(reclen_chirho as u16).to_ne_bytes());
                // Write d_type (u8)
                kernel_buf_chirho[offset_chirho + 18] = d_type_chirho;
                // Write d_name (NUL-terminated)
                kernel_buf_chirho[offset_chirho + 19..offset_chirho + 19 + name_bytes_chirho.len()]
                    .copy_from_slice(name_bytes_chirho);
                kernel_buf_chirho[offset_chirho + 19 + name_bytes_chirho.len()] = 0; // NUL
                // Zero any padding bytes
                for i_chirho in (offset_chirho + 19 + name_len_chirho)..(offset_chirho + reclen_chirho) {
                    kernel_buf_chirho[i_chirho] = 0;
                }

                bytes_written_chirho += reclen_chirho;
                true // continue iteration
            },
        );

        if let Err(errno_chirho) = readdir_result_chirho {
            error_chirho = Some(errno_chirho);
        }
    }

    // Debug logging removed for clean output

    if let Some(errno_chirho) = error_chirho {
        return errno_chirho;
    }

    // 3. Copy kernel buffer to user space
    if bytes_written_chirho > 0 {
        if let Err(_) = copy_to_user_chirho(
            dirp_chirho as u64,
            &kernel_buf_chirho[..bytes_written_chirho],
            bytes_written_chirho,
        ) {
            return -EFAULT_CHIRHO;
        }
    }

    bytes_written_chirho as i64
}

// ============================================================================
// Phase 3 batch 2 syscall implementations (P3-021)
// ============================================================================

/// Static counter for allocating fake file descriptors (eventfd, timerfd, etc.)
static NEXT_FAKE_FD_CHIRHO: AtomicU64 = AtomicU64::new(200);

/// Allocate a fake file descriptor number for stub fd-returning syscalls.
fn sys_fake_fd_chirho() -> i64 {
    let fd_chirho = NEXT_FAKE_FD_CHIRHO.fetch_add(1, Ordering::SeqCst);
    fd_chirho as i64
}

/// `sysinfo(2)` implementation.
///
/// Queries the real kernel heap configuration from `HeapConfigChirho` rather
/// than returning hardcoded values (audit A2-AUDIT-004).
///
/// `totalram` = heap size.  `freeram` = heap size minus a conservative
/// estimate of used memory (large-alloc counter * 1 MiB).  We intentionally
/// cap reported free memory at half of total to prevent musl/dropbear from
/// attempting huge allocations that trigger OOM.
fn sys_sysinfo_chirho(info_chirho: *mut SysinfoChirho) -> i64 {
    if info_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    use crate::allocator_chirho::{HeapConfigChirho, LARGE_ALLOC_COUNT_CHIRHO};

    let ticks_chirho = TICK_COUNTER_CHIRHO.load(Ordering::Relaxed);
    let uptime_secs_chirho = (ticks_chirho as i64 * 10) / 1000; // ~10ms per tick

    // Real total from allocator constants.
    let total_ram_chirho = HeapConfigChirho::TOTAL_SIZE_CHIRHO as u64;

    // Estimate used memory from the large-alloc counter (each counted alloc
    // is >256KB; conservatively assume ~1MiB average).  Clamp so that we
    // never report more than half the heap as free — this prevents musl from
    // attempting huge mmap-style allocations that would OOM our kernel.
    let large_allocs_chirho =
        LARGE_ALLOC_COUNT_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
    let estimated_used_chirho = large_allocs_chirho * 1024 * 1024; // ~1MiB per large alloc
    let free_ram_chirho = if estimated_used_chirho >= total_ram_chirho {
        // Extremely unlikely; clamp to 10% free so userspace doesn't panic.
        total_ram_chirho / 10
    } else {
        let raw_free_chirho = total_ram_chirho - estimated_used_chirho;
        // Cap at half of total to keep musl/dropbear well-behaved.
        raw_free_chirho.min(total_ram_chirho / 2)
    };

    let si_chirho = SysinfoChirho {
        uptime_chirho: uptime_secs_chirho,
        loads_chirho: [0; 3],
        totalram_chirho: total_ram_chirho,
        freeram_chirho: free_ram_chirho,
        sharedram_chirho: 0,
        bufferram_chirho: 0,
        totalswap_chirho: 0,
        freeswap_chirho: 0,
        procs_chirho: crate::task_chirho::task_count_chirho() as u16,
        _pad_chirho: [0; 6],
        totalhigh_chirho: 0,
        freehigh_chirho: 0,
        mem_unit_chirho: 1,
        _padding_chirho: [0; 4],
    };

    // SAFETY: Caller guarantees info_chirho is a valid writable pointer.
    unsafe {
        core::ptr::write(info_chirho, si_chirho);
    }
    0
}

// ============================================================================
// statfs default constants (audit A2-AUDIT-004)
// ============================================================================

/// ext4 filesystem magic number (EXT4_SUPER_MAGIC).
const EXT4_SUPER_MAGIC_CHIRHO: i64 = 0xEF53;
/// Default block size (4 KiB).
const STATFS_BLOCK_SIZE_CHIRHO: i64 = 4096;
/// Default total block count (512 MiB / 4 KiB = 131072 blocks).
const STATFS_DEFAULT_BLOCKS_CHIRHO: u64 = 131072;
/// Default free block count (~half of total).
const STATFS_DEFAULT_BFREE_CHIRHO: u64 = 65536;
/// Default total inode count.
const STATFS_DEFAULT_FILES_CHIRHO: u64 = 32768;
/// Default free inode count.
const STATFS_DEFAULT_FFREE_CHIRHO: u64 = 16384;
/// Default maximum filename length.
const STATFS_NAMELEN_CHIRHO: i64 = 255;
/// Default filesystem ID.
const STATFS_FSID_CHIRHO: [i32; 2] = [0x1234, 0x5678];

/// `statfs(2)` / `fstatfs(2)` implementation.
///
/// Returns an ext4-like statfs struct. sqlite3 uses this to detect
/// filesystem type and choose appropriate locking strategy.
///
/// Uses named constants instead of raw magic numbers (audit A2-AUDIT-004).
/// When ext4 is mounted, these could be replaced with real superblock data;
/// for now the defaults are accurate for the Alpine rootfs layout.
fn sys_statfs_chirho(buf_chirho: *mut StatfsChirho) -> i64 {
    if buf_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    let sf_chirho = StatfsChirho {
        f_type_chirho: EXT4_SUPER_MAGIC_CHIRHO,
        f_bsize_chirho: STATFS_BLOCK_SIZE_CHIRHO,
        f_blocks_chirho: STATFS_DEFAULT_BLOCKS_CHIRHO,
        f_bfree_chirho: STATFS_DEFAULT_BFREE_CHIRHO,
        f_bavail_chirho: STATFS_DEFAULT_BFREE_CHIRHO,
        f_files_chirho: STATFS_DEFAULT_FILES_CHIRHO,
        f_ffree_chirho: STATFS_DEFAULT_FFREE_CHIRHO,
        f_fsid_chirho: STATFS_FSID_CHIRHO,
        f_namelen_chirho: STATFS_NAMELEN_CHIRHO,
        f_frsize_chirho: STATFS_BLOCK_SIZE_CHIRHO,
        f_flags_chirho: 0,
        f_spare_chirho: [0; 4],
    };

    unsafe {
        core::ptr::write(buf_chirho, sf_chirho);
    }
    0
}

/// `sendfile(2)` implementation.
///
/// Copies data from in_fd to out_fd in kernel space (no user-space bounce).
/// Used by wget, cp, and other file transfer utilities.
fn sys_sendfile_chirho(
    out_fd_chirho: u64,
    in_fd_chirho: u64,
    _offset_ptr_chirho: u64,
    count_chirho: usize,
) -> i64 {
    // Read up to 4KB at a time from in_fd, write to out_fd.
    let chunk_size_chirho = core::cmp::min(count_chirho, 4096);
    let mut buf_chirho = alloc::vec![0u8; chunk_size_chirho];
    let mut total_chirho: usize = 0;

    while total_chirho < count_chirho {
        let to_read_chirho = core::cmp::min(chunk_size_chirho, count_chirho - total_chirho);
        let n_chirho = crate::fs_chirho::sys_read_real_chirho(
            in_fd_chirho,
            buf_chirho.as_mut_ptr() as u64,
            to_read_chirho,
        );
        if n_chirho <= 0 {
            break;
        }
        let written_chirho = if out_fd_chirho == 1 || out_fd_chirho == 2 {
            sys_write_chirho(out_fd_chirho, buf_chirho.as_ptr(), n_chirho as usize)
        } else {
            crate::fs_chirho::sys_write_real_chirho(
                out_fd_chirho,
                buf_chirho.as_ptr() as u64,
                n_chirho as usize,
            )
        };
        if written_chirho < 0 {
            break;
        }
        total_chirho += written_chirho as usize;
    }

    total_chirho as i64
}

/// `sched_getaffinity(2)` implementation.
///
/// Writes a 1-bit CPU affinity mask (single CPU) to user buffer.
fn sys_sched_getaffinity_chirho(
    _pid_chirho: u64,
    cpusetsize_chirho: u32,
    mask_chirho: *mut u8,
) -> i64 {
    if mask_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }
    if cpusetsize_chirho == 0 {
        return -EINVAL_CHIRHO;
    }

    // Write a single-CPU mask: bit 0 set, rest zero.
    // SAFETY: Caller guarantees mask_chirho is writable for cpusetsize_chirho bytes.
    unsafe {
        // Zero the whole buffer first
        core::ptr::write_bytes(mask_chirho, 0, cpusetsize_chirho as usize);
        // Set bit 0 (CPU 0)
        *mask_chirho = 1;
    }

    // Linux returns the size of cpumask_t that was copied.
    // Typically 8 bytes on x86_64.
    let ret_size_chirho = if cpusetsize_chirho < 8 { cpusetsize_chirho } else { 8 };
    ret_size_chirho as i64
}

/// `prctl(2)` implementation.
///
/// Handles PR_SET_NAME (set task comm), PR_GET_NAME (get task comm).
/// Returns 0 for all other sub-commands.
fn sys_prctl_chirho(
    option_chirho: u64,
    arg2_chirho: u64,
    _arg3_chirho: u64,
    _arg4_chirho: u64,
    _arg5_chirho: u64,
) -> i64 {
    match option_chirho {
        PR_SET_NAME_CHIRHO => {
            // arg2 points to a NUL-terminated string (up to 16 bytes including NUL)
            if arg2_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            // Read up to 15 bytes + NUL from user space
            let name_bytes_chirho = unsafe {
                let mut buf_chirho = [0u8; 16];
                for i_chirho in 0..15usize {
                    let byte_chirho = *(arg2_chirho as *const u8).add(i_chirho);
                    if byte_chirho == 0 {
                        break;
                    }
                    buf_chirho[i_chirho] = byte_chirho;
                }
                buf_chirho
            };

            // Set the current task's comm field
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                let mut task_chirho = task_arc_chirho.lock();
                task_chirho.comm_chirho = name_bytes_chirho;
            }
            0
        }
        PR_GET_NAME_CHIRHO => {
            // arg2 points to a buffer of at least 16 bytes
            if arg2_chirho == 0 {
                return -EFAULT_CHIRHO;
            }
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                let task_chirho = task_arc_chirho.lock();
                // SAFETY: Caller guarantees arg2 is writable for 16 bytes.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        task_chirho.comm_chirho.as_ptr(),
                        arg2_chirho as *mut u8,
                        16,
                    );
                }
            }
            0
        }
        _ => {
            // All other prctl options: silently succeed
            0
        }
    }
}

// ============================================================================
// Phase 8+9 syscall implementations
// ============================================================================

// pread64, pwrite64, readv implementations moved to the main syscall
// implementation section above (near sys_writev_chirho) with proper
// offset handling and VFS integration for musl compat.

/// `sched_getparam(2)` -- write zeroed sched_param to user buf.
///
/// The sched_param struct contains a single i32 field (sched_priority).
/// We write 0 (SCHED_NORMAL priority).
fn sys_sched_getparam_chirho(param_chirho: *mut u8) -> i64 {
    if param_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }
    // struct sched_param { int sched_priority; } => 4 bytes, value 0
    unsafe {
        core::ptr::write_bytes(param_chirho, 0, 4);
    }
    0
}

/// `gettimeofday(2)` implementation (A2-AUDIT-002).
///
/// Writes the current real time to a TimevalChirho struct in user space.
/// Uses BOOT_EPOCH_CHIRHO + monotonic offset from tick counter.
fn sys_gettimeofday_chirho(tv_chirho: *mut TimevalChirho) -> i64 {
    if tv_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    let (mono_sec_chirho, mono_nsec_chirho) = monotonic_from_ticks_chirho();
    let timeval_chirho = TimevalChirho {
        tv_sec_chirho: BOOT_EPOCH_CHIRHO + mono_sec_chirho,
        tv_usec_chirho: mono_nsec_chirho / 1_000, // ns to us
    };

    unsafe {
        core::ptr::write(tv_chirho, timeval_chirho);
    }
    0
}

/// `reboot(2)` -- print REBOOT message and halt.
fn sys_reboot_chirho() -> i64 {
    crate::serial_println_chirho!("[SYSCALL] REBOOT requested -- halting system");
    loop {
        x86_64::instructions::hlt();
    }
}

/// `getrusage(2)` -- write zeroed RusageChirho to user buf.
fn sys_getrusage_chirho(usage_chirho: *mut RusageChirho) -> i64 {
    if usage_chirho.is_null() {
        return -EFAULT_CHIRHO;
    }

    // Zero-fill the entire struct
    unsafe {
        core::ptr::write_bytes(
            usage_chirho as *mut u8,
            0,
            core::mem::size_of::<RusageChirho>(),
        );
    }
    0
}

/// `personality(2)` implementation.
///
/// Returns 0 (PER_LINUX) for all calls. Ignores the persona argument.
fn sys_personality_chirho(persona_chirho: u64) -> i64 {
    // 0xffffffff means "query current personality"
    if persona_chirho == 0xffffffff {
        return 0; // PER_LINUX
    }
    // Accept any persona, return previous (always 0 = PER_LINUX)
    0
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
        SYS_GETSOCKNAME_CHIRHO => "getsockname",
        SYS_GETPEERNAME_CHIRHO => "getpeername",
        SYS_SOCKETPAIR_CHIRHO => "socketpair",
        SYS_SETSOCKOPT_CHIRHO => "setsockopt",
        SYS_GETSOCKOPT_CHIRHO => "getsockopt",
        SYS_SENDMSG_CHIRHO => "sendmsg",
        SYS_RECVMSG_CHIRHO => "recvmsg",
        SYS_ACCEPT4_CHIRHO => "accept4",
        SYS_CAPGET_CHIRHO => "capget",
        SYS_CAPSET_CHIRHO => "capset",
        SYS_UNSHARE_CHIRHO => "unshare",
        SYS_SETNS_CHIRHO => "setns",
        SYS_SECCOMP_CHIRHO => "seccomp",
        SYS_BPF_CHIRHO => "bpf",
        SYS_LANDLOCK_CREATE_RULESET_CHIRHO => "landlock_create_ruleset",
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
        SYS_SYMLINK_CHIRHO => "symlink",
        SYS_READLINK_CHIRHO => "readlink",
        SYS_CHMOD_CHIRHO => "chmod",
        SYS_CHOWN_CHIRHO => "chown",
        SYS_GETUID_CHIRHO => "getuid",
        SYS_GETGID_CHIRHO => "getgid",
        SYS_GETEUID_CHIRHO => "geteuid",
        SYS_GETEGID_CHIRHO => "getegid",
        SYS_GETPPID_CHIRHO => "getppid",
        SYS_SETUID_CHIRHO => "setuid",
        SYS_SETGID_CHIRHO => "setgid",
        SYS_SETPGID_CHIRHO => "setpgid",
        SYS_GETPGRP_CHIRHO => "getpgrp",
        SYS_SETSID_CHIRHO => "setsid",
        SYS_SETREUID_CHIRHO => "setreuid",
        SYS_SETREGID_CHIRHO => "setregid",
        SYS_GETGROUPS_CHIRHO => "getgroups",
        SYS_SETGROUPS_CHIRHO => "setgroups",
        SYS_SETRESUID_CHIRHO => "setresuid",
        SYS_GETRESUID_CHIRHO => "getresuid",
        SYS_SETRESGID_CHIRHO => "setresgid",
        SYS_GETRESGID_CHIRHO => "getresgid",
        SYS_GETPGID_CHIRHO => "getpgid",
        SYS_GETSID_CHIRHO => "getsid",
        SYS_RT_SIGPENDING_CHIRHO => "rt_sigpending",
        SYS_RT_SIGSUSPEND_CHIRHO => "rt_sigsuspend",
        SYS_SIGALTSTACK_CHIRHO => "sigaltstack",
        SYS_TKILL_CHIRHO => "tkill",
        SYS_TGKILL_CHIRHO => "tgkill",
        SYS_TIMERFD_SETTIME_CHIRHO => "timerfd_settime",
        SYS_TIMERFD_GETTIME_CHIRHO => "timerfd_gettime",
        SYS_EVENTFD_CHIRHO => "eventfd",
        SYS_ARCH_PRCTL_CHIRHO => "arch_prctl",
        SYS_GETTID_CHIRHO => "gettid",
        SYS_FUTEX_CHIRHO => "futex",
        SYS_SET_TID_ADDRESS_CHIRHO => "set_tid_address",
        SYS_CLOCK_GETTIME_CHIRHO => "clock_gettime",
        SYS_EXIT_GROUP_CHIRHO => "exit_group",
        SYS_OPENAT_CHIRHO => "openat",
        SYS_MKDIRAT_CHIRHO => "mkdirat",
        SYS_NEWFSTATAT_CHIRHO => "newfstatat",
        SYS_UNLINKAT_CHIRHO => "unlinkat",
        SYS_SET_ROBUST_LIST_CHIRHO => "set_robust_list",
        SYS_GET_ROBUST_LIST_CHIRHO => "get_robust_list",
        SYS_PRLIMIT64_CHIRHO => "prlimit64",
        SYS_GETDENTS64_CHIRHO => "getdents64",
        SYS_READLINKAT_CHIRHO => "readlinkat",
        SYS_FACCESSAT_CHIRHO => "faccessat",
        SYS_GETRANDOM_CHIRHO => "getrandom",
        SYS_RENAMEAT2_CHIRHO => "renameat2",
        SYS_STATX_CHIRHO => "statx",
        SYS_PIPE2_CHIRHO => "pipe2",
        SYS_RSEQ_CHIRHO => "rseq",
        SYS_MOUNT_CHIRHO => "mount",
        SYS_UMOUNT2_CHIRHO => "umount2",
        SYS_EPOLL_WAIT_CHIRHO => "epoll_wait",
        SYS_EPOLL_CTL_CHIRHO => "epoll_ctl",
        SYS_PSELECT6_CHIRHO => "pselect6",
        SYS_PPOLL_CHIRHO => "ppoll",
        SYS_EPOLL_PWAIT_CHIRHO => "epoll_pwait",
        SYS_EPOLL_CREATE1_CHIRHO => "epoll_create1",
        SYS_STATFS_CHIRHO => "statfs",
        SYS_FSTATFS_CHIRHO => "fstatfs",
        SYS_SYSINFO_CHIRHO => "sysinfo",
        SYS_MKNOD_CHIRHO => "mknod",
        SYS_PERSONALITY_CHIRHO => "personality",
        SYS_PRCTL_CHIRHO => "prctl",
        SYS_SCHED_SETAFFINITY_CHIRHO => "sched_setaffinity",
        SYS_SCHED_GETAFFINITY_CHIRHO => "sched_getaffinity",
        SYS_CLOCK_NANOSLEEP_CHIRHO => "clock_nanosleep",
        SYS_MKNODAT_CHIRHO => "mknodat",
        SYS_TIMERFD_CREATE_CHIRHO => "timerfd_create",
        SYS_SIGNALFD4_CHIRHO => "signalfd4",
        SYS_EVENTFD2_CHIRHO => "eventfd2",
        SYS_DUP3_CHIRHO => "dup3",
        SYS_MEMFD_CREATE_CHIRHO => "memfd_create",
        // Phase 8+9 additions
        SYS_SENDFILE_CHIRHO => "sendfile",
        SYS_FDATASYNC_CHIRHO => "fdatasync",
        SYS_GETTIMEOFDAY_CHIRHO => "gettimeofday",
        SYS_GETRUSAGE_CHIRHO => "getrusage",
        SYS_TIMES_CHIRHO => "times",
        SYS_PTRACE_CHIRHO => "ptrace",
        SYS_SYSLOG_CHIRHO => "syslog",
        SYS_GETPRIORITY_CHIRHO => "getpriority",
        SYS_SETPRIORITY_CHIRHO => "setpriority",
        SYS_SCHED_SETPARAM_CHIRHO => "sched_setparam",
        SYS_SCHED_GETPARAM_CHIRHO => "sched_getparam",
        SYS_SCHED_SETSCHEDULER_CHIRHO => "sched_setscheduler",
        SYS_SCHED_GETSCHEDULER_CHIRHO => "sched_getscheduler",
        SYS_SCHED_GET_PRIORITY_MAX_CHIRHO => "sched_get_priority_max",
        SYS_SCHED_GET_PRIORITY_MIN_CHIRHO => "sched_get_priority_min",
        SYS_MLOCK_CHIRHO => "mlock",
        SYS_MUNLOCK_CHIRHO => "munlock",
        SYS_MLOCKALL_CHIRHO => "mlockall",
        SYS_MUNLOCKALL_CHIRHO => "munlockall",
        SYS_SYNC_CHIRHO => "sync",
        SYS_SETTIMEOFDAY_CHIRHO => "settimeofday",
        SYS_REBOOT_CHIRHO => "reboot",
        SYS_SETHOSTNAME_CHIRHO => "sethostname",
        SYS_SETXATTR_CHIRHO => "setxattr",
        SYS_GETXATTR_CHIRHO => "getxattr",
        SYS_LISTXATTR_CHIRHO => "listxattr",
        SYS_REMOVEXATTR_CHIRHO => "removexattr",
        SYS_FADVISE64_CHIRHO => "fadvise64",
        SYS_TIMER_CREATE_CHIRHO => "timer_create",
        SYS_TIMER_SETTIME_CHIRHO => "timer_settime",
        SYS_TIMER_GETTIME_CHIRHO => "timer_gettime",
        SYS_TIMER_DELETE_CHIRHO => "timer_delete",
        SYS_WAITID_CHIRHO => "waitid",
        SYS_SPLICE_CHIRHO => "splice",
        SYS_TEE_CHIRHO => "tee",
        SYS_VMSPLICE_CHIRHO => "vmsplice",
        SYS_FALLOCATE_CHIRHO => "fallocate",
        SYS_EXECVEAT_CHIRHO => "execveat",
        SYS_MLOCK2_CHIRHO => "mlock2",
        SYS_COPY_FILE_RANGE_CHIRHO => "copy_file_range",
        SYS_IO_URING_SETUP_CHIRHO => "io_uring_setup",
        SYS_IO_URING_ENTER_CHIRHO => "io_uring_enter",
        SYS_IO_URING_REGISTER_CHIRHO => "io_uring_register",
        SYS_CLONE3_CHIRHO => "clone3",
        // Phase 10: Massive syscall coverage
        SYS_FCHMOD_CHIRHO => "fchmod",
        SYS_FCHOWN_CHIRHO => "fchown",
        SYS_LCHOWN_CHIRHO => "lchown",
        SYS_UMASK_CHIRHO => "umask",
        SYS_GETRLIMIT_CHIRHO => "getrlimit",
        SYS_LSETXATTR_CHIRHO => "lsetxattr",
        SYS_FSETXATTR_CHIRHO => "fsetxattr",
        SYS_LGETXATTR_CHIRHO => "lgetxattr",
        SYS_FGETXATTR_CHIRHO => "fgetxattr",
        SYS_LLISTXATTR_CHIRHO => "llistxattr",
        SYS_FLISTXATTR_CHIRHO => "flistxattr",
        SYS_LREMOVEXATTR_CHIRHO => "lremovexattr",
        SYS_FREMOVEXATTR_CHIRHO => "fremovexattr",
        SYS_IOPRIO_SET_CHIRHO => "ioprio_set",
        SYS_IOPRIO_GET_CHIRHO => "ioprio_get",
        SYS_INOTIFY_INIT1_CHIRHO => "inotify_init1",
        SYS_INOTIFY_ADD_WATCH_CHIRHO => "inotify_add_watch",
        SYS_INOTIFY_RM_WATCH_CHIRHO => "inotify_rm_watch",
        SYS_FCHOWNAT_CHIRHO => "fchownat",
        SYS_FCHMODAT_CHIRHO => "fchmodat",
        SYS_LINKAT_CHIRHO => "linkat",
        SYS_SYMLINKAT_CHIRHO => "symlinkat",
        SYS_SYNC_FILE_RANGE_CHIRHO => "sync_file_range",
        SYS_UTIMENSAT_CHIRHO => "utimensat",
        SYS_FANOTIFY_INIT_CHIRHO => "fanotify_init",
        SYS_FANOTIFY_MARK_CHIRHO => "fanotify_mark",
        SYS_NAME_TO_HANDLE_AT_CHIRHO => "name_to_handle_at",
        SYS_OPEN_BY_HANDLE_AT_CHIRHO => "open_by_handle_at",
        _ => "unknown",
    }
}

/// Direct stdin read via serial port polling.
/// Bypasses VFS entirely — no heap allocation, no locks, minimal stack.
fn sys_read_stdin_chirho(buf_addr_chirho: u64, count_chirho: usize) -> i64 {
    if count_chirho == 0 || buf_addr_chirho == 0 {
        return 0;
    }


    // Enable interrupts so timer ticks keep running
    x86_64::instructions::interrupts::enable();

    // Poll serial port AND PS/2 keyboard buffer, return 1 byte at a time
    loop {
        // Check PS/2 keyboard input buffer first (lock-free, from QEMU window)
        if let Some(byte_chirho) = crate::fbconsole_chirho::KB_INPUT_CHIRHO.pop_chirho() {
            let ch_chirho = if byte_chirho == b'\r' { b'\n' } else { byte_chirho };
            let src_chirho = [ch_chirho];
            if crate::uaccess_chirho::copy_to_user_chirho(buf_addr_chirho, &src_chirho, 1).is_err() {
                return -14; // EFAULT
            }
            return 1;
        }
        // Check serial port (from terminal)
        let status_chirho: u8 = unsafe {
            x86_64::instructions::port::Port::<u8>::new(0x3FD).read()
        };
        if status_chirho & 0x01 != 0 {
            let byte_chirho: u8 = unsafe {
                x86_64::instructions::port::Port::<u8>::new(0x3F8).read()
            };
            let ch_chirho = if byte_chirho == b'\r' { b'\n' } else { byte_chirho };
            // Echo to framebuffer too
            if let Some(mut fb_chirho) = crate::fbconsole_chirho::FB_CONSOLE_CHIRHO.try_lock() {
                fb_chirho.write_byte_chirho(ch_chirho);
            }
            let src_chirho = [ch_chirho];
            if crate::uaccess_chirho::copy_to_user_chirho(buf_addr_chirho, &src_chirho, 1).is_err() {
                return -14; // EFAULT
            }
            return 1;
        }
        // Yield to scheduler so other tasks (e.g., dropbear daemon) can run
        // while we wait for keyboard input.
        crate::scheduler_chirho::yield_current_chirho();
    }
}

// ============================================================================
// Phase A1 batch: ppoll, clock_nanosleep, faccessat, getdents,
//                  fchmodat implementations
// ============================================================================

/// `ppoll(2)` implementation.
///
/// Like poll but accepts a timespec pointer and a signal mask.
/// Our implementation ignores the sigmask (signals not fully wired yet)
/// and converts the timespec to a millisecond timeout for the poll stub.
fn sys_ppoll_chirho(
    fds_ptr_chirho: u64,
    nfds_chirho: u32,
    tmo_ptr_chirho: u64,
    _sigmask_ptr_chirho: u64,
    _sigsetsize_chirho: u64,
) -> i64 {
    // Convert timespec to a timeout in ms (or -1 for infinite).
    // If tmo_ptr is NULL, timeout is infinite (blocking).
    let timeout_ms_chirho: i32 = if tmo_ptr_chirho == 0 {
        -1 // infinite
    } else {
        // Read struct timespec { i64 tv_sec; i64 tv_nsec; } from user
        let mut ts_buf_chirho = [0u8; 16];
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut ts_buf_chirho,
            tmo_ptr_chirho,
            16,
        ).is_err() {
            return -EFAULT_CHIRHO;
        }
        let sec_bytes_chirho = match <[u8; 8]>::try_from(&ts_buf_chirho[0..8]) {
            Ok(bytes_chirho) => bytes_chirho,
            Err(_) => return -EFAULT_CHIRHO,
        };
        let nsec_bytes_chirho = match <[u8; 8]>::try_from(&ts_buf_chirho[8..16]) {
            Ok(bytes_chirho) => bytes_chirho,
            Err(_) => return -EFAULT_CHIRHO,
        };
        let sec_chirho = i64::from_ne_bytes(sec_bytes_chirho);
        let nsec_chirho = i64::from_ne_bytes(nsec_bytes_chirho);
        // Convert to milliseconds, cap at i32::MAX
        let ms_chirho = sec_chirho.saturating_mul(1000)
            .saturating_add(nsec_chirho / 1_000_000);
        if ms_chirho > i32::MAX as i64 { i32::MAX } else { ms_chirho as i32 }
    };

    // Delegate to the existing poll implementation
    sys_poll_chirho(fds_ptr_chirho, nfds_chirho, timeout_ms_chirho)
}

/// `clock_nanosleep(2)` implementation.
///
/// Supports CLOCK_REALTIME (0) and CLOCK_MONOTONIC (1).
/// flag=0 means relative sleep, flag=TIMER_ABSTIME(1) means absolute.
/// For now we do a busy-spin approximation; a real implementation would
/// use the HPET/APIC timer to schedule a wakeup.
fn sys_clock_nanosleep_chirho(
    clock_id_chirho: u32,
    flags_chirho: u32,
    request_ptr_chirho: u64,
    _remain_ptr_chirho: u64,
) -> i64 {
    // Validate clock_id
    if clock_id_chirho > 1 {
        // We support CLOCK_REALTIME (0) and CLOCK_MONOTONIC (1)
        return -EINVAL_CHIRHO;
    }

    if request_ptr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    // Read the timespec from user space
    let mut ts_buf_chirho = [0u8; 16];
    if crate::uaccess_chirho::copy_from_user_chirho(
        &mut ts_buf_chirho,
        request_ptr_chirho,
        16,
    ).is_err() {
        return -EFAULT_CHIRHO;
    }

    let sec_bytes_chirho = match <[u8; 8]>::try_from(&ts_buf_chirho[0..8]) {
        Ok(bytes_chirho) => bytes_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };
    let nsec_bytes_chirho = match <[u8; 8]>::try_from(&ts_buf_chirho[8..16]) {
        Ok(bytes_chirho) => bytes_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };
    let sec_chirho = i64::from_ne_bytes(sec_bytes_chirho);
    let nsec_chirho = i64::from_ne_bytes(nsec_bytes_chirho);

    // Validate
    if nsec_chirho < 0 || nsec_chirho >= 1_000_000_000 {
        return -EINVAL_CHIRHO;
    }

    let _flags_abstime_chirho = flags_chirho & 1; // TIMER_ABSTIME = 1

    // For now, do a lightweight busy-wait using TSC.
    // Convert requested sleep to approximate TSC ticks.
    // Assume ~1 GHz TSC (conservative estimate).
    let total_ns_chirho = (sec_chirho as u64).saturating_mul(1_000_000_000)
        .saturating_add(nsec_chirho as u64);

    // For short sleeps (<1ms), busy-spin with hint.
    // For longer sleeps, yield to scheduler.
    if total_ns_chirho > 0 {
        let start_tsc_chirho = rdtsc_chirho();
        // Rough approximation: 1 billion TSC ticks per second.
        // The actual TSC frequency varies; this is a reasonable default.
        let tsc_per_ns_chirho: u64 = 1; // ~1 GHz
        let target_ticks_chirho = total_ns_chirho.saturating_mul(tsc_per_ns_chirho);

        // Cap the spin to prevent hanging: max 100ms of spinning
        let max_spin_ticks_chirho: u64 = 100_000_000;
        let spin_ticks_chirho = target_ticks_chirho.min(max_spin_ticks_chirho);

        while rdtsc_chirho().wrapping_sub(start_tsc_chirho) < spin_ticks_chirho {
            core::hint::spin_loop();
        }

        // For remaining time beyond spin limit, yield to scheduler
        if target_ticks_chirho > max_spin_ticks_chirho {
            crate::scheduler_chirho::yield_current_chirho();
        }
    }

    0 // success
}

/// `faccessat(2)` real implementation.
///
/// Checks if the file at the given path exists and is accessible.
/// Uses the VFS to resolve the path.  Since we run as root, permission
/// checks always pass; the main value is detecting ENOENT.
fn sys_faccessat_real_chirho(
    dirfd_chirho: i32,
    pathname_addr_chirho: u64,
    _mode_chirho: u32,
    _flags_chirho: u32,
) -> i64 {
    if pathname_addr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    // Read path from user space
    let pathname_chirho = match crate::uaccess_chirho::read_user_string_chirho(pathname_addr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Handle relative paths: resolve against dirfd
    let full_path_chirho = if !pathname_chirho.starts_with('/') {
        if dirfd_chirho == -100 { // AT_FDCWD
            let mut p_chirho = alloc::string::String::from("/");
            p_chirho.push_str(&pathname_chirho);
            p_chirho
        } else {
            // Resolve relative to dirfd's path
            match crate::fs_chirho::get_fd_path_chirho(dirfd_chirho as u64) {
                Some(dp_chirho) => {
                    let mut p_chirho = dp_chirho;
                    if !p_chirho.ends_with('/') { p_chirho.push('/'); }
                    p_chirho.push_str(&pathname_chirho);
                    p_chirho
                }
                None => {
                    let mut p_chirho = alloc::string::String::from("/");
                    p_chirho.push_str(&pathname_chirho);
                    p_chirho
                }
            }
        }
    } else {
        pathname_chirho
    };

    // Try to resolve the path through VFS
    match crate::fs_chirho::resolve_path_chirho(&full_path_chirho) {
        Ok(_) => 0, // file exists, access granted (we're root)
        Err(_) => -ENOENT_CHIRHO,
    }
}

/// `getdents(2)` implementation (old-style, syscall nr 78).
///
/// Uses the older `linux_dirent` format (not `linux_dirent64`):
///   - u64 d_ino       (inode number -- we use u64 even in "old" format for x86_64)
///   - i64 d_off       (offset to next dirent)
///   - u16 d_reclen    (length of this record)
///   - char d_name[]   (NUL-terminated filename)
///   - u8 d_type       (file type, appended after d_name + padding)
///
/// On x86_64 Linux, the "old" getdents actually uses the same layout
/// as getdents64 for compat. We implement the x86_64 ABI.
fn sys_getdents_chirho(
    fd_chirho: u64,
    dirp_chirho: *mut u8,
    count_chirho: usize,
) -> i64 {
    use crate::uaccess_chirho::copy_to_user_chirho;

    if dirp_chirho.is_null() || count_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    // A2-PROC-003: Use lookup_fd_chirho (per-process first, then global).
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Collect directory entries -- use the same linux_dirent64 layout
    // since on x86_64, getdents and getdents64 have identical struct layout.
    // Cap at 64KB to prevent userspace from causing huge kernel allocations.
    let capped_count_chirho = core::cmp::min(count_chirho, 64 * 1024);
    let mut kernel_buf_chirho = alloc::vec![0u8; capped_count_chirho];
    let mut bytes_written_chirho: usize = 0;
    let mut error_chirho: Option<i64> = None;

    {
        let mut file_guard_chirho = file_arc_chirho.lock();
        let readdir_result_chirho = file_guard_chirho.ops_chirho.readdir_chirho(
            &mut file_guard_chirho,
            &mut |name_chirho: &str, ino_chirho: u64, d_type_chirho: u8| -> bool {
                // Old linux_dirent on x86_64 layout:
                //   u64 d_ino     (8)
                //   u64 d_off     (8)
                //   u16 d_reclen  (2)
                //   char d_name[] (variable, NUL-terminated)
                //   u8 d_type follows after name+padding
                let name_bytes_chirho = name_chirho.as_bytes();
                let name_len_chirho = name_bytes_chirho.len() + 1; // +1 NUL
                // Header: 8 + 8 + 2 = 18, then name, then 1 byte d_type, align to 8
                let reclen_unaligned_chirho: usize = 18 + name_len_chirho + 1; // +1 for d_type
                let reclen_chirho = (reclen_unaligned_chirho + 7) & !7;

                if bytes_written_chirho + reclen_chirho > count_chirho {
                    return false;
                }

                let off_chirho = bytes_written_chirho;
                let d_off_chirho = (bytes_written_chirho + reclen_chirho) as u64;

                // d_ino
                kernel_buf_chirho[off_chirho..off_chirho + 8]
                    .copy_from_slice(&ino_chirho.to_ne_bytes());
                // d_off
                kernel_buf_chirho[off_chirho + 8..off_chirho + 16]
                    .copy_from_slice(&d_off_chirho.to_ne_bytes());
                // d_reclen
                kernel_buf_chirho[off_chirho + 16..off_chirho + 18]
                    .copy_from_slice(&(reclen_chirho as u16).to_ne_bytes());
                // d_name
                kernel_buf_chirho[off_chirho + 18..off_chirho + 18 + name_bytes_chirho.len()]
                    .copy_from_slice(name_bytes_chirho);
                kernel_buf_chirho[off_chirho + 18 + name_bytes_chirho.len()] = 0; // NUL
                // d_type at end (last byte before padding)
                kernel_buf_chirho[off_chirho + reclen_chirho - 1] = d_type_chirho;
                // Zero padding
                for i_chirho in (off_chirho + 18 + name_len_chirho)..(off_chirho + reclen_chirho - 1) {
                    kernel_buf_chirho[i_chirho] = 0;
                }

                bytes_written_chirho += reclen_chirho;
                true
            },
        );

        if let Err(errno_chirho) = readdir_result_chirho {
            error_chirho = Some(errno_chirho);
        }
    }

    if let Some(errno_chirho) = error_chirho {
        return errno_chirho;
    }

    if bytes_written_chirho > 0 {
        if copy_to_user_chirho(
            dirp_chirho as u64,
            &kernel_buf_chirho[..bytes_written_chirho],
            bytes_written_chirho,
        ).is_err() {
            return -EFAULT_CHIRHO;
        }
    }

    bytes_written_chirho as i64
}

/// `fchmodat(2)` implementation.
///
/// Changes the mode of the file at the given path relative to dirfd.
/// Since we're always root and most filesystems are tmpfs, we update
/// the inode's mode field directly.
fn sys_fchmodat_chirho(
    dirfd_chirho: i32,
    pathname_addr_chirho: u64,
    mode_chirho: u32,
    _flags_chirho: u32,
) -> i64 {
    if pathname_addr_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    // Read path from user space
    let pathname_chirho = match crate::uaccess_chirho::read_user_string_chirho(pathname_addr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Handle AT_FDCWD
    let full_path_chirho = if !pathname_chirho.starts_with('/') {
        if dirfd_chirho == -100 {
            let mut p_chirho = alloc::string::String::from("/");
            p_chirho.push_str(&pathname_chirho);
            p_chirho
        } else {
            pathname_chirho
        }
    } else {
        pathname_chirho
    };

    // Resolve the path
    match crate::fs_chirho::resolve_path_chirho(&full_path_chirho) {
        Ok((inode_arc_chirho, _)) => {
            // Update the mode bits (preserve file type, update permission bits)
            let mut inode_chirho = inode_arc_chirho.lock();
            let file_type_chirho = inode_chirho.mode_chirho & 0o170000; // S_IFMT
            inode_chirho.mode_chirho = file_type_chirho | (mode_chirho & 0o7777);
            0
        }
        Err(errno_chirho) => errno_chirho,
    }
}
