// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Power management stub for the Lineluya kernel.
//!
//! Provides the `reboot(2)` syscall with basic RESTART and POWER_OFF
//! handling, and a placeholder `init_power_chirho` function.

// ============================================================================
// Reboot command constants (matching Linux <linux/reboot.h>)
// ============================================================================

/// Magic values required by reboot(2) for safety.
#[allow(dead_code)]
pub const LINUX_REBOOT_MAGIC1_CHIRHO: u64 = 0xfee1dead;
#[allow(dead_code)]
pub const LINUX_REBOOT_MAGIC2_CHIRHO: u64 = 672274793; // 0x28121969

/// Command: restart the system.
pub const LINUX_REBOOT_CMD_RESTART_CHIRHO: u64 = 0x01234567;
/// Command: power off the system.
pub const LINUX_REBOOT_CMD_POWER_OFF_CHIRHO: u64 = 0x4321FEDC;
/// Command: halt the system.
#[allow(dead_code)]
pub const LINUX_REBOOT_CMD_HALT_CHIRHO: u64 = 0xCDEF0123;

// ============================================================================
// Syscall implementation
// ============================================================================

/// `reboot(2)` implementation.
///
/// Handles RESTART and POWER_OFF commands by printing a message and halting
/// the CPU.  Other commands also halt (there is no real ACPI/power
/// controller yet).
///
/// # Arguments
/// * `_magic1_chirho` — must be `LINUX_REBOOT_MAGIC1`
/// * `_magic2_chirho` — must be `LINUX_REBOOT_MAGIC2`
/// * `cmd_chirho` — reboot command
/// * `_arg_chirho` — optional argument (unused)
///
/// # Returns
/// This function never returns (diverging `-> !`).
pub fn sys_reboot_real_chirho(
    _magic1_chirho: u64,
    _magic2_chirho: u64,
    cmd_chirho: u64,
    _arg_chirho: u64,
) -> ! {
    match cmd_chirho {
        LINUX_REBOOT_CMD_RESTART_CHIRHO => {
            crate::serial_println_chirho!("[POWER] Rebooting...");
        }
        LINUX_REBOOT_CMD_POWER_OFF_CHIRHO => {
            crate::serial_println_chirho!("[POWER] Power off");
        }
        _ => {
            crate::serial_println_chirho!(
                "[POWER] reboot cmd={:#x} -- halting",
                cmd_chirho
            );
        }
    }

    // Halt the CPU indefinitely.
    loop {
        x86_64::instructions::hlt();
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the power management subsystem (stub).
///
/// A real implementation would probe ACPI tables and register shutdown
/// handlers.
pub fn init_power_chirho() {
    crate::serial_println_chirho!("[POWER] Power management initialized (stub)");
}
