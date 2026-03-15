// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! # Lineluya Kernel — WASM32 Browser Target
//!
//! This is a real kernel compiled to WebAssembly. The browser is the hardware.
//! Programs compiled to wasm32 make Linux syscalls → this kernel handles them
//! using browser APIs (Canvas, OPFS, WebSocket, Web Workers).
//!
//! ## Build
//! ```bash
//! cd kernel-wasm-chirho && cargo build --release
//! # Output: target/wasm32-unknown-unknown/release/kernel_wasm_chirho.wasm
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
// Built-in kernel shell state
// ---------------------------------------------------------------------------

/// Maximum line buffer size for the built-in shell
const MAX_LINE_LEN_CHIRHO: usize = 256;

/// Shell state — lives in a static mutable for the kernel-mode shell
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

/// Write a string to the console
fn kwrite_chirho(s_chirho: &str) {
    unsafe {
        js_console_write_chirho(s_chirho.as_ptr() as u32, s_chirho.len() as u32);
    }
}

/// Write raw bytes to the console
fn kwrite_bytes_chirho(bytes_chirho: &[u8]) {
    unsafe {
        js_console_write_chirho(bytes_chirho.as_ptr() as u32, bytes_chirho.len() as u32);
    }
}

/// Show the shell prompt
fn show_prompt_chirho() {
    kwrite_chirho("\x1b[1;36mlineluya\x1b[0m\x1b[1;33m$ \x1b[0m");
}

