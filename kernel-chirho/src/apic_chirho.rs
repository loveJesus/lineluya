// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Local APIC and I/O APIC stubs for the Lineluya kernel (Phase 8).
//!
//! Provides foundational structures and register-level accessors for the
//! x86-64 Advanced Programmable Interrupt Controller.  Actual hardware
//! programming is deferred until the ACPI tables have been parsed and the
//! APIC base address confirmed.

use core::ptr;

// ============================================================================
// Local APIC register offsets
// ============================================================================

/// Local APIC ID register offset.
#[allow(dead_code)]
pub const APIC_ID_CHIRHO: u32 = 0x020;

/// Local APIC Version register offset.
#[allow(dead_code)]
pub const APIC_VERSION_CHIRHO: u32 = 0x030;

/// Task Priority Register offset.
#[allow(dead_code)]
pub const APIC_TPR_CHIRHO: u32 = 0x080;

/// End-Of-Interrupt register offset.
#[allow(dead_code)]
pub const APIC_EOI_CHIRHO: u32 = 0x0B0;

/// Spurious Interrupt Vector Register offset.
#[allow(dead_code)]
pub const APIC_SVR_CHIRHO: u32 = 0x0F0;

/// Interrupt Command Register (low 32 bits) offset.
#[allow(dead_code)]
pub const APIC_ICR_LOW_CHIRHO: u32 = 0x300;

/// Interrupt Command Register (high 32 bits) offset.
#[allow(dead_code)]
pub const APIC_ICR_HIGH_CHIRHO: u32 = 0x310;

/// LVT Timer register offset.
#[allow(dead_code)]
pub const APIC_TIMER_LVT_CHIRHO: u32 = 0x320;

// ============================================================================
// Default base addresses
// ============================================================================

/// Default MMIO base address for the Local APIC.
#[allow(dead_code)]
const LOCAL_APIC_DEFAULT_BASE_CHIRHO: u64 = 0xFEE0_0000;

/// Default MMIO base address for the I/O APIC.
#[allow(dead_code)]
const IOAPIC_DEFAULT_BASE_CHIRHO: u64 = 0xFEC0_0000;

// ============================================================================
// Local APIC
// ============================================================================

/// Represents the local APIC for the current CPU.
#[allow(dead_code)]
pub struct LocalApicChirho {
    /// Virtual (or identity-mapped) base address of the APIC MMIO region.
    base_address_chirho: u64,
}

#[allow(dead_code)]
impl LocalApicChirho {
    /// Create a new `LocalApicChirho` with the default MMIO base.
    pub const fn new_chirho() -> Self {
        Self {
            base_address_chirho: LOCAL_APIC_DEFAULT_BASE_CHIRHO,
        }
    }

    /// Create a `LocalApicChirho` with a custom MMIO base address.
    pub const fn with_base_chirho(base_chirho: u64) -> Self {
        Self {
            base_address_chirho: base_chirho,
        }
    }

    /// Read a 32-bit register at the given offset from the APIC base.
    ///
    /// # Safety
    /// The caller must ensure the base address is correctly mapped and the
    /// offset corresponds to a valid APIC register.
    pub unsafe fn read_register_chirho(&self, offset_chirho: u32) -> u32 {
        let addr_chirho = self.base_address_chirho + offset_chirho as u64;
        unsafe { ptr::read_volatile(addr_chirho as *const u32) }
    }

    /// Write a 32-bit value to the register at the given offset.
    ///
    /// # Safety
    /// The caller must ensure the base address is correctly mapped and the
    /// offset corresponds to a valid APIC register.
    pub unsafe fn write_register_chirho(&self, offset_chirho: u32, value_chirho: u32) {
        let addr_chirho = self.base_address_chirho + offset_chirho as u64;
        unsafe { ptr::write_volatile(addr_chirho as *mut u32, value_chirho) }
    }

    /// Send an End-Of-Interrupt signal to the local APIC.
    ///
    /// # Safety
    /// Must only be called from an interrupt handler context with a valid
    /// APIC base mapping.
    pub unsafe fn send_eoi_chirho(&self) {
        unsafe { self.write_register_chirho(APIC_EOI_CHIRHO, 0) }
    }
}

/// Detect and initialise the local APIC.
///
/// Stub: logs a message and returns.  Full initialisation will enable the
/// APIC via the SVR, set the TPR, and configure the LVT timer once ACPI
/// parsing is available.
#[allow(dead_code)]
pub fn init_local_apic_chirho() {
    crate::serial_println_chirho!("[STUB] Local APIC init (base {:#X})", LOCAL_APIC_DEFAULT_BASE_CHIRHO);
}

// ============================================================================
// I/O APIC
// ============================================================================

/// Represents an I/O APIC (typically one per chipset).
#[allow(dead_code)]
pub struct IoApicChirho {
    /// MMIO base address of the I/O APIC registers.
    base_address_chirho: u64,
}

#[allow(dead_code)]
impl IoApicChirho {
    /// Create a new `IoApicChirho` with the default base address.
    pub const fn new_chirho() -> Self {
        Self {
            base_address_chirho: IOAPIC_DEFAULT_BASE_CHIRHO,
        }
    }

    /// Create an `IoApicChirho` with a custom base address.
    pub const fn with_base_chirho(base_chirho: u64) -> Self {
        Self {
            base_address_chirho: base_chirho,
        }
    }
}

/// Detect and initialise the I/O APIC.
///
/// Stub: logs a message and returns.  Full initialisation will read the
/// MADT to discover I/O APIC addresses, then program redirection entries.
#[allow(dead_code)]
pub fn init_ioapic_chirho() {
    crate::serial_println_chirho!("[STUB] I/O APIC init (base {:#X})", IOAPIC_DEFAULT_BASE_CHIRHO);
}
