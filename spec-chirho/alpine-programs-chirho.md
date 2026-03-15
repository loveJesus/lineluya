<!-- For God so loved the world that he gave his only begotten Son,
     that whoever believes in him should not perish but have eternal life. - John 3:16 -->

# Alpine Programs Syscall Requirements (P5-001 through P5-006)

This document maps the syscalls each Alpine Linux program needs against what
Lineluya currently implements.  "done" means the syscall is dispatched in
`kernel-chirho/src/syscall_chirho.rs` (even if it is a stub that returns 0 or
-ENOSYS for some sub-cases).  "todo" means it is either missing entirely or
returns -ENOSYS unconditionally and must be made functional for the program to
work.

Legend:
- **(done)** -- implemented or stubbed (returns a usable value)
- **(stub-ok)** -- stub that silently succeeds (0); enough for this program
- **(todo)** -- not implemented or returns -ENOSYS; must be completed
- **(partial)** -- dispatch exists but important sub-functionality is missing

---

## P5-001: apk (Alpine Package Manager)

`apk` is dynamically linked against musl.  It downloads packages over HTTPS,
extracts tarballs, and installs files to disk.  It shells out to `tar` and
may fork/exec helper processes.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| fstat              | 5        | **(done)**   |                                                 |
| stat               | 4        | **(done)**   |                                                 |
| lstat              | 6        | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   | MAP_PRIVATE, MAP_ANONYMOUS, MAP_FIXED all needed |
| mprotect           | 10       | **(done)**   |                                                 |
| munmap             | 11       | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| openat             | 257      | **(done)**   |                                                 |
| getdents64         | 217      | **(done)**   |                                                 |
| fcntl              | 72       | **(done)**   | F_DUPFD_CLOEXEC needed -- just landed           |
| ioctl              | 16       | **(done)**   | TIOCGWINSZ at minimum                           |
| fork               | 57       | **(done)**   |                                                 |
| execve             | 59       | **(done)**   |                                                 |
| wait4              | 61       | **(done)**   |                                                 |
| pipe / pipe2       | 22 / 293 | **(done)**   |                                                 |
| dup / dup2 / dup3  | 32/33/292| **(done)**   |                                                 |
| socket             | 41       | **(done)**   | AF_INET, SOCK_STREAM needed                     |
| connect            | 42       | **(done)**   | TCP connect to remote repos                     |
| sendto             | 44       | **(done)**   | DNS queries (UDP)                                |
| recvfrom           | 45       | **(done)**   | DNS responses (UDP)                              |
| sendmsg / recvmsg  | 46 / 47  | **(done)**   |                                                 |
| setsockopt         | 54       | **(done)**   |                                                 |
| getsockopt         | 55       | **(done)**   |                                                 |
| poll / ppoll       | 7 / 271  | **(done)**   | Wait for network I/O                             |
| futex              | 202      | **(done)**   | musl threading                                   |
| rt_sigaction       | 13       | **(done)**   |                                                 |
| rt_sigprocmask     | 14       | **(done)**   |                                                 |
| clock_gettime      | 228      | **(done)**   |                                                 |
| getuid/geteuid     | 102/107  | **(done)**   |                                                 |
| rename/renameat2   | 82/316   | **(done)**   |                                                 |
| unlink/unlinkat    | 87/263   | **(done)**   |                                                 |
| mkdir/mkdirat      | 83/258   | **(done)**   |                                                 |
| chdir              | 80       | **(done)**   |                                                 |
| getcwd             | 79       | **(done)**   |                                                 |
| readlink/readlinkat| 89/267   | **(done)**   |                                                 |
| chmod/fchmod       | 90/91    | **(done)**   | stub-ok                                          |
| chown/fchown       | 92/93    | **(done)**   | stub-ok                                          |
| flock              | 73       | **(stub-ok)**| apk uses advisory locks                          |
| symlinkat          | 266      | **(todo)**   | Package install creates symlinks                 |
| linkat             | 265      | **(todo)**   | Hard links for package files                     |
| pread64            | 17       | **(todo)**   | Currently returns -EBADF; apk reads archives     |
| pwrite64           | 18       | **(todo)**   | Currently returns -EBADF                         |
| sendfile           | 40       | **(todo)**   | Returns -ENOSYS; file copy optimization          |

### Blockers for apk

1. **Networking must actually work** -- socket/connect/sendto/recvfrom are dispatched but the
   TCP/IP stack must handle real traffic (DNS resolution, HTTPS via TLS).
