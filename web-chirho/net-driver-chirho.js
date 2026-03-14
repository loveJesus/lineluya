// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya WASM Network Driver
 *
 * This replaces a NIC driver. The browser kernel's socket syscalls
 * (connect, send, recv) flow through a WebSocket to a TCP proxy server.
 *
 * The browser CANNOT make raw TCP connections (security sandbox),
 * so we tunnel through WebSocket → proxy → real TCP.
 *
 * Syscall flow:
 *   Kernel: sys_socket()     → allocate handle
 *   Kernel: sys_connect()    → ws.send({type:"connect", host, port})
 *   Proxy:  open TCP          → ws.send({type:"connected"})
 *   Kernel: sys_send(data)   → ws.send({type:"data", data})
 *   Proxy:  TCP send          → TCP recv → ws.send({type:"data", data})
 *   Kernel: sys_recv()       → return buffered data
 */

class WasmNetDriverChirho {
  constructor(proxyUrlChirho = 'ws://localhost:8765') {
    this.proxyUrlChirho = proxyUrlChirho;
    this.wsChirho = null;
    this.socketsChirho = new Map(); // id -> { state, recvBuffer, resolve }
    this.nextIdChirho = 1;
    this.connectedChirho = false;
  }

  /** Connect to the proxy server */
  async initChirho() {
    return new Promise((resolveChirho, rejectChirho) => {
      this.wsChirho = new WebSocket(this.proxyUrlChirho);

      this.wsChirho.onopen = () => {
        this.connectedChirho = true;
        console.log('[NET] Connected to proxy:', this.proxyUrlChirho);
        resolveChirho();
      };

      this.wsChirho.onmessage = (evtChirho) => {
        const msgChirho = JSON.parse(evtChirho.data);
        this.handleProxyMessageChirho(msgChirho);
      };

      this.wsChirho.onerror = (errChirho) => {
        console.error('[NET] Proxy error:', errChirho);
        rejectChirho(errChirho);
      };

      this.wsChirho.onclose = () => {
        this.connectedChirho = false;
        console.log('[NET] Proxy disconnected');
      };
    });
  }

  handleProxyMessageChirho(msgChirho) {
    const sockChirho = this.socketsChirho.get(msgChirho.id_chirho);
    if (!sockChirho) return;

    switch (msgChirho.type_chirho) {
      case 'connected_chirho':
        sockChirho.stateChirho = 'connected';
        if (sockChirho.connectResolveChirho) {
          sockChirho.connectResolveChirho(0); // Success
          sockChirho.connectResolveChirho = null;
        }
        break;

      case 'data_chirho':
        const bytesChirho = Uint8Array.from(atob(msgChirho.data_chirho), c => c.charCodeAt(0));
        sockChirho.recvBufferChirho.push(bytesChirho);
        // Wake up any pending recv
        if (sockChirho.recvResolveChirho) {
          sockChirho.recvResolveChirho();
          sockChirho.recvResolveChirho = null;
        }
        break;

      case 'close_chirho':
        sockChirho.stateChirho = 'closed';
        break;

      case 'error_chirho':
        sockChirho.stateChirho = 'error';
        sockChirho.errorChirho = msgChirho.msg_chirho;
        if (sockChirho.connectResolveChirho) {
          sockChirho.connectResolveChirho(-111); // ECONNREFUSED
          sockChirho.connectResolveChirho = null;
        }
        break;
    }
  }

  /** sys_socket() — allocate a socket handle */
  socketChirho(familyChirho, typeChirho, protocolChirho) {
    const idChirho = String(this.nextIdChirho++);
    this.socketsChirho.set(idChirho, {
      idChirho,
      familyChirho,
      typeChirho,
      stateChirho: 'created',
      recvBufferChirho: [],
      connectResolveChirho: null,
      recvResolveChirho: null,
      errorChirho: null,
    });
    return parseInt(idChirho);
  }

  /** sys_connect() — connect to a remote host via proxy */
  async connectChirho(handleChirho, hostChirho, portChirho) {
    const idChirho = String(handleChirho);
    const sockChirho = this.socketsChirho.get(idChirho);
    if (!sockChirho || !this.connectedChirho) return -111;

    return new Promise((resolveChirho) => {
      sockChirho.connectResolveChirho = resolveChirho;
      this.wsChirho.send(JSON.stringify({
        type_chirho: 'connect_chirho',
        id_chirho: idChirho,
        host_chirho: hostChirho,
        port_chirho: portChirho,
      }));

      // Timeout after 10s
      setTimeout(() => {
        if (sockChirho.connectResolveChirho) {
          sockChirho.connectResolveChirho(-110); // ETIMEDOUT
          sockChirho.connectResolveChirho = null;
        }
      }, 10000);
    });
  }

  /** sys_send() — send data */
  sendChirho(handleChirho, dataChirho) {
    const idChirho = String(handleChirho);
    const sockChirho = this.socketsChirho.get(idChirho);
    if (!sockChirho || sockChirho.stateChirho !== 'connected') return -9; // EBADF

    const b64Chirho = btoa(String.fromCharCode(...dataChirho));
    this.wsChirho.send(JSON.stringify({
      type_chirho: 'data_chirho',
      id_chirho: idChirho,
      data_chirho: b64Chirho,
    }));
    return dataChirho.length;
  }

  /** sys_recv() — receive data (returns available bytes immediately) */
  recvChirho(handleChirho, maxLenChirho) {
    const idChirho = String(handleChirho);
    const sockChirho = this.socketsChirho.get(idChirho);
    if (!sockChirho) return { bytesChirho: 0, dataChirho: new Uint8Array(0) };

    if (sockChirho.recvBufferChirho.length === 0) {
      if (sockChirho.stateChirho === 'closed') return { bytesChirho: 0, dataChirho: new Uint8Array(0) };
      return { bytesChirho: -11, dataChirho: new Uint8Array(0) }; // EAGAIN
    }

    // Drain buffer up to maxLen
    const chunkChirho = sockChirho.recvBufferChirho.shift();
    if (chunkChirho.length <= maxLenChirho) {
      return { bytesChirho: chunkChirho.length, dataChirho: chunkChirho };
    }
    // Partial read — put remainder back
    sockChirho.recvBufferChirho.unshift(chunkChirho.slice(maxLenChirho));
    return { bytesChirho: maxLenChirho, dataChirho: chunkChirho.slice(0, maxLenChirho) };
  }

  /** sys_close() — close socket */
  closeChirho(handleChirho) {
    const idChirho = String(handleChirho);
    if (this.wsChirho && this.connectedChirho) {
      this.wsChirho.send(JSON.stringify({ type_chirho: 'close_chirho', id_chirho: idChirho }));
    }
    this.socketsChirho.delete(idChirho);
    return 0;
  }
}

if (typeof module !== 'undefined') module.exports = { WasmNetDriverChirho };
