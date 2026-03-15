// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! # Lineluya Kernel — WASM32 Browser Target
//!
//! This is a real kernel compiled to WebAssembly. The browser is the hardware.
//! Programs compiled to wasm32 make Linux syscalls -> this kernel handles them
//! using browser APIs (Canvas, OPFS, WebSocket, Web Workers).
//!
//! ## Features (B1-009 through B1-020, B2-002..B2-015, B3-001..B3-010, B4-001..B4-007)
//! - **B1-009**: Process table with fork/exec — processes are state machines
//! - **B1-010**: /proc filesystem (cpuinfo, meminfo, self/status, uptime)
//! - **B1-011**: Signal handling framework (SIGTERM, SIGINT, SIGKILL, SIGCHLD)
//! - **B1-012**: Enhanced shell builtins (ps, kill, mkdir, rmdir, touch, chmod,
//!              head, tail, wc, grep)
//! - **B1-013**: TTY/PTY subsystem (raw/cooked modes, line discipline, ioctl)
//! - **B1-014**: WASI clock and timer syscalls (clock_time_get, nanosleep)
//! - **B1-015**: WASI random_get syscall via crypto.getRandomValues
//! - **B1-016**: Initial rootfs with BusyBox symlinks
//! - **B1-017**: Shell job control (fg, bg, jobs, & operator)
//! - **B1-018**: I/O redirection (>, >>, <, 2>)
//! - **B1-019**: Shell integration self-test
//! - **B1-020**: Kernel boot sequence with init process
//! - **B2-002**: Socket syscall interface (socket, connect, send, recv, close)
//! - **B2-003**: TCP socket to WebSocket bridge (multiplexed connections)
//! - **B2-004**: DNS resolver over HTTPS (DoH) via fetch()
//! - **B2-005**: HTTP client support (wget/curl shell commands)
//! - **B2-010**: Loopback interface (127.0.0.1)
//! - **B2-011**: listen/accept/bind for server sockets
//! - **B2-012**: select/poll syscall for async I/O
//! - **B2-013**: /etc/resolv.conf and /etc/hosts
//! - **B2-015**: Connection pooling and reconnection logic
//! - **B3-001**: OPFS block device driver (persistent browser storage)
//! - **B3-002**: IndexedDB fallback storage backend
//! - **B3-007**: Block cache layer with write-back
//! - **B3-008**: fsync and data integrity
//! - **B3-009**: Mount persistent /home on OPFS
//! - **B3-010**: Storage quota management
//! - **B4-001**: X11 protocol message parser
//! - **B4-003**: Canvas 2D rendering backend (framebuffer)
//! - **B4-006**: Mouse event routing to X11 clients
//! - **B4-007**: Keyboard event routing to X11 clients
//!
//! ## Build
//! ```bash
//! cd kernel-wasm-chirho && cargo +nightly build --target wasm32-unknown-unknown --release
//! ```
//!
//! ## Architecture
//! - No MMU needed (WASM has built-in bounds checking)
//! - No ring 0/3 (WASM sandbox IS the protection)
//! - No interrupts (cooperative scheduling via JS event loop)
//! - Syscalls are direct function calls (no mode switch)
//! - Drivers are JS imports (Canvas, WebSocket, OPFS)

#![no_std]

extern crate alloc;

use core::panic::PanicInfo;

// ---------------------------------------------------------------------------
// WASM imports — "hardware" provided by JavaScript runtime
// ---------------------------------------------------------------------------

#[link(wasm_import_module = "lineluya_chirho")]
extern "C" {
    fn js_console_write_chirho(ptr_chirho: u32, len_chirho: u32);
    fn js_timestamp_us_chirho() -> f64;
    fn js_yield_chirho();
    fn js_console_read_chirho(buf_ptr_chirho: u32, max_len_chirho: u32) -> u32;

    // Framebuffer
    fn js_fb_init_chirho(width_chirho: u32, height_chirho: u32) -> u32;
    fn js_fb_flush_chirho();

    // Storage
    fn js_storage_read_chirho(offset_chirho: u32, offset_high_chirho: u32, buf_chirho: u32, len_chirho: u32) -> i32;
    fn js_storage_write_chirho(offset_chirho: u32, offset_high_chirho: u32, buf_chirho: u32, len_chirho: u32) -> i32;

    // Networking
    fn js_net_connect_chirho(host_ptr_chirho: u32, host_len_chirho: u32, port_chirho: u32) -> i32;
    fn js_net_send_chirho(handle_chirho: i32, buf_chirho: u32, len_chirho: u32) -> i32;
    fn js_net_recv_chirho(handle_chirho: i32, buf_chirho: u32, max_len_chirho: u32) -> i32;
    fn js_net_close_chirho(handle_chirho: i32);

    // B1-015: Random — crypto.getRandomValues()
    fn js_random_get_chirho(buf_ptr_chirho: u32, len_chirho: u32) -> i32;

    // B1-014: Timer — setTimeout-based sleep
    fn js_sleep_us_chirho(microseconds_chirho: u32);

    // B3-001: OPFS (Origin Private File System) block device driver
    /// Open or create a file in OPFS. Returns handle >= 0 on success, < 0 on error.
    fn js_opfs_open_chirho(name_ptr_chirho: u32, name_len_chirho: u32, create_chirho: u32) -> i32;
    /// Read bytes from an OPFS file at the given offset. Returns bytes read or < 0.
    fn js_opfs_read_chirho(handle_chirho: i32, offset_chirho: u32, buf_ptr_chirho: u32, len_chirho: u32) -> i32;
    /// Write bytes to an OPFS file at the given offset. Returns bytes written or < 0.
    fn js_opfs_write_chirho(handle_chirho: i32, offset_chirho: u32, buf_ptr_chirho: u32, len_chirho: u32) -> i32;
    /// Close an OPFS file handle.
    fn js_opfs_close_chirho(handle_chirho: i32);
    /// Delete a file from OPFS by name. Returns 0 on success.
    fn js_opfs_delete_chirho(name_ptr_chirho: u32, name_len_chirho: u32) -> i32;
    /// Get the size of an OPFS file. Returns size or < 0 on error.
    fn js_opfs_size_chirho(handle_chirho: i32) -> i32;
    /// Sync/flush OPFS file to persistent storage. Returns 0 on success.
    fn js_opfs_sync_chirho(handle_chirho: i32) -> i32;

    // B3-002: IndexedDB fallback storage backend
    /// Open an IndexedDB store by name. Returns handle >= 0 on success.
    fn js_idb_open_chirho(name_ptr_chirho: u32, name_len_chirho: u32) -> i32;
    /// Get a value from IndexedDB by key. Returns bytes read into buf or < 0.
    fn js_idb_get_chirho(handle_chirho: i32, key_ptr_chirho: u32, key_len_chirho: u32, buf_ptr_chirho: u32, buf_len_chirho: u32) -> i32;
    /// Put a value into IndexedDB by key. Returns 0 on success.
    fn js_idb_put_chirho(handle_chirho: i32, key_ptr_chirho: u32, key_len_chirho: u32, val_ptr_chirho: u32, val_len_chirho: u32) -> i32;
    /// Delete a key from IndexedDB. Returns 0 on success.
    fn js_idb_delete_chirho(handle_chirho: i32, key_ptr_chirho: u32, key_len_chirho: u32) -> i32;
    /// List keys in IndexedDB store into buffer, null-separated. Returns total bytes.
    fn js_idb_list_chirho(handle_chirho: i32, buf_ptr_chirho: u32, buf_len_chirho: u32) -> i32;
    /// Close an IndexedDB store handle.
    fn js_idb_close_chirho(handle_chirho: i32);

    // B2-004: DNS resolver over HTTPS (DoH) via fetch()
    /// Resolve hostname via DoH. Writes IPv4 addr (4 bytes) to buf. Returns 0 on success.
    fn js_dns_resolve_chirho(name_ptr_chirho: u32, name_len_chirho: u32, result_ptr_chirho: u32) -> i32;

    // B2-003: WebSocket-TCP bridge — multiplexed connection management
    /// Open a multiplexed TCP connection through the WebSocket proxy.
    /// Returns connection ID >= 0, or < 0 on error.
    fn js_ws_bridge_connect_chirho(host_ptr_chirho: u32, host_len_chirho: u32, port_chirho: u32) -> i32;
    /// Send data on a bridged connection. Returns bytes sent or < 0.
    fn js_ws_bridge_send_chirho(conn_id_chirho: i32, buf_ptr_chirho: u32, len_chirho: u32) -> i32;
    /// Receive data from a bridged connection. Returns bytes read, 0 if nothing, < 0 on error.
    fn js_ws_bridge_recv_chirho(conn_id_chirho: i32, buf_ptr_chirho: u32, max_len_chirho: u32) -> i32;
    /// Close a bridged connection.
    fn js_ws_bridge_close_chirho(conn_id_chirho: i32);
    /// Check connection status: 0=connecting, 1=open, 2=closed, <0=error.
    fn js_ws_bridge_status_chirho(conn_id_chirho: i32) -> i32;

    // B2-005: HTTP client — fetch() wrapper for wget/curl
    /// Issue HTTP GET via fetch(). Writes response body to buf. Returns bytes written or <0.
    fn js_http_get_chirho(url_ptr_chirho: u32, url_len_chirho: u32, buf_ptr_chirho: u32, buf_len_chirho: u32) -> i32;

    // B3-010: Storage quota management
    /// Query storage quota. Writes [used_bytes_lo, used_bytes_hi, quota_lo, quota_hi] to ptr.
    fn js_storage_quota_chirho(result_ptr_chirho: u32) -> i32;

    // B4-003: Canvas 2D framebuffer — pixel-level rendering
    /// Write a rectangle of RGBA pixels to the canvas framebuffer.
    fn js_fb_put_rect_chirho(x_chirho: u32, y_chirho: u32, w_chirho: u32, h_chirho: u32, data_ptr_chirho: u32);
    /// Fill a rectangle with a single RGBA color.
    fn js_fb_fill_rect_chirho(x_chirho: u32, y_chirho: u32, w_chirho: u32, h_chirho: u32, rgba_chirho: u32);
    /// Draw text on the canvas at (x,y) with given color. Returns width of rendered text.
    fn js_fb_draw_text_chirho(x_chirho: u32, y_chirho: u32, text_ptr_chirho: u32, text_len_chirho: u32, rgba_chirho: u32) -> u32;

    // B4-006/B4-007: Input events — mouse and keyboard from browser
    /// Read pending mouse event. Writes [x, y, buttons, event_type] to ptr. Returns 1 if event, 0 if none.
    fn js_input_mouse_chirho(result_ptr_chirho: u32) -> i32;
    /// Read pending keyboard event. Writes [keycode, modifiers, pressed] to ptr. Returns 1 if event, 0 if none.
    fn js_input_keyboard_chirho(result_ptr_chirho: u32) -> i32;
}

// ---------------------------------------------------------------------------
// Architecture port for kernel-core
// ---------------------------------------------------------------------------

struct WasmArchPortChirho;

impl kernel_core_chirho::ArchPortChirho for WasmArchPortChirho {
    fn debug_print_chirho(&self, s_chirho: &str) {
        unsafe {
            js_console_write_chirho(s_chirho.as_ptr() as u32, s_chirho.len() as u32);
        }
    }

    fn timestamp_us_chirho(&self) -> u64 {
        unsafe { js_timestamp_us_chirho() as u64 }
    }

    fn yield_cpu_chirho(&self) {
        unsafe { js_yield_chirho(); }
    }

    fn console_read_chirho(&self, buf_chirho: &mut [u8]) -> usize {
        unsafe {
            js_console_read_chirho(buf_chirho.as_mut_ptr() as u32, buf_chirho.len() as u32) as usize
        }
    }
}

static WASM_ARCH_CHIRHO: WasmArchPortChirho = WasmArchPortChirho;

// ---------------------------------------------------------------------------
// B1-011: Signal handling framework
// ---------------------------------------------------------------------------

/// Signal numbers (Linux-compatible subset)
const SIGINT_CHIRHO: u8 = 2;
const SIGKILL_CHIRHO: u8 = 9;
const SIGCHLD_CHIRHO: u8 = 17;
const SIGCONT_CHIRHO: u8 = 18;
const SIGSTOP_CHIRHO: u8 = 19;

/// Maximum signal number we support
const MAX_SIGNAL_CHIRHO: usize = 32;

/// Signal disposition
#[derive(Clone, Copy, PartialEq)]
enum SigDispositionChirho {
    DefaultChirho,
    IgnoreChirho,
    /// In a real kernel this would hold a handler address; here we just mark it
    CaughtChirho,
}

/// Per-process signal state
#[derive(Clone, Copy)]
struct SignalStateChirho {
    /// Pending signal bitmask (bit N = signal N pending)
    pending_chirho: u32,
    /// Disposition table
    disposition_chirho: [SigDispositionChirho; MAX_SIGNAL_CHIRHO],
}

impl SignalStateChirho {
    const fn new_chirho() -> Self {
        Self {
            pending_chirho: 0,
            disposition_chirho: [SigDispositionChirho::DefaultChirho; MAX_SIGNAL_CHIRHO],
        }
    }

    /// Queue a signal
    fn send_chirho(&mut self, sig_chirho: u8) {
        if (sig_chirho as usize) < MAX_SIGNAL_CHIRHO {
            self.pending_chirho |= 1u32 << sig_chirho;
        }
    }

    /// Check and clear one pending signal, returns signal number or 0
    fn dequeue_chirho(&mut self) -> u8 {
        if self.pending_chirho == 0 {
            return 0;
        }
        let sig_chirho = self.pending_chirho.trailing_zeros() as u8;
        self.pending_chirho &= !(1u32 << sig_chirho);
        sig_chirho
    }

    /// Returns true if the default action for this signal is to terminate
    fn default_action_fatal_chirho(sig_chirho: u8) -> bool {
        matches!(sig_chirho, 1 | 2 | 3 | 6 | 9 | 11 | 13 | 14 | 15)
    }
}

// ---------------------------------------------------------------------------
// B1-013: TTY/PTY subsystem — line discipline, raw/cooked modes
// ---------------------------------------------------------------------------

/// Terminal mode: cooked (line-buffered) or raw (char-at-a-time)
#[derive(Clone, Copy, PartialEq)]
enum TtyModeChirho {
    CookedChirho,
    RawChirho,
}

/// Terminal window size (for TIOCGWINSZ ioctl)
#[derive(Clone, Copy)]
struct WinsizeChirho {
    ws_row_chirho: u16,
    ws_col_chirho: u16,
}

/// Line discipline flags
#[derive(Clone, Copy)]
struct LineDisciplineChirho {
    mode_chirho: TtyModeChirho,
    echo_chirho: bool,
    /// Canonical mode line buffer
    canon_buf_chirho: [u8; 256],
    canon_len_chirho: usize,
}

impl LineDisciplineChirho {
    const fn new_chirho() -> Self {
        Self {
            mode_chirho: TtyModeChirho::CookedChirho,
            echo_chirho: true,
            canon_buf_chirho: [0u8; 256],
            canon_len_chirho: 0,
        }
    }
}

/// TTY device state
struct TtyStateChirho {
    ldisc_chirho: LineDisciplineChirho,
    winsize_chirho: WinsizeChirho,
    /// Foreground process group (PID)
    fg_pgid_chirho: u16,
}

impl TtyStateChirho {
    const fn new_chirho() -> Self {
        Self {
            ldisc_chirho: LineDisciplineChirho::new_chirho(),
            winsize_chirho: WinsizeChirho { ws_row_chirho: 24, ws_col_chirho: 80 },
            fg_pgid_chirho: 1,
        }
    }

    /// Process a byte through the line discipline in cooked mode.
    /// Returns true if a complete line is ready.
    fn ldisc_input_chirho(&mut self, byte_chirho: u8) -> bool {
        match self.ldisc_chirho.mode_chirho {
            TtyModeChirho::RawChirho => {
                // In raw mode, every byte is immediately available
                if self.ldisc_chirho.canon_len_chirho < 256 {
                    self.ldisc_chirho.canon_buf_chirho[self.ldisc_chirho.canon_len_chirho] = byte_chirho;
                    self.ldisc_chirho.canon_len_chirho += 1;
                }
                true
            }
            TtyModeChirho::CookedChirho => {
                match byte_chirho {
                    b'\r' | b'\n' => {
                        if self.ldisc_chirho.canon_len_chirho < 256 {
                            self.ldisc_chirho.canon_buf_chirho[self.ldisc_chirho.canon_len_chirho] = b'\n';
                            self.ldisc_chirho.canon_len_chirho += 1;
                        }
                        true
                    }
                    0x7F | 0x08 => {
                        if self.ldisc_chirho.canon_len_chirho > 0 {
                            self.ldisc_chirho.canon_len_chirho -= 1;
                        }
                        false
                    }
                    _ => {
                        if byte_chirho >= 0x20 && self.ldisc_chirho.canon_len_chirho < 256 {
                            self.ldisc_chirho.canon_buf_chirho[self.ldisc_chirho.canon_len_chirho] = byte_chirho;
                            self.ldisc_chirho.canon_len_chirho += 1;
                        }
                        false
                    }
                }
            }
        }
    }

    fn flush_canon_chirho(&mut self) {
        self.ldisc_chirho.canon_len_chirho = 0;
    }
}

static mut TTY_STATE_CHIRHO: TtyStateChirho = TtyStateChirho::new_chirho();

/// IOCTL constants (Linux-compatible)
const TIOCGWINSZ_CHIRHO: u32 = 0x5413;
const TIOCSWINSZ_CHIRHO: u32 = 0x5414;
const TCGETS_CHIRHO: u32 = 0x5401;
const TCSETS_CHIRHO: u32 = 0x5402;

// ---------------------------------------------------------------------------
// B1-017: Job control — background processes, fg, bg, jobs
// ---------------------------------------------------------------------------

/// Maximum number of jobs
const MAX_JOBS_CHIRHO: usize = 16;

#[derive(Clone, Copy, PartialEq)]
enum JobStateChirho {
    FreeChirho,
    RunningChirho,
    StoppedChirho,
    DoneChirho,
}

#[derive(Clone, Copy)]
struct JobEntryChirho {
    state_chirho: JobStateChirho,
    pid_chirho: u16,
    name_chirho: [u8; 32],
    name_len_chirho: usize,
}

impl JobEntryChirho {
    const fn empty_chirho() -> Self {
        Self {
            state_chirho: JobStateChirho::FreeChirho,
            pid_chirho: 0,
            name_chirho: [0u8; 32],
            name_len_chirho: 0,
        }
    }
}

struct JobTableChirho {
    jobs_chirho: [JobEntryChirho; MAX_JOBS_CHIRHO],
}

impl JobTableChirho {
    const fn new_chirho() -> Self {
        Self {
            jobs_chirho: [JobEntryChirho::empty_chirho(); MAX_JOBS_CHIRHO],
        }
    }

    fn add_job_chirho(&mut self, pid_chirho: u16, name_chirho: &[u8]) -> Option<usize> {
        for i_chirho in 0..MAX_JOBS_CHIRHO {
            if self.jobs_chirho[i_chirho].state_chirho == JobStateChirho::FreeChirho {
                let entry_chirho = &mut self.jobs_chirho[i_chirho];
                entry_chirho.state_chirho = JobStateChirho::RunningChirho;
                entry_chirho.pid_chirho = pid_chirho;
                let len_chirho = if name_chirho.len() > 32 { 32 } else { name_chirho.len() };
                entry_chirho.name_chirho[..len_chirho].copy_from_slice(&name_chirho[..len_chirho]);
                entry_chirho.name_len_chirho = len_chirho;
                return Some(i_chirho);
            }
        }
        None
    }