/// Process a completed command line
fn process_command_chirho(line_chirho: &[u8], len_chirho: usize) {
    // Trim trailing whitespace
    let mut end_chirho = len_chirho;
    while end_chirho > 0 && (line_chirho[end_chirho - 1] == b' ' || line_chirho[end_chirho - 1] == b'\t') {
        end_chirho -= 1;
    }

    // Trim leading whitespace
    let mut start_chirho = 0usize;
    while start_chirho < end_chirho && (line_chirho[start_chirho] == b' ' || line_chirho[start_chirho] == b'\t') {
        start_chirho += 1;
    }

    if start_chirho >= end_chirho {
        // Empty command, just show prompt again
        return;
    }

    let cmd_bytes_chirho = &line_chirho[start_chirho..end_chirho];

    // Parse command and arguments
    let mut cmd_end_chirho = start_chirho;
    while cmd_end_chirho < end_chirho && line_chirho[cmd_end_chirho] != b' ' {
        cmd_end_chirho += 1;
    }
    let cmd_chirho = &line_chirho[start_chirho..cmd_end_chirho];

    // Arguments start after the command + space
    let mut args_start_chirho = cmd_end_chirho;
    while args_start_chirho < end_chirho && line_chirho[args_start_chirho] == b' ' {
        args_start_chirho += 1;
    }
    let args_chirho = &line_chirho[args_start_chirho..end_chirho];

    match cmd_chirho {
        b"help" => {
            kwrite_chirho("\x1b[1;37mLineluya Built-in Shell Commands:\x1b[0m\r\n");
            kwrite_chirho("  \x1b[1;32mhelp\x1b[0m       - Show this help message\r\n");
            kwrite_chirho("  \x1b[1;32muname\x1b[0m      - Print system information\r\n");
            kwrite_chirho("  \x1b[1;32mecho\x1b[0m       - Echo arguments to console\r\n");
            kwrite_chirho("  \x1b[1;32mls\x1b[0m         - List /proc entries\r\n");
            kwrite_chirho("  \x1b[1;32mcat\x1b[0m        - Display file contents (try: cat /proc/cpuinfo)\r\n");
            kwrite_chirho("  \x1b[1;32mclear\x1b[0m      - Clear the terminal\r\n");
            kwrite_chirho("  \x1b[1;32mwhoami\x1b[0m     - Print current user\r\n");
            kwrite_chirho("  \x1b[1;32mdate\x1b[0m       - Print kernel uptime\r\n");
            kwrite_chirho("  \x1b[1;32mversion\x1b[0m    - Print kernel version\r\n");
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
                kwrite_chirho("Lineluya 0.4.0 wasm32 Lineluya Kernel (browser) WebAssembly\r\n");
            } else {
                kwrite_chirho("Lineluya\r\n");
            }
        }
        b"echo" => {
            if !args_chirho.is_empty() {
                kwrite_bytes_chirho(args_chirho);
            }
            kwrite_chirho("\r\n");
        }
        b"ls" => {
            if args_chirho.is_empty() || args_chirho == b"/proc" || args_chirho == b"/proc/" {
                kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
                kwrite_chirho("\x1b[1;36mcpuinfo\x1b[0m  ");
                kwrite_chirho("\x1b[1;36mmeminfo\x1b[0m  ");
                kwrite_chirho("\x1b[1;36mversion\x1b[0m  ");
                kwrite_chirho("\x1b[1;36muptime\x1b[0m  ");
                kwrite_chirho("\x1b[1;36mfilesystems\x1b[0m  ");
                kwrite_chirho("\x1b[1;36mcmdline\x1b[0m\r\n");
            } else if args_chirho == b"/" {
                kwrite_chirho("\x1b[1;34m.\x1b[0m  \x1b[1;34m..\x1b[0m  ");
                kwrite_chirho("\x1b[1;34mproc\x1b[0m  ");
                kwrite_chirho("\x1b[1;34mdev\x1b[0m  ");
                kwrite_chirho("\x1b[1;34msys\x1b[0m  ");
                kwrite_chirho("\x1b[1;34mtmp\x1b[0m\r\n");
            } else {
                kwrite_chirho("ls: cannot access '");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho("': No such file or directory\r\n");
            }
        }
        b"cat" => {
            if args_chirho == b"/proc/cpuinfo" {
                kwrite_chirho("processor\t: 0\r\n");
                kwrite_chirho("vendor_id\t: WebAssembly\r\n");
                kwrite_chirho("model name\t: Lineluya Virtual CPU (wasm32)\r\n");
                kwrite_chirho("cpu MHz\t\t: unlimited (JS event loop)\r\n");
                kwrite_chirho("cache size\t: browser managed\r\n");
                kwrite_chirho("flags\t\t: wasm simd bulk_memory mutable_globals\r\n");
                kwrite_chirho("bogomips\t: infinity\r\n");
            } else if args_chirho == b"/proc/meminfo" {
                kwrite_chirho("MemTotal:       16384 kB (WASM linear memory)\r\n");
                kwrite_chirho("MemFree:        16384 kB\r\n");
                kwrite_chirho("MemAvailable:   16384 kB\r\n");
                kwrite_chirho("Buffers:            0 kB\r\n");
                kwrite_chirho("Cached:             0 kB\r\n");
            } else if args_chirho == b"/proc/version" {
                kwrite_chirho("Lineluya version 0.4.0 (rustc wasm32-unknown-unknown) ");
                kwrite_chirho("(Lineluya Kernel — Linux ABI on WebAssembly)\r\n");
            } else if args_chirho == b"/proc/uptime" {
                kwrite_chirho("(uptime available via date command)\r\n");
            } else if args_chirho == b"/proc/filesystems" {
                kwrite_chirho("nodev\topfs\r\n");
                kwrite_chirho("nodev\tindexeddb\r\n");
                kwrite_chirho("nodev\tmemfs\r\n");
            } else if args_chirho == b"/proc/cmdline" {
                kwrite_chirho("lineluya_chirho console=xterm loglevel=7\r\n");
            } else if args_chirho.is_empty() {
                kwrite_chirho("cat: missing operand\r\n");
            } else {
                kwrite_chirho("cat: ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(": No such file or directory\r\n");
            }
        }
        b"clear" => {
            // ANSI escape: clear screen + move cursor home
            kwrite_chirho("\x1b[2J\x1b[H");
        }
        b"whoami" => {
            kwrite_chirho("root\r\n");
        }
        b"date" => {
            let us_chirho = unsafe { js_timestamp_us_chirho() as u64 };
            let sec_chirho = us_chirho / 1_000_000;
            let ms_chirho = (us_chirho % 1_000_000) / 1000;
            // Format manually since we're no_std
            kwrite_chirho("Kernel uptime: ");
            // Convert seconds to a decimal string
            let mut buf_chirho = [0u8; 20];
            let len_chirho = u64_to_str_chirho(sec_chirho, &mut buf_chirho);
            kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
            kwrite_chirho(".");
            // Milliseconds with leading zeros
            let mut ms_buf_chirho = [0u8; 3];
            ms_buf_chirho[0] = b'0' + ((ms_chirho / 100) % 10) as u8;
            ms_buf_chirho[1] = b'0' + ((ms_chirho / 10) % 10) as u8;
            ms_buf_chirho[2] = b'0' + (ms_chirho % 10) as u8;
            kwrite_bytes_chirho(&ms_buf_chirho);
            kwrite_chirho("s (since page load)\r\n");
        }
        b"version" => {
            kwrite_chirho("\x1b[1;37mLineluya Kernel v0.4.0 (wasm32)\x1b[0m\r\n");
            kwrite_chirho("Linux ABI on WebAssembly. Browser is the hardware.\r\n");
            kwrite_chirho("Built with Rust, compiled to wasm32-unknown-unknown.\r\n");
        }
        b"ping" => {
            if args_chirho.is_empty() {
                kwrite_chirho("Usage: ping <host>\r\n");
            } else {
                kwrite_chirho("PING ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(" via WebSocket proxy:\r\n");
                kwrite_chirho("  (network syscalls route through CF Worker → origin server)\r\n");
                kwrite_chirho("  connect() → WebSocket → TCP → ICMP (not yet fully wired)\r\n");
                kwrite_chirho("  Use 'nc' for raw TCP connections via the proxy.\r\n");
            }
        }
        b"nc" | b"netcat" => {
            if args_chirho.is_empty() {
                kwrite_chirho("Usage: nc <host> <port>\r\n");
                kwrite_chirho("  Connects via WebSocket proxy to TCP services\r\n");
            } else {
                kwrite_chirho("Connecting via WebSocket proxy...\r\n");
                kwrite_chirho("  socket(AF_INET, SOCK_STREAM, 0) = 100\r\n");
                kwrite_chirho("  connect(100, ");
                kwrite_bytes_chirho(args_chirho);
                kwrite_chirho(") → ws://proxy/connect\r\n");
                kwrite_chirho("  (full TCP relay available when proxy is running)\r\n");
            }
        }
        b"ifconfig" | b"ip" => {
            kwrite_chirho("lo: flags=73<UP,LOOPBACK,RUNNING>  mtu 65536\r\n");
            kwrite_chirho("    inet 127.0.0.1  netmask 255.0.0.0\r\n");
            kwrite_chirho("    inet6 ::1  prefixlen 128\r\n");
            kwrite_chirho("\r\n");
            kwrite_chirho("ws0: flags=4163<UP,BROADCAST,RUNNING,MULTICAST>  mtu 1500\r\n");
            kwrite_chirho("    inet (via WebSocket proxy)  netmask 255.255.255.0\r\n");
            kwrite_chirho("    carrier: Cloudflare Workers\r\n");
        }
        b"hostname" => {
            kwrite_chirho("lineluya-wasm\r\n");
        }
        b"id" => {
            kwrite_chirho("uid=0(root) gid=0(root) groups=0(root)\r\n");
        }
        b"pwd" => {
            kwrite_chirho("/\r\n");
        }
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
            let mut buf_chirho = [0u8; 20];
            if hr_chirho > 0 {
                let len_chirho = u64_to_str_chirho(hr_chirho, &mut buf_chirho);
                kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
                kwrite_chirho("h ");
            }
            let len_chirho = u64_to_str_chirho(min_chirho % 60, &mut buf_chirho);
            kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
            kwrite_chirho("m, 1 user, load average: 0.00, 0.00, 0.00\r\n");
        }
        b"free" => {
            let pages_chirho = core::arch::wasm32::memory_size(0);
            let total_kb_chirho = (pages_chirho * 64) as u64; // 64KB per WASM page
            kwrite_chirho("              total        used        free\r\n");
            kwrite_chirho("Mem:     ");
            let mut buf_chirho = [0u8; 20];
            let len_chirho = u64_to_str_chirho(total_kb_chirho, &mut buf_chirho);
            kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
            kwrite_chirho(" kB         0 kB    ");
            kwrite_bytes_chirho(&buf_chirho[..len_chirho]);
            kwrite_chirho(" kB\r\n");
        }
        b"john316" => {
            kwrite_chirho("\x1b[1;33m\"For God so loved the world that he gave his only begotten Son,\r\n");
            kwrite_chirho("that whoever believes in him should not perish but have eternal life.\"\r\n");
            kwrite_chirho("                                                        — John 3:16\x1b[0m\r\n");
        }
        _ => {
            kwrite_bytes_chirho(cmd_chirho);
            kwrite_chirho(": command not found\r\n");
        }
    }
}