2. **pread64 / pwrite64** must work for real fds (archive extraction).
3. **symlinkat / linkat** needed for package installation.

---

## P5-002: sqlite3

`sqlite3` is a single static binary.  It uses mmap for database I/O, file
locking for concurrency, and fcntl for journal mode.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| openat             | 257      | **(done)**   |                                                 |
| fstat              | 5        | **(done)**   |                                                 |
| stat               | 4        | **(done)**   |                                                 |
| lstat              | 6        | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   | MAP_SHARED needed for WAL mode                   |
| munmap             | 11       | **(done)**   |                                                 |
| mprotect           | 10       | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| lseek              | 8        | **(done)**   |                                                 |
| fcntl              | 72       | **(done)**   | F_SETLK, F_GETLK, F_RDLCK, F_WRLCK needed      |
| flock              | 73       | **(stub-ok)**| Advisory lock                                    |
| ftruncate          | 77       | **(stub-ok)**| Journal truncation                               |
| fsync / fdatasync  | 74 / 75  | **(stub-ok)**| WAL sync                                         |
| unlink             | 87       | **(done)**   | Journal cleanup                                  |
| access / faccessat | 21 / 269 | **(done)**   |                                                 |
| getcwd             | 79       | **(done)**   |                                                 |
| getpid             | 39       | **(done)**   | Lock ownership                                   |
| geteuid            | 107      | **(done)**   |                                                 |
| ioctl              | 16       | **(done)**   | TIOCGWINSZ for interactive mode                  |
| rt_sigaction       | 13       | **(done)**   |                                                 |
| pread64            | 17       | **(todo)**   | Database page reads at offset                    |
| pwrite64           | 18       | **(todo)**   | Database page writes at offset                   |

### Blockers for sqlite3

1. **pread64 / pwrite64** -- sqlite uses these heavily for random-access database I/O.
   Currently returns -EBADF; must be properly implemented.
2. **fcntl F_SETLK/F_GETLK** -- must implement POSIX file locking (not just F_DUPFD_CLOEXEC).
3. **mmap MAP_SHARED** -- needed for WAL mode; verify our mmap handles shared mappings.

---

## P5-003: gcc (GCC Compiler)

GCC is a multi-process pipeline: cpp -> cc1 -> as -> ld.  Each stage
fork+exec's the next.  Heavy file I/O and memory mapping for object files.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| openat             | 257      | **(done)**   |                                                 |
| fstat              | 5        | **(done)**   |                                                 |
| stat               | 4        | **(done)**   |                                                 |
| lstat              | 6        | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   | MAP_PRIVATE for .o/.a file mapping               |
| mprotect           | 10       | **(done)**   |                                                 |
| munmap             | 11       | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| fork               | 57       | **(done)**   | Spawns cc1, as, ld                               |
| vfork              | 58       | **(done)**   |                                                 |
| execve             | 59       | **(done)**   |                                                 |
| wait4              | 61       | **(done)**   |                                                 |
| pipe / pipe2       | 22 / 293 | **(done)**   | Pipeline between compiler stages                 |
| dup / dup2         | 32 / 33  | **(done)**   | Redirect stdin/stdout between stages             |
| fcntl              | 72       | **(done)**   |                                                 |
| getpid             | 39       | **(done)**   | Temp file naming                                 |
| getcwd             | 79       | **(done)**   |                                                 |
| access             | 21       | **(done)**   | Search for headers/libraries in $PATH            |
| unlink             | 87       | **(done)**   | Temp file cleanup                                |
| rename             | 82       | **(done)**   | Atomic output replacement                        |
| lseek              | 8        | **(done)**   |                                                 |
| getdents64         | 217      | **(done)**   |                                                 |
| rt_sigaction       | 13       | **(done)**   |                                                 |
| rt_sigprocmask     | 14       | **(done)**   |                                                 |
| clock_gettime      | 228      | **(done)**   |                                                 |
| uname              | 63       | **(done)**   | Target triple detection                          |
| pread64            | 17       | **(todo)**   | Reading archive (.a) members at offset           |
| readv / writev     | 19 / 20  | **(partial)**| readv returns -ENOSYS; writev done               |
| clone              | 56       | **(done)**   | Some GCC builds use threads for LTO              |
| futex              | 202      | **(done)**   | Thread synchronization                           |

### Blockers for gcc

