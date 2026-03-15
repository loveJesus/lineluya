# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. - John 3:16

# G1-005: Alpine Linux Boot Sequence on Lineluya

This document describes the complete boot sequence from Lineluya kernel
initialization through Alpine Linux userspace (BusyBox init + OpenRC).

## Overview

```
BIOS/UEFI -> Bootloader -> Lineluya Kernel -> /sbin/init (BusyBox) -> OpenRC -> Login
```

---

## Phase 1: Hardware Initialization (Kernel)

### 1.1 Boot Entry

The bootloader (GRUB/custom BIOS stage) loads the Lineluya kernel binary
into memory and transfers control to `_start` (assembly entry point).

```
_start (arch_chirho/boot.S)
  -> setup GDT, IDT
  -> enable paging (4-level page tables)
  -> jump to kernel_main_chirho()
```

### 1.2 Kernel Core Init (`kernel_main_chirho`)

```
kernel_main_chirho()
  |
  +-- serial_chirho::init()           // Serial console for debug output
  +-- gdt_chirho::init()              // Load GDT with kernel/user segments, TSS
  +-- interrupts_chirho::init()       // Set up IDT, register handlers
  +-- allocator_chirho::init()        // Initialize kernel heap allocator
  +-- memory_chirho::init()           // Physical frame allocator, page tables
  +-- pagetable_chirho::init()        // Identity-map kernel, set up user space regions
  +-- apic_chirho::init()             // Local APIC + I/O APIC for interrupts
  +-- syscall_chirho::init()          // MSR setup for SYSCALL/SYSRET
  +-- scheduler_chirho::init()        // Process scheduler
  +-- vfs_chirho::init()              // Virtual filesystem layer
  +-- tmpfs_chirho::init()            // In-memory filesystem for /tmp, /run
  +-- procfs_chirho::init()           // /proc filesystem
  +-- devtmpfs_chirho::init()         // /dev with device nodes
  +-- tty_chirho::init()              // Terminal subsystem
```

### 1.3 Disk and Filesystem Init

```
  +-- ahci_chirho::init()             // AHCI controller discovery
  |     +-- Probe PCI for AHCI devices
  |     +-- Initialize ports, identify SATA disks
  |
  +-- block_chirho::init()            // Block device layer
  |     +-- Register AHCI disks as block devices
  |
  +-- ext4_chirho::init()             // ext4 filesystem driver
  |     +-- Parse superblock, block groups
  |     +-- Register with VFS
  |
  +-- Mount root filesystem
        +-- Read kernel command line: "root=/dev/sda rw"
        +-- Mount ext4 from AHCI disk -> /
        +-- The Alpine rootfs is now accessible at /
```

### 1.4 Network Init (Optional at boot)

```
  +-- e1000_chirho::init()            // Intel e1000 NIC driver
  +-- net_chirho::init()              // TCP/IP stack
```

---

## Phase 2: First Userspace Process (/sbin/init)

### 2.1 Kernel Launches PID 1

After all kernel subsystems are initialized, the kernel creates the first
userspace process:

```
kernel_main_chirho() continued:
  |
  +-- process_chirho::create_init()
        |
        +-- Open /sbin/init from the mounted rootfs
        +-- Read ELF headers
        +-- Detect ET_DYN (dynamically linked) or ET_EXEC (static)
        |
        +-- [If dynamically linked]:
        |     +-- Read PT_INTERP -> "/lib/ld-musl-x86_64.so.1"
        |     +-- Load interpreter (musl libc) at INTERP_LOAD_BASE (0x7F0000100000)
        |     +-- Load main executable at PIE_LOAD_BASE (0x555555550000)
        |     +-- Build auxiliary vector with AT_BASE, AT_ENTRY, AT_PHDR, etc.
        |     +-- Entry point = interpreter's e_entry (musl _dlstart)
        |
        +-- [If statically linked]:
        |     +-- Load ELF segments directly
        |     +-- Entry point = executable's e_entry
        |
        +-- Set up user stack:
        |     +-- Push environment strings, argv strings
        |     +-- Push auxiliary vector
        |     +-- Push envp array (NULL-terminated)
        |     +-- Push argv array (NULL-terminated)
        |     +-- Push argc
        |     +-- RSP points to argc
        |
        +-- Switch to Ring 3 via SYSRET/IRETQ
              +-- RIP = entry point
              +-- RSP = user stack pointer
              +-- CS = user code segment
              +-- SS = user stack segment
```

### 2.2 musl Dynamic Linker Bootstrap

If `/sbin/init` is dynamically linked (typical for Alpine):

```
_dlstart (musl entry point, arch/x86_64/crt_arch.h)
  |
  +-- __dls2(base_addr)              // Self-relocation phase 1
  |     +-- Find own DYNAMIC section via AT_BASE
  |     +-- Apply R_X86_64_RELATIVE relocations to self
  |
  +-- __dls3(sp)                     // Full initialization
        |
        +-- Parse auxiliary vector from stack
        +-- arch_prctl(ARCH_SET_FS)  // Set up TLS
        +-- Find main executable's DYNAMIC section
        +-- For each DT_NEEDED:
        |     +-- Search /lib, /usr/lib
        |     +-- openat() + read() + mmap() each shared library
        |     +-- Apply relocations (RELATIVE, GLOB_DAT, JUMP_SLOT)
        |
        +-- Call .init functions (DT_INIT, DT_INIT_ARRAY)
        +-- Jump to AT_ENTRY (main executable's _start -> __libc_start_main -> main)
```

