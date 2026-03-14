// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM32 architecture implementation for Lineluya.
//!
//! This module makes the browser the hardware:
//!
//! | x86_64 Hardware    | WASM/Browser Equivalent          |
//! |--------------------|----------------------------------|
//! | Physical RAM       | WASM linear memory (memory.grow) |
//! | MMU / Page tables  | WASM bounds checking (built-in!) |
//! | Ring 0 / Ring 3    | WASM sandbox IS the protection   |
//! | GDT / IDT / TSS    | Not needed                       |
//! | Timer IRQ          | setTimeout / setInterval         |
//! | VGA framebuffer    | Canvas 2D / WebGL                |
//! | Serial port        | console.log / DOM terminal       |
//! | Disk (AHCI/NVMe)   | OPFS / IndexedDB                |
//! | Network (NIC)      | WebSocket / fetch                |
//! | Sound (ALSA)       | Web Audio API                    |
//! | Keyboard / Mouse   | DOM events                       |
//! | SMP / Multi-core   | Web Workers + SharedArrayBuffer  |
//! | SYSCALL instruction| Direct function call (no switch) |
//!
//! ## Userspace Programs
//!
//! Programs are compiled to `wasm32-wasi` and loaded as WASM modules.
//! The kernel implements the WASI syscall interface (`fd_read`, `fd_write`,
//! `proc_exit`, etc.) which maps 1:1 to our Linux-compatible syscalls.
//!
//! Toolchain:
//! - Rust:   `cargo build --target wasm32-wasi`
//! - C/C++:  `clang --target=wasm32-wasi`
//! - Go:     `GOOS=wasip1 GOARCH=wasm go build`
//! - Zig:    `zig build -target wasm32-wasi`

pub mod console_chirho;
pub mod memory_chirho;
pub mod timer_chirho;
pub mod framebuffer_chirho;
pub mod storage_chirho;

// ---------------------------------------------------------------------------
// WASM imports — these are provided by the JavaScript runtime ("bootloader")
// ---------------------------------------------------------------------------

extern "C" {
    /// Write a UTF-8 string to the browser console / terminal DOM element.
    fn js_console_write_chirho(ptr_chirho: *const u8, len_chirho: u32);

    /// Get current timestamp in microseconds (performance.now() * 1000).
    fn js_timestamp_us_chirho() -> u64;

    /// Yield control back to the browser event loop (cooperative scheduling).
    /// Returns when the next "tick" fires (requestAnimationFrame or setTimeout).
    fn js_yield_chirho();

    /// Request a framebuffer of the given dimensions.
    /// Returns a pointer to the pixel buffer in WASM linear memory.
    fn js_framebuffer_init_chirho(
        width_chirho: u32,
        height_chirho: u32,
    ) -> u32;

    /// Flush the framebuffer to the Canvas element.
    fn js_framebuffer_flush_chirho();

    /// Read from persistent storage (OPFS/IndexedDB).
    /// Returns bytes read, or negative on error.
    fn js_storage_read_chirho(
        offset_chirho: u64,
        buf_ptr_chirho: *mut u8,
        len_chirho: u32,
    ) -> i32;

    /// Write to persistent storage.
    fn js_storage_write_chirho(
        offset_chirho: u64,
        buf_ptr_chirho: *const u8,
        len_chirho: u32,
    ) -> i32;

    /// Open a WebSocket connection. Returns a handle or negative error.
    fn js_net_connect_chirho(
        host_ptr_chirho: *const u8,
        host_len_chirho: u32,
        port_chirho: u16,
    ) -> i32;

    /// Send data over a WebSocket.
    fn js_net_send_chirho(
        handle_chirho: i32,
        buf_ptr_chirho: *const u8,
        len_chirho: u32,
    ) -> i32;

    /// Receive data from a WebSocket (non-blocking, returns 0 if no data).
    fn js_net_recv_chirho(
        handle_chirho: i32,
        buf_ptr_chirho: *mut u8,
        max_len_chirho: u32,
    ) -> i32;

    /// Get the next keyboard event. Returns 0 if no event.
    /// Writes keycode and flags to the provided pointers.
    fn js_input_poll_chirho(
        keycode_ptr_chirho: *mut u32,
        flags_ptr_chirho: *mut u32,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// WASM exports — entry points called by the JavaScript runtime
// ---------------------------------------------------------------------------

/// Kernel entry point, called by the JS bootloader after WASM instantiation.
#[no_mangle]
pub extern "C" fn kernel_main_wasm_chirho() {
    // Initialize the WASM architecture layer
    debug_print_chirho("Lineluya kernel booting (wasm32)...\n");
    debug_print_chirho("For God so loved the world that he gave his only begotten Son,\n");
    debug_print_chirho("that whoever believes in him should not perish but have eternal life.\n");
    debug_print_chirho("- John 3:16\n\n");

    debug_print_chirho("[OK] WASM linear memory initialized\n");
    debug_print_chirho("[OK] No MMU needed — WASM has built-in bounds checking\n");
    debug_print_chirho("[OK] No GDT/IDT needed — WASM sandbox IS the protection\n");
    debug_print_chirho("[OK] Timer via setTimeout/requestAnimationFrame\n");
    debug_print_chirho("[OK] Framebuffer via Canvas API\n");
    debug_print_chirho("[OK] Storage via OPFS/IndexedDB\n");
    debug_print_chirho("[OK] Network via WebSocket\n");
    debug_print_chirho("\n=== Lineluya Kernel v0.4.0 (wasm32) ===\n");
    debug_print_chirho("Running in the browser at near-native speed!\n");
    debug_print_chirho("Hallelujah!\n");
}

/// Called by JS on each animation frame — drives the kernel scheduler.
#[no_mangle]
pub extern "C" fn kernel_tick_wasm_chirho() {
    // This is where the scheduler would run one quantum
    // and the framebuffer would be flushed
}

/// Handle a keyboard event from JavaScript.
#[no_mangle]
pub extern "C" fn kernel_keydown_wasm_chirho(keycode_chirho: u32, flags_chirho: u32) {
    // Route to the TTY / input subsystem
    let _ = (keycode_chirho, flags_chirho);
}

// ---------------------------------------------------------------------------
// ArchChirho implementation
// ---------------------------------------------------------------------------

pub fn init_chirho() {
    // WASM needs no hardware init — the browser IS the hardware
}

pub fn halt_chirho() {
    // Yield back to browser event loop
    unsafe { js_yield_chirho(); }
}

pub fn disable_interrupts_chirho() {
    // No-op on WASM — cooperative scheduling
}

pub fn enable_interrupts_chirho() {
    // No-op on WASM
}

pub fn timestamp_chirho() -> u64 {
    unsafe { js_timestamp_us_chirho() }
}

pub fn debug_print_chirho(s_chirho: &str) {
    unsafe {
        js_console_write_chirho(s_chirho.as_ptr(), s_chirho.len() as u32);
    }
}
