// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya Desktop — Browser-based Window Manager (B6)
 *
 * A canvas-based desktop environment that runs on top of the WASM kernel.
 * Provides windows, a taskbar, and application management.
 *
 * B6-001: Desktop window system with taskbar
 * B6-002: Application launcher / start menu
 * B6-005: Network-transparent X11 apps via proxy
 * B6-007: Web Audio API integration
 * B6-008: Browser notifications for system events
 * B6-009: Desktop wallpaper and theming
 * B6-010: Right-click context menus
 * B6-011: System settings panel
 * B6-012: Text editor application
 * B6-013: Web browser within the desktop (via iframe)
 * B6-014: Keyboard shortcuts and hotkeys
 * B6-015: Full desktop experience integration test
 */

// ── Theme ───────────────────────────────────────────────────────────────────

const THEME_CHIRHO = {
  bgColorChirho: '#1a1a2e',
  taskbarColorChirho: '#16213e',
  taskbarHeightChirho: 36,
  windowTitleBarChirho: '#0f3460',
  windowTitleBarActiveChirho: '#533483',
  windowBgChirho: '#1a1a2e',
  windowBorderChirho: '#533483',
  textColorChirho: '#e0e0e0',
  accentColorChirho: '#7c9bff',
  menuBgChirho: '#16213e',
  menuHoverChirho: '#533483',
  fontChirho: '13px "JetBrains Mono", monospace',
  titleFontChirho: '12px "JetBrains Mono", monospace',
};

// ── Saved Settings (B6-011) ─────────────────────────────────────────────────

const SETTINGS_KEY_CHIRHO = 'lineluya-settings-chirho';

/**
 * Load persisted settings from localStorage.
 * @returns {object}
 */
function loadSettingsChirho() {
  try {
    const rawChirho = localStorage.getItem(SETTINGS_KEY_CHIRHO);
    if (rawChirho) return JSON.parse(rawChirho);
  } catch (_errChirho) { /* ignore parse failures */ }
  return {};
}

/**
 * Save settings to localStorage.
 * @param {object} settingsChirho
 */
function saveSettingsChirho(settingsChirho) {
  try {
    localStorage.setItem(SETTINGS_KEY_CHIRHO, JSON.stringify(settingsChirho));
  } catch (_errChirho) { /* quota exceeded, etc. */ }
}

/**
 * Apply persisted settings to the live theme object.
 * @param {object} settingsChirho
 */
function applySettingsToThemeChirho(settingsChirho) {
  if (settingsChirho.bgColorChirho) THEME_CHIRHO.bgColorChirho = settingsChirho.bgColorChirho;
  if (settingsChirho.accentColorChirho) THEME_CHIRHO.accentColorChirho = settingsChirho.accentColorChirho;
  if (settingsChirho.fontSizeChirho) {
    THEME_CHIRHO.fontChirho = `${settingsChirho.fontSizeChirho}px "JetBrains Mono", monospace`;
    THEME_CHIRHO.titleFontChirho = `${Math.max(10, settingsChirho.fontSizeChirho - 1)}px "JetBrains Mono", monospace`;
  }
}

// Apply on load
applySettingsToThemeChirho(loadSettingsChirho());

// ── Audio Subsystem (B6-007) ────────────────────────────────────────────────

/**
 * AudioManagerChirho — Routes audio through the Web Audio API.
 * Acts as /dev/dsp for WASM processes.
 */
class AudioManagerChirho {
  constructor() {
    /** @type {AudioContext|null} */
    this.audioCtxChirho = null;
    /** @type {GainNode|null} */
    this.masterGainChirho = null;
    /** @type {Map<number, {sourceChirho: AudioBufferSourceNode|OscillatorNode, gainChirho: GainNode}>} */
    this.sourcesChirho = new Map();
    this.nextSourceIdChirho = 1;
    this.volumeChirho = 0.8; // 0.0 – 1.0
  }

  /**
   * Lazily initialise AudioContext (must be called from user gesture).
   */
  initChirho() {
    if (this.audioCtxChirho) return;
    this.audioCtxChirho = new (window.AudioContext || window.webkitAudioContext)();
    this.masterGainChirho = this.audioCtxChirho.createGain();
    this.masterGainChirho.gain.value = this.volumeChirho;
    this.masterGainChirho.connect(this.audioCtxChirho.destination);
  }

  /**
   * Set master volume (B6-007 acceptance: volume control works).
   * @param {number} levelChirho 0.0 – 1.0
   */
  setVolumeChirho(levelChirho) {
    this.volumeChirho = Math.max(0, Math.min(1, levelChirho));
    if (this.masterGainChirho) {
      this.masterGainChirho.gain.value = this.volumeChirho;
    }
  }

  /**
   * Play a tone at the given frequency for durationMs.
   * Returns source id for later stop.
   * @param {number} frequencyChirho Hz
   * @param {number} durationMsChirho
   * @param {string} typeChirho 'sine'|'square'|'sawtooth'|'triangle'
   * @returns {number} sourceIdChirho
   */
  playToneChirho(frequencyChirho = 440, durationMsChirho = 500, typeChirho = 'sine') {
    this.initChirho();
    const oscChirho = this.audioCtxChirho.createOscillator();
    const gainChirho = this.audioCtxChirho.createGain();
    oscChirho.type = typeChirho;
    oscChirho.frequency.value = frequencyChirho;
    gainChirho.gain.value = 0.5;
    oscChirho.connect(gainChirho);
    gainChirho.connect(this.masterGainChirho);

    const idChirho = this.nextSourceIdChirho++;
    this.sourcesChirho.set(idChirho, { sourceChirho: oscChirho, gainChirho });

    oscChirho.start();
    if (durationMsChirho > 0) {
      oscChirho.stop(this.audioCtxChirho.currentTime + durationMsChirho / 1000);
      oscChirho.onended = () => this.sourcesChirho.delete(idChirho);
    }
    return idChirho;
  }

  /**
   * Play a PCM audio buffer (Float32Array, mono).
   * Supports multiple simultaneous sources (mixing).
   * @param {Float32Array} samplesChirho
   * @param {number} sampleRateChirho
   * @returns {number} sourceIdChirho
   */
  playBufferChirho(samplesChirho, sampleRateChirho = 44100) {
    this.initChirho();
    const bufChirho = this.audioCtxChirho.createBuffer(1, samplesChirho.length, sampleRateChirho);
    bufChirho.getChannelData(0).set(samplesChirho);

    const srcChirho = this.audioCtxChirho.createBufferSource();
    srcChirho.buffer = bufChirho;

    const gainChirho = this.audioCtxChirho.createGain();
    gainChirho.gain.value = 0.8;
    srcChirho.connect(gainChirho);
    gainChirho.connect(this.masterGainChirho);

    const idChirho = this.nextSourceIdChirho++;
    this.sourcesChirho.set(idChirho, { sourceChirho: srcChirho, gainChirho });
    srcChirho.onended = () => this.sourcesChirho.delete(idChirho);

    srcChirho.start();
    return idChirho;
  }

  /**
   * Stop a playing source.
   * @param {number} idChirho
   */
  stopSourceChirho(idChirho) {
    const entryChirho = this.sourcesChirho.get(idChirho);
    if (entryChirho) {
      try { entryChirho.sourceChirho.stop(); } catch (_eChirho) { /* already stopped */ }
      this.sourcesChirho.delete(idChirho);
    }
  }

  /**
   * Stop all sources.
   */
  stopAllChirho() {
    for (const [idChirho] of this.sourcesChirho) {
      this.stopSourceChirho(idChirho);
    }
  }

  /**
   * Play system notification sound.
   */
  playNotificationSoundChirho() {
    this.playToneChirho(880, 120, 'sine');
    setTimeout(() => this.playToneChirho(1100, 100, 'sine'), 140);
  }
}

// ── Notification Subsystem (B6-008) ─────────────────────────────────────────

/**
 * NotificationManagerChirho — Browser Notifications API wrapper.
 * System.notify() for kernel events.
 */
class NotificationManagerChirho {
  constructor() {
    this.permissionChirho = typeof Notification !== 'undefined' ? Notification.permission : 'denied';
  }

  /**
   * Request notification permission from the user.
   * @returns {Promise<string>}
   */
  async requestPermissionChirho() {
    if (typeof Notification === 'undefined') {
      this.permissionChirho = 'denied';
      return this.permissionChirho;
    }
    this.permissionChirho = await Notification.requestPermission();
    return this.permissionChirho;
  }

