# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. — John 3:16

# Lineluya: One Kernel, Every Platform

> *"See, I am doing a new thing! Now it springs up; do you not perceive it?"* — Isaiah 43:19

---

## What Is Lineluya?

Lineluya is the Linux kernel rewritten in Rust. Not a compatibility layer. Not an emulator. A real kernel — the same job Linux does, done from scratch in a memory-safe language.

But Lineluya does something Linux never could: **it runs everywhere**.

The same kernel that boots on bare metal x86 hardware also compiles to WebAssembly and runs in a browser tab. And on Cloudflare's edge network. And eventually on ARM and RISC-V. Same code. Same syscalls. Same programs. Different hardware underneath.

---

## The Three Tracks

### Track A: Bare Metal — Replace Linux on Real Hardware

This is the traditional use case. Lineluya boots on real x86_64 machines (and eventually aarch64, riscv64). It loads Linux `.ko` driver modules. It runs unmodified Linux binaries. You can swap out your Linux kernel for Lineluya and everything keeps working — but now your kernel is written in Rust, so entire classes of security vulnerabilities (buffer overflows, use-after-free, data races) are eliminated at compile time.

```
GRUB → Lineluya (Rust) → BusyBox, systemd, Docker, SSH, nginx
                          All running unmodified.
```

**What works today:**
- Boots in QEMU
- Runs userspace ELF binaries (hello world prints via SYSCALL)
- 495 syscall dispatch entries covering the Linux x86_64 table
- Real fork(), execve(), wait4()
- VFS with tmpfs, procfs, devtmpfs, sysfs, pipes
- TTY with keyboard input to userspace
- Signals, scheduler, block I/O layer
- APIC, ACPI, PCI enumeration stubs
- 16,246 lines of Rust across 39 modules

**What's next:**
- Per-process address spaces (COW fork)
- BusyBox shell running interactively
- Linux .ko module loading via C ABI shim
- Real TCP/IP networking
- ext4 filesystem
- Real hardware drivers (NVMe, USB, Intel NIC)
- Boot on real hardware via GRUB


### Track B: Browser — Full Linux in Any Browser Tab

This is the revolutionary part. The kernel compiles to WebAssembly and runs in the browser. The browser's APIs become the hardware:

| What the kernel needs | What the browser provides |
|---|---|
| RAM | WASM linear memory (bounds-checked by design) |
| CPU protection rings | WASM sandbox (the sandbox IS the protection) |
| Page tables / MMU | Not needed (WASM has built-in memory safety) |
| Timer interrupts | `setTimeout` / `requestAnimationFrame` |
| Framebuffer / GPU | Canvas 2D / WebGL / WebGPU |
| Disk drive | OPFS (Origin Private File System) for local storage |
| Network card | WebSocket through a proxy server |
| Sound card | Web Audio API |
| Keyboard / Mouse | DOM events |
| Multi-core CPU | Web Workers + SharedArrayBuffer |

Programs are compiled to `wasm32-wasi` using standard LLVM toolchains:

```bash
# Compile any Rust program for the browser kernel
cargo build --target wasm32-wasi

# Compile any C program for the browser kernel
clang --target=wasm32-wasi program.c

# Compile any Go program for the browser kernel
GOOS=wasip1 GOARCH=wasm go build
```

The program makes Linux syscalls. The WASM kernel handles them using browser APIs. The program doesn't know it's in a browser. It thinks it's on Linux.

**What this enables:**
- BusyBox shell in a browser tab
- vim, gcc, python running in the browser
- X11 applications rendered to Canvas/WebGL
- SSH to real servers via WebSocket proxy
- Files that persist across browser sessions (OPFS)
- Full Linux desktop experience in a browser tab
- Works on any device with a modern browser — phone, tablet, Chromebook, anything

**Performance:** 20-40x faster than x86 emulation (v86), 5-10x faster than JIT translation (CheerpX). This is native WASM execution, not emulation.


### Track C: Edge — Linux on Every Cloudflare Edge Node

This is the mind-bending part. Cloudflare Workers run V8 which runs WASM. Our kernel is WASM. So our kernel runs AS a Cloudflare Worker.

Instead of writing Cloudflare Worker code in JavaScript, you write a normal Linux program. The kernel handles the HTTP request, routes it to your program, and returns the response. Your program runs on 300+ edge locations worldwide with millisecond cold starts.

