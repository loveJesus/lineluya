// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! AHCI (Advanced Host Controller Interface) / SATA driver for the
//! Lineluya kernel (A5-004).
//!
//! Provides:
//! - AHCI HBA (Host Bus Adapter) register structures
//! - Port initialization and spin-up
//! - IDENTIFY DEVICE command (ATA command 0xEC)
//!
//! Reference: AHCI 1.3.1 specification
//!            Serial ATA AHCI 1.3.1 — <https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/serial-ata-ahci-spec-rev1-3-1.pdf>

use core::ptr;
use core::sync::atomic::{fence, Ordering};

use crate::pci_chirho::{self, PciDeviceChirho};

// ============================================================================
// AHCI PCI class/subclass
// ============================================================================

/// PCI class code for mass storage controllers.
const PCI_CLASS_STORAGE_CHIRHO: u8 = 0x01;
/// PCI subclass for SATA controllers.
const PCI_SUBCLASS_SATA_CHIRHO: u8 = 0x06;
/// PCI programming interface for AHCI.
const PCI_PROGIF_AHCI_CHIRHO: u8 = 0x01;

// ============================================================================
// AHCI HBA memory-mapped registers (Generic Host Control)
// ============================================================================

/// AHCI Generic Host Control registers (at ABAR, BAR5).
///
/// These are the global HBA registers at the base of the AHCI MMIO region.
#[repr(C)]
#[derive(Debug)]
#[allow(dead_code)]
pub struct HbaMemChirho {
    /// Host Capabilities (CAP).
    pub cap_chirho: u32,
    /// Global Host Control (GHC).
    pub ghc_chirho: u32,
    /// Interrupt Status (IS) — one bit per port.
    pub is_chirho: u32,
    /// Ports Implemented (PI) — bitmask of which ports exist.
    pub pi_chirho: u32,
    /// AHCI Version (VS).
    pub vs_chirho: u32,
    /// Command Completion Coalescing Control.
    pub ccc_ctl_chirho: u32,
    /// Command Completion Coalescing Ports.
    pub ccc_ports_chirho: u32,
    /// Enclosure Management Location.
    pub em_loc_chirho: u32,
    /// Enclosure Management Control.
    pub em_ctl_chirho: u32,
    /// Host Capabilities Extended (CAP2).
    pub cap2_chirho: u32,
    /// BIOS/OS Handoff Control and Status.
    pub bohc_chirho: u32,
    /// Reserved.
    pub reserved_chirho: [u8; 0xA0 - 0x2C],
    /// Vendor-specific registers.
    pub vendor_chirho: [u8; 0x100 - 0xA0],
    /// Port registers (up to 32 ports, each 0x80 bytes).
    pub ports_chirho: [HbaPortChirho; 32],
}

// ============================================================================
// GHC register bits
// ============================================================================

/// AHCI Enable bit in GHC register.
const GHC_AE_CHIRHO: u32 = 1 << 31;
/// HBA Reset bit in GHC register.
#[allow(dead_code)]
const GHC_HR_CHIRHO: u32 = 1 << 0;
/// Interrupt Enable bit in GHC register.
#[allow(dead_code)]
const GHC_IE_CHIRHO: u32 = 1 << 1;

// ============================================================================
// AHCI Port registers
// ============================================================================

