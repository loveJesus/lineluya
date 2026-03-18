// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! VirtIO device driver framework for the Lineluya kernel (A4-001).
//!
//! Implements VirtIO PCI device discovery (vendor 0x1AF4), virtqueue
//! management (descriptor ring, available ring, used ring), and a
//! VirtIO-blk block device driver using MMIO transport.
//!
//! Reference: VirtIO 1.2 specification — <https://docs.oasis-open.org/virtio/virtio/v1.2/>

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr;
use core::sync::atomic::{fence, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

use crate::block_chirho::{BlockDeviceChirho, SECTOR_SIZE_CHIRHO};
use crate::pci_chirho::{
    pci_assign_bar_chirho, pci_config_read_u32_chirho, pci_config_write_u32_chirho,
    PciDeviceChirho, PCI_CMD_IO_SPACE_CHIRHO, PCI_CMD_BUS_MASTER_CHIRHO,
};

// ============================================================================
// VirtIO PCI vendor / device constants
// ============================================================================

/// Red Hat / VirtIO vendor ID.
const VIRTIO_VENDOR_ID_CHIRHO: u16 = 0x1AF4;

/// VirtIO-blk transitional PCI device ID (legacy range 0x1000-0x103F).
const VIRTIO_BLK_DEVICE_ID_CHIRHO: u16 = 0x1001;

/// VirtIO-blk modern (non-transitional) PCI device ID.
const VIRTIO_BLK_MODERN_DEVICE_ID_CHIRHO: u16 = 0x1042;

/// VirtIO-net transitional PCI device ID (legacy range).
const VIRTIO_NET_DEVICE_ID_CHIRHO: u16 = 0x1000;

/// VirtIO-net modern (non-transitional) PCI device ID.
const VIRTIO_NET_MODERN_DEVICE_ID_CHIRHO: u16 = 0x1041;

/// PCI Subsystem ID for block devices in transitional mode.
#[allow(dead_code)]
const VIRTIO_BLK_SUBSYS_ID_CHIRHO: u16 = 0x0002;

// ============================================================================
// VirtIO device initialization states (typestate pattern)
// ============================================================================

/// VirtIO device bring-up phases.
///
/// Documents the expected initialization sequence per VirtIO spec §3.1.
/// Currently used for logging/documentation; future: enforce via typestate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioInitStateChirho {
    /// Device discovered on PCI bus, vendor/device IDs match.
    DiscoveredChirho,
    /// ACKNOWLEDGE + DRIVER status bits set, feature negotiation in progress.
    FeaturesNegotiatingChirho,
    /// FEATURES_OK status bit set, features accepted by both sides.
    FeaturesNegotiatedChirho,
    /// Virtqueues allocated and configured.
    QueuesReadyChirho,
    /// DRIVER_OK status bit set, device is fully operational.
    DriverReadyChirho,
    /// Device initialization failed.
    FailedChirho,
}

// ============================================================================
// VirtIO device status bits (§2.1)
// ============================================================================

/// Indicates that the guest OS has found the device and recognized it.
const VIRTIO_STATUS_ACKNOWLEDGE_CHIRHO: u32 = 1;
/// Indicates that the guest OS knows how to drive the device.
const VIRTIO_STATUS_DRIVER_CHIRHO: u32 = 2;
/// Feature negotiation is complete.
const VIRTIO_STATUS_FEATURES_OK_CHIRHO: u32 = 8;
/// The driver is ready and the device is set up.
const VIRTIO_STATUS_DRIVER_OK_CHIRHO: u32 = 4;
/// Something went wrong; device has given up on the driver.
#[allow(dead_code)]
const VIRTIO_STATUS_FAILED_CHIRHO: u32 = 128;

// ============================================================================
// VirtIO-blk request types (§5.2.6)
// ============================================================================

/// Read sectors from the device.
const VIRTIO_BLK_T_IN_CHIRHO: u32 = 0;
/// Write sectors to the device.
const VIRTIO_BLK_T_OUT_CHIRHO: u32 = 1;

/// Status byte returned by the device on success.
const VIRTIO_BLK_S_OK_CHIRHO: u8 = 0;

// ============================================================================
// VirtIO MMIO register offsets (§4.2.2, modern MMIO transport)
// ============================================================================

/// Magic value register — must read 0x74726976 ("virt").
const VIRTIO_MMIO_MAGIC_VALUE_CHIRHO: usize = 0x000;
/// Version register — 2 for modern devices.
const VIRTIO_MMIO_VERSION_CHIRHO: usize = 0x004;
/// Device type (1 = net, 2 = blk, etc.).
const VIRTIO_MMIO_DEVICE_ID_CHIRHO: usize = 0x008;
/// Vendor ID.
const VIRTIO_MMIO_VENDOR_ID_CHIRHO: usize = 0x00C;
/// Device feature bits (select with HostFeaturesSel).
const VIRTIO_MMIO_HOST_FEATURES_CHIRHO: usize = 0x010;
/// Device feature selection register.
const VIRTIO_MMIO_HOST_FEATURES_SEL_CHIRHO: usize = 0x014;
/// Driver (guest) feature bits.
const VIRTIO_MMIO_GUEST_FEATURES_CHIRHO: usize = 0x020;
/// Driver feature selection register.
const VIRTIO_MMIO_GUEST_FEATURES_SEL_CHIRHO: usize = 0x024;
/// Queue selector.
const VIRTIO_MMIO_QUEUE_SEL_CHIRHO: usize = 0x030;
/// Maximum queue size for selected queue.
const VIRTIO_MMIO_QUEUE_NUM_MAX_CHIRHO: usize = 0x034;
/// Queue size (must be ≤ max).
const VIRTIO_MMIO_QUEUE_NUM_CHIRHO: usize = 0x038;
/// Queue ready flag.
const VIRTIO_MMIO_QUEUE_READY_CHIRHO: usize = 0x044;
/// Queue notify register.
const VIRTIO_MMIO_QUEUE_NOTIFY_CHIRHO: usize = 0x050;
/// Interrupt status.
const VIRTIO_MMIO_INTERRUPT_STATUS_CHIRHO: usize = 0x060;
/// Interrupt acknowledge.
const VIRTIO_MMIO_INTERRUPT_ACK_CHIRHO: usize = 0x064;
/// Device status.
const VIRTIO_MMIO_STATUS_CHIRHO: usize = 0x070;
/// Physical address of descriptor table (low 32 bits).
const VIRTIO_MMIO_QUEUE_DESC_LOW_CHIRHO: usize = 0x080;
/// Physical address of descriptor table (high 32 bits).
const VIRTIO_MMIO_QUEUE_DESC_HIGH_CHIRHO: usize = 0x084;
/// Physical address of available ring (low 32 bits).
const VIRTIO_MMIO_QUEUE_AVAIL_LOW_CHIRHO: usize = 0x090;
/// Physical address of available ring (high 32 bits).
const VIRTIO_MMIO_QUEUE_AVAIL_HIGH_CHIRHO: usize = 0x094;
/// Physical address of used ring (low 32 bits).
const VIRTIO_MMIO_QUEUE_USED_LOW_CHIRHO: usize = 0x0A0;
/// Physical address of used ring (high 32 bits).
const VIRTIO_MMIO_QUEUE_USED_HIGH_CHIRHO: usize = 0x0A4;
/// Device configuration space starts here (device-specific).
const VIRTIO_MMIO_CONFIG_CHIRHO: usize = 0x100;

/// Expected magic value for a VirtIO MMIO device.
const VIRTIO_MMIO_MAGIC_CHIRHO: u32 = 0x7472_6976; // "virt"

// ============================================================================
// VirtIO legacy PCI I/O port register offsets (from BAR0 base, §4.1 legacy)
// ============================================================================

/// Device Features (32-bit, read).
const VIRTIO_IO_DEVICE_FEATURES_CHIRHO: u16 = 0x00;
/// Guest (Driver) Features (32-bit, write).
const VIRTIO_IO_GUEST_FEATURES_CHIRHO: u16 = 0x04;
/// Queue Address — PFN of virtqueue (32-bit, write).
const VIRTIO_IO_QUEUE_ADDRESS_CHIRHO: u16 = 0x08;
/// Queue Size (16-bit, read).
const VIRTIO_IO_QUEUE_SIZE_CHIRHO: u16 = 0x0C;
/// Queue Select (16-bit, write).
const VIRTIO_IO_QUEUE_SELECT_CHIRHO: u16 = 0x0E;
/// Queue Notify (16-bit, write).
const VIRTIO_IO_QUEUE_NOTIFY_CHIRHO: u16 = 0x10;
/// Device Status (8-bit, read/write).
const VIRTIO_IO_DEVICE_STATUS_CHIRHO: u16 = 0x12;
/// ISR Status (8-bit, read).
#[allow(dead_code)]
const VIRTIO_IO_ISR_STATUS_CHIRHO: u16 = 0x13;
/// Device-specific config starts here (for blk: capacity at 0x14, 64-bit).
const VIRTIO_IO_CONFIG_CHIRHO: u16 = 0x14;

/// Legacy virtqueue alignment (4096 bytes = page size).
const VIRTIO_LEGACY_QUEUE_ALIGN_CHIRHO: usize = 4096;

// ============================================================================
// VirtQueue descriptor flags
// ============================================================================

/// This descriptor continues into the next one (chained).
pub const VRING_DESC_F_NEXT_CHIRHO: u16 = 1;
/// This descriptor's buffer is device-writable (otherwise device-readable).
pub const VRING_DESC_F_WRITE_CHIRHO: u16 = 2;

// ============================================================================
// Default queue size
// ============================================================================

/// Default number of entries in a virtqueue.
const VIRTQUEUE_SIZE_CHIRHO: u16 = 128;

// ============================================================================
// VringDescChirho — virtqueue descriptor (§2.7.5)
// ============================================================================

/// A single virtqueue descriptor entry.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VringDescChirho {
    /// Physical address of the data buffer.
    pub addr_chirho: u64,
    /// Length of the data buffer in bytes.
    pub len_chirho: u32,
    /// Descriptor flags (`VRING_DESC_F_NEXT_CHIRHO`, `VRING_DESC_F_WRITE_CHIRHO`).
    pub flags_chirho: u16,
    /// Index of the next descriptor if `VRING_DESC_F_NEXT_CHIRHO` is set.
    pub next_chirho: u16,
}

// ============================================================================
// VringAvailChirho — available ring (§2.7.6)
// ============================================================================

/// The available ring header.  The ring entries follow immediately in memory.
#[repr(C)]
pub struct VringAvailChirho {
    /// Flags (e.g. `VRING_AVAIL_F_NO_INTERRUPT`).
    pub flags_chirho: u16,
    /// Index of the next entry the driver will write.
    pub idx_chirho: u16,
    // Followed by `queue_size` × u16 ring entries.
}

// ============================================================================
// VringUsedElemChirho / VringUsedChirho — used ring (§2.7.8)
// ============================================================================

/// A single used ring element.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VringUsedElemChirho {
    /// Index of the head descriptor that was used.
    pub id_chirho: u32,
    /// Total number of bytes written into the descriptor chain.
    pub len_chirho: u32,
}

