// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM timer — uses performance.now() via JS import.
//!
//! Replaces x86 PIT/APIC timer. In WASM, "interrupts" don't exist —
//! instead, the JS runtime calls `kernel_tick_wasm_chirho` on each
//! requestAnimationFrame (~60Hz) or setTimeout interval.

use core::sync::atomic::{AtomicU64, Ordering};

extern "C" {
    fn js_timestamp_us_chirho() -> u64;
}

/// Monotonic tick counter, incremented by the JS tick loop.
static TICK_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Boot timestamp in microseconds.
static BOOT_TIME_US_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Initialize the timer subsystem.
pub fn init_timer_chirho() {
    let now_chirho = unsafe { js_timestamp_us_chirho() };
    BOOT_TIME_US_CHIRHO.store(now_chirho, Ordering::Relaxed);
}

/// Called by JS on each tick (requestAnimationFrame).
pub fn tick_chirho() {
    TICK_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed);
}

/// Get current tick count.
pub fn ticks_chirho() -> u64 {
    TICK_COUNT_CHIRHO.load(Ordering::Relaxed)
}

/// Get uptime in microseconds.
pub fn uptime_us_chirho() -> u64 {
    let now_chirho = unsafe { js_timestamp_us_chirho() };
    let boot_chirho = BOOT_TIME_US_CHIRHO.load(Ordering::Relaxed);
    now_chirho.saturating_sub(boot_chirho)
}
