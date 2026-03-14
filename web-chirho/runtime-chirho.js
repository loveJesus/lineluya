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
      },
    };
  }

  /** Boot the kernel — load WASM and call kernel_main */
  async bootChirho(wasmUrlChirho) {
    // Initialize xterm.js
    this.initTerminalChirho();

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
