// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel command line parsing for the Lineluya kernel (A5-019).
//!
//! Parses the `key=value` and boolean parameters from the kernel command
//! line string (passed by the bootloader or embedded in the bzImage).
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

// ============================================================================
// Parsing
// ============================================================================

/// Parse a kernel command line string into key=value pairs.
///
/// Parameters are space-separated. A parameter without `=` is treated
/// as a boolean flag (value = "").
#[allow(dead_code)]
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

    crate::serial_println_chirho!(
        "[CMDLINE] Parsed {} parameters from: {}",
        params_chirho.len(),
        cmdline_chirho
    );
}

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