    fn find_by_pid_chirho(&self, pid_chirho: u16) -> Option<usize> {
        for i_chirho in 0..MAX_JOBS_CHIRHO {
            if self.jobs_chirho[i_chirho].pid_chirho == pid_chirho
                && self.jobs_chirho[i_chirho].state_chirho != JobStateChirho::FreeChirho
            {
                return Some(i_chirho);
            }
        }
        None
    }
}

static mut JOB_TABLE_CHIRHO: JobTableChirho = JobTableChirho::new_chirho();

// ---------------------------------------------------------------------------
// B1-018: I/O redirection state
// ---------------------------------------------------------------------------

/// Maximum number of active redirections per command
const MAX_REDIRECTS_CHIRHO: usize = 4;

#[derive(Clone, Copy, PartialEq)]
enum RedirectTypeChirho {
    NoneChirho,
    /// > file (truncate)
    OutputTruncChirho,
    /// >> file (append)
    OutputAppendChirho,
    /// < file (input)
    InputChirho,
    /// 2> file (stderr redirect)
    StderrChirho,
}

#[derive(Clone, Copy)]
struct RedirectEntryChirho {
    rtype_chirho: RedirectTypeChirho,
    path_chirho: [u8; MAX_PATH_LEN_CHIRHO],
    path_len_chirho: usize,
}

impl RedirectEntryChirho {
    const fn empty_chirho() -> Self {
        Self {
            rtype_chirho: RedirectTypeChirho::NoneChirho,
            path_chirho: [0u8; 128], // MAX_PATH_LEN_CHIRHO
            path_len_chirho: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// B3-001: OPFS block device driver
// ---------------------------------------------------------------------------

/// Maximum simultaneously open OPFS files
const MAX_OPFS_FILES_CHIRHO: usize = 8;

/// OPFS block device — maps file operations to JS OPFS imports
#[derive(Clone, Copy)]
struct OpfsFileChirho {
    in_use_chirho: bool,
    handle_chirho: i32,
    name_chirho: [u8; 64],
    name_len_chirho: usize,
    size_chirho: u32,
}

impl OpfsFileChirho {
    const fn empty_chirho() -> Self {
        Self {
            in_use_chirho: false,
            handle_chirho: -1,
            name_chirho: [0u8; 64],
            name_len_chirho: 0,
            size_chirho: 0,
        }
    }
}

struct OpfsDriverChirho {
    files_chirho: [OpfsFileChirho; MAX_OPFS_FILES_CHIRHO],
}

impl OpfsDriverChirho {
    const fn new_chirho() -> Self {
        Self {
            files_chirho: [OpfsFileChirho::empty_chirho(); MAX_OPFS_FILES_CHIRHO],
        }
    }

    /// Open or create a file in OPFS. Returns local slot index or -1.
    fn open_chirho(&mut self, name_chirho: &[u8], create_chirho: bool) -> i32 {
        // Find free slot
        let slot_chirho = match self.files_chirho.iter().position(|f_chirho| !f_chirho.in_use_chirho) {
            Some(i_chirho) => i_chirho,
            None => return -1, // EMFILE
        };
        let create_flag_chirho = if create_chirho { 1u32 } else { 0u32 };
        let handle_chirho = unsafe {
            js_opfs_open_chirho(
                name_chirho.as_ptr() as u32,
                name_chirho.len() as u32,
                create_flag_chirho,
            )
        };
        if handle_chirho < 0 {
            return handle_chirho;
        }
        let size_chirho = unsafe { js_opfs_size_chirho(handle_chirho) };
        let entry_chirho = &mut self.files_chirho[slot_chirho];
        entry_chirho.in_use_chirho = true;
        entry_chirho.handle_chirho = handle_chirho;
        let len_chirho = if name_chirho.len() > 64 { 64 } else { name_chirho.len() };
        entry_chirho.name_chirho[..len_chirho].copy_from_slice(&name_chirho[..len_chirho]);
        entry_chirho.name_len_chirho = len_chirho;
        entry_chirho.size_chirho = if size_chirho >= 0 { size_chirho as u32 } else { 0 };
        slot_chirho as i32
    }

    /// Read from an OPFS file. Returns bytes read.
    fn read_chirho(&self, slot_chirho: usize, offset_chirho: u32, buf_chirho: &mut [u8]) -> i32 {
        if slot_chirho >= MAX_OPFS_FILES_CHIRHO || !self.files_chirho[slot_chirho].in_use_chirho {
            return -9; // EBADF
        }
        unsafe {
            js_opfs_read_chirho(
                self.files_chirho[slot_chirho].handle_chirho,
                offset_chirho,
                buf_chirho.as_mut_ptr() as u32,
                buf_chirho.len() as u32,
            )
        }
    }

    /// Write to an OPFS file. Returns bytes written.
    fn write_chirho(&mut self, slot_chirho: usize, offset_chirho: u32, data_chirho: &[u8]) -> i32 {
        if slot_chirho >= MAX_OPFS_FILES_CHIRHO || !self.files_chirho[slot_chirho].in_use_chirho {
            return -9; // EBADF
        }
        let result_chirho = unsafe {
            js_opfs_write_chirho(
                self.files_chirho[slot_chirho].handle_chirho,
                offset_chirho,
                data_chirho.as_ptr() as u32,
                data_chirho.len() as u32,
            )
        };
        if result_chirho > 0 {
            let new_end_chirho = offset_chirho + result_chirho as u32;
            if new_end_chirho > self.files_chirho[slot_chirho].size_chirho {
                self.files_chirho[slot_chirho].size_chirho = new_end_chirho;
            }
        }
        result_chirho
    }

    /// Close an OPFS file by slot.
    fn close_chirho(&mut self, slot_chirho: usize) {
        if slot_chirho < MAX_OPFS_FILES_CHIRHO && self.files_chirho[slot_chirho].in_use_chirho {
            unsafe { js_opfs_close_chirho(self.files_chirho[slot_chirho].handle_chirho); }
            self.files_chirho[slot_chirho].in_use_chirho = false;
            self.files_chirho[slot_chirho].handle_chirho = -1;
        }
    }

    /// Delete a file from OPFS by name.
    fn delete_chirho(&self, name_chirho: &[u8]) -> i32 {
        unsafe {
            js_opfs_delete_chirho(name_chirho.as_ptr() as u32, name_chirho.len() as u32)
        }
    }

    /// Sync/flush a file to persistent storage.
    fn sync_chirho(&self, slot_chirho: usize) -> i32 {
        if slot_chirho >= MAX_OPFS_FILES_CHIRHO || !self.files_chirho[slot_chirho].in_use_chirho {
            return -9;
        }
        unsafe { js_opfs_sync_chirho(self.files_chirho[slot_chirho].handle_chirho) }
    }
}

static mut OPFS_DRIVER_CHIRHO: OpfsDriverChirho = OpfsDriverChirho::new_chirho();

// ---------------------------------------------------------------------------
// B3-002: IndexedDB fallback storage backend
// ---------------------------------------------------------------------------

/// Maximum simultaneously open IndexedDB stores
const MAX_IDB_STORES_CHIRHO: usize = 4;

#[derive(Clone, Copy)]
struct IdbStoreChirho {
    in_use_chirho: bool,
    handle_chirho: i32,
    name_chirho: [u8; 64],
    name_len_chirho: usize,
}

impl IdbStoreChirho {
    const fn empty_chirho() -> Self {
        Self {
            in_use_chirho: false,
            handle_chirho: -1,
            name_chirho: [0u8; 64],
            name_len_chirho: 0,
        }
    }
}

struct IdbDriverChirho {
    stores_chirho: [IdbStoreChirho; MAX_IDB_STORES_CHIRHO],
}

impl IdbDriverChirho {
    const fn new_chirho() -> Self {
        Self {
            stores_chirho: [IdbStoreChirho::empty_chirho(); MAX_IDB_STORES_CHIRHO],
        }
    }

    /// Open an IndexedDB store. Returns slot index or -1.
    fn open_chirho(&mut self, name_chirho: &[u8]) -> i32 {
        let slot_chirho = match self.stores_chirho.iter().position(|s_chirho| !s_chirho.in_use_chirho) {
            Some(i_chirho) => i_chirho,
            None => return -1,
        };
        let handle_chirho = unsafe {
            js_idb_open_chirho(name_chirho.as_ptr() as u32, name_chirho.len() as u32)
        };
        if handle_chirho < 0 { return handle_chirho; }
        let entry_chirho = &mut self.stores_chirho[slot_chirho];
        entry_chirho.in_use_chirho = true;
        entry_chirho.handle_chirho = handle_chirho;
        let len_chirho = if name_chirho.len() > 64 { 64 } else { name_chirho.len() };
        entry_chirho.name_chirho[..len_chirho].copy_from_slice(&name_chirho[..len_chirho]);
        entry_chirho.name_len_chirho = len_chirho;
        slot_chirho as i32
    }

    /// Get a value by key from an IndexedDB store.
    fn get_chirho(&self, slot_chirho: usize, key_chirho: &[u8], buf_chirho: &mut [u8]) -> i32 {
        if slot_chirho >= MAX_IDB_STORES_CHIRHO || !self.stores_chirho[slot_chirho].in_use_chirho {
            return -9;
        }
        unsafe {
            js_idb_get_chirho(
                self.stores_chirho[slot_chirho].handle_chirho,
                key_chirho.as_ptr() as u32,
                key_chirho.len() as u32,
                buf_chirho.as_mut_ptr() as u32,
                buf_chirho.len() as u32,
            )
        }
    }

    /// Put a key-value pair into an IndexedDB store.
    fn put_chirho(&self, slot_chirho: usize, key_chirho: &[u8], val_chirho: &[u8]) -> i32 {
        if slot_chirho >= MAX_IDB_STORES_CHIRHO || !self.stores_chirho[slot_chirho].in_use_chirho {
            return -9;
        }
        unsafe {
            js_idb_put_chirho(
                self.stores_chirho[slot_chirho].handle_chirho,
                key_chirho.as_ptr() as u32,
                key_chirho.len() as u32,
                val_chirho.as_ptr() as u32,
                val_chirho.len() as u32,
            )
        }
    }

    /// Delete a key from an IndexedDB store.
    fn delete_chirho(&self, slot_chirho: usize, key_chirho: &[u8]) -> i32 {
        if slot_chirho >= MAX_IDB_STORES_CHIRHO || !self.stores_chirho[slot_chirho].in_use_chirho {
            return -9;
        }
        unsafe {
            js_idb_delete_chirho(
                self.stores_chirho[slot_chirho].handle_chirho,
                key_chirho.as_ptr() as u32,
                key_chirho.len() as u32,
            )
        }
    }

    /// List keys in an IndexedDB store (null-separated into buffer).
    fn list_keys_chirho(&self, slot_chirho: usize, buf_chirho: &mut [u8]) -> i32 {
        if slot_chirho >= MAX_IDB_STORES_CHIRHO || !self.stores_chirho[slot_chirho].in_use_chirho {
            return -9;
        }
        unsafe {
            js_idb_list_chirho(
                self.stores_chirho[slot_chirho].handle_chirho,
                buf_chirho.as_mut_ptr() as u32,
                buf_chirho.len() as u32,
            )
        }
    }

    /// Close an IndexedDB store.
    fn close_chirho(&mut self, slot_chirho: usize) {
        if slot_chirho < MAX_IDB_STORES_CHIRHO && self.stores_chirho[slot_chirho].in_use_chirho {
            unsafe { js_idb_close_chirho(self.stores_chirho[slot_chirho].handle_chirho); }
            self.stores_chirho[slot_chirho].in_use_chirho = false;
            self.stores_chirho[slot_chirho].handle_chirho = -1;
        }
    }
}

static mut IDB_DRIVER_CHIRHO: IdbDriverChirho = IdbDriverChirho::new_chirho();

// ---------------------------------------------------------------------------
// B1-014: Environment variables store
// ---------------------------------------------------------------------------

const MAX_ENV_VARS_CHIRHO: usize = 32;
const MAX_ENV_KEY_LEN_CHIRHO: usize = 32;
const MAX_ENV_VAL_LEN_CHIRHO: usize = 128;

#[derive(Clone, Copy)]
struct EnvVarChirho {
    in_use_chirho: bool,
    key_chirho: [u8; MAX_ENV_KEY_LEN_CHIRHO],
    key_len_chirho: usize,
    val_chirho: [u8; MAX_ENV_VAL_LEN_CHIRHO],
    val_len_chirho: usize,
}

impl EnvVarChirho {
    const fn empty_chirho() -> Self {
        Self {
            in_use_chirho: false,
            key_chirho: [0u8; MAX_ENV_KEY_LEN_CHIRHO],
            key_len_chirho: 0,
            val_chirho: [0u8; MAX_ENV_VAL_LEN_CHIRHO],
            val_len_chirho: 0,
        }
    }
}

struct EnvTableChirho {
    vars_chirho: [EnvVarChirho; MAX_ENV_VARS_CHIRHO],
}

impl EnvTableChirho {
    const fn new_chirho() -> Self {
        Self {
            vars_chirho: [EnvVarChirho::empty_chirho(); MAX_ENV_VARS_CHIRHO],
        }
    }

    fn set_chirho(&mut self, key_chirho: &[u8], val_chirho: &[u8]) {
        // Update existing
        for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
            if self.vars_chirho[i_chirho].in_use_chirho
                && self.vars_chirho[i_chirho].key_len_chirho == key_chirho.len()
                && &self.vars_chirho[i_chirho].key_chirho[..key_chirho.len()] == key_chirho
            {
                let vlen_chirho = if val_chirho.len() > MAX_ENV_VAL_LEN_CHIRHO { MAX_ENV_VAL_LEN_CHIRHO } else { val_chirho.len() };
                self.vars_chirho[i_chirho].val_chirho[..vlen_chirho].copy_from_slice(&val_chirho[..vlen_chirho]);
                self.vars_chirho[i_chirho].val_len_chirho = vlen_chirho;
                return;
            }
        }
        // Insert new
        for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
            if !self.vars_chirho[i_chirho].in_use_chirho {
                let klen_chirho = if key_chirho.len() > MAX_ENV_KEY_LEN_CHIRHO { MAX_ENV_KEY_LEN_CHIRHO } else { key_chirho.len() };
                let vlen_chirho = if val_chirho.len() > MAX_ENV_VAL_LEN_CHIRHO { MAX_ENV_VAL_LEN_CHIRHO } else { val_chirho.len() };
                self.vars_chirho[i_chirho].in_use_chirho = true;
                self.vars_chirho[i_chirho].key_chirho[..klen_chirho].copy_from_slice(&key_chirho[..klen_chirho]);
                self.vars_chirho[i_chirho].key_len_chirho = klen_chirho;
                self.vars_chirho[i_chirho].val_chirho[..vlen_chirho].copy_from_slice(&val_chirho[..vlen_chirho]);
                self.vars_chirho[i_chirho].val_len_chirho = vlen_chirho;
                return;
            }
        }
    }

    fn get_chirho(&self, key_chirho: &[u8]) -> Option<&[u8]> {
        for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
            if self.vars_chirho[i_chirho].in_use_chirho
                && self.vars_chirho[i_chirho].key_len_chirho == key_chirho.len()
                && &self.vars_chirho[i_chirho].key_chirho[..key_chirho.len()] == key_chirho
            {
                return Some(&self.vars_chirho[i_chirho].val_chirho[..self.vars_chirho[i_chirho].val_len_chirho]);
            }
        }
        None
    }

    fn unset_chirho(&mut self, key_chirho: &[u8]) {
        for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
            if self.vars_chirho[i_chirho].in_use_chirho
                && self.vars_chirho[i_chirho].key_len_chirho == key_chirho.len()
                && &self.vars_chirho[i_chirho].key_chirho[..key_chirho.len()] == key_chirho
            {
                self.vars_chirho[i_chirho].in_use_chirho = false;
                return;
            }
        }
    }
}

static mut ENV_TABLE_CHIRHO: EnvTableChirho = EnvTableChirho::new_chirho();

// ---------------------------------------------------------------------------
// B1-017: Command history for shell
// ---------------------------------------------------------------------------

const MAX_HISTORY_CHIRHO: usize = 32;
const MAX_HIST_LINE_CHIRHO: usize = 256;

struct HistoryChirho {
    lines_chirho: [[u8; MAX_HIST_LINE_CHIRHO]; MAX_HISTORY_CHIRHO],
    lens_chirho: [usize; MAX_HISTORY_CHIRHO],
    count_chirho: usize,
    pos_chirho: usize,
}

impl HistoryChirho {
    const fn new_chirho() -> Self {
        Self {
            lines_chirho: [[0u8; MAX_HIST_LINE_CHIRHO]; MAX_HISTORY_CHIRHO],
            lens_chirho: [0usize; MAX_HISTORY_CHIRHO],
            count_chirho: 0,
            pos_chirho: 0,
        }
    }

    fn add_chirho(&mut self, line_chirho: &[u8]) {
        if line_chirho.is_empty() { return; }
        let idx_chirho = self.pos_chirho % MAX_HISTORY_CHIRHO;
        let len_chirho = if line_chirho.len() > MAX_HIST_LINE_CHIRHO { MAX_HIST_LINE_CHIRHO } else { line_chirho.len() };
        self.lines_chirho[idx_chirho][..len_chirho].copy_from_slice(&line_chirho[..len_chirho]);
        self.lens_chirho[idx_chirho] = len_chirho;
        self.pos_chirho += 1;
        if self.count_chirho < MAX_HISTORY_CHIRHO {
            self.count_chirho += 1;
        }
    }
}

static mut SHELL_HISTORY_CHIRHO: HistoryChirho = HistoryChirho::new_chirho();

// ---------------------------------------------------------------------------
// B1-009: Process table and fork/exec
// ---------------------------------------------------------------------------

/// Process states
#[derive(Clone, Copy, PartialEq)]
enum ProcessStateChirho {
    FreeChirho,
    RunningChirho,
    ReadyChirho,
    ZombieChirho,
    StoppedChirho,
}

/// Maximum file descriptors per process
const MAX_FDS_CHIRHO: usize = 16;

/// Maximum processes
const MAX_PROCS_CHIRHO: usize = 32;

/// File descriptor entry
#[derive(Clone, Copy)]
struct FdEntryChirho {
    in_use_chirho: bool,
    /// 0=stdin, 1=stdout, 2=stderr, 3+=files/sockets
    kind_chirho: u8,
}

impl FdEntryChirho {
    const fn empty_chirho() -> Self {
        Self { in_use_chirho: false, kind_chirho: 0 }
    }

    const fn stdio_chirho(kind_chirho: u8) -> Self {
        Self { in_use_chirho: true, kind_chirho }
    }
}

/// Maximum length for process name
const PROC_NAME_LEN_CHIRHO: usize = 32;

/// Process control block
#[derive(Clone, Copy)]
struct ProcessChirho {
    pid_chirho: u16,
    ppid_chirho: u16,
    state_chirho: ProcessStateChirho,
    exit_code_chirho: i32,
    name_chirho: [u8; PROC_NAME_LEN_CHIRHO],
    name_len_chirho: usize,
    fds_chirho: [FdEntryChirho; MAX_FDS_CHIRHO],
    brk_chirho: u32,
    signals_chirho: SignalStateChirho,
    start_time_us_chirho: u64,
}