/// The used ring header.  The ring entries follow immediately in memory.
#[repr(C)]
pub struct VringUsedChirho {
    /// Flags.
    pub flags_chirho: u16,
    /// Index of the next entry the device will write.
    pub idx_chirho: u16,
    // Followed by `queue_size` × VringUsedElemChirho ring entries.
}

// ============================================================================
// VirtQueueChirho — complete virtqueue
// ============================================================================

/// A complete VirtIO split virtqueue with descriptor table, available ring,
/// and used ring.
///
/// ## Safety (audit unsafe-003)
///
/// The VirtQueue uses `Vec` for all internal storage (no raw pointers).
/// All descriptor access goes through safe methods: `alloc_desc_chirho`,
/// `free_desc_chirho`, `push_avail_chirho`, `pop_used_chirho`. The only
/// unsafe boundary is writing the physical address to device registers
/// (done in the transport-specific setup code, not here).
pub struct VirtQueueChirho {
    /// Number of entries (descriptors) in this queue.
    pub size_chirho: u16,
    /// The descriptor table, heap-allocated.
    pub desc_chirho: Vec<VringDescChirho>,
    /// Ring of available descriptor indices for the device to consume.
    pub avail_ring_chirho: Vec<u16>,
    /// Flags field for the available ring.
    pub avail_flags_chirho: u16,
    /// Driver-side index into the available ring.
    pub avail_idx_chirho: u16,
    /// Ring of used descriptor elements returned by the device.
    pub used_ring_chirho: Vec<VringUsedElemChirho>,
    /// Flags field for the used ring.
    #[allow(dead_code)]
    pub used_flags_chirho: u16,
    /// Device-side index into the used ring (last seen by driver).
    pub last_used_idx_chirho: u16,
    /// Bitmap tracking which descriptors are free.
    pub free_bitmap_chirho: Vec<bool>,
    /// Number of free descriptors.
    pub num_free_chirho: u16,
}

impl VirtQueueChirho {
    /// Allocate a new virtqueue with `size_chirho` entries.
    pub fn new_chirho(size_chirho: u16) -> Self {
        let sz_chirho = size_chirho as usize;
        Self {
            size_chirho,
            desc_chirho: vec![VringDescChirho::default(); sz_chirho],
            avail_ring_chirho: vec![0u16; sz_chirho],
            avail_flags_chirho: 0,
            avail_idx_chirho: 0,
            used_ring_chirho: vec![VringUsedElemChirho::default(); sz_chirho],
            used_flags_chirho: 0,
            last_used_idx_chirho: 0,
            free_bitmap_chirho: vec![true; sz_chirho],
            num_free_chirho: size_chirho,
        }
    }

    /// Allocate a single free descriptor index, returning `None` if exhausted.
    pub fn alloc_desc_chirho(&mut self) -> Option<u16> {
        for (i_chirho, free_chirho) in self.free_bitmap_chirho.iter_mut().enumerate() {
            if *free_chirho {
                *free_chirho = false;
                self.num_free_chirho -= 1;
                return Some(i_chirho as u16);
            }
        }
        None
    }

    /// Release a descriptor back to the free pool.
    pub fn free_desc_chirho(&mut self, idx_chirho: u16) {
        let i_chirho = idx_chirho as usize;
        if i_chirho < self.free_bitmap_chirho.len() && !self.free_bitmap_chirho[i_chirho] {
            self.free_bitmap_chirho[i_chirho] = true;
            self.num_free_chirho += 1;
        }
    }

    /// Push a descriptor chain head into the available ring.
    pub fn push_avail_chirho(&mut self, head_chirho: u16) {
        let idx_chirho = (self.avail_idx_chirho % self.size_chirho) as usize;
        self.avail_ring_chirho[idx_chirho] = head_chirho;
        fence(Ordering::Release);
        self.avail_idx_chirho = self.avail_idx_chirho.wrapping_add(1);
    }

    /// Check whether the device has returned any used descriptors since last
    /// we checked.
    pub fn has_used_chirho(&self) -> bool {
        // In a real driver we would read the device's used ring idx via MMIO
        // or volatile memory.  For now, this stub compares our cached index.
        false
    }

    /// Pop the next used element, if any.  Returns `(head_desc_idx, bytes_written)`.
    #[allow(dead_code)]
    pub fn pop_used_chirho(&mut self) -> Option<(u32, u32)> {
        if !self.has_used_chirho() {
            return None;
        }
        let idx_chirho = (self.last_used_idx_chirho % self.size_chirho) as usize;
        let elem_chirho = self.used_ring_chirho[idx_chirho];
        self.last_used_idx_chirho = self.last_used_idx_chirho.wrapping_add(1);
        Some((elem_chirho.id_chirho, elem_chirho.len_chirho))
    }
}

// ============================================================================
// VirtioBlkReqChirho — on-device request header (§5.2.6)
// ============================================================================

/// VirtIO-blk request header, placed at the start of the first descriptor.
#[repr(C)]
struct VirtioBlkReqChirho {
    /// Request type (`VIRTIO_BLK_T_IN_CHIRHO` or `VIRTIO_BLK_T_OUT_CHIRHO`).
    type_chirho: u32,
    /// Reserved (must be 0).
    reserved_chirho: u32,
    /// Starting sector.
    sector_chirho: u64,
}

// ============================================================================
// MMIO helper functions
// ============================================================================

/// Read a 32-bit register from a VirtIO MMIO base address.
///
/// # Safety
/// `base_chirho` must point to a valid VirtIO MMIO region.
unsafe fn mmio_read32_chirho(base_chirho: usize, offset_chirho: usize) -> u32 {
    let addr_chirho = (base_chirho + offset_chirho) as *const u32;
    unsafe { ptr::read_volatile(addr_chirho) }
}

/// Write a 32-bit register at a VirtIO MMIO base address.
///
/// # Safety
/// `base_chirho` must point to a valid VirtIO MMIO region.
unsafe fn mmio_write32_chirho(base_chirho: usize, offset_chirho: usize, value_chirho: u32) {
    let addr_chirho = (base_chirho + offset_chirho) as *mut u32;
    unsafe { ptr::write_volatile(addr_chirho, value_chirho) }
}

// ============================================================================
// VirtioMmioTransportChirho — MMIO transport wrapper
// ============================================================================

/// Wrapper around a VirtIO MMIO register region, providing safe helpers for
/// device initialization and queue management.
pub struct VirtioMmioTransportChirho {
    /// Virtual (or identity-mapped physical) base address of the MMIO region.
    base_chirho: usize,
}

impl VirtioMmioTransportChirho {
    /// Create a new transport backed by the MMIO region at `base_chirho`.
    pub fn new_chirho(base_chirho: usize) -> Self {
        Self { base_chirho }
    }

    /// Validate the magic value register.
    pub fn check_magic_chirho(&self) -> bool {
        let magic_chirho = unsafe { mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_MAGIC_VALUE_CHIRHO) };
        magic_chirho == VIRTIO_MMIO_MAGIC_CHIRHO
    }

    /// Read the MMIO version register.
    pub fn version_chirho(&self) -> u32 {
        unsafe { mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_VERSION_CHIRHO) }
    }

    /// Read the device type register.
    pub fn device_id_chirho(&self) -> u32 {
        unsafe { mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_DEVICE_ID_CHIRHO) }
    }

    /// Read the vendor ID register.
    pub fn vendor_id_chirho(&self) -> u32 {
        unsafe { mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_VENDOR_ID_CHIRHO) }
    }

    /// Read the current device status.
    pub fn read_status_chirho(&self) -> u32 {
        unsafe { mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_STATUS_CHIRHO) }
    }

    /// Write the device status register.
    pub fn write_status_chirho(&self, status_chirho: u32) {
        unsafe { mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_STATUS_CHIRHO, status_chirho) }
    }

    /// Reset the device by writing zero to the status register.
    pub fn reset_chirho(&self) {
        self.write_status_chirho(0);
    }

    /// Perform the standard device initialization handshake (§3.1.1):
    ///   1. Reset
    ///   2. Set ACKNOWLEDGE
    ///   3. Set DRIVER
    ///   4. Negotiate features (accept none for now)
    ///   5. Set FEATURES_OK
    ///   6. Set DRIVER_OK
    pub fn init_device_chirho(&self) -> Result<(), &'static str> {
        // Step 1: Reset
        self.reset_chirho();

        // Step 2: Acknowledge
        let mut status_chirho = VIRTIO_STATUS_ACKNOWLEDGE_CHIRHO;
        self.write_status_chirho(status_chirho);

        // Step 3: Driver
        status_chirho |= VIRTIO_STATUS_DRIVER_CHIRHO;
        self.write_status_chirho(status_chirho);

        // Step 4: Read device features & accept none (keep it simple).
        unsafe {
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_HOST_FEATURES_SEL_CHIRHO, 0);
            let _features_chirho = mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_HOST_FEATURES_CHIRHO);
            // Accept no optional features.
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_GUEST_FEATURES_SEL_CHIRHO, 0);
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_GUEST_FEATURES_CHIRHO, 0);
        }

        // Step 5: Features OK
        status_chirho |= VIRTIO_STATUS_FEATURES_OK_CHIRHO;
        self.write_status_chirho(status_chirho);

        // Verify FEATURES_OK was accepted by the device.
        let readback_chirho = self.read_status_chirho();
        if readback_chirho & VIRTIO_STATUS_FEATURES_OK_CHIRHO == 0 {
            return Err("VirtIO device did not accept FEATURES_OK");
        }

        // Step 6: Driver OK
        status_chirho |= VIRTIO_STATUS_DRIVER_OK_CHIRHO;
        self.write_status_chirho(status_chirho);

        Ok(())
    }

    /// Read the maximum queue size for queue `idx_chirho`.
    pub fn queue_num_max_chirho(&self, idx_chirho: u32) -> u32 {
        unsafe {
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_QUEUE_SEL_CHIRHO, idx_chirho);
            mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_QUEUE_NUM_MAX_CHIRHO)
        }
    }

    /// Set the queue size for the currently selected queue.
    pub fn set_queue_num_chirho(&self, num_chirho: u32) {
        unsafe {
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_QUEUE_NUM_CHIRHO, num_chirho);
        }
    }

    /// Configure the physical addresses of the descriptor, available, and used
    /// ring areas for the currently selected queue.
    pub fn set_queue_addr_chirho(
        &self,
        desc_phys_chirho: u64,
        avail_phys_chirho: u64,
        used_phys_chirho: u64,
    ) {
        unsafe {
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_DESC_LOW_CHIRHO,
                desc_phys_chirho as u32,
            );
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_DESC_HIGH_CHIRHO,
                (desc_phys_chirho >> 32) as u32,
            );
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_AVAIL_LOW_CHIRHO,
                avail_phys_chirho as u32,
            );
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_AVAIL_HIGH_CHIRHO,
                (avail_phys_chirho >> 32) as u32,
            );
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_USED_LOW_CHIRHO,
                used_phys_chirho as u32,
            );
            mmio_write32_chirho(
                self.base_chirho,
                VIRTIO_MMIO_QUEUE_USED_HIGH_CHIRHO,
                (used_phys_chirho >> 32) as u32,
            );
        }
    }

    /// Mark the currently selected queue as ready.
    pub fn set_queue_ready_chirho(&self) {
        unsafe {
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_QUEUE_READY_CHIRHO, 1);
        }
    }

    /// Notify the device that new buffers are available in queue `idx_chirho`.
    pub fn notify_queue_chirho(&self, idx_chirho: u32) {
        unsafe {
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_QUEUE_NOTIFY_CHIRHO, idx_chirho);
        }
    }

    /// Read and acknowledge the interrupt status register.
    pub fn ack_interrupt_chirho(&self) -> u32 {
        unsafe {
            let status_chirho = mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_INTERRUPT_STATUS_CHIRHO);
            mmio_write32_chirho(self.base_chirho, VIRTIO_MMIO_INTERRUPT_ACK_CHIRHO, status_chirho);
            status_chirho
        }
    }

    /// Read a 64-bit value from the device configuration space at `offset_chirho`.
    pub fn read_config64_chirho(&self, offset_chirho: usize) -> u64 {
        unsafe {
            let lo_chirho = mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_CONFIG_CHIRHO + offset_chirho) as u64;
            let hi_chirho = mmio_read32_chirho(self.base_chirho, VIRTIO_MMIO_CONFIG_CHIRHO + offset_chirho + 4) as u64;
            (hi_chirho << 32) | lo_chirho
        }
    }
}

