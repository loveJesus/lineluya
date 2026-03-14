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
// Syscall dispatch — Linux syscall numbers → implementations
// ---------------------------------------------------------------------------

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
        // socket — create via JS proxy
        41 => unsafe { js_net_connect_chirho(0, 0, 0) }, // placeholder
        // connect
        42 => {
            // Would parse sockaddr and connect via proxy
            -111 // ECONNREFUSED
        }
        // sendto
        44 => unsafe {
            js_net_send_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho)
        },
        // recvfrom
        45 => unsafe {
            js_net_recv_chirho(arg0_chirho as i32, arg1_chirho, arg2_chirho)
        },

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
        "Lineluya kernel booting (wasm32)...\n",
        "For God so loved the world that he gave his only begotten Son,\n",
        "that whoever believes in him should not perish but have eternal life.\n",
        "- John 3:16\n",
        "\n",
        "[OK] WASM linear memory (no MMU needed)\n",
        "[OK] WASM sandbox (no ring 0/3 needed)\n",
        "[OK] Browser console driver\n",
        "[OK] Syscall dispatch ready\n",
        "\n",
        "=== Lineluya Kernel v0.4.0 (wasm32) ===\n",
        "Linux ABI on WebAssembly. Browser is the hardware.\n",
        "\n",
    );
    unsafe { js_console_write_chirho(boot_msg_chirho.as_ptr() as u32, boot_msg_chirho.len() as u32); }
}

/// Scheduler tick — called by JS on requestAnimationFrame.
#[no_mangle]
pub extern "C" fn kernel_tick_chirho() {
    // Future: run scheduler, flush framebuffer
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
    let msg_chirho = "!!! KERNEL PANIC !!!\n";
    unsafe { js_console_write_chirho(msg_chirho.as_ptr() as u32, msg_chirho.len() as u32); }
    loop {}
}