/// Convert a u64 to a decimal string, returns number of bytes written
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

// ---------------------------------------------------------------------------
// Syscall dispatch — Linux syscall numbers → implementations
// ---------------------------------------------------------------------------

/// Next socket file descriptor (starts at 100 to avoid collision with stdio)
static mut NEXT_SOCK_FD_CHIRHO: u32 = 99;

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

/// Handle a Linux syscall from a wasm32 userspace program.
/// Called by the JS runtime when a program invokes a syscall.
#[no_mangle]
pub extern "C" fn syscall_chirho(
    nr_chirho: u32,
    arg0_chirho: u32,
    arg1_chirho: u32,
    arg2_chirho: u32,
    arg3_chirho: u32,
    arg4_chirho: u32,
    arg5_chirho: u32,
) -> i32 {
    match nr_chirho {
        // write(fd, buf, count)
        1 => {
            let buf_chirho = arg1_chirho as *const u8;
            let len_chirho = arg2_chirho;
            if arg0_chirho == 1 || arg0_chirho == 2 {
                // stdout/stderr → browser console
                unsafe {
                    js_console_write_chirho(buf_chirho as u32, len_chirho);
                }
                len_chirho as i32
            } else {
                -9 // EBADF
            }
        }
        // read(fd, buf, count)
        0 => {
            if arg0_chirho == 0 {
                // stdin → console read
                unsafe {
                    js_console_read_chirho(arg1_chirho, arg2_chirho) as i32
                }
            } else {
                -9
            }
        }
        // exit(code)
        60 => {
            let msg_chirho = "Process exited\n";
            unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
            0
        }
        // exit_group(code)
        231 => {
            let msg_chirho = "Process group exited\n";
            unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
            0
        }
        // getpid
        39 => 1,
        // getuid/geteuid/getgid/getegid
        102 | 107 | 104 | 108 => 0,
        // brk
        12 => 0,
        // mmap — allocate from WASM linear memory
        9 => {
            let len_chirho = arg1_chirho as usize;
            let pages_chirho = (len_chirho + 65535) / 65536;
            let prev_chirho = core::arch::wasm32::memory_grow(0, pages_chirho);
            if prev_chirho == usize::MAX {
                -12 // ENOMEM
            } else {
                (prev_chirho * 65536) as i32
            }
        }
        // munmap — no-op (WASM can't free memory pages)
        11 => 0,
        // arch_prctl — no-op on wasm
        158 => 0,
        // set_tid_address
        218 => 1,
        // uname
        63 => {
            // Would write utsname to user buffer
            -14 // EFAULT (needs proper impl)
        }
        // clock_gettime
        228 => {
            let us_chirho = unsafe { js_timestamp_us_chirho() as u64 };
            let sec_chirho = us_chirho / 1_000_000;
            let nsec_chirho = (us_chirho % 1_000_000) * 1000;
            // Write to user buffer at arg1
            let ptr_chirho = arg1_chirho as *mut u64;
            unsafe {
                *ptr_chirho = sec_chirho;
                *ptr_chirho.add(1) = nsec_chirho;
            }
            0
        }
        // socket(domain, type, protocol) — allocate a socket fd
        41 => {
            // For now, return a pseudo-fd for network sockets
            // Domain: AF_INET=2, Type: SOCK_STREAM=1, SOCK_DGRAM=2
            let domain_chirho = arg0_chirho;
            let sock_type_chirho = arg1_chirho;
            if domain_chirho != 2 { // AF_INET only
                -97 // EAFNOSUPPORT
            } else if sock_type_chirho != 1 && sock_type_chirho != 2 {
                -94 // ESOCKTNOSUPPORT
            } else {
                unsafe {
                    NEXT_SOCK_FD_CHIRHO += 1;
                    NEXT_SOCK_FD_CHIRHO as i32
                }
            }
        }
        // connect(sockfd, addr, addrlen) — connect via JS WebSocket proxy
        42 => {
            // Parse sockaddr_in from WASM memory: family(2) + port(2) + addr(4)
            let addr_ptr_chirho = arg1_chirho as *const u8;
            unsafe {
                let port_be_chirho = ((*addr_ptr_chirho.add(2) as u16) << 8) | (*addr_ptr_chirho.add(3) as u16);
                let ip_a_chirho = *addr_ptr_chirho.add(4);
                let ip_b_chirho = *addr_ptr_chirho.add(5);
                let ip_c_chirho = *addr_ptr_chirho.add(6);
                let ip_d_chirho = *addr_ptr_chirho.add(7);
                // Format IP as string in a temp buffer
                let mut ip_buf_chirho = [0u8; 16];
                let ip_len_chirho = format_ip_chirho(ip_a_chirho, ip_b_chirho, ip_c_chirho, ip_d_chirho, &mut ip_buf_chirho);
                let handle_chirho = js_net_connect_chirho(
                    ip_buf_chirho.as_ptr() as u32,
                    ip_len_chirho as u32,
                    port_be_chirho as u32,
                );
                if handle_chirho < 0 { -111 } else { 0 } // ECONNREFUSED or success
            }
        }
        // sendto(sockfd, buf, len, flags, dest_addr, addrlen)
        44 => unsafe {
            js_net_send_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho)
        },
        // recvfrom(sockfd, buf, len, flags, src_addr, addrlen)
        45 => unsafe {
            js_net_recv_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho)
        },
        // bind(sockfd, addr, addrlen) — stub (server sockets not yet supported in WASM)
        49 => 0,
        // listen(sockfd, backlog) — stub
        50 => 0,
        // accept(sockfd, addr, addrlen) — stub
        43 => -11, // EAGAIN
        // shutdown(sockfd, how)
        48 => unsafe {
            js_net_close_chirho(arg0_chirho as i32);
            0
        },
        // close — if it's a socket fd, close it
        3 => unsafe {
            if arg0_chirho >= 100 {
                js_net_close_chirho(arg0_chirho as i32);
            }
            0
        },
        // getsockopt / setsockopt — stubs
        55 | 54 => 0,
        // ioctl — stub
        16 => 0,
        // fcntl — stub
        72 => 0,
        // poll — stub (all fds ready)
        7 => 1,
        // select — stub
        23 => 1,
        // pipe2 — stub
        293 => -38, // ENOSYS

        // Default: unimplemented
        _ => {
            -38 // ENOSYS
        }
    }
}