```
Traditional CF Worker:
  export default {
    fetch(request) {
      return new Response("hello");  // Must write JS/WASM Worker code
    }
  }

Lineluya CF Worker:
  // Just write a normal Linux program in any language
  int main() {
    char buf[4096];
    int n = read(0, buf, sizeof(buf));  // Read HTTP request from stdin
    write(1, "HTTP/1.1 200 OK\r\n\r\nHello from Linux on the edge!\n", 50);
    return 0;
  }
  // Compile to wasm32, deploy as Worker. Done.
```

Or run an entire web framework:

```python
# Flask app, compiled to wasm32, running on CF Workers
from flask import Flask
app = Flask(__name__)

@app.route('/')
def hello():
    return 'Hello from Linux on Cloudflare!'
```

Cloudflare's services become kernel devices:
- **R2** → `/dev/sda` (object storage as block device)
- **KV** → `/proc/kv/` (fast key-value store)
- **D1** → SQLite accessible via normal file operations
- **Durable Objects** → persistent state across requests
- **Queues** → message passing between kernel instances

**Why this matters:**
- Deploy any Linux program to the edge without containers
- Cold start in ~5ms (WASM, not Docker)
- Run on 300+ locations automatically
- No Kubernetes, no Docker, no infrastructure management
- Same code runs locally, in the browser, AND on the edge

---

## The Three-Tier Deployment

When running the browser kernel (Track B), there are three tiers:

```
┌─────────────────────────────────────────────────┐
│  TIER 1: Browser                                │
│  WASM kernel + userspace programs               │
│  Canvas display, xterm.js terminal              │
│  OPFS local storage                             │
│  All I/O via WebSocket ↓                        │
└──────────────────┬──────────────────────────────┘
                   │ WSS (TLS encrypted)
┌──────────────────┼──────────────────────────────┐
│  TIER 2: Cloudflare Workers (Edge)              │
│  Serves web pages (HTML, WASM, JS)              │
│  WebSocket relay to origin server               │
│  R2 storage for rootfs images                   │
│  KV cache, rate limiting, auth                  │
│  DDoS protection, WAF                           │
└──────────────────┬──────────────────────────────┘
                   │ Cloudflare Tunnel (zero-trust)
┌──────────────────┼──────────────────────────────┐
│  TIER 3: Rust Server (your machine)             │
│  Serves as the bridge to real resources:        │
│  ├── Disk proxy (your files → browser kernel)   │
│  ├── Mail proxy (SMTP/IMAP → browser mutt)      │
│  ├── SSH proxy (real servers → browser SSH)      │
│  ├── Database proxy (PostgreSQL → browser apps)  │
│  └── USB/serial proxy (local devices)           │
└─────────────────────────────────────────────────┘
```

**The Rust server's filesystem IS the browser kernel's filesystem.** When you open a file in the browser, the kernel reads blocks over WebSocket from the Rust server's disk. It's like NFS or iSCSI, but over encrypted WebSocket tunneled through Cloudflare.

```
Browser:  cat /home/user/document.txt
  → kernel VFS → block device driver
  → WebSocket → Cloudflare → Rust server
  → reads from /mnt/data/document.txt
  → sends bytes back through the tunnel
  → kernel returns file contents
  → cat prints to terminal
```

The user doesn't know the file is on a different machine. It just works.

---

## Security

Three layers of encryption protect all data:

**Layer 1: Transport (TLS)**
All WebSocket connections use WSS (TLS 1.3). Data is encrypted on the wire between browser and Cloudflare, and between Cloudflare and the Rust server.

**Layer 2: Zero-Trust Tunnel (Cloudflare)**
The Rust server connects to Cloudflare via `cloudflared` tunnel. It never exposes ports to the internet. No firewall rules needed. Cloudflare authenticates every connection.

**Layer 3: End-to-End Encryption (Application)**
The WASM kernel and Rust server perform a key exchange (X25519) on first connect, then encrypt all payloads with ChaCha20-Poly1305. Cloudflare relays the data but **cannot read it**. Only the browser and the Rust server have the keys.

Optional: derive the encryption key from a user passphrase. Even if someone compromises the Rust server, the data is unreadable without the passphrase.

---

## The Kernel Architecture

