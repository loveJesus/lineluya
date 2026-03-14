// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya WebSocket-to-TCP Proxy
 *
 * Bridges the browser's WASM kernel to real network services.
 * The kernel's socket syscalls (connect, send, recv) become WebSocket
 * messages to this proxy, which opens real TCP connections.
 *
 * Protocol:
 *   Browser → Proxy: { type: "connect", host, port, id }
 *   Proxy → Browser: { type: "connected", id } or { type: "error", id, msg }
 *   Browser → Proxy: { type: "data", id, data: base64 }
 *   Proxy → Browser: { type: "data", id, data: base64 }
 *   Browser → Proxy: { type: "close", id }
 *
 * Run with: bun run web-chirho/proxy-chirho/index-chirho.ts
 */

const PORT_CHIRHO = parseInt(process.env.PROXY_PORT_CHIRHO || "8765");
const ALLOWED_PORTS_CHIRHO = new Set([
  22, 80, 443, 8080, 8443, 3000, 5000, // Common services
]);

// Track active TCP connections per WebSocket client
type ConnectionChirho = {
  socketChirho: Awaited<ReturnType<typeof Bun.connect>>;
  idChirho: string;
};

const connectionsChirho = new Map<string, ConnectionChirho>();

const serverChirho = Bun.serve({
  port: PORT_CHIRHO,
  fetch(reqChirho, serverChirho) {
    const urlChirho = new URL(reqChirho.url);

    // Health check
    if (urlChirho.pathname === "/health-chirho") {
      return new Response("Lineluya proxy alive - John 3:16\n");
    }

    // WebSocket upgrade
    if (serverChirho.upgrade(reqChirho)) {
      return; // Upgraded
    }

    return new Response("Lineluya Network Proxy\nWebSocket endpoint: ws://localhost:" + PORT_CHIRHO + "\n");
  },

  websocket: {
    open(wsChirho) {
      console.log("[PROXY] Client connected");
    },

    async message(wsChirho, msgChirho) {
      try {
        const cmdChirho = JSON.parse(typeof msgChirho === "string" ? msgChirho : new TextDecoder().decode(msgChirho));

        switch (cmdChirho.type_chirho) {
          case "connect_chirho": {
            const { host_chirho, port_chirho, id_chirho } = cmdChirho;

            // Security: only allow specific ports
            if (!ALLOWED_PORTS_CHIRHO.has(port_chirho)) {
              wsChirho.send(JSON.stringify({
                type_chirho: "error_chirho",
                id_chirho,
                msg_chirho: `Port ${port_chirho} not allowed`,
              }));
              return;
            }

            console.log(`[PROXY] Connect: ${host_chirho}:${port_chirho} (${id_chirho})`);

            try {
              const tcpChirho = await Bun.connect({
                hostname: host_chirho,
                port: port_chirho,
                socket: {
                  data(socketChirho, dataChirho) {
                    // Forward TCP data to WebSocket as base64
                    const b64Chirho = Buffer.from(dataChirho).toString("base64");
                    wsChirho.send(JSON.stringify({
                      type_chirho: "data_chirho",
                      id_chirho,
                      data_chirho: b64Chirho,
                    }));
                  },
                  close() {
                    connectionsChirho.delete(id_chirho);
                    wsChirho.send(JSON.stringify({
                      type_chirho: "close_chirho",
                      id_chirho,
                    }));
                  },
                  error(socketChirho, errChirho) {
                    wsChirho.send(JSON.stringify({
                      type_chirho: "error_chirho",
                      id_chirho,
                      msg_chirho: errChirho.message,
                    }));
                  },
                },
              });

              connectionsChirho.set(id_chirho, { socketChirho: tcpChirho, idChirho: id_chirho });

              wsChirho.send(JSON.stringify({
                type_chirho: "connected_chirho",
                id_chirho,
              }));
            } catch (errChirho: any) {
              wsChirho.send(JSON.stringify({
                type_chirho: "error_chirho",
                id_chirho,
                msg_chirho: errChirho.message,
              }));
            }
            break;
          }

          case "data_chirho": {
            const connChirho = connectionsChirho.get(cmdChirho.id_chirho);
            if (connChirho) {
              const bytesChirho = Buffer.from(cmdChirho.data_chirho, "base64");
              connChirho.socketChirho.write(bytesChirho);
            }
            break;
          }

          case "close_chirho": {
            const connChirho = connectionsChirho.get(cmdChirho.id_chirho);
            if (connChirho) {
              connChirho.socketChirho.end();
              connectionsChirho.delete(cmdChirho.id_chirho);
            }
            break;
          }
        }
      } catch (errChirho) {
        console.error("[PROXY] Error:", errChirho);
      }
    },

    close(wsChirho) {
      // Clean up all connections for this client
      for (const [idChirho, connChirho] of connectionsChirho) {
        connChirho.socketChirho.end();
      }
      connectionsChirho.clear();
      console.log("[PROXY] Client disconnected");
    },
  },
});

console.log(`[PROXY] Lineluya network proxy listening on ws://localhost:${PORT_CHIRHO}`);
console.log(`[PROXY] Allowed ports: ${[...ALLOWED_PORTS_CHIRHO].join(", ")}`);
console.log("[PROXY] For God so loved the world - John 3:16");
