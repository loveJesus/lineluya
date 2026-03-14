// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya WASM Runtime — the "bootloader" for the browser kernel.
 *
 * This JavaScript module provides the "hardware" that the WASM kernel
 * imports. It maps browser APIs to kernel driver interfaces:
 *
 *   Canvas    → Framebuffer
 *   console   → Serial port
 *   OPFS      → Block device
 *   WebSocket → Network interface
 *   DOM events → Input devices
 *   setTimeout → Timer interrupts
 */

class LineluyaRuntimeChirho {
  constructor(canvasIdChirho, terminalIdChirho) {
    this.canvasChirho = document.getElementById(canvasIdChirho);
    this.ctxChirho = this.canvasChirho?.getContext('2d');
    this.terminalChirho = document.getElementById(terminalIdChirho);
    this.instanceChirho = null;
    this.memoryChirho = null;
    this.framebufferPtrChirho = 0;
    this.fbWidthChirho = 0;
    this.fbHeightChirho = 0;
    this.keyQueueChirho = [];
    this.runningChirho = false;

    // Keyboard input
    document.addEventListener('keydown', (eChirho) => {
      this.keyQueueChirho.push({
        keycodeChirho: eChirho.keyCode,
        flagsChirho: (eChirho.shiftKey ? 1 : 0) | (eChirho.ctrlKey ? 2 : 0) | (eChirho.altKey ? 4 : 0),
      });
      if (this.instanceChirho) {
        this.instanceChirho.exports.kernel_keydown_wasm_chirho(eChirho.keyCode,
          (eChirho.shiftKey ? 1 : 0) | (eChirho.ctrlKey ? 2 : 0) | (eChirho.altKey ? 4 : 0));
      }
    });
  }

  /** Build the WASM import object — these are the "hardware drivers" */
  getImportsChirho() {
    const selfChirho = this;
    return {
      env: {
        // --- Serial Console ---
        js_console_write_chirho(ptrChirho, lenChirho) {
          const bytesChirho = new Uint8Array(selfChirho.memoryChirho.buffer, ptrChirho, lenChirho);
          const textChirho = new TextDecoder().decode(bytesChirho);
          if (selfChirho.terminalChirho) {
            selfChirho.terminalChirho.textContent += textChirho;
            selfChirho.terminalChirho.scrollTop = selfChirho.terminalChirho.scrollHeight;
          }
          // Also log to browser console
          if (textChirho.trim()) console.log('[KERNEL]', textChirho.trimEnd());
        },

        // --- Timer ---
        js_timestamp_us_chirho() {
          return BigInt(Math.floor(performance.now() * 1000));
        },

        js_yield_chirho() {
          // In a synchronous context this is a no-op.
          // Real yielding happens via the requestAnimationFrame loop.
        },

        // --- Framebuffer (Canvas) ---
        js_framebuffer_init_chirho(widthChirho, heightChirho) {
          selfChirho.fbWidthChirho = widthChirho;
          selfChirho.fbHeightChirho = heightChirho;
          if (selfChirho.canvasChirho) {
            selfChirho.canvasChirho.width = widthChirho;
            selfChirho.canvasChirho.height = heightChirho;
          }
          // Allocate framebuffer in WASM memory
          // For now, return a fixed offset (kernel manages memory)
          const bytesNeededChirho = widthChirho * heightChirho * 4; // RGBA
          selfChirho.framebufferPtrChirho = 0x100000; // 1MB offset
          return selfChirho.framebufferPtrChirho;
        },

        js_framebuffer_flush_chirho() {
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
        js_storage_read_chirho(offsetChirho, bufPtrChirho, lenChirho) {
          // Stub — real impl uses OPFS SyncAccessHandle
          return 0;
        },

        js_storage_write_chirho(offsetChirho, bufPtrChirho, lenChirho) {
          return lenChirho;
        },

        // --- Networking (WebSocket) ---
        js_net_connect_chirho(hostPtrChirho, hostLenChirho, portChirho) {
          return -1; // Stub
        },

        js_net_send_chirho(handleChirho, bufPtrChirho, lenChirho) {
          return -1;
        },

        js_net_recv_chirho(handleChirho, bufPtrChirho, maxLenChirho) {
          return 0;
        },

        // --- Input ---
        js_input_poll_chirho(keycodePtrChirho, flagsPtrChirho) {
          if (selfChirho.keyQueueChirho.length === 0) return 0;
          const evtChirho = selfChirho.keyQueueChirho.shift();
          const viewChirho = new DataView(selfChirho.memoryChirho.buffer);
          viewChirho.setUint32(keycodePtrChirho, evtChirho.keycodeChirho, true);
          viewChirho.setUint32(flagsPtrChirho, evtChirho.flagsChirho, true);
          return 1;
        },
      },
    };
  }

  /** Boot the kernel — load WASM and call kernel_main */
  async bootChirho(wasmUrlChirho) {
    if (this.terminalChirho) {
      this.terminalChirho.textContent = '';
    }

    try {
      const responseChirho = await fetch(wasmUrlChirho);
      const wasmBytesChirho = await responseChirho.arrayBuffer();

      const moduleChirho = await WebAssembly.compile(wasmBytesChirho);
      const importsChirho = this.getImportsChirho();

      // Create memory (256 pages = 16MB initial, growable to 1GB)
      this.memoryChirho = new WebAssembly.Memory({
        initial: 256,
        maximum: 16384,
        shared: false,
      });
      importsChirho.env.memory = this.memoryChirho;

      this.instanceChirho = await WebAssembly.instantiate(moduleChirho, importsChirho);

      // If the WASM module exports its own memory, use that
      if (this.instanceChirho.exports.memory) {
        this.memoryChirho = this.instanceChirho.exports.memory;
      }

      // Boot the kernel!
      console.log('[RUNTIME] Booting Lineluya kernel (wasm32)...');
      this.instanceChirho.exports.kernel_main_wasm_chirho();

      // Start the tick loop (scheduler + framebuffer)
      this.runningChirho = true;
      this.tickLoopChirho();

    } catch (errChirho) {
      console.error('[RUNTIME] Boot failed:', errChirho);
      if (this.terminalChirho) {
        this.terminalChirho.textContent += `\n!!! BOOT FAILED: ${errChirho.message}\n`;
      }
    }
  }

  /** Animation frame loop — drives scheduler ticks and framebuffer flush */
  tickLoopChirho() {
    if (!this.runningChirho) return;

    if (this.instanceChirho?.exports?.kernel_tick_wasm_chirho) {
      this.instanceChirho.exports.kernel_tick_wasm_chirho();
    }

    requestAnimationFrame(() => this.tickLoopChirho());
  }

  /** Shut down the kernel */
  shutdownChirho() {
    this.runningChirho = false;
    console.log('[RUNTIME] Kernel halted.');
  }
}

// Export for module usage
if (typeof module !== 'undefined') module.exports = { LineluyaRuntimeChirho };
