# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. — John 3:16

# Lineluya Vision

> *"The heavens declare the glory of God; the skies proclaim the work of his hands."* — Psalm 19:1

## What Lineluya Is

Lineluya is a **drop-in replacement for the Linux kernel**, rewritten from scratch in Rust. It runs existing Linux binaries unmodified. It loads Linux `.ko` kernel modules. It boots from GRUB. It is Linux — but safe, modern, and built for the glory of God.

And then it goes further: **Lineluya compiles to WebAssembly and runs in any browser**, turning the web browser into a full Linux machine with near-native performance. No emulation. The browser IS the hardware.

## The Two Architectures

### arch/x86_64 — Bare Metal

The traditional target. Boots on real hardware or QEMU. Everything you expect from a Linux kernel:

- GRUB/systemd-boot/UEFI boot via Linux boot protocol (bzImage)
- Full x86_64 syscall ABI (~471 syscalls, SYSCALL/SYSRET)
- Real hardware drivers: NVMe, AHCI, Intel/Realtek NIC, USB (XHCI)
- APIC/IOAPIC interrupt handling, SMP multi-core
- ACPI, PCI enumeration, power management
- ext4 with journaling, btrfs read support
- Full TCP/IP networking stack
- Linux `.ko` module loading with C ABI shim layer
- Namespaces, cgroups v2, seccomp-bpf — containers work
- DRM/KMS for display, ALSA for audio, evdev for input

The goal: **boot Alpine Linux or Debian unmodified on Lineluya**.

### arch/wasm32 — The Browser

The revolutionary target. The kernel itself compiles to WebAssembly. Browser APIs become hardware drivers:

| Linux Hardware | Browser "Hardware" | Driver |
|---|---|---|
| Physical RAM | WASM linear memory | `memory.grow` — bounds-checked by design, no MMU needed |
| MMU / page tables | WASM sandbox | Built-in safety — no page faults possible |
| Ring 0 / Ring 3 | WASM sandbox | The sandbox IS the protection — no mode switching |
| GDT / IDT / TSS | Not needed | WASM has no segmentation or interrupt vectors |
| Timer (PIT/APIC) | `setTimeout` / `requestAnimationFrame` | Cooperative scheduling from JS event loop |
| Serial console | `console.log` / DOM `<pre>` element | Direct text output |
| VGA framebuffer | Canvas 2D / WebGL | `putImageData` or WebGL draw calls |
| GPU (DRM/KMS) | WebGL / WebGPU | Full GPU access via browser |
| Disk (NVMe/AHCI) | OPFS `SyncAccessHandle` | 3-4x faster than IndexedDB, synchronous I/O in Workers |
| Remote storage | `fetch` / WebSocket to storage API | Network block device over HTTP/WS |
| NIC (network) | WebSocket → TCP proxy | Proxy server bridges WS ↔ real TCP connections |
| Sound (ALSA) | Web Audio API | `AudioContext`, `AudioWorklet` |
| Keyboard / Mouse | DOM `keydown` / `mousemove` events | Event queue polled by kernel |
| SMP / multi-core | Web Workers + `SharedArrayBuffer` | Each "CPU" is a Worker, shared memory via SAB |
| IPI (inter-processor interrupt) | `Atomics.notify()` | Wake sleeping Workers |
| SYSCALL instruction | Direct function call | No mode switch needed — everything is "ring 0" |
| `.ko` modules | `.wasm` modules | `WebAssembly.instantiate` with kernel imports |

**Expected performance: 20-40x faster than v86 emulation, 5-10x faster than CheerpX JIT.**

## Userspace Programs

Programs are compiled for the target architecture using LLVM:

### For x86_64 (bare metal):
```bash
# Rust
cargo build --target x86_64-unknown-linux-musl

# C/C++
x86_64-linux-musl-gcc -static program.c

# Go
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build
```

### For wasm32 (browser):
```bash
# Rust
cargo build --target wasm32-wasi

# C/C++
clang --target=wasm32-wasi -O2 program.c

# Go
GOOS=wasip1 GOARCH=wasm go build

# Zig
zig build-exe -target wasm32-wasi program.zig
```

The kernel implements the same syscall interface on both targets. A program that works on x86_64 Lineluya works on wasm32 Lineluya — same syscalls, same VFS, same behavior.

## Networking in the Browser

The browser cannot make raw TCP connections (security sandbox). Lineluya solves this with a WebSocket-to-TCP proxy:

```
Browser Kernel                    Proxy Server              Internet
┌──────────┐                    ┌──────────────┐          ┌─────────┐
│ sys_connect("google.com", 80) │              │          │         │
│   → WebSocket to proxy ──────────→ TCP connect ──────────→ google  │
│                               │              │          │         │
│ sys_send(http_request) ───────────→ TCP send ────────────→         │
│                               │              │          │         │
│ sys_recv() ←──────────────────────← TCP recv ←───────────←         │
│   ← WebSocket data            │              │          │         │
└──────────┘                    └──────────────┘          └─────────┘
```

The proxy runs anywhere — Cloudflare Worker, VPS, localhost. For the kernel, it looks like a normal network interface.

