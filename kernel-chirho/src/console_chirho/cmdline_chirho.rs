// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel command line parsing for the Lineluya kernel (E1-014).
//!
//! Parses the `key=value` and boolean parameters from the kernel command
//! line string (passed by the bootloader or embedded in the bzImage).
//!
//! Supports wiring the command line from multiple boot protocols:
//! - `bootloader` crate (Rust bootloader)
//! - Multiboot2 (GRUB) via tag parsing
//! - Linux bzImage boot protocol via boot_params
//!
//! Common parameters:
//! - `root=/dev/sda1`  — root filesystem device
//! - `init=/sbin/init` — init process path
//! - `console=ttyS0`   — console device
//! - `loglevel=7`      — kernel log level
//! - `quiet`           — suppress boot messages
//! - `ro` / `rw`       — mount root read-only or read-write
//! - `panic=N`         — reboot after N seconds on panic

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Parsed parameter
// ============================================================================

/// A single parsed kernel command line parameter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CmdlineParamChirho {
    /// Parameter key (e.g. "root", "init", "console").
    pub key_chirho: String,
    /// Parameter value (empty string for boolean flags).
    pub value_chirho: String,
}

// ============================================================================
// Global command line storage
// ============================================================================

/// The raw command line string.
static CMDLINE_RAW_CHIRHO: Mutex<String> = Mutex::new(String::new());

/// Parsed command line parameters.
static CMDLINE_PARAMS_CHIRHO: Mutex<Vec<CmdlineParamChirho>> = Mutex::new(Vec::new());

/// Whether the command line has been initialized.
static CMDLINE_INITIALIZED_CHIRHO: Mutex<bool> = Mutex::new(false);

// ============================================================================
// Parsing
// ============================================================================

/// Parse a kernel command line string into key=value pairs.
///
/// Parameters are space-separated. A parameter without `=` is treated
/// as a boolean flag (value = "").
pub fn parse_cmdline_chirho(cmdline_chirho: &str) {
    let mut raw_chirho = CMDLINE_RAW_CHIRHO.lock();
    raw_chirho.clear();
    raw_chirho.push_str(cmdline_chirho);

    let mut params_chirho = CMDLINE_PARAMS_CHIRHO.lock();
    params_chirho.clear();

    for token_chirho in cmdline_chirho.split_whitespace() {
        if token_chirho.is_empty() {
            continue;
        }

        let (key_chirho, value_chirho) = if let Some(eq_pos_chirho) = token_chirho.find('=') {
            (
                &token_chirho[..eq_pos_chirho],
                &token_chirho[eq_pos_chirho + 1..],
            )
        } else {
            (token_chirho, "")
        };

        params_chirho.push(CmdlineParamChirho {
            key_chirho: String::from(key_chirho),
            value_chirho: String::from(value_chirho),
        });
    }

    let count_chirho = params_chirho.len();
    drop(params_chirho);
    drop(raw_chirho);

    let mut init_chirho = CMDLINE_INITIALIZED_CHIRHO.lock();
    *init_chirho = true;
    drop(init_chirho);

    crate::serial_println_chirho!(
        "[CMDLINE] Parsed {} parameters from: {}",
        count_chirho,
        cmdline_chirho
    );
}

/// Initialize command line from the `bootloader` crate's BootInfo.
///
/// The `bootloader` crate does not currently pass a command line,
/// so this sets a default. When Multiboot2 or bzImage provides one,
/// it will be parsed instead.
pub fn init_cmdline_from_bootinfo_chirho() {
    let already_init_chirho = { *CMDLINE_INITIALIZED_CHIRHO.lock() };
    if already_init_chirho {
        crate::serial_println_chirho!("[CMDLINE] Already initialized, skipping bootinfo init");
        return;
    }
    // Default command line when no bootloader provides one.
    parse_cmdline_chirho("console=ttyS0 loglevel=7");
    crate::serial_println_chirho!("[CMDLINE] Initialized with default command line");
}

