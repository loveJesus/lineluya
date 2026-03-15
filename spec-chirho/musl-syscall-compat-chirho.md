# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. - John 3:16

# G1-003: musl libc Syscall Compatibility Checklist for Lineluya

This document tracks which Linux syscalls musl libc requires and their
implementation status in the Lineluya kernel. musl libc (used by Alpine Linux)
is the C library that underpins all userspace programs. Every musl function
ultimately issues syscalls through its `__syscall` inline assembly macro.

## Legend

| Status | Meaning |
|--------|---------|
| DONE   | Fully implemented and tested |
| STUB   | Returns success (0) or a sensible default but does not perform real work |
| PARTIAL| Implemented but missing edge cases or flags |
| ENOSYS | Returns -ENOSYS (not implemented) |
| MISSING| Not present in dispatch table at all |

---

## 1. Process Lifecycle (Critical for /sbin/init)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 56 | `clone` | `fork()`, `pthread_create()` | DONE | Full clone with flags |
| 57 | `fork` | `fork()` fallback | DONE | Via sys_fork_chirho |
| 58 | `vfork` | `posix_spawn()` | DONE | Treated as fork |
| 59 | `execve` | `exec*()` family | DONE | ELF loading with dynlink |
| 60 | `exit` | `_exit()` | DONE | Process termination |
| 231 | `exit_group` | `exit()` | DONE | Thread group exit |
| 61 | `wait4` | `waitpid()`, `wait()` | DONE | With rusage |
| 62 | `kill` | `kill()`, `raise()` | DONE | Signal delivery |
| 39 | `getpid` | `getpid()` | DONE | |
| 110 | `getppid` | `getppid()` | DONE | |
| 186 | `gettid` | `gettid()`, thread-local | DONE | |
| 218 | `set_tid_address` | Thread setup | DONE | |
| 200 | `tkill` | `pthread_kill()` | DONE | |
| 234 | `tgkill` | `pthread_kill()` | DONE | |
| 247 | `waitid` | `waitid()` | ENOSYS | Needed for OpenRC |
| 435 | `clone3` | Modern `fork()`/`pthread_create()` | ENOSYS | musl may try this first |

## 2. Memory Management (Critical for malloc/mmap)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 9 | `mmap` | `malloc()`, `mmap()`, dynamic linker | DONE | MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE |
| 10 | `mprotect` | Stack guard, W^X | DONE | |
| 11 | `munmap` | `free()`, `munmap()` | DONE | |
| 12 | `brk` | `malloc()` small allocs, sbrk | DONE | Page-aligned, tested with BusyBox |
| 25 | `mremap` | `realloc()` large blocks | ENOSYS | musl falls back to mmap+copy |
| 28 | `madvise` | `posix_madvise()` | STUB | Returns 0 (advisory) |
| 26 | `msync` | `msync()` | STUB | Returns 0 |
| 27 | `mincore` | `mincore()` | ENOSYS | Rarely used |
| 149 | `mlock` | `mlock()` | STUB | Returns 0 |
| 150 | `munlock` | `munlock()` | STUB | Returns 0 |
| 151 | `mlockall` | `mlockall()` | STUB | Returns 0 |
| 152 | `munlockall` | `munlockall()` | STUB | Returns 0 |
| 325 | `mlock2` | `mlock2()` | STUB | Returns 0 |