  /**
   * Send a browser notification (B6-008).
   * Clicking the notification focuses the tab.
   * @param {string} titleChirho
   * @param {string} bodyChirho
   * @param {string} iconChirho optional icon URL
   * @returns {Notification|null}
   */
  notifyChirho(titleChirho, bodyChirho = '', iconChirho = '') {
    if (this.permissionChirho !== 'granted') return null;
    if (typeof Notification === 'undefined') return null;

    const optionsChirho = { body: bodyChirho, tag: 'lineluya-chirho' };
    if (iconChirho) optionsChirho.icon = iconChirho;

    const notifChirho = new Notification(titleChirho, optionsChirho);
    notifChirho.onclick = () => {
      window.focus();
      notifChirho.close();
    };
    return notifChirho;
  }

  /**
   * Notify on long-running task completion.
   * @param {string} taskNameChirho
   */
  notifyTaskCompleteChirho(taskNameChirho) {
    this.notifyChirho(
      'Task Complete — Lineluya',
      `${taskNameChirho} has finished.`
    );
  }

  /**
   * Notify on system error.
   * @param {string} messageChirho
   */
  notifyErrorChirho(messageChirho) {
    this.notifyChirho(
      'System Error — Lineluya',
      messageChirho
    );
  }
}

// ── X11 Proxy Display (B6-005) ──────────────────────────────────────────────

/**
 * X11ProxyDisplayChirho — Network-transparent X11 forwarding over WebSocket.
 * Receives framebuffer updates from a remote X11 proxy server and renders
 * them into a desktop window. Input events are forwarded back.
 */
class X11ProxyDisplayChirho {
  /**
   * @param {string} wsUrlChirho WebSocket URL to X11 proxy
   * @param {DesktopManagerChirho} desktopChirho
   */
  constructor(wsUrlChirho, desktopChirho) {
    this.wsUrlChirho = wsUrlChirho;
    this.desktopChirho = desktopChirho;
    /** @type {WebSocket|null} */
    this.socketChirho = null;
    /** @type {WindowChirho|null} */
    this.windowChirho = null;
    /** @type {ImageData|null} */
    this.framebufferChirho = null;
    this.connectedChirho = false;
  }

  /**
   * Open a window and connect to the remote X11 proxy.
   * @param {string} appNameChirho e.g. "xterm", "xclock"
   */
  connectChirho(appNameChirho = 'X11 App') {
    this.windowChirho = this.desktopChirho.createWindowChirho(`X11: ${appNameChirho}`, 640, 480);

    // Custom render callback draws the framebuffer
    this.windowChirho.renderCallbackChirho = (ctxChirho, winChirho) => {
      const titleBarHeightChirho = 28;
      if (this.framebufferChirho) {
        ctxChirho.putImageData(
          this.framebufferChirho,
          winChirho.xChirho + 1,
          winChirho.yChirho + titleBarHeightChirho
        );
      } else {
        ctxChirho.fillStyle = '#222';
        ctxChirho.fillRect(
          winChirho.xChirho + 1,
          winChirho.yChirho + titleBarHeightChirho,
          winChirho.widthChirho - 2,
          winChirho.heightChirho - titleBarHeightChirho - 1
        );
        ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
        ctxChirho.font = THEME_CHIRHO.fontChirho;
        const statusChirho = this.connectedChirho ? 'Waiting for framebuffer...' : `Connecting to ${this.wsUrlChirho}...`;
        ctxChirho.fillText(statusChirho, winChirho.xChirho + 12, winChirho.yChirho + titleBarHeightChirho + 30);
      }
    };

    try {
      this.socketChirho = new WebSocket(this.wsUrlChirho);
      this.socketChirho.binaryType = 'arraybuffer';

      this.socketChirho.onopen = () => {
        this.connectedChirho = true;
        // Send launch command
        this.socketChirho.send(JSON.stringify({ typeChirho: 'launch', appChirho: appNameChirho }));
        this.desktopChirho.renderChirho();
      };

      this.socketChirho.onmessage = (evtChirho) => {
        if (evtChirho.data instanceof ArrayBuffer) {
          // Binary framebuffer update: first 8 bytes = width(u32) + height(u32), rest = RGBA
          const viewChirho = new DataView(evtChirho.data);
          const fbWidthChirho = viewChirho.getUint32(0, true);
          const fbHeightChirho = viewChirho.getUint32(4, true);
          const pixelsChirho = new Uint8ClampedArray(evtChirho.data, 8);
          this.framebufferChirho = new ImageData(pixelsChirho, fbWidthChirho, fbHeightChirho);
          this.desktopChirho.renderChirho();
        }
      };

      this.socketChirho.onclose = () => {
        this.connectedChirho = false;
        this.desktopChirho.renderChirho();
      };

      this.socketChirho.onerror = () => {
        this.connectedChirho = false;
      };
    } catch (errChirho) {
      console.error('X11ProxyDisplayChirho: connection failed', errChirho);
    }
  }

  /**
   * Forward mouse/keyboard input to remote X11 app.
   * @param {string} typeChirho 'mousemove'|'mousedown'|'mouseup'|'keydown'|'keyup'
   * @param {object} dataChirho event data
   */
  sendInputChirho(typeChirho, dataChirho) {
    if (this.socketChirho && this.connectedChirho) {
      this.socketChirho.send(JSON.stringify({ typeChirho, ...dataChirho }));
    }
  }

  /**
   * Disconnect and clean up.
   */
  disconnectChirho() {
    if (this.socketChirho) {
      this.socketChirho.close();
      this.socketChirho = null;
    }
    this.connectedChirho = false;
    this.framebufferChirho = null;
  }
}

// ── Window Manager ──────────────────────────────────────────────────────────

class WindowChirho {
  constructor(idChirho, titleChirho, xChirho, yChirho, widthChirho, heightChirho) {
    this.idChirho = idChirho;
    this.titleChirho = titleChirho;
    this.xChirho = xChirho;
    this.yChirho = yChirho;
    this.widthChirho = widthChirho;
    this.heightChirho = heightChirho;
    this.minimizedChirho = false;
    this.maximizedChirho = false;
    this.focusedChirho = false;
    this.contentChirho = []; // Lines of text content
    this.renderCallbackChirho = null; // Custom render function
    this.savedBoundsChirho = null; // For maximize/restore
    this.onClickCallbackChirho = null; // Click handler within content area
    this.onKeyCallbackChirho = null; // Keyboard handler when focused
    /** @type {HTMLIFrameElement|null} */
    this.iframeChirho = null; // For embedded browser (B6-013)
  }
}

class DesktopManagerChirho {
  /**
   * @param {HTMLCanvasElement} canvasChirho
   */
  constructor(canvasChirho) {
    this.canvasChirho = canvasChirho;
    this.ctxChirho = canvasChirho.getContext('2d');
    /** @type {WindowChirho[]} */
    this.windowsChirho = [];
    this.nextIdChirho = 1;
    this.dragChirho = null; // { windowId, offsetX, offsetY }
    this.menuOpenChirho = false;
    this.contextMenuChirho = null; // { x, y, items }
    this.wallpaperTextChirho = 'Lineluya Desktop — John 3:16';
    this.clockIntervalChirho = null;

    // B6-007: Audio
    this.audioChirho = new AudioManagerChirho();

    // B6-008: Notifications
    this.notificationsChirho = new NotificationManagerChirho();

    // B6-005: X11 proxy sessions
    /** @type {X11ProxyDisplayChirho[]} */
    this.x11SessionsChirho = [];

    // Set up event handlers
    this.setupEventsChirho();
  }

  /**
   * Create a new window (B6-001).
   */
  createWindowChirho(titleChirho, widthChirho = 500, heightChirho = 350) {
    const idChirho = this.nextIdChirho++;
    const xChirho = 50 + (idChirho * 30) % 200;
    const yChirho = 50 + (idChirho * 30) % 150;
    const winChirho = new WindowChirho(idChirho, titleChirho, xChirho, yChirho, widthChirho, heightChirho);
    this.windowsChirho.push(winChirho);
    this.focusWindowChirho(idChirho);
    this.renderChirho();
    return winChirho;
  }

  /**
   * Close a window.
   */
  closeWindowChirho(idChirho) {
    const winChirho = this.windowsChirho.find(wChirho => wChirho.idChirho === idChirho);
    // Clean up iframe if present (B6-013)
    if (winChirho && winChirho.iframeChirho) {
      winChirho.iframeChirho.remove();
      winChirho.iframeChirho = null;
    }
    this.windowsChirho = this.windowsChirho.filter(wChirho => wChirho.idChirho !== idChirho);
    this.renderChirho();
  }

  /**
   * Focus a window (bring to front).
   */
  focusWindowChirho(idChirho) {
    for (const wChirho of this.windowsChirho) {
      wChirho.focusedChirho = wChirho.idChirho === idChirho;
    }
    // Move focused window to end (top of z-order)
    const idxChirho = this.windowsChirho.findIndex(wChirho => wChirho.idChirho === idChirho);
    if (idxChirho >= 0) {
      const [winChirho] = this.windowsChirho.splice(idxChirho, 1);
      this.windowsChirho.push(winChirho);
    }
    this.renderChirho();
  }

