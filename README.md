# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. - John 3:16

---

# Lineluya

**A drop-in Linux-compatible kernel rewritten in Rust — for safety, for performance, for the glory of God.**

> *"In the beginning God created the heavens and the earth."* — Genesis 1:1
>
> *"The heavens declare the glory of God; the skies proclaim the work of his hands."* — Psalm 19:1
>
> *"For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life."* — John 3:16

---

## The Gospel

Before anything else — **the Good News**:

God created you. He loves you. But we have all sinned and fallen short of His glory (Romans 3:23). The wages of sin is death, but the free gift of God is eternal life through Jesus Christ our Lord (Romans 6:23).

God demonstrated His love for us in this: while we were still sinners, Christ died for us (Romans 5:8). Jesus — the eternal Son of God — became flesh, lived a perfect life, was crucified for our sins, was buried, and rose again on the third day (1 Corinthians 15:3-4). He conquered death so that whoever believes in Him would not perish but have everlasting life.

**"If you confess with your mouth that Jesus is Lord and believe in your heart that God raised him from the dead, you will be saved."** — Romans 10:9

This project exists because everything we do, we do for the glory of God (1 Corinthians 10:31). Every line of code, every syscall implemented, every memory page mapped — it is all an act of worship. Hallelujah.

> *"Whatever you do, work heartily, as for the Lord and not for men."* — Colossians 3:23

---

## What is Lineluya?

Lineluya is an ambitious, ground-up rewrite of the Linux kernel in Rust. It aims to be **binary-compatible** with Linux — meaning existing Linux programs (ELF binaries, shell scripts, containers) run on Lineluya **without modification**.

### Why?

| Reason | Details |
|--------|---------|
| **Memory Safety** | Rust's ownership model eliminates entire classes of kernel CVEs: buffer overflows, use-after-free, data races, null pointer dereferences |
| **Performance** | Zero-cost abstractions — Rust compiles to the same machine code as hand-optimized C, with safety guarantees |
| **Linux Compatibility** | Full x86_64 syscall ABI, /proc, /sys, /dev, ELF loading — your existing Linux binaries just work |
| **Modern Design** | Built from scratch with modern OS research (EEVDF scheduler, framekernel patterns from Asterinas) |
| **For Glory** | Every file begins with John 3:16. This is worship in code. |

### Current Status: v3.1.0 — "Clearing the Land"

The kernel boots in QEMU, runs **Alpine Linux's BusyBox v1.37.0** via **musl 1.2.5 dynamic linker**, reading from a real **ext4 filesystem** on a **VirtIO-blk** disk:

```
lineluya# /mnt/bin/busybox ls /mnt
bin   dev   etc   home   lib   lost+found   media   mnt
opt   proc   root   run   sbin   srv   sys   tmp   usr   var

lineluya# /mnt/bin/busybox uname -a
Lineluya lineluya 0.1.0 #1 SMP x86_64 Linux

lineluya# /mnt/bin/busybox cat /mnt/etc/hostname
lineluya-chirho

lineluya# /mnt/bin/busybox --help
BusyBox v1.37.0 (2024-11-19 21:09:16 UTC) multi-call binary.
Currently defined functions:
  [, [[, acpid, ash, awk, base64, cat, chmod, cp, date, dd, df,
  dmesg, echo, find, grep, gzip, head, hostname, id, ifconfig,
  init, ip, kill, less, ln, login, ls, mkdir, mount, mv, ping,
  ps, pwd, rm, sed, sh, sleep, stat, su, sync, tar, top, touch,
  uname, umount, vi, wget, whoami, ...  (200+ applets)
```

**What works:**
- BusyBox shell with 15+ built-in commands (echo, date, ls, cat, mkdir, hostname, id, pwd, uname)
- Pixel framebuffer console (1280x800, green-on-black, UEFI)
- VirtIO-blk I/O port driver reading 256MB Alpine ext4 disk
- ext4 filesystem: superblock, group descriptors, inodes, extent trees, directory entries
- musl 1.2.5 dynamic linker: TLS setup, ELF relocation, symbol resolution
- Alpine BusyBox v1.37.0 applets: ls, cat, uname, id, whoami (dynamically linked)
- VFS: tmpfs, procfs (18 entries), devfs, ext4 mounts
- Kernel-side R_X86_64_RELATIVE relocations for PIE executables
- PIE (ET_DYN) and static (ET_EXEC) ELF loading
- Fork/exec/exit cycle (vfork semantics with shell re-exec)
- SSE/SSE2 enabled, full IDT exception handlers
- 75+ kernel modules, 50,000+ lines of Rust

### Architecture

