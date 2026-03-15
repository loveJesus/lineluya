// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! PCI bus enumeration for the Lineluya kernel (A5-003).
//!
//! Uses legacy PCI Configuration Mechanism #1 (ports 0xCF8 / 0xCFC) to
//! read and write 32-bit aligned config-space registers. Provides:
//! - Device/vendor ID scanning across all buses
//! - BAR (Base Address Register) reading and decoding
//! - PCI capability list walking (MSI, MSI-X, Power Mgmt, etc.)
//!
//! Reference: PCI Local Bus Specification 3.0

use alloc::vec::Vec;
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
// PCI config-space register offsets
// ============================================================================

/// Vendor ID / Device ID (offset 0x00).
const PCI_REG_VENDOR_DEVICE_CHIRHO: u8 = 0x00;
/// Command / Status (offset 0x04).
const PCI_REG_COMMAND_STATUS_CHIRHO: u8 = 0x04;
/// Revision ID / Class Code (offset 0x08).
const PCI_REG_CLASS_REV_CHIRHO: u8 = 0x08;
/// Cache Line / Latency / Header Type / BIST (offset 0x0C).
const PCI_REG_HEADER_CHIRHO: u8 = 0x0C;
/// BAR0 (offset 0x10).
const PCI_REG_BAR0_CHIRHO: u8 = 0x10;
/// Capabilities Pointer (offset 0x34, for header type 0).
const PCI_REG_CAP_PTR_CHIRHO: u8 = 0x34;
/// Interrupt Line / Pin (offset 0x3C).
const PCI_REG_INTERRUPT_CHIRHO: u8 = 0x3C;

// ============================================================================
// PCI command register bits
// ============================================================================

/// Enable I/O space access.
#[allow(dead_code)]
pub const PCI_CMD_IO_SPACE_CHIRHO: u16 = 1 << 0;
/// Enable memory space access.
#[allow(dead_code)]
pub const PCI_CMD_MEMORY_SPACE_CHIRHO: u16 = 1 << 1;
/// Enable bus mastering (DMA).
#[allow(dead_code)]
pub const PCI_CMD_BUS_MASTER_CHIRHO: u16 = 1 << 2;
/// Disable INTx# assertion.
#[allow(dead_code)]
pub const PCI_CMD_INTX_DISABLE_CHIRHO: u16 = 1 << 10;

// ============================================================================
// PCI capability IDs
// ============================================================================

/// PCI Power Management capability.
#[allow(dead_code)]
pub const PCI_CAP_PM_CHIRHO: u8 = 0x01;
/// MSI capability.
#[allow(dead_code)]
pub const PCI_CAP_MSI_CHIRHO: u8 = 0x05;
/// MSI-X capability.
#[allow(dead_code)]
pub const PCI_CAP_MSIX_CHIRHO: u8 = 0x11;
/// PCI Express capability.
#[allow(dead_code)]
pub const PCI_CAP_PCIE_CHIRHO: u8 = 0x10;
/// Vendor-specific capability.
#[allow(dead_code)]
pub const PCI_CAP_VENDOR_CHIRHO: u8 = 0x09;

// ============================================================================
// PCI device descriptor
// ============================================================================

/// Describes a device discovered during PCI bus enumeration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PciDeviceChirho {
    /// PCI bus number (0-255).
    pub bus_chirho: u8,
    /// Device number on the bus (0-31).
    pub device_chirho: u8,
    /// Function number (0-7).
    pub function_chirho: u8,
    /// Vendor ID from config-space offset 0x00.
    pub vendor_id_chirho: u16,
    /// Device ID from config-space offset 0x00.
    pub device_id_chirho: u16,
    /// Class code (offset 0x08, bits 31:24).
    pub class_code_chirho: u8,
    /// Subclass (offset 0x08, bits 23:16).
    pub subclass_chirho: u8,
    /// Programming interface (offset 0x08, bits 15:8).
    pub prog_if_chirho: u8,
    /// Revision ID.
    pub revision_id_chirho: u8,
    /// Header type (0x00 = normal, 0x01 = PCI-PCI bridge, 0x02 = CardBus).
    pub header_type_chirho: u8,
    /// Interrupt line.
    pub interrupt_line_chirho: u8,
    /// Interrupt pin.
    pub interrupt_pin_chirho: u8,
}