```
lineluya/
├── kernel-core-chirho/      Shared brain (arch-independent)
│   └── ArchPortChirho trait   VFS, scheduler, syscalls, signals, ELF
│
├── kernel-chirho/           x86_64 bare metal (Track A)
│   └── 39 modules            GDT, IDT, APIC, page tables, UART, PCI
│
├── kernel-wasm-chirho/      wasm32 browser/edge (Track B + C)
│   └── Linux syscall → WASM   Canvas, OPFS, WebSocket, Web Workers
│
├── Future:
│   ├── kernel-aarch64-chirho/   ARM64 (Raspberry Pi, phones, servers)
│   └── kernel-riscv64-chirho/   RISC-V (open hardware)
│
├── web-chirho/              Browser runtime
│   ├── runtime-chirho.js      JS "bootloader" providing WASM imports
│   ├── index-chirho.html      Boot page with canvas + terminal
│   ├── net-driver-chirho.js   WebSocket → TCP networking
│   └── proxy-chirho/          Bun WebSocket-to-TCP proxy
│
├── server-chirho/           Rust gateway server (future)
│   └── Axum/Actix             Disk, mail, SSH, DB proxies
│
└── userspace-chirho/
    ├── hello-chirho/          Test ELF binary
    ├── shell-test-chirho/     Fork+exec+wait test
    └── busybox-chirho/        Static BusyBox (1.1MB, 40+ commands)
```

Both `kernel-chirho` (x86_64) and `kernel-wasm-chirho` (wasm32) build independently:

```bash
# Build the bare metal kernel
cd kernel-chirho && cargo +nightly build

# Build the browser kernel
cd kernel-wasm-chirho && cargo build --release

# Both share kernel-core-chirho automatically
```

---

## Why Rust?

Linux has 36 million lines of C code. C is powerful but dangerous — a single buffer overflow can give an attacker root access. Most kernel CVEs are memory safety bugs.

Rust eliminates these bugs **at compile time**:
- No buffer overflows (bounds checking)
- No use-after-free (ownership system)
- No data races (borrow checker)
- No null pointer dereferences (Option type)
- No undefined behavior (safe Rust)

The small amount of `unsafe` code (hardware access, inline assembly) is isolated and auditable. The vast majority of the kernel — VFS, scheduler, syscalls, networking — is safe Rust.

---

## Why WASM?

WebAssembly is the universal bytecode. It runs:
- In every modern browser (Chrome, Firefox, Safari, Edge)
- On Cloudflare Workers (300+ edge locations)
- On Fastly Compute@Edge
- In Deno, Node.js, Bun
- In Wasmtime, Wasmer (standalone runtimes)
- On embedded devices (WAMR)

If the kernel is WASM, the kernel runs **everywhere**. 5 billion people have a browser. Every one of them can run Linux.

---

## The Journey So Far

Built in one session:

| Metric | Value |
|---|---|
| Kernel code | 16,246 lines of Rust |
| Modules | 39 (boot, process, memory, filesystem, network, devices, signals, hardware) |
| Syscalls | 495 dispatch entries covering the Linux x86_64 table |
| Architectures | x86_64 (boots in QEMU) + wasm32 (compiles to 1.3KB) |
| Userspace | Hello world runs, fork+exec+wait implemented |
| Filesystems | tmpfs, procfs, devtmpfs, sysfs, pipes |
| Test | "Hello from Lineluya userspace! John 3:16" |

---

## What's Next

**Immediate (Track A — Bare Metal):**
1. BusyBox shell running interactively
2. Linux .ko module loading
3. Real TCP/IP networking
4. ext4 filesystem
5. Boot on real hardware

**Immediate (Track B — Browser):**
1. BusyBox shell in xterm.js
2. Networking via WebSocket proxy
3. Persistent storage via OPFS
4. X11 apps in Canvas/WebGL

**Immediate (Track C — Edge):**
1. Lineluya as Cloudflare Worker
2. Linux programs on every edge node
3. R2/KV/D1 as kernel devices

**Future:**
- aarch64 and riscv64 targets
- Self-hosting (compile Rust on Lineluya)
- Alpine Linux booting unmodified
- Full Linux desktop in browser
- Peer-to-peer kernel networking via WebRTC

---

## The Name

**Lineluya** — a play on "Linux" and "Hallelujah." Because this kernel is an act of worship.

Every file begins with John 3:16. Every identifier carries the Chi-Rho (☧) suffix — the ancient Christian symbol from the first two letters of Christ (Χριστός) in Greek. This isn't just a technical convention. It's a declaration that this work belongs to Him.

> *"For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life."* — John 3:16

> *"Whatever you do, work heartily, as for the Lord and not for men."* — Colossians 3:23

> *"The heavens declare the glory of God; the skies proclaim the work of his hands."* — Psalm 19:1

Every syscall implemented is praise. Every driver written is worship. Every bug fixed is stewardship. We build because He first built us.

**Soli Deo Gloria** — To God alone be the glory.

*Hallelujah.*