1. **pread64** -- reading .a archive members (ar format uses offsets).
2. **readv** -- some I/O paths use scatter/gather; currently returns -ENOSYS.
3. **Large memory** -- GCC with LTO can use 500MB+; ensure mmap/brk handle large allocations.
4. All subprograms (cc1, as, collect2, ld) must be loadable via execve -- PIE ELF loading works.

---

## P5-004: dropbear (SSH Server)

Dropbear is a lightweight SSH server.  It needs networking, PTY allocation,
and process management for login shells.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| openat             | 257      | **(done)**   |                                                 |
| socket             | 41       | **(done)**   | AF_INET, SOCK_STREAM                             |
| bind               | 49       | **(done)**   | Bind to port 22                                  |
| listen             | 50       | **(done)**   |                                                 |
| accept / accept4   | 43 / 288 | **(done)**   | Accept incoming SSH connections                  |
| setsockopt         | 54       | **(done)**   | SO_REUSEADDR                                     |
| fork               | 57       | **(done)**   | One child per connection                         |
| execve             | 59       | **(done)**   | Exec login shell                                 |
| wait4              | 61       | **(done)**   | Reap child processes                             |
| setsid             | 112      | **(done)**   | Session leader for PTY                           |
| dup2               | 33       | **(done)**   | Redirect PTY slave to stdin/stdout/stderr        |
| pipe / pipe2       | 22 / 293 | **(done)**   |                                                 |
| select / pselect6  | 23 / 270 | **(done)**   | Multiplex network + PTY I/O                      |
| poll               | 7        | **(done)**   |                                                 |
| fcntl              | 72       | **(done)**   | O_NONBLOCK                                       |
| kill               | 62       | **(done)**   | Signal child on disconnect                       |
| rt_sigaction       | 13       | **(done)**   | SIGCHLD handler                                  |
| rt_sigprocmask     | 14       | **(done)**   |                                                 |
| getuid/geteuid     | 102/107  | **(done)**   |                                                 |
| setuid/setgid      | 105/106  | **(done)**   | Drop privileges after auth                      |
| chdir              | 80       | **(done)**   | cd to home directory                             |
| getpid             | 39       | **(done)**   |                                                 |
| uname              | 63       | **(done)**   |                                                 |
| getrandom          | 318      | **(done)**   | Cryptographic key generation                     |
| clock_gettime      | 228      | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| ioctl (PTY)        | 16       | **(partial)**| TIOCGWINSZ done; TIOCSCTTY, TIOCGPTN, TIOCSPTLCK needed |
| openat /dev/ptmx   | 257      | **(todo)**   | PTY master allocation                            |
| ptsname / unlockpt | --       | **(todo)**   | Libc wrappers; need ioctl TIOCGPTN + TIOCSPTLCK |
| /dev/pts/* nodes   | --       | **(todo)**   | devpts filesystem must create slave nodes        |

### Blockers for dropbear

1. **PTY subsystem** -- `/dev/ptmx` must be openable; ioctl TIOCGPTN (get PTY number),
   TIOCSPTLCK (unlock slave), TIOCSCTTY (set controlling terminal) must work.
   `/dev/pts/N` slave devices must be auto-created by devpts filesystem.
2. **Networking must handle real TCP** -- accept() must block and return connected fds.
3. **getrandom** must return actual entropy (not just zeros) for SSH key exchange.

---

## P5-005: python3

Python 3 is a large interpreter with extensive use of mmap, signal handling,
and filesystem introspection.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| openat             | 257      | **(done)**   | Module search                                    |
| fstat              | 5        | **(done)**   |                                                 |
| stat               | 4        | **(done)**   | Module import stat checks                        |
| lstat              | 6        | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   | Allocation arena                                 |
| mprotect           | 10       | **(done)**   |                                                 |
| munmap             | 11       | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| lseek              | 8        | **(done)**   |                                                 |
| getdents64         | 217      | **(done)**   | os.listdir()                                     |
| getcwd             | 79       | **(done)**   |                                                 |
| readlink           | 89       | **(done)**   | /proc/self/exe resolution                        |
| rt_sigaction       | 13       | **(done)**   | Signal module                                    |
| rt_sigprocmask     | 14       | **(done)**   |                                                 |
| sigaltstack        | 131      | **(done)**   | Stack overflow detection                         |
| ioctl              | 16       | **(done)**   | TIOCGWINSZ for terminal size                     |
| fcntl              | 72       | **(done)**   |                                                 |
| dup / dup2         | 32 / 33  | **(done)**   |                                                 |
| pipe / pipe2       | 22 / 293 | **(done)**   | subprocess module                                |
| fork               | 57       | **(done)**   | subprocess/multiprocessing                       |
| execve             | 59       | **(done)**   |                                                 |
| wait4              | 61       | **(done)**   |                                                 |
| getpid/getppid     | 39/110   | **(done)**   |                                                 |
| getuid/getgid/geteuid/getegid | 102/104/107/108 | **(done)** |                            |
| clock_gettime      | 228      | **(done)**   | time module                                      |
| gettimeofday       | 96       | **(done)**   |                                                 |
| futex              | 202      | **(done)**   | GIL and threading                                |
| arch_prctl         | 158      | **(done)**   | TLS setup                                        |
| set_tid_address    | 218      | **(done)**   |                                                 |
| prlimit64          | 302      | **(done)**   | resource module                                  |
| getrandom          | 318      | **(done)**   | os.urandom(), secrets module                     |
| sysinfo            | 99       | **(done)**   |                                                 |
| uname              | 63       | **(done)**   | platform module                                  |
| access / faccessat | 21 / 269 | **(done)**   |                                                 |
| rename/renameat2   | 82/316   | **(done)**   |                                                 |
| unlink/unlinkat    | 87/263   | **(done)**   |                                                 |
| mkdir/mkdirat      | 83/258   | **(done)**   |                                                 |
| chdir              | 80       | **(done)**   |                                                 |
| chmod/fchmod       | 90/91    | **(done)**   | stub-ok for os.chmod                             |
| select / pselect6  | 23 / 270 | **(done)**   | selectors module                                 |
| epoll_create1      | 291      | **(done)**   | selectors.EpollSelector                          |
| epoll_ctl          | 233      | **(done)**   |                                                 |
| epoll_wait/pwait   | 232/281  | **(done)**   |                                                 |
| socket             | 41       | **(done)**   | socket module                                    |
| connect            | 42       | **(done)**   |                                                 |
| bind               | 49       | **(done)**   |                                                 |
| listen             | 50       | **(done)**   |                                                 |
| pread64            | 17       | **(todo)**   | Used by importlib for .pyc reading               |
| readv              | 19       | **(todo)**   | Returns -ENOSYS; used by some I/O paths          |
| mremap             | 25       | **(todo)**   | Returns -ENOSYS; used for arena resizing         |

### Blockers for python3

1. **pread64** -- importlib reads `.pyc` files at specific offsets.
2. **/proc/self/exe** readlink must resolve to the actual python3 binary path.
3. **Large mmap space** -- Python uses many small mmap allocations for arenas; verify no fragmentation issues.
4. **readv** and **mremap** should be implemented for full compatibility.