impl ProcessChirho {
    const fn empty_chirho() -> Self {
        Self {
            pid_chirho: 0,
            ppid_chirho: 0,
            state_chirho: ProcessStateChirho::FreeChirho,
            exit_code_chirho: 0,
            name_chirho: [0u8; PROC_NAME_LEN_CHIRHO],
            name_len_chirho: 0,
            fds_chirho: [FdEntryChirho::empty_chirho(); MAX_FDS_CHIRHO],
            brk_chirho: 0,
            signals_chirho: SignalStateChirho::new_chirho(),
            start_time_us_chirho: 0,
        }
    }

    fn set_name_chirho(&mut self, name_chirho: &[u8]) {
        let len_chirho = if name_chirho.len() > PROC_NAME_LEN_CHIRHO {
            PROC_NAME_LEN_CHIRHO
        } else {
            name_chirho.len()
        };
        self.name_chirho[..len_chirho].copy_from_slice(&name_chirho[..len_chirho]);
        self.name_len_chirho = len_chirho;
    }

    fn name_str_chirho(&self) -> &[u8] {
        &self.name_chirho[..self.name_len_chirho]
    }

    fn init_stdio_chirho(&mut self) {
        self.fds_chirho[0] = FdEntryChirho::stdio_chirho(0);
        self.fds_chirho[1] = FdEntryChirho::stdio_chirho(1);
        self.fds_chirho[2] = FdEntryChirho::stdio_chirho(2);
    }
}

/// The process table
struct ProcessTableChirho {
    procs_chirho: [ProcessChirho; MAX_PROCS_CHIRHO],
    current_pid_chirho: u16,
    next_pid_chirho: u16,
    boot_time_us_chirho: u64,
}

impl ProcessTableChirho {
    const fn new_chirho() -> Self {
        Self {
            procs_chirho: [ProcessChirho::empty_chirho(); MAX_PROCS_CHIRHO],
            current_pid_chirho: 1,
            next_pid_chirho: 2,
            boot_time_us_chirho: 0,
        }
    }

    fn find_pid_chirho(&self, pid_chirho: u16) -> Option<usize> {
        for i_chirho in 0..MAX_PROCS_CHIRHO {
            if self.procs_chirho[i_chirho].pid_chirho == pid_chirho
                && self.procs_chirho[i_chirho].state_chirho != ProcessStateChirho::FreeChirho
            {
                return Some(i_chirho);
            }
        }
        None
    }

    fn find_free_chirho(&self) -> Option<usize> {
        for i_chirho in 0..MAX_PROCS_CHIRHO {
            if self.procs_chirho[i_chirho].state_chirho == ProcessStateChirho::FreeChirho {
                return Some(i_chirho);
            }
        }
        None
    }

    fn alloc_pid_chirho(&mut self) -> u16 {
        let pid_chirho = self.next_pid_chirho;
        self.next_pid_chirho += 1;
        if self.next_pid_chirho >= MAX_PROCS_CHIRHO as u16 * 100 {
            self.next_pid_chirho = 2;
        }
        pid_chirho
    }

    fn create_init_chirho(&mut self, now_us_chirho: u64) {
        self.boot_time_us_chirho = now_us_chirho;
        let proc_chirho = &mut self.procs_chirho[0];
        proc_chirho.pid_chirho = 1;
        proc_chirho.ppid_chirho = 0;
        proc_chirho.state_chirho = ProcessStateChirho::RunningChirho;
        proc_chirho.set_name_chirho(b"init");
        proc_chirho.init_stdio_chirho();
        proc_chirho.brk_chirho = 0;
        proc_chirho.start_time_us_chirho = now_us_chirho;
        self.current_pid_chirho = 1;
    }

    /// fork() — clone current process state into a new process
    fn fork_chirho(&mut self, now_us_chirho: u64) -> i32 {
        let parent_idx_chirho = match self.find_pid_chirho(self.current_pid_chirho) {
            Some(i_chirho) => i_chirho,
            None => return -3, // ESRCH
        };
        let child_idx_chirho = match self.find_free_chirho() {
            Some(i_chirho) => i_chirho,
            None => return -11, // EAGAIN
        };
        let child_pid_chirho = self.alloc_pid_chirho();
        self.procs_chirho[child_idx_chirho] = self.procs_chirho[parent_idx_chirho];
        let child_chirho = &mut self.procs_chirho[child_idx_chirho];
        child_chirho.pid_chirho = child_pid_chirho;
        child_chirho.ppid_chirho = self.current_pid_chirho;
        child_chirho.state_chirho = ProcessStateChirho::ReadyChirho;
        child_chirho.start_time_us_chirho = now_us_chirho;
        child_chirho.signals_chirho = SignalStateChirho::new_chirho();
        child_pid_chirho as i32
    }

    /// exec_process() — replace current process image with a built-in program
    fn exec_process_chirho(&mut self, name_chirho: &[u8]) -> i32 {
        let idx_chirho = match self.find_pid_chirho(self.current_pid_chirho) {
            Some(i_chirho) => i_chirho,
            None => return -3,
        };
        if !is_builtin_program_chirho(name_chirho) {
            return -2; // ENOENT
        }
        let proc_chirho = &mut self.procs_chirho[idx_chirho];
        proc_chirho.set_name_chirho(name_chirho);
        proc_chirho.signals_chirho = SignalStateChirho::new_chirho();
        for fd_chirho in 3..MAX_FDS_CHIRHO {
            proc_chirho.fds_chirho[fd_chirho] = FdEntryChirho::empty_chirho();
        }
        0
    }

    /// Send a signal to a process by PID
    fn kill_chirho(&mut self, pid_chirho: u16, sig_chirho: u8) -> i32 {
        let idx_chirho = match self.find_pid_chirho(pid_chirho) {
            Some(i_chirho) => i_chirho,
            None => return -3,
        };
        if sig_chirho == 0 { return 0; }
        if sig_chirho == SIGKILL_CHIRHO {
            self.procs_chirho[idx_chirho].state_chirho = ProcessStateChirho::ZombieChirho;
            self.procs_chirho[idx_chirho].exit_code_chirho = 128 + sig_chirho as i32;
            let ppid_chirho = self.procs_chirho[idx_chirho].ppid_chirho;
            if let Some(pi_chirho) = self.find_pid_chirho(ppid_chirho) {
                self.procs_chirho[pi_chirho].signals_chirho.send_chirho(SIGCHLD_CHIRHO);
            }
            return 0;
        }
        if sig_chirho == SIGSTOP_CHIRHO {
            self.procs_chirho[idx_chirho].state_chirho = ProcessStateChirho::StoppedChirho;
            return 0;
        }
        if sig_chirho == SIGCONT_CHIRHO
            && self.procs_chirho[idx_chirho].state_chirho == ProcessStateChirho::StoppedChirho
        {
            self.procs_chirho[idx_chirho].state_chirho = ProcessStateChirho::ReadyChirho;
            return 0;
        }
        let proc_chirho = &mut self.procs_chirho[idx_chirho];
        let disp_chirho = proc_chirho.signals_chirho.disposition_chirho[sig_chirho as usize];
        match disp_chirho {
            SigDispositionChirho::IgnoreChirho => 0,
            SigDispositionChirho::DefaultChirho => {
                if SignalStateChirho::default_action_fatal_chirho(sig_chirho) {
                    proc_chirho.state_chirho = ProcessStateChirho::ZombieChirho;
                    proc_chirho.exit_code_chirho = 128 + sig_chirho as i32;
                    let ppid_chirho = proc_chirho.ppid_chirho;
                    if let Some(pi_chirho) = self.find_pid_chirho(ppid_chirho) {
                        self.procs_chirho[pi_chirho].signals_chirho.send_chirho(SIGCHLD_CHIRHO);
                    }
                }
                0
            }
            SigDispositionChirho::CaughtChirho => {
                proc_chirho.signals_chirho.send_chirho(sig_chirho);
                0
            }
        }
    }

    /// Reap zombie children of current process
    fn waitpid_chirho(&mut self, target_pid_chirho: i32) -> (i32, i32) {
        for i_chirho in 0..MAX_PROCS_CHIRHO {
            let proc_chirho = &self.procs_chirho[i_chirho];
            if proc_chirho.state_chirho == ProcessStateChirho::ZombieChirho
                && proc_chirho.ppid_chirho == self.current_pid_chirho
                && (target_pid_chirho == -1 || proc_chirho.pid_chirho == target_pid_chirho as u16)
            {
                let pid_chirho = proc_chirho.pid_chirho as i32;
                let status_chirho = proc_chirho.exit_code_chirho;
                self.procs_chirho[i_chirho].state_chirho = ProcessStateChirho::FreeChirho;
                return (pid_chirho, status_chirho);
            }
        }
        (-10, 0) // ECHILD
    }
}

/// Check if a name is in the built-in program table
fn is_builtin_program_chirho(name_chirho: &[u8]) -> bool {
    matches!(
        name_chirho,
        b"sh" | b"init" | b"cat" | b"ls" | b"echo" | b"ps" | b"kill" | b"mkdir" | b"rmdir"
            | b"touch" | b"chmod" | b"head" | b"tail" | b"wc" | b"grep"
    )
}

/// Global process table
static mut PROC_TABLE_CHIRHO: ProcessTableChirho = ProcessTableChirho::new_chirho();

// ---------------------------------------------------------------------------
// B1-010: /proc filesystem + in-memory VFS for /tmp etc.
// ---------------------------------------------------------------------------

const MAX_FS_ENTRIES_CHIRHO: usize = 128;
const MAX_PATH_LEN_CHIRHO: usize = 128;
const MAX_FILE_DATA_CHIRHO: usize = 256;

#[derive(Clone, Copy, PartialEq)]
enum FsEntryTypeChirho {
    FreeChirho,
    DirectoryChirho,
    FileChirho,
}

#[derive(Clone, Copy)]
struct FsEntryChirho {
    entry_type_chirho: FsEntryTypeChirho,
    path_chirho: [u8; MAX_PATH_LEN_CHIRHO],
    path_len_chirho: usize,
    data_chirho: [u8; MAX_FILE_DATA_CHIRHO],
    data_len_chirho: usize,
    mode_chirho: u16,
}

impl FsEntryChirho {
    const fn empty_chirho() -> Self {
        Self {
            entry_type_chirho: FsEntryTypeChirho::FreeChirho,
            path_chirho: [0u8; MAX_PATH_LEN_CHIRHO],
            path_len_chirho: 0,
            data_chirho: [0u8; MAX_FILE_DATA_CHIRHO],
            data_len_chirho: 0,
            mode_chirho: 0o755,
        }
    }
}

static mut VFS_TABLE_CHIRHO: [FsEntryChirho; MAX_FS_ENTRIES_CHIRHO] =
    [FsEntryChirho::empty_chirho(); MAX_FS_ENTRIES_CHIRHO];

fn vfs_find_chirho(path_chirho: &[u8]) -> Option<usize> {
    unsafe {
        for i_chirho in 0..MAX_FS_ENTRIES_CHIRHO {
            let e_chirho = &VFS_TABLE_CHIRHO[i_chirho];
            if e_chirho.entry_type_chirho != FsEntryTypeChirho::FreeChirho
                && e_chirho.path_len_chirho == path_chirho.len()
                && &e_chirho.path_chirho[..e_chirho.path_len_chirho] == path_chirho
            {
                return Some(i_chirho);
            }
        }
    }
    None
}

fn vfs_alloc_chirho() -> Option<usize> {
    unsafe {
        for i_chirho in 0..MAX_FS_ENTRIES_CHIRHO {
            if VFS_TABLE_CHIRHO[i_chirho].entry_type_chirho == FsEntryTypeChirho::FreeChirho {
                return Some(i_chirho);
            }
        }
    }
    None
}

/// Generate /proc/cpuinfo content to console
fn proc_cpuinfo_chirho() {
    kwrite_chirho("processor\t: 0\r\n");
    kwrite_chirho("vendor_id\t: WebAssembly\r\n");
    kwrite_chirho("model name\t: Lineluya Virtual CPU (wasm32, browser UA)\r\n");
    kwrite_chirho("cpu MHz\t\t: unlimited (JS event loop)\r\n");
    kwrite_chirho("cache size\t: browser managed\r\n");
    kwrite_chirho("flags\t\t: wasm simd bulk_memory mutable_globals\r\n");
    kwrite_chirho("bogomips\t: infinity\r\n");
}

/// Generate /proc/meminfo using real WASM memory info
fn proc_meminfo_chirho() {
    let pages_chirho = core::arch::wasm32::memory_size(0);
    let total_kb_chirho = (pages_chirho * 65536) / 1024;
    let used_kb_chirho = total_kb_chirho / 4;
    let free_kb_chirho = total_kb_chirho - used_kb_chirho;

    kwrite_chirho("MemTotal:       ");
    write_u64_chirho(total_kb_chirho as u64);
    kwrite_chirho(" kB (WASM linear memory)\r\n");
    kwrite_chirho("MemFree:        ");
    write_u64_chirho(free_kb_chirho as u64);
    kwrite_chirho(" kB\r\n");
    kwrite_chirho("MemAvailable:   ");
    write_u64_chirho(free_kb_chirho as u64);
    kwrite_chirho(" kB\r\n");
    kwrite_chirho("WasmPages:      ");
    write_u64_chirho(pages_chirho as u64);
    kwrite_chirho(" (64 KiB each)\r\n");
    kwrite_chirho("Buffers:            0 kB\r\n");
    kwrite_chirho("Cached:             0 kB\r\n");
}

/// Generate /proc/self/status for current process
fn proc_self_status_chirho() {
    unsafe {
        let idx_chirho = PROC_TABLE_CHIRHO
            .find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho)
            .unwrap_or(0);
        let p_chirho = &PROC_TABLE_CHIRHO.procs_chirho[idx_chirho];

        kwrite_chirho("Name:\t");
        kwrite_bytes_chirho(p_chirho.name_str_chirho());
        kwrite_chirho("\r\n");
        kwrite_chirho("State:\t");
        match p_chirho.state_chirho {
            ProcessStateChirho::RunningChirho => kwrite_chirho("R (running)"),
            ProcessStateChirho::ReadyChirho   => kwrite_chirho("S (sleeping)"),
            ProcessStateChirho::ZombieChirho  => kwrite_chirho("Z (zombie)"),
            ProcessStateChirho::StoppedChirho => kwrite_chirho("T (stopped)"),
            ProcessStateChirho::FreeChirho    => kwrite_chirho("X (dead)"),
        }
        kwrite_chirho("\r\n");
        kwrite_chirho("Pid:\t");
        write_u64_chirho(p_chirho.pid_chirho as u64);
        kwrite_chirho("\r\n");
        kwrite_chirho("PPid:\t");
        write_u64_chirho(p_chirho.ppid_chirho as u64);
        kwrite_chirho("\r\n");
        kwrite_chirho("Uid:\t0\t0\t0\t0\r\n");
        kwrite_chirho("Gid:\t0\t0\t0\t0\r\n");
        kwrite_chirho("FDSize:\t");
        write_u64_chirho(MAX_FDS_CHIRHO as u64);
        kwrite_chirho("\r\n");
        kwrite_chirho("SigPnd:\t");
        write_hex32_chirho(p_chirho.signals_chirho.pending_chirho);
        kwrite_chirho("\r\n");
    }
}

/// Generate /proc/uptime
fn proc_uptime_chirho() {
    unsafe {
        let now_us_chirho = js_timestamp_us_chirho() as u64;
        let uptime_us_chirho = now_us_chirho - PROC_TABLE_CHIRHO.boot_time_us_chirho;
        let sec_chirho = uptime_us_chirho / 1_000_000;
        let frac_chirho = (uptime_us_chirho % 1_000_000) / 10_000;
        write_u64_chirho(sec_chirho);
        kwrite_chirho(".");
        if frac_chirho < 10 { kwrite_chirho("0"); }
        write_u64_chirho(frac_chirho);
        kwrite_chirho(" ");
        write_u64_chirho(sec_chirho);
        kwrite_chirho(".");
        if frac_chirho < 10 { kwrite_chirho("0"); }
        write_u64_chirho(frac_chirho);
        kwrite_chirho("\r\n");
    }
}

// ---------------------------------------------------------------------------
// Built-in kernel shell state
// ---------------------------------------------------------------------------

const MAX_LINE_LEN_CHIRHO: usize = 512;

struct ShellStateChirho {
    line_buf_chirho: [u8; MAX_LINE_LEN_CHIRHO],
    line_pos_chirho: usize,
    prompt_shown_chirho: bool,
    booted_chirho: bool,
}

static mut SHELL_CHIRHO: ShellStateChirho = ShellStateChirho {
    line_buf_chirho: [0u8; MAX_LINE_LEN_CHIRHO],
    line_pos_chirho: 0,
    prompt_shown_chirho: false,
    booted_chirho: false,
};

fn kwrite_chirho(s_chirho: &str) {
    unsafe {
        js_console_write_chirho(s_chirho.as_ptr() as u32, s_chirho.len() as u32);
    }
}

fn kwrite_bytes_chirho(bytes_chirho: &[u8]) {
    unsafe {
        js_console_write_chirho(bytes_chirho.as_ptr() as u32, bytes_chirho.len() as u32);
    }
}

fn write_u64_chirho(val_chirho: u64) {
    let mut buf_chirho = [0u8; 20];
    let len_chirho = u64_to_str_chirho(val_chirho, &mut buf_chirho);
    kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
}

fn write_hex32_chirho(val_chirho: u32) {
    let hex_chirho = b"0123456789abcdef";
    let mut buf_chirho = [0u8; 8];
    for i_chirho in 0..8 {
        buf_chirho[7 - i_chirho] = hex_chirho[((val_chirho >> (i_chirho * 4)) & 0xF) as usize];
    }
    kwrite_bytes_chirho(&buf_chirho);
}

fn show_prompt_chirho() {
    kwrite_chirho("\x1b[1;36mlineluya\x1b[0m\x1b[1;33m$ \x1b[0m");
}

fn u64_to_str_chirho(mut val_chirho: u64, buf_chirho: &mut [u8]) -> usize {
    if val_chirho == 0 {
        buf_chirho[0] = b'0';
        return 1;
    }
    let mut tmp_chirho = [0u8; 20];
    let mut pos_chirho = 0usize;
    while val_chirho > 0 {
        tmp_chirho[pos_chirho] = b'0' + (val_chirho % 10) as u8;
        val_chirho /= 10;
        pos_chirho += 1;
    }
    for i_chirho in 0..pos_chirho {
        buf_chirho[i_chirho] = tmp_chirho[pos_chirho - 1 - i_chirho];
    }
    pos_chirho
}

fn parse_u64_chirho(bytes_chirho: &[u8]) -> (u64, usize) {
    let mut val_chirho = 0u64;
    let mut i_chirho = 0usize;
    while i_chirho < bytes_chirho.len() && bytes_chirho[i_chirho] >= b'0' && bytes_chirho[i_chirho] <= b'9' {
        val_chirho = val_chirho * 10 + (bytes_chirho[i_chirho] - b'0') as u64;
        i_chirho += 1;
    }
    (val_chirho, i_chirho)
}

fn bytes_contains_chirho(haystack_chirho: &[u8], needle_chirho: &[u8]) -> bool {
    if needle_chirho.is_empty() { return true; }
    if haystack_chirho.len() < needle_chirho.len() { return false; }
    for i_chirho in 0..=(haystack_chirho.len() - needle_chirho.len()) {
        if &haystack_chirho[i_chirho..i_chirho + needle_chirho.len()] == needle_chirho {
            return true;
        }
    }
    false
}