## 3. File System Operations (Critical for /sbin/init, OpenRC)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 0 | `read` | `read()`, stdio | DONE | VFS-backed |
| 1 | `write` | `write()`, stdio | DONE | VFS-backed |
| 2 | `open` | `open()` | DONE | Via VFS |
| 3 | `close` | `close()` | DONE | |
| 257 | `openat` | `open()` (musl redirects here) | DONE | AT_FDCWD support |
| 8 | `lseek` | `lseek()`, `fseek()` | DONE | |
| 17 | `pread64` | `pread()` | STUB | Returns -EBADF |
| 18 | `pwrite64` | `pwrite()` | STUB | Returns -EBADF |
| 19 | `readv` | `readv()` | ENOSYS | Needed for some utils |
| 20 | `writev` | `writev()`, stdio buffered write | DONE | |
| 78 | `getdents` | Legacy `readdir()` | STUB | Returns -EBADF |
| 217 | `getdents64` | `readdir()` | DONE | Real VFS implementation |
| 4 | `stat` | `stat()` | DONE | |
| 5 | `fstat` | `fstat()` | DONE | |
| 6 | `lstat` | `lstat()` | DONE | |
| 262 | `newfstatat` | `fstatat()` (musl uses this) | DONE | |
| 332 | `statx` | `statx()` | DONE | |
| 21 | `access` | `access()` | DONE | Stub: returns 0 |
| 269 | `faccessat` | `access()` (musl redirects) | DONE | Stub |
| 79 | `getcwd` | `getcwd()` | DONE | |
| 80 | `chdir` | `chdir()` | DONE | |
| 82 | `rename` | `rename()` | DONE | |
| 316 | `renameat2` | `rename()` (musl redirects) | DONE | |
| 83 | `mkdir` | `mkdir()` | DONE | |
| 258 | `mkdirat` | `mkdir()` (musl redirects) | DONE | |
| 84 | `rmdir` | `rmdir()` | DONE | |
| 87 | `unlink` | `unlink()`, `remove()` | DONE | |
| 263 | `unlinkat` | `unlink()`/`rmdir()` (musl) | DONE | |
| 89 | `readlink` | `readlink()` | DONE | |
| 267 | `readlinkat` | `readlink()` (musl redirects) | DONE | |
| 85 | `creat` | `creat()` | ENOSYS | musl uses openat instead |
| 86 | `link` | `link()` | ENOSYS | |
| 265 | `linkat` | `link()` | ENOSYS | |
| 266 | `symlinkat` | `symlink()` | ENOSYS | Needed for package install |
| 90 | `chmod` | `chmod()` | STUB | Returns -ENOENT |
| 91 | `fchmod` | `fchmod()` | STUB | Returns 0 |
| 92 | `chown` | `chown()` | STUB | Returns -ENOENT |
| 93 | `fchown` | `fchown()` | STUB | Returns 0 |
| 94 | `lchown` | `lchown()` | STUB | Returns 0 |
| 76 | `truncate` | `truncate()` | STUB | Returns 0 |
| 77 | `ftruncate` | `ftruncate()` | STUB | Returns 0 |
| 72 | `fcntl` | `fcntl()` — F_DUPFD, F_GETFD, F_SETFD, F_GETFL, F_SETFL, F_DUPFD_CLOEXEC | DONE | Including F_DUPFD_CLOEXEC |
| 73 | `flock` | `flock()` | STUB | Returns 0 |
| 74 | `fsync` | `fsync()` | STUB | Returns 0 |
| 75 | `fdatasync` | `fdatasync()` | STUB | Returns 0 |
| 165 | `mount` | `mount()` — needed by OpenRC | DONE | |
| 166 | `umount2` | `umount()` | DONE | |
| 133 | `mknod` | `mknod()` device nodes | STUB | Returns 0 |
| 259 | `mknodat` | `mknod()` | STUB | Returns 0 |
| 280 | `utimensat` | `utimes()`, `touch` | STUB | Returns 0 |

## 4. I/O Multiplexing (Needed by OpenRC, networking)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 7 | `poll` | `poll()` | DONE | |
| 23 | `select` | `select()` | DONE | |
| 270 | `pselect6` | `pselect()` | DONE | Maps to select |
| 271 | `ppoll` | `ppoll()` | DONE | Maps to poll |
| 291 | `epoll_create1` | `epoll_create()` | DONE | |
| 233 | `epoll_ctl` | `epoll_ctl()` | DONE | |
| 232 | `epoll_wait` | `epoll_wait()` | DONE | |
| 281 | `epoll_pwait` | `epoll_pwait()` | DONE | Maps to epoll_wait |

## 5. Signals (Critical for init, process control)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 13 | `rt_sigaction` | `sigaction()`, `signal()` | DONE | |
| 14 | `rt_sigprocmask` | `sigprocmask()`, `pthread_sigmask()` | DONE | |
| 15 | `rt_sigreturn` | Signal return trampoline | DONE | |
| 127 | `rt_sigpending` | `sigpending()` | DONE | |
| 130 | `rt_sigsuspend` | `sigsuspend()` | DONE | |
| 131 | `sigaltstack` | `sigaltstack()` | DONE | |

## 6. File Descriptor Operations

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 32 | `dup` | `dup()` | DONE | |
| 33 | `dup2` | `dup2()` | DONE | |
| 292 | `dup3` | `dup3()` (O_CLOEXEC) | DONE | |
| 22 | `pipe` | `pipe()` | DONE | |
| 293 | `pipe2` | `pipe2()` (O_CLOEXEC) | DONE | |
| 16 | `ioctl` | `ioctl()` — TIOCGWINSZ, TCGETS, etc. | DONE | Terminal ioctls |