// ============================================================================
// VirtioIoTransportChirho — Legacy PCI I/O port transport wrapper
// ============================================================================

/// Wrapper around a VirtIO legacy PCI I/O port register region, providing
/// safe helpers for device initialization and queue management.
///
/// VirtIO legacy devices on QEMU expose registers via an I/O port BAR (BAR0).
/// The register layout differs from modern MMIO transport: offsets are smaller,
/// queues are configured with a single PFN write (page-aligned), and the
/// device-specific config immediately follows the common header at offset 0x14.
///
/// Reference: VirtIO 1.0 spec §4.1.4.8 (Legacy Interface), QEMU virtio-pci.
pub struct VirtioIoTransportChirho {
    /// I/O port base address (BAR0 & 0xFFFC).
    base_chirho: u16,
}

impl VirtioIoTransportChirho {
    /// Create a new I/O port transport from the given base port address.
    pub fn new_chirho(base_chirho: u16) -> Self {
        Self { base_chirho }
    }

    // -- 32-bit I/O port helpers --

    /// Read a 32-bit value from I/O port at `base + offset`.
    unsafe fn read32_chirho(&self, offset_chirho: u16) -> u32 {
        let mut port_chirho = Port::<u32>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.read() }
    }

    /// Write a 32-bit value to I/O port at `base + offset`.
    unsafe fn write32_chirho(&self, offset_chirho: u16, value_chirho: u32) {
        let mut port_chirho = Port::<u32>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.write(value_chirho) }
    }

    // -- 16-bit I/O port helpers --

    /// Read a 16-bit value from I/O port at `base + offset`.
    unsafe fn read16_chirho(&self, offset_chirho: u16) -> u16 {
        let mut port_chirho = Port::<u16>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.read() }
    }

    /// Write a 16-bit value to I/O port at `base + offset`.
    unsafe fn write16_chirho(&self, offset_chirho: u16, value_chirho: u16) {
        let mut port_chirho = Port::<u16>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.write(value_chirho) }
    }

    // -- 8-bit I/O port helpers --

    /// Read an 8-bit value from I/O port at `base + offset`.
    unsafe fn read8_chirho(&self, offset_chirho: u16) -> u8 {
        let mut port_chirho = Port::<u8>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.read() }
    }

    /// Write an 8-bit value to I/O port at `base + offset`.
    unsafe fn write8_chirho(&self, offset_chirho: u16, value_chirho: u8) {
        let mut port_chirho = Port::<u8>::new(self.base_chirho + offset_chirho);
        unsafe { port_chirho.write(value_chirho) }
    }

    // -- Device status operations --

    /// Read the current device status byte.
    pub fn read_status_chirho(&self) -> u8 {
        unsafe { self.read8_chirho(VIRTIO_IO_DEVICE_STATUS_CHIRHO) }
    }

    /// Write the device status byte.
    pub fn write_status_chirho(&self, status_chirho: u8) {
        unsafe { self.write8_chirho(VIRTIO_IO_DEVICE_STATUS_CHIRHO, status_chirho) }
    }

    /// Reset the device by writing zero to the status register.
    pub fn reset_chirho(&self) {
        self.write_status_chirho(0);
    }

    // -- Feature negotiation --

    /// Read the 32-bit device feature bits.
    pub fn read_device_features_chirho(&self) -> u32 {
        unsafe { self.read32_chirho(VIRTIO_IO_DEVICE_FEATURES_CHIRHO) }
    }

    /// Write the 32-bit guest (driver) accepted feature bits.
    pub fn write_guest_features_chirho(&self, features_chirho: u32) {
        unsafe { self.write32_chirho(VIRTIO_IO_GUEST_FEATURES_CHIRHO, features_chirho) }
    }

    // -- Queue operations --

    /// Select a virtqueue by index.
    pub fn select_queue_chirho(&self, queue_idx_chirho: u16) {
        unsafe { self.write16_chirho(VIRTIO_IO_QUEUE_SELECT_CHIRHO, queue_idx_chirho) }
    }

    /// Read the maximum queue size for the currently selected queue.
    pub fn read_queue_size_chirho(&self) -> u16 {
        unsafe { self.read16_chirho(VIRTIO_IO_QUEUE_SIZE_CHIRHO) }
    }

    /// Write the queue address as a Page Frame Number (physical address >> 12).
    /// Legacy VirtIO uses a single PFN for the entire virtqueue layout
    /// (descriptors, available ring, used ring packed contiguously with
    /// page alignment between available and used).
    pub fn write_queue_pfn_chirho(&self, pfn_chirho: u32) {
        unsafe { self.write32_chirho(VIRTIO_IO_QUEUE_ADDRESS_CHIRHO, pfn_chirho) }
    }

    /// Notify the device that new buffers are available in the given queue.
    pub fn notify_queue_chirho(&self, queue_idx_chirho: u16) {
        unsafe { self.write16_chirho(VIRTIO_IO_QUEUE_NOTIFY_CHIRHO, queue_idx_chirho) }
    }

    /// Read and return the ISR status byte (also acknowledges the interrupt).
    pub fn read_isr_chirho(&self) -> u8 {
        unsafe { self.read8_chirho(VIRTIO_IO_ISR_STATUS_CHIRHO) }
    }

    // -- Device-specific config --

    /// Read a 32-bit value from the device-specific config space at `offset`.
    pub fn read_config32_chirho(&self, offset_chirho: u16) -> u32 {
        unsafe { self.read32_chirho(VIRTIO_IO_CONFIG_CHIRHO + offset_chirho) }
    }

    /// Read a 64-bit value from the device-specific config space at `offset`,
    /// as two 32-bit reads (low then high).
    pub fn read_config64_chirho(&self, offset_chirho: u16) -> u64 {
        let lo_chirho = self.read_config32_chirho(offset_chirho) as u64;
        let hi_chirho = self.read_config32_chirho(offset_chirho + 4) as u64;
        (hi_chirho << 32) | lo_chirho
    }

    /// Read a single byte from the device-specific config space at `offset`.
    pub fn read_config8_chirho(&self, offset_chirho: u16) -> u8 {
        unsafe { self.read8_chirho(VIRTIO_IO_CONFIG_CHIRHO + offset_chirho) }
    }
}

// ============================================================================
// VirtioBlkDeviceChirho — VirtIO block device driver
// ============================================================================

/// Transport discriminator — whether the block device uses MMIO or I/O port.
enum VirtioTransportKindChirho {
    /// Modern MMIO transport.
    MmioChirho(VirtioMmioTransportChirho),
    /// Legacy PCI I/O port transport.
    IoChirho(VirtioIoTransportChirho),
}

/// A VirtIO block device backed by MMIO or I/O port transport.
pub struct VirtioBlkDeviceChirho {
    /// Transport for register access (MMIO or I/O port).
    transport_chirho: VirtioTransportKindChirho,
    /// The request virtqueue (queue index 0).
    vq_chirho: Mutex<VirtQueueChirho>,
    /// Total capacity in 512-byte sectors (read from device config).
    capacity_sectors_chirho: u64,
    /// For I/O port legacy transport: physical address of the virtqueue
    /// memory region (contiguous desc + avail + used), so we can compute
    /// pointers for the device's DMA.  Not used for MMIO transport.
    vq_phys_base_chirho: u64,
}

// SAFETY: The Mutex around the virtqueue ensures interior-mutable fields are
// synchronised.  The MMIO transport performs volatile register accesses and
// the I/O port transport performs x86 port I/O, both inherently safe from a
// Rust aliasing perspective.
unsafe impl Send for VirtioBlkDeviceChirho {}
unsafe impl Sync for VirtioBlkDeviceChirho {}

impl VirtioBlkDeviceChirho {
    /// Probe and initialize a VirtIO-blk device at the given MMIO base address.
    ///
    /// Returns `None` if the address does not contain a valid VirtIO block device.
    pub fn probe_mmio_chirho(base_addr_chirho: usize) -> Option<Self> {
        let transport_chirho = VirtioMmioTransportChirho::new_chirho(base_addr_chirho);

        // Validate the magic value.
        if !transport_chirho.check_magic_chirho() {
            return None;
        }

        // Must be version 2 (modern) and device type 2 (block).
        if transport_chirho.version_chirho() < 1 {
            return None;
        }
        if transport_chirho.device_id_chirho() != 2 {
            return None;
        }

        // Perform the initialization handshake.
        if transport_chirho.init_device_chirho().is_err() {
            return None;
        }

        // Read device capacity (first 8 bytes of device config = u64 sector count).
        let capacity_chirho = transport_chirho.read_config64_chirho(0);

        // Set up virtqueue 0.
        let max_size_chirho = transport_chirho.queue_num_max_chirho(0);
        let queue_size_chirho = if max_size_chirho == 0 {
            return None; // no queue available
        } else if (max_size_chirho as u16) < VIRTQUEUE_SIZE_CHIRHO {
            max_size_chirho as u16
        } else {
            VIRTQUEUE_SIZE_CHIRHO
        };

        let vq_chirho = VirtQueueChirho::new_chirho(queue_size_chirho);

        // Tell the device the queue size.
        transport_chirho.set_queue_num_chirho(queue_size_chirho as u32);

        // In a full implementation we would set up physical addresses here.
        // For now the queue data lives in kernel heap — physical addresses
        // will be filled in when we integrate with the page-table layer.
        transport_chirho.set_queue_ready_chirho();

        Some(Self {
            transport_chirho: VirtioTransportKindChirho::MmioChirho(transport_chirho),
            vq_chirho: Mutex::new(vq_chirho),
            capacity_sectors_chirho: capacity_chirho,
            vq_phys_base_chirho: 0,
        })
    }

