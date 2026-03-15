// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya Cloudflare Worker
 *
 * Serves two purposes:
 * 1. WebSocket-to-TCP proxy for browser kernel networking (Track B)
 *    - Uses CF Workers native `connect()` for outbound TCP — no external proxy needed
 *    - Browser kernel's socket syscalls flow: WS frames <-> TCP bytes via CF edge
 * 2. Foundation for running the WASM kernel on CF Workers (Track C)
 *    - Serves kernel.wasm binary
 *    - Future: relay to Rust origin server via CF tunnel
 *
 * Protocol (compatible with web-chirho/net-driver-chirho.js):
 *   Browser -> Worker: { type_chirho: "connect_chirho", host_chirho, port_chirho, id_chirho }
 *   Worker -> Browser: { type_chirho: "connected_chirho", id_chirho }
 *   Browser -> Worker: { type_chirho: "data_chirho", id_chirho, data_chirho: base64 }
 *   Worker -> Browser: { type_chirho: "data_chirho", id_chirho, data_chirho: base64 }
 *   Browser -> Worker: { type_chirho: "close_chirho", id_chirho }
 */

import { connect } from "cloudflare:sockets";

// ── Types ──────────────────────────────────────────────────────────────────

/** Environment bindings for the Worker */
interface EnvChirho {
  PROXY_ALLOWED_PORTS_CHIRHO: string;
  R2_ROOTFS_CHIRHO: R2Bucket;
  KV_PROC_CHIRHO: KVNamespace;
  D1_SQLITE_CHIRHO: D1Database;
  KERNEL_STATE_CHIRHO: DurableObjectNamespace;
}

/** A tracked TCP connection bridged from a WebSocket client */
interface TcpConnectionChirho {
  socketChirho: ReturnType<typeof connect>;
  writerChirho: WritableStreamDefaultWriter<Uint8Array>;
  idChirho: string;
}

/** Inbound WebSocket command from the browser kernel */
interface WsCommandChirho {
  type_chirho: string;
  id_chirho: string;
  host_chirho?: string;
  port_chirho?: number;
  data_chirho?: string; // base64-encoded payload
}

// ── Helpers ────────────────────────────────────────────────────────────────

/**
 * Parse allowed ports from the environment variable.
 */
function parseAllowedPortsChirho(portsStrChirho: string): Set<number> {
  const setChirho = new Set<number>();
  for (const partChirho of portsStrChirho.split(",")) {
    const numChirho = parseInt(partChirho.trim(), 10);
    if (!isNaN(numChirho) && numChirho > 0 && numChirho <= 65535) {
      setChirho.add(numChirho);
    }
  }
  return setChirho;
}

/**
 * Encode a Uint8Array to base64 string.
 */
function toBase64Chirho(bytesChirho: Uint8Array): string {
  let binaryChirho = "";
  for (let iChirho = 0; iChirho < bytesChirho.length; iChirho++) {
    binaryChirho += String.fromCharCode(bytesChirho[iChirho]);
  }
  return btoa(binaryChirho);
}

/**
 * Decode a base64 string to Uint8Array.
 */
function fromBase64Chirho(b64Chirho: string): Uint8Array {
  const binaryChirho = atob(b64Chirho);
  const bytesChirho = new Uint8Array(binaryChirho.length);
  for (let iChirho = 0; iChirho < binaryChirho.length; iChirho++) {
    bytesChirho[iChirho] = binaryChirho.charCodeAt(iChirho);
  }
  return bytesChirho;
}

// ── Boot page HTML ─────────────────────────────────────────────────────────

