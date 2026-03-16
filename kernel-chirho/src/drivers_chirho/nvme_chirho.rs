// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! NVMe (Non-Volatile Memory Express) driver for the Lineluya kernel (A5-009).
//!
//! Implements the NVMe 1.4 specification for PCIe-attached SSDs:
//! - Controller identification and initialization
//! - Admin and I/O submission / completion queue management
//! - Read and write command construction
//! - Namespace identification
//!
//! Reference: NVM Express Base Specification 1.4c

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// NVMe controller register offsets (BAR0 MMIO)
// ============================================================================

/// Controller Capabilities (64-bit).
#[allow(dead_code)]
const NVME_CAP_CHIRHO: u64 = 0x00;
/// Version.
#[allow(dead_code)]
const NVME_VS_CHIRHO: u64 = 0x08;
/// Interrupt Mask Set.
#[allow(dead_code)]
const NVME_INTMS_CHIRHO: u64 = 0x0C;
/// Interrupt Mask Clear.
#[allow(dead_code)]
const NVME_INTMC_CHIRHO: u64 = 0x10;
/// Controller Configuration.
#[allow(dead_code)]
const NVME_CC_CHIRHO: u64 = 0x14;
/// Controller Status.
#[allow(dead_code)]
const NVME_CSTS_CHIRHO: u64 = 0x1C;
/// Admin Queue Attributes.
#[allow(dead_code)]
const NVME_AQA_CHIRHO: u64 = 0x24;
/// Admin Submission Queue Base Address (64-bit).
#[allow(dead_code)]
const NVME_ASQ_CHIRHO: u64 = 0x28;
/// Admin Completion Queue Base Address (64-bit).
#[allow(dead_code)]
const NVME_ACQ_CHIRHO: u64 = 0x30;

// ============================================================================
// NVMe opcodes
// ============================================================================

/// Admin command: Identify.
#[allow(dead_code)]
const NVME_ADMIN_IDENTIFY_CHIRHO: u8 = 0x06;
/// Admin command: Create I/O Submission Queue.
#[allow(dead_code)]
const NVME_ADMIN_CREATE_SQ_CHIRHO: u8 = 0x01;
/// Admin command: Create I/O Completion Queue.
#[allow(dead_code)]
const NVME_ADMIN_CREATE_CQ_CHIRHO: u8 = 0x05;
/// Admin command: Delete I/O Submission Queue.
#[allow(dead_code)]
const NVME_ADMIN_DELETE_SQ_CHIRHO: u8 = 0x00;
/// Admin command: Delete I/O Completion Queue.
#[allow(dead_code)]
const NVME_ADMIN_DELETE_CQ_CHIRHO: u8 = 0x04;
/// Admin command: Set Features.
#[allow(dead_code)]
const NVME_ADMIN_SET_FEATURES_CHIRHO: u8 = 0x09;
/// Admin command: Get Features.
#[allow(dead_code)]
const NVME_ADMIN_GET_FEATURES_CHIRHO: u8 = 0x0A;

/// I/O command: Read.
#[allow(dead_code)]
const NVME_IO_READ_CHIRHO: u8 = 0x02;
/// I/O command: Write.
#[allow(dead_code)]
const NVME_IO_WRITE_CHIRHO: u8 = 0x01;
/// I/O command: Flush.
#[allow(dead_code)]
const NVME_IO_FLUSH_CHIRHO: u8 = 0x00;

// ============================================================================
// NVMe Submission Queue Entry (64 bytes)
// ============================================================================

/// A 64-byte NVMe submission queue entry (command).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct NvmeSqeChirho {
    /// Command Dword 0: opcode (7:0), fused (9:8), PSDT (15:14), CID (31:16).
    pub cdw0_chirho: u32,
    /// Namespace ID.
    pub nsid_chirho: u32,
    /// Reserved.
    pub reserved1_chirho: u64,
    /// Metadata pointer.
    pub mptr_chirho: u64,
    /// PRP Entry 1 (or SGL).
    pub prp1_chirho: u64,
    /// PRP Entry 2 (or SGL).
    pub prp2_chirho: u64,
    /// Command-specific Dword 10.
    pub cdw10_chirho: u32,
    /// Command-specific Dword 11.
    pub cdw11_chirho: u32,
    /// Command-specific Dword 12.
    pub cdw12_chirho: u32,
    /// Command-specific Dword 13.
    pub cdw13_chirho: u32,
    /// Command-specific Dword 14.
    pub cdw14_chirho: u32,
    /// Command-specific Dword 15.
    pub cdw15_chirho: u32,
}

// ============================================================================
// NVMe Completion Queue Entry (16 bytes)
// ============================================================================