    /// Probe and initialize a VirtIO-blk device via legacy PCI I/O port transport.
    ///
    /// `io_base_chirho` is the I/O port base address (BAR0 & 0xFFFC).
    ///
    /// Legacy VirtIO initialization sequence (§3.1.1, legacy interface):
    ///   1. Reset device (status = 0)
    ///   2. Set ACKNOWLEDGE (status |= 1)
    ///   3. Set DRIVER (status |= 2)
    ///   4. Read device features, write accepted features
    ///   5. Set up virtqueue 0 (read size, allocate, write PFN)
    ///   6. Set DRIVER_OK (status |= 4)
    ///   7. Read capacity from device config
    ///
    /// Returns `None` if the device cannot be initialized.
    pub fn probe_io_chirho(io_base_chirho: u16) -> Option<Self> {
        let transport_chirho = VirtioIoTransportChirho::new_chirho(io_base_chirho);

        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: probing at I/O base {:#06x}",
            io_base_chirho
        );

        // Step 1: Reset the device.
        transport_chirho.reset_chirho();

        // Step 2: Acknowledge — guest OS has found the device.
        let mut status_chirho: u8 = VIRTIO_STATUS_ACKNOWLEDGE_CHIRHO as u8;
        transport_chirho.write_status_chirho(status_chirho);

        // Step 3: Driver — guest OS knows how to drive the device.
        status_chirho |= VIRTIO_STATUS_DRIVER_CHIRHO as u8;
        transport_chirho.write_status_chirho(status_chirho);

        // Step 4: Feature negotiation.
        // Read what the device offers.
        let device_features_chirho = transport_chirho.read_device_features_chirho();
        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: device features = {:#010x}",
            device_features_chirho
        );
        // Accept no optional features for now (safe baseline).
        // Accept basic features. Feature bit 5 = VIRTIO_RING_F_EVENT_IDX is
        // commonly expected. Let's accept whatever the device offers except
        // features we don't support.
        let accepted_chirho = device_features_chirho & 0x3F; // accept bits 0-5
        transport_chirho.write_guest_features_chirho(accepted_chirho);

        // Step 5: Set up virtqueue 0.
        transport_chirho.select_queue_chirho(0);
        let queue_size_chirho = transport_chirho.read_queue_size_chirho();
        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: queue 0 max size = {}",
            queue_size_chirho
        );

        if queue_size_chirho == 0 {
            crate::serial_debug_chirho!("    VirtIO-blk I/O: queue size is 0, aborting");
            return None;
        }

        // Clamp to our default if device supports more.
        // Legacy VirtIO: MUST use the device's queue size (it's fixed).
        // The device expects desc/avail/used tables sized for this many entries.
        let actual_size_chirho = queue_size_chirho;

        // Allocate the legacy virtqueue layout: contiguous physical memory
        // containing descriptors + available ring + used ring, with the used
        // ring page-aligned.
        //
        // Layout (per VirtIO legacy spec):
        //   Descriptors: actual_size * 16 bytes
        //   Available ring: 2 (flags) + 2 (idx) + actual_size * 2 (ring) + 2 (used_event)
        //   Padding to 4096 alignment
        //   Used ring: 2 (flags) + 2 (idx) + actual_size * 8 (ring) + 2 (avail_event)
        let desc_table_bytes_chirho = (actual_size_chirho as usize) * 16;
        let avail_ring_bytes_chirho = 4 + (actual_size_chirho as usize) * 2 + 2;
        let avail_end_chirho = desc_table_bytes_chirho + avail_ring_bytes_chirho;
        let used_ring_offset_chirho =
            (avail_end_chirho + VIRTIO_LEGACY_QUEUE_ALIGN_CHIRHO - 1)
                & !(VIRTIO_LEGACY_QUEUE_ALIGN_CHIRHO - 1);
        let used_ring_bytes_chirho = 4 + (actual_size_chirho as usize) * 8 + 2;
        let total_bytes_chirho = used_ring_offset_chirho + used_ring_bytes_chirho;