---

## P5-006: rust / cargo

Cargo is the Rust build system.  It is a large, heavily multithreaded
program that downloads crates, invokes rustc, and links with lld/cc.

### Syscalls required

| Syscall            | Linux NR | Status       | Notes                                          |
|--------------------|----------|--------------|-------------------------------------------------|
| read               | 0        | **(done)**   |                                                 |
| write              | 1        | **(done)**   |                                                 |
| open               | 2        | **(done)**   |                                                 |
| close              | 3        | **(done)**   |                                                 |
| openat             | 257      | **(done)**   |                                                 |
| fstat              | 5        | **(done)**   |                                                 |
| stat               | 4        | **(done)**   |                                                 |
| lstat              | 6        | **(done)**   |                                                 |
| mmap               | 9        | **(done)**   | jemalloc, codegen memory                         |
| mprotect           | 10       | **(done)**   |                                                 |
| munmap             | 11       | **(done)**   |                                                 |
| brk                | 12       | **(done)**   |                                                 |
| mremap             | 25       | **(todo)**   | jemalloc arena resizing                          |
| fork               | 57       | **(done)**   | Spawning rustc, linker                           |
| vfork              | 58       | **(done)**   |                                                 |
| clone              | 56       | **(done)**   | Thread pool for parallel compilation             |
| execve             | 59       | **(done)**   |                                                 |
| wait4              | 61       | **(done)**   |                                                 |
| pipe / pipe2       | 22 / 293 | **(done)**   |                                                 |
| dup / dup2 / dup3  | 32/33/292| **(done)**   |                                                 |
| fcntl              | 72       | **(done)**   | File locks for cargo registry                    |
| flock              | 73       | **(stub-ok)**| Cargo uses flock for build directory locking     |
| socket             | 41       | **(done)**   | HTTP(S) to crates.io                             |
| connect            | 42       | **(done)**   |                                                 |
| sendto / recvfrom  | 44 / 45  | **(done)**   | DNS                                              |
| setsockopt         | 54       | **(done)**   |                                                 |
| getdents64         | 217      | **(done)**   |                                                 |
| getcwd             | 79       | **(done)**   |                                                 |
| readlink/readlinkat| 89/267   | **(done)**   | /proc/self/exe                                   |
| rt_sigaction       | 13       | **(done)**   |                                                 |
| rt_sigprocmask     | 14       | **(done)**   |                                                 |
| futex              | 202      | **(done)**   | Rayon thread pool, std::sync                     |
| arch_prctl         | 158      | **(done)**   | TLS for threads                                  |
| set_tid_address    | 218      | **(done)**   |                                                 |
| clock_gettime      | 228      | **(done)**   | Build timing                                     |
| getrandom          | 318      | **(done)**   | HashMap random state                             |
| sysinfo            | 99       | **(done)**   | Memory limits                                    |
| uname              | 63       | **(done)**   | Target triple                                    |
| rename/renameat2   | 82/316   | **(done)**   | Atomic file replacement                          |
| unlink/unlinkat    | 87/263   | **(done)**   |                                                 |
| mkdir/mkdirat      | 83/258   | **(done)**   |                                                 |
| chdir              | 80       | **(done)**   |                                                 |
| access / faccessat | 21 / 269 | **(done)**   |                                                 |
| epoll_create1      | 291      | **(done)**   | Tokio/mio event loop                             |
| epoll_ctl          | 233      | **(done)**   |                                                 |
| epoll_wait/pwait   | 232/281  | **(done)**   |                                                 |
| prlimit64          | 302      | **(done)**   |                                                 |
| pread64            | 17       | **(todo)**   | Reading .rlib/.a archive members                 |
| pwrite64           | 18       | **(todo)**   | Writing build artifacts                          |
| readv              | 19       | **(todo)**   | Returns -ENOSYS; Tokio scatter I/O               |
| symlinkat          | 266      | **(todo)**   | Cargo creates symlinks in target/                |
| linkat             | 265      | **(todo)**   | Hard links for incremental compilation cache     |
| sendfile           | 40       | **(todo)**   | Returns -ENOSYS; zero-copy file transfer         |
| copy_file_range    | 326      | **(todo)**   | Returns -ENOSYS; efficient file copy             |