/// Describes a Base Address Register value.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PciBarChirho {
    /// BAR index (0-5).
    pub index_chirho: u8,
    /// Whether this is a memory BAR (true) or I/O BAR (false).
    pub is_memory_chirho: bool,
    /// Base address (masked to alignment).
    pub base_address_chirho: u64,
    /// Size of the region in bytes.
    pub size_chirho: u64,
    /// For memory BARs: is it 64-bit?
    pub is_64bit_chirho: bool,
    /// For memory BARs: is it prefetchable?
    pub is_prefetchable_chirho: bool,
}

/// A PCI capability list entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PciCapabilityChirho {
    /// Capability ID.
    pub id_chirho: u8,
    /// Offset in config space where this capability starts.
    pub offset_chirho: u8,
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

/// Read a 16-bit value from PCI config space at an arbitrary offset.
///
/// # Safety
/// Performs raw I/O port access.
#[allow(dead_code)]
pub unsafe fn pci_config_read_u16_chirho(
    bus_chirho: u8,
    dev_chirho: u8,
    func_chirho: u8,
    offset_chirho: u8,
) -> u16 {
    let dword_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, offset_chirho & 0xFC) };
    let shift_chirho = ((offset_chirho & 2) * 8) as u32;
    ((dword_chirho >> shift_chirho) & 0xFFFF) as u16
}

/// Read an 8-bit value from PCI config space at an arbitrary offset.
///
/// # Safety
/// Performs raw I/O port access.
#[allow(dead_code)]
pub unsafe fn pci_config_read_u8_chirho(
    bus_chirho: u8,
    dev_chirho: u8,
    func_chirho: u8,
    offset_chirho: u8,
) -> u8 {
    let dword_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, offset_chirho & 0xFC) };
    let shift_chirho = ((offset_chirho & 3) * 8) as u32;
    ((dword_chirho >> shift_chirho) & 0xFF) as u8
}

// ============================================================================
// BAR reading and decoding
// ============================================================================