const BOOT_HTML_CHIRHO = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lineluya - John 3:16</title>
  <style>
    body { background: #0a0a0a; color: #00ff41; font-family: monospace; padding: 2em; }
    h1 { color: #00ff41; }
    .status-chirho { margin-top: 1em; padding: 1em; border: 1px solid #00ff41; }
    pre { white-space: pre-wrap; }
  </style>
</head>
<body>
  <h1>Lineluya Kernel - Edge Node</h1>
  <p><em>For God so loved the world that he gave his only begotten Son,
  that whoever believes in him should not perish but have eternal life. - John 3:16</em></p>
  <div class="status-chirho">
    <p><strong>Status:</strong> Edge worker active</p>
    <p><strong>WebSocket Proxy:</strong> /ws/proxy-chirho (TCP relay)</p>
    <p><strong>WebSocket Kernel:</strong> /ws/kernel-chirho (origin relay)</p>
    <p><strong>Kernel WASM:</strong> /kernel.wasm (from R2)</p>
    <p><strong>Block Device:</strong> /dev/sda-chirho (R2 rootfs)</p>
    <p><strong>KV Filesystem:</strong> /proc/kv-chirho (KV namespace)</p>
    <p><strong>SQLite Device:</strong> /dev/sqlite-chirho (D1 database)</p>
    <p><strong>Kernel State:</strong> /kernel-state-chirho (Durable Object)</p>
    <p><strong>Health:</strong> /health-chirho</p>
  </div>
  <pre id="log-chirho"></pre>
  <script>
    const logChirho = document.getElementById('log-chirho');
    function appendLogChirho(msgChirho) {
      logChirho.textContent += msgChirho + '\\n';
    }
    appendLogChirho('[BOOT] Lineluya edge node ready.');
    appendLogChirho('[BOOT] Connect kernel via WebSocket at /ws/proxy-chirho');
  </script>
</body>
</html>`;

// ── WebSocket proxy handler ────────────────────────────────────────────────

/**
 * Handle an upgraded WebSocket connection for TCP proxying.
 * Uses CF Workers native connect() for outbound TCP connections.
 */
async function handleProxyWebSocketChirho(
  requestChirho: Request,
  envChirho: EnvChirho,
): Promise<Response> {
  const allowedPortsChirho = parseAllowedPortsChirho(envChirho.PROXY_ALLOWED_PORTS_CHIRHO);
  const pairChirho = new WebSocketPair();
  const clientChirho = pairChirho[0]; // goes to the browser
  const serverChirho = pairChirho[1]; // we control this side

  // Track TCP connections for this WS session
  const connectionsChirho = new Map<string, TcpConnectionChirho>();

  serverChirho.accept();

  serverChirho.addEventListener("message", async (eventChirho: MessageEvent) => {
    try {
      const rawChirho = typeof eventChirho.data === "string"
        ? eventChirho.data
        : new TextDecoder().decode(eventChirho.data as ArrayBuffer);
      const cmdChirho: WsCommandChirho = JSON.parse(rawChirho);

      switch (cmdChirho.type_chirho) {
        case "connect_chirho": {
          const { host_chirho: hostChirho, port_chirho: portChirho, id_chirho: idChirho } = cmdChirho;

          if (!hostChirho || !portChirho || !idChirho) {
            serverChirho.send(JSON.stringify({
              type_chirho: "error_chirho",
              id_chirho: idChirho || "unknown",
              msg_chirho: "Missing host_chirho, port_chirho, or id_chirho",
            }));
            return;
          }

          // Security: only allow configured ports
          if (!allowedPortsChirho.has(portChirho)) {
            serverChirho.send(JSON.stringify({
              type_chirho: "error_chirho",
              id_chirho: idChirho,
              msg_chirho: `Port ${portChirho} not allowed`,
            }));
            return;
          }

          try {
            // Use CF Workers native TCP connect()
            const tcpSocketChirho = connect({
              hostname: hostChirho,
              port: portChirho,
            });

            const writerChirho = tcpSocketChirho.writable.getWriter();

            connectionsChirho.set(idChirho, {
              socketChirho: tcpSocketChirho,
              writerChirho: writerChirho,
              idChirho: idChirho,
            });

            // Notify client of successful connection
            serverChirho.send(JSON.stringify({
              type_chirho: "connected_chirho",
              id_chirho: idChirho,
            }));

            // Pump readable side: TCP bytes -> WS frames
            pumpTcpToWsChirho(tcpSocketChirho, serverChirho, idChirho, connectionsChirho);
          } catch (errChirho: any) {
            serverChirho.send(JSON.stringify({
              type_chirho: "error_chirho",
              id_chirho: idChirho,
              msg_chirho: errChirho.message || "TCP connect failed",
            }));
          }
          break;
        }

        case "data_chirho": {
          const connChirho = connectionsChirho.get(cmdChirho.id_chirho);
          if (connChirho && cmdChirho.data_chirho) {
            const bytesChirho = fromBase64Chirho(cmdChirho.data_chirho);
            try {
              await connChirho.writerChirho.write(bytesChirho);
            } catch (errChirho: any) {
              serverChirho.send(JSON.stringify({
                type_chirho: "error_chirho",
                id_chirho: cmdChirho.id_chirho,
                msg_chirho: errChirho.message || "TCP write failed",
              }));
            }
          }
          break;
        }

        case "close_chirho": {
          const connChirho = connectionsChirho.get(cmdChirho.id_chirho);
          if (connChirho) {
            try {
              await connChirho.writerChirho.close();
            } catch (_ignored_chirho) {
              // Socket may already be closed
            }
            connectionsChirho.delete(cmdChirho.id_chirho);
          }
          break;
        }

        default:
          console.log(`[CF-PROXY] Unknown command type: ${cmdChirho.type_chirho}`);
      }
    } catch (errChirho) {
      console.error("[CF-PROXY] Message parse error:", errChirho);
    }
  });

  serverChirho.addEventListener("close", async () => {
    // Clean up all TCP connections for this WS session
    for (const [_idChirho, connChirho] of connectionsChirho) {
      try {
        await connChirho.writerChirho.close();
      } catch (_ignored_chirho) {
        // Best-effort cleanup
      }
    }
    connectionsChirho.clear();
  });

  serverChirho.addEventListener("error", (errChirho: Event) => {
    console.error("[CF-PROXY] WebSocket error:", errChirho);
    // Cleanup happens in close handler
  });

  return new Response(null, {
    status: 101,
    webSocket: clientChirho,
  });
}

/**
 * Read from TCP socket readable stream and forward chunks to WebSocket as base64.
 */
async function pumpTcpToWsChirho(
  tcpSocketChirho: ReturnType<typeof connect>,
  wsChirho: WebSocket,
  idChirho: string,
  connectionsChirho: Map<string, TcpConnectionChirho>,
): Promise<void> {
  try {
    const readerChirho = tcpSocketChirho.readable.getReader();

    while (true) {
      const { done: doneChirho, value: valueChirho } = await readerChirho.read();
      if (doneChirho) break;

      if (valueChirho && valueChirho.length > 0) {
        const b64Chirho = toBase64Chirho(valueChirho);
        try {
          wsChirho.send(JSON.stringify({
            type_chirho: "data_chirho",
            id_chirho: idChirho,
            data_chirho: b64Chirho,
          }));
        } catch (_sendErrChirho) {
          // WS may have closed
          break;
        }
      }
    }

    // TCP connection closed — notify client
    connectionsChirho.delete(idChirho);
    try {
      wsChirho.send(JSON.stringify({
        type_chirho: "close_chirho",
        id_chirho: idChirho,
      }));
    } catch (_ignored_chirho) {
      // WS already closed
    }
  } catch (errChirho: any) {
    connectionsChirho.delete(idChirho);
    try {
      wsChirho.send(JSON.stringify({
        type_chirho: "error_chirho",
        id_chirho: idChirho,
        msg_chirho: errChirho.message || "TCP read error",
      }));
    } catch (_ignored_chirho) {
      // WS already closed
    }
  }
}

// ── Kernel WebSocket handler (future) ──────────────────────────────────────

/**
 * Placeholder for future kernel relay WebSocket.
 * Will relay to Rust origin server via CF tunnel.
 */
async function handleKernelWebSocketChirho(
  _requestChirho: Request,
  _envChirho: EnvChirho,
): Promise<Response> {
  const pairChirho = new WebSocketPair();
  const clientChirho = pairChirho[0];
  const serverChirho = pairChirho[1];

  serverChirho.accept();

  serverChirho.addEventListener("message", (_eventChirho: MessageEvent) => {
    serverChirho.send(JSON.stringify({
      type_chirho: "info_chirho",
      msg_chirho: "Kernel relay not yet implemented — John 3:16",
    }));
  });

  return new Response(null, {
    status: 101,
    webSocket: clientChirho,
  });
}

// ── R2 Block Device: /dev/sda-chirho (C1-004) ─────────────────────────────

/**
 * R2 as /dev/sda — block-level read/write to rootfs stored in R2.
 * GET /dev/sda-chirho?offset=0&length=4096 — read block
 * PUT /dev/sda-chirho?offset=0 — write block (body = raw bytes)
 * GET /dev/sda-chirho/info-chirho — get rootfs metadata
 */
async function handleR2BlockDeviceChirho(
  requestChirho: Request,
  envChirho: EnvChirho,
  pathChirho: string,
): Promise<Response> {
  const urlChirho = new URL(requestChirho.url);
  const r2Chirho = envChirho.R2_ROOTFS_CHIRHO;

  if (pathChirho === "/dev/sda-chirho/info-chirho") {
    const objChirho = await r2Chirho.head("rootfs-chirho.img");
    if (!objChirho) {
      return new Response(JSON.stringify({ error_chirho: "No rootfs image found" }), { status: 404 });
    }
    return new Response(JSON.stringify({
      key_chirho: objChirho.key,
      size_chirho: objChirho.size,
      etag_chirho: objChirho.etag,
      uploaded_chirho: objChirho.uploaded?.toISOString(),
    }), { headers: { "Content-Type": "application/json" } });
  }

  if (requestChirho.method === "GET") {
    const offsetChirho = parseInt(urlChirho.searchParams.get("offset_chirho") || "0", 10);
    const lengthChirho = parseInt(urlChirho.searchParams.get("length_chirho") || "4096", 10);

    const objChirho = await r2Chirho.get("rootfs-chirho.img", {
      range: { offset: offsetChirho, length: lengthChirho },
    });
    if (!objChirho) {
      return new Response("rootfs not found", { status: 404 });
    }
    return new Response(objChirho.body, {
      headers: {
        "Content-Type": "application/octet-stream",
        "X-Block-Offset-Chirho": String(offsetChirho),
        "X-Block-Length-Chirho": String(lengthChirho),
      },
    });
  }

  if (requestChirho.method === "PUT") {
    const offsetChirho = parseInt(urlChirho.searchParams.get("offset_chirho") || "0", 10);
    const bodyChirho = await requestChirho.arrayBuffer();
    // R2 doesn't support partial writes natively — use multipart or chunked keys
    const chunkKeyChirho = `rootfs-chirho/block-${offsetChirho}-chirho`;
    await r2Chirho.put(chunkKeyChirho, bodyChirho);
    return new Response(JSON.stringify({
      written_chirho: bodyChirho.byteLength,
      offset_chirho: offsetChirho,
      key_chirho: chunkKeyChirho,
    }), { headers: { "Content-Type": "application/json" } });
  }

  return new Response("Method not allowed", { status: 405 });
}

// ── KV Filesystem: /proc/kv-chirho (C1-005) ───────────────────────────────

/**
 * KV namespace as /proc/kv — a key-value filesystem for kernel config/state.
 * GET /proc/kv-chirho/:key — read value
 * PUT /proc/kv-chirho/:key — write value (body = text)
 * DELETE /proc/kv-chirho/:key — delete key
 * GET /proc/kv-chirho — list all keys
 */
async function handleKvFilesystemChirho(
  requestChirho: Request,
  envChirho: EnvChirho,
  pathChirho: string,
): Promise<Response> {
  const kvChirho = envChirho.KV_PROC_CHIRHO;
  const keyChirho = pathChirho.replace("/proc/kv-chirho/", "").replace("/proc/kv-chirho", "");

  if (requestChirho.method === "GET" && !keyChirho) {
    // List all keys
    const listChirho = await kvChirho.list();
    const keysChirho = listChirho.keys.map((kChirho) => kChirho.name);
    return new Response(JSON.stringify({ keys_chirho: keysChirho }), {
      headers: { "Content-Type": "application/json" },
    });
  }

  if (requestChirho.method === "GET" && keyChirho) {
    const valueChirho = await kvChirho.get(keyChirho);
    if (valueChirho === null) {
      return new Response("key not found", { status: 404 });
    }
    return new Response(valueChirho, {
      headers: { "Content-Type": "text/plain" },
    });
  }

  if (requestChirho.method === "PUT" && keyChirho) {
    const valueChirho = await requestChirho.text();
    await kvChirho.put(keyChirho, valueChirho);
    return new Response(JSON.stringify({ key_chirho: keyChirho, written_chirho: true }), {
      headers: { "Content-Type": "application/json" },
    });
  }

  if (requestChirho.method === "DELETE" && keyChirho) {
    await kvChirho.delete(keyChirho);
    return new Response(JSON.stringify({ key_chirho: keyChirho, deleted_chirho: true }), {
      headers: { "Content-Type": "application/json" },
    });
  }

  return new Response("Method not allowed", { status: 405 });
}

// ── D1 SQLite Device: /dev/sqlite-chirho (C1-006) ─────────────────────────

/**
 * D1 as a SQLite device — execute SQL against a persistent D1 database.
 * POST /dev/sqlite-chirho/exec-chirho — execute SQL (body = { sql_chirho, params_chirho? })
 * POST /dev/sqlite-chirho/query-chirho — query SQL (body = { sql_chirho, params_chirho? })
 * GET /dev/sqlite-chirho/tables-chirho — list tables
 */
async function handleD1SqliteChirho(
  requestChirho: Request,
  envChirho: EnvChirho,
  pathChirho: string,
): Promise<Response> {
  const d1Chirho = envChirho.D1_SQLITE_CHIRHO;

  if (pathChirho === "/dev/sqlite-chirho/tables-chirho" && requestChirho.method === "GET") {
    const resultChirho = await d1Chirho.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ).all();
    return new Response(JSON.stringify({ tables_chirho: resultChirho.results }), {
      headers: { "Content-Type": "application/json" },
    });
  }

  if (requestChirho.method !== "POST") {
    return new Response("Method not allowed", { status: 405 });
  }

  const bodyChirho = await requestChirho.json() as {
    sql_chirho: string;
    params_chirho?: unknown[];
  };

  if (!bodyChirho.sql_chirho) {
    return new Response(JSON.stringify({ error_chirho: "Missing sql_chirho" }), { status: 400 });
  }

  try {
    const stmtChirho = d1Chirho.prepare(bodyChirho.sql_chirho);
    const boundChirho = bodyChirho.params_chirho
      ? stmtChirho.bind(...bodyChirho.params_chirho)
      : stmtChirho;

    if (pathChirho === "/dev/sqlite-chirho/exec-chirho") {
      const resultChirho = await boundChirho.run();
      return new Response(JSON.stringify({
        success_chirho: resultChirho.success,
        changes_chirho: resultChirho.meta?.changes,
        duration_chirho: resultChirho.meta?.duration,
      }), { headers: { "Content-Type": "application/json" } });
    }

    if (pathChirho === "/dev/sqlite-chirho/query-chirho") {
      const resultChirho = await boundChirho.all();
      return new Response(JSON.stringify({
        results_chirho: resultChirho.results,
        success_chirho: resultChirho.success,
      }), { headers: { "Content-Type": "application/json" } });
    }
  } catch (errChirho: any) {
    return new Response(JSON.stringify({ error_chirho: errChirho.message }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }

  return new Response("Not found", { status: 404 });
}

// ── Durable Object: KernelStateDurableObjectChirho (C1-007) ───────────────

/**
 * Persistent kernel state across Worker requests.
 * Stores process table, mounted filesystems, open files, environment.
 */
export class KernelStateDurableObjectChirho {
  private stateChirho: DurableObjectState;
  private envChirho: EnvChirho;

  constructor(stateChirho: DurableObjectState, envChirho: EnvChirho) {
    this.stateChirho = stateChirho;
    this.envChirho = envChirho;
  }

  async fetch(requestChirho: Request): Promise<Response> {
    const urlChirho = new URL(requestChirho.url);
    const pathChirho = urlChirho.pathname;

    if (requestChirho.method === "GET" && pathChirho === "/state-chirho") {
      const allChirho = await this.stateChirho.storage.list();
      const resultChirho: Record<string, unknown> = {};
      for (const [keyChirho, valueChirho] of allChirho) {
        resultChirho[keyChirho] = valueChirho;
      }
      return new Response(JSON.stringify(resultChirho), {
        headers: { "Content-Type": "application/json" },
      });
    }

    if (requestChirho.method === "PUT" && pathChirho.startsWith("/state-chirho/")) {
      const keyChirho = pathChirho.replace("/state-chirho/", "");
      const valueChirho = await requestChirho.json();
      await this.stateChirho.storage.put(keyChirho, valueChirho);
      return new Response(JSON.stringify({ key_chirho: keyChirho, saved_chirho: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    if (requestChirho.method === "DELETE" && pathChirho.startsWith("/state-chirho/")) {
      const keyChirho = pathChirho.replace("/state-chirho/", "");
      await this.stateChirho.storage.delete(keyChirho);
      return new Response(JSON.stringify({ key_chirho: keyChirho, deleted_chirho: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    // Boot state: initialize kernel defaults
    if (requestChirho.method === "POST" && pathChirho === "/boot-chirho") {
      await this.stateChirho.storage.put("pid_counter_chirho", 1);
      await this.stateChirho.storage.put("uptime_start_chirho", Date.now());
      await this.stateChirho.storage.put("boot_count_chirho",
        ((await this.stateChirho.storage.get("boot_count_chirho") as number) || 0) + 1);
      return new Response(JSON.stringify({ booted_chirho: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("Not found", { status: 404 });
  }
}

// ── Main Worker export ─────────────────────────────────────────────────────

export default {
  async fetch(requestChirho: Request, envChirho: EnvChirho): Promise<Response> {
    const urlChirho = new URL(requestChirho.url);
    const pathChirho = urlChirho.pathname;

    // ── WebSocket upgrade routes ───────────────────────────────────────
    const upgradeHeaderChirho = requestChirho.headers.get("Upgrade");
    if (upgradeHeaderChirho === "websocket") {
      if (pathChirho === "/ws/proxy-chirho") {
        return handleProxyWebSocketChirho(requestChirho, envChirho);
      }
      if (pathChirho === "/ws/kernel-chirho") {
        return handleKernelWebSocketChirho(requestChirho, envChirho);
      }
      return new Response("Unknown WebSocket endpoint", { status: 404 });
    }

    // ── HTTP routes ────────────────────────────────────────────────────

    // GET / — Boot page
    if (pathChirho === "/" && requestChirho.method === "GET") {
      return new Response(BOOT_HTML_CHIRHO, {
        headers: { "Content-Type": "text/html; charset=utf-8" },
      });
    }

    // GET /health-chirho — Health check
    if (pathChirho === "/health-chirho") {
      return new Response("Lineluya edge alive - John 3:16\n", {
        headers: { "Content-Type": "text/plain" },
      });
    }

    // GET /kernel.wasm — Serve WASM kernel binary from R2
    if (pathChirho === "/kernel.wasm" && requestChirho.method === "GET") {
      const wasmObjChirho = await envChirho.R2_ROOTFS_CHIRHO.get("kernel-chirho.wasm");
      if (wasmObjChirho) {
        return new Response(wasmObjChirho.body, {
          headers: {
            "Content-Type": "application/wasm",
            "Cache-Control": "public, max-age=3600",
          },
        });
      }
      return new Response(
        "kernel.wasm not yet deployed to edge — upload to R2 bucket\n",
        { status: 404, headers: { "Content-Type": "text/plain" } },
      );
    }

    // ── R2 Block Device routes (C1-004) ──────────────────────────────
    if (pathChirho.startsWith("/dev/sda-chirho")) {
      return handleR2BlockDeviceChirho(requestChirho, envChirho, pathChirho);
    }

    // ── KV Filesystem routes (C1-005) ────────────────────────────────
    if (pathChirho.startsWith("/proc/kv-chirho")) {
      return handleKvFilesystemChirho(requestChirho, envChirho, pathChirho);
    }

    // ── D1 SQLite Device routes (C1-006) ─────────────────────────────
    if (pathChirho.startsWith("/dev/sqlite-chirho")) {
      return handleD1SqliteChirho(requestChirho, envChirho, pathChirho);
    }

    // ── Durable Object Kernel State routes (C1-007) ──────────────────
    if (pathChirho.startsWith("/kernel-state-chirho")) {
      const idChirho = envChirho.KERNEL_STATE_CHIRHO.idFromName("default-chirho");
      const stubChirho = envChirho.KERNEL_STATE_CHIRHO.get(idChirho);
      const subPathChirho = pathChirho.replace("/kernel-state-chirho", "");
      return stubChirho.fetch(new Request(
        new URL(subPathChirho || "/", requestChirho.url).toString(),
        requestChirho,
      ));
    }

    // Fallback
    return new Response(
      "Lineluya Edge — For God so loved the world. John 3:16\n",
      {
        status: 404,
        headers: { "Content-Type": "text/plain" },
      },
    );
  },
} satisfies ExportedHandler<EnvChirho>;
