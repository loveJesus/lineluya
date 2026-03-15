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
 * B6-009: Desktop wallpaper and theming
 * B6-010: Right-click context menus
 * B6-014: Keyboard shortcuts and hotkeys
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

    // Clock (right side)
    const nowChirho = new Date();
    const timeStrChirho = nowChirho.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    ctxChirho.fillStyle = THEME_CHIRHO.textColorChirho;
    ctxChirho.font = THEME_CHIRHO.titleFontChirho;
    ctxChirho.textAlign = 'right';
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
      if (eChirho.altKey && eChirho.key === 'F4') {
        eChirho.preventDefault();
        const focusedChirho = this.windowsChirho.find(wChirho => wChirho.focusedChirho);
        if (focusedChirho) this.closeWindowChirho(focusedChirho.idChirho);
      }
    });

    // Clock update
    this.clockIntervalChirho = setInterval(() => this.renderChirho(), 60000);
  }

  onMouseDownChirho(eChirho) {
    const xChirho = eChirho.offsetX;
    const yChirho = eChirho.offsetY;

    // Close context menu on click
    if (this.contextMenuChirho) {
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

      // Check window body (focus only)
      if (xChirho >= winChirho.xChirho && xChirho <= winChirho.xChirho + winChirho.widthChirho &&
          yChirho >= winChirho.yChirho && yChirho <= winChirho.yChirho + winChirho.heightChirho) {
        this.focusWindowChirho(winChirho.idChirho);
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
   * Show context menu (B6-010).
   */
  showContextMenuChirho(xChirho, yChirho) {
    this.contextMenuChirho = {
      xChirho,
      yChirho,
      itemsChirho: [
        { labelChirho: 'New Terminal', actionChirho: () => this.createWindowChirho('Terminal') },
        { labelChirho: 'File Manager', actionChirho: () => this.createWindowChirho('Files') },
        { labelChirho: 'System Info', actionChirho: () => this.openSystemInfoChirho() },
        { labelChirho: 'About', actionChirho: () => this.openAboutChirho() },
      ],
    };
    this.renderChirho();
  }

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
}

// Export
if (typeof window !== 'undefined') {
  window.DesktopManagerChirho = DesktopManagerChirho;
}

export { DesktopManagerChirho, WindowChirho, THEME_CHIRHO };
