// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Intel e1000/e1000e Gigabit Ethernet NIC driver for the Lineluya kernel (A5-011).
//!
//! Provides:
//! - PCI discovery of Intel 8254x/8257x NICs
//! - MMIO register access (BAR0)
//! - TX/RX descriptor ring structures
//! - Basic initialization sequence (reset, link up, interrupt enable)
//! - Packet transmit and receive stubs
//!
//! Reference: Intel 8254x Software Developer's Manual

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// e1000 register offsets
// ============================================================================

/// Device Control Register.
#[allow(dead_code)]
const E1000_CTRL_CHIRHO: u32 = 0x0000;
/// Device Status Register.
#[allow(dead_code)]
const E1000_STATUS_CHIRHO: u32 = 0x0008;
/// EEPROM/Flash Control Register.
#[allow(dead_code)]
const E1000_EECD_CHIRHO: u32 = 0x0010;
/// EEPROM Read Register.
#[allow(dead_code)]
const E1000_EERD_CHIRHO: u32 = 0x0014;
/// Interrupt Cause Read.
#[allow(dead_code)]
const E1000_ICR_CHIRHO: u32 = 0x00C0;
/// Interrupt Mask Set/Read.
#[allow(dead_code)]
const E1000_IMS_CHIRHO: u32 = 0x00D0;
/// Interrupt Mask Clear.
#[allow(dead_code)]
const E1000_IMC_CHIRHO: u32 = 0x00D8;
/// Receive Control Register.
#[allow(dead_code)]
const E1000_RCTL_CHIRHO: u32 = 0x0100;
/// Transmit Control Register.
#[allow(dead_code)]
const E1000_TCTL_CHIRHO: u32 = 0x0400;
/// RX Descriptor Base Address Low.
#[allow(dead_code)]
const E1000_RDBAL_CHIRHO: u32 = 0x2800;
/// RX Descriptor Base Address High.
#[allow(dead_code)]
const E1000_RDBAH_CHIRHO: u32 = 0x2804;
/// RX Descriptor Length.
#[allow(dead_code)]
const E1000_RDLEN_CHIRHO: u32 = 0x2808;
/// RX Descriptor Head.
#[allow(dead_code)]
const E1000_RDH_CHIRHO: u32 = 0x2810;
/// RX Descriptor Tail.
#[allow(dead_code)]
const E1000_RDT_CHIRHO: u32 = 0x2818;
/// TX Descriptor Base Address Low.
#[allow(dead_code)]
const E1000_TDBAL_CHIRHO: u32 = 0x3800;
/// TX Descriptor Base Address High.
#[allow(dead_code)]
const E1000_TDBAH_CHIRHO: u32 = 0x3804;
/// TX Descriptor Length.
#[allow(dead_code)]
const E1000_TDLEN_CHIRHO: u32 = 0x3808;
/// TX Descriptor Head.
#[allow(dead_code)]
const E1000_TDH_CHIRHO: u32 = 0x3810;
/// TX Descriptor Tail.
#[allow(dead_code)]
const E1000_TDT_CHIRHO: u32 = 0x3818;
/// Receive Address Low (RAL0).
#[allow(dead_code)]
const E1000_RAL0_CHIRHO: u32 = 0x5400;
/// Receive Address High (RAH0).
#[allow(dead_code)]
const E1000_RAH0_CHIRHO: u32 = 0x5404;
/// Multicast Table Array (128 entries).
#[allow(dead_code)]
const E1000_MTA_CHIRHO: u32 = 0x5200;

// ============================================================================
// Control register bits
// ============================================================================

/// Software reset.
#[allow(dead_code)]
const E1000_CTRL_RST_CHIRHO: u32 = 1 << 26;
/// Set Link Up.
#[allow(dead_code)]
const E1000_CTRL_SLU_CHIRHO: u32 = 1 << 6;
/// Auto-Speed Detection Enable.
#[allow(dead_code)]
const E1000_CTRL_ASDE_CHIRHO: u32 = 1 << 5;

// ============================================================================
// Receive control bits
// ============================================================================

/// Receiver Enable.
#[allow(dead_code)]
const E1000_RCTL_EN_CHIRHO: u32 = 1 << 1;
/// Store Bad Packets.
#[allow(dead_code)]
const E1000_RCTL_SBP_CHIRHO: u32 = 1 << 2;
/// Unicast Promiscuous.
#[allow(dead_code)]
const E1000_RCTL_UPE_CHIRHO: u32 = 1 << 3;
/// Multicast Promiscuous.
#[allow(dead_code)]
const E1000_RCTL_MPE_CHIRHO: u32 = 1 << 4;
/// Broadcast Accept.
#[allow(dead_code)]
const E1000_RCTL_BAM_CHIRHO: u32 = 1 << 15;
/// Buffer Size = 2048 (bits 17:16 = 00).
#[allow(dead_code)]
const E1000_RCTL_BSIZE_2048_CHIRHO: u32 = 0 << 16;
/// Strip Ethernet CRC.
#[allow(dead_code)]
const E1000_RCTL_SECRC_CHIRHO: u32 = 1 << 26;