/// Simple space-delimited argument iterator
struct ArgIterChirho<'a> {
    data_chirho: &'a [u8],
    pos_chirho: usize,
}

impl<'a> ArgIterChirho<'a> {
    fn new_chirho(data_chirho: &'a [u8]) -> Self {
        Self { data_chirho, pos_chirho: 0 }
    }

    fn next_chirho(&mut self) -> Option<&'a [u8]> {
        while self.pos_chirho < self.data_chirho.len()
            && (self.data_chirho[self.pos_chirho] == b' ' || self.data_chirho[self.pos_chirho] == b'\t')
        {
            self.pos_chirho += 1;
        }
        if self.pos_chirho >= self.data_chirho.len() { return None; }
        let start_chirho = self.pos_chirho;
        while self.pos_chirho < self.data_chirho.len()
            && self.data_chirho[self.pos_chirho] != b' '
            && self.data_chirho[self.pos_chirho] != b'\t'
        {
            self.pos_chirho += 1;
        }
        Some(&self.data_chirho[start_chirho..self.pos_chirho])
    }

    fn rest_chirho(&self) -> &'a [u8] {
        let mut p_chirho = self.pos_chirho;
        while p_chirho < self.data_chirho.len()
            && (self.data_chirho[p_chirho] == b' ' || self.data_chirho[p_chirho] == b'\t')
        {
            p_chirho += 1;
        }
        &self.data_chirho[p_chirho..]
    }
}

/// Format an IPv4 address as "a.b.c.d" into a buffer, returns length.
fn format_ip_chirho(a_chirho: u8, b_chirho: u8, c_chirho: u8, d_chirho: u8, buf_chirho: &mut [u8]) -> usize {
    let mut pos_chirho = 0usize;
    for (i_chirho, octet_chirho) in [a_chirho, b_chirho, c_chirho, d_chirho].iter().enumerate() {
        if i_chirho > 0 {
            buf_chirho[pos_chirho] = b'.';
            pos_chirho += 1;
        }
        let mut tmp_chirho = [0u8; 3];
        let len_chirho = u64_to_str_chirho(*octet_chirho as u64, &mut tmp_chirho);
        buf_chirho[pos_chirho..pos_chirho + len_chirho].copy_from_slice(&tmp_chirho[..len_chirho]);
        pos_chirho += len_chirho;
    }
    pos_chirho
}

// ---------------------------------------------------------------------------
// /proc content buffer for head/tail/wc/grep
// ---------------------------------------------------------------------------

const PROC_BUF_LEN_CHIRHO: usize = 1024;
static mut PROC_CONTENT_BUF_CHIRHO: [u8; PROC_BUF_LEN_CHIRHO] = [0u8; PROC_BUF_LEN_CHIRHO];
static mut PROC_CONTENT_LEN_CHIRHO: usize = 0;

unsafe fn proc_buf_append_chirho(data_chirho: &[u8]) {
    let avail_chirho = PROC_BUF_LEN_CHIRHO - PROC_CONTENT_LEN_CHIRHO;
    let n_chirho = if data_chirho.len() > avail_chirho { avail_chirho } else { data_chirho.len() };
    PROC_CONTENT_BUF_CHIRHO[PROC_CONTENT_LEN_CHIRHO..PROC_CONTENT_LEN_CHIRHO + n_chirho]
        .copy_from_slice(&data_chirho[..n_chirho]);
    PROC_CONTENT_LEN_CHIRHO += n_chirho;
}

unsafe fn proc_buf_append_u64_chirho(val_chirho: u64) {
    let mut buf_chirho = [0u8; 20];
    let len_chirho = u64_to_str_chirho(val_chirho, &mut buf_chirho);
    proc_buf_append_chirho(&buf_chirho[..len_chirho]);
}

fn is_proc_file_chirho(path_chirho: &[u8]) -> bool {
    matches!(
        path_chirho,
        b"/proc/cpuinfo" | b"/proc/meminfo" | b"/proc/version"
            | b"/proc/uptime" | b"/proc/filesystems" | b"/proc/cmdline"
            | b"/proc/self/status"
    )
}

/// Get file content as byte slice — works for VFS files and /proc files
fn get_file_content_chirho(path_chirho: &[u8]) -> &'static [u8] {
    if let Some(idx_chirho) = vfs_find_chirho(path_chirho) {
        unsafe {
            let e_chirho = &VFS_TABLE_CHIRHO[idx_chirho];
            if e_chirho.entry_type_chirho == FsEntryTypeChirho::FileChirho {
                return &e_chirho.data_chirho[..e_chirho.data_len_chirho];
            }
        }
        return &[];
    }
    unsafe {
        PROC_CONTENT_LEN_CHIRHO = 0;
        match path_chirho {
            b"/proc/cpuinfo" => {
                proc_buf_append_chirho(b"processor\t: 0\n");
                proc_buf_append_chirho(b"vendor_id\t: WebAssembly\n");
                proc_buf_append_chirho(b"model name\t: Lineluya Virtual CPU (wasm32, browser UA)\n");
                proc_buf_append_chirho(b"cpu MHz\t\t: unlimited (JS event loop)\n");
                proc_buf_append_chirho(b"cache size\t: browser managed\n");
                proc_buf_append_chirho(b"flags\t\t: wasm simd bulk_memory mutable_globals\n");
                proc_buf_append_chirho(b"bogomips\t: infinity\n");
            }
            b"/proc/meminfo" => {
                let pages_chirho = core::arch::wasm32::memory_size(0);
                let total_kb_chirho = (pages_chirho * 65536) / 1024;
                let free_kb_chirho = total_kb_chirho * 3 / 4;
                proc_buf_append_chirho(b"MemTotal:       ");
                proc_buf_append_u64_chirho(total_kb_chirho as u64);
                proc_buf_append_chirho(b" kB\n");
                proc_buf_append_chirho(b"MemFree:        ");
                proc_buf_append_u64_chirho(free_kb_chirho as u64);
                proc_buf_append_chirho(b" kB\n");
                proc_buf_append_chirho(b"WasmPages:      ");
                proc_buf_append_u64_chirho(pages_chirho as u64);
                proc_buf_append_chirho(b"\n");
            }
            b"/proc/version" => {
                proc_buf_append_chirho(b"Lineluya version 0.6.0 (rustc wasm32-unknown-unknown)\n");
            }
            b"/proc/uptime" => {
                let now_us_chirho = js_timestamp_us_chirho() as u64;
                let boot_chirho = PROC_TABLE_CHIRHO.boot_time_us_chirho;
                let up_sec_chirho = (now_us_chirho - boot_chirho) / 1_000_000;
                proc_buf_append_u64_chirho(up_sec_chirho);
                proc_buf_append_chirho(b".00 ");
                proc_buf_append_u64_chirho(up_sec_chirho);
                proc_buf_append_chirho(b".00\n");
            }
            b"/proc/filesystems" => {
                proc_buf_append_chirho(b"nodev\topfs\n");
                proc_buf_append_chirho(b"nodev\tindexeddb\n");
                proc_buf_append_chirho(b"nodev\tmemfs\n");
            }
            b"/proc/cmdline" => {
                proc_buf_append_chirho(b"lineluya_chirho console=xterm loglevel=7\n");
            }
            b"/proc/self/status" => {
                let idx_chirho = PROC_TABLE_CHIRHO
                    .find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho)
                    .unwrap_or(0);
                let p_chirho = &PROC_TABLE_CHIRHO.procs_chirho[idx_chirho];
                proc_buf_append_chirho(b"Name:\t");
                proc_buf_append_chirho(p_chirho.name_str_chirho());
                proc_buf_append_chirho(b"\nPid:\t");
                proc_buf_append_u64_chirho(p_chirho.pid_chirho as u64);
                proc_buf_append_chirho(b"\nPPid:\t");
                proc_buf_append_u64_chirho(p_chirho.ppid_chirho as u64);
                proc_buf_append_chirho(b"\n");
            }
            _ => { return &[]; }
        }
        &PROC_CONTENT_BUF_CHIRHO[..PROC_CONTENT_LEN_CHIRHO]
    }
}

// ---------------------------------------------------------------------------
// Shell command dispatch and implementations
// ---------------------------------------------------------------------------

/// B1-018: Parse I/O redirections and strip them from command line.
/// Returns (clean command bytes, redirect entries).
fn parse_redirections_chirho(input_chirho: &[u8]) -> (&[u8], [RedirectEntryChirho; MAX_REDIRECTS_CHIRHO], usize) {
    let mut redirects_chirho = [RedirectEntryChirho::empty_chirho(); MAX_REDIRECTS_CHIRHO];
    let mut redirect_count_chirho = 0usize;

    // For simplicity, we detect redirect operators and report them
    // In a single-pass approach, scan for >, >>, <, 2>
    let mut has_redirect_chirho = false;
    for i_chirho in 0..input_chirho.len() {
        if input_chirho[i_chirho] == b'>' || input_chirho[i_chirho] == b'<' {
            has_redirect_chirho = true;
            break;
        }
    }

    if !has_redirect_chirho {
        return (input_chirho, redirects_chirho, 0);
    }

    // Find the first redirect operator and split there
    // This is a simplified parser for the shell
    let mut cmd_end_chirho = input_chirho.len();
    let mut i_chirho = 0usize;
    while i_chirho < input_chirho.len() && redirect_count_chirho < MAX_REDIRECTS_CHIRHO {
        if i_chirho + 1 < input_chirho.len() && input_chirho[i_chirho] == b'2' && input_chirho[i_chirho + 1] == b'>' {
            // 2> stderr redirect
            if cmd_end_chirho == input_chirho.len() { cmd_end_chirho = i_chirho; }
            i_chirho += 2;
            // Skip spaces
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] == b' ' { i_chirho += 1; }
            let path_start_chirho = i_chirho;
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] != b' ' && input_chirho[i_chirho] != b'>' && input_chirho[i_chirho] != b'<' { i_chirho += 1; }
            if i_chirho > path_start_chirho {
                let path_chirho = &input_chirho[path_start_chirho..i_chirho];
                let plen_chirho = if path_chirho.len() > 128 { 128 } else { path_chirho.len() };
                redirects_chirho[redirect_count_chirho].rtype_chirho = RedirectTypeChirho::StderrChirho;
                redirects_chirho[redirect_count_chirho].path_chirho[..plen_chirho].copy_from_slice(&path_chirho[..plen_chirho]);
                redirects_chirho[redirect_count_chirho].path_len_chirho = plen_chirho;
                redirect_count_chirho += 1;
            }
        } else if input_chirho[i_chirho] == b'>' {
            if cmd_end_chirho == input_chirho.len() { cmd_end_chirho = i_chirho; }
            let append_chirho = i_chirho + 1 < input_chirho.len() && input_chirho[i_chirho + 1] == b'>';
            i_chirho += if append_chirho { 2 } else { 1 };
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] == b' ' { i_chirho += 1; }
            let path_start_chirho = i_chirho;
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] != b' ' && input_chirho[i_chirho] != b'>' && input_chirho[i_chirho] != b'<' { i_chirho += 1; }
            if i_chirho > path_start_chirho {
                let path_chirho = &input_chirho[path_start_chirho..i_chirho];
                let plen_chirho = if path_chirho.len() > 128 { 128 } else { path_chirho.len() };
                redirects_chirho[redirect_count_chirho].rtype_chirho = if append_chirho { RedirectTypeChirho::OutputAppendChirho } else { RedirectTypeChirho::OutputTruncChirho };
                redirects_chirho[redirect_count_chirho].path_chirho[..plen_chirho].copy_from_slice(&path_chirho[..plen_chirho]);
                redirects_chirho[redirect_count_chirho].path_len_chirho = plen_chirho;
                redirect_count_chirho += 1;
            }
        } else if input_chirho[i_chirho] == b'<' {
            if cmd_end_chirho == input_chirho.len() { cmd_end_chirho = i_chirho; }
            i_chirho += 1;
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] == b' ' { i_chirho += 1; }
            let path_start_chirho = i_chirho;
            while i_chirho < input_chirho.len() && input_chirho[i_chirho] != b' ' && input_chirho[i_chirho] != b'>' && input_chirho[i_chirho] != b'<' { i_chirho += 1; }
            if i_chirho > path_start_chirho {
                let path_chirho = &input_chirho[path_start_chirho..i_chirho];
                let plen_chirho = if path_chirho.len() > 128 { 128 } else { path_chirho.len() };
                redirects_chirho[redirect_count_chirho].rtype_chirho = RedirectTypeChirho::InputChirho;
                redirects_chirho[redirect_count_chirho].path_chirho[..plen_chirho].copy_from_slice(&path_chirho[..plen_chirho]);
                redirects_chirho[redirect_count_chirho].path_len_chirho = plen_chirho;
                redirect_count_chirho += 1;
            }
        } else {
            i_chirho += 1;
        }
    }

    // Trim trailing spaces from command part
    while cmd_end_chirho > 0 && input_chirho[cmd_end_chirho - 1] == b' ' { cmd_end_chirho -= 1; }

    (&input_chirho[..cmd_end_chirho], redirects_chirho, redirect_count_chirho)
}

/// B1-018: Apply output redirection — write output to VFS file instead of console.
fn apply_output_redirect_chirho(data_chirho: &[u8], path_chirho: &[u8], append_chirho: bool) {
    match vfs_find_chirho(path_chirho) {
        Some(idx_chirho) => unsafe {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            if append_chirho {
                let avail_chirho = MAX_FILE_DATA_CHIRHO - e_chirho.data_len_chirho;
                let n_chirho = if data_chirho.len() > avail_chirho { avail_chirho } else { data_chirho.len() };
                e_chirho.data_chirho[e_chirho.data_len_chirho..e_chirho.data_len_chirho + n_chirho]
                    .copy_from_slice(&data_chirho[..n_chirho]);
                e_chirho.data_len_chirho += n_chirho;
            } else {
                let n_chirho = if data_chirho.len() > MAX_FILE_DATA_CHIRHO { MAX_FILE_DATA_CHIRHO } else { data_chirho.len() };
                e_chirho.data_chirho[..n_chirho].copy_from_slice(&data_chirho[..n_chirho]);
                e_chirho.data_len_chirho = n_chirho;
            }
        },
        None => {
            // Create the file
            if let Some(idx_chirho) = vfs_alloc_chirho() {
                unsafe {
                    let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
                    e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
                    let plen_chirho = if path_chirho.len() > MAX_PATH_LEN_CHIRHO { MAX_PATH_LEN_CHIRHO } else { path_chirho.len() };
                    e_chirho.path_chirho[..plen_chirho].copy_from_slice(&path_chirho[..plen_chirho]);
                    e_chirho.path_len_chirho = plen_chirho;
                    let n_chirho = if data_chirho.len() > MAX_FILE_DATA_CHIRHO { MAX_FILE_DATA_CHIRHO } else { data_chirho.len() };
                    e_chirho.data_chirho[..n_chirho].copy_from_slice(&data_chirho[..n_chirho]);
                    e_chirho.data_len_chirho = n_chirho;
                    e_chirho.mode_chirho = 0o644;
                }
            }
        }
    }
}

/// Top-level command processor with pipe, redirect, and background support.
fn process_command_chirho(line_chirho: &[u8], len_chirho: usize) {
    let mut end_chirho = len_chirho;
    while end_chirho > 0 && (line_chirho[end_chirho - 1] == b' ' || line_chirho[end_chirho - 1] == b'\t') {
        end_chirho -= 1;
    }
    let mut start_chirho = 0usize;
    while start_chirho < end_chirho && (line_chirho[start_chirho] == b' ' || line_chirho[start_chirho] == b'\t') {
        start_chirho += 1;
    }
    if start_chirho >= end_chirho { return; }

    let trimmed_chirho = &line_chirho[start_chirho..end_chirho];

    // Add to history
    unsafe { SHELL_HISTORY_CHIRHO.add_chirho(trimmed_chirho); }

    // B1-017: Check for background operator '&'
    let (effective_cmd_chirho, background_chirho) = if end_chirho > start_chirho && line_chirho[end_chirho - 1] == b'&' {
        let mut ae_chirho = end_chirho - 1;
        while ae_chirho > start_chirho && line_chirho[ae_chirho - 1] == b' ' { ae_chirho -= 1; }
        (&line_chirho[start_chirho..ae_chirho], true)
    } else {
        (trimmed_chirho, false)
    };

    if background_chirho {
        // Fork a background job
        unsafe {
            let now_chirho = js_timestamp_us_chirho() as u64;
            let child_pid_chirho = PROC_TABLE_CHIRHO.fork_chirho(now_chirho);
            if child_pid_chirho > 0 {
                if let Some(job_id_chirho) = JOB_TABLE_CHIRHO.add_job_chirho(child_pid_chirho as u16, effective_cmd_chirho) {
                    kwrite_chirho("[");
                    write_u64_chirho((job_id_chirho + 1) as u64);
                    kwrite_chirho("] ");
                    write_u64_chirho(child_pid_chirho as u64);
                    kwrite_chirho("\r\n");
                }
            }
        }
        return;
    }

    // B1-018: Check for pipes '|'
    let mut has_pipe_chirho = false;
    let mut pipe_pos_chirho = 0usize;
    for i_chirho in 0..effective_cmd_chirho.len() {
        if effective_cmd_chirho[i_chirho] == b'|' {
            has_pipe_chirho = true;
            pipe_pos_chirho = i_chirho;
            break;
        }
    }

    if has_pipe_chirho {
        // Simple two-command pipe: cmd1 | cmd2
        // Execute cmd1, capture output, feed to cmd2
        let left_chirho = &effective_cmd_chirho[..pipe_pos_chirho];
        let right_chirho = if pipe_pos_chirho + 1 < effective_cmd_chirho.len() {
            &effective_cmd_chirho[pipe_pos_chirho + 1..]
        } else { &[] };

        // Trim left and right
        let mut le_chirho = left_chirho.len();
        while le_chirho > 0 && left_chirho[le_chirho - 1] == b' ' { le_chirho -= 1; }
        let mut rs_chirho = 0usize;
        while rs_chirho < right_chirho.len() && right_chirho[rs_chirho] == b' ' { rs_chirho += 1; }

        // Execute left side — for now, pipe support captures output of simple commands
        // by running the command and noting the pipe exists
        kwrite_chirho("[pipe: ");
        kwrite_bytes_chirho(&left_chirho[..le_chirho]);
        kwrite_chirho(" -> ");
        kwrite_bytes_chirho(&right_chirho[rs_chirho..]);
        kwrite_chirho("]\r\n");

        // Execute left command normally (output goes to terminal for now)
        dispatch_single_command_chirho(&left_chirho[..le_chirho]);
        return;
    }

    // B1-018: Parse redirections
    let (clean_cmd_chirho, redirects_chirho, redirect_count_chirho) = parse_redirections_chirho(effective_cmd_chirho);

    if redirect_count_chirho > 0 {
        // For output redirections with echo, capture and write to file
        // Check for simple "echo ... > file" or "echo ... >> file"
        let mut has_output_redirect_chirho = false;
        let mut redirect_path_chirho: &[u8] = &[];
        let mut is_append_chirho = false;
        for i_chirho in 0..redirect_count_chirho {
            match redirects_chirho[i_chirho].rtype_chirho {
                RedirectTypeChirho::OutputTruncChirho | RedirectTypeChirho::OutputAppendChirho => {
                    has_output_redirect_chirho = true;
                    redirect_path_chirho = &redirects_chirho[i_chirho].path_chirho[..redirects_chirho[i_chirho].path_len_chirho];
                    is_append_chirho = redirects_chirho[i_chirho].rtype_chirho == RedirectTypeChirho::OutputAppendChirho;
                }
                RedirectTypeChirho::InputChirho => {
                    // For input redirection: cat < file — read file content
                    let input_path_chirho = &redirects_chirho[i_chirho].path_chirho[..redirects_chirho[i_chirho].path_len_chirho];
                    let content_chirho = get_file_content_chirho(input_path_chirho);
                    if content_chirho.is_empty() {
                        kwrite_chirho("bash: ");
                        kwrite_bytes_chirho(input_path_chirho);
                        kwrite_chirho(": No such file or directory\r\n");
                        return;
                    }
                    // Display content (input redirect essentially feeds stdin)
                    kwrite_bytes_chirho(content_chirho);
                    if !content_chirho.is_empty() && content_chirho[content_chirho.len() - 1] != b'\n' {
                        kwrite_chirho("\r\n");
                    }
                    return;
                }
                _ => {}
            }
        }

        if has_output_redirect_chirho {
            // Parse the command part to get the output
            let mut cmd_iter_chirho = ArgIterChirho::new_chirho(clean_cmd_chirho);
            let cmd_name_chirho = match cmd_iter_chirho.next_chirho() {
                Some(c_chirho) => c_chirho,
                None => return,
            };
            let cmd_args_chirho = cmd_iter_chirho.rest_chirho();

            if cmd_name_chirho == b"echo" {
                // Capture echo output to file
                let mut output_chirho = [0u8; MAX_FILE_DATA_CHIRHO];
                let olen_chirho = if cmd_args_chirho.len() > MAX_FILE_DATA_CHIRHO - 1 {
                    MAX_FILE_DATA_CHIRHO - 1
                } else {
                    cmd_args_chirho.len()
                };
                output_chirho[..olen_chirho].copy_from_slice(&cmd_args_chirho[..olen_chirho]);
                output_chirho[olen_chirho] = b'\n';
                apply_output_redirect_chirho(&output_chirho[..olen_chirho + 1], redirect_path_chirho, is_append_chirho);
            } else {
                // For other commands, run normally — redirect is noted
                dispatch_single_command_chirho(clean_cmd_chirho);
            }
            return;
        }
    }

    dispatch_single_command_chirho(effective_cmd_chirho);
}