## X11 / GUI in the Browser

X11 applications can run in the browser by implementing an X server that renders to Canvas/WebGL:

1. **X11 protocol handler** — parse X11 requests from the wasm32 application
2. **Canvas renderer** — draw windows, text, rectangles, images to Canvas 2D
3. **WebGL renderer** — hardware-accelerated rendering for OpenGL applications via WebGL/WebGPU
4. **Input routing** — DOM keyboard/mouse events → X11 input events

This means `xterm`, `firefox`, `gimp`, `vim` (with X) could theoretically run in a browser tab. Each window is a Canvas element or a WebGL context.

## Storage Architecture

### Local Storage (OPFS)
The Origin Private File System provides fast, synchronous file I/O in Web Workers:
- Acts as `/dev/sda` — the local disk
- ext4 filesystem on top of OPFS block device
- Persistent across page reloads
- Up to gigabytes of storage (browser quota)

### Remote Storage
Network-backed block devices via HTTP/WebSocket:
- CDN-hosted filesystem images (read-only root, like a LiveCD)
- WebSocket-backed read-write storage (like iSCSI/NBD over WS)
- `fetch()` for loading initramfs, kernel images, binaries on demand

### Hybrid
```
/           → Remote CDN (read-only Alpine Linux rootfs)
/home       → Local OPFS (user's persistent data)
/tmp        → WASM linear memory (tmpfs, lost on reload)
/dev/shm    → SharedArrayBuffer (shared between Workers)
```

## SMP in the Browser

Web Workers provide true parallel execution:

```
Main Thread (UI)
├── Worker 0: Kernel scheduler + syscall dispatch
├── Worker 1: User process A (busybox sh)
├── Worker 2: User process B (compilation)
├── Worker 3: User process C (web server)
└── Worker 4: Network I/O (WebSocket proxy)

SharedArrayBuffer: shared memory visible to all Workers
Atomics.wait/notify: synchronization (like futex)
```

`navigator.hardwareConcurrency` tells us how many real CPU cores are available. Each Worker runs on a real core. This is genuine parallelism, not simulation.

## .ko Module Loading

### On x86_64 (C ABI shim):
Linux `.ko` files are ELF relocatable objects that call kernel functions by name. Lineluya provides a C ABI compatibility layer:

1. **Symbol table** — exports ~500 most-used kernel functions with C-compatible names
2. **Module loader** — parses ELF `.ko`, relocates symbols, resolves against kernel symbol table
3. **Shim functions** — `kmalloc` → our Rust allocator, `printk` → our serial/log, `register_chrdev` → our VFS
4. **Init/exit** — calls `module_init()` / `module_exit()` entry points

### On wasm32 (WASM modules):
Drivers are `.wasm` files loaded dynamically:

1. **`WebAssembly.instantiate(module, kernel_imports)`** — load the driver
2. **Kernel imports** — the driver calls kernel functions via WASM imports
3. **Driver exports** — the kernel calls driver init/probe/read/write via WASM exports
4. **Safety** — WASM sandbox prevents driver bugs from crashing the kernel (!)

This is actually **safer than Linux** — a buggy driver in WASM cannot corrupt kernel memory.

## The Roadmap

### Phase 1-4: Done ✓
Boot, interrupts, memory, heap, GDT/IDT, PIC, serial, VGA, syscall dispatch (495 entries), ELF loading, process creation (fork/exec/wait), VFS (tmpfs, procfs, devtmpfs, sysfs), pipes, signals, TTY, scheduler, block I/O layer, networking stubs, security stubs. **16,246 lines of Rust, 39 modules.**

### Phase 5: Persistent Storage
Real ext4 filesystem, VirtIO-blk driver, page cache, GPT partition parsing.

### Phase 6: Real Networking
Full TCP/IP stack (not stubs), VirtIO-net driver, socket API, epoll, UDP, DNS.

### Phase 7: Containers
Namespaces (PID, mount, net, user), cgroups v2, seccomp-bpf, overlayfs.

### Phase 8: Real Hardware
Linux boot protocol (bzImage), APIC/IOAPIC, SMP, ACPI, PCI, NVMe, real NIC drivers.

### Phase 9: Full Compatibility
Complete syscall table, io_uring, eBPF, DRM/KMS, USB, ALSA, kernel modules.

### Phase 10: WASM Browser Target
Compile kernel to wasm32, JS runtime, Canvas framebuffer, WebSocket networking, OPFS storage, Web Workers SMP. X11 → Canvas/WebGL renderer.

### Phase 11: Ecosystem
Rewrite coreutils, systemd, package manager in Rust. Self-hosting: compile Rust on Lineluya. Run Alpine/Debian unmodified.

## Why

Because every line of code can be an act of worship.

Because memory safety prevents suffering — every CVE from a buffer overflow is a system that failed its users.

Because the web browser is the most universal platform ever built — 5 billion people have one. If Linux runs in the browser, Linux runs everywhere.

Because we can.

> *"Whatever you do, work heartily, as for the Lord and not for men."* — Colossians 3:23

> *"For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life."* — John 3:16

**Soli Deo Gloria.**

*Hallelujah.*