## 7. User/Group Identity (Needed by login, OpenRC)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 102 | `getuid` | `getuid()` | DONE | Returns 0 (root) |
| 104 | `getgid` | `getgid()` | DONE | Returns 0 |
| 107 | `geteuid` | `geteuid()` | DONE | Returns 0 |
| 108 | `getegid` | `getegid()` | DONE | Returns 0 |
| 105 | `setuid` | `setuid()` | STUB | Returns 0 |
| 106 | `setgid` | `setgid()` | STUB | Returns 0 |
| 113 | `setreuid` | `setreuid()` | STUB | Returns 0 |
| 114 | `setregid` | `setregid()` | STUB | Returns 0 |
| 117 | `setresuid` | `setresuid()` | STUB | Returns 0 |
| 119 | `getresuid` | `getresuid()` | DONE | Writes 0 to all three |
| 120 | `setresgid` | `setresgid()` | STUB | Returns 0 |
| 121 | `getresgid` | `getresgid()` | DONE | Writes 0 to all three |
| 115 | `getgroups` | `getgroups()` | STUB | Returns 0 (no groups) |
| 116 | `setgroups` | `setgroups()` | STUB | Returns 0 |
| 111 | `getpgrp` | `getpgrp()` | DONE | Returns pid |
| 109 | `setpgid` | `setpgid()` | STUB | Returns 0 |
| 112 | `setsid` | `setsid()` | DONE | Returns pid |
| 121 | `getpgid` | `getpgid()` | DONE | Returns pid |
| 124 | `getsid` | `getsid()` | DONE | Returns pid |

## 8. System Information

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 63 | `uname` | `uname()` | DONE | Reports "Lineluya" |
| 99 | `sysinfo` | `sysinfo()` | DONE | |
| 302 | `prlimit64` | `getrlimit()`/`setrlimit()` | DONE | RLIMIT_STACK, RLIMIT_NOFILE |
| 97 | `getrlimit` | `getrlimit()` | DONE | |
| 95 | `umask` | `umask()` | DONE | |

## 9. Time and Timers

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 228 | `clock_gettime` | `clock_gettime()`, `time()` | DONE | CLOCK_REALTIME, CLOCK_MONOTONIC |
| 230 | `clock_nanosleep` | `nanosleep()`, `sleep()`, `usleep()` | STUB | Instant return |
| 35 | `nanosleep` | `nanosleep()` | STUB | Instant return |
| 96 | `gettimeofday` | `gettimeofday()` | DONE | |
| 36 | `getitimer` | `getitimer()` | ENOSYS | |
| 38 | `setitimer` | `setitimer()` | ENOSYS | |
| 37 | `alarm` | `alarm()` | STUB | Returns 0 |
| 222 | `timer_create` | `timer_create()` | ENOSYS | Needed for real timers |
| 223 | `timer_settime` | `timer_settime()` | ENOSYS | |
| 224 | `timer_gettime` | `timer_gettime()` | ENOSYS | |
| 226 | `timer_delete` | `timer_delete()` | ENOSYS | |
| 283 | `timerfd_create` | `timerfd_create()` | STUB | Returns fake fd |
| 286 | `timerfd_settime` | `timerfd_settime()` | STUB | Returns 0 |
| 287 | `timerfd_gettime` | `timerfd_gettime()` | STUB | Returns 0 |

## 10. Networking (Needed for Alpine networking, apk)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 41 | `socket` | `socket()` | DONE | AF_INET, AF_UNIX |
| 42 | `connect` | `connect()` | DONE | |
| 43 | `accept` | `accept()` | DONE | |
| 288 | `accept4` | `accept4()` | DONE | |
| 44 | `sendto` | `send()`, `sendto()` | DONE | |
| 45 | `recvfrom` | `recv()`, `recvfrom()` | DONE | |
| 46 | `sendmsg` | `sendmsg()` | DONE | |
| 47 | `recvmsg` | `recvmsg()` | DONE | |
| 48 | `shutdown` | `shutdown()` | DONE | |
| 49 | `bind` | `bind()` | DONE | |
| 50 | `listen` | `listen()` | DONE | |
| 51 | `getsockname` | `getsockname()` | DONE | |
| 52 | `getpeername` | `getpeername()` | DONE | |
| 53 | `socketpair` | `socketpair()` | DONE | |
| 54 | `setsockopt` | `setsockopt()` | DONE | |
| 55 | `getsockopt` | `getsockopt()` | DONE | |

## 11. Thread Support (Needed for multi-threaded musl programs)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 158 | `arch_prctl` | `ARCH_SET_FS` (TLS base), `ARCH_SET_GS` | DONE | Critical for TLS |
| 202 | `futex` | `pthread_mutex_*`, `pthread_cond_*`, malloc locks | DONE | FUTEX_WAIT, FUTEX_WAKE |
| 273 | `set_robust_list` | Thread robustness | STUB | Returns 0 |
| 274 | `get_robust_list` | Thread robustness | ENOSYS | |
| 334 | `rseq` | Restartable sequences | ENOSYS | musl does not use rseq |