        // Allocate vring from the bootloader's physical memory direct mapping.
        // This GUARANTEES contiguous physical memory (critical for DMA!).
        // Heap allocations are virtually contiguous but may span non-contiguous
        // physical pages — the device expects contiguous physical memory.
        static VRING_PHYS_NEXT_CHIRHO: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0x800000); // Start at 8MB physical

        let alloc_pages_chirho = ((total_bytes_chirho + 4095) / 4096) as u64;
        let phys_base_chirho = VRING_PHYS_NEXT_CHIRHO.fetch_add(
            alloc_pages_chirho * 4096,
            core::sync::atomic::Ordering::SeqCst,
        );
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let aligned_ptr_chirho = (phys_base_chirho + phys_offset_chirho) as usize;

        // Zero the physically contiguous DMA region
        unsafe { core::ptr::write_bytes(aligned_ptr_chirho as *mut u8, 0, total_bytes_chirho); }

        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: vq virt={:#x} phys={:#x} size={} total_bytes={}",
            aligned_ptr_chirho,
            phys_base_chirho,
            actual_size_chirho,
            total_bytes_chirho
        );

        // Zero the entire region (already zeroed by vec!).
        // Write the queue PFN to the device.
        let pfn_chirho = (phys_base_chirho >> 12) as u32;
        transport_chirho.write_queue_pfn_chirho(pfn_chirho);

        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: queue PFN = {:#x} (phys page {:#x})",
            pfn_chirho,
            pfn_chirho as u64 * 4096
        );

        // Step 5b: Set FEATURES_OK (VirtIO 1.0 transitional devices may need this)
        status_chirho |= 8; // FEATURES_OK
        transport_chirho.write_status_chirho(status_chirho);
        // Verify FEATURES_OK is still set (device may clear it if unhappy)
        let verify_status_chirho = transport_chirho.read_status_chirho();
        if verify_status_chirho & 8 == 0 {
            crate::serial_debug_chirho!("    VirtIO-blk I/O: WARNING: FEATURES_OK not accepted");
        }

        // Step 6: Driver OK — device is live.
        status_chirho = verify_status_chirho | VIRTIO_STATUS_DRIVER_OK_CHIRHO as u8;
        transport_chirho.write_status_chirho(status_chirho);

        let final_status_chirho = transport_chirho.read_status_chirho();
        // Read back queue address to verify PFN was written
        transport_chirho.select_queue_chirho(0);
        let readback_pfn_chirho = unsafe { transport_chirho.read32_chirho(VIRTIO_IO_QUEUE_ADDRESS_CHIRHO) };
        let readback_size_chirho = transport_chirho.read_queue_size_chirho();
        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: device status after init = {:#04x}, queue PFN readback = {:#x} (expected {:#x}), qsize = {}",
            final_status_chirho, readback_pfn_chirho, pfn_chirho, readback_size_chirho
        );

        // Step 7: Read capacity from device-specific config.
        // For virtio-blk, capacity is a u64 at config offset 0.
        let capacity_chirho = transport_chirho.read_config64_chirho(0);
        crate::serial_debug_chirho!(
            "    VirtIO-blk I/O: capacity = {} sectors ({} MiB)",
            capacity_chirho,
            capacity_chirho / 2048
        );

        // Build the VirtQueueChirho from the raw memory layout.
        // The device and driver share this memory via DMA: the descriptor
        // table starts at the aligned base, available ring follows, and
        // used ring starts at used_ring_offset.
        let vq_chirho = VirtQueueChirho::new_chirho(actual_size_chirho);

        // DMA memory is at a fixed physical address — no heap allocation to leak.

        // The VirtQueueChirho struct keeps Vec-based copies for building
        // request chains; submit_and_wait_io copies descriptors into the
        // device-shared memory region at vq_phys_base before notifying.

        Some(Self {
            transport_chirho: VirtioTransportKindChirho::IoChirho(transport_chirho),
            vq_chirho: Mutex::new(vq_chirho),
            capacity_sectors_chirho: capacity_chirho,
            vq_phys_base_chirho: aligned_ptr_chirho as u64,
        })
    }

    /// Build a three-descriptor chain for a VirtIO-blk request:
    ///   desc 0: request header (device-readable)
    ///   desc 1: data buffer   (device-readable for write, device-writable for read)
    ///   desc 2: status byte   (device-writable)
    ///
    /// Returns the head descriptor index, or `None` if the queue is full.
    fn build_request_chain_chirho(
        vq_chirho: &mut VirtQueueChirho,
        req_header_chirho: &VirtioBlkReqChirho,
        data_buf_chirho: &mut [u8],
        status_chirho: &mut u8,
        is_write_chirho: bool,
    ) -> Option<u16> {
        // We need 3 free descriptors.
        if vq_chirho.num_free_chirho < 3 {
            return None;
        }

        let d0_chirho = vq_chirho.alloc_desc_chirho()?;
        let d1_chirho = vq_chirho.alloc_desc_chirho()?;
        let d2_chirho = vq_chirho.alloc_desc_chirho()?;

        // Descriptor 0: request header (device-readable).
        vq_chirho.desc_chirho[d0_chirho as usize] = VringDescChirho {
            addr_chirho: req_header_chirho as *const VirtioBlkReqChirho as u64,
            len_chirho: core::mem::size_of::<VirtioBlkReqChirho>() as u32,
            flags_chirho: VRING_DESC_F_NEXT_CHIRHO,
            next_chirho: d1_chirho,
        };

        // Descriptor 1: data buffer.
        let data_flags_chirho = if is_write_chirho {
            VRING_DESC_F_NEXT_CHIRHO // device-readable for writes
        } else {
            VRING_DESC_F_NEXT_CHIRHO | VRING_DESC_F_WRITE_CHIRHO // device-writable for reads
        };
        vq_chirho.desc_chirho[d1_chirho as usize] = VringDescChirho {
            addr_chirho: data_buf_chirho.as_ptr() as u64,
            len_chirho: data_buf_chirho.len() as u32,
            flags_chirho: data_flags_chirho,
            next_chirho: d2_chirho,
        };

        // Descriptor 2: status byte (device-writable).
        vq_chirho.desc_chirho[d2_chirho as usize] = VringDescChirho {
            addr_chirho: status_chirho as *mut u8 as u64,
            len_chirho: 1,
            flags_chirho: VRING_DESC_F_WRITE_CHIRHO,
            next_chirho: 0,
        };

        // Push the chain head onto the available ring.
        vq_chirho.push_avail_chirho(d0_chirho);

        Some(d0_chirho)
    }

    /// Notify the device based on transport type.
    fn notify_chirho(&self, queue_idx_chirho: u16) {
        match &self.transport_chirho {
            VirtioTransportKindChirho::MmioChirho(mmio_chirho) => {
                mmio_chirho.notify_queue_chirho(queue_idx_chirho as u32);
            }
            VirtioTransportKindChirho::IoChirho(io_chirho) => {
                io_chirho.notify_queue_chirho(queue_idx_chirho);
            }
        }
    }

    /// Submit a synchronous block read/write and busy-wait for completion
    /// via MMIO transport.
    ///
    /// In a production driver this would use interrupts and a waker; here
    /// we poll the used ring for simplicity.
    fn submit_and_wait_mmio_chirho(
        &self,
        sector_chirho: u64,
        buf_chirho: &mut [u8],
        is_write_chirho: bool,
    ) -> Result<(), i64> {
        let req_header_chirho = VirtioBlkReqChirho {
            type_chirho: if is_write_chirho {
                VIRTIO_BLK_T_OUT_CHIRHO
            } else {
                VIRTIO_BLK_T_IN_CHIRHO
            },
            reserved_chirho: 0,
            sector_chirho,
        };
        let mut status_byte_chirho: u8 = 0xFF;

        {
            let mut vq_chirho = self.vq_chirho.lock();
            let head_chirho = Self::build_request_chain_chirho(
                &mut vq_chirho,
                &req_header_chirho,
                buf_chirho,
                &mut status_byte_chirho,
                is_write_chirho,
            );
            match head_chirho {
                Some(_h_chirho) => {
                    // Notify the device that queue 0 has new buffers.
                    self.notify_chirho(0);
                }
                None => return Err(-12), // ENOMEM — queue full
            }
        }

        // Busy-wait for the device to process the request.
        // In real hardware we'd WFI/HLT + interrupt; for now just spin briefly.
        // The device writes `status_byte_chirho` when done.
        let mut spins_chirho: u32 = 0;
        while status_byte_chirho == 0xFF {
            core::hint::spin_loop();
            spins_chirho += 1;
            if spins_chirho > 1_000_000 {
                return Err(-5); // EIO — timed out
            }
            // Read volatile to see the update from the device.
            let current_status_chirho =
                unsafe { ptr::read_volatile(&status_byte_chirho as *const u8) };
            if current_status_chirho != 0xFF {
                break;
            }
        }

        // Free the descriptors (in a real driver, read them from the used ring).
        // Skipped here because the chain was built inline for this request.

        if status_byte_chirho == VIRTIO_BLK_S_OK_CHIRHO {
            Ok(())
        } else {
            Err(-5) // EIO
        }
    }

    /// Submit a synchronous block read/write via legacy I/O port transport.
    ///
    /// For the legacy VirtIO interface, the descriptor table, available ring
    /// and used ring all live in a single contiguous physical memory region
    /// that the device accesses via DMA.  We:
    ///   1. Build a 3-descriptor chain (header, data, status) in shared memory
    ///   2. Update the available ring in shared memory
    ///   3. Notify the device via I/O port write
    ///   4. Poll the used ring in shared memory for completion
    #[allow(unused_mut)] // status_byte is mutated by device DMA, not by Rust code
    fn submit_and_wait_io_chirho(
        &self,
        sector_chirho: u64,
        buf_chirho: &mut [u8],
        is_write_chirho: bool,
    ) -> Result<(), i64> {
        let vq_base_chirho = self.vq_phys_base_chirho as usize;
        if vq_base_chirho == 0 {
            return Err(-19); // ENODEV
        }

        // Debug logging removed for performance (was 8 lines per sector read)
        let mut vq_chirho = self.vq_chirho.lock();
        let queue_size_chirho = vq_chirho.size_chirho as usize;

        // Compute shared memory layout pointers.
        let desc_table_bytes_chirho = queue_size_chirho * 16;
        let desc_base_chirho = vq_base_chirho as *mut VringDescChirho;
        let avail_base_chirho =
            (vq_base_chirho + desc_table_bytes_chirho) as *mut u16;
        // Avail ring: [flags:u16][idx:u16][ring: queue_size * u16][used_event:u16]
        let avail_end_chirho = desc_table_bytes_chirho + 4 + queue_size_chirho * 2 + 2;
        let used_ring_offset_chirho =
            (avail_end_chirho + VIRTIO_LEGACY_QUEUE_ALIGN_CHIRHO - 1)
                & !(VIRTIO_LEGACY_QUEUE_ALIGN_CHIRHO - 1);
        let used_base_chirho =
            (vq_base_chirho + used_ring_offset_chirho) as *mut u16;
        // Used ring: [flags:u16][idx:u16][ring: queue_size * VringUsedElemChirho (8 bytes)]

        // Allocate 3 descriptors.
        if vq_chirho.num_free_chirho < 3 {
            return Err(-12); // ENOMEM
        }
        let d0_chirho = vq_chirho.alloc_desc_chirho().ok_or(-12i64)?;
        let d1_chirho = vq_chirho.alloc_desc_chirho().ok_or(-12i64)?;
        let d2_chirho = vq_chirho.alloc_desc_chirho().ok_or(-12i64)?;

        // Use a SINGLE contiguous block request struct.
        // Layout: { type: u32, reserved: u32, sector: u64, data: [u8; N], status: u8 }
        // Data size = buf_chirho.len() (512 for single sector, 4096 for full block).
        let data_size_chirho = buf_chirho.len();
        #[repr(C, packed)]
        struct BlkReqFullChirho {
            type_chirho: u32,
            reserved_chirho: u32,
            sector_chirho: u64,
            // data follows at offset 16 — variable length in the DMA buffer
            // status follows at offset 16 + data_size
        }

        // DMA buffer at fixed physical address (reused per request).
        // Layout: [header 16 bytes][data N bytes][status 1 byte]
        // N = data_size_chirho (512 for single sector, up to 4096 for block)
        let req_phys_chirho: u64 = 0x900000; // 9MB physical — 2 pages available
        let phys_off_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let req_virt_chirho = (req_phys_chirho + phys_off_chirho) as *mut u8;

        // Write header
        unsafe {
            ptr::write_volatile(req_virt_chirho as *mut u32,
                if is_write_chirho { VIRTIO_BLK_T_OUT_CHIRHO } else { VIRTIO_BLK_T_IN_CHIRHO });
            ptr::write_volatile(req_virt_chirho.add(4) as *mut u32, 0); // reserved
            ptr::write_volatile(req_virt_chirho.add(8) as *mut u64, sector_chirho);
            // Zero the data area
            ptr::write_bytes(req_virt_chirho.add(16), 0, data_size_chirho);
            // Set status to 0xFF
            ptr::write_volatile(req_virt_chirho.add(16 + data_size_chirho), 0xFF);
        }

        // Copy write data
        if is_write_chirho {
            unsafe {
                ptr::copy_nonoverlapping(buf_chirho.as_ptr(), req_virt_chirho.add(16), data_size_chirho);
            }
        }

        let header_phys_chirho = req_phys_chirho;
        let data_phys_chirho = req_phys_chirho + 16;
        let status_phys_chirho = req_phys_chirho + 16 + data_size_chirho as u64;

        let status_virt_chirho = unsafe { req_virt_chirho.add(16 + data_size_chirho) };
        let data_virt_chirho = unsafe { req_virt_chirho.add(16) };

        // Debug logging removed for performance

        // Write descriptor 0: request header (device-readable).
        unsafe {
            let d0_ptr_chirho = desc_base_chirho.add(d0_chirho as usize);
            ptr::write_volatile(
                d0_ptr_chirho,
                VringDescChirho {
                    addr_chirho: header_phys_chirho,
                    len_chirho: core::mem::size_of::<VirtioBlkReqChirho>() as u32,
                    flags_chirho: VRING_DESC_F_NEXT_CHIRHO,
                    next_chirho: d1_chirho,
                },
            );
        }

        // Write descriptor 1: data buffer.
        let data_flags_chirho = if is_write_chirho {
            VRING_DESC_F_NEXT_CHIRHO
        } else {
            VRING_DESC_F_NEXT_CHIRHO | VRING_DESC_F_WRITE_CHIRHO
        };
        unsafe {
            let d1_ptr_chirho = desc_base_chirho.add(d1_chirho as usize);
            ptr::write_volatile(
                d1_ptr_chirho,
                VringDescChirho {
                    addr_chirho: data_phys_chirho,
                    len_chirho: buf_chirho.len() as u32,
                    flags_chirho: data_flags_chirho,
                    next_chirho: d2_chirho,
                },
            );
        }

        // Write descriptor 2: status byte (device-writable).
        unsafe {
            let d2_ptr_chirho = desc_base_chirho.add(d2_chirho as usize);
            ptr::write_volatile(
                d2_ptr_chirho,
                VringDescChirho {
                    addr_chirho: status_phys_chirho,
                    len_chirho: 1,
                    flags_chirho: VRING_DESC_F_WRITE_CHIRHO,
                    next_chirho: 0,
                },
            );
        }

        // Update the available ring in shared memory.
        // avail_base layout: [flags:u16][idx:u16][ring[0]:u16][ring[1]:u16]...
        let avail_idx_chirho = vq_chirho.avail_idx_chirho;
        let ring_slot_chirho = (avail_idx_chirho % vq_chirho.size_chirho) as usize;
        unsafe {
            // Write the descriptor head index into the ring slot.
            // ring entries start at avail_base + 4 bytes (after flags + idx).
            let ring_ptr_chirho = avail_base_chirho.add(2 + ring_slot_chirho);
            ptr::write_volatile(ring_ptr_chirho, d0_chirho);

            // Memory barrier to ensure descriptor writes are visible before idx update.
            fence(Ordering::Release);

            // Update avail idx (at avail_base + 2 bytes).
            let idx_ptr_chirho = avail_base_chirho.add(1);
            let new_idx_chirho = avail_idx_chirho.wrapping_add(1);
            ptr::write_volatile(idx_ptr_chirho, new_idx_chirho);
        }
        vq_chirho.avail_idx_chirho = avail_idx_chirho.wrapping_add(1);

        // Memory barrier before notify.
        fence(Ordering::SeqCst);

        // Notify the device.
        self.notify_chirho(0);

        // Poll the used ring for completion.
        // used_base layout: [flags:u16][idx:u16][ring[0]: {id:u32, len:u32}]...
        let last_used_chirho = vq_chirho.last_used_idx_chirho;
        let mut spins_chirho: u32 = 0;
        loop {
            // Read the used ring idx (at used_base + 2 bytes = offset 1 in u16 units).
            let used_idx_chirho = unsafe {
                ptr::read_volatile(used_base_chirho.add(1))
            };

            if used_idx_chirho != last_used_chirho {
                // Device has processed at least one request.
                vq_chirho.last_used_idx_chirho = used_idx_chirho;
                break;
            }

            core::hint::spin_loop();
            spins_chirho += 1;
            if spins_chirho > 1_000_000 {
                let used_flags_chirho = unsafe { ptr::read_volatile(used_base_chirho) };
                let used_idx_final_chirho = unsafe { ptr::read_volatile(used_base_chirho.add(1)) };
                crate::serial_debug_chirho!(
                    "    [VirtIO-IO] TIMEOUT: last_used={} used_flags={} used_idx={} isr={}",
                    last_used_chirho, used_flags_chirho, used_idx_final_chirho,
                    match &self.transport_chirho {
                        VirtioTransportKindChirho::IoChirho(t) => t.read_isr_chirho(),
                        _ => 0xFF,
                    }
                );
                // Free descriptors before returning error.
                vq_chirho.free_desc_chirho(d0_chirho);
                vq_chirho.free_desc_chirho(d1_chirho);
                vq_chirho.free_desc_chirho(d2_chirho);
                return Err(-5); // EIO — timed out
            }
        }

        // Read the status byte from DMA region (device wrote it via DMA).
        let final_status_chirho = unsafe { ptr::read_volatile(status_virt_chirho) };

        // Copy read data back from the contiguous request struct
        if !is_write_chirho && final_status_chirho == VIRTIO_BLK_S_OK_CHIRHO {
            unsafe {
                ptr::copy_nonoverlapping(data_virt_chirho, buf_chirho.as_mut_ptr(), buf_chirho.len());
            }
        }

        // Request buffer is at a fixed physical address — no heap dealloc needed.

        // Free descriptors.
        vq_chirho.free_desc_chirho(d0_chirho);
        vq_chirho.free_desc_chirho(d1_chirho);
        vq_chirho.free_desc_chirho(d2_chirho);

        if final_status_chirho == VIRTIO_BLK_S_OK_CHIRHO {
            Ok(())
        } else {
            crate::serial_debug_chirho!(
                "    VirtIO-blk I/O: request failed, status={}",
                final_status_chirho
            );
            Err(-5) // EIO
        }
    }

    /// Submit a synchronous block read/write, dispatching to the appropriate
    /// transport implementation.
    fn submit_and_wait_chirho(
        &self,
        sector_chirho: u64,
        buf_chirho: &mut [u8],
        is_write_chirho: bool,
    ) -> Result<(), i64> {
        match &self.transport_chirho {
            VirtioTransportKindChirho::MmioChirho(_) => {
                self.submit_and_wait_mmio_chirho(sector_chirho, buf_chirho, is_write_chirho)
            }
            VirtioTransportKindChirho::IoChirho(_) => {
                self.submit_and_wait_io_chirho(sector_chirho, buf_chirho, is_write_chirho)
            }
        }
    }
}

