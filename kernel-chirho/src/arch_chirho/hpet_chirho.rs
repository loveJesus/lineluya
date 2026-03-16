// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! HPET (High Precision Event Timer) and LAPIC timer subsystem for the
//! Lineluya kernel (A5-007 / A5-008).
//!
//! Provides:
//! - HPET MMIO register access and one-shot / periodic timer configuration
//! - LAPIC timer calibration using the HPET main counter
//! - TSC-based `clock_gettime` fast path
//!
//! Reference: Intel HPET Specification 1.0a, Intel SDM Vol 3 Ch 10.

use core::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// HPET MMIO register offsets
// ============================================================================

/// General Capabilities and ID Register.
#[allow(dead_code)]
const HPET_CAP_ID_CHIRHO: u64 = 0x000;
/// General Configuration Register.
#[allow(dead_code)]
const HPET_CONFIG_CHIRHO: u64 = 0x010;
/// General Interrupt Status Register.
#[allow(dead_code)]
const HPET_INT_STATUS_CHIRHO: u64 = 0x020;
/// Main Counter Value Register.
#[allow(dead_code)]
const HPET_COUNTER_CHIRHO: u64 = 0x0F0;
/// Timer N Configuration and Capabilities (N = 0..31).
#[allow(dead_code)]
const fn hpet_timer_config_chirho(n_chirho: u64) -> u64 {
    0x100 + 0x20 * n_chirho
}
/// Timer N Comparator Value (N = 0..31).
#[allow(dead_code)]
const fn hpet_timer_comparator_chirho(n_chirho: u64) -> u64 {
    0x108 + 0x20 * n_chirho
}

// ============================================================================
// HPET configuration bits
// ============================================================================

/// Enable bit in HPET General Configuration Register.
#[allow(dead_code)]
const HPET_ENABLE_BIT_CHIRHO: u64 = 1 << 0;
/// Legacy Replacement Route bit.
#[allow(dead_code)]
const HPET_LEGACY_REPLACE_CHIRHO: u64 = 1 << 1;

/// Timer configuration: enable interrupt.
#[allow(dead_code)]
const HPET_TN_INT_ENABLE_CHIRHO: u64 = 1 << 2;
/// Timer configuration: periodic mode.
#[allow(dead_code)]
const HPET_TN_PERIODIC_CHIRHO: u64 = 1 << 3;
/// Timer configuration: set accumulator (for periodic mode).
#[allow(dead_code)]
const HPET_TN_SET_VAL_CHIRHO: u64 = 1 << 6;
/// Timer configuration: 32-bit mode.
#[allow(dead_code)]
const HPET_TN_32BIT_CHIRHO: u64 = 1 << 8;

// ============================================================================
// LAPIC timer register offsets (from APIC base)
// ============================================================================

/// LAPIC Timer LVT register.
#[allow(dead_code)]
const LAPIC_TIMER_LVT_CHIRHO: u64 = 0x320;
/// LAPIC Timer Initial Count.
#[allow(dead_code)]
const LAPIC_TIMER_INIT_CHIRHO: u64 = 0x380;
/// LAPIC Timer Current Count.
#[allow(dead_code)]
const LAPIC_TIMER_CURRENT_CHIRHO: u64 = 0x390;
/// LAPIC Timer Divide Configuration.
#[allow(dead_code)]
const LAPIC_TIMER_DIVIDE_CHIRHO: u64 = 0x3E0;

/// LVT Timer mode: one-shot.
#[allow(dead_code)]
const LAPIC_TIMER_ONESHOT_CHIRHO: u32 = 0;
/// LVT Timer mode: periodic.
#[allow(dead_code)]
const LAPIC_TIMER_PERIODIC_CHIRHO: u32 = 1 << 17;
/// LVT Timer mode: TSC-deadline.
#[allow(dead_code)]
const LAPIC_TIMER_TSC_DEADLINE_CHIRHO: u32 = 2 << 17;
/// LVT mask bit (disables interrupt delivery).
#[allow(dead_code)]
const LAPIC_TIMER_MASK_CHIRHO: u32 = 1 << 16;

// ============================================================================
// Global timer state
// ============================================================================

/// HPET period in femtoseconds (from capabilities register).
/// Set during `init_hpet_chirho`. 0 = not yet initialized.
static HPET_PERIOD_FS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Virtual base address of the HPET MMIO registers.
/// Set during `init_hpet_chirho`.
static HPET_VIRT_BASE_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// LAPIC timer ticks per millisecond. Calibrated during boot.
static LAPIC_TICKS_PER_MS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Monotonic tick counter incremented by the timer interrupt handler.
static MONOTONIC_TICKS_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Default LAPIC base address.
#[allow(dead_code)]
const LAPIC_DEFAULT_BASE_CHIRHO: u64 = 0xFEE0_0000;

// ============================================================================
// HPET MMIO access helpers
// ============================================================================