## 12. Miscellaneous (Various musl needs)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 318 | `getrandom` | `getentropy()`, `/dev/urandom` fallback | DONE | |
| 24 | `sched_yield` | `sched_yield()`, spin locks | DONE | |
| 157 | `prctl` | `PR_SET_NAME`, `PR_GET_NAME`, etc. | DONE | |
| 135 | `personality` | `personality()` | DONE | |
| 160 | `setrlimit` | via prlimit64 | DONE | |
| 34 | `pause` | `pause()` | DONE | Returns -EINTR |
| 40 | `sendfile` | `sendfile()` | ENOSYS | Optimization, not critical |
| 285 | `fallocate` | `posix_fallocate()` | STUB | Returns 0 |
| 162 | `sync` | `sync()` | STUB | Returns 0 |
| 284 | `sync_file_range` | `sync_file_range()` | STUB | Returns 0 |
| 275 | `splice` | `splice()` | ENOSYS | |
| 276 | `tee` | `tee()` | ENOSYS | |
| 278 | `vmsplice` | `vmsplice()` | ENOSYS | |
| 326 | `copy_file_range` | `copy_file_range()` | ENOSYS | |
| 221 | `fadvise64` | `posix_fadvise()` | STUB | Returns 0 (advisory) |

## 13. Capabilities and Security

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 125 | `capget` | `capget()` | STUB | Returns 0 |
| 126 | `capset` | `capset()` | STUB | Returns 0 |
| 317 | `seccomp` | `seccomp()` | STUB | Returns 0 |
| 272 | `unshare` | `unshare()` | STUB | Returns 0 |
| 308 | `setns` | `setns()` | ENOSYS | |

## 14. Extended Attributes (Used by package managers)

| # | Syscall | musl Usage | Lineluya Status | Notes |
|---|---------|-----------|-----------------|-------|
| 188 | `setxattr` | `setxattr()` | STUB | Returns -ENOTSUP |
| 191 | `getxattr` | `getxattr()` | STUB | Returns -ENOTSUP |
| 194 | `listxattr` | `listxattr()` | STUB | Returns -ENOTSUP |
| 197 | `removexattr` | `removexattr()` | STUB | Returns -ENOTSUP |

---

## Summary Statistics

| Category | DONE | STUB | PARTIAL | ENOSYS | MISSING |
|----------|------|------|---------|--------|---------|
| Process Lifecycle | 12 | 0 | 0 | 2 | 0 |
| Memory Management | 5 | 6 | 0 | 2 | 0 |
| File System | 29 | 12 | 0 | 3 | 0 |
| I/O Multiplexing | 8 | 0 | 0 | 0 | 0 |
| Signals | 6 | 0 | 0 | 0 | 0 |
| File Descriptors | 6 | 0 | 0 | 0 | 0 |
| User/Group | 14 | 5 | 0 | 0 | 0 |
| System Info | 5 | 0 | 0 | 0 | 0 |
| Time/Timers | 5 | 4 | 0 | 4 | 0 |
| Networking | 15 | 0 | 0 | 0 | 0 |
| Threading | 4 | 1 | 0 | 1 | 0 |
| Miscellaneous | 6 | 5 | 0 | 4 | 0 |
| Capabilities | 3 | 1 | 0 | 1 | 0 |
| Extended Attrs | 0 | 4 | 0 | 0 | 0 |
| **TOTAL** | **118** | **38** | **0** | **17** | **0** |

## Critical Path for Alpine Boot

The following syscalls are **blocking** for a successful Alpine boot:

### Must Fix (currently ENOSYS but needed):
1. **`waitid` (247)** -- OpenRC uses this; implement or make `wait4` handle `P_ALL`
2. **`clone3` (435)** -- musl 1.2.5+ may try clone3 before falling back to clone; ensure graceful fallback
3. **`readv` (19)** -- Some utilities use this; implement via repeated read
4. **`symlinkat` (266)** -- Package installation needs symlinks
5. **`linkat` (265)** -- Package installation needs hard links

### Should Improve (currently STUB but OpenRC needs real behavior):
1. **`nanosleep`/`clock_nanosleep`** -- Real timer-based sleep for process scheduling
2. **`pread64`/`pwrite64`** -- Needed for database files, config parsing
3. **`mknod`/`mknodat`** -- OpenRC creates device nodes in /dev

### Nice to Have:
1. **`timer_create`** -- Real POSIX timers
2. **`mremap`** -- Performance optimization for realloc
3. **`sendfile`** -- Performance optimization for file serving