impl BlockDeviceChirho for VirtioBlkDeviceChirho {
    fn read_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &mut [u8],
    ) -> Result<(), i64> {
        if buf_chirho.len() < SECTOR_SIZE_CHIRHO {
            return Err(-22); // EINVAL
        }
        self.submit_and_wait_chirho(block_nr_chirho, buf_chirho, false)
    }

    fn write_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &[u8],
    ) -> Result<(), i64> {
        if buf_chirho.len() < SECTOR_SIZE_CHIRHO {
            return Err(-22); // EINVAL
        }
        crate::serial_debug_chirho!(
            "[VirtIO-blk] WRITE sector={} len={}",
            block_nr_chirho,
            buf_chirho.len()
        );
        // The method needs a mutable slice for the descriptor chain, but for
        // writes the device only reads from it.  Copy into a temporary buffer.
        let mut tmp_chirho = vec![0u8; buf_chirho.len()];
        tmp_chirho.copy_from_slice(buf_chirho);
        let result_chirho = self.submit_and_wait_chirho(block_nr_chirho, &mut tmp_chirho, true);
        if let Err(err_chirho) = &result_chirho {
            crate::serial_debug_chirho!(
                "[VirtIO-blk] WRITE FAILED sector={} err={}",
                block_nr_chirho,
                err_chirho
            );
        }
        result_chirho
    }

    fn block_size_chirho(&self) -> usize {
        SECTOR_SIZE_CHIRHO
    }

    fn total_blocks_chirho(&self) -> u64 {
        self.capacity_sectors_chirho
    }
}

// ============================================================================
// PCI bus scanning for VirtIO devices
// ============================================================================

/// Scan PCI bus 0 for VirtIO devices (vendor 0x1AF4).
///
/// Returns a list of PCI device descriptors for any VirtIO devices found.
/// This reuses the low-level PCI config-space accessors from `pci_chirho`.
#[allow(dead_code)]
pub fn scan_pci_virtio_chirho() -> Vec<PciDeviceChirho> {
    let mut found_chirho: Vec<PciDeviceChirho> = Vec::new();

    for dev_chirho in 0u8..32 {
        for func_chirho in 0u8..8 {
            let reg0_chirho =
                unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x00) };
            let vendor_id_chirho = (reg0_chirho & 0xFFFF) as u16;

            if vendor_id_chirho == 0xFFFF {
                if func_chirho == 0 {
                    break;
                }
                continue;
            }

            if vendor_id_chirho == VIRTIO_VENDOR_ID_CHIRHO {
                let device_id_chirho = ((reg0_chirho >> 16) & 0xFFFF) as u16;
                let reg2_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x08) };
                let class_code_chirho = ((reg2_chirho >> 24) & 0xFF) as u8;
                let subclass_chirho = ((reg2_chirho >> 16) & 0xFF) as u8;

                let reg3_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x08) };
                let prog_if_chirho = ((reg3_chirho >> 8) & 0xFF) as u8;
                let revision_id_chirho = (reg3_chirho & 0xFF) as u8;
                let hdr_type_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x0C) };
                let header_type_chirho = ((hdr_type_chirho >> 16) & 0xFF) as u8;
                let irq_reg_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, func_chirho, 0x3C) };
                let interrupt_line_chirho = (irq_reg_chirho & 0xFF) as u8;
                let interrupt_pin_chirho = ((irq_reg_chirho >> 8) & 0xFF) as u8;
                found_chirho.push(PciDeviceChirho {
                    bus_chirho: 0,
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
                });
            }

            // Check multi-function bit
            if func_chirho == 0 {
                let header_type_chirho =
                    unsafe { pci_config_read_u32_chirho(0, dev_chirho, 0, 0x0C) };
                if (header_type_chirho >> 16) & 0x80 == 0 {
                    break;
                }
            }
        }
    }

    found_chirho
}

/// Check if a PCI device is a VirtIO-blk device (transitional or modern).
#[allow(dead_code)]
pub fn is_virtio_blk_chirho(device_chirho: &PciDeviceChirho) -> bool {
    device_chirho.vendor_id_chirho == VIRTIO_VENDOR_ID_CHIRHO
        && (device_chirho.device_id_chirho == VIRTIO_BLK_DEVICE_ID_CHIRHO
            || device_chirho.device_id_chirho == VIRTIO_BLK_MODERN_DEVICE_ID_CHIRHO)
}

/// Check if a PCI device is a VirtIO-net device (transitional or modern).
/// P3-001: VirtIO-net detection.
#[allow(dead_code)]
pub fn is_virtio_net_chirho(device_chirho: &PciDeviceChirho) -> bool {
    device_chirho.vendor_id_chirho == VIRTIO_VENDOR_ID_CHIRHO
        && (device_chirho.device_id_chirho == VIRTIO_NET_DEVICE_ID_CHIRHO
            || device_chirho.device_id_chirho == VIRTIO_NET_MODERN_DEVICE_ID_CHIRHO)
}

/// Read a PCI BAR (Base Address Register) for a device.
///
/// `bar_index_chirho` is 0–5.  Returns the raw BAR value (address + type bits).
#[allow(dead_code)]
pub fn read_pci_bar_chirho(device_chirho: &PciDeviceChirho, bar_index_chirho: u8) -> u32 {
    let offset_chirho = 0x10 + (bar_index_chirho * 4);
    unsafe {
        pci_config_read_u32_chirho(
            device_chirho.bus_chirho,
            device_chirho.device_chirho,
            device_chirho.function_chirho,
            offset_chirho,
        )
    }
}

// ============================================================================
// Kernel initialization entry point
// ============================================================================

/// Acknowledge a VirtIO PCI interrupt by reading the ISR status register.
/// Reading ISR automatically clears it on the device side.
/// Called from the interrupt handler (IRQ 11, vector 43).
pub fn ack_virtio_interrupt_chirho() {
    // Read ISR from all known VirtIO I/O base addresses.
    // Each VirtIO device has ISR at iobase + 0x13.
    // We store the I/O bases during PCI scan.
    let bases_chirho = VIRTIO_IO_BASES_CHIRHO.lock();
    for &base_chirho in bases_chirho.iter() {
        let transport_chirho = VirtioIoTransportChirho::new_chirho(base_chirho);
        let _isr_chirho = transport_chirho.read_isr_chirho();
    }
}

/// Known VirtIO I/O port base addresses (populated during PCI scan).
static VIRTIO_IO_BASES_CHIRHO: spin::Mutex<alloc::vec::Vec<u16>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// Initialize VirtIO subsystem: scan PCI bus, log findings, and attempt to
/// probe any VirtIO-blk devices found.  If a block device is successfully
/// initialised, read sector 0 and log the first 16 bytes as a smoke test
/// (P2-001 / P2-002).
pub fn init_virtio_chirho() {
    crate::serial_debug_chirho!("VirtIO: scanning PCI bus for VirtIO devices...");

    let devices_chirho = scan_pci_virtio_chirho();

    if devices_chirho.is_empty() {
        crate::serial_debug_chirho!("VirtIO: no VirtIO devices found on PCI bus 0");
    } else {
        for dev_chirho in &devices_chirho {
            crate::serial_debug_chirho!(
                "  VirtIO PCI {:02x}:{:02x}.{} device_id={:#06x} class={:#04x}",
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                dev_chirho.device_id_chirho,
                dev_chirho.class_code_chirho,
            );
            if is_virtio_blk_chirho(dev_chirho) {
                crate::serial_debug_chirho!("    -> VirtIO-blk device detected");
                probe_and_test_blk_chirho(dev_chirho);
            }
            // P3-001: Detect VirtIO-net devices
            if is_virtio_net_chirho(dev_chirho) {
                crate::serial_debug_chirho!(
                    "    -> VirtIO-net device detected (PCI {:02x}:{:02x}.{}, device_id={:#06x})",
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    dev_chirho.device_id_chirho,
                );
                let bar0_chirho = read_pci_bar_chirho(dev_chirho, 0);
                let is_io_bar_chirho = bar0_chirho & 1 != 0;

                if is_io_bar_chirho {
                    // I/O port transport — probe via net_chirho
                    let io_base_chirho = (bar0_chirho & 0xFFFC) as u16;
                    crate::serial_debug_chirho!(
                        "    VirtIO-net BAR0 is I/O port: raw={:#x} base={:#06x}",
                        bar0_chirho,
                        io_base_chirho
                    );
                    enable_io_and_busmaster_chirho(dev_chirho);
                    VIRTIO_IO_BASES_CHIRHO.lock().push(io_base_chirho);
                    crate::net_chirho::probe_virtio_net_io_chirho(io_base_chirho);
                } else {
                    // MMIO transport — log but skip (crashes in UEFI)
                    crate::serial_debug_chirho!(
                        "    VirtIO-net BAR0 is MMIO: {:#010x} — skipping (UEFI unsafe)",
                        bar0_chirho
                    );
                }
            }
        }
    }

    // Skip QEMU MMIO probe if we already found PCI devices — accessing
    // unmapped MMIO addresses (0x1000_1000..) causes page faults in UEFI boot.
    if devices_chirho.is_empty() {
        probe_qemu_mmio_chirho();
    }

    crate::serial_debug_chirho!(
        "VirtIO: scan complete, {} PCI device(s) found",
        devices_chirho.len()
    );

    // P2-003 / P2-004: After block device registration, try to parse ext4
    // superblock and mount the filesystem at /mnt.
    probe_ext4_and_mount_chirho();
}