/// Per-port register set, starting at ABAR + 0x100 + (port * 0x80).
#[repr(C)]
#[derive(Debug)]
#[allow(dead_code)]
pub struct HbaPortChirho {
    /// Command List Base Address (lower 32 bits).
    pub clb_chirho: u32,
    /// Command List Base Address (upper 32 bits).
    pub clbu_chirho: u32,
    /// FIS Base Address (lower 32 bits).
    pub fb_chirho: u32,
    /// FIS Base Address (upper 32 bits).
    pub fbu_chirho: u32,
    /// Interrupt Status.
    pub is_chirho: u32,
    /// Interrupt Enable.
    pub ie_chirho: u32,
    /// Command and Status.
    pub cmd_chirho: u32,
    /// Reserved.
    pub reserved0_chirho: u32,
    /// Task File Data (error and status).
    pub tfd_chirho: u32,
    /// Signature.
    pub sig_chirho: u32,
    /// Serial ATA Status (SCR0: SStatus).
    pub ssts_chirho: u32,
    /// Serial ATA Control (SCR2: SControl).
    pub sctl_chirho: u32,
    /// Serial ATA Error (SCR1: SError).
    pub serr_chirho: u32,
    /// Serial ATA Active (SCR3: SActive).
    pub sact_chirho: u32,
    /// Command Issue — set bit N to issue command in slot N.
    pub ci_chirho: u32,
    /// Serial ATA Notification (SCR4: SNotification).
    pub sntf_chirho: u32,
    /// FIS-Based Switching Control.
    pub fbs_chirho: u32,
    /// Device Sleep.
    pub devslp_chirho: u32,
    /// Reserved.
    pub reserved1_chirho: [u8; 0x70 - 0x48],
    /// Vendor-specific.
    pub vendor_chirho: [u8; 0x80 - 0x70],
}

// ============================================================================
// Port CMD register bits
// ============================================================================

/// Start (process command list).
const PORT_CMD_ST_CHIRHO: u32 = 1 << 0;
/// Spin-Up Device.
#[allow(dead_code)]
const PORT_CMD_SUD_CHIRHO: u32 = 1 << 1;
/// Power On Device.
#[allow(dead_code)]
const PORT_CMD_POD_CHIRHO: u32 = 1 << 2;
/// FIS Receive Enable.
const PORT_CMD_FRE_CHIRHO: u32 = 1 << 4;
/// FIS Receive Running (read-only).
const PORT_CMD_FR_CHIRHO: u32 = 1 << 14;
/// Command List Running (read-only).
const PORT_CMD_CR_CHIRHO: u32 = 1 << 15;

// ============================================================================
// Port signatures
// ============================================================================

/// SATA device signature.
const SATA_SIG_ATA_CHIRHO: u32 = 0x0000_0101;
/// ATAPI device signature.
#[allow(dead_code)]
const SATA_SIG_ATAPI_CHIRHO: u32 = 0xEB14_0101;
/// Enclosure management bridge signature.
#[allow(dead_code)]
const SATA_SIG_SEMB_CHIRHO: u32 = 0xC33C_0101;
/// Port multiplier signature.
#[allow(dead_code)]
const SATA_SIG_PM_CHIRHO: u32 = 0x9669_0101;

// ============================================================================
// Device detection (SStatus DET field)
// ============================================================================

/// SStatus DET: Device detected, PHY communication established.
const SSTS_DET_PRESENT_CHIRHO: u32 = 0x3;
/// SStatus IPM: Interface in active state.
const SSTS_IPM_ACTIVE_CHIRHO: u32 = 0x1;

// ============================================================================
// Command structures (Command List, Command Table, FIS)
// ============================================================================

/// Command List Header entry (one of 32 slots). Each is 32 bytes.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct HbaCmdHeaderChirho {
    /// Bits [4:0] = Command FIS Length in DWORDs (2-16).
    /// Bit 5 = ATAPI.
    /// Bit 6 = Write (1=H2D, 0=D2H).
    /// Bit 7 = Prefetchable.
    pub flags_chirho: u8,
    /// Bits [3:0] = Port Multiplier Port.
    /// Bit 4 = Reset.
    /// Bit 5 = BIST.
    /// Bit 6 = Clear Busy upon R_OK.
    /// Bit 7 = Reserved.
    pub flags2_chirho: u8,
    /// Physical Region Descriptor Table Length (entries, max 65535).
    pub prdtl_chirho: u16,
    /// Physical Region Descriptor Byte Count (updated by HBA).
    pub prdbc_chirho: u32,
    /// Command Table Descriptor Base Address (lower, 128-byte aligned).
    pub ctba_chirho: u32,
    /// Command Table Descriptor Base Address (upper 32 bits).
    pub ctbau_chirho: u32,
    /// Reserved.
    pub reserved_chirho: [u32; 4],
}