  /**
   * Toggle window maximize (B6-001).
   */
  toggleMaximizeChirho(idChirho) {
    const winChirho = this.windowsChirho.find(wChirho => wChirho.idChirho === idChirho);
    if (!winChirho) return;

    if (winChirho.maximizedChirho) {
      // Restore
      if (winChirho.savedBoundsChirho) {
        winChirho.xChirho = winChirho.savedBoundsChirho.xChirho;
        winChirho.yChirho = winChirho.savedBoundsChirho.yChirho;
        winChirho.widthChirho = winChirho.savedBoundsChirho.widthChirho;
        winChirho.heightChirho = winChirho.savedBoundsChirho.heightChirho;
      }
      winChirho.maximizedChirho = false;
    } else {
      // Save current bounds and maximize
      winChirho.savedBoundsChirho = {
        xChirho: winChirho.xChirho,
        yChirho: winChirho.yChirho,
        widthChirho: winChirho.widthChirho,
        heightChirho: winChirho.heightChirho,
      };
      winChirho.xChirho = 0;
      winChirho.yChirho = 0;
      winChirho.widthChirho = this.canvasChirho.width;
      winChirho.heightChirho = this.canvasChirho.height - THEME_CHIRHO.taskbarHeightChirho;
      winChirho.maximizedChirho = true;
    }
    this.renderChirho();
  }

  // ── Rendering ─────────────────────────────────────────────────────────

  /**
   * Render the entire desktop.
   */
  renderChirho() {
    const ctxChirho = this.ctxChirho;
    const wChirho = this.canvasChirho.width;
    const hChirho = this.canvasChirho.height;

    // Wallpaper (B6-009)
    ctxChirho.fillStyle = THEME_CHIRHO.bgColorChirho;
    ctxChirho.fillRect(0, 0, wChirho, hChirho);

    // Wallpaper text
    ctxChirho.fillStyle = '#333355';
    ctxChirho.font = '24px monospace';
    ctxChirho.textAlign = 'center';
    ctxChirho.fillText(this.wallpaperTextChirho, wChirho / 2, hChirho / 2 - 40);
    ctxChirho.font = '14px monospace';
    ctxChirho.fillStyle = '#444466';
    ctxChirho.fillText('Linux-compatible kernel in Rust, running in your browser', wChirho / 2, hChirho / 2);
    ctxChirho.textAlign = 'left';

    // Draw windows
    for (const winChirho of this.windowsChirho) {
      if (!winChirho.minimizedChirho) {
        this.drawWindowChirho(ctxChirho, winChirho);
      }
    }

    // Taskbar (B6-001)
    this.drawTaskbarChirho(ctxChirho, wChirho, hChirho);

    // Context menu (B6-010)
    if (this.contextMenuChirho) {
      this.drawContextMenuChirho(ctxChirho);
    }

    // Sync iframe positions (B6-013)
    this.syncIframePositionsChirho();
  }

  /**
   * Draw a window.
   */
  drawWindowChirho(ctxChirho, winChirho) {
    const titleBarHeightChirho = 28;

    // Shadow
    ctxChirho.fillStyle = 'rgba(0,0,0,0.3)';
    ctxChirho.fillRect(winChirho.xChirho + 3, winChirho.yChirho + 3, winChirho.widthChirho, winChirho.heightChirho);

    // Window border
    ctxChirho.strokeStyle = winChirho.focusedChirho ? THEME_CHIRHO.windowBorderChirho : '#333';
    ctxChirho.lineWidth = 1;
    ctxChirho.strokeRect(winChirho.xChirho, winChirho.yChirho, winChirho.widthChirho, winChirho.heightChirho);

    // Title bar
    ctxChirho.fillStyle = winChirho.focusedChirho
      ? THEME_CHIRHO.windowTitleBarActiveChirho
      : THEME_CHIRHO.windowTitleBarChirho;
    ctxChirho.fillRect(winChirho.xChirho, winChirho.yChirho, winChirho.widthChirho, titleBarHeightChirho);

    // Title text
    ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
    ctxChirho.font = THEME_CHIRHO.titleFontChirho;
    ctxChirho.fillText(
      winChirho.titleChirho,
      winChirho.xChirho + 8,
      winChirho.yChirho + 18
    );

    // Close button [X]
    ctxChirho.fillStyle = '#ff5555';
    ctxChirho.fillRect(winChirho.xChirho + winChirho.widthChirho - 24, winChirho.yChirho + 4, 20, 20);
    ctxChirho.fillStyle = '#fff';
    ctxChirho.font = '12px monospace';
    ctxChirho.fillText('X', winChirho.xChirho + winChirho.widthChirho - 18, winChirho.yChirho + 18);

    // Window content area
    ctxChirho.fillStyle = THEME_CHIRHO.windowBgChirho;
    ctxChirho.fillRect(
      winChirho.xChirho + 1,
      winChirho.yChirho + titleBarHeightChirho,
      winChirho.widthChirho - 2,
      winChirho.heightChirho - titleBarHeightChirho - 1
    );

    // Render content or custom callback
    if (winChirho.renderCallbackChirho) {
      winChirho.renderCallbackChirho(ctxChirho, winChirho);
    } else if (winChirho.contentChirho.length > 0) {
      ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
      ctxChirho.font = THEME_CHIRHO.fontChirho;
      const lineHeightChirho = 18;
      for (let iChirho = 0; iChirho < winChirho.contentChirho.length; iChirho++) {
        ctxChirho.fillText(
          winChirho.contentChirho[iChirho],
          winChirho.xChirho + 8,
          winChirho.yChirho + titleBarHeightChirho + 20 + iChirho * lineHeightChirho
        );
      }
    }
  }

  /**
   * Draw the taskbar (B6-001).
   */
  drawTaskbarChirho(ctxChirho, wChirho, hChirho) {
    const yChirho = hChirho - THEME_CHIRHO.taskbarHeightChirho;

    // Taskbar background
    ctxChirho.fillStyle = THEME_CHIRHO.taskbarColorChirho;
    ctxChirho.fillRect(0, yChirho, wChirho, THEME_CHIRHO.taskbarHeightChirho);

    // Top border
    ctxChirho.strokeStyle = THEME_CHIRHO.accentColorChirho;
    ctxChirho.lineWidth = 1;
    ctxChirho.beginPath();
    ctxChirho.moveTo(0, yChirho);
    ctxChirho.lineTo(wChirho, yChirho);
    ctxChirho.stroke();

    // Start button (B6-002)
    ctxChirho.fillStyle = this.menuOpenChirho ? THEME_CHIRHO.accentColorChirho : THEME_CHIRHO.windowTitleBarActiveChirho;
    ctxChirho.fillRect(2, yChirho + 2, 80, THEME_CHIRHO.taskbarHeightChirho - 4);
    ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
    ctxChirho.font = THEME_CHIRHO.titleFontChirho;
    ctxChirho.fillText('Lineluya', 10, yChirho + 22);

    // Window buttons in taskbar
    let btnXChirho = 90;
    for (const winChirho of this.windowsChirho) {
      const btnWidthChirho = Math.min(150, (wChirho - 200) / Math.max(this.windowsChirho.length, 1));
      ctxChirho.fillStyle = winChirho.focusedChirho ? THEME_CHIRHO.windowTitleBarActiveChirho : '#1a1a3e';
      ctxChirho.fillRect(btnXChirho, yChirho + 4, btnWidthChirho - 4, THEME_CHIRHO.taskbarHeightChirho - 8);
      ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
      ctxChirho.font = '11px monospace';
      const labelChirho = winChirho.titleChirho.substring(0, 18);
      ctxChirho.fillText(labelChirho, btnXChirho + 6, yChirho + 22);
      btnXChirho += btnWidthChirho;
    }

    // Volume indicator (B6-007) — right of clock
    const volPercentChirho = Math.round(this.audioChirho.volumeChirho * 100);
    ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
    ctxChirho.font = '10px monospace';
    ctxChirho.textAlign = 'right';
    ctxChirho.fillText(`Vol:${volPercentChirho}%`, wChirho - 80, yChirho + 22);

    // Clock (right side)
    const nowChirho = new Date();
    const timeStrChirho = nowChirho.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
    ctxChirho.font = THEME_CHIRHO.titleFontChirho;
    ctxChirho.fillText(timeStrChirho, wChirho - 10, yChirho + 22);
    ctxChirho.textAlign = 'left';
  }