/// Try to read BAR0 for a VirtIO-blk PCI device, probe MMIO at that address,
/// and if successful read sector 0 and log the first 16 bytes (P2-002).
///
/// If UEFI left BAR0 unconfigured (zero), we program it ourselves via
/// `pci_assign_bar_chirho` before attempting the MMIO probe.
fn probe_and_test_blk_chirho(pci_dev_chirho: &PciDeviceChirho) {
    let bar0_raw_chirho = read_pci_bar_chirho(pci_dev_chirho, 0);
    // Memory BAR: bit 0 == 0; I/O BAR: bit 0 == 1.
    let is_io_bar_chirho = bar0_raw_chirho & 1 != 0;

    if is_io_bar_chirho {
        // ---- I/O port transport (legacy VirtIO PCI) ----
        let io_base_chirho = (bar0_raw_chirho & 0xFFFC) as u16;
        crate::serial_debug_chirho!(
            "    VirtIO-blk BAR0 is I/O port: raw={:#x} base={:#06x}",
            bar0_raw_chirho,
            io_base_chirho
        );

        // Enable I/O space access + bus mastering in PCI command register.
        enable_io_and_busmaster_chirho(pci_dev_chirho);
        VIRTIO_IO_BASES_CHIRHO.lock().push(io_base_chirho);

        match VirtioBlkDeviceChirho::probe_io_chirho(io_base_chirho) {
            Some(blk_dev_chirho) => {
                crate::serial_debug_chirho!(
                    "    VirtIO-blk I/O init OK — capacity {} sectors ({} MiB)",
                    blk_dev_chirho.capacity_sectors_chirho,
                    blk_dev_chirho.capacity_sectors_chirho / 2048
                );
                // Read sector 0 and log the first 16 bytes.
                test_read_sector0_chirho(&blk_dev_chirho);

                // Register the device in the global block device registry.
                use alloc::boxed::Box;
                use alloc::string::String;
                let name_chirho = String::from("vda");
                let reg_result_chirho = crate::block_chirho::BLOCK_REGISTRY_CHIRHO
                    .register_chirho(name_chirho, alloc::sync::Arc::new(blk_dev_chirho));
                match reg_result_chirho {
                    Ok(idx_chirho) => {
                        crate::serial_debug_chirho!(
                            "    Registered VirtIO-blk (I/O) as block device index {}",
                            idx_chirho
                        );
                    }
                    Err(err_chirho) => {
                        crate::serial_debug_chirho!(
                            "    Failed to register VirtIO-blk (I/O): {}",
                            err_chirho
                        );
                    }
                }
            }
            None => {
                crate::serial_debug_chirho!(
                    "    VirtIO-blk I/O probe failed at base={:#06x}",
                    io_base_chirho
                );
            }
        }
        return;
    }

    // ---- MMIO transport (modern VirtIO PCI) ----
    let mut mmio_phys_chirho = (bar0_raw_chirho & 0xFFFF_FFF0) as u64;

    // If BAR0 is zero the UEFI firmware did not assign an MMIO address.
    // Program one ourselves using the PCI BAR bump allocator.
    if mmio_phys_chirho == 0 {
        crate::serial_debug_chirho!(
            "    VirtIO-blk BAR0 is zero — assigning MMIO via pci_assign_bar_chirho"
        );
        match unsafe { pci_assign_bar_chirho(pci_dev_chirho, 0) } {
            Some(assigned_chirho) => {
                mmio_phys_chirho = assigned_chirho;
                crate::serial_debug_chirho!(
                    "    VirtIO-blk BAR0 assigned at phys {:#010x}",
                    mmio_phys_chirho
                );
            }
            None => {
                crate::serial_debug_chirho!(
                    "    VirtIO-blk BAR0 assignment failed — cannot probe"
                );
                return;
            }
        }
    } else {
        // BAR was already configured — ensure bus master + memory space are enabled.
        unsafe { pci_dev_chirho.enable_bus_master_chirho() };
    }

    // Convert physical BAR address to a virtual address using the
    // bootloader-provided physical memory offset.
    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let mmio_virt_chirho = (mmio_phys_chirho + phys_offset_chirho) as usize;

    crate::serial_debug_chirho!(
        "    Probing VirtIO-blk MMIO: phys={:#010x} virt={:#x}...",
        mmio_phys_chirho,
        mmio_virt_chirho
    );

    match VirtioBlkDeviceChirho::probe_mmio_chirho(mmio_virt_chirho) {
        Some(blk_dev_chirho) => {
            crate::serial_debug_chirho!(
                "    VirtIO-blk init OK — capacity {} sectors ({} MiB)",
                blk_dev_chirho.capacity_sectors_chirho,
                blk_dev_chirho.capacity_sectors_chirho / 2048
            );
            // P2-002: read sector 0 and log the first 16 bytes.
            test_read_sector0_chirho(&blk_dev_chirho);

            // Register the device in the global block device registry.
            use alloc::boxed::Box;
            use alloc::string::String;
            let name_chirho = String::from("vda");
            let reg_result_chirho = crate::block_chirho::BLOCK_REGISTRY_CHIRHO
                .register_chirho(name_chirho, alloc::sync::Arc::new(blk_dev_chirho));
            match reg_result_chirho {
                Ok(idx_chirho) => {
                    crate::serial_debug_chirho!(
                        "    Registered VirtIO-blk as block device index {}",
                        idx_chirho
                    );
                }
                Err(err_chirho) => {
                    crate::serial_debug_chirho!(
                        "    Failed to register VirtIO-blk: {}",
                        err_chirho
                    );
                }
            }
        }
        None => {
            crate::serial_debug_chirho!(
                "    VirtIO-blk MMIO probe failed at phys={:#010x} virt={:#x}",
                mmio_phys_chirho,
                mmio_virt_chirho
            );
        }
    }
}

/// Enable I/O space access and bus mastering in the PCI command register.
///
/// Required before accessing VirtIO legacy I/O port BARs.
pub fn enable_io_and_busmaster_chirho(dev_chirho: &PciDeviceChirho) {
    unsafe {
        let cmd_status_chirho = pci_config_read_u32_chirho(
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            0x04, // PCI Command/Status register
        );
        let cmd_chirho = (cmd_status_chirho & 0xFFFF) as u16;
        let status_chirho = ((cmd_status_chirho >> 16) & 0xFFFF) as u16;
        let new_cmd_chirho =
            cmd_chirho | PCI_CMD_IO_SPACE_CHIRHO | PCI_CMD_BUS_MASTER_CHIRHO;
        let new_dword_chirho = (new_cmd_chirho as u32) | ((status_chirho as u32) << 16);
        pci_config_write_u32_chirho(
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            0x04,
            new_dword_chirho,
        );
    }

    crate::serial_debug_chirho!(
        "    PCI: enabled I/O space + bus master for {:02x}:{:02x}.{}",
        dev_chirho.bus_chirho,
        dev_chirho.device_chirho,
        dev_chirho.function_chirho
    );
}

/// Probe QEMU's default VirtIO-MMIO address range for block devices.
fn probe_qemu_mmio_chirho() {
    const QEMU_MMIO_BASE_CHIRHO: usize = 0x1000_1000;
    const QEMU_MMIO_STEP_CHIRHO: usize = 0x1000;
    const QEMU_MMIO_COUNT_CHIRHO: usize = 8;

    for i_chirho in 0..QEMU_MMIO_COUNT_CHIRHO {
        let addr_chirho = QEMU_MMIO_BASE_CHIRHO + i_chirho * QEMU_MMIO_STEP_CHIRHO;
        let transport_chirho = VirtioMmioTransportChirho::new_chirho(addr_chirho);
        if !transport_chirho.check_magic_chirho() {
            continue;
        }
        let dev_id_chirho = transport_chirho.device_id_chirho();
        crate::serial_debug_chirho!(
            "  VirtIO-MMIO at {:#x}: device_type={} vendor={:#x}",
            addr_chirho,
            dev_id_chirho,
            transport_chirho.vendor_id_chirho()
        );
        // Type 2 = block device
        if dev_id_chirho == 2 {
            crate::serial_debug_chirho!("    -> VirtIO-blk (MMIO transport)");
            match VirtioBlkDeviceChirho::probe_mmio_chirho(addr_chirho) {
                Some(blk_dev_chirho) => {
                    crate::serial_debug_chirho!(
                        "    VirtIO-blk MMIO init OK — capacity {} sectors",
                        blk_dev_chirho.capacity_sectors_chirho
                    );
                    test_read_sector0_chirho(&blk_dev_chirho);
                }
                None => {
                    crate::serial_debug_chirho!("    VirtIO-blk MMIO probe failed");
                }
            }
        }
        // P3-001: Type 1 = network device
        if dev_id_chirho == 1 {
            crate::serial_debug_chirho!(
                "    -> VirtIO-net (MMIO transport) at {:#x}, vendor={:#x}",
                addr_chirho,
                transport_chirho.vendor_id_chirho()
            );
        }
    }
}

/// P2-002: Read sector 0 from a VirtIO-blk device and log the first 16 bytes.
/// This should show the MBR/GPT signature if a disk image is attached.
fn test_read_sector0_chirho(dev_chirho: &VirtioBlkDeviceChirho) {
    use crate::block_chirho::{BlockDeviceChirho, SECTOR_SIZE_CHIRHO};

    let mut buf_chirho = vec![0u8; SECTOR_SIZE_CHIRHO];
    match dev_chirho.read_block_chirho(0, &mut buf_chirho) {
        Ok(()) => {
            crate::serial_debug_chirho!("    Sector 0 read OK — first 16 bytes:");
            // Format the first 16 bytes as hex.
            let mut hex_chirho = [0u8; 48]; // 16 * 3 max
            let mut pos_chirho = 0usize;
            for byte_chirho in &buf_chirho[..16] {
                let hi_chirho = HEX_DIGITS_CHIRHO[(*byte_chirho >> 4) as usize];
                let lo_chirho = HEX_DIGITS_CHIRHO[(*byte_chirho & 0x0F) as usize];
                hex_chirho[pos_chirho] = hi_chirho;
                hex_chirho[pos_chirho + 1] = lo_chirho;
                hex_chirho[pos_chirho + 2] = b' ';
                pos_chirho += 3;
            }
            if let Ok(hex_str_chirho) = core::str::from_utf8(&hex_chirho[..pos_chirho]) {
                crate::serial_debug_chirho!("    {}", hex_str_chirho);
            }
            // Check for MBR/GPT signatures.
            if buf_chirho[510] == 0x55 && buf_chirho[511] == 0xAA {
                crate::serial_debug_chirho!("    MBR boot signature detected (0x55AA)");
            }
            if &buf_chirho[0..8] == b"EFI PART" {
                crate::serial_debug_chirho!("    GPT header signature detected");
            }
        }
        Err(err_chirho) => {
            crate::serial_debug_chirho!(
                "    Sector 0 read FAILED (err={}). Expected if no disk attached or \
                 MMIO addresses not identity-mapped.",
                err_chirho
            );
        }
    }
}

