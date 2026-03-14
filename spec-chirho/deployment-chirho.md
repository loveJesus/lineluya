# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. — John 3:16

# Lineluya Deployment Architecture

## Three-Tier Stack

```
┌─────────────────────────────────────────────────────────┐
│  BROWSER (Tier 1)                                       │
│  ┌─────────────────────────────────────────────────┐    │
│  │  Lineluya WASM Kernel                           │    │
│  │  ├── Linux syscall dispatch                     │    │
│  │  ├── VFS (tmpfs, procfs in memory)              │    │
│  │  ├── Process scheduler (Web Workers)            │    │
│  │  ├── Canvas/WebGL framebuffer                   │    │
│  │  └── xterm.js terminal                          │    │
│  └──────────────┬──────────────────────────────────┘    │
│                 │ WebSocket                              │
└─────────────────┼───────────────────────────────────────┘
                  │
┌─────────────────┼───────────────────────────────────────┐
│  CLOUDFLARE WORKERS (Tier 2 — Edge)                     │
│                 │                                        │
│  ┌──────────────┴──────────────────────────────────┐    │
│  │  Edge Proxy Worker                              │    │
│  │  ├── WebSocket ↔ TCP routing                    │    │
│  │  ├── Auth / session management                  │    │
│  │  ├── R2 storage (static rootfs images)          │    │
│  │  ├── KV cache (DNS, metadata)                   │    │
│  │  ├── Durable Objects (persistent state)         │    │
│  │  └── Rate limiting / security                   │    │
│  └──────────────┬──────────────────────────────────┘    │
│                 │ fetch / WebSocket                      │
└─────────────────┼───────────────────────────────────────┘
                  │
┌─────────────────┼───────────────────────────────────────┐
│  RUST SERVER (Tier 3 — Origin)                          │
│                 │                                        │
│  ┌──────────────┴──────────────────────────────────┐    │
│  │  Lineluya Gateway Server (Rust + Axum/Actix)    │    │
│  │  ├── Serves web pages (kernel boot UI)          │    │
│  │  ├── WebSocket endpoint for browser kernels     │    │
│  │  ├── TCP proxy (connect to any TCP service)     │    │
│  │  ├── Disk proxy (mount real drives, serve blocks)│   │
│  │  ├── Mail proxy (SMTP/IMAP ↔ WebSocket)         │    │
│  │  ├── SSH proxy (SSH ↔ WebSocket)                │    │
│  │  ├── Database proxy (PostgreSQL/MySQL ↔ WS)     │    │
│  │  ├── USB/serial proxy (local devices ↔ WS)      │    │
│  │  └── File sync (local FS ↔ browser OPFS)        │    │
│  └─────────────────────────────────────────────────┘    │
│                 │                                        │
│  ┌──────────────┴──────────────────────────────────┐    │
│  │  Real Resources                                 │    │
│  │  ├── /dev/sda (local disk)                      │    │
│  │  ├── Mail server (Postfix/Dovecot)              │    │
│  │  ├── SSH server (OpenSSH)                       │    │
│  │  ├── PostgreSQL / MySQL                         │    │
│  │  ├── S3 / MinIO object storage                  │    │
│  │  └── USB devices, serial ports                  │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

## Tier 1: Browser (WASM Kernel)

The kernel runs as WebAssembly in the browser tab. It provides:
- Full Linux syscall interface to WASM userspace programs
- In-memory filesystems (tmpfs, procfs, devtmpfs)
- Process management via Web Workers
- Display via Canvas/WebGL
- All I/O routed through WebSocket to Tier 2/3

**No server needed for basic operation** — the kernel boots and runs programs locally. The server tiers are for I/O that requires real network/disk access.

## Tier 2: Cloudflare Workers (Edge)

Runs on Cloudflare's global edge network (~300 locations). Handles:

### WebSocket Routing
```
Browser → wss://kernel.lineluya.com/ws → Worker → origin server
```
The Worker authenticates the connection, applies rate limits, and routes WebSocket frames to the appropriate origin backend.

### Static Assets (R2)
```
Browser → https://kernel.lineluya.com/rootfs/alpine.img → R2
```
Pre-built rootfs images (Alpine, Debian) stored in R2. The browser kernel mounts these as read-only root via fetch(), caching blocks in OPFS.

### Edge Caching (KV)
- DNS resolution cache
- Filesystem metadata cache
- Session tokens

### Durable Objects
- Per-user persistent state (home directory sync)
- Collaborative sessions (shared kernel instances)

### Deployment
```bash
# Deploy the edge proxy
cd web-chirho/cf-worker-chirho
wrangler deploy
```

## Tier 3: Rust Server (Origin)

A Rust server (Axum or Actix-web) that provides the "heavy" I/O:

### Web Server
Serves the boot page, WASM kernel binary, and static assets:
```
GET /                    → Boot page (index-chirho.html)
GET /kernel.wasm         → WASM kernel binary
GET /runtime.js          → JS runtime
GET /xterm.js            → Terminal emulator
```

### Proxy Endpoints
Each proxy type is a WebSocket endpoint:

```
ws://server/proxy/tcp    → Generic TCP proxy
  Client sends: { host, port }
  Bidirectional byte stream follows