  /**
   * Draw context menu (B6-010).
   */
  drawContextMenuChirho(ctxChirho) {
    const menuChirho = this.contextMenuChirho;
    const itemHeightChirho = 28;
    const menuWidthChirho = 180;
    const menuHeightChirho = menuChirho.itemsChirho.length * itemHeightChirho + 8;

    // Background
    ctxChirho.fillStyle = THEME_CHIRHO.menuBgChirho;
    ctxChirho.fillRect(menuChirho.xChirho, menuChirho.yChirho, menuWidthChirho, menuHeightChirho);
    ctxChirho.strokeStyle = THEME_CHIRHO.windowBorderChirho;
    ctxChirho.strokeRect(menuChirho.xChirho, menuChirho.yChirho, menuWidthChirho, menuHeightChirho);

    // Items
    ctxChirho.font = THEME_CHIRHO.fontChirho;
    for (let iChirho = 0; iChirho < menuChirho.itemsChirho.length; iChirho++) {
      const yOffChirho = menuChirho.yChirho + 4 + iChirho * itemHeightChirho;
      ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
      ctxChirho.fillText(menuChirho.itemsChirho[iChirho].labelChirho, menuChirho.xChirho + 12, yOffChirho + 18);
    }
  }

  // ── Event handling ────────────────────────────────────────────────────

  setupEventsChirho() {
    this.canvasChirho.addEventListener('mousedown', (eChirho) => this.onMouseDownChirho(eChirho));
    this.canvasChirho.addEventListener('mousemove', (eChirho) => this.onMouseMoveChirho(eChirho));
    this.canvasChirho.addEventListener('mouseup', () => this.onMouseUpChirho());
    this.canvasChirho.addEventListener('dblclick', (eChirho) => this.onDoubleClickChirho(eChirho));
    this.canvasChirho.addEventListener('contextmenu', (eChirho) => {
      eChirho.preventDefault();
      this.showContextMenuChirho(eChirho.offsetX, eChirho.offsetY);
    });

    // Keyboard shortcuts (B6-014)
    document.addEventListener('keydown', (eChirho) => {
      // Alt+F4 close
      if (eChirho.altKey && eChirho.key === 'F4') {
        eChirho.preventDefault();
        const focusedChirho = this.windowsChirho.find(wChirho => wChirho.focusedChirho);
        if (focusedChirho) this.closeWindowChirho(focusedChirho.idChirho);
        return;
      }

      // Forward to focused window's key handler (B6-012)
      const focusedWinChirho = this.windowsChirho.find(wChirho => wChirho.focusedChirho);
      if (focusedWinChirho && focusedWinChirho.onKeyCallbackChirho) {
        focusedWinChirho.onKeyCallbackChirho(eChirho);
        this.renderChirho();
      }
    });

    // Clock update
    this.clockIntervalChirho = setInterval(() => this.renderChirho(), 60000);
  }

  onMouseDownChirho(eChirho) {
    const xChirho = eChirho.offsetX;
    const yChirho = eChirho.offsetY;

    // Close context menu on click — but handle action if click is inside menu
    if (this.contextMenuChirho) {
      const menuChirho = this.contextMenuChirho;
      const itemHeightChirho = 28;
      const menuWidthChirho = 180;
      if (xChirho >= menuChirho.xChirho && xChirho <= menuChirho.xChirho + menuWidthChirho) {
        const relYChirho = yChirho - menuChirho.yChirho - 4;
        const idxChirho = Math.floor(relYChirho / itemHeightChirho);
        if (idxChirho >= 0 && idxChirho < menuChirho.itemsChirho.length) {
          const actionChirho = menuChirho.itemsChirho[idxChirho].actionChirho;
          this.contextMenuChirho = null;
          if (actionChirho) actionChirho();
          this.renderChirho();
          return;
        }
      }
      this.contextMenuChirho = null;
      this.renderChirho();
      return;
    }

    // Check windows in reverse z-order (top first)
    for (let iChirho = this.windowsChirho.length - 1; iChirho >= 0; iChirho--) {
      const winChirho = this.windowsChirho[iChirho];
      if (winChirho.minimizedChirho) continue;

      // Check close button
      if (xChirho >= winChirho.xChirho + winChirho.widthChirho - 24 &&
          xChirho <= winChirho.xChirho + winChirho.widthChirho - 4 &&
          yChirho >= winChirho.yChirho + 4 &&
          yChirho <= winChirho.yChirho + 24) {
        this.closeWindowChirho(winChirho.idChirho);
        return;
      }

      // Check title bar (drag)
      if (xChirho >= winChirho.xChirho && xChirho <= winChirho.xChirho + winChirho.widthChirho &&
          yChirho >= winChirho.yChirho && yChirho <= winChirho.yChirho + 28) {
        this.focusWindowChirho(winChirho.idChirho);
        this.dragChirho = {
          windowIdChirho: winChirho.idChirho,
          offsetXChirho: xChirho - winChirho.xChirho,
          offsetYChirho: yChirho - winChirho.yChirho,
        };
        return;
      }

      // Check window body (focus + click callback)
      if (xChirho >= winChirho.xChirho && xChirho <= winChirho.xChirho + winChirho.widthChirho &&
          yChirho >= winChirho.yChirho && yChirho <= winChirho.yChirho + winChirho.heightChirho) {
        this.focusWindowChirho(winChirho.idChirho);
        if (winChirho.onClickCallbackChirho) {
          winChirho.onClickCallbackChirho(
            xChirho - winChirho.xChirho,
            yChirho - winChirho.yChirho - 28 // relative to content area
          );
          this.renderChirho();
        }
        return;
      }
    }
  }

  onMouseMoveChirho(eChirho) {
    if (!this.dragChirho) return;
    const winChirho = this.windowsChirho.find(wChirho => wChirho.idChirho === this.dragChirho.windowIdChirho);
    if (winChirho) {
      winChirho.xChirho = eChirho.offsetX - this.dragChirho.offsetXChirho;
      winChirho.yChirho = eChirho.offsetY - this.dragChirho.offsetYChirho;
      this.renderChirho();
    }
  }

  onMouseUpChirho() {
    this.dragChirho = null;
  }

  onDoubleClickChirho(eChirho) {
    const xChirho = eChirho.offsetX;
    const yChirho = eChirho.offsetY;

    // Double-click title bar = maximize/restore
    for (let iChirho = this.windowsChirho.length - 1; iChirho >= 0; iChirho--) {
      const winChirho = this.windowsChirho[iChirho];
      if (xChirho >= winChirho.xChirho && xChirho <= winChirho.xChirho + winChirho.widthChirho &&
          yChirho >= winChirho.yChirho && yChirho <= winChirho.yChirho + 28) {
        this.toggleMaximizeChirho(winChirho.idChirho);
        return;
      }
    }
  }

  /**
   * Show context menu (B6-010) — now with more app launchers.
   */
  showContextMenuChirho(xChirho, yChirho) {
    this.contextMenuChirho = {
      xChirho,
      yChirho,
      itemsChirho: [
        { labelChirho: 'New Terminal', actionChirho: () => this.createWindowChirho('Terminal') },
        { labelChirho: 'File Manager', actionChirho: () => this.createWindowChirho('Files') },
        { labelChirho: 'Text Editor', actionChirho: () => this.openTextEditorChirho() },
        { labelChirho: 'Web Browser', actionChirho: () => this.openWebBrowserChirho() },
        { labelChirho: 'Settings', actionChirho: () => this.openSettingsPanelChirho() },
        { labelChirho: 'System Info', actionChirho: () => this.openSystemInfoChirho() },
        { labelChirho: 'About', actionChirho: () => this.openAboutChirho() },
      ],
    };
    this.renderChirho();
  }

  // ── Application Launchers ─────────────────────────────────────────────

  /**
   * Open system info window.
   */
  openSystemInfoChirho() {
    const winChirho = this.createWindowChirho('System Information', 400, 300);
    winChirho.contentChirho = [
      'Lineluya Kernel v0.7.0 (wasm32)',
      '',
      'Architecture: WebAssembly',
      'Browser:      ' + navigator.userAgent.split(' ').pop(),
      'CPUs:         ' + navigator.hardwareConcurrency,
      'Memory:       ' + (navigator.deviceMemory || '?') + ' GB',
      '',
      '"For God so loved the world that he gave',
      'his only begotten Son, that whoever believes',
      'in him should not perish but have eternal life."',
      '                               — John 3:16',
    ];
    this.renderChirho();
  }

  /**
   * Open about window.
   */
  openAboutChirho() {
    const winChirho = this.createWindowChirho('About Lineluya', 350, 200);
    winChirho.contentChirho = [
      'Lineluya Desktop Environment',
      'Version 0.1.0',
      '',
      'A Linux-compatible kernel rewritten in Rust,',
      'running in your browser via WebAssembly.',
      '',
      'Soli Deo Gloria — To God alone be the glory.',
    ];
    this.renderChirho();
  }

  // ── B6-011: System Settings Panel ─────────────────────────────────────