/// Hex digit lookup table for formatting bytes.
const HEX_DIGITS_CHIRHO: &[u8; 16] = b"0123456789abcdef";

// ============================================================================
// P2-003 / P2-004: ext4 superblock parsing and VFS mount at /mnt
//
// TODO(fs-dyn-003): Make the ext4 root mount configurable via kernel cmdline.
// Currently the first VirtIO-blk device with a valid ext4 superblock is
// hardcoded as the root filesystem ("/"). The cmdline module already parses
// `root=/dev/vda` style parameters — this should be used to select which
// block device to mount and at which path. The mount point should also be
// configurable (e.g., `rootmount=/` vs `rootmount=/mnt`).
// ============================================================================

/// After block device registration, read the ext4 superblock from the first
/// block device, parse it, log results, and if valid mount the ext4
/// filesystem at /mnt so `resolve_path_chirho` can access it.
fn probe_ext4_and_mount_chirho() {
    use alloc::string::String;
    use crate::block_chirho::BLOCK_REGISTRY_CHIRHO;
    use crate::ext4_chirho::{
        parse_superblock_chirho, parse_group_descs_chirho,
        Ext4MountChirho, SUPERBLOCK_OFFSET_CHIRHO,
    };

    let device_count_chirho = BLOCK_REGISTRY_CHIRHO.count_chirho();
    crate::serial_debug_chirho!("[EXT4] Block device count: {}", device_count_chirho);
    if device_count_chirho == 0 {
        crate::serial_debug_chirho!("[EXT4] No block devices registered, skipping ext4 probe");
        return;
    }

    crate::serial_debug_chirho!(
        "[EXT4] Probing device 0 for ext4 superblock (byte offset {})...",
        SUPERBLOCK_OFFSET_CHIRHO
    );

    // P2-003: Read sector 2 (byte offset 1024 = ext4 superblock location).
    // The superblock is at byte offset 1024, which spans sectors 2 and 3
    // (each sector = 512 bytes).
    let sb_start_sector_chirho = SUPERBLOCK_OFFSET_CHIRHO / 512; // = 2
    let mut sb_data_chirho = vec![0u8; 1024];

    for i_chirho in 0..2u64 {
        if let Err(err_chirho) = BLOCK_REGISTRY_CHIRHO.read_block_chirho(
            0,
            sb_start_sector_chirho + i_chirho,
            &mut sb_data_chirho[(i_chirho as usize * 512)..((i_chirho as usize + 1) * 512)],
        ) {
            crate::serial_debug_chirho!(
                "[EXT4] Failed to read superblock sector {} (err={})",
                sb_start_sector_chirho + i_chirho,
                err_chirho
            );
            return;
        }
    }

    // Parse the superblock.
    let sb_chirho = match parse_superblock_chirho(&sb_data_chirho) {
        Some(sb_chirho) => sb_chirho,
        None => {
            crate::serial_debug_chirho!(
                "[EXT4] No valid ext4 superblock found (magic mismatch)"
            );
            return;
        }
    };

    // Log superblock info (P2-003 deliverable).
    let block_size_chirho = sb_chirho.block_size_chirho();
    let total_blocks_chirho = sb_chirho.total_blocks_chirho();
    let inodes_count_chirho = sb_chirho.s_inodes_count_chirho;
    let volume_name_chirho = sb_chirho.volume_name_chirho();
    let free_blocks_chirho = sb_chirho.free_blocks_chirho();
    let free_inodes_chirho = sb_chirho.s_free_inodes_count_chirho;
    let bg_count_chirho = sb_chirho.block_group_count_chirho();

    crate::serial_debug_chirho!("[EXT4] === Superblock parsed successfully ===");
    crate::serial_debug_chirho!(
        "[EXT4]   Block size:     {} bytes",
        block_size_chirho
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Total blocks:   {} ({} MiB)",
        total_blocks_chirho,
        (total_blocks_chirho * block_size_chirho as u64) / (1024 * 1024)
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Free blocks:    {}",
        free_blocks_chirho
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Total inodes:   {}",
        inodes_count_chirho
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Free inodes:    {}",
        free_inodes_chirho
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Block groups:   {}",
        bg_count_chirho
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Volume name:    \"{}\"",
        if volume_name_chirho.is_empty() { "<none>" } else { &volume_name_chirho }
    );
    crate::serial_debug_chirho!(
        "[EXT4]   Features:       extents={} 64bit={} journal={}",
        sb_chirho.has_extents_chirho(),
        sb_chirho.has_64bit_chirho(),
        sb_chirho.has_journal_chirho()
    );

    // P2-004: Create Ext4MountChirho and mount at /mnt.
    // Read group descriptors from the block following the superblock.
    let gd_size_chirho = sb_chirho.group_desc_size_chirho();
    let gdt_block_chirho: u64 = if block_size_chirho == 1024 { 2 } else { 1 };
    let gdt_bytes_chirho = bg_count_chirho as usize * gd_size_chirho as usize;
    let gdt_blocks_needed_chirho =
        (gdt_bytes_chirho + block_size_chirho as usize - 1) / block_size_chirho as usize;

    let mut gdt_data_chirho = Vec::new();
    let sectors_per_block_chirho = block_size_chirho as u64 / 512;

    for blk_chirho in 0..gdt_blocks_needed_chirho as u64 {
        let abs_block_chirho = gdt_block_chirho + blk_chirho;
        let start_sec_chirho = abs_block_chirho * sectors_per_block_chirho;

        let mut buf_chirho = vec![0u8; block_size_chirho as usize];
        let mut read_ok_chirho = true;
        for s_chirho in 0..sectors_per_block_chirho {
            let off_chirho = (s_chirho as usize) * 512;
            if BLOCK_REGISTRY_CHIRHO
                .read_block_chirho(0, start_sec_chirho + s_chirho, &mut buf_chirho[off_chirho..off_chirho + 512])
                .is_err()
            {
                read_ok_chirho = false;
                break;
            }
        }
        if !read_ok_chirho {
            crate::serial_debug_chirho!("[EXT4] Failed to read GDT block, aborting mount");
            return;
        }
        gdt_data_chirho.extend_from_slice(&buf_chirho);
    }

    let group_descs_chirho =
        parse_group_descs_chirho(&gdt_data_chirho, bg_count_chirho, gd_size_chirho);

    crate::serial_debug_chirho!(
        "[EXT4] Read {} block group descriptors",
        group_descs_chirho.len()
    );

    // Build the Ext4MountChirho.
    let ext4_mount_chirho = Ext4MountChirho {
        sb_chirho,
        group_descs_chirho,
        block_size_chirho,
        device_id_chirho: 0,
        mode_chirho: crate::ext4_chirho::MountModeChirho::ReadOnlyChirho,
    };

    // Create the VFS superblock for ext4.
    let ext4_vfs_sb_chirho = crate::ext4_chirho::mount_ext4_vfs_chirho(ext4_mount_chirho);

    // Mount ext4 as the root filesystem ("/") so Alpine paths work
    // natively: /bin/busybox, /usr/bin/sqlite3, /lib/ld-musl-x86_64.so.1.
    // Also mount at /mnt for backward compatibility with existing scripts.
    {
        let mut mounts_chirho = crate::fs_chirho::MOUNT_TABLE_CHIRHO.lock();
        // Root mount — ext4 becomes the primary filesystem
        mounts_chirho.push(crate::fs_chirho::MountPointChirho {
            path_chirho: String::from("/"),
            superblock_chirho: ext4_vfs_sb_chirho.clone(),
        });
        // Backward compat: /mnt also points to ext4 root
        mounts_chirho.push(crate::fs_chirho::MountPointChirho {
            path_chirho: String::from("/mnt"),
            superblock_chirho: ext4_vfs_sb_chirho,
        });
    }

    // Create /mnt directory in root tmpfs for backward compatibility.
    {
        match crate::fs_chirho::resolve_parent_live_chirho("/mnt") {
            Ok((parent_chirho, name_chirho)) => {
                let parent_guard_chirho = parent_chirho.lock();
                if let Err(mkdir_error_chirho) = parent_guard_chirho.ops_chirho.mkdir_chirho(
                    &parent_guard_chirho,
                    &name_chirho,
                    0o755,
                ) {
                    crate::serial_debug_chirho!(
                        "[EXT4] verify mount: mkdir /mnt shim failed: {}",
                        mkdir_error_chirho
                    );
                }
            }
            Err(_) => {}
        }
    }

    crate::serial_debug_chirho!(
        "[EXT4] ext4 filesystem mounted at / and /mnt ({} block groups, {} blocks)",
        bg_count_chirho,
        total_blocks_chirho
    );

    // P2-006 / P2-007: Verify that `ls /mnt` works through the VFS by
    // resolving /mnt (the ext4 root) and listing its directory entries.
    // This confirms the full path: VFS mount table -> ext4 InodeOps ->
    // ext4 readdir -> VFS FileOps -> getdents64 pipeline is operational.
    verify_ext4_mount_chirho();
}

/// Verify the ext4 mount at /mnt by listing root directory contents.
///
/// Called immediately after mount to confirm the VFS-to-ext4 pipeline works.
/// Exercises: resolve_path_chirho -> Ext4InodeOps::lookup -> Ext4FileOps::readdir.
fn verify_ext4_mount_chirho() {
    use alloc::string::String;
    use alloc::vec::Vec;

    match crate::fs_chirho::resolve_path_chirho("/mnt") {
        Ok((inode_chirho, file_ops_chirho)) => {
            crate::serial_debug_chirho!("[EXT4] P2-006: /mnt resolved successfully");

            // Create a temporary File to call readdir through the VFS file ops.
            let mut file_chirho = crate::vfs_chirho::FileChirho {
                inode_chirho: inode_chirho,
                pos_chirho: 0,
                flags_chirho: crate::vfs_chirho::O_RDONLY_CHIRHO | crate::vfs_chirho::O_DIRECTORY_CHIRHO,
                ops_chirho: file_ops_chirho,
            };

            let mut entry_names_chirho: Vec<String> = Vec::new();
            if let Err(readdir_error_chirho) = file_ops_chirho.readdir_chirho(
                &mut file_chirho,
                &mut |name_chirho: &str, _ino_chirho: u64, _dt_chirho: u8| -> bool {
                    entry_names_chirho.push(String::from(name_chirho));
                    true
                },
            ) {
                crate::serial_debug_chirho!(
                    "[EXT4] P2-007: /mnt readdir failed: {}",
                    readdir_error_chirho
                );
            }

            if entry_names_chirho.is_empty() {
                crate::serial_debug_chirho!(
                    "[EXT4] P2-007: WARNING: /mnt readdir returned 0 entries"
                );
            } else {
                crate::serial_debug_chirho!(
                    "[EXT4] P2-007: ls /mnt => {} entries: {:?}",
                    entry_names_chirho.len(),
                    &entry_names_chirho[..core::cmp::min(entry_names_chirho.len(), 20)]
                );
            }
        }
        Err(errno_chirho) => {
            crate::serial_debug_chirho!(
                "[EXT4] P2-006: ERROR: failed to resolve /mnt (errno={})",
                errno_chirho
            );
        }
    }
}