impl PciDeviceChirho {
    /// Read and decode a BAR at the given index (0-5).
    ///
    /// For 64-bit memory BARs, the next BAR index holds the upper 32 bits,
    /// so bar_index+1 is consumed implicitly.
    ///
    /// # Safety
    /// Performs PCI config space I/O.
    #[allow(dead_code)]
    pub unsafe fn read_bar_chirho(&self, bar_index_chirho: u8) -> Option<PciBarChirho> {
        if bar_index_chirho > 5 {
            return None;
        }

        let offset_chirho = PCI_REG_BAR0_CHIRHO + bar_index_chirho * 4;
        let original_chirho = unsafe {
            pci_config_read_u32_chirho(
                self.bus_chirho,
                self.device_chirho,
                self.function_chirho,
                offset_chirho,
            )
        };

        if original_chirho == 0 {
            return None; // BAR not implemented
        }

        let is_io_chirho = original_chirho & 1 != 0;

        if is_io_chirho {
            // I/O BAR
            let base_chirho = (original_chirho & 0xFFFF_FFFC) as u64;

            // Write all 1s to determine size
            unsafe {
                pci_config_write_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                    0xFFFF_FFFF,
                );
            }
            let size_mask_chirho = unsafe {
                pci_config_read_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                )
            };
            // Restore original value
            unsafe {
                pci_config_write_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                    original_chirho,
                );
            }

            let size_chirho = !((size_mask_chirho & 0xFFFF_FFFC) as u64) + 1;
            let size_chirho = size_chirho & 0xFFFF; // I/O bars are 16-bit address space

            Some(PciBarChirho {
                index_chirho: bar_index_chirho,
                is_memory_chirho: false,
                base_address_chirho: base_chirho,
                size_chirho,
                is_64bit_chirho: false,
                is_prefetchable_chirho: false,
            })
        } else {
            // Memory BAR
            let bar_type_chirho = (original_chirho >> 1) & 0x3;
            let is_64bit_chirho = bar_type_chirho == 2;
            let is_prefetchable_chirho = original_chirho & (1 << 3) != 0;

            let base_low_chirho = (original_chirho & 0xFFFF_FFF0) as u64;

            // Write all 1s to determine size (low 32)
            unsafe {
                pci_config_write_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                    0xFFFF_FFFF,
                );
            }
            let size_low_chirho = unsafe {
                pci_config_read_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                )
            };
            unsafe {
                pci_config_write_u32_chirho(
                    self.bus_chirho,
                    self.device_chirho,
                    self.function_chirho,
                    offset_chirho,
                    original_chirho,
                );
            }

            let (base_chirho, size_chirho) = if is_64bit_chirho && bar_index_chirho < 5 {
                let offset_hi_chirho = offset_chirho + 4;
                let original_hi_chirho = unsafe {
                    pci_config_read_u32_chirho(
                        self.bus_chirho,
                        self.device_chirho,
                        self.function_chirho,
                        offset_hi_chirho,
                    )
                };
                unsafe {
                    pci_config_write_u32_chirho(
                        self.bus_chirho,
                        self.device_chirho,
                        self.function_chirho,
                        offset_hi_chirho,
                        0xFFFF_FFFF,
                    );
                }
                let size_hi_chirho = unsafe {
                    pci_config_read_u32_chirho(
                        self.bus_chirho,
                        self.device_chirho,
                        self.function_chirho,
                        offset_hi_chirho,
                    )
                };
                unsafe {
                    pci_config_write_u32_chirho(
                        self.bus_chirho,
                        self.device_chirho,
                        self.function_chirho,
                        offset_hi_chirho,
                        original_hi_chirho,
                    );
                }

                let base_chirho = base_low_chirho | ((original_hi_chirho as u64) << 32);
                let combined_size_chirho =
                    ((size_hi_chirho as u64) << 32) | ((size_low_chirho & 0xFFFF_FFF0) as u64);
                let size_chirho = (!combined_size_chirho).wrapping_add(1);
                (base_chirho, size_chirho)
            } else {
                let masked_chirho = (size_low_chirho & 0xFFFF_FFF0) as u64;
                let size_chirho = (!(masked_chirho | 0xFFFF_FFFF_0000_0000)).wrapping_add(1);
                (base_low_chirho, size_chirho)
            };

            Some(PciBarChirho {
                index_chirho: bar_index_chirho,
                is_memory_chirho: true,
                base_address_chirho: base_chirho,
                size_chirho,
                is_64bit_chirho,
                is_prefetchable_chirho,
            })
        }
    }

    /// Read all implemented BARs for this device.
    ///
    /// # Safety
    /// Performs PCI config space I/O.
    #[allow(dead_code)]
    pub unsafe fn read_all_bars_chirho(&self) -> Vec<PciBarChirho> {
        let mut bars_chirho = Vec::new();
        let mut idx_chirho = 0u8;
        while idx_chirho < 6 {
            if let Some(bar_chirho) = unsafe { self.read_bar_chirho(idx_chirho) } {
                let skip_chirho = if bar_chirho.is_64bit_chirho { 2 } else { 1 };
                bars_chirho.push(bar_chirho);
                idx_chirho += skip_chirho;
            } else {
                idx_chirho += 1;
            }
        }
        bars_chirho
    }

    /// Enable bus mastering for this device.
    ///
    /// # Safety
    /// Modifies PCI config space.
    #[allow(dead_code)]
    pub unsafe fn enable_bus_master_chirho(&self) {
        let cmd_chirho = unsafe {
            pci_config_read_u16_chirho(
                self.bus_chirho,
                self.device_chirho,
                self.function_chirho,
                PCI_REG_COMMAND_STATUS_CHIRHO,
            )
        };
        let new_cmd_chirho = cmd_chirho | PCI_CMD_BUS_MASTER_CHIRHO | PCI_CMD_MEMORY_SPACE_CHIRHO;
        let status_chirho = unsafe {
            pci_config_read_u16_chirho(
                self.bus_chirho,
                self.device_chirho,
                self.function_chirho,
                PCI_REG_COMMAND_STATUS_CHIRHO + 2,
            )
        };
        let dword_chirho = (new_cmd_chirho as u32) | ((status_chirho as u32) << 16);
        unsafe {
            pci_config_write_u32_chirho(
                self.bus_chirho,
                self.device_chirho,
                self.function_chirho,
                PCI_REG_COMMAND_STATUS_CHIRHO,
                dword_chirho,
            );
        }
    }
}

// ============================================================================
// Capability list walking
// ============================================================================