/// Read a 64-bit register from the HPET MMIO region.
///
/// # Safety
/// The HPET base must be correctly mapped.
#[allow(dead_code)]
unsafe fn hpet_read_chirho(offset_chirho: u64) -> u64 {
    let base_chirho = HPET_VIRT_BASE_CHIRHO.load(Ordering::Relaxed);
    if base_chirho == 0 {
        return 0;
    }
    unsafe { core::ptr::read_volatile((base_chirho + offset_chirho) as *const u64) }
}

/// Write a 64-bit value to an HPET MMIO register.
///
/// # Safety
/// The HPET base must be correctly mapped.
#[allow(dead_code)]
unsafe fn hpet_write_chirho(offset_chirho: u64, value_chirho: u64) {
    let base_chirho = HPET_VIRT_BASE_CHIRHO.load(Ordering::Relaxed);
    if base_chirho == 0 {
        return;
    }
    unsafe { core::ptr::write_volatile((base_chirho + offset_chirho) as *mut u64, value_chirho) }
}

// ============================================================================
// HPET initialization
// ============================================================================

/// Initialize the HPET from the ACPI-discovered base address.
///
/// Reads the capabilities register, enables the main counter, and
/// optionally configures Timer 0 for periodic interrupts via legacy
/// replacement routing.
///
/// # Safety
/// Requires physical memory to be mapped.
#[allow(dead_code)]
pub unsafe fn init_hpet_chirho(phys_offset_chirho: u64) {
    let acpi_info_chirho = crate::acpi_chirho::ACPI_INFO_CHIRHO.lock();
    let hpet_base_phys_chirho = acpi_info_chirho.hpet_base_addr_chirho;

    if hpet_base_phys_chirho == 0 {
        crate::serial_println_chirho!("[HPET] No HPET found in ACPI tables");
        return;
    }

    let hpet_virt_chirho = phys_offset_chirho + hpet_base_phys_chirho;
    HPET_VIRT_BASE_CHIRHO.store(hpet_virt_chirho, Ordering::Relaxed);

    // Read capabilities: upper 32 bits contain period in femtoseconds
    let cap_chirho = unsafe { hpet_read_chirho(HPET_CAP_ID_CHIRHO) };
    let period_fs_chirho = cap_chirho >> 32;
    let num_timers_chirho = ((cap_chirho >> 8) & 0x1F) + 1;
    let rev_chirho = cap_chirho & 0xFF;
    let counter_64_chirho = (cap_chirho >> 13) & 1 != 0;

    HPET_PERIOD_FS_CHIRHO.store(period_fs_chirho, Ordering::Relaxed);

    crate::serial_println_chirho!(
        "[HPET] rev={} timers={} 64bit={} period={}fs ({} MHz)",
        rev_chirho,
        num_timers_chirho,
        counter_64_chirho,
        period_fs_chirho,
        1_000_000_000_000_000u64 / period_fs_chirho / 1_000_000
    );

    // Stop the main counter
    let config_chirho = unsafe { hpet_read_chirho(HPET_CONFIG_CHIRHO) };
    unsafe { hpet_write_chirho(HPET_CONFIG_CHIRHO, config_chirho & !HPET_ENABLE_BIT_CHIRHO) };

    // Reset the main counter to 0
    unsafe { hpet_write_chirho(HPET_COUNTER_CHIRHO, 0) };

    // Enable the main counter (and legacy replacement routing)
    unsafe {
        hpet_write_chirho(
            HPET_CONFIG_CHIRHO,
            HPET_ENABLE_BIT_CHIRHO | HPET_LEGACY_REPLACE_CHIRHO,
        )
    };

    crate::serial_println_chirho!("[HPET] Initialized and counter running");
}

/// Read the HPET main counter value.
#[allow(dead_code)]
pub fn hpet_counter_chirho() -> u64 {
    unsafe { hpet_read_chirho(HPET_COUNTER_CHIRHO) }
}

/// Convert HPET ticks to nanoseconds.
#[allow(dead_code)]
pub fn hpet_ticks_to_ns_chirho(ticks_chirho: u64) -> u64 {
    let period_fs_chirho = HPET_PERIOD_FS_CHIRHO.load(Ordering::Relaxed);
    if period_fs_chirho == 0 {
        return 0;
    }
    // period_fs is femtoseconds per tick; ns = ticks * period_fs / 1_000_000
    (ticks_chirho as u128 * period_fs_chirho as u128 / 1_000_000) as u64
}

// ============================================================================
// LAPIC timer calibration
// ============================================================================