/// Initialize command line from a raw C string pointer.
///
/// Used when booting via Linux bzImage protocol — the bootloader places
/// the command line at the physical address in `boot_params.hdr.cmd_line_ptr`.
///
/// # Safety
/// `ptr_chirho` must point to a valid NUL-terminated C string in mapped memory.
#[allow(dead_code)]
pub unsafe fn init_cmdline_from_ptr_chirho(ptr_chirho: *const u8) {
    if ptr_chirho.is_null() {
        crate::serial_println_chirho!("[CMDLINE] Null command line pointer, using default");
        parse_cmdline_chirho("console=ttyS0 loglevel=7");
        return;
    }

    // Find the NUL terminator (max 4096 bytes).
    let mut len_chirho: usize = 0;
    while len_chirho < 4096 {
        if unsafe { *ptr_chirho.add(len_chirho) } == 0 {
            break;
        }
        len_chirho += 1;
    }

    let bytes_chirho = unsafe { core::slice::from_raw_parts(ptr_chirho, len_chirho) };
    if let Ok(cmdline_str_chirho) = core::str::from_utf8(bytes_chirho) {
        crate::serial_println_chirho!(
            "[CMDLINE] From bootloader pointer: \"{}\"",
            cmdline_str_chirho
        );
        parse_cmdline_chirho(cmdline_str_chirho);
    } else {
        crate::serial_println_chirho!("[CMDLINE] Invalid UTF-8 in command line, using default");
        parse_cmdline_chirho("console=ttyS0 loglevel=7");
    }
}

/// Initialize command line from a Multiboot2 boot info structure (E1-014).
///
/// Parses the Multiboot2 tag list looking for the command line tag (type 1).
///
/// # Safety
/// `mbi_ptr_chirho` must point to a valid Multiboot2 boot information structure.
#[allow(dead_code)]
pub unsafe fn init_cmdline_from_multiboot2_chirho(mbi_ptr_chirho: *const u8) {
    if let Some(cmdline_chirho) =
        unsafe { crate::multiboot2_header_chirho::parse_mb2_cmdline_chirho(mbi_ptr_chirho) }
    {
        crate::serial_println_chirho!(
            "[CMDLINE] From Multiboot2: \"{}\"",
            cmdline_chirho
        );
        parse_cmdline_chirho(cmdline_chirho);
    } else {
        crate::serial_println_chirho!("[CMDLINE] No Multiboot2 command line tag, using default");
        parse_cmdline_chirho("console=ttyS0 loglevel=7");
    }
}

// ============================================================================
// Query functions
// ============================================================================

/// Look up a command line parameter by key.
///
/// Returns `Some(value)` if found, `None` if not present.
#[allow(dead_code)]
pub fn get_param_chirho(key_chirho: &str) -> Option<String> {
    let params_chirho = CMDLINE_PARAMS_CHIRHO.lock();
    for param_chirho in params_chirho.iter() {
        if param_chirho.key_chirho == key_chirho {
            return Some(param_chirho.value_chirho.clone());
        }
    }
    None
}

/// Check if a boolean flag is present on the command line.
#[allow(dead_code)]
pub fn has_flag_chirho(key_chirho: &str) -> bool {
    let params_chirho = CMDLINE_PARAMS_CHIRHO.lock();
    params_chirho.iter().any(|p_chirho| p_chirho.key_chirho == key_chirho)
}

/// Get the `root=` parameter value, or a default.
#[allow(dead_code)]
pub fn root_device_chirho() -> String {
    get_param_chirho("root").unwrap_or_else(|| String::from("/dev/sda1"))
}

/// Get the `init=` parameter value, or `/sbin/init`.
#[allow(dead_code)]
pub fn init_path_chirho() -> String {
    get_param_chirho("init").unwrap_or_else(|| String::from("/sbin/init"))
}

/// Get the `console=` parameter value.
#[allow(dead_code)]
pub fn console_device_chirho() -> Option<String> {
    get_param_chirho("console")
}

/// Get the `loglevel=` parameter as a u8, default 7.
#[allow(dead_code)]
pub fn log_level_chirho() -> u8 {
    get_param_chirho("loglevel")
        .and_then(|s_chirho| s_chirho.parse::<u8>().ok())
        .unwrap_or(7)
}

/// Return the raw command line string.
#[allow(dead_code)]
pub fn raw_cmdline_chirho() -> String {
    CMDLINE_RAW_CHIRHO.lock().clone()
}

/// Check if the `quiet` flag is set.
#[allow(dead_code)]
pub fn is_quiet_chirho() -> bool {
    has_flag_chirho("quiet")
}

/// Get the `panic=` timeout in seconds (0 = no auto-reboot).
#[allow(dead_code)]
pub fn panic_timeout_chirho() -> u64 {
    get_param_chirho("panic")
        .and_then(|s_chirho| s_chirho.parse::<u64>().ok())
        .unwrap_or(0)
}