/// Walk the PCI capability linked list for a device.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn walk_capabilities_chirho(dev_chirho: &PciDeviceChirho) -> Vec<PciCapabilityChirho> {
    let mut caps_chirho = Vec::new();

    // Check if capabilities list is supported (Status register bit 4).
    let status_chirho = unsafe {
        pci_config_read_u16_chirho(
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            PCI_REG_COMMAND_STATUS_CHIRHO + 2,
        )
    };
    if status_chirho & (1 << 4) == 0 {
        return caps_chirho; // No capabilities list
    }

    // Read capabilities pointer (offset 0x34, bits 7:0).
    let mut cap_offset_chirho = unsafe {
        pci_config_read_u8_chirho(
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            PCI_REG_CAP_PTR_CHIRHO,
        )
    };
    cap_offset_chirho &= 0xFC; // Must be dword-aligned

    let mut visited_chirho = 0u32; // Safety counter to prevent infinite loops
    while cap_offset_chirho != 0 && visited_chirho < 48 {
        let cap_header_chirho = unsafe {
            pci_config_read_u32_chirho(
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                cap_offset_chirho,
            )
        };

        let cap_id_chirho = (cap_header_chirho & 0xFF) as u8;
        let next_chirho = ((cap_header_chirho >> 8) & 0xFC) as u8;

        caps_chirho.push(PciCapabilityChirho {
            id_chirho: cap_id_chirho,
            offset_chirho: cap_offset_chirho,
        });

        cap_offset_chirho = next_chirho;
        visited_chirho += 1;
    }

    caps_chirho
}

/// Return a human-readable name for a PCI capability ID.
#[allow(dead_code)]
pub fn capability_name_chirho(cap_id_chirho: u8) -> &'static str {
    match cap_id_chirho {
        0x01 => "Power Management",
        0x03 => "VPD",
        0x05 => "MSI",
        0x09 => "Vendor Specific",
        0x10 => "PCIe",
        0x11 => "MSI-X",
        0x12 => "SATA",
        0x13 => "Advanced Features",
        _ => "Unknown",
    }
}

// ============================================================================
// Bus enumeration
// ============================================================================

/// Probe a single device/function and return a [`PciDeviceChirho`] if present.
#[allow(dead_code)]
unsafe fn probe_function_chirho(
    bus_chirho: u8,
    dev_chirho: u8,
    func_chirho: u8,
) -> Option<PciDeviceChirho> {
    let reg0_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, PCI_REG_VENDOR_DEVICE_CHIRHO) };
    let vendor_id_chirho = (reg0_chirho & 0xFFFF) as u16;

    if vendor_id_chirho == PCI_VENDOR_NONE_CHIRHO || vendor_id_chirho == 0 {
        return None;
    }

    let device_id_chirho = ((reg0_chirho >> 16) & 0xFFFF) as u16;

    let reg2_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, PCI_REG_CLASS_REV_CHIRHO) };
    let class_code_chirho = ((reg2_chirho >> 24) & 0xFF) as u8;
    let subclass_chirho = ((reg2_chirho >> 16) & 0xFF) as u8;
    let prog_if_chirho = ((reg2_chirho >> 8) & 0xFF) as u8;
    let revision_id_chirho = (reg2_chirho & 0xFF) as u8;

    let reg3_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, PCI_REG_HEADER_CHIRHO) };
    let header_type_chirho = ((reg3_chirho >> 16) & 0x7F) as u8;

    let irq_reg_chirho =
        unsafe { pci_config_read_u32_chirho(bus_chirho, dev_chirho, func_chirho, PCI_REG_INTERRUPT_CHIRHO) };
    let interrupt_line_chirho = (irq_reg_chirho & 0xFF) as u8;
    let interrupt_pin_chirho = ((irq_reg_chirho >> 8) & 0xFF) as u8;

    Some(PciDeviceChirho {
        bus_chirho,
        device_chirho: dev_chirho,
        function_chirho: func_chirho,
        vendor_id_chirho,
        device_id_chirho,
        class_code_chirho,
        subclass_chirho,
        prog_if_chirho,
        revision_id_chirho,
        header_type_chirho,
        interrupt_line_chirho,
        interrupt_pin_chirho,
    })
}

