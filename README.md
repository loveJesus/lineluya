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

### Current Status: v3.2.0 — "Clearing the Land"

The kernel boots in QEMU, runs **real Alpine Linux programs** via **musl 1.2.5 dynamic linker** with **DHCP networking**, reading from a **512MB ext4 rootfs** on **VirtIO-blk**:

```
lineluya# echo hello
hello

lineluya# ls /
bin   dev   etc   home   lib   lost+found   media   mnt
opt   proc   root   run   sbin   srv   sys   tmp   usr   var

lineluya# sqlite3 :memory: "SELECT 316, 42+1;"
316|43

lineluya# apk --version
apk-tools 2.14.6, compiled for x86_64.

lineluya# id
uid=0(root) gid=0(root)

lineluya# date
Sun Mar 15 00:00:00 UTC 2026

lineluya# cat /etc/hostname
localhost
```

**Verified working in QEMU (x86_64):**
- **SQLite 3.51.2** — executes SQL queries from ext4 disk (dynamically linked)
- **apk-tools 2.14.6** — Alpine package manager runs
- **DHCP networking** — IP=10.0.2.15, GW=10.0.2.2, DNS=10.0.2.3 via VirtIO-net
- BusyBox shell with color `ls`, cat, date, id, echo, uname (200+ applets)
- Pixel framebuffer console (1280x800, green-on-black, UEFI)
- VirtIO-blk + VirtIO-net I/O port drivers
- ext4 mounted at `/` with symlink following
- musl 1.2.5: full ELF loading with GLOB_DAT/JUMP_SLOT symbol resolution
- 75+ syscalls, 60+ kernel symbol exports for .ko module loading
- VFS: ext4 at /, tmpfs at /tmp, procfs, devtmpfs, sysfs
- Fork/exec/exit cycle with shell re-exec
- 60,000+ lines of Rust across 75+ kernel modules

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