/// Physical Region Descriptor Table entry (PRDT entry). Each is 16 bytes.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct HbaPrdtEntryChirho {
    /// Data Base Address (lower, word-aligned).
    pub dba_chirho: u32,
    /// Data Base Address (upper 32 bits).
    pub dbau_chirho: u32,
    /// Reserved.
    pub reserved_chirho: u32,
    /// Byte Count (bit 31 = Interrupt on Completion, bits 21:0 = byte count - 1).
    pub dbc_chirho: u32,
}

/// Host-to-Device Register FIS (FIS Type 0x27).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct FisRegH2dChirho {
    /// FIS type = 0x27.
    pub fis_type_chirho: u8,
    /// Bits [3:0] = Port Multiplier, Bit 7 = Command/Control (1=command).
    pub flags_chirho: u8,
    /// ATA command register.
    pub command_chirho: u8,
    /// ATA features register (lower).
    pub feature_lo_chirho: u8,
    /// LBA bits [7:0].
    pub lba0_chirho: u8,
    /// LBA bits [15:8].
    pub lba1_chirho: u8,
    /// LBA bits [23:16].
    pub lba2_chirho: u8,
    /// Device register.
    pub device_chirho: u8,
    /// LBA bits [31:24].
    pub lba3_chirho: u8,
    /// LBA bits [39:32].
    pub lba4_chirho: u8,
    /// LBA bits [47:40].
    pub lba5_chirho: u8,
    /// Features register (upper).
    pub feature_hi_chirho: u8,
    /// Sector count (lower).
    pub count_lo_chirho: u8,
    /// Sector count (upper).
    pub count_hi_chirho: u8,
    /// Isochronous command completion.
    pub icc_chirho: u8,
    /// Control register.
    pub control_chirho: u8,
    /// Reserved.
    pub reserved_chirho: [u8; 4],
}

/// ATA IDENTIFY DEVICE command.
const ATA_CMD_IDENTIFY_CHIRHO: u8 = 0xEC;

/// FIS type: Register Host-to-Device.
const FIS_TYPE_REG_H2D_CHIRHO: u8 = 0x27;

// ============================================================================
// IDENTIFY DEVICE result
// ============================================================================

/// Parsed result from IDENTIFY DEVICE.
#[derive(Debug)]
#[allow(dead_code)]
pub struct IdentifyResultChirho {
    /// Model number string (40 chars, ATA byte-swapped).
    pub model_chirho: [u8; 40],
    /// Serial number string (20 chars, ATA byte-swapped).
    pub serial_chirho: [u8; 20],
    /// Firmware revision string (8 chars, ATA byte-swapped).
    pub firmware_chirho: [u8; 8],
    /// Total number of addressable sectors (LBA48).
    pub total_sectors_chirho: u64,
    /// Whether LBA48 is supported.
    pub lba48_supported_chirho: bool,
    /// Logical sector size in bytes (default 512).
    pub sector_size_chirho: u32,
}