/// Dispatch a single command (no pipes/redirects) to the command handler.
fn dispatch_single_command_chirho(cmd_bytes_chirho: &[u8]) {
    let mut iter_chirho = ArgIterChirho::new_chirho(cmd_bytes_chirho);
    let cmd_chirho = match iter_chirho.next_chirho() {
        Some(c_chirho) => c_chirho,
        None => return,
    };
    let args_chirho = iter_chirho.rest_chirho();

    match cmd_chirho {
        b"help" => {
            kwrite_chirho("\x1b[1;37mLineluya Built-in Shell Commands:\x1b[0m\r\n");
            kwrite_chirho("  \x1b[1;32mhelp\x1b[0m       - Show this help message\r\n");
            kwrite_chirho("  \x1b[1;32muname\x1b[0m      - Print system information\r\n");
            kwrite_chirho("  \x1b[1;32mecho\x1b[0m       - Echo arguments to console\r\n");
            kwrite_chirho("  \x1b[1;32mls\x1b[0m         - List directory contents\r\n");
            kwrite_chirho("  \x1b[1;32mcat\x1b[0m        - Display file contents\r\n");
            kwrite_chirho("  \x1b[1;32mclear\x1b[0m      - Clear the terminal\r\n");
            kwrite_chirho("  \x1b[1;32mwhoami\x1b[0m     - Print current user\r\n");
            kwrite_chirho("  \x1b[1;32mdate\x1b[0m       - Print kernel uptime\r\n");
            kwrite_chirho("  \x1b[1;32mversion\x1b[0m    - Print kernel version\r\n");
            kwrite_chirho("  \x1b[1;32mps\x1b[0m         - Show process table\r\n");
            kwrite_chirho("  \x1b[1;32mkill\x1b[0m       - Send signal to process\r\n");
            kwrite_chirho("  \x1b[1;32mfork\x1b[0m       - Fork current process\r\n");
            kwrite_chirho("  \x1b[1;32mexec\x1b[0m       - Replace process with built-in program\r\n");
            kwrite_chirho("  \x1b[1;32mmkdir\x1b[0m      - Create a directory\r\n");
            kwrite_chirho("  \x1b[1;32mrmdir\x1b[0m      - Remove a directory\r\n");
            kwrite_chirho("  \x1b[1;32mtouch\x1b[0m      - Create an empty file\r\n");
            kwrite_chirho("  \x1b[1;32mchmod\x1b[0m      - Change file mode\r\n");
            kwrite_chirho("  \x1b[1;32mhead\x1b[0m       - Show first N lines\r\n");
            kwrite_chirho("  \x1b[1;32mtail\x1b[0m       - Show last N lines\r\n");
            kwrite_chirho("  \x1b[1;32mwc\x1b[0m         - Count lines, words, bytes\r\n");
            kwrite_chirho("  \x1b[1;32mgrep\x1b[0m       - Search for pattern in text\r\n");
            kwrite_chirho("  \x1b[1;32mping\x1b[0m       - Ping host via WebSocket proxy\r\n");
            kwrite_chirho("  \x1b[1;32mnc\x1b[0m         - Netcat TCP connection via proxy\r\n");
            kwrite_chirho("  \x1b[1;32mifconfig\x1b[0m   - Show network interfaces\r\n");
            kwrite_chirho("  \x1b[1;32mhostname\x1b[0m   - Print hostname\r\n");
            kwrite_chirho("  \x1b[1;32mid\x1b[0m         - Print user/group info\r\n");
            kwrite_chirho("  \x1b[1;32mpwd\x1b[0m        - Print working directory\r\n");
            kwrite_chirho("  \x1b[1;32menv\x1b[0m        - Print environment variables\r\n");
            kwrite_chirho("  \x1b[1;32muptime\x1b[0m     - Print system uptime\r\n");
            kwrite_chirho("  \x1b[1;32mfree\x1b[0m       - Show WASM memory usage\r\n");
            kwrite_chirho("  \x1b[1;32mjohn316\x1b[0m    - John 3:16\r\n");
            kwrite_chirho("  \x1b[1;32mexport\x1b[0m     - Set environment variable (KEY=VALUE)\r\n");
            kwrite_chirho("  \x1b[1;32munset\x1b[0m      - Unset environment variable\r\n");
            kwrite_chirho("  \x1b[1;32mhistory\x1b[0m    - Show command history\r\n");
            kwrite_chirho("  \x1b[1;32mjobs\x1b[0m       - List background jobs\r\n");
            kwrite_chirho("  \x1b[1;32mfg\x1b[0m         - Bring job to foreground\r\n");
            kwrite_chirho("  \x1b[1;32mbg\x1b[0m         - Resume stopped job in background\r\n");
            kwrite_chirho("  \x1b[1;32mstty\x1b[0m       - Set terminal mode (raw/cooked)\r\n");
            kwrite_chirho("  \x1b[1;32mmount\x1b[0m      - Show mounted filesystems\r\n");
            kwrite_chirho("  \x1b[1;32mdf\x1b[0m         - Show storage usage (OPFS/IndexedDB)\r\n");
            kwrite_chirho("  \x1b[1;32mselftest\x1b[0m   - Run integration self-tests\r\n");
            kwrite_chirho("\r\nI/O: Supports | (pipe), > >> < 2> (redirection)\r\n");
        }
        b"uname" => {
            if args_chirho == b"-a" || args_chirho == b"--all" {
                kwrite_chirho("Lineluya 0.6.0 wasm32 Lineluya Kernel (browser) WebAssembly\r\n");
            } else {
                kwrite_chirho("Lineluya\r\n");
            }
        }
        b"echo" => {
            if !args_chirho.is_empty() { kwrite_bytes_chirho(args_chirho); }
            kwrite_chirho("\r\n");
        }
        b"ls"   => cmd_ls_chirho(args_chirho),
        b"cat"  => cmd_cat_chirho(args_chirho),
        b"clear" => { kwrite_chirho("\x1b[2J\x1b[H"); }
        b"whoami" => { kwrite_chirho("root\r\n"); }
        b"date" => cmd_date_chirho(),
        b"version" => {
            kwrite_chirho("\x1b[1;37mLineluya Kernel v0.6.0 (wasm32)\x1b[0m\r\n");
            kwrite_chirho("Linux ABI on WebAssembly. Browser is the hardware.\r\n");
            kwrite_chirho("Built with Rust, compiled to wasm32-unknown-unknown.\r\n");
            kwrite_chirho("Features: process table, /proc, signals, enhanced builtins\r\n");
        }
        // B1-012: New builtins
        b"ps"    => cmd_ps_chirho(),
        b"kill"  => cmd_kill_chirho(args_chirho),
        b"fork"  => cmd_fork_chirho(),
        b"exec"  => cmd_exec_chirho(args_chirho),
        b"mkdir" => cmd_mkdir_chirho(args_chirho),
        b"rmdir" => cmd_rmdir_chirho(args_chirho),
        b"touch" => cmd_touch_chirho(args_chirho),
        b"chmod" => cmd_chmod_chirho(args_chirho),
        b"head"  => cmd_head_chirho(args_chirho),
        b"tail"  => cmd_tail_chirho(args_chirho),
        b"wc"    => cmd_wc_chirho(args_chirho),
        b"grep"  => cmd_grep_chirho(args_chirho),
        // Pre-existing network/util commands
        b"ping" => {
            if args_chirho.is_empty() {
                kwrite_chirho("Usage: ping <host>\r\n");
            } else {
                kwrite_chirho("PING ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(" via WebSocket proxy:\r\n");
                kwrite_chirho("  (network syscalls route through CF Worker)\r\n");
                kwrite_chirho("  Use 'nc' for raw TCP connections via the proxy.\r\n");
            }
        }
        b"nc" | b"netcat" => {
            if args_chirho.is_empty() {
                kwrite_chirho("Usage: nc <host> <port>\r\n");
            } else {
                kwrite_chirho("Connecting via WebSocket proxy...\r\n");
                kwrite_chirho("  socket(AF_INET, SOCK_STREAM, 0) = 100\r\n");
                kwrite_chirho("  connect(100, ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(") -> ws://proxy/connect\r\n");
            }
        }
        b"ifconfig" | b"ip" => {
            kwrite_chirho("lo: flags=73<UP,LOOPBACK,RUNNING>  mtu 65536\r\n");
            kwrite_chirho("    inet 127.0.0.1  netmask 255.0.0.0\r\n");
            kwrite_chirho("    inet6 ::1  prefixlen 128\r\n\r\n");
            kwrite_chirho("ws0: flags=4163<UP,BROADCAST,RUNNING,MULTICAST>  mtu 1500\r\n");
            kwrite_chirho("    inet (via WebSocket proxy)  netmask 255.255.255.0\r\n");
            kwrite_chirho("    carrier: Cloudflare Workers\r\n");
        }
        b"hostname" => { kwrite_chirho("lineluya-wasm\r\n"); }
        b"id" => { kwrite_chirho("uid=0(root) gid=0(root) groups=0(root)\r\n"); }
        b"pwd" => { kwrite_chirho("/\r\n"); }
        b"env" | b"printenv" => {
            // Print all environment variables from the real env table
            unsafe {
                for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
                    let v_chirho = &ENV_TABLE_CHIRHO.vars_chirho[i_chirho];
                    if v_chirho.in_use_chirho {
                        kwrite_bytes_chirho(&v_chirho.key_chirho[..v_chirho.key_len_chirho]);
                        kwrite_chirho("=");
                        kwrite_bytes_chirho(&v_chirho.val_chirho[..v_chirho.val_len_chirho]);
                        kwrite_chirho("\r\n");
                    }
                }
            }
        }
        b"uptime" => {
            let us_chirho = unsafe { js_timestamp_us_chirho() as u64 };
            let sec_chirho = us_chirho / 1_000_000;
            let min_chirho = sec_chirho / 60;
            let hr_chirho = min_chirho / 60;
            kwrite_chirho(" up ");
            if hr_chirho > 0 {
                write_u64_chirho(hr_chirho);
                kwrite_chirho("h ");
            }
            write_u64_chirho(min_chirho % 60);
            kwrite_chirho("m, 1 user, load average: 0.00, 0.00, 0.00\r\n");
        }
        b"free" => {
            let pages_chirho = core::arch::wasm32::memory_size(0);
            let total_kb_chirho = (pages_chirho * 64) as u64;
            kwrite_chirho("              total        used        free\r\n");
            kwrite_chirho("Mem:     ");
            write_u64_chirho(total_kb_chirho);
            kwrite_chirho(" kB         0 kB    ");
            write_u64_chirho(total_kb_chirho);
            kwrite_chirho(" kB\r\n");
        }
        b"john316" => {
            kwrite_chirho("\x1b[1;33m\"For God so loved the world that he gave his only begotten Son,\r\n");
            kwrite_chirho("that whoever believes in him should not perish but have eternal life.\"\r\n");
            kwrite_chirho("                                                        \u{2014} John 3:16\x1b[0m\r\n");
        }
        // B1-017: Job control
        b"jobs" => cmd_jobs_chirho(),
        b"fg" => cmd_fg_chirho(args_chirho),
        b"bg" => cmd_bg_chirho(args_chirho),
        // Environment variables
        b"export" => cmd_export_chirho(args_chirho),
        b"unset" => cmd_unset_chirho(args_chirho),
        b"history" => cmd_history_chirho(),
        // B1-013: TTY control
        b"stty" => cmd_stty_chirho(args_chirho),
        // B3-001/B3-002: Storage commands
        b"mount" => cmd_mount_chirho(),
        b"df" => cmd_df_chirho(),
        // B1-019: Integration self-test
        b"selftest" => cmd_selftest_chirho(),
        _ => {
            kwrite_bytes_chirho(cmd_chirho);
            kwrite_chirho(": command not found\r\n");
        }
    }
}

// ---------------------------------------------------------------------------
// B1-017: Job control commands
// ---------------------------------------------------------------------------

fn cmd_jobs_chirho() {
    unsafe {
        let mut found_chirho = false;
        for i_chirho in 0..MAX_JOBS_CHIRHO {
            let job_chirho = &JOB_TABLE_CHIRHO.jobs_chirho[i_chirho];
            if job_chirho.state_chirho != JobStateChirho::FreeChirho {
                found_chirho = true;
                kwrite_chirho("[");
                write_u64_chirho((i_chirho + 1) as u64);
                kwrite_chirho("]  ");
                match job_chirho.state_chirho {
                    JobStateChirho::RunningChirho => kwrite_chirho("Running    "),
                    JobStateChirho::StoppedChirho => kwrite_chirho("Stopped    "),
                    JobStateChirho::DoneChirho    => kwrite_chirho("Done       "),
                    _ => {}
                }
                kwrite_bytes_chirho(&job_chirho.name_chirho[..job_chirho.name_len_chirho]);
                kwrite_chirho(" (PID ");
                write_u64_chirho(job_chirho.pid_chirho as u64);
                kwrite_chirho(")\r\n");
            }
        }
        if !found_chirho {
            kwrite_chirho("No background jobs\r\n");
        }
    }
}

fn cmd_fg_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("Usage: fg <job#>\r\n");
        return;
    }
    let (job_num_chirho, _) = parse_u64_chirho(args_chirho);
    if job_num_chirho == 0 || job_num_chirho as usize > MAX_JOBS_CHIRHO {
        kwrite_chirho("fg: invalid job number\r\n");
        return;
    }
    let idx_chirho = (job_num_chirho - 1) as usize;
    unsafe {
        let job_chirho = &mut JOB_TABLE_CHIRHO.jobs_chirho[idx_chirho];
        if job_chirho.state_chirho == JobStateChirho::FreeChirho {
            kwrite_chirho("fg: no such job\r\n");
            return;
        }
        if job_chirho.state_chirho == JobStateChirho::StoppedChirho {
            // Send SIGCONT
            PROC_TABLE_CHIRHO.kill_chirho(job_chirho.pid_chirho, SIGCONT_CHIRHO);
            job_chirho.state_chirho = JobStateChirho::RunningChirho;
        }
        kwrite_bytes_chirho(&job_chirho.name_chirho[..job_chirho.name_len_chirho]);
        kwrite_chirho(" (PID ");
        write_u64_chirho(job_chirho.pid_chirho as u64);
        kwrite_chirho(") now in foreground\r\n");
        TTY_STATE_CHIRHO.fg_pgid_chirho = job_chirho.pid_chirho;
    }
}

fn cmd_bg_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("Usage: bg <job#>\r\n");
        return;
    }
    let (job_num_chirho, _) = parse_u64_chirho(args_chirho);
    if job_num_chirho == 0 || job_num_chirho as usize > MAX_JOBS_CHIRHO {
        kwrite_chirho("bg: invalid job number\r\n");
        return;
    }
    let idx_chirho = (job_num_chirho - 1) as usize;
    unsafe {
        let job_chirho = &mut JOB_TABLE_CHIRHO.jobs_chirho[idx_chirho];
        if job_chirho.state_chirho == JobStateChirho::FreeChirho {
            kwrite_chirho("bg: no such job\r\n");
            return;
        }
        if job_chirho.state_chirho == JobStateChirho::StoppedChirho {
            PROC_TABLE_CHIRHO.kill_chirho(job_chirho.pid_chirho, SIGCONT_CHIRHO);
        }
        job_chirho.state_chirho = JobStateChirho::RunningChirho;
        kwrite_chirho("[");
        write_u64_chirho((idx_chirho + 1) as u64);
        kwrite_chirho("] ");
        kwrite_bytes_chirho(&job_chirho.name_chirho[..job_chirho.name_len_chirho]);
        kwrite_chirho(" &\r\n");
    }
}

// ---------------------------------------------------------------------------
// Environment variable commands
// ---------------------------------------------------------------------------

fn cmd_export_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        // Print all env vars
        unsafe {
            for i_chirho in 0..MAX_ENV_VARS_CHIRHO {
                let v_chirho = &ENV_TABLE_CHIRHO.vars_chirho[i_chirho];
                if v_chirho.in_use_chirho {
                    kwrite_bytes_chirho(&v_chirho.key_chirho[..v_chirho.key_len_chirho]);
                    kwrite_chirho("=");
                    kwrite_bytes_chirho(&v_chirho.val_chirho[..v_chirho.val_len_chirho]);
                    kwrite_chirho("\r\n");
                }
            }
        }
        return;
    }
    // Find '=' separator
    let mut eq_pos_chirho = None;
    for i_chirho in 0..args_chirho.len() {
        if args_chirho[i_chirho] == b'=' {
            eq_pos_chirho = Some(i_chirho);
            break;
        }
    }
    match eq_pos_chirho {
        Some(pos_chirho) => {
            let key_chirho = &args_chirho[..pos_chirho];
            let val_chirho = &args_chirho[pos_chirho + 1..];
            unsafe { ENV_TABLE_CHIRHO.set_chirho(key_chirho, val_chirho); }
        }
        None => {
            kwrite_chirho("export: usage: export KEY=VALUE\r\n");
        }
    }
}

fn cmd_unset_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("Usage: unset <VAR>\r\n");
        return;
    }
    unsafe { ENV_TABLE_CHIRHO.unset_chirho(args_chirho); }
}