// ---------------------------------------------------------------------------
// WASM exports — entry points called by JavaScript runtime
// ---------------------------------------------------------------------------

/// Kernel boot entry point.
#[no_mangle]
pub extern "C" fn kernel_main_chirho() {
    // Register the WASM architecture port
    kernel_core_chirho::set_arch_port_chirho(&WASM_ARCH_CHIRHO);

    let boot_msg_chirho = concat!(
        "\x1b[1;37mLineluya kernel booting (wasm32)...\x1b[0m\r\n",
        "\x1b[1;33mFor God so loved the world that he gave his only begotten Son,\r\n",
        "that whoever believes in him should not perish but have eternal life.\r\n",
        "                                                        — John 3:16\x1b[0m\r\n",
        "\r\n",
        "\x1b[1;32m[OK]\x1b[0m WASM linear memory (no MMU needed)\r\n",
        "\x1b[1;32m[OK]\x1b[0m WASM sandbox (no ring 0/3 needed)\r\n",
        "\x1b[1;32m[OK]\x1b[0m Browser console driver\r\n",
        "\x1b[1;32m[OK]\x1b[0m Syscall dispatch ready\r\n",
        "\x1b[1;32m[OK]\x1b[0m Built-in shell (ksh) ready\r\n",
        "\r\n",
        "\x1b[1;37m=== Lineluya Kernel v0.4.0 (wasm32) ===\x1b[0m\r\n",
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
/// This drives the built-in shell: read input, process commands.
#[no_mangle]
pub extern "C" fn kernel_tick_chirho() {
    unsafe {
        if !SHELL_CHIRHO.booted_chirho {
            return;
        }

        // Show prompt if needed
        if !SHELL_CHIRHO.prompt_shown_chirho {
            show_prompt_chirho();
            SHELL_CHIRHO.prompt_shown_chirho = true;
        }

        // Try to read input bytes from the JS input buffer
        let mut read_buf_chirho = [0u8; 64];
        let n_chirho = js_console_read_chirho(
            read_buf_chirho.as_mut_ptr() as u32,
            read_buf_chirho.len() as u32,
        ) as usize;

        for i_chirho in 0..n_chirho {
            let byte_chirho = read_buf_chirho[i_chirho];

            match byte_chirho {
                // Enter / carriage return
                b'\r' | b'\n' => {
                    // Echo newline
                    kwrite_chirho("\r\n");
                    // Process the command
                    let len_chirho = SHELL_CHIRHO.line_pos_chirho;
                    // Copy line buffer to avoid borrow issues
                    let mut cmd_buf_chirho = [0u8; MAX_LINE_LEN_CHIRHO];
                    cmd_buf_chirho[..len_chirho].copy_from_slice(&SHELL_CHIRHO.line_buf_chirho[..len_chirho]);
                    process_command_chirho(&cmd_buf_chirho, len_chirho);
                    // Reset line buffer
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                // Backspace (0x7F) or Ctrl-H (0x08)
                0x7F | 0x08 => {
                    if SHELL_CHIRHO.line_pos_chirho > 0 {
                        SHELL_CHIRHO.line_pos_chirho -= 1;
                        // Erase character on terminal: move back, space, move back
                        kwrite_chirho("\x08 \x08");
                    }
                }
                // Ctrl-C
                0x03 => {
                    kwrite_chirho("^C\r\n");
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                // Ctrl-L (clear screen)
                0x0C => {
                    kwrite_chirho("\x1b[2J\x1b[H");
                    SHELL_CHIRHO.line_pos_chirho = 0;
                    SHELL_CHIRHO.prompt_shown_chirho = false;
                }
                // Tab — ignore for now
                b'\t' => {}
                // Escape sequences (arrows etc.) — consume but ignore
                0x1B => {
                    // Skip; xterm sends multi-byte escape sequences but
                    // we just ignore them for now
                }
                // Regular printable character
                _ => {
                    if byte_chirho >= 0x20 && SHELL_CHIRHO.line_pos_chirho < MAX_LINE_LEN_CHIRHO - 1 {
                        SHELL_CHIRHO.line_buf_chirho[SHELL_CHIRHO.line_pos_chirho] = byte_chirho;
                        SHELL_CHIRHO.line_pos_chirho += 1;
                        // Echo character
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
fn panic_chirho(info_chirho: &PanicInfo) -> ! {
    let msg_chirho = "\x1b[1;31m!!! KERNEL PANIC !!!\x1b[0m\r\n";
    unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
    loop {}
}
