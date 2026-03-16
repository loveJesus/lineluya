// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Power management for the Lineluya kernel (E1-005).
//!
//! Provides ACPI S5 (soft power off) and system reboot via:
//! - ACPI PM1a control register (S5 sleep type)
//! - Keyboard controller reset (port 0x64, pulse CPU reset line)
//! - Triple fault (load null IDT and trigger interrupt)
//! - QEMU/Bochs debug exit (I/O port 0xf4)
//!
//! The `reboot(2)` syscall dispatches to these mechanisms.

use x86_64::instructions::port::Port;

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
// ACPI power off (S5 state) — E1-005
// ============================================================================

/// ACPI PM1 control register SLP_TYP shift (bits 10-12).
const SLP_TYP_SHIFT_CHIRHO: u16 = 10;

/// ACPI PM1 control register SLP_EN bit (bit 13) — triggers sleep.
const SLP_EN_BIT_CHIRHO: u16 = 1 << 13;

/// S5 sleep type value (common on PIIX4 / ICH chipsets and QEMU).
/// The actual value comes from the DSDT \_S5 object, but 5 is the
/// most common default for QEMU's PIIX4 PM and real hardware.
const S5_SLP_TYP_DEFAULT_CHIRHO: u16 = 5;

/// QEMU debug exit port — writing here causes QEMU to exit.
const QEMU_EXIT_PORT_CHIRHO: u16 = 0xf4;

/// Attempt ACPI S5 power off using the PM1a control block.
///
/// Reads the PM1a control block address from the ACPI subsystem.
/// If ACPI was not initialized, falls back to QEMU exit port.
fn acpi_poweroff_chirho() {
    // Try to get PM1a control block from ACPI info.
    let pm1a_cnt_chirho = {
        let acpi_info_chirho = crate::acpi_chirho::ACPI_INFO_CHIRHO.lock();
        acpi_info_chirho.pm1a_control_block_chirho
    };

    if pm1a_cnt_chirho != 0 {
        crate::serial_println_chirho!(
            "[POWER] ACPI S5 shutdown via PM1a at {:#06x}",
            pm1a_cnt_chirho
        );

        // Write SLP_TYP | SLP_EN to PM1a control register.
        let val_chirho = (S5_SLP_TYP_DEFAULT_CHIRHO << SLP_TYP_SHIFT_CHIRHO) | SLP_EN_BIT_CHIRHO;
        unsafe {
            let mut port_chirho: Port<u16> = Port::new(pm1a_cnt_chirho as u16);
            port_chirho.write(val_chirho);
        }

        // If we reach here, the chipset did not honor the sleep request.
        crate::serial_println_chirho!("[POWER] ACPI S5 did not take effect, trying QEMU exit...");
    } else {
        crate::serial_println_chirho!("[POWER] No ACPI PM1a block, trying QEMU exit port...");
    }

    // Fallback: QEMU debug exit.
    qemu_exit_chirho();
}

/// Exit QEMU via the debug I/O port (0xf4).
///
/// QEMU exits with code `(value << 1) | 1`, so writing 0 gives exit code 1.
fn qemu_exit_chirho() {
    crate::serial_println_chirho!("[POWER] Writing to QEMU exit port {:#06x}", QEMU_EXIT_PORT_CHIRHO);
    unsafe {
        let mut port_chirho: Port<u32> = Port::new(QEMU_EXIT_PORT_CHIRHO);
        port_chirho.write(0u32);
    }
}

// ============================================================================
// Reboot mechanisms — E1-005
// ============================================================================

/// PS/2 keyboard controller I/O port.
const KBC_CMD_PORT_CHIRHO: u16 = 0x64;
/// PS/2 keyboard controller data port.
const KBC_DATA_PORT_CHIRHO: u16 = 0x60;
/// Command to pulse the CPU reset line via keyboard controller.
const KBC_RESET_CMD_CHIRHO: u8 = 0xFE;
/// Keyboard controller status register — input buffer full bit.
const KBC_INPUT_FULL_CHIRHO: u8 = 0x02;

/// Reboot via the 8042 keyboard controller reset line.
///
/// Sends command 0xFE to the keyboard controller, which pulses the
/// CPU reset line, causing a warm reboot.
fn kbc_reboot_chirho() {
    crate::serial_println_chirho!("[POWER] Keyboard controller CPU reset (port 0x64, cmd 0xFE)");

    unsafe {
        let mut cmd_port_chirho: Port<u8> = Port::new(KBC_CMD_PORT_CHIRHO);
        let mut status_port_chirho: Port<u8> = Port::new(KBC_CMD_PORT_CHIRHO);

        // Wait for the keyboard controller input buffer to be empty.
        for _ in 0..10000 {
            if status_port_chirho.read() & KBC_INPUT_FULL_CHIRHO == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Pulse the CPU reset line.
        cmd_port_chirho.write(KBC_RESET_CMD_CHIRHO);
    }
}

/// Reboot by loading a null IDT and triggering a triple fault.
///
/// This is the ultimate fallback — works on any x86 system. Loading
/// a zero-length IDT and then triggering an interrupt causes a
/// double fault (no handler), then a triple fault, which resets the CPU.
fn triple_fault_reboot_chirho() {
    crate::serial_println_chirho!("[POWER] Triple fault reboot (null IDT)");

    unsafe {
        // Load a null IDT descriptor (base=0, limit=0).
        let null_idt_chirho: [u8; 10] = [0; 10];
        core::arch::asm!(
            "lidt [{}]",
            "int3",
            in(reg) null_idt_chirho.as_ptr(),
            options(noreturn)
        );
    }
}

// ============================================================================
// Syscall implementation
// ============================================================================

/// `reboot(2)` implementation.
///
/// Handles RESTART and POWER_OFF commands using real hardware mechanisms:
/// - POWER_OFF: ACPI S5 sleep -> QEMU exit -> halt
/// - RESTART:   keyboard controller reset -> triple fault -> halt
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

            // Disable interrupts before reset attempts.
            x86_64::instructions::interrupts::disable();

            // Try keyboard controller reset first (most compatible).
            kbc_reboot_chirho();

            // Wait a moment for the reset to take effect.
            for _ in 0..1_000_000 {
                core::hint::spin_loop();
            }

            // Fallback to triple fault.
            triple_fault_reboot_chirho();
        }
        LINUX_REBOOT_CMD_POWER_OFF_CHIRHO => {
            crate::serial_println_chirho!("[POWER] Power off...");

            // Disable interrupts.
            x86_64::instructions::interrupts::disable();

            // Try ACPI S5 shutdown.
            acpi_poweroff_chirho();

            // If we are still running, just halt.
            crate::serial_println_chirho!("[POWER] Shutdown failed, halting CPU.");
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

/// Initialize the power management subsystem.
///
/// Logs the available shutdown/reboot mechanisms based on ACPI state.
pub fn init_power_chirho() {
    let pm1a_chirho = {
        let acpi_info_chirho = crate::acpi_chirho::ACPI_INFO_CHIRHO.lock();
        acpi_info_chirho.pm1a_control_block_chirho
    };

    if pm1a_chirho != 0 {
        crate::serial_println_chirho!(
            "[POWER] ACPI S5 available (PM1a={:#06x}), KBC reset, triple fault",
            pm1a_chirho
        );
    } else {
        crate::serial_println_chirho!(
            "[POWER] No ACPI PM1a — using KBC reset / triple fault / QEMU exit"
        );
    }
}