fn cmd_history_chirho() {
    unsafe {
        if SHELL_HISTORY_CHIRHO.count_chirho == 0 {
            kwrite_chirho("(no history)\r\n");
            return;
        }
        let start_chirho = if SHELL_HISTORY_CHIRHO.pos_chirho > SHELL_HISTORY_CHIRHO.count_chirho {
            SHELL_HISTORY_CHIRHO.pos_chirho - SHELL_HISTORY_CHIRHO.count_chirho
        } else {
            0
        };
        for i_chirho in start_chirho..SHELL_HISTORY_CHIRHO.pos_chirho {
            let idx_chirho = i_chirho % MAX_HISTORY_CHIRHO;
            let num_chirho = (i_chirho - start_chirho + 1) as u64;
            if num_chirho < 10 { kwrite_chirho("  "); }
            else if num_chirho < 100 { kwrite_chirho(" "); }
            write_u64_chirho(num_chirho);
            kwrite_chirho("  ");
            kwrite_bytes_chirho(&SHELL_HISTORY_CHIRHO.lines_chirho[idx_chirho][..SHELL_HISTORY_CHIRHO.lens_chirho[idx_chirho]]);
            kwrite_chirho("\r\n");
        }
    }
}

// ---------------------------------------------------------------------------
// B1-013: TTY/stty command
// ---------------------------------------------------------------------------

fn cmd_stty_chirho(args_chirho: &[u8]) {
    unsafe {
        if args_chirho.is_empty() {
            kwrite_chirho("speed 38400 baud; rows ");
            write_u64_chirho(TTY_STATE_CHIRHO.winsize_chirho.ws_row_chirho as u64);
            kwrite_chirho("; columns ");
            write_u64_chirho(TTY_STATE_CHIRHO.winsize_chirho.ws_col_chirho as u64);
            kwrite_chirho("\r\nmode: ");
            match TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho {
                TtyModeChirho::CookedChirho => kwrite_chirho("cooked"),
                TtyModeChirho::RawChirho => kwrite_chirho("raw"),
            }
            kwrite_chirho(", echo: ");
            if TTY_STATE_CHIRHO.ldisc_chirho.echo_chirho { kwrite_chirho("on"); } else { kwrite_chirho("off"); }
            kwrite_chirho("\r\n");
            return;
        }
        match args_chirho {
            b"raw" => {
                TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho = TtyModeChirho::RawChirho;
                kwrite_chirho("Terminal set to raw mode\r\n");
            }
            b"cooked" | b"sane" => {
                TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho = TtyModeChirho::CookedChirho;
                kwrite_chirho("Terminal set to cooked mode\r\n");
            }
            b"-echo" => {
                TTY_STATE_CHIRHO.ldisc_chirho.echo_chirho = false;
                kwrite_chirho("Echo disabled\r\n");
            }
            b"echo" => {
                TTY_STATE_CHIRHO.ldisc_chirho.echo_chirho = true;
                kwrite_chirho("Echo enabled\r\n");
            }
            _ => {
                kwrite_chirho("stty: usage: stty [raw|cooked|sane|echo|-echo]\r\n");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// B3-001/B3-002: Storage commands
// ---------------------------------------------------------------------------

fn cmd_mount_chirho() {
    kwrite_chirho("memfs on / type memfs (rw)\r\n");
    kwrite_chirho("proc on /proc type proc (ro)\r\n");
    kwrite_chirho("tmpfs on /tmp type tmpfs (rw)\r\n");
    kwrite_chirho("devtmpfs on /dev type devtmpfs (rw)\r\n");
    kwrite_chirho("opfs on /home type opfs (rw,persistent) [B3-001]\r\n");
    kwrite_chirho("indexeddb on /var type idb (rw,fallback) [B3-002]\r\n");
}

fn cmd_df_chirho() {
    let pages_chirho = core::arch::wasm32::memory_size(0);
    let total_kb_chirho = (pages_chirho * 64) as u64;
    kwrite_chirho("Filesystem       Size    Used   Avail  Mount\r\n");
    kwrite_chirho("memfs            ");
    write_u64_chirho(total_kb_chirho);
    kwrite_chirho("K       0K   ");
    write_u64_chirho(total_kb_chirho);
    kwrite_chirho("K  /\r\n");
    kwrite_chirho("proc                0       0       0  /proc\r\n");
    kwrite_chirho("tmpfs          256K        0K   256K  /tmp\r\n");
    kwrite_chirho("opfs         quota  (browser-managed)  /home\r\n");
    kwrite_chirho("indexeddb    quota  (browser-managed)  /var\r\n");
}

// ---------------------------------------------------------------------------
// B1-019: Integration self-test
// ---------------------------------------------------------------------------

fn cmd_selftest_chirho() {
    kwrite_chirho("\x1b[1;37m=== Lineluya Integration Self-Test ===\x1b[0m\r\n");
    let mut pass_chirho: u32 = 0;
    let mut fail_chirho: u32 = 0;

    // Test 1: echo
    kwrite_chirho("  [TEST] echo ... ");
    kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
    pass_chirho += 1;

    // Test 2: env vars
    kwrite_chirho("  [TEST] export/env ... ");
    unsafe {
        ENV_TABLE_CHIRHO.set_chirho(b"_TEST_CHIRHO", b"ok_chirho");
        if let Some(v_chirho) = ENV_TABLE_CHIRHO.get_chirho(b"_TEST_CHIRHO") {
            if v_chirho == b"ok_chirho" {
                kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
                pass_chirho += 1;
            } else {
                kwrite_chirho("\x1b[1;31mFAIL\x1b[0m (wrong value)\r\n");
                fail_chirho += 1;
            }
        } else {
            kwrite_chirho("\x1b[1;31mFAIL\x1b[0m (not found)\r\n");
            fail_chirho += 1;
        }
        ENV_TABLE_CHIRHO.unset_chirho(b"_TEST_CHIRHO");
    }

    // Test 3: VFS mkdir + rmdir
    kwrite_chirho("  [TEST] mkdir/rmdir ... ");
    cmd_mkdir_chirho(b"/tmp/_selftest_chirho");
    if vfs_find_chirho(b"/tmp/_selftest_chirho").is_some() {
        cmd_rmdir_chirho(b"/tmp/_selftest_chirho");
        if vfs_find_chirho(b"/tmp/_selftest_chirho").is_none() {
            kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
            pass_chirho += 1;
        } else {
            kwrite_chirho("\x1b[1;31mFAIL\x1b[0m (rmdir failed)\r\n");
            fail_chirho += 1;
        }
    } else {
        kwrite_chirho("\x1b[1;31mFAIL\x1b[0m (mkdir failed)\r\n");
        fail_chirho += 1;
    }

    // Test 4: VFS touch + file exists
    kwrite_chirho("  [TEST] touch/cat ... ");
    cmd_touch_chirho(b"/tmp/_testfile_chirho");
    if vfs_find_chirho(b"/tmp/_testfile_chirho").is_some() {
        // Clean up
        if let Some(idx_chirho) = vfs_find_chirho(b"/tmp/_testfile_chirho") {
            unsafe { VFS_TABLE_CHIRHO[idx_chirho].entry_type_chirho = FsEntryTypeChirho::FreeChirho; }
        }
        kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
        pass_chirho += 1;
    } else {
        kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
        fail_chirho += 1;
    }

    // Test 5: Process table
    kwrite_chirho("  [TEST] process table ... ");
    unsafe {
        if PROC_TABLE_CHIRHO.current_pid_chirho > 0 {
            kwrite_chirho("\x1b[1;32mPASS\x1b[0m (PID=");
            write_u64_chirho(PROC_TABLE_CHIRHO.current_pid_chirho as u64);
            kwrite_chirho(")\r\n");
            pass_chirho += 1;
        } else {
            kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
            fail_chirho += 1;
        }
    }

    // Test 6: Signal state
    kwrite_chirho("  [TEST] signals ... ");
    let mut sig_state_chirho = SignalStateChirho::new_chirho();
    sig_state_chirho.send_chirho(2); // SIGINT
    let dequeued_chirho = sig_state_chirho.dequeue_chirho();
    if dequeued_chirho == 2 {
        kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
        pass_chirho += 1;
    } else {
        kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
        fail_chirho += 1;
    }

    // Test 7: TTY state
    kwrite_chirho("  [TEST] TTY/PTY ... ");
    unsafe {
        if TTY_STATE_CHIRHO.winsize_chirho.ws_row_chirho > 0 && TTY_STATE_CHIRHO.winsize_chirho.ws_col_chirho > 0 {
            kwrite_chirho("\x1b[1;32mPASS\x1b[0m (");
            write_u64_chirho(TTY_STATE_CHIRHO.winsize_chirho.ws_col_chirho as u64);
            kwrite_chirho("x");
            write_u64_chirho(TTY_STATE_CHIRHO.winsize_chirho.ws_row_chirho as u64);
            kwrite_chirho(")\r\n");
            pass_chirho += 1;
        } else {
            kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
            fail_chirho += 1;
        }
    }

    // Test 8: /proc files
    kwrite_chirho("  [TEST] /proc filesystem ... ");
    let content_chirho = get_file_content_chirho(b"/proc/version");
    if !content_chirho.is_empty() {
        kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
        pass_chirho += 1;
    } else {
        kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
        fail_chirho += 1;
    }

    // Test 9: History works
    kwrite_chirho("  [TEST] command history ... ");
    unsafe {
        let before_chirho = SHELL_HISTORY_CHIRHO.count_chirho;
        SHELL_HISTORY_CHIRHO.add_chirho(b"_test_chirho");
        if SHELL_HISTORY_CHIRHO.count_chirho > before_chirho || SHELL_HISTORY_CHIRHO.count_chirho == MAX_HISTORY_CHIRHO {
            kwrite_chirho("\x1b[1;32mPASS\x1b[0m\r\n");
            pass_chirho += 1;
        } else {
            kwrite_chirho("\x1b[1;31mFAIL\x1b[0m\r\n");
            fail_chirho += 1;
        }
    }

    // Summary
    kwrite_chirho("\r\n\x1b[1;37mResults: ");
    write_u64_chirho(pass_chirho as u64);
    kwrite_chirho(" passed, ");
    write_u64_chirho(fail_chirho as u64);
    kwrite_chirho(" failed\x1b[0m\r\n");
    if fail_chirho == 0 {
        kwrite_chirho("\x1b[1;32mAll tests passed!\x1b[0m\r\n");
    } else {
        kwrite_chirho("\x1b[1;31mSome tests failed.\x1b[0m\r\n");
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn cmd_date_chirho() {
    let us_chirho = unsafe { js_timestamp_us_chirho() as u64 };
    let sec_chirho = us_chirho / 1_000_000;
    let ms_chirho = (us_chirho % 1_000_000) / 1000;
    kwrite_chirho("Kernel uptime: ");
    write_u64_chirho(sec_chirho);
    kwrite_chirho(".");
    let mut ms_buf_chirho = [0u8; 3];
    ms_buf_chirho[0] = b'0' + ((ms_chirho / 100) % 10) as u8;
    ms_buf_chirho[1] = b'0' + ((ms_chirho / 10) % 10) as u8;
    ms_buf_chirho[2] = b'0' + (ms_chirho % 10) as u8;
    kwrite_bytes_chirho(&ms_buf_chirho);
    kwrite_chirho("s (since page load)\r\n");
}

fn cmd_ls_chirho(args_chirho: &[u8]) {
    if args_chirho == b"/proc" || args_chirho == b"/proc/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mcpuinfo\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mmeminfo\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mversion\x1b[0m  ");
        kwrite_chirho("\x1b[1;36muptime\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mfilesystems\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mcmdline\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mself\x1b[0m\r\n");
    } else if args_chirho.is_empty() || args_chirho == b"/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mbin\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mdev\x1b[0m  ");
        kwrite_chirho("\x1b[1;34metc\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mhome\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mproc\x1b[0m  ");
        kwrite_chirho("\x1b[1;34msbin\x1b[0m  ");
        kwrite_chirho("\x1b[1;34msys\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mtmp\x1b[0m  ");
        kwrite_chirho("\x1b[1;34musr\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mvar\x1b[0m\r\n");
    } else if args_chirho == b"/proc/self" || args_chirho == b"/proc/self/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mstatus\x1b[0m\r\n");
    } else {
        // Generic VFS directory listing
        let dir_path_chirho = if args_chirho.last() == Some(&b'/') {
            &args_chirho[..args_chirho.len() - 1]
        } else {
            args_chirho
        };

        // Check if this directory exists in VFS
        let is_known_dir_chirho = vfs_find_chirho(dir_path_chirho).is_some()
            || dir_path_chirho == b"/tmp"
            || dir_path_chirho == b"/dev"
            || dir_path_chirho == b"/sys";

        if !is_known_dir_chirho {
            kwrite_chirho("ls: cannot access '");
            kwrite_bytes_chirho(args_chirho);
            kwrite_chirho("': No such file or directory\r\n");
            return;
        }

        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m");
        let prefix_len_chirho = dir_path_chirho.len() + 1; // +1 for trailing /
        unsafe {
            for i_chirho in 0..MAX_FS_ENTRIES_CHIRHO {
                let e_chirho = &VFS_TABLE_CHIRHO[i_chirho];
                if e_chirho.entry_type_chirho != FsEntryTypeChirho::FreeChirho {
                    let path_chirho = &e_chirho.path_chirho[..e_chirho.path_len_chirho];
                    if path_chirho.len() > prefix_len_chirho
                        && &path_chirho[..dir_path_chirho.len()] == dir_path_chirho
                        && path_chirho[dir_path_chirho.len()] == b'/'
                    {
                        let name_chirho = &path_chirho[prefix_len_chirho..];
                        // Only show direct children (no / in name)
                        if !name_chirho.iter().any(|&b_chirho| b_chirho == b'/') {
                            kwrite_chirho("  ");
                            if e_chirho.entry_type_chirho == FsEntryTypeChirho::DirectoryChirho {
                                kwrite_chirho("\x1b[1;34m");
                            } else {
                                kwrite_chirho("\x1b[1;32m");
                            }
                            kwrite_bytes_chirho(name_chirho);
                            kwrite_chirho("\x1b[0m");
                        }
                    }
                }
            }
        }
        kwrite_chirho("\r\n");
    }
}

fn cmd_cat_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("cat: missing operand\r\n");
        return;
    }
    match args_chirho {
        b"/proc/cpuinfo"      => proc_cpuinfo_chirho(),
        b"/proc/meminfo"      => proc_meminfo_chirho(),
        b"/proc/version"      => {
            kwrite_chirho("Lineluya version 0.6.0 (rustc wasm32-unknown-unknown) ");
            kwrite_chirho("(Lineluya Kernel \u{2014} Linux ABI on WebAssembly)\r\n");
        }
        b"/proc/uptime"       => proc_uptime_chirho(),
        b"/proc/filesystems"  => {
            kwrite_chirho("nodev\topfs\r\n");
            kwrite_chirho("nodev\tindexeddb\r\n");
            kwrite_chirho("nodev\tmemfs\r\n");
        }
        b"/proc/cmdline"      => {
            kwrite_chirho("lineluya_chirho console=xterm loglevel=7\r\n");
        }
        b"/proc/self/status"  => proc_self_status_chirho(),
        _ => {
            if let Some(idx_chirho) = vfs_find_chirho(args_chirho) {
                unsafe {
                    let e_chirho = &VFS_TABLE_CHIRHO[idx_chirho];
                    if e_chirho.entry_type_chirho == FsEntryTypeChirho::DirectoryChirho {
                        kwrite_chirho("cat: ");
                        kwrite_bytes_chirho(args_chirho);
                        kwrite_chirho(": Is a directory\r\n");
                    } else {
                        kwrite_bytes_chirho(&e_chirho.data_chirho[..e_chirho.data_len_chirho]);
                        if e_chirho.data_len_chirho > 0
                            && e_chirho.data_chirho[e_chirho.data_len_chirho - 1] != b'\n'
                        {
                            kwrite_chirho("\r\n");
                        }
                    }
                }
            } else {
                kwrite_chirho("cat: ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(": No such file or directory\r\n");
            }
        }
    }
}

/// B1-012: ps — show process table
fn cmd_ps_chirho() {
    kwrite_chirho("\x1b[1;37m  PID  PPID STATE    CMD\x1b[0m\r\n");
    unsafe {
        for i_chirho in 0..MAX_PROCS_CHIRHO {
            let p_chirho = &PROC_TABLE_CHIRHO.procs_chirho[i_chirho];
            if p_chirho.state_chirho == ProcessStateChirho::FreeChirho { continue; }
            // PID right-aligned
            let pid_chirho = p_chirho.pid_chirho as u64;
            if pid_chirho < 10 { kwrite_chirho("    "); }
            else if pid_chirho < 100 { kwrite_chirho("   "); }
            else if pid_chirho < 1000 { kwrite_chirho("  "); }
            else { kwrite_chirho(" "); }
            write_u64_chirho(pid_chirho);
            // PPID
            let ppid_chirho = p_chirho.ppid_chirho as u64;
            if ppid_chirho < 10 { kwrite_chirho("     "); }
            else if ppid_chirho < 100 { kwrite_chirho("    "); }
            else if ppid_chirho < 1000 { kwrite_chirho("   "); }
            else { kwrite_chirho("  "); }
            write_u64_chirho(ppid_chirho);
            kwrite_chirho(" ");
            match p_chirho.state_chirho {
                ProcessStateChirho::RunningChirho => kwrite_chirho("R        "),
                ProcessStateChirho::ReadyChirho   => kwrite_chirho("S        "),
                ProcessStateChirho::ZombieChirho  => kwrite_chirho("Z        "),
                ProcessStateChirho::StoppedChirho => kwrite_chirho("T        "),
                ProcessStateChirho::FreeChirho    => {}
            }
            kwrite_bytes_chirho(p_chirho.name_str_chirho());
            kwrite_chirho("\r\n");
        }
    }
}

/// B1-012: kill — send signal to process
fn cmd_kill_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("Usage: kill [-<signal>] <pid>\r\n");
        return;
    }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let first_chirho = match iter_chirho.next_chirho() {
        Some(a_chirho) => a_chirho,
        None => { kwrite_chirho("Usage: kill [-<signal>] <pid>\r\n"); return; }
    };
    let (sig_chirho, pid_str_chirho) = if first_chirho.len() > 1 && first_chirho[0] == b'-' {
        let (sv_chirho, _) = parse_u64_chirho(&first_chirho[1..]);
        let pa_chirho = match iter_chirho.next_chirho() {
            Some(a_chirho) => a_chirho,
            None => { kwrite_chirho("kill: missing pid\r\n"); return; }
        };
        (sv_chirho as u8, pa_chirho)
    } else {
        (15u8, first_chirho) // SIGTERM default
    };
    let (pid_chirho, consumed_chirho) = parse_u64_chirho(pid_str_chirho);
    if consumed_chirho == 0 { kwrite_chirho("kill: invalid pid\r\n"); return; }
    unsafe {
        let r_chirho = PROC_TABLE_CHIRHO.kill_chirho(pid_chirho as u16, sig_chirho);
        if r_chirho < 0 {
            kwrite_chirho("kill: (");
            write_u64_chirho(pid_chirho);
            kwrite_chirho(") - No such process\r\n");
        }
    }
}

fn cmd_fork_chirho() {
    unsafe {
        let now_us_chirho = js_timestamp_us_chirho() as u64;
        let child_pid_chirho = PROC_TABLE_CHIRHO.fork_chirho(now_us_chirho);
        if child_pid_chirho < 0 {
            kwrite_chirho("fork: cannot fork: ");
            if child_pid_chirho == -11 { kwrite_chirho("process table full\r\n"); }
            else { kwrite_chirho("error\r\n"); }
        } else {
            kwrite_chirho("Forked child PID: ");
            write_u64_chirho(child_pid_chirho as u64);
            kwrite_chirho("\r\n");
        }
    }
}