impl IdentifyResultChirho {
    /// Parse from the 512-byte IDENTIFY DEVICE buffer (256 words).
    #[allow(dead_code)]
    pub fn from_buffer_chirho(buf_chirho: &[u16; 256]) -> Self {
        // Words 27-46: Model number (40 bytes, ATA byte-swapped)
        let mut model_chirho = [0u8; 40];
        for i_chirho in 0..20 {
            let w_chirho = buf_chirho[27 + i_chirho];
            model_chirho[i_chirho * 2] = (w_chirho >> 8) as u8;
            model_chirho[i_chirho * 2 + 1] = (w_chirho & 0xFF) as u8;
        }

        // Words 10-19: Serial number (20 bytes)
        let mut serial_chirho = [0u8; 20];
        for i_chirho in 0..10 {
            let w_chirho = buf_chirho[10 + i_chirho];
            serial_chirho[i_chirho * 2] = (w_chirho >> 8) as u8;
            serial_chirho[i_chirho * 2 + 1] = (w_chirho & 0xFF) as u8;
        }

        // Words 23-26: Firmware revision (8 bytes)
        let mut firmware_chirho = [0u8; 8];
        for i_chirho in 0..4 {
            let w_chirho = buf_chirho[23 + i_chirho];
            firmware_chirho[i_chirho * 2] = (w_chirho >> 8) as u8;
            firmware_chirho[i_chirho * 2 + 1] = (w_chirho & 0xFF) as u8;
        }

        // Word 83 bit 10: LBA48 support
        let lba48_supported_chirho = buf_chirho[83] & (1 << 10) != 0;

        // Words 100-103: Total sectors (LBA48)
        let total_sectors_chirho = if lba48_supported_chirho {
            (buf_chirho[100] as u64)
                | ((buf_chirho[101] as u64) << 16)
                | ((buf_chirho[102] as u64) << 32)
                | ((buf_chirho[103] as u64) << 48)
        } else {
            // Words 60-61: Total sectors (LBA28)
            (buf_chirho[60] as u64) | ((buf_chirho[61] as u64) << 16)
        };

        // Word 106: Physical / Logical sector size
        let sector_size_chirho = if buf_chirho[106] & (1 << 12) != 0 {
            // Logical sector size in words (words 117-118)
            let words_chirho =
                (buf_chirho[117] as u32) | ((buf_chirho[118] as u32) << 16);
            words_chirho * 2
        } else {
            512
        };

        Self {
            model_chirho,
            serial_chirho,
            firmware_chirho,
            total_sectors_chirho,
            lba48_supported_chirho,
            sector_size_chirho,
        }
    }

    /// Log the identify result to serial.
    #[allow(dead_code)]
    pub fn dump_chirho(&self) {
        let model_str_chirho =
            core::str::from_utf8(&self.model_chirho).unwrap_or("???").trim();
        let serial_str_chirho =
            core::str::from_utf8(&self.serial_chirho).unwrap_or("???").trim();
        let fw_str_chirho =
            core::str::from_utf8(&self.firmware_chirho).unwrap_or("???").trim();

        crate::serial_println_chirho!("AHCI IDENTIFY DEVICE:");
        crate::serial_println_chirho!("  Model:    {}", model_str_chirho);
        crate::serial_println_chirho!("  Serial:   {}", serial_str_chirho);
        crate::serial_println_chirho!("  Firmware: {}", fw_str_chirho);
        crate::serial_println_chirho!("  LBA48:    {}", self.lba48_supported_chirho);
        crate::serial_println_chirho!("  Sectors:  {}", self.total_sectors_chirho);
        crate::serial_println_chirho!(
            "  Capacity: {} MiB",
            self.total_sectors_chirho * self.sector_size_chirho as u64 / (1024 * 1024)
        );
        crate::serial_println_chirho!("  Sector:   {} bytes", self.sector_size_chirho);
    }
}

// ============================================================================
// AHCI port operations
// ============================================================================

/// Check whether a port has a device attached.
///
/// # Safety
/// `port_chirho` must point to a valid, mapped HBA port register block.
#[allow(dead_code)]
pub unsafe fn port_has_device_chirho(port_chirho: &HbaPortChirho) -> bool {
    let ssts_chirho = unsafe { ptr::read_volatile(&port_chirho.ssts_chirho) };
    let det_chirho = ssts_chirho & 0x0F;
    let ipm_chirho = (ssts_chirho >> 8) & 0x0F;
    det_chirho == SSTS_DET_PRESENT_CHIRHO && ipm_chirho == SSTS_IPM_ACTIVE_CHIRHO
}

/// Determine the type of device attached based on its signature.
///
/// # Safety
/// `port_chirho` must point to a valid, mapped HBA port register block.
#[allow(dead_code)]
pub unsafe fn port_device_type_chirho(port_chirho: &HbaPortChirho) -> &'static str {
    let sig_chirho = unsafe { ptr::read_volatile(&port_chirho.sig_chirho) };
    match sig_chirho {
        SATA_SIG_ATA_CHIRHO => "SATA",
        SATA_SIG_ATAPI_CHIRHO => "SATAPI",
        SATA_SIG_SEMB_CHIRHO => "SEMB",
        SATA_SIG_PM_CHIRHO => "PM",
        _ => "Unknown",
    }
}