/// Scan a single PCI bus and return all devices found.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn scan_bus_chirho(bus_chirho: u8) -> Vec<PciDeviceChirho> {
    let mut devices_chirho = Vec::new();

    for dev_chirho in 0u8..32 {
        // Check function 0 first
        let func0_chirho = unsafe { probe_function_chirho(bus_chirho, dev_chirho, 0) };
        let func0_dev_chirho = match func0_chirho {
            Some(d_chirho) => d_chirho,
            None => continue,
        };

        // Check if multi-function
        let hdr_reg_chirho = unsafe {
            pci_config_read_u32_chirho(bus_chirho, dev_chirho, 0, PCI_REG_HEADER_CHIRHO)
        };
        let is_multifunction_chirho = (hdr_reg_chirho >> 16) & 0x80 != 0;

        devices_chirho.push(func0_dev_chirho);

        if is_multifunction_chirho {
            for func_chirho in 1u8..8 {
                if let Some(dev_found_chirho) =
                    unsafe { probe_function_chirho(bus_chirho, dev_chirho, func_chirho) }
                {
                    devices_chirho.push(dev_found_chirho);
                }
            }
        }
    }

    devices_chirho
}

/// Scan all 256 PCI buses and return every device found.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn scan_all_buses_chirho() -> Vec<PciDeviceChirho> {
    let mut all_devices_chirho = Vec::new();
    for bus_chirho in 0u8..=255 {
        let mut bus_devices_chirho = unsafe { scan_bus_chirho(bus_chirho) };
        all_devices_chirho.append(&mut bus_devices_chirho);
    }
    all_devices_chirho
}

/// Find a PCI device by class code and subclass.
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn find_by_class_chirho(
    class_chirho: u8,
    subclass_chirho: u8,
) -> Vec<PciDeviceChirho> {
    let all_chirho = unsafe { scan_bus_chirho(0) };
    all_chirho
        .into_iter()
        .filter(|d_chirho| {
            d_chirho.class_code_chirho == class_chirho
                && d_chirho.subclass_chirho == subclass_chirho
        })
        .collect()
}

/// Find a PCI device by vendor and device ID (bus 0 only for speed).
///
/// # Safety
/// Performs PCI config space I/O.
#[allow(dead_code)]
pub unsafe fn find_by_id_chirho(
    vendor_id_chirho: u16,
    device_id_chirho: u16,
) -> Option<PciDeviceChirho> {
    let all_chirho = unsafe { scan_bus_chirho(0) };
    all_chirho
        .into_iter()
        .find(|d_chirho| {
            d_chirho.vendor_id_chirho == vendor_id_chirho
                && d_chirho.device_id_chirho == device_id_chirho
        })
}

/// Scan PCI bus 0 and log every device found (backward-compatible entry).
#[allow(dead_code)]
pub fn enumerate_pci_bus_chirho() {
    crate::serial_println_chirho!("PCI: enumerating bus 0 ...");
    let devices_chirho = unsafe { scan_bus_chirho(0) };

    for dev_chirho in &devices_chirho {
        crate::serial_println_chirho!(
            "  PCI {:02x}:{:02x}.{} vendor={:#06x} device={:#06x} class={:#04x} sub={:#04x} prog_if={:#04x} rev={:#04x}",
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            dev_chirho.vendor_id_chirho,
            dev_chirho.device_id_chirho,
            dev_chirho.class_code_chirho,
            dev_chirho.subclass_chirho,
            dev_chirho.prog_if_chirho,
            dev_chirho.revision_id_chirho,
        );

        // Log BARs
        let bars_chirho = unsafe { dev_chirho.read_all_bars_chirho() };
        for bar_chirho in &bars_chirho {
            let type_str_chirho = if bar_chirho.is_memory_chirho {
                if bar_chirho.is_64bit_chirho { "MEM64" } else { "MEM32" }
            } else {
                "IO"
            };
            crate::serial_println_chirho!(
                "    BAR{}: {} base={:#018x} size={:#x}{}",
                bar_chirho.index_chirho,
                type_str_chirho,
                bar_chirho.base_address_chirho,
                bar_chirho.size_chirho,
                if bar_chirho.is_prefetchable_chirho { " prefetchable" } else { "" }
            );
        }

        // Log capabilities
        let caps_chirho = unsafe { walk_capabilities_chirho(dev_chirho) };
        for cap_chirho in &caps_chirho {
            crate::serial_println_chirho!(
                "    CAP: id={:#04x} ({}) at offset {:#04x}",
                cap_chirho.id_chirho,
                capability_name_chirho(cap_chirho.id_chirho),
                cap_chirho.offset_chirho
            );
        }
    }

    crate::serial_println_chirho!("PCI: found {} device(s) on bus 0", devices_chirho.len());
}

/// Kernel boot-time PCI initialisation entry point.
#[allow(dead_code)]
pub fn init_pci_chirho() {
    enumerate_pci_bus_chirho();
}
