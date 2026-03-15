// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya WASM Runtime — the "bootloader" for the browser kernel.
 *
 * This JavaScript module provides the "hardware" that the WASM kernel
 * imports. It maps browser APIs to kernel driver interfaces:
 *
 *   xterm.js  → Serial console (terminal emulation)
 *   Canvas    → Framebuffer
 *   OPFS      → Block device
 *   WebSocket → Network interface
 *   DOM events → Input devices
 *   setTimeout → Timer interrupts
 */

class LineluyaRuntimeChirho {
  constructor(canvasIdChirho, xtermContainerIdChirho) {
    this.canvasChirho = document.getElementById(canvasIdChirho);
    this.ctxChirho = this.canvasChirho?.getContext('2d');
    this.xtermContainerChirho = document.getElementById(xtermContainerIdChirho);
    this.terminalChirho = null; // xterm.js Terminal instance
    this.instanceChirho = null;
    this.memoryChirho = null;
    this.framebufferPtrChirho = 0;
    this.fbWidthChirho = 0;
    this.fbHeightChirho = 0;
    this.inputBufferChirho = []; // Bytes queued from xterm.js onData
    this.runningChirho = false;
  }

  /** Initialize xterm.js terminal */
  initTerminalChirho() {
    if (!this.xtermContainerChirho) return;

    // Clear any existing content safely
    while (this.xtermContainerChirho.firstChild) {
      this.xtermContainerChirho.removeChild(this.xtermContainerChirho.firstChild);
    }

    this.terminalChirho = new window.Terminal({
      cursorBlink: true,
      cursorStyle: 'block',
      fontSize: 14,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Courier New', monospace",
      theme: {
        background: '#0a0a0a',
        foreground: '#e0e0e0',
        cursor: '#7c9bff',
        selectionBackground: '#7c9bff44',
        black: '#0a0a0a',
        red: '#ff5555',
        green: '#50fa7b',
        yellow: '#f1fa8c',
        blue: '#7c9bff',
        magenta: '#ff79c6',
        cyan: '#8be9fd',
        white: '#e0e0e0',
        brightBlack: '#6272a4',
        brightRed: '#ff6e6e',
        brightGreen: '#69ff94',
        brightYellow: '#ffffa5',
        brightBlue: '#d6acff',
        brightMagenta: '#ff92df',
        brightCyan: '#a4ffff',
        brightWhite: '#ffffff',
      },
      rows: 24,
      cols: 80,
      scrollback: 1000,
      convertEol: false,
    });

    this.terminalChirho.open(this.xtermContainerChirho);

    // When user types in xterm, queue bytes for kernel to read
    this.terminalChirho.onData((dataChirho) => {
      for (let iChirho = 0; iChirho < dataChirho.length; iChirho++) {
        this.inputBufferChirho.push(dataChirho.charCodeAt(iChirho));
      }
    });

    // Focus the terminal
    this.terminalChirho.focus();
  }

