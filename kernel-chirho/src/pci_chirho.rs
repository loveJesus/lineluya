// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! PCI bus enumeration for the Lineluya kernel (Phase 8).
//!
//! Uses legacy PCI Configuration Mechanism #1 (ports 0xCF8 / 0xCFC) to
//! read and write 32-bit aligned config-space registers.

use x86_64::instructions::port::Port;

// ============================================================================
// PCI configuration I/O ports
// ============================================================================

/// PCI Configuration Address port.
const PCI_CONFIG_ADDRESS_CHIRHO: u16 = 0x0CF8;
/// PCI Configuration Data port.
const PCI_CONFIG_DATA_CHIRHO: u16 = 0x0CFC;

/// Vendor ID indicating that no device is present.
const PCI_VENDOR_NONE_CHIRHO: u16 = 0xFFFF;

// ============================================================================
// PCI device descriptor
// ============================================================================

/// Describes a device discovered during PCI bus enumeration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PciDeviceChirho {
    /// PCI bus number (0–255).
    pub bus_chirho: u8,
    /// Device number on the bus (0–31).
    pub device_chirho: u8,
    /// Function number (0–7).
    pub function_chirho: u8,
    /// Vendor ID from config-space offset 0x00.
    pub vendor_id_chirho: u16,
    /// Device ID from config-space offset 0x00.
    pub device_id_chirho: u16,
    /// Class code (offset 0x08, bits 31:24).
    pub class_code_chirho: u8,
    /// Subclass (offset 0x08, bits 23:16).
    pub subclass_chirho: u8,
}

// ============================================================================
// Config-space access helpers
// ============================================================================

/// Build the 32-bit address value for PCI Configuration Mechanism #1.
fn config_address_chirho(bus_chirho: u8, dev_chirho: u8, func_chirho: u8, offset_chirho: u8) -> u32 {
    (1u32 << 31) // Enable bit
        | ((bus_chirho as u32) << 16)
        | ((dev_chirho as u32 & 0x1F) << 11)
        | ((func_chirho as u32 & 0x07) << 8)
        | ((offset_chirho as u32) & 0xFC) // must be 4-byte aligned
}

/// Read a 32-bit value from PCI config space.
///
/// # Safety
/// Performs raw I/O port access.
#[allow(dead_code)]
pub unsafe fn pci_config_read_u32_chirho(
    bus_chirho: u8,
    dev_chirho: u8,
    func_chirho: u8,
    offset_chirho: u8,
) -> u32 {
    let addr_chirho = config_address_chirho(bus_chirho, dev_chirho, func_chirho, offset_chirho);
    let mut address_port_chirho = Port::<u32>::new(PCI_CONFIG_ADDRESS_CHIRHO);
    let mut data_port_chirho = Port::<u32>::new(PCI_CONFIG_DATA_CHIRHO);
    unsafe {
        address_port_chirho.write(addr_chirho);
        data_port_chirho.read()
    }
}

/// Write a 32-bit value to PCI config space.
///
/// # Safety
/// Performs raw I/O port access.
#[allow(dead_code)]
pub unsafe fn pci_config_write_u32_chirho(
    bus_chirho: u8,
    dev_chirho: u8,
    func_chirho: u8,
    offset_chirho: u8,
    value_chirho: u32,
) {
    let addr_chirho = config_address_chirho(bus_chirho, dev_chirho, func_chirho, offset_chirho);
    let mut address_port_chirho = Port::<u32>::new(PCI_CONFIG_ADDRESS_CHIRHO);
    let mut data_port_chirho = Port::<u32>::new(PCI_CONFIG_DATA_CHIRHO);
    unsafe {
        address_port_chirho.write(addr_chirho);
        data_port_chirho.write(value_chirho);
    }
}

// ============================================================================
// Bus enumeration
// ============================================================================

/// Scan PCI bus 0 and log every device found.
///
/// Iterates over all 32 device slots and (for multi-function devices)
/// all 8 functions.  Devices with vendor ID 0xFFFF are absent.
#[allow(dead_code)]
pub fn enumerate_pci_bus_chirho() {
    crate::serial_println_chirho!("PCI: enumerating bus 0 ...");
    let mut count_chirho: u32 = 0;

    for dev_chirho in 0u8..32 {
        for func_chirho in 0u8..8 {
            let reg0_chirho = unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x00) };
            let vendor_id_chirho = (reg0_chirho & 0xFFFF) as u16;

            if vendor_id_chirho == PCI_VENDOR_NONE_CHIRHO {
                if func_chirho == 0 {
                    break; // no device at this slot
                }
                continue;
            }

            let device_id_chirho = ((reg0_chirho >> 16) & 0xFFFF) as u16;

            let reg2_chirho = unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x08) };
            let class_code_chirho = ((reg2_chirho >> 24) & 0xFF) as u8;
            let subclass_chirho = ((reg2_chirho >> 16) & 0xFF) as u8;

            crate::serial_println_chirho!(
                "  PCI 00:{:02x}.{} vendor={:#06x} device={:#06x} class={:#04x} sub={:#04x}",
                dev_chirho,
                func_chirho,
                vendor_id_chirho,
                device_id_chirho,
                class_code_chirho,
                subclass_chirho,
            );

            count_chirho += 1;

            // If function 0 is not multi-function, skip functions 1-7
            if func_chirho == 0 {
                let header_type_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, 0, 0x0C) };
                if (header_type_chirho >> 16) & 0x80 == 0 {
                    break; // single-function device
                }
            }
        }
    }

    crate::serial_println_chirho!("PCI: found {} device(s) on bus 0", count_chirho);
}

/// Kernel boot-time PCI initialisation entry point.
#[allow(dead_code)]
pub fn init_pci_chirho() {
    enumerate_pci_bus_chirho();
}