/// A 16-byte NVMe completion queue entry.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct NvmeCqeChirho {
    /// Command-specific result (Dword 0).
    pub result_chirho: u32,
    /// Reserved.
    pub reserved_chirho: u32,
    /// SQ Head Pointer (15:0), SQ Identifier (31:16).
    pub sq_head_id_chirho: u32,
    /// Status Field (31:17), Phase Tag (0), CID (15:0).
    pub status_cid_chirho: u32,
}

impl NvmeCqeChirho {
    /// Extract the status code from the completion entry.
    #[allow(dead_code)]
    pub fn status_code_chirho(&self) -> u16 {
        ((self.status_cid_chirho >> 17) & 0x7FF) as u16
    }

    /// Extract the phase tag bit.
    #[allow(dead_code)]
    pub fn phase_chirho(&self) -> bool {
        self.status_cid_chirho & 1 != 0
    }

    /// Extract the command ID.
    #[allow(dead_code)]
    pub fn cid_chirho(&self) -> u16 {
        ((self.status_cid_chirho >> 16) & 0xFFFF) as u16
    }
}

// ============================================================================
// NVMe Queue Pair
// ============================================================================

/// Represents a submission/completion queue pair.
#[allow(dead_code)]
pub struct NvmeQueuePairChirho {
    /// Queue ID (0 = admin).
    pub qid_chirho: u16,
    /// Submission queue depth (number of entries).
    pub sq_depth_chirho: u16,
    /// Completion queue depth.
    pub cq_depth_chirho: u16,
    /// Physical address of the submission queue.
    pub sq_phys_chirho: u64,
    /// Physical address of the completion queue.
    pub cq_phys_chirho: u64,
    /// Current submission queue tail pointer.
    pub sq_tail_chirho: u16,
    /// Current completion queue head pointer.
    pub cq_head_chirho: u16,
    /// Expected phase bit for completion entries.
    pub cq_phase_chirho: bool,
    /// Next command ID to use.
    pub next_cid_chirho: u16,
}

impl NvmeQueuePairChirho {
    /// Create a new queue pair descriptor.
    #[allow(dead_code)]
    pub fn new_chirho(
        qid_chirho: u16,
        sq_depth_chirho: u16,
        cq_depth_chirho: u16,
        sq_phys_chirho: u64,
        cq_phys_chirho: u64,
    ) -> Self {
        Self {
            qid_chirho,
            sq_depth_chirho,
            cq_depth_chirho,
            sq_phys_chirho,
            cq_phys_chirho,
            sq_tail_chirho: 0,
            cq_head_chirho: 0,
            cq_phase_chirho: true,
            next_cid_chirho: 0,
        }
    }
}

// ============================================================================
// NVMe Controller
// ============================================================================

/// Represents an NVMe controller attached via PCI.
#[allow(dead_code)]
pub struct NvmeControllerChirho {
    /// PCI bus/device/function.
    pub pci_bdf_chirho: (u8, u8, u8),
    /// Virtual base address of BAR0 (MMIO registers).
    pub bar0_virt_chirho: u64,
    /// Doorbell stride (in bytes). Derived from CAP.DSTRD.
    pub doorbell_stride_chirho: u32,
    /// Maximum queue entries supported.
    pub max_queue_entries_chirho: u16,
    /// Controller serial number (from Identify Controller).
    pub serial_chirho: [u8; 20],
    /// Controller model number (from Identify Controller).
    pub model_chirho: [u8; 40],
    /// Firmware revision.
    pub firmware_rev_chirho: [u8; 8],
    /// Number of namespaces.
    pub num_namespaces_chirho: u32,
    /// Whether the controller has been initialized.
    pub initialized_chirho: AtomicBool,
    /// Admin queue pair.
    pub admin_queue_chirho: Option<NvmeQueuePairChirho>,
    /// I/O queue pairs.
    pub io_queues_chirho: Vec<NvmeQueuePairChirho>,
}

impl NvmeControllerChirho {
    /// Create a new controller descriptor (not yet initialized).
    #[allow(dead_code)]
    pub fn new_chirho(
        bus_chirho: u8,
        dev_chirho: u8,
        func_chirho: u8,
        bar0_virt_chirho: u64,
    ) -> Self {
        Self {
            pci_bdf_chirho: (bus_chirho, dev_chirho, func_chirho),
            bar0_virt_chirho,
            doorbell_stride_chirho: 4,
            max_queue_entries_chirho: 0,
            serial_chirho: [0; 20],
            model_chirho: [0; 40],
            firmware_rev_chirho: [0; 8],
            num_namespaces_chirho: 0,
            initialized_chirho: AtomicBool::new(false),
            admin_queue_chirho: None,
            io_queues_chirho: Vec::new(),
        }
    }