/// Stop a port's command engine (clear ST and FRE, wait for CR and FR to clear).
///
/// # Safety
/// `port_chirho` must point to a valid, mapped HBA port register block.
#[allow(dead_code)]
pub unsafe fn port_stop_cmd_chirho(port_chirho: &mut HbaPortChirho) {
    let mut cmd_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };

    // Clear ST (stop command processing)
    cmd_chirho &= !PORT_CMD_ST_CHIRHO;
    unsafe { ptr::write_volatile(&mut port_chirho.cmd_chirho, cmd_chirho) };

    // Wait for CR (Command List Running) to clear
    let mut timeout_chirho = 500_000u32;
    loop {
        let val_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };
        if val_chirho & PORT_CMD_CR_CHIRHO == 0 {
            break;
        }
        timeout_chirho -= 1;
        if timeout_chirho == 0 {
            crate::serial_println_chirho!("AHCI: timeout waiting for CR to clear");
            break;
        }
    }

    // Clear FRE
    cmd_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };
    cmd_chirho &= !PORT_CMD_FRE_CHIRHO;
    unsafe { ptr::write_volatile(&mut port_chirho.cmd_chirho, cmd_chirho) };

    // Wait for FR to clear
    timeout_chirho = 500_000;
    loop {
        let val_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };
        if val_chirho & PORT_CMD_FR_CHIRHO == 0 {
            break;
        }
        timeout_chirho -= 1;
        if timeout_chirho == 0 {
            crate::serial_println_chirho!("AHCI: timeout waiting for FR to clear");
            break;
        }
    }
}

/// Start a port's command engine (set FRE then ST).
///
/// # Safety
/// Command list and FIS memory must be set up before calling.
#[allow(dead_code)]
pub unsafe fn port_start_cmd_chirho(port_chirho: &mut HbaPortChirho) {
    // Wait until CR is clear
    let mut timeout_chirho = 500_000u32;
    loop {
        let val_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };
        if val_chirho & PORT_CMD_CR_CHIRHO == 0 {
            break;
        }
        timeout_chirho -= 1;
        if timeout_chirho == 0 {
            crate::serial_println_chirho!("AHCI: timeout waiting for CR before start");
            return;
        }
    }

    let mut cmd_chirho = unsafe { ptr::read_volatile(&port_chirho.cmd_chirho) };
    cmd_chirho |= PORT_CMD_FRE_CHIRHO;
    unsafe { ptr::write_volatile(&mut port_chirho.cmd_chirho, cmd_chirho) };

    cmd_chirho |= PORT_CMD_ST_CHIRHO;
    unsafe { ptr::write_volatile(&mut port_chirho.cmd_chirho, cmd_chirho) };
}

/// Clear the SError register for a port.
///
/// # Safety
/// `port_chirho` must point to a valid, mapped HBA port register block.
#[allow(dead_code)]
pub unsafe fn port_clear_serr_chirho(port_chirho: &mut HbaPortChirho) {
    let serr_chirho = unsafe { ptr::read_volatile(&port_chirho.serr_chirho) };
    unsafe { ptr::write_volatile(&mut port_chirho.serr_chirho, serr_chirho) };
}

// ============================================================================
// AHCI HBA initialization
// ============================================================================

/// Enable AHCI mode in the GHC register (set AE bit).
///
/// # Safety
/// `hba_chirho` must point to a valid, mapped HBA memory region.
#[allow(dead_code)]
pub unsafe fn enable_ahci_chirho(hba_chirho: &mut HbaMemChirho) {
    let mut ghc_chirho = unsafe { ptr::read_volatile(&hba_chirho.ghc_chirho) };
    ghc_chirho |= GHC_AE_CHIRHO;
    unsafe { ptr::write_volatile(&mut hba_chirho.ghc_chirho, ghc_chirho) };
    fence(Ordering::SeqCst);
    crate::serial_println_chirho!("AHCI: AHCI mode enabled");
}