ws://server/proxy/disk   → Block device proxy
  Client sends: { op: "read", offset, len }
  Server responds with block data
  Client sends: { op: "write", offset, data }

ws://server/proxy/mail   → SMTP/IMAP proxy
  Client sends SMTP/IMAP commands
  Server relays to mail server

ws://server/proxy/ssh    → SSH proxy
  Client sends SSH protocol frames
  Server relays to sshd

ws://server/proxy/db     → Database proxy
  Client sends SQL queries (PostgreSQL wire protocol)
  Server relays to database
```

### Disk Proxy Detail
The Rust server can mount local drives and expose them as network block devices:
```rust
// Serve blocks from a local disk image or real device
async fn handle_disk_proxy(ws: WebSocket, disk_path: &str) {
    let file = File::open(disk_path).await?;
    loop {
        let msg = ws.recv().await?;
        match msg.op {
            "read" => {
                file.seek(msg.offset).await?;
                let data = file.read(msg.len).await?;
                ws.send(data).await?;
            }
            "write" => {
                file.seek(msg.offset).await?;
                file.write(&msg.data).await?;
                ws.send("ok").await?;
            }
        }
    }
}
```

### Mail Proxy Detail
Connect the browser kernel's mail client to a real mail server:
```
Browser: mutt (compiled to wasm32)
  → sys_connect("mail.example.com", 993)
  → kernel routes through WebSocket
  → Rust server opens real TCP to mail.example.com:993
  → IMAP traffic flows bidirectionally
```

### SSH Proxy Detail
```
Browser: ssh (compiled to wasm32)
  → sys_connect("server.com", 22)
  → WebSocket → Rust server → real TCP to server.com:22
  → Full SSH session in the browser
```

## Communication Protocol

All WebSocket messages use a simple envelope:

```json
{
  "type_chirho": "connect_chirho | data_chirho | close_chirho | error_chirho",
  "channel_chirho": "tcp | disk | mail | ssh | db",
  "id_chirho": "unique-connection-id",
  "data_chirho": "base64-encoded-bytes",
  "meta_chirho": { "host_chirho": "...", "port_chirho": 22 }
}
```

## Security Model

1. **Browser sandbox** — WASM kernel cannot access anything outside its sandbox
2. **Cloudflare WAF** — DDoS protection, bot filtering at the edge
3. **Auth** — User authenticates to the Worker, gets a session token
4. **Allowlists** — Rust server only proxies to configured destinations
5. **Encryption** — All WebSocket connections over WSS (TLS)
6. **CORS/COOP/COEP** — Required headers for SharedArrayBuffer (SMP)

## Quick Start

```bash
# 1. Start the Rust server (serves pages + proxies I/O)
cd server-chirho
cargo run -- --port 8080 --disk /path/to/rootfs.img

# 2. Deploy Cloudflare Worker (optional, for production)
cd web-chirho/cf-worker-chirho
wrangler deploy

# 3. Open browser
open http://localhost:8080
# → Lineluya boots, BusyBox shell appears in xterm.js
# → Files persist in OPFS, networking via WS proxy
```

## Future: Peer-to-Peer

WebRTC data channels could enable direct browser-to-browser connections:
- Two Lineluya kernels in different browsers communicate directly
- No proxy server needed for kernel-to-kernel networking
- Distributed computing across browser tabs worldwide

---

*"Whatever you do, work heartily, as for the Lord and not for men."* — Colossians 3:23