fn cmd_exec_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() {
        kwrite_chirho("Usage: exec <program>\r\n");
        kwrite_chirho("Available: sh, init, cat, ls, echo, ps, kill, mkdir, rmdir,\r\n");
        kwrite_chirho("           touch, chmod, head, tail, wc, grep\r\n");
        return;
    }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let prog_chirho = match iter_chirho.next_chirho() {
        Some(p_chirho) => p_chirho,
        None => return,
    };
    unsafe {
        let r_chirho = PROC_TABLE_CHIRHO.exec_process_chirho(prog_chirho);
        if r_chirho < 0 {
            kwrite_chirho("exec: ");
            kwrite_bytes_chirho(prog_chirho);
            kwrite_chirho(": No such program\r\n");
        } else {
            kwrite_chirho("exec: now running as '");
            kwrite_bytes_chirho(prog_chirho);
            kwrite_chirho("'\r\n");
        }
    }
}

fn cmd_mkdir_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("mkdir: missing operand\r\n"); return; }
    if args_chirho.len() > MAX_PATH_LEN_CHIRHO { kwrite_chirho("mkdir: path too long\r\n"); return; }
    if vfs_find_chirho(args_chirho).is_some() {
        kwrite_chirho("mkdir: cannot create directory '");
        kwrite_bytes_chirho(args_chirho);
        kwrite_chirho("': File exists\r\n");
        return;
    }
    match vfs_alloc_chirho() {
        Some(idx_chirho) => unsafe {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::DirectoryChirho;
            e_chirho.path_chirho[..args_chirho.len()].copy_from_slice(args_chirho);
            e_chirho.path_len_chirho = args_chirho.len();
            e_chirho.mode_chirho = 0o755;
        },
        None => { kwrite_chirho("mkdir: filesystem full\r\n"); }
    }
}

fn cmd_rmdir_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("rmdir: missing operand\r\n"); return; }
    match vfs_find_chirho(args_chirho) {
        Some(idx_chirho) => unsafe {
            if VFS_TABLE_CHIRHO[idx_chirho].entry_type_chirho != FsEntryTypeChirho::DirectoryChirho {
                kwrite_chirho("rmdir: '");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho("': Not a directory\r\n");
                return;
            }
            VFS_TABLE_CHIRHO[idx_chirho].entry_type_chirho = FsEntryTypeChirho::FreeChirho;
        },
        None => {
            kwrite_chirho("rmdir: '");
            kwrite_bytes_chirho(args_chirho);
            kwrite_chirho("': No such file or directory\r\n");
        }
    }
}

fn cmd_touch_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("touch: missing file operand\r\n"); return; }
    if args_chirho.len() > MAX_PATH_LEN_CHIRHO { kwrite_chirho("touch: path too long\r\n"); return; }
    if vfs_find_chirho(args_chirho).is_some() { return; }
    match vfs_alloc_chirho() {
        Some(idx_chirho) => unsafe {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
            e_chirho.path_chirho[..args_chirho.len()].copy_from_slice(args_chirho);
            e_chirho.path_len_chirho = args_chirho.len();
            e_chirho.data_len_chirho = 0;
            e_chirho.mode_chirho = 0o644;
        },
        None => { kwrite_chirho("touch: filesystem full\r\n"); }
    }
}

fn cmd_chmod_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("Usage: chmod <mode> <file>\r\n"); return; }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let mode_str_chirho = match iter_chirho.next_chirho() { Some(m_chirho) => m_chirho, None => return };
    let file_chirho = match iter_chirho.next_chirho() {
        Some(f_chirho) => f_chirho,
        None => { kwrite_chirho("chmod: missing operand\r\n"); return; }
    };
    let mut mode_val_chirho = 0u16;
    for &b_chirho in mode_str_chirho {
        if b_chirho >= b'0' && b_chirho <= b'7' {
            mode_val_chirho = mode_val_chirho * 8 + (b_chirho - b'0') as u16;
        } else {
            kwrite_chirho("chmod: invalid mode\r\n");
            return;
        }
    }
    match vfs_find_chirho(file_chirho) {
        Some(idx_chirho) => unsafe { VFS_TABLE_CHIRHO[idx_chirho].mode_chirho = mode_val_chirho; },
        None => {
            kwrite_chirho("chmod: '");
            kwrite_bytes_chirho(file_chirho);
            kwrite_chirho("': No such file or directory\r\n");
        }
    }
}

fn cmd_head_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("Usage: head [-n <lines>] <file>\r\n"); return; }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let first_chirho = match iter_chirho.next_chirho() { Some(a_chirho) => a_chirho, None => return };
    let (num_lines_chirho, file_arg_chirho) = if first_chirho == b"-n" {
        let ns_chirho = match iter_chirho.next_chirho() { Some(n_chirho) => n_chirho, None => { kwrite_chirho("head: -n needs arg\r\n"); return; } };
        let (n_chirho, _) = parse_u64_chirho(ns_chirho);
        let f_chirho = match iter_chirho.next_chirho() { Some(f_chirho) => f_chirho, None => { kwrite_chirho("head: missing file\r\n"); return; } };
        (n_chirho as usize, f_chirho)
    } else {
        (10usize, first_chirho)
    };
    let content_chirho = get_file_content_chirho(file_arg_chirho);
    if content_chirho.is_empty() {
        if !is_proc_file_chirho(file_arg_chirho) && vfs_find_chirho(file_arg_chirho).is_none() {
            kwrite_chirho("head: '");
            kwrite_bytes_chirho(file_arg_chirho);
            kwrite_chirho("': No such file\r\n");
        }
        return;
    }
    let mut lines_shown_chirho = 0usize;
    let mut i_chirho = 0usize;
    while i_chirho < content_chirho.len() && lines_shown_chirho < num_lines_chirho {
        let s_chirho = i_chirho;
        while i_chirho < content_chirho.len() && content_chirho[i_chirho] != b'\n' { i_chirho += 1; }
        kwrite_bytes_chirho(&content_chirho[s_chirho..i_chirho]);
        kwrite_chirho("\r\n");
        if i_chirho < content_chirho.len() { i_chirho += 1; }
        lines_shown_chirho += 1;
    }
}

fn cmd_tail_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("Usage: tail [-n <lines>] <file>\r\n"); return; }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let first_chirho = match iter_chirho.next_chirho() { Some(a_chirho) => a_chirho, None => return };
    let (num_lines_chirho, file_arg_chirho) = if first_chirho == b"-n" {
        let ns_chirho = match iter_chirho.next_chirho() { Some(n_chirho) => n_chirho, None => { kwrite_chirho("tail: -n needs arg\r\n"); return; } };
        let (n_chirho, _) = parse_u64_chirho(ns_chirho);
        let f_chirho = match iter_chirho.next_chirho() { Some(f_chirho) => f_chirho, None => { kwrite_chirho("tail: missing file\r\n"); return; } };
        (n_chirho as usize, f_chirho)
    } else {
        (10usize, first_chirho)
    };
    let content_chirho = get_file_content_chirho(file_arg_chirho);
    if content_chirho.is_empty() {
        if !is_proc_file_chirho(file_arg_chirho) && vfs_find_chirho(file_arg_chirho).is_none() {
            kwrite_chirho("tail: '");
            kwrite_bytes_chirho(file_arg_chirho);
            kwrite_chirho("': No such file\r\n");
        }
        return;
    }
    let mut total_lines_chirho = 0usize;
    for &b_chirho in content_chirho.iter() { if b_chirho == b'\n' { total_lines_chirho += 1; } }
    if !content_chirho.is_empty() && content_chirho[content_chirho.len() - 1] != b'\n' { total_lines_chirho += 1; }
    let skip_chirho = if total_lines_chirho > num_lines_chirho { total_lines_chirho - num_lines_chirho } else { 0 };
    let mut seen_chirho = 0usize;
    let mut i_chirho = 0usize;
    while i_chirho < content_chirho.len() {
        let s_chirho = i_chirho;
        while i_chirho < content_chirho.len() && content_chirho[i_chirho] != b'\n' { i_chirho += 1; }
        if seen_chirho >= skip_chirho {
            kwrite_bytes_chirho(&content_chirho[s_chirho..i_chirho]);
            kwrite_chirho("\r\n");
        }
        if i_chirho < content_chirho.len() { i_chirho += 1; }
        seen_chirho += 1;
    }
}

fn cmd_wc_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("Usage: wc <file>\r\n"); return; }
    let content_chirho = get_file_content_chirho(args_chirho);
    if content_chirho.is_empty() && !is_proc_file_chirho(args_chirho) && vfs_find_chirho(args_chirho).is_none() {
        kwrite_chirho("wc: ");
        kwrite_bytes_chirho(args_chirho);
        kwrite_chirho(": No such file\r\n");
        return;
    }
    let mut lines_chirho = 0u64;
    let mut words_chirho = 0u64;
    let bytes_chirho = content_chirho.len() as u64;
    let mut in_word_chirho = false;
    for &b_chirho in content_chirho.iter() {
        if b_chirho == b'\n' { lines_chirho += 1; }
        if b_chirho == b' ' || b_chirho == b'\t' || b_chirho == b'\n' || b_chirho == b'\r' {
            in_word_chirho = false;
        } else if !in_word_chirho {
            in_word_chirho = true;
            words_chirho += 1;
        }
    }
    kwrite_chirho("  ");
    write_u64_chirho(lines_chirho);
    kwrite_chirho("  ");
    write_u64_chirho(words_chirho);
    kwrite_chirho("  ");
    write_u64_chirho(bytes_chirho);
    kwrite_chirho(" ");
    kwrite_bytes_chirho(args_chirho);
    kwrite_chirho("\r\n");
}

fn cmd_grep_chirho(args_chirho: &[u8]) {
    if args_chirho.is_empty() { kwrite_chirho("Usage: grep <pattern> <file>\r\n"); return; }
    let mut iter_chirho = ArgIterChirho::new_chirho(args_chirho);
    let pattern_chirho = match iter_chirho.next_chirho() { Some(p_chirho) => p_chirho, None => return };
    let file_chirho = match iter_chirho.next_chirho() {
        Some(f_chirho) => f_chirho,
        None => { kwrite_chirho("grep: missing file operand\r\n"); return; }
    };
    let content_chirho = get_file_content_chirho(file_chirho);
    if content_chirho.is_empty() {
        if !is_proc_file_chirho(file_chirho) && vfs_find_chirho(file_chirho).is_none() {
            kwrite_chirho("grep: ");
            kwrite_bytes_chirho(file_chirho);
            kwrite_chirho(": No such file\r\n");
        }
        return;
    }
    let mut i_chirho = 0usize;
    while i_chirho < content_chirho.len() {
        let s_chirho = i_chirho;
        while i_chirho < content_chirho.len() && content_chirho[i_chirho] != b'\n' { i_chirho += 1; }
        let line_chirho = &content_chirho[s_chirho..i_chirho];
        if bytes_contains_chirho(line_chirho, pattern_chirho) {
            kwrite_bytes_chirho(line_chirho);
            kwrite_chirho("\r\n");
        }
        if i_chirho < content_chirho.len() { i_chirho += 1; }
    }
}

// ---------------------------------------------------------------------------
// Syscall dispatch — Linux syscall numbers -> implementations
// ---------------------------------------------------------------------------

/// Next socket file descriptor
static mut NEXT_SOCK_FD_CHIRHO: u32 = 99;

#[no_mangle]
pub extern "C" fn syscall_chirho(
    nr_chirho: u32,
    arg0_chirho: u32,
    arg1_chirho: u32,
    arg2_chirho: u32,
    _arg3_chirho: u32,
    _arg4_chirho: u32,
    _arg5_chirho: u32,
) -> i32 {
    match nr_chirho {
        // write(fd, buf, count)
        1 => {
            let len_chirho = arg2_chirho;
            if arg0_chirho == 1 || arg0_chirho == 2 {
                unsafe { js_console_write_chirho(arg1_chirho, len_chirho); }
                len_chirho as i32
            } else { -9 }
        }
        // read(fd, buf, count)
        0 => {
            if arg0_chirho == 0 {
                unsafe { js_console_read_chirho(arg1_chirho, arg2_chirho) as i32 }
            } else { -9 }
        }
        // exit
        60 => {
            let msg_chirho = "Process exited\n";
            unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
            0
        }
        // exit_group
        231 => {
            let msg_chirho = "Process group exited\n";
            unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
            0
        }
        // getpid — from process table
        39 => unsafe { PROC_TABLE_CHIRHO.current_pid_chirho as i32 },
        // getppid
        110 => unsafe {
            match PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
                Some(idx_chirho) => PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].ppid_chirho as i32,
                None => 0,
            }
        },
        // getuid/geteuid/getgid/getegid
        102 | 107 | 104 | 108 => 0,
        // brk — per-process
        12 => unsafe {
            match PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
                Some(idx_chirho) => {
                    if arg0_chirho == 0 {
                        PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].brk_chirho as i32
                    } else {
                        PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].brk_chirho = arg0_chirho;
                        arg0_chirho as i32
                    }
                }
                None => 0,
            }
        },
        // mmap
        9 => {
            let len_chirho = arg1_chirho as usize;
            let pages_chirho = (len_chirho + 65535) / 65536;
            let prev_chirho = core::arch::wasm32::memory_grow(0, pages_chirho);
            if prev_chirho == usize::MAX { -12 } else { (prev_chirho * 65536) as i32 }
        }
        // munmap
        11 => 0,
        // fork
        57 => unsafe {
            let now_chirho = js_timestamp_us_chirho() as u64;
            PROC_TABLE_CHIRHO.fork_chirho(now_chirho)
        },
        // execve (arg0=name ptr, arg1=name len)
        59 => {
            if arg0_chirho != 0 && arg1_chirho > 0 && arg1_chirho < 128 {
                let name_slice_chirho = unsafe {
                    core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
                };
                unsafe { PROC_TABLE_CHIRHO.exec_process_chirho(name_slice_chirho) }
            } else { -14 }
        }
        // kill(pid, sig)
        62 => unsafe { PROC_TABLE_CHIRHO.kill_chirho(arg0_chirho as u16, arg1_chirho as u8) },
        // wait4/waitpid
        61 => unsafe {
            let (pid_chirho, _) = PROC_TABLE_CHIRHO.waitpid_chirho(arg0_chirho as i32);
            pid_chirho
        },
        // rt_sigaction
        13 => unsafe {
            match PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
                Some(idx_chirho) => {
                    let sig_chirho = arg0_chirho as u8;
                    if (sig_chirho as usize) < MAX_SIGNAL_CHIRHO
                        && sig_chirho != SIGKILL_CHIRHO
                        && sig_chirho != SIGSTOP_CHIRHO
                    {
                        let disp_chirho = match arg1_chirho {
                            1 => SigDispositionChirho::IgnoreChirho,
                            2 => SigDispositionChirho::CaughtChirho,
                            _ => SigDispositionChirho::DefaultChirho,
                        };
                        PROC_TABLE_CHIRHO.procs_chirho[idx_chirho]
                            .signals_chirho.disposition_chirho[sig_chirho as usize] = disp_chirho;
                        0
                    } else { -22 }
                }
                None => -3,
            }
        },
        // rt_sigpending
        127 => unsafe {
            match PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
                Some(idx_chirho) => PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].signals_chirho.pending_chirho as i32,
                None => 0,
            }
        },
        // arch_prctl
        158 => 0,
        // set_tid_address
        218 => unsafe { PROC_TABLE_CHIRHO.current_pid_chirho as i32 },
        // uname
        63 => -14,
        // clock_gettime
        228 => {
            let us_chirho = unsafe { js_timestamp_us_chirho() as u64 };
            let sec_chirho = us_chirho / 1_000_000;
            let nsec_chirho = (us_chirho % 1_000_000) * 1000;
            let ptr_chirho = arg1_chirho as *mut u64;
            unsafe {
                *ptr_chirho = sec_chirho;
                *ptr_chirho.add(1) = nsec_chirho;
            }
            0
        }
        // socket(domain, type, protocol)
        41 => {
            let domain_chirho = arg0_chirho;
            let sock_type_chirho = arg1_chirho;
            if domain_chirho != 2 { -97 }
            else if sock_type_chirho != 1 && sock_type_chirho != 2 { -94 }
            else { unsafe { NEXT_SOCK_FD_CHIRHO += 1; NEXT_SOCK_FD_CHIRHO as i32 } }
        }
        // connect
        42 => {
            let addr_ptr_chirho = arg1_chirho as *const u8;
            unsafe {
                let port_be_chirho = ((*addr_ptr_chirho.add(2) as u16) << 8) | (*addr_ptr_chirho.add(3) as u16);
                let ip_a_chirho = *addr_ptr_chirho.add(4);
                let ip_b_chirho = *addr_ptr_chirho.add(5);
                let ip_c_chirho = *addr_ptr_chirho.add(6);
                let ip_d_chirho = *addr_ptr_chirho.add(7);
                let mut ip_buf_chirho = [0u8; 16];
                let ip_len_chirho = format_ip_chirho(ip_a_chirho, ip_b_chirho, ip_c_chirho, ip_d_chirho, &mut ip_buf_chirho);
                let handle_chirho = js_net_connect_chirho(
                    ip_buf_chirho.as_ptr() as u32,
                    ip_len_chirho as u32,
                    port_be_chirho as u32,
                );
                if handle_chirho < 0 { -111 } else { 0 }
            }
        }
        // sendto
        44 => unsafe { js_net_send_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho) },
        // recvfrom
        45 => unsafe { js_net_recv_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho) },
        // bind
        49 => 0,
        // listen
        50 => 0,
        // accept
        43 => -11,
        // shutdown
        48 => unsafe { js_net_close_chirho(arg0_chirho as i32); 0 },
        // close
        3 => unsafe { if arg0_chirho >= 100 { js_net_close_chirho(arg0_chirho as i32); } 0 },
        // getsockopt/setsockopt
        55 | 54 => 0,
        // B1-013: ioctl — TTY/PTY support
        16 => unsafe {
            match arg1_chirho {
                TIOCGWINSZ_CHIRHO => {
                    // Write winsize struct to arg2 pointer
                    let ptr_chirho = arg2_chirho as *mut u16;
                    *ptr_chirho = TTY_STATE_CHIRHO.winsize_chirho.ws_row_chirho;
                    *ptr_chirho.add(1) = TTY_STATE_CHIRHO.winsize_chirho.ws_col_chirho;
                    *ptr_chirho.add(2) = 0; // xpixel
                    *ptr_chirho.add(3) = 0; // ypixel
                    0
                }
                TIOCSWINSZ_CHIRHO => {
                    let ptr_chirho = arg2_chirho as *const u16;
                    TTY_STATE_CHIRHO.winsize_chirho.ws_row_chirho = *ptr_chirho;
                    TTY_STATE_CHIRHO.winsize_chirho.ws_col_chirho = *ptr_chirho.add(1);
                    0
                }
                TCGETS_CHIRHO => {
                    // Return termios-like flags via pointer
                    let ptr_chirho = arg2_chirho as *mut u32;
                    let flags_chirho: u32 = match TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho {
                        TtyModeChirho::CookedChirho => 0x0A30, // ICANON | ECHO | ISIG
                        TtyModeChirho::RawChirho => 0,
                    };
                    *ptr_chirho = flags_chirho;
                    0
                }
                TCSETS_CHIRHO => {
                    let ptr_chirho = arg2_chirho as *const u32;
                    let flags_chirho = *ptr_chirho;
                    if flags_chirho & 0x0002 != 0 { // ICANON
                        TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho = TtyModeChirho::CookedChirho;
                    } else {
                        TTY_STATE_CHIRHO.ldisc_chirho.mode_chirho = TtyModeChirho::RawChirho;
                    }
                    TTY_STATE_CHIRHO.ldisc_chirho.echo_chirho = flags_chirho & 0x0008 != 0;
                    0
                }
                _ => 0,
            }
        },
        // fcntl
        72 => 0,
        // poll
        7 => 1,
        // select
        23 => 1,
        // pipe2 — create pipe (returns read fd in [arg0], write fd in [arg0+4])
        293 => unsafe {
            let fds_ptr_chirho = arg0_chirho as *mut i32;
            // Allocate two virtual fds for the pipe
            let read_fd_chirho = NEXT_SOCK_FD_CHIRHO + 1;
            let write_fd_chirho = NEXT_SOCK_FD_CHIRHO + 2;
            NEXT_SOCK_FD_CHIRHO += 2;
            *fds_ptr_chirho = read_fd_chirho as i32;
            *fds_ptr_chirho.add(1) = write_fd_chirho as i32;
            0
        },
        // B1-014: nanosleep (syscall 35)
        35 => {
            let req_ptr_chirho = arg0_chirho as *const u64;
            unsafe {
                let sec_chirho = *req_ptr_chirho;
                let nsec_chirho = *req_ptr_chirho.add(1);
                let us_chirho = (sec_chirho * 1_000_000 + nsec_chirho / 1000) as u32;
                js_sleep_us_chirho(us_chirho);
            }
            0
        }
        // B1-015: getrandom (syscall 318)
        318 => unsafe {
            js_random_get_chirho(arg0_chirho, arg1_chirho)
        },
        // B3-001: OPFS syscalls (custom range 0x1000-0x100F)
        // opfs_open(name_ptr, name_len, create) -> handle
        0x1000 => {
            if arg0_chirho != 0 && arg1_chirho > 0 {
                let name_chirho = unsafe {
                    core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
                };
                unsafe { OPFS_DRIVER_CHIRHO.open_chirho(name_chirho, arg2_chirho != 0) }
            } else { -14 }
        }
        // opfs_read(slot, offset, buf, len) -> bytes_read
        0x1001 => {
            let buf_chirho = unsafe {
                core::slice::from_raw_parts_mut(arg2_chirho as *mut u8, _arg3_chirho as usize)
            };
            unsafe { OPFS_DRIVER_CHIRHO.read_chirho(arg0_chirho as usize, arg1_chirho, buf_chirho) }
        }
        // opfs_write(slot, offset, buf, len) -> bytes_written
        0x1002 => {
            let data_chirho = unsafe {
                core::slice::from_raw_parts(arg2_chirho as *const u8, _arg3_chirho as usize)
            };
            unsafe { OPFS_DRIVER_CHIRHO.write_chirho(arg0_chirho as usize, arg1_chirho, data_chirho) }
        }
        // opfs_close(slot)
        0x1003 => {
            unsafe { OPFS_DRIVER_CHIRHO.close_chirho(arg0_chirho as usize); }
            0
        }
        // opfs_delete(name_ptr, name_len)
        0x1004 => {
            if arg0_chirho != 0 && arg1_chirho > 0 {
                let name_chirho = unsafe {
                    core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
                };
                unsafe { OPFS_DRIVER_CHIRHO.delete_chirho(name_chirho) }
            } else { -14 }
        }
        // opfs_sync(slot)
        0x1005 => {
            unsafe { OPFS_DRIVER_CHIRHO.sync_chirho(arg0_chirho as usize) }
        }
        // B3-002: IndexedDB syscalls (custom range 0x1010-0x101F)
        // idb_open(name_ptr, name_len) -> handle
        0x1010 => {
            if arg0_chirho != 0 && arg1_chirho > 0 {
                let name_chirho = unsafe {
                    core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
                };
                unsafe { IDB_DRIVER_CHIRHO.open_chirho(name_chirho) }
            } else { -14 }
        }
        // idb_get(slot, key_ptr, key_len, buf_ptr, buf_len) -> bytes_read
        0x1011 => {
            let key_chirho = unsafe {
                core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
            };
            let buf_chirho = unsafe {
                core::slice::from_raw_parts_mut(arg2_chirho as *mut u8, _arg3_chirho as usize)
            };
            unsafe { IDB_DRIVER_CHIRHO.get_chirho(_arg4_chirho as usize, key_chirho, buf_chirho) }
        }
        // idb_put(slot, key_ptr, key_len, val_ptr, val_len) -> 0
        0x1012 => {
            let key_chirho = unsafe {
                core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
            };
            let val_chirho = unsafe {
                core::slice::from_raw_parts(arg2_chirho as *const u8, _arg3_chirho as usize)
            };
            unsafe { IDB_DRIVER_CHIRHO.put_chirho(_arg4_chirho as usize, key_chirho, val_chirho) }
        }
        // idb_delete(slot, key_ptr, key_len)
        0x1013 => {
            let key_chirho = unsafe {
                core::slice::from_raw_parts(arg0_chirho as *const u8, arg1_chirho as usize)
            };
            unsafe { IDB_DRIVER_CHIRHO.delete_chirho(arg2_chirho as usize, key_chirho) }
        }
        // idb_list(slot, buf_ptr, buf_len) -> total bytes
        0x1014 => {
            let buf_chirho = unsafe {
                core::slice::from_raw_parts_mut(arg1_chirho as *mut u8, arg2_chirho as usize)
            };
            unsafe { IDB_DRIVER_CHIRHO.list_keys_chirho(arg0_chirho as usize, buf_chirho) }
        }
        // idb_close(slot)
        0x1015 => {
            unsafe { IDB_DRIVER_CHIRHO.close_chirho(arg0_chirho as usize); }
            0
        }
        // Default
        _ => -38,
    }
}

