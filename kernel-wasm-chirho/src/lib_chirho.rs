// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! # Lineluya Kernel — WASM32 Browser Target
//!
//! This is a real kernel compiled to WebAssembly. The browser is the hardware.
//! Programs compiled to wasm32 make Linux syscalls -> this kernel handles them
//! using browser APIs (Canvas, OPFS, WebSocket, Web Workers).
//!
//! ## Features (B1-009 through B1-012)
//! - **B1-009**: Process table with fork/exec — processes are state machines
//! - **B1-010**: /proc filesystem (cpuinfo, meminfo, self/status, uptime)
//! - **B1-011**: Signal handling framework (SIGTERM, SIGINT, SIGKILL, SIGCHLD)
//! - **B1-012**: Enhanced shell builtins (ps, kill, mkdir, rmdir, touch, chmod,
//!              head, tail, wc, grep)
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

const MAX_FS_ENTRIES_CHIRHO: usize = 64;
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
                proc_buf_append_chirho(b"Lineluya version 0.5.0 (rustc wasm32-unknown-unknown)\n");
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

    let cmd_bytes_chirho = &line_chirho[start_chirho..end_chirho];
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
        }
        b"uname" => {
            if args_chirho == b"-a" || args_chirho == b"--all" {
                kwrite_chirho("Lineluya 0.5.0 wasm32 Lineluya Kernel (browser) WebAssembly\r\n");
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
            kwrite_chirho("\x1b[1;37mLineluya Kernel v0.5.0 (wasm32)\x1b[0m\r\n");
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
        b"env" => {
            kwrite_chirho("HOME=/root\r\n");
            kwrite_chirho("PATH=/bin:/sbin:/usr/bin\r\n");
            kwrite_chirho("SHELL=/bin/ksh\r\n");
            kwrite_chirho("TERM=xterm-256color\r\n");
            kwrite_chirho("ARCH=wasm32\r\n");
            kwrite_chirho("KERNEL=Lineluya\r\n");
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
        _ => {
            kwrite_bytes_chirho(cmd_chirho);
            kwrite_chirho(": command not found\r\n");
        }
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
    if args_chirho.is_empty() || args_chirho == b"/proc" || args_chirho == b"/proc/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mcpuinfo\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mmeminfo\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mversion\x1b[0m  ");
        kwrite_chirho("\x1b[1;36muptime\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mfilesystems\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mcmdline\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mself\x1b[0m\r\n");
    } else if args_chirho == b"/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mproc\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mdev\x1b[0m  ");
        kwrite_chirho("\x1b[1;34msys\x1b[0m  ");
        kwrite_chirho("\x1b[1;34mtmp\x1b[0m\r\n");
    } else if args_chirho == b"/proc/self" || args_chirho == b"/proc/self/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
        kwrite_chirho("\x1b[1;36mstatus\x1b[0m\r\n");
    } else if args_chirho == b"/tmp" || args_chirho == b"/tmp/" {
        kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m");
        unsafe {
            for i_chirho in 0..MAX_FS_ENTRIES_CHIRHO {
                let e_chirho = &VFS_TABLE_CHIRHO[i_chirho];
                if e_chirho.entry_type_chirho != FsEntryTypeChirho::FreeChirho {
                    let path_chirho = &e_chirho.path_chirho[..e_chirho.path_len_chirho];
                    if path_chirho.len() > 5 && &path_chirho[..5] == b"/tmp/" {
                        let name_chirho = &path_chirho[5..];
                        if !name_chirho.iter().any(|&b_chirho| b_chirho == b'/') {
                            kwrite_chirho("  ");
                            if e_chirho.entry_type_chirho == FsEntryTypeChirho::DirectoryChirho {
                                kwrite_chirho("\x1b[1;34m");
                            }
                            kwrite_bytes_chirho(name_chirho);
                            if e_chirho.entry_type_chirho == FsEntryTypeChirho::DirectoryChirho {
                                kwrite_chirho("\x1b[0m");
                            }
                        }
                    }
                }
            }
        }
        kwrite_chirho("\r\n");
    } else {
        kwrite_chirho("ls: cannot access '");
        kwrite_bytes_chirho(args_chirho);
        kwrite_chirho("': No such file or directory\r\n");
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
            kwrite_chirho("Lineluya version 0.5.0 (rustc wasm32-unknown-unknown) ");
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
        // ioctl
        16 => 0,
        // fcntl
        72 => 0,
        // poll
        7 => 1,
        // select
        23 => 1,
        // pipe2
        293 => -38,
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

    // Initialize process table
    unsafe {
        let now_us_chirho = js_timestamp_us_chirho() as u64;
        PROC_TABLE_CHIRHO.create_init_chirho(now_us_chirho);
        // Create shell process (PID 2)
        let shell_pid_chirho = PROC_TABLE_CHIRHO.fork_chirho(now_us_chirho);
        if shell_pid_chirho > 0 {
            if let Some(i_chirho) = PROC_TABLE_CHIRHO.find_pid_chirho(shell_pid_chirho as u16) {
                PROC_TABLE_CHIRHO.procs_chirho[i_chirho].set_name_chirho(b"ksh");
                PROC_TABLE_CHIRHO.procs_chirho[i_chirho].state_chirho = ProcessStateChirho::RunningChirho;
                PROC_TABLE_CHIRHO.current_pid_chirho = shell_pid_chirho as u16;
            }
        }
        // Create /tmp in VFS
        if let Some(idx_chirho) = vfs_alloc_chirho() {
            VFS_TABLE_CHIRHO[idx_chirho].entry_type_chirho = FsEntryTypeChirho::DirectoryChirho;
            let p_chirho = b"/tmp";
            VFS_TABLE_CHIRHO[idx_chirho].path_chirho[..p_chirho.len()].copy_from_slice(p_chirho);
            VFS_TABLE_CHIRHO[idx_chirho].path_len_chirho = p_chirho.len();
        }
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
        "\x1b[1;32m[OK]\x1b[0m Enhanced shell (ps, kill, mkdir, grep, wc...)\r\n",
        "\r\n",
        "\x1b[1;37m=== Lineluya Kernel v0.5.0 (wasm32) ===\x1b[0m\r\n",
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