// ============================================================================
// Transmit control bits
// ============================================================================

/// Transmit Enable.
#[allow(dead_code)]
const E1000_TCTL_EN_CHIRHO: u32 = 1 << 1;
/// Pad Short Packets.
#[allow(dead_code)]
const E1000_TCTL_PSP_CHIRHO: u32 = 1 << 3;
/// Collision Threshold (default = 15).
#[allow(dead_code)]
const E1000_TCTL_CT_CHIRHO: u32 = 15 << 4;
/// Collision Distance (default = 64 for full-duplex).
#[allow(dead_code)]
const E1000_TCTL_COLD_CHIRHO: u32 = 64 << 12;

// ============================================================================
// Descriptor structures
// ============================================================================

/// RX legacy descriptor (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct E1000RxDescChirho {
    /// Physical address of the receive buffer.
    pub buffer_addr_chirho: u64,
    /// Length of received data.
    pub length_chirho: u16,
    /// Packet checksum.
    pub checksum_chirho: u16,
    /// Status bits (bit 0 = DD = Descriptor Done).
    pub status_chirho: u8,
    /// Errors.
    pub errors_chirho: u8,
    /// Special / VLAN tag.
    pub special_chirho: u16,
}

/// TX legacy descriptor (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct E1000TxDescChirho {
    /// Physical address of the transmit buffer.
    pub buffer_addr_chirho: u64,
    /// Length of data to transmit.
    pub length_chirho: u16,
    /// Checksum offset.
    pub cso_chirho: u8,
    /// Command bits.
    pub cmd_chirho: u8,
    /// Status / Reserved.
    pub status_chirho: u8,
    /// Checksum start.
    pub css_chirho: u8,
    /// Special / VLAN tag.
    pub special_chirho: u16,
}

/// TX command bits.
#[allow(dead_code)]
const E1000_TXD_CMD_EOP_CHIRHO: u8 = 1 << 0;  // End of Packet
#[allow(dead_code)]
const E1000_TXD_CMD_IFCS_CHIRHO: u8 = 1 << 1; // Insert FCS (CRC)
#[allow(dead_code)]
const E1000_TXD_CMD_RS_CHIRHO: u8 = 1 << 3;   // Report Status

/// RX status bits.
#[allow(dead_code)]
const E1000_RXD_STAT_DD_CHIRHO: u8 = 1 << 0;  // Descriptor Done
#[allow(dead_code)]
const E1000_RXD_STAT_EOP_CHIRHO: u8 = 1 << 1;  // End of Packet

// ============================================================================
// e1000 NIC driver
// ============================================================================

/// Number of descriptors per ring.
#[allow(dead_code)]
const NUM_RX_DESCS_CHIRHO: usize = 32;
#[allow(dead_code)]
const NUM_TX_DESCS_CHIRHO: usize = 32;

/// Represents an e1000 NIC.
#[allow(dead_code)]
pub struct E1000DeviceChirho {
    /// Virtual base address of BAR0 MMIO.
    pub bar0_virt_chirho: u64,
    /// PCI bus/device/function.
    pub pci_bdf_chirho: (u8, u8, u8),
    /// MAC address (6 bytes).
    pub mac_addr_chirho: [u8; 6],
    /// Whether the device has been initialized.
    pub initialized_chirho: AtomicBool,
}

impl E1000DeviceChirho {
    /// Create a new e1000 device descriptor.
    #[allow(dead_code)]
    pub fn new_chirho(
        bus_chirho: u8,
        dev_chirho: u8,
        func_chirho: u8,
        bar0_virt_chirho: u64,
    ) -> Self {
        Self {
            bar0_virt_chirho,
            pci_bdf_chirho: (bus_chirho, dev_chirho, func_chirho),
            mac_addr_chirho: [0; 6],
            initialized_chirho: AtomicBool::new(false),
        }
    }

