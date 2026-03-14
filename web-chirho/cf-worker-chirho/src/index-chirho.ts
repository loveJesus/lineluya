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
    <p><strong>WebSocket Proxy:</strong> /ws/proxy-chirho</p>
    <p><strong>Kernel WASM:</strong> /kernel.wasm</p>
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

    // GET /kernel.wasm — Serve WASM kernel binary (placeholder: will serve from R2)
    if (pathChirho === "/kernel.wasm" && requestChirho.method === "GET") {
      // TODO: Serve from R2 bucket or KV when kernel binary is deployed
      return new Response(
        "kernel.wasm not yet deployed to edge — build with `make wasm` first\n",
        {
          status: 404,
          headers: { "Content-Type": "text/plain" },
        },
      );
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