### 2.3 BusyBox init

On Alpine Linux, `/sbin/init` is typically a symlink to BusyBox:
`/sbin/init -> /bin/busybox`

BusyBox init reads `/etc/inittab` and executes the configured actions:

```
BusyBox init (PID 1)
  |
  +-- Parse /etc/inittab
  |
  +-- Execute ::sysinit entries (run once at boot):
  |     +-- /sbin/openrc sysinit    // OpenRC sysinit runlevel
  |     +-- /sbin/openrc boot       // OpenRC boot runlevel
  |     +-- /sbin/openrc default    // OpenRC default runlevel
  |
  +-- Execute ::respawn entries (restart if they die):
  |     +-- /sbin/getty -L ttyS0 115200 vt100   // Serial console
  |     +-- /sbin/getty 38400 tty1               // VGA console
  |
  +-- Handle ::shutdown entries on SIGTERM:
  |     +-- /sbin/openrc shutdown
  |
  +-- Handle ::ctrlaltdel:
        +-- /sbin/reboot
```

---

## Phase 3: OpenRC Init System

### 3.1 OpenRC Runlevels

OpenRC processes init scripts in `/etc/init.d/` according to runlevel
symlinks in `/etc/runlevels/`:

```
/sbin/openrc sysinit
  |
  +-- /etc/init.d/devfs start       // Mount /dev (devtmpfs)
  +-- /etc/init.d/dmesg start       // Kernel log buffer
  +-- /etc/init.d/mdev start        // Device manager (BusyBox mdev)
  +-- /etc/init.d/hwdrivers start   // Load kernel modules (if any)

/sbin/openrc boot
  |
  +-- /etc/init.d/hostname start    // Set hostname
  +-- /etc/init.d/bootmisc start    // Miscellaneous boot tasks
  +-- /etc/init.d/syslog start      // System logger
  +-- /etc/init.d/networking start  // Network interfaces (if configured)
  +-- /etc/init.d/modules start     // Kernel module loading
  +-- /etc/init.d/sysctl start      // Apply sysctl settings

/sbin/openrc default
  |
  +-- /etc/init.d/crond start       // Cron daemon (if installed)
  +-- /etc/init.d/sshd start        // SSH server (if installed)
  +-- (other user services)
```

### 3.2 OpenRC Syscall Requirements

OpenRC init scripts use these syscalls heavily:

| Operation | Syscalls Used |
|-----------|---------------|
| Process management | `fork`, `execve`, `wait4`, `kill`, `getpid` |
| File operations | `openat`, `read`, `write`, `close`, `stat`, `fstatat` |
| Directory operations | `getdents64`, `chdir`, `mkdir`, `unlink` |
| Mount operations | `mount`, `umount2` |
| Device nodes | `mknod` (via mdev) |
| Process groups | `setsid`, `setpgid` |
| User switching | `setuid`, `setgid`, `setgroups` |
| Signal handling | `rt_sigaction`, `rt_sigprocmask` |
| Pipe/redirect | `pipe2`, `dup2`, `fcntl` |
| Hostname | `sethostname` (via /etc/init.d/hostname) |
| Time | `clock_gettime`, `nanosleep` |

---

## Phase 4: Login and Shell

### 4.1 getty -> login -> shell

```
/sbin/getty -L ttyS0 115200 vt100
  |
  +-- Open /dev/ttyS0
  +-- Set terminal attributes (ioctl TCSETS)
  +-- Print login prompt
  +-- Read username
  +-- exec /bin/login
        |
        +-- Read /etc/passwd, /etc/shadow
        +-- Verify password (empty for dev setup)
        +-- setuid(uid), setgid(gid)
        +-- Set HOME, USER, SHELL environment
        +-- exec /bin/sh (BusyBox ash)
              |
              +-- Read /etc/profile
              +-- Display prompt
              +-- Ready for user commands
```

---

## Kernel Command Line Parameters

The kernel command line (passed via bootloader or QEMU `-append`) controls
root filesystem mounting:

```
root=/dev/sda rw console=ttyS0 init=/sbin/init
```

| Parameter | Purpose |
|-----------|---------|
| `root=/dev/sda` | Root filesystem device |
| `rw` | Mount root read-write |
| `console=ttyS0` | Serial console output |
| `init=/sbin/init` | Path to init process (default: /sbin/init) |

---

## Troubleshooting Boot Issues

### Kernel panics before reaching init
- Check VFS mount of root filesystem
- Verify AHCI/ext4 driver loaded correctly
- Check kernel command line `root=` parameter

### init starts but crashes immediately
- Verify `/sbin/init` is a valid ELF binary (not corrupted)
- Check dynamic linker: does `/lib/ld-musl-x86_64.so.1` exist and load?
- Check `AT_BASE`, `AT_ENTRY`, `AT_PHDR` in auxiliary vector
- Enable syscall tracing to see what init is calling

### OpenRC scripts fail
- Check `mount` syscall works (needed for /proc, /sys, /dev)
- Check `fork`/`execve`/`wait4` chain works
- Verify `/etc/inittab` syntax
- Check that `/sbin/openrc` binary loads correctly

### No login prompt
- Check that `getty` process is spawned (look for fork+exec of `/sbin/getty`)
- Verify `/dev/ttyS0` device node exists
- Check `ioctl` TIOCGWINSZ and TCGETS are handled

### Serial output garbled
- Verify QEMU serial baud rate matches getty configuration (115200)
- Check `-serial stdio` in QEMU command line