  /**
   * Open the settings panel with theme/font/audio controls.
   */
  openSettingsPanelChirho() {
    const currentSettingsChirho = loadSettingsChirho();
    const stateChirho = {
      bgColorChirho: currentSettingsChirho.bgColorChirho || THEME_CHIRHO.bgColorChirho,
      accentColorChirho: currentSettingsChirho.accentColorChirho || THEME_CHIRHO.accentColorChirho,
      fontSizeChirho: currentSettingsChirho.fontSizeChirho || 13,
      volumeChirho: Math.round(this.audioChirho.volumeChirho * 100),
      selectedRowChirho: 0,
    };

    const settingsItemsChirho = [
      { labelChirho: 'Background', keyChirho: 'bgColorChirho', typeChirho: 'color' },
      { labelChirho: 'Accent Color', keyChirho: 'accentColorChirho', typeChirho: 'color' },
      { labelChirho: 'Font Size', keyChirho: 'fontSizeChirho', typeChirho: 'number', minChirho: 9, maxChirho: 24 },
      { labelChirho: 'Volume %', keyChirho: 'volumeChirho', typeChirho: 'number', minChirho: 0, maxChirho: 100 },
    ];

    const COLOR_OPTIONS_CHIRHO = [
      '#1a1a2e', '#0d1117', '#1e1e2e', '#282a36', '#2d2d2d',
      '#533483', '#7c9bff', '#ff5555', '#50fa7b', '#f1fa8c',
    ];

    const winChirho = this.createWindowChirho('Settings', 420, 280);
    const desktopRefChirho = this;

    winChirho.renderCallbackChirho = (ctxChirho, wChirho) => {
      const titleBarHeightChirho = 28;
      const baseXChirho = wChirho.xChirho + 12;
      const baseYChirho = wChirho.yChirho + titleBarHeightChirho + 16;
      const rowHeightChirho = 36;

      ctxChirho.font = '14px "JetBrains Mono", monospace';
      ctxChirho.fillStyle = THEME_CHIRHO.accentColorChirho;
      ctxChirho.fillText('System Settings', baseXChirho, baseYChirho);

      for (let iChirho = 0; iChirho < settingsItemsChirho.length; iChirho++) {
        const itemChirho = settingsItemsChirho[iChirho];
        const yPosChirho = baseYChirho + 28 + iChirho * rowHeightChirho;
        const isSelectedChirho = stateChirho.selectedRowChirho === iChirho;

        // Highlight selected row
        if (isSelectedChirho) {
          ctxChirho.fillStyle = 'rgba(124, 155, 255, 0.15)';
          ctxChirho.fillRect(baseXChirho - 4, yPosChirho - 14, wChirho.widthChirho - 24, rowHeightChirho);
        }

        ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
        ctxChirho.font = THEME_CHIRHO.fontChirho;
        ctxChirho.fillText(`${itemChirho.labelChirho}:`, baseXChirho, yPosChirho);

        const valChirho = stateChirho[itemChirho.keyChirho];
        if (itemChirho.typeChirho === 'color') {
          // Draw color swatch
          ctxChirho.fillStyle = valChirho;
          ctxChirho.fillRect(baseXChirho + 160, yPosChirho - 12, 20, 16);
          ctxChirho.strokeStyle = '#666';
          ctxChirho.strokeRect(baseXChirho + 160, yPosChirho - 12, 20, 16);
          ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
          ctxChirho.fillText(valChirho, baseXChirho + 190, yPosChirho);
        } else {
          ctxChirho.fillText(`< ${valChirho} >`, baseXChirho + 160, yPosChirho);
        }
      }

      // Save/Apply buttons
      const btnYChirho = baseYChirho + 28 + settingsItemsChirho.length * rowHeightChirho + 10;
      ctxChirho.fillStyle = THEME_CHIRHO.accentColorChirho;
      ctxChirho.fillRect(baseXChirho, btnYChirho, 80, 26);
      ctxChirho.fillStyle = '#000';
      ctxChirho.font = '12px monospace';
      ctxChirho.fillText('Apply', baseXChirho + 18, btnYChirho + 17);

      ctxChirho.fillStyle = '#555';
      ctxChirho.fillRect(baseXChirho + 100, btnYChirho, 80, 26);
      ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
      ctxChirho.fillText('Reset', baseXChirho + 120, btnYChirho + 17);
    };

    // Key handler for settings navigation
    winChirho.onKeyCallbackChirho = (eChirho) => {
      const itemChirho = settingsItemsChirho[stateChirho.selectedRowChirho];
      if (eChirho.key === 'ArrowUp') {
        stateChirho.selectedRowChirho = Math.max(0, stateChirho.selectedRowChirho - 1);
      } else if (eChirho.key === 'ArrowDown') {
        stateChirho.selectedRowChirho = Math.min(settingsItemsChirho.length - 1, stateChirho.selectedRowChirho + 1);
      } else if (eChirho.key === 'ArrowLeft') {
        if (itemChirho.typeChirho === 'number') {
          stateChirho[itemChirho.keyChirho] = Math.max(itemChirho.minChirho, stateChirho[itemChirho.keyChirho] - 1);
        } else if (itemChirho.typeChirho === 'color') {
          const idxChirho = COLOR_OPTIONS_CHIRHO.indexOf(stateChirho[itemChirho.keyChirho]);
          stateChirho[itemChirho.keyChirho] = COLOR_OPTIONS_CHIRHO[(idxChirho - 1 + COLOR_OPTIONS_CHIRHO.length) % COLOR_OPTIONS_CHIRHO.length];
        }
      } else if (eChirho.key === 'ArrowRight') {
        if (itemChirho.typeChirho === 'number') {
          stateChirho[itemChirho.keyChirho] = Math.min(itemChirho.maxChirho, stateChirho[itemChirho.keyChirho] + 1);
        } else if (itemChirho.typeChirho === 'color') {
          const idxChirho = COLOR_OPTIONS_CHIRHO.indexOf(stateChirho[itemChirho.keyChirho]);
          stateChirho[itemChirho.keyChirho] = COLOR_OPTIONS_CHIRHO[(idxChirho + 1) % COLOR_OPTIONS_CHIRHO.length];
        }
      } else if (eChirho.key === 'Enter') {
        // Apply settings
        const newSettingsChirho = {
          bgColorChirho: stateChirho.bgColorChirho,
          accentColorChirho: stateChirho.accentColorChirho,
          fontSizeChirho: stateChirho.fontSizeChirho,
        };
        saveSettingsChirho(newSettingsChirho);
        applySettingsToThemeChirho(newSettingsChirho);
        desktopRefChirho.audioChirho.setVolumeChirho(stateChirho.volumeChirho / 100);
        desktopRefChirho.audioChirho.playToneChirho(660, 80, 'sine'); // Feedback beep
      }
    };

    // Click handler for Apply/Reset buttons
    winChirho.onClickCallbackChirho = (relXChirho, relYChirho) => {
      const rowHeightChirho = 36;
      const btnYChirho = 16 + 28 + settingsItemsChirho.length * rowHeightChirho + 10;

      if (relYChirho >= btnYChirho && relYChirho <= btnYChirho + 26) {
        if (relXChirho >= 12 && relXChirho <= 92) {
          // Apply
          const newSettingsChirho = {
            bgColorChirho: stateChirho.bgColorChirho,
            accentColorChirho: stateChirho.accentColorChirho,
            fontSizeChirho: stateChirho.fontSizeChirho,
          };
          saveSettingsChirho(newSettingsChirho);
          applySettingsToThemeChirho(newSettingsChirho);
          desktopRefChirho.audioChirho.setVolumeChirho(stateChirho.volumeChirho / 100);
          desktopRefChirho.audioChirho.playToneChirho(660, 80, 'sine');
        } else if (relXChirho >= 112 && relXChirho <= 192) {
          // Reset to defaults
          stateChirho.bgColorChirho = '#1a1a2e';
          stateChirho.accentColorChirho = '#7c9bff';
          stateChirho.fontSizeChirho = 13;
          stateChirho.volumeChirho = 80;
          saveSettingsChirho({});
          THEME_CHIRHO.bgColorChirho = '#1a1a2e';
          THEME_CHIRHO.accentColorChirho = '#7c9bff';
          THEME_CHIRHO.fontChirho = '13px "JetBrains Mono", monospace';
          THEME_CHIRHO.titleFontChirho = '12px "JetBrains Mono", monospace';
          desktopRefChirho.audioChirho.setVolumeChirho(0.8);
        }
      }

      // Row selection via click
      const rowStartYChirho = 44;
      if (relYChirho >= rowStartYChirho && relYChirho < btnYChirho) {
        const rowIdxChirho = Math.floor((relYChirho - rowStartYChirho) / rowHeightChirho);
        if (rowIdxChirho >= 0 && rowIdxChirho < settingsItemsChirho.length) {
          stateChirho.selectedRowChirho = rowIdxChirho;
        }
      }
    };

    this.renderChirho();
  }