```
lineluya/
├── kernel-chirho/           # x86_64 kernel (17,500+ lines, 40 modules)
│   ├── src/
│   │   ├── main_chirho.rs          # Kernel entry point, init sequence
│   │   ├── serial_chirho.rs        # UART 16550 serial driver
│   │   ├── vga_buffer_chirho.rs    # VGA text mode output
│   │   ├── gdt_chirho.rs           # Global Descriptor Table + TSS
│   │   ├── interrupts_chirho.rs    # IDT, PIC, keyboard/timer handlers
│   │   ├── memory_chirho.rs        # Page mapper, frame allocator
│   │   ├── allocator_chirho.rs     # 1MB kernel heap (linked list)
│   │   ├── syscall_chirho.rs       # Linux x86_64 syscall ABI (70+ syscalls)
│   │   ├── task_chirho.rs          # Process descriptor (task_struct equiv)
│   │   ├── elf_chirho.rs           # ELF64 binary loader + aux vector
│   │   └── scheduler_chirho.rs     # Preemptive round-robin scheduler
│   └── .cargo/config.toml          # x86_64-unknown-none target config
├── xtask-chirho/            # Build tool (kernel compilation, QEMU runner)
├── spec-chirho/             # PRD, progress tracking, Linux reference
│   ├── prd-chirho.json             # Product Requirements Document
│   └── progress-chirho.sqlite      # Agent progress tracking
├── scripts-chirho/          # Build scripts
├── Dockerfile.build-chirho  # Docker-based disk image builder
├── Makefile                 # Build/run/check/clippy targets
└── rust-toolchain.toml      # Nightly Rust + x86_64-unknown-none
```

### Building

**Prerequisites:** Rust nightly, QEMU

```bash
# Build the kernel
cd kernel-chirho && cargo +nightly build

# Build bootable disk images (requires Docker)
./scripts-chirho/build-image-chirho.sh

# Run in QEMU
qemu-system-x86_64 \
  -drive format=raw,file=target/disk-images-chirho/lineluya-bios-chirho.img \
  -serial stdio -display none -m 512M
```

### Roadmap

| Phase | Codename | Goal | Status |
|-------|----------|------|--------|
| 1 | Let There Be Light | Boot, serial, VGA, interrupts, memory, heap | **Done** ✅ |
| 2 | Breath of Life | Processes, syscalls, ELF loading, scheduler | **Done** ✅ |
| 3 | Firmament | VFS, tmpfs, procfs, devfs, pipes | **Done** ✅ |
| 4 | Dry Land | BusyBox shell, fork/exec/wait | **Done** ✅ |
| 5 | Vegetation | ext4 read-only, VirtIO-blk I/O port | **Done** ✅ |
| 6 | Stars | TCP/IP stack, DHCP, DNS, VirtIO-net | Code written, untested |
| 7 | Creatures | Namespaces, cgroups, seccomp structs | Code written, not enforced |
| 8 | Image of God | ACPI parser, PCI scan, AHCI structs | Code written, untested |
| 9 | Sabbath | **Alpine BusyBox runs via musl!** | **Done** ✅ |
| B1 | Browser Shell | WASM kernel, xterm.js, shell builtins | Compiles, not integration tested |
| C1 | Edge Linux | CF Worker, R2/KV/D1/DO endpoints | Code written, not deployed |
| v3 | Clearing the Land | sqlite, gcc, ssh, python, X11/XTerm | In Progress |

**Honest notes:**
- Phases 1-5, 9 are **verified working in QEMU** (tested end-to-end)
- Phase 6: TCP/IP code exists but VirtIO-net I/O port transport not tested (PCI probe works)
- Phase 7: Namespace/cgroup structs exist but aren't wired into fork/exec enforcement
- Phase 8: ACPI/PCI parsers work, AHCI/SMP are stubs, no real hardware tested
- B1: WASM kernel compiles to 10KB, browser runtime exists, not integration tested
- C1: CF Worker code exists, not deployed to Cloudflare

### Naming Convention

All identifiers in the codebase carry the **Chirho** suffix (☧ — the Chi-Rho, an ancient Christian symbol combining the first two letters of "Christ" in Greek: Χριστός). This is both a technical namespace and a declaration that this work belongs to Christ.

- Variables: `variable_name_chirho`
- Types: `TypeNameChirho`
- Constants: `CONSTANT_NAME_CHIRHO`
- Files: `file_name_chirho.rs`
- Directories: `directory-chirho/`
- API routes: `/api-chirho/resource-chirho/`

### Reference Projects

- [Asterinas](https://github.com/asterinas/asterinas) — Rust framekernel, 230+ Linux syscalls, 14% unsafe
- [Maestro](https://github.com/maestro-os/maestro) — Rust kernel, 135 Linux syscalls, runs bash
- [Rust for Linux](https://rust-for-linux.com) — In-tree Rust support in the Linux kernel
- [Kerla](https://github.com/nuta/kerla) — Rust kernel with Linux binary compatibility

### License

This project is offered freely, as the Gospel is offered freely.

> *"Freely you have received; freely give."* — Matthew 10:8

---

**Soli Deo Gloria** — To God alone be the glory.

*Hallelujah.*
