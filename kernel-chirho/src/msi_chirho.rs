// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! MSI (Message Signaled Interrupts) and MSI-X support for the Lineluya
//! kernel (A5-008).
//!
//! MSI/MSI-X replace legacy INTx pin-based interrupts with in-band PCI
//! messages. Each MSI vector writes a small message to a LAPIC address,
//! delivering an interrupt directly without IOAPIC involvement.
//!
//! Provides:
//! - MSI capability detection and configuration
//! - MSI-X capability detection and table access
//! - Interrupt vector allocation helpers
//!
//! Reference: PCI Local Bus Specification 3.0 Ch 6.8

// ============================================================================
// MSI capability structure (PCI config space)
// ============================================================================

/// MSI Message Address — always targets the LAPIC MMIO region.
/// Bits [31:20] = 0xFEE (fixed), [19:12] = destination APIC ID,
/// [11:4] = reserved, [3] = RH, [2] = DM.
#[allow(dead_code)]
const MSI_ADDRESS_BASE_CHIRHO: u32 = 0xFEE0_0000;

/// MSI capability offsets (relative to capability pointer).
#[allow(dead_code)]
const MSI_CAP_CONTROL_CHIRHO: u8 = 0x02; // Message Control (16-bit)
#[allow(dead_code)]
const MSI_CAP_ADDR_LO_CHIRHO: u8 = 0x04; // Message Address (32-bit)
#[allow(dead_code)]
const MSI_CAP_ADDR_HI_CHIRHO: u8 = 0x08; // Message Upper Address (if 64-bit capable)
#[allow(dead_code)]
const MSI_CAP_DATA_32_CHIRHO: u8 = 0x08; // Message Data (if 32-bit address)
#[allow(dead_code)]
const MSI_CAP_DATA_64_CHIRHO: u8 = 0x0C; // Message Data (if 64-bit address)

/// Message Control register bits.
#[allow(dead_code)]
const MSI_CTRL_ENABLE_CHIRHO: u16 = 1 << 0;
/// Multiple Message Capable (bits 3:1).
#[allow(dead_code)]
const MSI_CTRL_MMC_MASK_CHIRHO: u16 = 0x0E;
/// Multiple Message Enable (bits 6:4).
#[allow(dead_code)]
const MSI_CTRL_MME_MASK_CHIRHO: u16 = 0x70;
/// 64-bit address capable.
#[allow(dead_code)]
const MSI_CTRL_64BIT_CHIRHO: u16 = 1 << 7;
/// Per-vector masking capable.
#[allow(dead_code)]
const MSI_CTRL_PVM_CHIRHO: u16 = 1 << 8;

// ============================================================================
// MSI-X capability structure
// ============================================================================

/// MSI-X capability offsets.
#[allow(dead_code)]
const MSIX_CAP_CONTROL_CHIRHO: u8 = 0x02; // Message Control (16-bit)
#[allow(dead_code)]
const MSIX_CAP_TABLE_CHIRHO: u8 = 0x04;   // Table Offset / BIR
#[allow(dead_code)]
const MSIX_CAP_PBA_CHIRHO: u8 = 0x08;     // PBA Offset / BIR

/// MSI-X Message Control bits.
#[allow(dead_code)]
const MSIX_CTRL_ENABLE_CHIRHO: u16 = 1 << 15;
/// Function Mask.
#[allow(dead_code)]
const MSIX_CTRL_FUNC_MASK_CHIRHO: u16 = 1 << 14;
/// Table Size mask (bits 10:0).
#[allow(dead_code)]
const MSIX_CTRL_TABLE_SIZE_MASK_CHIRHO: u16 = 0x07FF;

/// MSI-X table entry (16 bytes per entry, in BAR-mapped memory).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct MsixTableEntryChirho {
    /// Message Address (lower 32 bits).
    pub msg_addr_lo_chirho: u32,
    /// Message Address (upper 32 bits).
    pub msg_addr_hi_chirho: u32,
    /// Message Data.
    pub msg_data_chirho: u32,
    /// Vector Control (bit 0 = mask).
    pub vector_control_chirho: u32,
}

// ============================================================================
// MSI descriptor
// ============================================================================

/// Describes an MSI configuration for a PCI device.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MsiConfigChirho {
    /// PCI bus/device/function.
    pub pci_bdf_chirho: (u8, u8, u8),
    /// Offset of the MSI capability in PCI config space.
    pub cap_offset_chirho: u8,
    /// Whether the device supports 64-bit message address.
    pub is_64bit_chirho: bool,
    /// Number of vectors the device is capable of (1, 2, 4, 8, 16, or 32).
    pub vectors_capable_chirho: u8,
    /// Number of vectors currently allocated.
    pub vectors_allocated_chirho: u8,
    /// Base interrupt vector number.
    pub base_vector_chirho: u8,
    /// Target APIC ID.
    pub target_apic_id_chirho: u8,
}

/// Describes an MSI-X configuration for a PCI device.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MsixConfigChirho {
    /// PCI bus/device/function.
    pub pci_bdf_chirho: (u8, u8, u8),
    /// Offset of the MSI-X capability in PCI config space.
    pub cap_offset_chirho: u8,
    /// Number of table entries (0-based count from the register + 1).
    pub table_size_chirho: u16,
    /// BAR index where the MSI-X table lives.
    pub table_bir_chirho: u8,
    /// Offset within the BAR of the MSI-X table.
    pub table_offset_chirho: u32,
    /// BAR index where the PBA (Pending Bit Array) lives.
    pub pba_bir_chirho: u8,
    /// Offset within the BAR of the PBA.
    pub pba_offset_chirho: u32,
}