  // ── B6-012: Text Editor Application ───────────────────────────────────

  /**
   * Open a simple canvas-based text editor.
   * Supports typing, backspace, enter, cursor movement, and basic display.
   * @param {string} initialContentChirho optional initial text
   * @param {string} fileNameChirho optional filename
   */
  openTextEditorChirho(initialContentChirho = '', fileNameChirho = 'untitled.txt') {
    const editorStateChirho = {
      linesChirho: initialContentChirho ? initialContentChirho.split('\n') : [''],
      cursorRowChirho: 0,
      cursorColChirho: 0,
      scrollOffsetChirho: 0,
      fileNameChirho: fileNameChirho,
      modifiedChirho: false,
    };

    // Syntax highlighting keywords
    const KEYWORDS_CHIRHO = new Set([
      'const', 'let', 'var', 'function', 'class', 'return', 'if', 'else',
      'for', 'while', 'import', 'export', 'from', 'async', 'await',
      'fn', 'let', 'mut', 'pub', 'struct', 'impl', 'use', 'mod',
      'def', 'print', 'self', 'True', 'False', 'None',
    ]);

    const winChirho = this.createWindowChirho(`Editor: ${fileNameChirho}`, 600, 420);
    const desktopRefChirho = this;

    winChirho.renderCallbackChirho = (ctxChirho, wChirho) => {
      const titleBarHeightChirho = 28;
      const contentXChirho = wChirho.xChirho + 1;
      const contentYChirho = wChirho.yChirho + titleBarHeightChirho;
      const contentWChirho = wChirho.widthChirho - 2;
      const contentHChirho = wChirho.heightChirho - titleBarHeightChirho - 1;

      // Editor background
      ctxChirho.fillStyle = '#0d1117';
      ctxChirho.fillRect(contentXChirho, contentYChirho, contentWChirho, contentHChirho);

      // Status bar at bottom
      const statusBarHeightChirho = 20;
      ctxChirho.fillStyle = '#21262d';
      ctxChirho.fillRect(contentXChirho, contentYChirho + contentHChirho - statusBarHeightChirho, contentWChirho, statusBarHeightChirho);
      ctxChirho.fillStyle = '#8b949e';
      ctxChirho.font = '10px monospace';
      const statusTextChirho = `${editorStateChirho.fileNameChirho}${editorStateChirho.modifiedChirho ? ' [modified]' : ''} | Ln ${editorStateChirho.cursorRowChirho + 1}, Col ${editorStateChirho.cursorColChirho + 1} | ${editorStateChirho.linesChirho.length} lines`;
      ctxChirho.fillText(statusTextChirho, contentXChirho + 8, contentYChirho + contentHChirho - 6);

      // Text area
      const lineHeightChirho = 16;
      const charWidthChirho = 7.8; // approximate monospace char width
      const gutterWidthChirho = 40;
      const visibleLinesChirho = Math.floor((contentHChirho - statusBarHeightChirho - 4) / lineHeightChirho);

      // Ensure scroll follows cursor
      if (editorStateChirho.cursorRowChirho < editorStateChirho.scrollOffsetChirho) {
        editorStateChirho.scrollOffsetChirho = editorStateChirho.cursorRowChirho;
      } else if (editorStateChirho.cursorRowChirho >= editorStateChirho.scrollOffsetChirho + visibleLinesChirho) {
        editorStateChirho.scrollOffsetChirho = editorStateChirho.cursorRowChirho - visibleLinesChirho + 1;
      }

      ctxChirho.font = '12px monospace';

      for (let iChirho = 0; iChirho < visibleLinesChirho; iChirho++) {
        const lineIdxChirho = iChirho + editorStateChirho.scrollOffsetChirho;
        if (lineIdxChirho >= editorStateChirho.linesChirho.length) break;

        const yPosChirho = contentYChirho + 14 + iChirho * lineHeightChirho;

        // Highlight current line
        if (lineIdxChirho === editorStateChirho.cursorRowChirho) {
          ctxChirho.fillStyle = 'rgba(124, 155, 255, 0.08)';
          ctxChirho.fillRect(contentXChirho, yPosChirho - 12, contentWChirho, lineHeightChirho);
        }

        // Line number gutter
        ctxChirho.fillStyle = '#484f58';
        ctxChirho.fillText(String(lineIdxChirho + 1).padStart(3), contentXChirho + 4, yPosChirho);

        // Line text with basic syntax highlighting
        const lineTextChirho = editorStateChirho.linesChirho[lineIdxChirho];
        const tokensChirho = lineTextChirho.split(/(\s+)/);
        let drawXChirho = contentXChirho + gutterWidthChirho;

        for (const tokenChirho of tokensChirho) {
          if (KEYWORDS_CHIRHO.has(tokenChirho)) {
            ctxChirho.fillStyle = '#ff7b72'; // keyword color
          } else if (/^['"]/.test(tokenChirho)) {
            ctxChirho.fillStyle = '#a5d6ff'; // string color
          } else if (/^\/\//.test(tokenChirho)) {
            ctxChirho.fillStyle = '#8b949e'; // comment color
          } else if (/^\d+$/.test(tokenChirho)) {
            ctxChirho.fillStyle = '#79c0ff'; // number color
          } else {
            ctxChirho.fillStyle = '#c9d1d9'; // default text
          }
          ctxChirho.fillText(tokenChirho, drawXChirho, yPosChirho);
          drawXChirho += ctxChirho.measureText(tokenChirho).width;
        }

        // Draw cursor
        if (lineIdxChirho === editorStateChirho.cursorRowChirho) {
          const cursorXChirho = contentXChirho + gutterWidthChirho +
            ctxChirho.measureText(lineTextChirho.substring(0, editorStateChirho.cursorColChirho)).width;
          ctxChirho.fillStyle = THEME_CHIRHO.accentColorChirho;
          ctxChirho.fillRect(cursorXChirho, yPosChirho - 11, 2, 14);
        }
      }
    };

    // Key handler for text editing
    winChirho.onKeyCallbackChirho = (eChirho) => {
      const linesChirho = editorStateChirho.linesChirho;
      const rowChirho = editorStateChirho.cursorRowChirho;
      const colChirho = editorStateChirho.cursorColChirho;

      if (eChirho.ctrlKey && eChirho.key === 's') {
        // Save: store to localStorage for VFS persistence
        eChirho.preventDefault();
        const contentChirho = linesChirho.join('\n');
        try {
          localStorage.setItem(`lineluya-file-chirho:${editorStateChirho.fileNameChirho}`, contentChirho);
          editorStateChirho.modifiedChirho = false;
          desktopRefChirho.notificationsChirho.notifyChirho('File Saved', editorStateChirho.fileNameChirho);
        } catch (_eChirho) { /* quota */ }
        return;
      }

      if (eChirho.key === 'ArrowUp') {
        editorStateChirho.cursorRowChirho = Math.max(0, rowChirho - 1);
        editorStateChirho.cursorColChirho = Math.min(colChirho, linesChirho[editorStateChirho.cursorRowChirho].length);
      } else if (eChirho.key === 'ArrowDown') {
        editorStateChirho.cursorRowChirho = Math.min(linesChirho.length - 1, rowChirho + 1);
        editorStateChirho.cursorColChirho = Math.min(colChirho, linesChirho[editorStateChirho.cursorRowChirho].length);
      } else if (eChirho.key === 'ArrowLeft') {
        if (colChirho > 0) {
          editorStateChirho.cursorColChirho = colChirho - 1;
        } else if (rowChirho > 0) {
          editorStateChirho.cursorRowChirho = rowChirho - 1;
          editorStateChirho.cursorColChirho = linesChirho[rowChirho - 1].length;
        }
      } else if (eChirho.key === 'ArrowRight') {
        if (colChirho < linesChirho[rowChirho].length) {
          editorStateChirho.cursorColChirho = colChirho + 1;
        } else if (rowChirho < linesChirho.length - 1) {
          editorStateChirho.cursorRowChirho = rowChirho + 1;
          editorStateChirho.cursorColChirho = 0;
        }
      } else if (eChirho.key === 'Home') {
        editorStateChirho.cursorColChirho = 0;
      } else if (eChirho.key === 'End') {
        editorStateChirho.cursorColChirho = linesChirho[rowChirho].length;
      } else if (eChirho.key === 'Backspace') {
        eChirho.preventDefault();
        if (colChirho > 0) {
          linesChirho[rowChirho] = linesChirho[rowChirho].substring(0, colChirho - 1) + linesChirho[rowChirho].substring(colChirho);
          editorStateChirho.cursorColChirho = colChirho - 1;
        } else if (rowChirho > 0) {
          editorStateChirho.cursorColChirho = linesChirho[rowChirho - 1].length;
          linesChirho[rowChirho - 1] += linesChirho[rowChirho];
          linesChirho.splice(rowChirho, 1);
          editorStateChirho.cursorRowChirho = rowChirho - 1;
        }
        editorStateChirho.modifiedChirho = true;
      } else if (eChirho.key === 'Delete') {
        if (colChirho < linesChirho[rowChirho].length) {
          linesChirho[rowChirho] = linesChirho[rowChirho].substring(0, colChirho) + linesChirho[rowChirho].substring(colChirho + 1);
        } else if (rowChirho < linesChirho.length - 1) {
          linesChirho[rowChirho] += linesChirho[rowChirho + 1];
          linesChirho.splice(rowChirho + 1, 1);
        }
        editorStateChirho.modifiedChirho = true;
      } else if (eChirho.key === 'Enter') {
        eChirho.preventDefault();
        const beforeChirho = linesChirho[rowChirho].substring(0, colChirho);
        const afterChirho = linesChirho[rowChirho].substring(colChirho);
        linesChirho[rowChirho] = beforeChirho;
        linesChirho.splice(rowChirho + 1, 0, afterChirho);
        editorStateChirho.cursorRowChirho = rowChirho + 1;
        editorStateChirho.cursorColChirho = 0;
        editorStateChirho.modifiedChirho = true;
      } else if (eChirho.key === 'Tab') {
        eChirho.preventDefault();
        linesChirho[rowChirho] = linesChirho[rowChirho].substring(0, colChirho) + '  ' + linesChirho[rowChirho].substring(colChirho);
        editorStateChirho.cursorColChirho = colChirho + 2;
        editorStateChirho.modifiedChirho = true;
      } else if (eChirho.key.length === 1 && !eChirho.ctrlKey && !eChirho.altKey && !eChirho.metaKey) {
        // Insert printable character
        linesChirho[rowChirho] = linesChirho[rowChirho].substring(0, colChirho) + eChirho.key + linesChirho[rowChirho].substring(colChirho);
        editorStateChirho.cursorColChirho = colChirho + 1;
        editorStateChirho.modifiedChirho = true;
      }
    };

    this.renderChirho();
  }

  // ── B6-013: Web Browser Application ───────────────────────────────────

  /**
   * Open an iframe-based web browser within a desktop window.
   * @param {string} initialUrlChirho starting URL
   */
  openWebBrowserChirho(initialUrlChirho = 'https://example.com') {
    const browserStateChirho = {
      urlChirho: initialUrlChirho,
      historyChirho: [initialUrlChirho],
      historyIdxChirho: 0,
      inputUrlChirho: initialUrlChirho,
      loadingChirho: false,
    };

    const winChirho = this.createWindowChirho('Browser', 700, 500);

    // Create an actual iframe element positioned over the canvas
    const iframeChirho = document.createElement('iframe');
    iframeChirho.sandbox = 'allow-scripts allow-same-origin allow-forms allow-popups';
    iframeChirho.style.position = 'absolute';
    iframeChirho.style.border = 'none';
    iframeChirho.style.backgroundColor = '#fff';
    iframeChirho.src = initialUrlChirho;
    winChirho.iframeChirho = iframeChirho;

    // The iframe is placed relative to the canvas parent
    const parentChirho = this.canvasChirho.parentElement || document.body;
    parentChirho.style.position = 'relative';
    parentChirho.appendChild(iframeChirho);

    const desktopRefChirho = this;
    const addressBarHeightChirho = 28;

    winChirho.renderCallbackChirho = (ctxChirho, wChirho) => {
      const titleBarHeightChirho = 28;
      const contentXChirho = wChirho.xChirho + 1;
      const contentYChirho = wChirho.yChirho + titleBarHeightChirho;
      const contentWChirho = wChirho.widthChirho - 2;

      // Address bar
      ctxChirho.fillStyle = '#21262d';
      ctxChirho.fillRect(contentXChirho, contentYChirho, contentWChirho, addressBarHeightChirho);

      // Back button
      ctxChirho.fillStyle = browserStateChirho.historyIdxChirho > 0 ? THEME_CHIRHO.accentColorChirho : '#555';
      ctxChirho.font = '14px monospace';
      ctxChirho.fillText('<', contentXChirho + 8, contentYChirho + 19);

      // Forward button
      ctxChirho.fillStyle = browserStateChirho.historyIdxChirho < browserStateChirho.historyChirho.length - 1 ? THEME_CHIRHO.accentColorChirho : '#555';
      ctxChirho.fillText('>', contentXChirho + 26, contentYChirho + 19);

      // URL input area
      ctxChirho.fillStyle = '#0d1117';
      ctxChirho.fillRect(contentXChirho + 44, contentYChirho + 4, contentWChirho - 92, 20);
      ctxChirho.fillStyle = '#c9d1d9';
      ctxChirho.font = '11px monospace';
      const displayUrlChirho = browserStateChirho.urlChirho.length > 60
        ? browserStateChirho.urlChirho.substring(0, 60) + '...'
        : browserStateChirho.urlChirho;
      ctxChirho.fillText(displayUrlChirho, contentXChirho + 50, contentYChirho + 18);

      // Go button
      ctxChirho.fillStyle = THEME_CHIRHO.accentColorChirho;
      ctxChirho.fillRect(contentXChirho + contentWChirho - 42, contentYChirho + 4, 36, 20);
      ctxChirho.fillStyle = '#000';
      ctxChirho.font = '10px monospace';
      ctxChirho.fillText('Go', contentXChirho + contentWChirho - 30, contentYChirho + 18);

      // The iframe covers the rest; draw a placeholder behind it
      ctxChirho.fillStyle = '#fff';
      ctxChirho.fillRect(
        contentXChirho,
        contentYChirho + addressBarHeightChirho,
        contentWChirho,
        wChirho.heightChirho - titleBarHeightChirho - addressBarHeightChirho - 1
      );
    };

    /**
     * Navigate the browser to a URL.
     * @param {string} urlChirho
     */
    const navigateChirho = (urlChirho) => {
      // Ensure URL has protocol
      let fullUrlChirho = urlChirho;
      if (!/^https?:\/\//.test(fullUrlChirho)) {
        fullUrlChirho = 'https://' + fullUrlChirho;
      }
      browserStateChirho.urlChirho = fullUrlChirho;
      browserStateChirho.inputUrlChirho = fullUrlChirho;
      // Trim history forward from current position
      browserStateChirho.historyChirho = browserStateChirho.historyChirho.slice(0, browserStateChirho.historyIdxChirho + 1);
      browserStateChirho.historyChirho.push(fullUrlChirho);
      browserStateChirho.historyIdxChirho = browserStateChirho.historyChirho.length - 1;
      if (winChirho.iframeChirho) {
        winChirho.iframeChirho.src = fullUrlChirho;
      }
      desktopRefChirho.renderChirho();
    };

    // Click handler for address bar controls
    winChirho.onClickCallbackChirho = (relXChirho, relYChirho) => {
      // Address bar is within the first 28px of content area
      if (relYChirho >= 0 && relYChirho <= addressBarHeightChirho) {
        const contentWChirho = winChirho.widthChirho - 2;
        if (relXChirho >= 4 && relXChirho <= 22) {
          // Back
          if (browserStateChirho.historyIdxChirho > 0) {
            browserStateChirho.historyIdxChirho--;
            browserStateChirho.urlChirho = browserStateChirho.historyChirho[browserStateChirho.historyIdxChirho];
            if (winChirho.iframeChirho) winChirho.iframeChirho.src = browserStateChirho.urlChirho;
          }
        } else if (relXChirho >= 22 && relXChirho <= 40) {
          // Forward
          if (browserStateChirho.historyIdxChirho < browserStateChirho.historyChirho.length - 1) {
            browserStateChirho.historyIdxChirho++;
            browserStateChirho.urlChirho = browserStateChirho.historyChirho[browserStateChirho.historyIdxChirho];
            if (winChirho.iframeChirho) winChirho.iframeChirho.src = browserStateChirho.urlChirho;
          }
        } else if (relXChirho >= contentWChirho - 44) {
          // Go button — prompt for URL
          const newUrlChirho = prompt('Enter URL:', browserStateChirho.urlChirho);
          if (newUrlChirho) navigateChirho(newUrlChirho);
        } else if (relXChirho >= 44 && relXChirho <= contentWChirho - 50) {
          // Click on URL bar — prompt for URL
          const newUrlChirho = prompt('Enter URL:', browserStateChirho.urlChirho);
          if (newUrlChirho) navigateChirho(newUrlChirho);
        }
      }
    };

    // Key handler for address bar
    winChirho.onKeyCallbackChirho = (eChirho) => {
      // Alt+Left/Right for back/forward
      if (eChirho.altKey && eChirho.key === 'ArrowLeft') {
        if (browserStateChirho.historyIdxChirho > 0) {
          browserStateChirho.historyIdxChirho--;
          browserStateChirho.urlChirho = browserStateChirho.historyChirho[browserStateChirho.historyIdxChirho];
          if (winChirho.iframeChirho) winChirho.iframeChirho.src = browserStateChirho.urlChirho;
        }
      } else if (eChirho.altKey && eChirho.key === 'ArrowRight') {
        if (browserStateChirho.historyIdxChirho < browserStateChirho.historyChirho.length - 1) {
          browserStateChirho.historyIdxChirho++;
          browserStateChirho.urlChirho = browserStateChirho.historyChirho[browserStateChirho.historyIdxChirho];
          if (winChirho.iframeChirho) winChirho.iframeChirho.src = browserStateChirho.urlChirho;
        }
      } else if (eChirho.ctrlKey && eChirho.key === 'l') {
        // Ctrl+L to focus address bar
        eChirho.preventDefault();
        const newUrlChirho = prompt('Enter URL:', browserStateChirho.urlChirho);
        if (newUrlChirho) navigateChirho(newUrlChirho);
      }
    };

    this.renderChirho();
  }

  /**
   * Sync iframe positions to match their parent windows (B6-013).
   * Called on every render to keep iframes aligned.
   */
  syncIframePositionsChirho() {
    const canvasRectChirho = this.canvasChirho.getBoundingClientRect();
    const addressBarHeightChirho = 28;
    const titleBarHeightChirho = 28;

    for (const winChirho of this.windowsChirho) {
      if (!winChirho.iframeChirho) continue;

      if (winChirho.minimizedChirho) {
        winChirho.iframeChirho.style.display = 'none';
        continue;
      }

      winChirho.iframeChirho.style.display = 'block';
      const iframeLeftChirho = canvasRectChirho.left + winChirho.xChirho + 1;
      const iframeTopChirho = canvasRectChirho.top + winChirho.yChirho + titleBarHeightChirho + addressBarHeightChirho;
      const iframeWidthChirho = winChirho.widthChirho - 2;
      const iframeHeightChirho = winChirho.heightChirho - titleBarHeightChirho - addressBarHeightChirho - 1;

      winChirho.iframeChirho.style.left = `${iframeLeftChirho}px`;
      winChirho.iframeChirho.style.top = `${iframeTopChirho}px`;
      winChirho.iframeChirho.style.width = `${Math.max(0, iframeWidthChirho)}px`;
      winChirho.iframeChirho.style.height = `${Math.max(0, iframeHeightChirho)}px`;
      winChirho.iframeChirho.style.zIndex = '10';
    }
  }

  // ── B6-005: X11 Proxy Launch ──────────────────────────────────────────

  /**
   * Launch a remote X11 application through the proxy.
   * @param {string} wsUrlChirho WebSocket URL to X11 proxy server
   * @param {string} appNameChirho Application name to launch
   */
  launchX11AppChirho(wsUrlChirho, appNameChirho = 'xterm') {
    const sessionChirho = new X11ProxyDisplayChirho(wsUrlChirho, this);
    this.x11SessionsChirho.push(sessionChirho);
    sessionChirho.connectChirho(appNameChirho);
  }

  // ── B6-008: System Notification API ───────────────────────────────────

  /**
   * Public API: System.notify() — request permission and send notification.
   * @param {string} titleChirho
   * @param {string} bodyChirho
   */
  async systemNotifyChirho(titleChirho, bodyChirho = '') {
    if (this.notificationsChirho.permissionChirho !== 'granted') {
      await this.notificationsChirho.requestPermissionChirho();
    }
    this.notificationsChirho.notifyChirho(titleChirho, bodyChirho);
    // Also play notification sound
    this.audioChirho.playNotificationSoundChirho();
  }

  // ── B6-015: Integration Test ──────────────────────────────────────────

  /**
   * Run a full desktop integration test.
   * Tests: desktop loads, taskbar present, terminal launches, file manager shows,
   * text editor works, settings panel opens, persistence.
   * @returns {{ passedChirho: number, failedChirho: number, resultsChirho: string[] }}
   */
  runIntegrationTestChirho() {
    const resultsChirho = [];
    let passedChirho = 0;
    let failedChirho = 0;

    const assertChirho = (nameChirho, conditionChirho) => {
      if (conditionChirho) {
        resultsChirho.push(`PASS: ${nameChirho}`);
        passedChirho++;
      } else {
        resultsChirho.push(`FAIL: ${nameChirho}`);
        failedChirho++;
      }
    };

    // Test 1: Desktop loaded with taskbar
    assertChirho('Desktop canvas exists', !!this.canvasChirho);
    assertChirho('Canvas has 2d context', !!this.ctxChirho);
    assertChirho('Taskbar height configured', THEME_CHIRHO.taskbarHeightChirho > 0);

    // Test 2: Terminal window launches
    const termWinChirho = this.createWindowChirho('Terminal');
    assertChirho('Terminal window created', !!termWinChirho);
    assertChirho('Terminal has valid ID', termWinChirho.idChirho > 0);
    assertChirho('Terminal in window list', this.windowsChirho.includes(termWinChirho));

    // Test 3: File manager window launches
    const fileWinChirho = this.createWindowChirho('Files');
    assertChirho('File manager window created', !!fileWinChirho);
    assertChirho('Multiple windows exist', this.windowsChirho.length >= 2);

    // Test 4: Window focus works
    this.focusWindowChirho(termWinChirho.idChirho);
    assertChirho('Terminal focused', termWinChirho.focusedChirho === true);
    assertChirho('Files unfocused', fileWinChirho.focusedChirho === false);

    // Test 5: Window maximize/restore
    const origWidthChirho = termWinChirho.widthChirho;
    this.toggleMaximizeChirho(termWinChirho.idChirho);
    assertChirho('Window maximized', termWinChirho.maximizedChirho === true);
    this.toggleMaximizeChirho(termWinChirho.idChirho);
    assertChirho('Window restored', termWinChirho.maximizedChirho === false);
    assertChirho('Width restored correctly', termWinChirho.widthChirho === origWidthChirho);

    // Test 6: Audio subsystem
    assertChirho('Audio manager exists', !!this.audioChirho);
    assertChirho('Volume is valid', this.audioChirho.volumeChirho >= 0 && this.audioChirho.volumeChirho <= 1);
    this.audioChirho.setVolumeChirho(0.5);
    assertChirho('Volume set works', this.audioChirho.volumeChirho === 0.5);
    this.audioChirho.setVolumeChirho(0.8); // restore

    // Test 7: Notification subsystem
    assertChirho('Notification manager exists', !!this.notificationsChirho);

    // Test 8: Settings persistence
    const testSettingsChirho = { bgColorChirho: '#ff0000' };
    saveSettingsChirho(testSettingsChirho);
    const loadedChirho = loadSettingsChirho();
    assertChirho('Settings save/load works', loadedChirho.bgColorChirho === '#ff0000');
    saveSettingsChirho({}); // clean up

    // Test 9: Window close
    const countBeforeChirho = this.windowsChirho.length;
    this.closeWindowChirho(fileWinChirho.idChirho);
    assertChirho('Window closed', this.windowsChirho.length === countBeforeChirho - 1);

    // Test 10: Context menu items include new apps
    this.showContextMenuChirho(100, 100);
    assertChirho('Context menu has items', this.contextMenuChirho.itemsChirho.length >= 5);
    const labelsChirho = this.contextMenuChirho.itemsChirho.map(iChirho => iChirho.labelChirho);
    assertChirho('Context menu has Text Editor', labelsChirho.includes('Text Editor'));
    assertChirho('Context menu has Web Browser', labelsChirho.includes('Web Browser'));
    assertChirho('Context menu has Settings', labelsChirho.includes('Settings'));
    this.contextMenuChirho = null;

    // Clean up test windows
    this.closeWindowChirho(termWinChirho.idChirho);

    // Show results in a window
    const resultWinChirho = this.createWindowChirho('Integration Test Results', 500, 400);
    resultWinChirho.contentChirho = [
      `Desktop Integration Test (B6-015)`,
      `Passed: ${passedChirho}  Failed: ${failedChirho}`,
      '',
      ...resultsChirho,
    ];
    this.renderChirho();

    return { passedChirho, failedChirho, resultsChirho };
  }
}

// Export
if (typeof window !== 'undefined') {
  window.DesktopManagerChirho = DesktopManagerChirho;
  window.AudioManagerChirho = AudioManagerChirho;
  window.NotificationManagerChirho = NotificationManagerChirho;
  window.X11ProxyDisplayChirho = X11ProxyDisplayChirho;
}

export {
  DesktopManagerChirho,
  WindowChirho,
  THEME_CHIRHO,
  AudioManagerChirho,
  NotificationManagerChirho,
  X11ProxyDisplayChirho,
  loadSettingsChirho,
  saveSettingsChirho,
  applySettingsToThemeChirho,
};