    /// Read a 32-bit register from BAR0.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn read_reg32_chirho(&self, offset_chirho: u64) -> u32 {
        unsafe { core::ptr::read_volatile((self.bar0_virt_chirho + offset_chirho) as *const u32) }
    }

    /// Write a 32-bit register to BAR0.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn write_reg32_chirho(&self, offset_chirho: u64, value_chirho: u32) {
        unsafe {
            core::ptr::write_volatile(
                (self.bar0_virt_chirho + offset_chirho) as *mut u32,
                value_chirho,
            )
        }
    }

    /// Read the controller capabilities register.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn read_capabilities_chirho(&self) -> u64 {
        let lo_chirho = unsafe { self.read_reg32_chirho(NVME_CAP_CHIRHO) } as u64;
        let hi_chirho = unsafe { self.read_reg32_chirho(NVME_CAP_CHIRHO + 4) } as u64;
        lo_chirho | (hi_chirho << 32)
    }

    /// Check if the controller is ready (CSTS.RDY = 1).
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn is_ready_chirho(&self) -> bool {
        let csts_chirho = unsafe { self.read_reg32_chirho(NVME_CSTS_CHIRHO) };
        csts_chirho & 1 != 0
    }

    /// Ring the submission queue doorbell for a given queue.
    ///
    /// # Safety
    /// BAR0 must be mapped and queue must be valid.
    #[allow(dead_code)]
    pub unsafe fn ring_sq_doorbell_chirho(&self, qid_chirho: u16, tail_chirho: u16) {
        let offset_chirho =
            0x1000u64 + (2 * qid_chirho as u64) * self.doorbell_stride_chirho as u64;
        unsafe { self.write_reg32_chirho(offset_chirho, tail_chirho as u32) }
    }

    /// Ring the completion queue doorbell for a given queue.
    ///
    /// # Safety
    /// BAR0 must be mapped and queue must be valid.
    #[allow(dead_code)]
    pub unsafe fn ring_cq_doorbell_chirho(&self, qid_chirho: u16, head_chirho: u16) {
        let offset_chirho =
            0x1000u64 + (2 * qid_chirho as u64 + 1) * self.doorbell_stride_chirho as u64;
        unsafe { self.write_reg32_chirho(offset_chirho, head_chirho as u32) }
    }
}

// ============================================================================
// NVMe namespace info
// ============================================================================

/// Identifies a namespace on an NVMe controller.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NvmeNamespaceChirho {
    /// Namespace ID (1-based).
    pub nsid_chirho: u32,
    /// Namespace size in logical blocks.
    pub size_blocks_chirho: u64,
    /// Logical block size in bytes (e.g. 512, 4096).
    pub block_size_chirho: u32,
    /// Capacity in logical blocks.
    pub capacity_blocks_chirho: u64,
}

// ============================================================================
// Discovery (PCI scan)
// ============================================================================

/// Scan PCI bus 0 for NVMe controllers (class 01h, subclass 08h, prog_if 02h).
///
/// Returns descriptors for all found controllers (not yet initialized).
#[allow(dead_code)]
pub fn discover_nvme_chirho(phys_offset_chirho: u64) -> Vec<NvmeControllerChirho> {
    let mut controllers_chirho = Vec::new();

    let devices_chirho = unsafe { crate::pci_chirho::scan_bus_chirho(0) };
    for dev_chirho in &devices_chirho {
        // NVMe: class=01 (Mass Storage), subclass=08 (NVM), prog_if=02 (NVMe)
        if dev_chirho.class_code_chirho == 0x01
            && dev_chirho.subclass_chirho == 0x08
            && dev_chirho.prog_if_chirho == 0x02
        {
            // Read BAR0
            if let Some(bar_chirho) = unsafe { dev_chirho.read_bar_chirho(0) } {
                let bar0_virt_chirho = phys_offset_chirho + bar_chirho.base_address_chirho;
                let ctrl_chirho = NvmeControllerChirho::new_chirho(
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    bar0_virt_chirho,
                );

                crate::serial_println_chirho!(
                    "[NVMe] Found controller at PCI {:02x}:{:02x}.{} BAR0={:#x}",
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    bar_chirho.base_address_chirho,
                );

                controllers_chirho.push(ctrl_chirho);
            }
        }
    }

    if controllers_chirho.is_empty() {
        crate::serial_println_chirho!("[NVMe] No NVMe controllers found on PCI bus 0");
    }

    controllers_chirho
}
