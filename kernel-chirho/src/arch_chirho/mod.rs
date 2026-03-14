// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Architecture abstraction layer for Lineluya.
//!
//! The kernel supports multiple architectures:
//! - **x86_64**: Bare metal / QEMU — traditional Linux kernel target
//! - **wasm32**: Browser / WASM runtime — runs in any modern browser
//!
//! Each architecture implements the [`ArchChirho`] trait which provides
//! hardware-specific primitives. Shared kernel code uses this trait
//! instead of calling architecture-specific functions directly.

#[cfg(target_arch = "x86_64")]
pub mod x86_64_chirho;

#[cfg(target_arch = "wasm32")]
pub mod wasm32_chirho;

/// Architecture abstraction trait.
///
/// Every target architecture implements this to provide the kernel with
/// hardware-specific primitives. The kernel's portable code calls these
/// instead of using inline assembly or architecture-specific crates.
pub trait ArchChirho {
    /// Initialize the architecture-specific hardware.
    fn init_chirho();

    /// Halt the CPU until the next interrupt (or yield in WASM).
    fn halt_chirho();

    /// Disable interrupts (no-op on WASM — single-threaded cooperative).
    fn disable_interrupts_chirho();

    /// Enable interrupts (no-op on WASM).
    fn enable_interrupts_chirho();

    /// Read the current timestamp counter (TSC on x86, performance.now on WASM).
    fn timestamp_chirho() -> u64;

    /// Write to the debug/serial console.
    fn debug_print_chirho(s: &str);
}