/// Probe all implemented ports and log attached devices.
///
/// # Safety
/// `hba_chirho` must point to a valid, mapped HBA memory region.
#[allow(dead_code)]
pub unsafe fn probe_ports_chirho(hba_chirho: &HbaMemChirho) {
    let pi_chirho = unsafe { ptr::read_volatile(&hba_chirho.pi_chirho) };
    let vs_chirho = unsafe { ptr::read_volatile(&hba_chirho.vs_chirho) };
    let cap_chirho = unsafe { ptr::read_volatile(&hba_chirho.cap_chirho) };
    let num_ports_chirho = (cap_chirho & 0x1F) + 1;
    let num_cmd_slots_chirho = ((cap_chirho >> 8) & 0x1F) + 1;

    crate::serial_println_chirho!(
        "AHCI: version={}.{} ports={} cmd_slots={} PI={:#010x}",
        vs_chirho >> 16,
        vs_chirho & 0xFFFF,
        num_ports_chirho,
        num_cmd_slots_chirho,
        pi_chirho
    );

    for i_chirho in 0..32u32 {
        if pi_chirho & (1 << i_chirho) == 0 {
            continue;
        }
        let port_chirho = &hba_chirho.ports_chirho[i_chirho as usize];
        if unsafe { port_has_device_chirho(port_chirho) } {
            let dev_type_chirho = unsafe { port_device_type_chirho(port_chirho) };
            let sig_chirho = unsafe { ptr::read_volatile(&port_chirho.sig_chirho) };
            crate::serial_println_chirho!(
                "  Port {}: {} device (sig={:#010x})",
                i_chirho,
                dev_type_chirho,
                sig_chirho
            );
        }
    }
}

// ============================================================================
// AHCI discovery from PCI
// ============================================================================

/// Scan PCI for AHCI controllers and initialize the first one found.
///
/// AHCI controllers have class=0x01 (mass storage), subclass=0x06 (SATA),
/// prog_if=0x01 (AHCI 1.0).
///
/// # Safety
/// Performs PCI and MMIO access.
#[allow(dead_code)]
pub unsafe fn init_ahci_chirho(phys_offset_chirho: u64) {
    crate::serial_println_chirho!("AHCI: scanning PCI for SATA controllers...");

    let devices_chirho = unsafe { pci_chirho::scan_bus_chirho(0) };
    for dev_chirho in &devices_chirho {
        if dev_chirho.class_code_chirho != PCI_CLASS_STORAGE_CHIRHO
            || dev_chirho.subclass_chirho != PCI_SUBCLASS_SATA_CHIRHO
        {
            continue;
        }

        crate::serial_println_chirho!(
            "AHCI: found SATA controller at PCI {:02x}:{:02x}.{} (prog_if={:#04x})",
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            dev_chirho.prog_if_chirho
        );

        if dev_chirho.prog_if_chirho != PCI_PROGIF_AHCI_CHIRHO {
            crate::serial_println_chirho!("AHCI: not AHCI prog_if, skipping");
            continue;
        }

        // Enable bus mastering and memory space
        unsafe { dev_chirho.enable_bus_master_chirho() };

        // Read BAR5 (ABAR) — the AHCI base address
        let bar5_chirho = unsafe { dev_chirho.read_bar_chirho(5) };
        let abar_chirho = match bar5_chirho {
            Some(bar_chirho) if bar_chirho.is_memory_chirho => bar_chirho.base_address_chirho,
            _ => {
                crate::serial_println_chirho!("AHCI: BAR5 not a memory BAR");
                continue;
            }
        };

        crate::serial_println_chirho!("AHCI: ABAR at {:#018x}", abar_chirho);

        // Map ABAR to virtual address
        let hba_virt_chirho = phys_offset_chirho + abar_chirho;
        let hba_chirho = unsafe { &mut *(hba_virt_chirho as *mut HbaMemChirho) };

        // Enable AHCI mode
        unsafe { enable_ahci_chirho(hba_chirho) };

        // Probe all ports
        unsafe { probe_ports_chirho(hba_chirho) };

        // Only initialize the first controller
        return;
    }

    crate::serial_println_chirho!("AHCI: no AHCI controller found");
}
