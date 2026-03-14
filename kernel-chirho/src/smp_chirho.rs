// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Symmetric Multi-Processing (SMP) stubs for the Lineluya kernel (Phase 8).
//!
//! Provides basic CPU information tracking and a boot-time initialisation
//! stub.  Full AP (Application Processor) start-up via INIT-SIPI-SIPI
//! will be implemented once the APIC and ACPI subsystems are functional.

use core::sync::atomic::{AtomicU32, Ordering};

// ============================================================================
// CPU bookkeeping
// ============================================================================

/// Number of CPUs detected (including the BSP).
#[allow(dead_code)]
pub static CPU_COUNT_CHIRHO: AtomicU32 = AtomicU32::new(1);

/// Describes a single logical CPU.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CpuInfoChirho {
    /// Kernel-assigned CPU index (0 = BSP).
    pub cpu_id_chirho: u32,
    /// Local APIC ID for this CPU.
    pub apic_id_chirho: u32,
    /// `true` if this is the Bootstrap Processor.
    pub is_bsp_chirho: bool,
}

// ============================================================================
// SMP initialisation
// ============================================================================

/// Initialise SMP subsystem.
///
/// Stub: logs that only the BSP is running.  A full implementation will
/// parse the MADT, prepare trampoline code, and send INIT/SIPI to each AP.
#[allow(dead_code)]
pub fn init_smp_chirho() {
    crate::serial_println_chirho!("SMP: BSP only (single-CPU mode)");
    CPU_COUNT_CHIRHO.store(1, Ordering::SeqCst);
}

/// Return the kernel CPU ID of the calling processor.
///
/// Stub: always returns 0 (BSP).  With full SMP support this would read
/// the local APIC ID and map it to a kernel CPU index.
#[allow(dead_code)]
pub fn cpu_id_chirho() -> u32 {
    0
}