/// Calibrate the LAPIC timer against the HPET main counter.
///
/// Sets LAPIC timer to count down from 0xFFFF_FFFF, waits ~10ms using
/// the HPET, then calculates ticks-per-millisecond.
///
/// # Safety
/// Requires HPET to be initialized and LAPIC mapped.
#[allow(dead_code)]
pub unsafe fn calibrate_lapic_timer_chirho(phys_offset_chirho: u64) {
    let period_fs_chirho = HPET_PERIOD_FS_CHIRHO.load(Ordering::Relaxed);
    if period_fs_chirho == 0 {
        crate::serial_println_chirho!("[LAPIC TIMER] Cannot calibrate: HPET not available");
        return;
    }

    let lapic_base_chirho = phys_offset_chirho + LAPIC_DEFAULT_BASE_CHIRHO;

    // Set divide value to 16
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_DIVIDE_CHIRHO) as *mut u32,
            0x03, // divide by 16
        );
    }

    // Set LVT timer to one-shot, masked, vector 32
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_LVT_CHIRHO) as *mut u32,
            LAPIC_TIMER_MASK_CHIRHO | 32,
        );
    }

    // Calculate how many HPET ticks = 10ms
    // period_fs is femtoseconds per tick
    // 10ms = 10_000_000 ns = 10_000_000_000_000 fs
    let target_fs_chirho: u64 = 10_000_000_000_000;
    let hpet_target_ticks_chirho = target_fs_chirho / period_fs_chirho;

    // Read HPET counter start
    let hpet_start_chirho = unsafe { hpet_read_chirho(HPET_COUNTER_CHIRHO) };

    // Start LAPIC timer counting down from max
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_INIT_CHIRHO) as *mut u32,
            0xFFFF_FFFF,
        );
    }

    // Busy-wait for HPET to advance by target ticks
    loop {
        let current_chirho = unsafe { hpet_read_chirho(HPET_COUNTER_CHIRHO) };
        if current_chirho.wrapping_sub(hpet_start_chirho) >= hpet_target_ticks_chirho {
            break;
        }
    }

    // Read LAPIC current count
    let lapic_current_chirho = unsafe {
        core::ptr::read_volatile(
            (lapic_base_chirho + LAPIC_TIMER_CURRENT_CHIRHO) as *const u32,
        )
    };
    let elapsed_ticks_chirho = 0xFFFF_FFFFu32 - lapic_current_chirho;
    let ticks_per_ms_chirho = (elapsed_ticks_chirho as u64) / 10;

    LAPIC_TICKS_PER_MS_CHIRHO.store(ticks_per_ms_chirho, Ordering::Relaxed);

    crate::serial_println_chirho!(
        "[LAPIC TIMER] Calibrated: {} ticks/ms ({} MHz bus, div16)",
        ticks_per_ms_chirho,
        ticks_per_ms_chirho * 16 / 1000
    );
}

/// Start the LAPIC timer in periodic mode at the given frequency (Hz).
///
/// # Safety
/// Requires LAPIC to be mapped and calibrated.
#[allow(dead_code)]
pub unsafe fn start_lapic_periodic_chirho(phys_offset_chirho: u64, hz_chirho: u32) {
    let ticks_per_ms_chirho = LAPIC_TICKS_PER_MS_CHIRHO.load(Ordering::Relaxed);
    if ticks_per_ms_chirho == 0 {
        crate::serial_println_chirho!("[LAPIC TIMER] Not calibrated");
        return;
    }

    let lapic_base_chirho = phys_offset_chirho + LAPIC_DEFAULT_BASE_CHIRHO;

    // Calculate initial count for desired frequency
    let interval_ms_chirho = 1000u64 / (hz_chirho as u64);
    let init_count_chirho = ticks_per_ms_chirho * interval_ms_chirho;

    // Set divide by 16
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_DIVIDE_CHIRHO) as *mut u32,
            0x03,
        );
    }

    // Set LVT: periodic mode, vector 32, unmasked
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_LVT_CHIRHO) as *mut u32,
            LAPIC_TIMER_PERIODIC_CHIRHO | 32,
        );
    }

    // Set initial count (starts the timer)
    unsafe {
        core::ptr::write_volatile(
            (lapic_base_chirho + LAPIC_TIMER_INIT_CHIRHO) as *mut u32,
            init_count_chirho as u32,
        );
    }

    crate::serial_println_chirho!(
        "[LAPIC TIMER] Started periodic @ {}Hz (init_count={})",
        hz_chirho,
        init_count_chirho
    );
}

// ============================================================================
// Monotonic clock
// ============================================================================

/// Increment the monotonic tick counter. Called from the timer interrupt.
#[allow(dead_code)]
pub fn tick_chirho() {
    MONOTONIC_TICKS_CHIRHO.fetch_add(1, Ordering::Relaxed);
}

/// Return the monotonic tick count.
#[allow(dead_code)]
pub fn monotonic_ticks_chirho() -> u64 {
    MONOTONIC_TICKS_CHIRHO.load(Ordering::Relaxed)
}

/// Return monotonic time in nanoseconds (approximate).
#[allow(dead_code)]
pub fn monotonic_ns_chirho() -> u64 {
    // If HPET is available, use it directly
    let hpet_base_chirho = HPET_VIRT_BASE_CHIRHO.load(Ordering::Relaxed);
    if hpet_base_chirho != 0 {
        let ticks_chirho = hpet_counter_chirho();
        return hpet_ticks_to_ns_chirho(ticks_chirho);
    }
    // Fallback: assume PIT at ~1000Hz (1ms per tick)
    MONOTONIC_TICKS_CHIRHO.load(Ordering::Relaxed) * 1_000_000
}