  /** Build the WASM import object — these are the "hardware drivers" */
  getImportsChirho() {
    const selfChirho = this;
    return {
      lineluya_chirho: {
        // --- Serial Console (xterm.js) ---
        js_console_write_chirho(ptrChirho, lenChirho) {
          const bytesChirho = new Uint8Array(selfChirho.memoryChirho.buffer, ptrChirho, lenChirho);
          const textChirho = new TextDecoder().decode(bytesChirho);
          if (selfChirho.terminalChirho) {
            selfChirho.terminalChirho.write(textChirho);
          }
          // Also log to browser console (strip ANSI for readability)
          const cleanChirho = textChirho.replace(/\x1b\[[0-9;]*m/g, '').trimEnd();
          if (cleanChirho) console.log('[KERNEL]', cleanChirho);
        },

        // --- Console read from xterm.js input buffer ---
        js_console_read_chirho(bufPtrChirho, maxLenChirho) {
          const countChirho = Math.min(selfChirho.inputBufferChirho.length, maxLenChirho);
          if (countChirho === 0) return 0;
          const viewChirho = new Uint8Array(selfChirho.memoryChirho.buffer, bufPtrChirho, countChirho);
          for (let iChirho = 0; iChirho < countChirho; iChirho++) {
            viewChirho[iChirho] = selfChirho.inputBufferChirho.shift();
          }
          return countChirho;
        },

        // --- Timer ---
        js_timestamp_us_chirho() {
          return performance.now() * 1000;
        },

        js_yield_chirho() {
          // In a synchronous context this is a no-op.
          // Real yielding happens via the requestAnimationFrame loop.
        },

        // --- Framebuffer (Canvas) ---
        js_fb_init_chirho(widthChirho, heightChirho) {
          selfChirho.fbWidthChirho = widthChirho;
          selfChirho.fbHeightChirho = heightChirho;
          if (selfChirho.canvasChirho) {
            selfChirho.canvasChirho.width = widthChirho;
            selfChirho.canvasChirho.height = heightChirho;
          }
          const bytesNeededChirho = widthChirho * heightChirho * 4;
          selfChirho.framebufferPtrChirho = 0x100000;
          return selfChirho.framebufferPtrChirho;
        },

        js_fb_flush_chirho() {
          if (!selfChirho.ctxChirho || !selfChirho.fbWidthChirho) return;
          const pixelsChirho = new Uint8ClampedArray(
            selfChirho.memoryChirho.buffer,
            selfChirho.framebufferPtrChirho,
            selfChirho.fbWidthChirho * selfChirho.fbHeightChirho * 4
          );
          const imgDataChirho = new ImageData(pixelsChirho, selfChirho.fbWidthChirho, selfChirho.fbHeightChirho);
          selfChirho.ctxChirho.putImageData(imgDataChirho, 0, 0);
        },

        // --- Block Storage (OPFS/IndexedDB) ---
        js_storage_read_chirho(offsetChirho, offsetHighChirho, bufPtrChirho, lenChirho) {
          return 0;
        },

        js_storage_write_chirho(offsetChirho, offsetHighChirho, bufPtrChirho, lenChirho) {
          return lenChirho;
        },

        // --- Networking (WebSocket) ---
        js_net_connect_chirho(hostPtrChirho, hostLenChirho, portChirho) {
          return -1;
        },

        js_net_send_chirho(handleChirho, bufPtrChirho, lenChirho) {
          return -1;
        },

        js_net_recv_chirho(handleChirho, bufPtrChirho, maxLenChirho) {
          return 0;
        },

        js_net_close_chirho(handleChirho) {
          // no-op stub
        },

        // --- B1-015: Random ---
        js_random_get_chirho(bufPtrChirho, lenChirho) {
          const viewChirho = new Uint8Array(selfChirho.memoryChirho.buffer, bufPtrChirho, lenChirho);
          crypto.getRandomValues(viewChirho);
          return 0;
        },

        // --- B1-014: Timer ---
        js_sleep_us_chirho(microsecondsChirho) {
          // In sync context, sleep is a no-op (JS is single-threaded)
          // Real sleep would need Atomics.wait in a SharedArrayBuffer worker
        },

        // --- B3-001: OPFS block device ---
        js_opfs_open_chirho(namePtrChirho, nameLenChirho, createChirho) {
          // Stub — OPFS requires Worker with sync access handle
          console.log('[OPFS] open stub called');
          return -1;
        },
        js_opfs_read_chirho(handleChirho, offsetChirho, bufPtrChirho, lenChirho) { return -1; },
        js_opfs_write_chirho(handleChirho, offsetChirho, bufPtrChirho, lenChirho) { return -1; },
        js_opfs_close_chirho(handleChirho) {},
        js_opfs_delete_chirho(namePtrChirho, nameLenChirho) { return -1; },
        js_opfs_size_chirho(handleChirho) { return -1; },
        js_opfs_sync_chirho(handleChirho) { return 0; },

        // --- B3-002: IndexedDB ---
        js_idb_open_chirho(namePtrChirho, nameLenChirho) {
          console.log('[IDB] open stub called');
          return -1;
        },
        js_idb_get_chirho(handleChirho, keyPtrChirho, keyLenChirho, bufPtrChirho, bufLenChirho) { return -1; },
        js_idb_put_chirho(handleChirho, keyPtrChirho, keyLenChirho, valPtrChirho, valLenChirho) { return -1; },
        js_idb_delete_chirho(handleChirho, keyPtrChirho, keyLenChirho) { return -1; },
        js_idb_list_chirho(handleChirho, bufPtrChirho, bufLenChirho) { return 0; },
        js_idb_close_chirho(handleChirho) {},

        // --- B2-004: DNS resolver over HTTPS (DoH) ---
        js_dns_resolve_chirho(namePtrChirho, nameLenChirho, resultPtrChirho) {
          // Async DNS — fire and forget, return -1 for "pending"
          const nameChirho = new TextDecoder().decode(
            new Uint8Array(selfChirho.memoryChirho.buffer, namePtrChirho, nameLenChirho)
          );
          console.log('[DNS] resolve:', nameChirho);
          // Attempt DoH query asynchronously (result won't be ready synchronously)
          fetch(`https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(nameChirho)}&type=A`, {
            headers: { 'Accept': 'application/dns-json' }
          }).then(rChirho => rChirho.json()).then(dataChirho => {
            if (dataChirho.Answer && dataChirho.Answer.length > 0) {
              const ipChirho = dataChirho.Answer[0].data;
              console.log('[DNS] resolved:', nameChirho, '->', ipChirho);
              // Could store result for next poll
            }
          }).catch(eChirho => console.warn('[DNS] resolve failed:', eChirho));
          return -1; // Pending
        },

        // --- B2-003: WebSocket-TCP bridge ---
        js_ws_bridge_connect_chirho(hostPtrChirho, hostLenChirho, portChirho) {
          const hostChirho = new TextDecoder().decode(
            new Uint8Array(selfChirho.memoryChirho.buffer, hostPtrChirho, hostLenChirho)
          );
          console.log(`[WS-BRIDGE] connect ${hostChirho}:${portChirho}`);
          // In production, connect to CF Worker proxy
          return -1; // Stub
        },
        js_ws_bridge_send_chirho(connIdChirho, bufPtrChirho, lenChirho) { return -1; },
        js_ws_bridge_recv_chirho(connIdChirho, bufPtrChirho, maxLenChirho) { return 0; },
        js_ws_bridge_close_chirho(connIdChirho) {},
        js_ws_bridge_status_chirho(connIdChirho) { return 2; }, // closed

        // --- B2-005: HTTP client via fetch() ---
        js_http_get_chirho(urlPtrChirho, urlLenChirho, bufPtrChirho, bufLenChirho) {
          // Synchronous fetch is not possible in main thread.
          // This stub logs and returns -1. Real impl would use Worker + Atomics.wait.
          const urlChirho = new TextDecoder().decode(
            new Uint8Array(selfChirho.memoryChirho.buffer, urlPtrChirho, urlLenChirho)
          );
          console.log('[HTTP] GET', urlChirho);
          // Fire async fetch (result not available synchronously)
          fetch(urlChirho).then(rChirho => rChirho.text()).then(bodyChirho => {
            console.log('[HTTP] response length:', bodyChirho.length);
          }).catch(eChirho => console.warn('[HTTP] fetch failed:', eChirho));
          return -1; // Async pending
        },

        // --- B3-010: Storage quota ---
        js_storage_quota_chirho(resultPtrChirho) {
          // Try navigator.storage.estimate() (async, so return stub values)
          if (navigator.storage && navigator.storage.estimate) {
            navigator.storage.estimate().then(estimateChirho => {
              console.log('[QUOTA]', estimateChirho);
            });
          }
          // Write stub values: 0 used, 100MB quota
          const viewChirho = new Uint32Array(selfChirho.memoryChirho.buffer, resultPtrChirho, 4);
          viewChirho[0] = 0; viewChirho[1] = 0;
          viewChirho[2] = 100 * 1024 * 1024; viewChirho[3] = 0;
          return 0;
        },

        // --- B4-003: Canvas 2D framebuffer ---
        js_fb_put_rect_chirho(xChirho, yChirho, wChirho, hChirho, dataPtrChirho) {
          if (!selfChirho.ctxChirho || wChirho === 0 || hChirho === 0) return;
          try {
            const pixelsChirho = new Uint8ClampedArray(
              selfChirho.memoryChirho.buffer, dataPtrChirho, wChirho * hChirho * 4
            );
            const imgChirho = new ImageData(pixelsChirho, wChirho, hChirho);
            selfChirho.ctxChirho.putImageData(imgChirho, xChirho, yChirho);
          } catch (eChirho) {
            console.warn('[FB] putRect failed:', eChirho);
          }
        },

        js_fb_fill_rect_chirho(xChirho, yChirho, wChirho, hChirho, rgbaChirho) {
          if (!selfChirho.ctxChirho) return;
          const rChirho = (rgbaChirho >> 16) & 0xFF;
          const gChirho = (rgbaChirho >> 8) & 0xFF;
          const bChirho = rgbaChirho & 0xFF;
          const aChirho = ((rgbaChirho >> 24) & 0xFF) / 255;
          selfChirho.ctxChirho.fillStyle = `rgba(${rChirho},${gChirho},${bChirho},${aChirho})`;
          selfChirho.ctxChirho.fillRect(xChirho, yChirho, wChirho, hChirho);
        },

        js_fb_draw_text_chirho(xChirho, yChirho, textPtrChirho, textLenChirho, rgbaChirho) {
          if (!selfChirho.ctxChirho) return 0;
          const textChirho = new TextDecoder().decode(
            new Uint8Array(selfChirho.memoryChirho.buffer, textPtrChirho, textLenChirho)
          );
          const rChirho = (rgbaChirho >> 16) & 0xFF;
          const gChirho = (rgbaChirho >> 8) & 0xFF;
          const bChirho = rgbaChirho & 0xFF;
          selfChirho.ctxChirho.fillStyle = `rgb(${rChirho},${gChirho},${bChirho})`;
          selfChirho.ctxChirho.font = '12px monospace';
          selfChirho.ctxChirho.fillText(textChirho, xChirho, yChirho + 12);
          return selfChirho.ctxChirho.measureText(textChirho).width | 0;
        },

        // --- B4-006: Mouse events ---
        js_input_mouse_chirho(resultPtrChirho) {
          if (!selfChirho.mouseEventsChirho || selfChirho.mouseEventsChirho.length === 0) return 0;
          const evtChirho = selfChirho.mouseEventsChirho.shift();
          const viewChirho = new Uint32Array(selfChirho.memoryChirho.buffer, resultPtrChirho, 4);
          viewChirho[0] = evtChirho.xChirho;
          viewChirho[1] = evtChirho.yChirho;
          viewChirho[2] = evtChirho.buttonsChirho;
          viewChirho[3] = evtChirho.typeChirho;
          return 1;
        },

        // --- B4-007: Keyboard events ---
        js_input_keyboard_chirho(resultPtrChirho) {
          if (!selfChirho.keyEventsChirho || selfChirho.keyEventsChirho.length === 0) return 0;
          const evtChirho = selfChirho.keyEventsChirho.shift();
          const viewChirho = new Uint32Array(selfChirho.memoryChirho.buffer, resultPtrChirho, 3);
          viewChirho[0] = evtChirho.keycodeChirho;
          viewChirho[1] = evtChirho.modifiersChirho;
          viewChirho[2] = evtChirho.pressedChirho ? 1 : 0;
          return 1;
        },
      },
    };
  }

  /** Set up mouse/keyboard event listeners for B4 X11 input */
  initInputEventsChirho() {
    this.mouseEventsChirho = [];
    this.keyEventsChirho = [];

    if (this.canvasChirho) {
      this.canvasChirho.addEventListener('mousemove', (eChirho) => {
        const rectChirho = this.canvasChirho.getBoundingClientRect();
        this.mouseEventsChirho.push({
          xChirho: (eChirho.clientX - rectChirho.left) | 0,
          yChirho: (eChirho.clientY - rectChirho.top) | 0,
          buttonsChirho: eChirho.buttons,
          typeChirho: 0, // motion
        });
      });
      this.canvasChirho.addEventListener('mousedown', (eChirho) => {
        const rectChirho = this.canvasChirho.getBoundingClientRect();
        this.mouseEventsChirho.push({
          xChirho: (eChirho.clientX - rectChirho.left) | 0,
          yChirho: (eChirho.clientY - rectChirho.top) | 0,
          buttonsChirho: eChirho.buttons,
          typeChirho: 1, // press
        });
      });
      this.canvasChirho.addEventListener('mouseup', (eChirho) => {
        const rectChirho = this.canvasChirho.getBoundingClientRect();
        this.mouseEventsChirho.push({
          xChirho: (eChirho.clientX - rectChirho.left) | 0,
          yChirho: (eChirho.clientY - rectChirho.top) | 0,
          buttonsChirho: eChirho.buttons,
          typeChirho: 2, // release
        });
      });

      // Keyboard events on the canvas (needs tabindex for focus)
      this.canvasChirho.setAttribute('tabindex', '0');
      this.canvasChirho.addEventListener('keydown', (eChirho) => {
        this.keyEventsChirho.push({
          keycodeChirho: eChirho.keyCode,
          modifiersChirho: (eChirho.shiftKey ? 1 : 0) | (eChirho.ctrlKey ? 4 : 0) | (eChirho.altKey ? 8 : 0),
          pressedChirho: true,
        });
      });
      this.canvasChirho.addEventListener('keyup', (eChirho) => {
        this.keyEventsChirho.push({
          keycodeChirho: eChirho.keyCode,
          modifiersChirho: (eChirho.shiftKey ? 1 : 0) | (eChirho.ctrlKey ? 4 : 0) | (eChirho.altKey ? 8 : 0),
          pressedChirho: false,
        });
      });
    }
  }

  /** Boot the kernel — load WASM and call kernel_main */
  async bootChirho(wasmUrlChirho) {
    // Initialize xterm.js and input event listeners
    this.initTerminalChirho();
    this.initInputEventsChirho();

    try {
      const responseChirho = await fetch(wasmUrlChirho);
      if (!responseChirho.ok) {
        throw new Error(`Failed to fetch ${wasmUrlChirho}: ${responseChirho.status}`);
      }
      const wasmBytesChirho = await responseChirho.arrayBuffer();

      const moduleChirho = await WebAssembly.compile(wasmBytesChirho);
      const importsChirho = this.getImportsChirho();

      // Create memory (256 pages = 16MB initial, growable to 1GB)
      this.memoryChirho = new WebAssembly.Memory({
        initial: 256,
        maximum: 16384,
        shared: false,
      });
      importsChirho.lineluya_chirho.memory = this.memoryChirho;

      this.instanceChirho = await WebAssembly.instantiate(moduleChirho, importsChirho);

      // If the WASM module exports its own memory, use that
      if (this.instanceChirho.exports.memory) {
        this.memoryChirho = this.instanceChirho.exports.memory;
      }

      // Boot the kernel!
      console.log('[RUNTIME] Booting Lineluya kernel (wasm32)...');
      this.instanceChirho.exports.kernel_main_chirho();

      // Start the tick loop (scheduler + shell input processing)
      this.runningChirho = true;
      this.tickLoopChirho();

    } catch (errChirho) {
      console.error('[RUNTIME] Boot failed:', errChirho);
      if (this.terminalChirho) {
        this.terminalChirho.write('\r\n\x1b[1;31m!!! BOOT FAILED: ' + errChirho.message + '\x1b[0m\r\n');
      }
    }
  }

  /** Animation frame loop — drives scheduler ticks and shell input */
  tickLoopChirho() {
    if (!this.runningChirho) return;

    if (this.instanceChirho?.exports?.kernel_tick_chirho) {
      this.instanceChirho.exports.kernel_tick_chirho();
    }

    requestAnimationFrame(() => this.tickLoopChirho());
  }

  /** Shut down the kernel */
  shutdownChirho() {
    this.runningChirho = false;
    if (this.terminalChirho) {
      this.terminalChirho.write('\r\n\x1b[1;31mKernel halted.\x1b[0m\r\n');
    }
    console.log('[RUNTIME] Kernel halted.');
  }
}

// Export for module usage
if (typeof module !== 'undefined') module.exports = { LineluyaRuntimeChirho };