### Blockers for rust/cargo

1. **pread64 / pwrite64** -- critical for reading/writing .rlib archive members.
2. **mremap** -- jemalloc (Rust's default allocator) uses mremap for arena management.
3. **clone with CLONE_THREAD** -- Rayon thread pool needs proper thread creation.
4. **Networking** -- cargo fetch needs working TCP + DNS to reach crates.io.
5. **symlinkat / linkat** -- cargo creates symlinks and hard links in the target directory.
6. **Memory** -- rustc + LLVM can use 1GB+ for large crates; ensure large mmap regions work.

---

## Summary: Cross-Cutting Gaps

These syscalls appear across multiple programs and are the highest-priority
items to implement:

| Syscall        | Programs needing it                     | Priority |
|----------------|-----------------------------------------|----------|
| **pread64**    | apk, sqlite3, gcc, python3, cargo       | P0       |
| **pwrite64**   | apk, sqlite3, cargo                     | P0       |
| **readv**      | gcc, python3, cargo                     | P1       |
| **mremap**     | python3, cargo                          | P1       |
| **symlinkat**  | apk, cargo                              | P1       |
| **linkat**     | apk, cargo                              | P1       |
| **PTY subsys** | dropbear (ioctl TIOCGPTN/TIOCSPTLCK/TIOCSCTTY, /dev/ptmx, /dev/pts/*) | P1 |
| **sendfile**   | apk, cargo                              | P2       |
| **copy_file_range** | cargo                              | P2       |
| **fcntl locks**| sqlite3 (F_SETLK/F_GETLK)              | P2       |

### Currently implemented syscall count

Lineluya dispatches ~150 unique syscall numbers in `syscall_chirho.rs`.
Of those, roughly 120 return functional values and ~30 are stubs (return 0 or
-ENOSYS).  The six programs above need approximately 8-10 additional real
implementations to become functional.