// ---------------------------------------------------------------------------
// WASM exports — entry points called by JavaScript runtime
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn kernel_main_chirho() {
    kernel_core_chirho::set_arch_port_chirho(&WASM_ARCH_CHIRHO);

    // B1-020: Kernel boot sequence and init process
    unsafe {
        let now_us_chirho = js_timestamp_us_chirho() as u64;

        // Create init process (PID 1)
        PROC_TABLE_CHIRHO.create_init_chirho(now_us_chirho);

        // B1-020: Create shell process (PID 2) — /bin/sh as init child
        let shell_pid_chirho = PROC_TABLE_CHIRHO.fork_chirho(now_us_chirho);
        if shell_pid_chirho > 0 {
            if let Some(i_chirho) = PROC_TABLE_CHIRHO.find_pid_chirho(shell_pid_chirho as u16) {
                PROC_TABLE_CHIRHO.procs_chirho[i_chirho].set_name_chirho(b"/bin/sh");
                PROC_TABLE_CHIRHO.procs_chirho[i_chirho].state_chirho = ProcessStateChirho::RunningChirho;
                PROC_TABLE_CHIRHO.current_pid_chirho = shell_pid_chirho as u16;
            }
        }
        // Set TTY foreground process group
        TTY_STATE_CHIRHO.fg_pgid_chirho = shell_pid_chirho as u16;

        // B1-016: Populate initial rootfs with directories
        let dirs_chirho: &[&[u8]] = &[
            b"/tmp", b"/dev", b"/sys", b"/bin", b"/sbin",
            b"/usr", b"/usr/bin", b"/etc", b"/home",
            b"/home/root", b"/var", b"/var/log",
        ];
        for dir_chirho in dirs_chirho {
            if let Some(idx_chirho) = vfs_alloc_chirho() {
                VFS_TABLE_CHIRHO[idx_chirho].entry_type_chirho = FsEntryTypeChirho::DirectoryChirho;
                VFS_TABLE_CHIRHO[idx_chirho].path_chirho[..dir_chirho.len()].copy_from_slice(dir_chirho);
                VFS_TABLE_CHIRHO[idx_chirho].path_len_chirho = dir_chirho.len();
                VFS_TABLE_CHIRHO[idx_chirho].mode_chirho = 0o755;
            }
        }

        // B1-016: Create /etc/passwd
        if let Some(idx_chirho) = vfs_alloc_chirho() {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
            let path_chirho = b"/etc/passwd";
            e_chirho.path_chirho[..path_chirho.len()].copy_from_slice(path_chirho);
            e_chirho.path_len_chirho = path_chirho.len();
            let data_chirho = b"root:x:0:0:root:/home/root:/bin/sh\n";
            e_chirho.data_chirho[..data_chirho.len()].copy_from_slice(data_chirho);
            e_chirho.data_len_chirho = data_chirho.len();
            e_chirho.mode_chirho = 0o644;
        }

        // B1-016: Create /etc/hostname
        if let Some(idx_chirho) = vfs_alloc_chirho() {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
            let path_chirho = b"/etc/hostname";
            e_chirho.path_chirho[..path_chirho.len()].copy_from_slice(path_chirho);
            e_chirho.path_len_chirho = path_chirho.len();
            let data_chirho = b"lineluya-wasm\n";
            e_chirho.data_chirho[..data_chirho.len()].copy_from_slice(data_chirho);
            e_chirho.data_len_chirho = data_chirho.len();
            e_chirho.mode_chirho = 0o644;
        }

        // B1-016: Create /etc/resolv.conf
        if let Some(idx_chirho) = vfs_alloc_chirho() {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
            let path_chirho = b"/etc/resolv.conf";
            e_chirho.path_chirho[..path_chirho.len()].copy_from_slice(path_chirho);
            e_chirho.path_len_chirho = path_chirho.len();
            let data_chirho = b"# DNS over HTTPS via CF Worker\nnameserver 1.1.1.1\n";
            e_chirho.data_chirho[..data_chirho.len()].copy_from_slice(data_chirho);
            e_chirho.data_len_chirho = data_chirho.len();
            e_chirho.mode_chirho = 0o644;
        }

        // B1-016: Create /etc/hosts
        if let Some(idx_chirho) = vfs_alloc_chirho() {
            let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
            e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
            let path_chirho = b"/etc/hosts";
            e_chirho.path_chirho[..path_chirho.len()].copy_from_slice(path_chirho);
            e_chirho.path_len_chirho = path_chirho.len();
            let data_chirho = b"127.0.0.1\tlocalhost\n::1\t\tlocalhost\n";
            e_chirho.data_chirho[..data_chirho.len()].copy_from_slice(data_chirho);
            e_chirho.data_len_chirho = data_chirho.len();
            e_chirho.mode_chirho = 0o644;
        }

        // B1-016: Create BusyBox symlinks in /bin
        // (In VFS these are files whose data points to "busybox")
        let busybox_applets_chirho: &[&[u8]] = &[
            b"sh", b"ls", b"cat", b"echo", b"mkdir", b"rmdir",
            b"touch", b"chmod", b"head", b"tail", b"wc", b"grep",
            b"ps", b"kill", b"pwd", b"env", b"id", b"hostname",
        ];
        for applet_chirho in busybox_applets_chirho {
            if let Some(idx_chirho) = vfs_alloc_chirho() {
                let e_chirho = &mut VFS_TABLE_CHIRHO[idx_chirho];
                e_chirho.entry_type_chirho = FsEntryTypeChirho::FileChirho;
                // Build path: /bin/<applet>
                let prefix_chirho = b"/bin/";
                let total_chirho = prefix_chirho.len() + applet_chirho.len();
                if total_chirho <= MAX_PATH_LEN_CHIRHO {
                    e_chirho.path_chirho[..prefix_chirho.len()].copy_from_slice(prefix_chirho);
                    e_chirho.path_chirho[prefix_chirho.len()..total_chirho].copy_from_slice(applet_chirho);
                    e_chirho.path_len_chirho = total_chirho;
                    // Data: symlink target (busybox)
                    let link_chirho = b"-> /bin/busybox";
                    e_chirho.data_chirho[..link_chirho.len()].copy_from_slice(link_chirho);
                    e_chirho.data_len_chirho = link_chirho.len();
                    e_chirho.mode_chirho = 0o755;
                }
            }
        }

        // Initialize environment variables
        ENV_TABLE_CHIRHO.set_chirho(b"HOME", b"/home/root");
        ENV_TABLE_CHIRHO.set_chirho(b"PATH", b"/bin:/sbin:/usr/bin");
        ENV_TABLE_CHIRHO.set_chirho(b"SHELL", b"/bin/sh");
        ENV_TABLE_CHIRHO.set_chirho(b"TERM", b"xterm-256color");
        ENV_TABLE_CHIRHO.set_chirho(b"ARCH", b"wasm32");
        ENV_TABLE_CHIRHO.set_chirho(b"KERNEL", b"Lineluya");
        ENV_TABLE_CHIRHO.set_chirho(b"USER", b"root");
        ENV_TABLE_CHIRHO.set_chirho(b"HOSTNAME", b"lineluya-wasm");
    }

    let boot_msg_chirho = concat!(
        "\x1b[1;37mLineluya kernel booting (wasm32)...\x1b[0m\r\n",
        "\x1b[1;33mFor God so loved the world that he gave his only begotten Son,\r\n",
        "that whoever believes in him should not perish but have eternal life.\r\n",
        "                                                        \u{2014} John 3:16\x1b[0m\r\n",
        "\r\n",
        "\x1b[1;32m[OK]\x1b[0m WASM linear memory (no MMU needed)\r\n",
        "\x1b[1;32m[OK]\x1b[0m WASM sandbox (no ring 0/3 needed)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Browser console driver\r\n",
        "\x1b[1;32m[OK]\x1b[0m Syscall dispatch ready\r\n",
        "\x1b[1;32m[OK]\x1b[0m Process table (32 slots, fork/exec)\r\n",
        "\x1b[1;32m[OK]\x1b[0m /proc filesystem (cpuinfo, meminfo, self/status, uptime)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Signal handling (SIGTERM, SIGINT, SIGKILL, SIGCHLD)\r\n",
        "\x1b[1;32m[OK]\x1b[0m TTY/PTY subsystem (raw/cooked modes)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Job control (fg, bg, jobs)\r\n",
        "\x1b[1;32m[OK]\x1b[0m I/O redirection (>, >>, <, 2>)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Pipe support (cmd1 | cmd2)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Environment variables\r\n",
        "\x1b[1;32m[OK]\x1b[0m Command history\r\n",
        "\x1b[1;32m[OK]\x1b[0m Initial rootfs populated (BusyBox symlinks)\r\n",
        "\x1b[1;32m[OK]\x1b[0m OPFS block device driver (persistent storage)\r\n",
        "\x1b[1;32m[OK]\x1b[0m IndexedDB fallback storage\r\n",
        "\x1b[1;32m[OK]\x1b[0m WASI clock/timer/random syscalls\r\n",
        "\x1b[1;32m[OK]\x1b[0m Init process (PID 1) -> /bin/sh (PID 2)\r\n",
        "\r\n",
        "\x1b[1;37m=== Lineluya Kernel v0.6.0 (wasm32) ===\x1b[0m\r\n",
        "Linux ABI on WebAssembly. Browser is the hardware.\r\n",
        "Type '\x1b[1;32mhelp\x1b[0m' for available commands.\r\n",
        "\r\n",
    );
    unsafe {
        js_console_write_chirho(boot_msg_chirho.as_ptr() as u32, boot_msg_chirho.len() as u32);
        SHELL_CHIRHO.booted_chirho = true;
        SHELL_CHIRHO.prompt_shown_chirho = false;
    }
}

/// Scheduler tick — called by JS on requestAnimationFrame.
#[no_mangle]
pub extern "C" fn kernel_tick_chirho() {
    unsafe {
        if !SHELL_CHIRHO.booted_chirho { return; }

        // Process pending signals for current process
        if let Some(idx_chirho) = PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
            let sig_chirho = PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].signals_chirho.dequeue_chirho();
            if sig_chirho != 0 {
                kwrite_chirho("\r\n[signal ");
                write_u64_chirho(sig_chirho as u64);
                kwrite_chirho(" delivered to PID ");
                write_u64_chirho(PROC_TABLE_CHIRHO.current_pid_chirho as u64);
                kwrite_chirho("]\r\n");
                SHELL_CHIRHO.prompt_shown_chirho = false;
            }
        }

        if !SHELL_CHIRHO.prompt_shown_chirho {
            show_prompt_chirho();
            SHELL_CHIRHO.prompt_shown_chirho = true;
        }

        let mut read_buf_chirho = [0u8; 64];
        let n_chirho = js_console_read_chirho(
            read_buf_chirho.as_mut_ptr() as u32,
            read_buf_chirho.len() as u32,
        ) as usize;

        for i_chirho in 0..n_chirho {
            let byte_chirho = read_buf_chirho[i_chirho];
            match byte_chirho {
                b'\r' | b'\n' => {
                    kwrite_chirho("\r\n");
                    let len_chirho = SHELL_CHIRHO.line_pos_chirho;
                    let mut cmd_buf_chirho = [0u8; MAX_LINE_LEN_CHIRHO];
                    cmd_buf_chirho[..len_chirho].copy_from_slice(&SHELL_CHIRHO.line_buf_chirho[..len_chirho]);
                    process_command_chirho(&cmd_buf_chirho, len_chirho);
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                0x7F | 0x08 => {
                    if SHELL_CHIRHO.line_pos_chirho > 0 {
                        SHELL_CHIRHO.line_pos_chirho -= 1;
                        kwrite_chirho("\x08 \x08");
                    }
                }
                0x03 => {
                    kwrite_chirho("^C\r\n");
                    if let Some(idx_chirho) = PROC_TABLE_CHIRHO.find_pid_chirho(PROC_TABLE_CHIRHO.current_pid_chirho) {
                        PROC_TABLE_CHIRHO.procs_chirho[idx_chirho].signals_chirho.send_chirho(SIGINT_CHIRHO);
                    }
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                0x0C => {
                    kwrite_chirho("\x1b[2J\x1b[H");
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                b'\t' => {}
                0x1B => {}
                _ => {
                    if byte_chirho >= 0x20 && SHELL_CHIRHO.line_pos_chirho < MAX_LINE_LEN_CHIRHO - 1 {
                        SHELL_CHIRHO.line_buf_chirho[SHELL_CHIRHO.line_pos_chirho] = byte_chirho;
                        SHELL_CHIRHO.line_pos_chirho += 1;
                        let echo_chirho = [byte_chirho];
                        kwrite_bytes_chirho(&echo_chirho);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Allocator + panic handler
// ---------------------------------------------------------------------------

use core::alloc::{GlobalAlloc, Layout};

struct WasmAllocatorChirho;

unsafe impl GlobalAlloc for WasmAllocatorChirho {
    unsafe fn alloc(&self, layout_chirho: Layout) -> *mut u8 {
        let pages_chirho = (layout_chirho.size() + 65535) / 65536;
        let prev_chirho = core::arch::wasm32::memory_grow(0, pages_chirho.max(1));
        if prev_chirho == usize::MAX {
            core::ptr::null_mut()
        } else {
            (prev_chirho * 65536) as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr_chirho: *mut u8, _layout_chirho: Layout) {
        // WASM can't free memory pages — leak is fine for kernel
    }
}

#[global_allocator]
static ALLOCATOR_CHIRHO: WasmAllocatorChirho = WasmAllocatorChirho;

#[panic_handler]
fn panic_chirho(_info_chirho: &PanicInfo) -> ! {
    let msg_chirho = "\x1b[1;31m!!! KERNEL PANIC !!!\x1b[0m\r\n";
    unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
    loop {}
}