// ============================================================================
// MSI configuration helpers
// ============================================================================

/// Build the MSI message address value targeting a specific APIC ID.
#[allow(dead_code)]
pub fn msi_address_chirho(apic_id_chirho: u8) -> u32 {
    MSI_ADDRESS_BASE_CHIRHO | ((apic_id_chirho as u32) << 12)
}

/// Build the MSI message data value for a given interrupt vector.
///
/// Edge-triggered, fixed delivery mode.
#[allow(dead_code)]
pub fn msi_data_chirho(vector_chirho: u8) -> u32 {
    vector_chirho as u32
}

/// Configure MSI for a PCI device.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn configure_msi_chirho(config_chirho: &MsiConfigChirho) {
    let (bus_chirho, dev_chirho, func_chirho) = config_chirho.pci_bdf_chirho;
    let cap_chirho = config_chirho.cap_offset_chirho;

    // Read current control
    let ctrl_chirho = unsafe {
        crate::pci_chirho::pci_config_read_u16_chirho(
            bus_chirho, dev_chirho, func_chirho, cap_chirho + MSI_CAP_CONTROL_CHIRHO,
        )
    };

    // Set message address
    let addr_chirho = msi_address_chirho(config_chirho.target_apic_id_chirho);
    unsafe {
        crate::pci_chirho::pci_config_write_u32_chirho(
            bus_chirho, dev_chirho, func_chirho,
            cap_chirho + MSI_CAP_ADDR_LO_CHIRHO,
            addr_chirho,
        );
    }

    // Set message data
    let data_offset_chirho = if config_chirho.is_64bit_chirho {
        // Write upper address as 0
        unsafe {
            crate::pci_chirho::pci_config_write_u32_chirho(
                bus_chirho, dev_chirho, func_chirho,
                cap_chirho + MSI_CAP_ADDR_HI_CHIRHO,
                0,
            );
        }
        MSI_CAP_DATA_64_CHIRHO
    } else {
        MSI_CAP_DATA_32_CHIRHO
    };

    let data_chirho = msi_data_chirho(config_chirho.base_vector_chirho);
    unsafe {
        crate::pci_chirho::pci_config_write_u32_chirho(
            bus_chirho, dev_chirho, func_chirho,
            cap_chirho + data_offset_chirho,
            data_chirho,
        );
    }

    // Enable MSI (set bit 0 of control, set MME to 0 = 1 vector)
    let new_ctrl_chirho = (ctrl_chirho & !MSI_CTRL_MME_MASK_CHIRHO) | MSI_CTRL_ENABLE_CHIRHO;
    // Write control as part of a 32-bit write
    let full_dword_chirho = unsafe {
        crate::pci_chirho::pci_config_read_u32_chirho(
            bus_chirho, dev_chirho, func_chirho,
            cap_chirho & 0xFC,
        )
    };
    let ctrl_offset_in_dword_chirho = (cap_chirho + MSI_CAP_CONTROL_CHIRHO) & 3;
    let mask_chirho = 0xFFFF << (ctrl_offset_in_dword_chirho * 8);
    let new_dword_chirho = (full_dword_chirho & !mask_chirho)
        | ((new_ctrl_chirho as u32) << (ctrl_offset_in_dword_chirho * 8));
    unsafe {
        crate::pci_chirho::pci_config_write_u32_chirho(
            bus_chirho, dev_chirho, func_chirho,
            (cap_chirho + MSI_CAP_CONTROL_CHIRHO) & 0xFC,
            new_dword_chirho,
        );
    }

    crate::serial_println_chirho!(
        "[MSI] Configured PCI {:02x}:{:02x}.{} vector={} apic_id={}",
        bus_chirho, dev_chirho, func_chirho,
        config_chirho.base_vector_chirho,
        config_chirho.target_apic_id_chirho,
    );
}

/// Detect MSI capability on a PCI device.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn detect_msi_chirho(
    dev_chirho: &crate::pci_chirho::PciDeviceChirho,
) -> Option<MsiConfigChirho> {
    let caps_chirho = unsafe { crate::pci_chirho::walk_capabilities_chirho(dev_chirho) };
    for cap_chirho in &caps_chirho {
        if cap_chirho.id_chirho == crate::pci_chirho::PCI_CAP_MSI_CHIRHO {
            let ctrl_chirho = unsafe {
                crate::pci_chirho::pci_config_read_u16_chirho(
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    cap_chirho.offset_chirho + MSI_CAP_CONTROL_CHIRHO,
                )
            };

            let is_64bit_chirho = ctrl_chirho & MSI_CTRL_64BIT_CHIRHO != 0;
            let mmc_chirho = ((ctrl_chirho & MSI_CTRL_MMC_MASK_CHIRHO) >> 1) as u8;
            let vectors_chirho = 1u8 << mmc_chirho;

            return Some(MsiConfigChirho {
                pci_bdf_chirho: (
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                ),
                cap_offset_chirho: cap_chirho.offset_chirho,
                is_64bit_chirho,
                vectors_capable_chirho: vectors_chirho,
                vectors_allocated_chirho: 0,
                base_vector_chirho: 0,
                target_apic_id_chirho: 0,
            });
        }
    }
    None
}