    /// Read a 32-bit MMIO register.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn read_reg_chirho(&self, offset_chirho: u32) -> u32 {
        unsafe {
            core::ptr::read_volatile(
                (self.bar0_virt_chirho + offset_chirho as u64) as *const u32,
            )
        }
    }

    /// Write a 32-bit MMIO register.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn write_reg_chirho(&self, offset_chirho: u32, value_chirho: u32) {
        unsafe {
            core::ptr::write_volatile(
                (self.bar0_virt_chirho + offset_chirho as u64) as *mut u32,
                value_chirho,
            )
        }
    }

    /// Read the MAC address from the Receive Address registers.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn read_mac_addr_chirho(&mut self) {
        let ral_chirho = unsafe { self.read_reg_chirho(E1000_RAL0_CHIRHO) };
        let rah_chirho = unsafe { self.read_reg_chirho(E1000_RAH0_CHIRHO) };

        self.mac_addr_chirho[0] = (ral_chirho & 0xFF) as u8;
        self.mac_addr_chirho[1] = ((ral_chirho >> 8) & 0xFF) as u8;
        self.mac_addr_chirho[2] = ((ral_chirho >> 16) & 0xFF) as u8;
        self.mac_addr_chirho[3] = ((ral_chirho >> 24) & 0xFF) as u8;
        self.mac_addr_chirho[4] = (rah_chirho & 0xFF) as u8;
        self.mac_addr_chirho[5] = ((rah_chirho >> 8) & 0xFF) as u8;
    }

    /// Perform a software reset of the NIC.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn reset_chirho(&self) {
        let ctrl_chirho = unsafe { self.read_reg_chirho(E1000_CTRL_CHIRHO) };
        unsafe { self.write_reg_chirho(E1000_CTRL_CHIRHO, ctrl_chirho | E1000_CTRL_RST_CHIRHO) };

        // Wait for reset to complete (spin briefly)
        for _ in 0..10000 {
            core::hint::spin_loop();
        }

        // Disable all interrupts during init
        unsafe { self.write_reg_chirho(E1000_IMC_CHIRHO, 0xFFFF_FFFF) };
    }

    /// Set link up.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn link_up_chirho(&self) {
        let ctrl_chirho = unsafe { self.read_reg_chirho(E1000_CTRL_CHIRHO) };
        unsafe {
            self.write_reg_chirho(
                E1000_CTRL_CHIRHO,
                ctrl_chirho | E1000_CTRL_SLU_CHIRHO | E1000_CTRL_ASDE_CHIRHO,
            )
        };
    }

    /// Check if link is up.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn is_link_up_chirho(&self) -> bool {
        let status_chirho = unsafe { self.read_reg_chirho(E1000_STATUS_CHIRHO) };
        status_chirho & (1 << 1) != 0 // LU bit
    }
}

// ============================================================================
// Known Intel NIC device IDs
// ============================================================================

/// Intel vendor ID.
#[allow(dead_code)]
const INTEL_VENDOR_ID_CHIRHO: u16 = 0x8086;

/// Known e1000 device IDs.
#[allow(dead_code)]
const E1000_DEVICE_IDS_CHIRHO: &[u16] = &[
    0x100E, // 82540EM (QEMU default)
    0x100F, // 82545EM
    0x1004, // 82543GC
    0x10D3, // 82574L
    0x10EA, // 82577LM
    0x153A, // I217-LM
    0x1533, // I210
    0x15B8, // I219-V
];

// ============================================================================
// Discovery
// ============================================================================

/// Scan PCI bus for Intel e1000/e1000e NICs.
#[allow(dead_code)]
pub fn discover_e1000_chirho(phys_offset_chirho: u64) -> Vec<E1000DeviceChirho> {
    let mut nics_chirho = Vec::new();

    let devices_chirho = unsafe { crate::pci_chirho::scan_bus_chirho(0) };
    for dev_chirho in &devices_chirho {
        if dev_chirho.vendor_id_chirho == INTEL_VENDOR_ID_CHIRHO
            && E1000_DEVICE_IDS_CHIRHO.contains(&dev_chirho.device_id_chirho)
        {
            if let Some(bar_chirho) = unsafe { dev_chirho.read_bar_chirho(0) } {
                let bar0_virt_chirho = phys_offset_chirho + bar_chirho.base_address_chirho;
                let nic_chirho = E1000DeviceChirho::new_chirho(
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    bar0_virt_chirho,
                );
                crate::serial_println_chirho!(
                    "[e1000] Found NIC at PCI {:02x}:{:02x}.{} devid={:#06x}",
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    dev_chirho.device_id_chirho,
                );
                nics_chirho.push(nic_chirho);
            }
        }
    }

    if nics_chirho.is_empty() {
        crate::serial_println_chirho!("[e1000] No Intel NICs found on PCI bus 0");
    }

    nics_chirho
}
