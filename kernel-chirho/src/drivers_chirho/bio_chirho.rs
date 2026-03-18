// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Block I/O integration layer for the Lineluya kernel (A4-002).
//!
//! Builds on top of [`block_chirho`] to provide:
//!   - A higher-level block device registry with mount-point awareness
//!   - Bio request batching (submit multiple sectors at once)
//!   - VFS integration: mapping mount paths to block devices
//!   - A simple FIFO request queue (no I/O elevator yet)
//!
//! This module bridges the gap between the raw block device trait
//! ([`block_chirho::BlockDeviceChirho`]) and the VFS layer ([`vfs_chirho`]),
//! enabling filesystems to be mounted on block devices.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block_chirho::{
    BioChirho, BioDirectionChirho, BlockDeviceChirho, RequestQueueChirho, SECTOR_SIZE_CHIRHO,
};

// ============================================================================
// Error codes (Linux errno values)
// ============================================================================

/// No such device.
const ENODEV_CHIRHO: i64 = -19;
/// Invalid argument.
const EINVAL_CHIRHO: i64 = -22;
/// No such device or address.
const ENXIO_CHIRHO: i64 = -6;
/// I/O error.
const EIO_CHIRHO: i64 = -5;
/// Resource busy.
const EBUSY_CHIRHO: i64 = -16;
/// No space left on device (registry full).
const ENOSPC_CHIRHO: i64 = -28;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of block devices in the enhanced registry.
const MAX_BIO_DEVICES_CHIRHO: usize = 32;

/// Maximum number of mount-point bindings.
const MAX_MOUNT_BINDINGS_CHIRHO: usize = 32;

/// Maximum sectors per single bio request batch.
const MAX_SECTORS_PER_BIO_CHIRHO: u64 = 256;

// ============================================================================
// BioStatusChirho — status of a bio request
// ============================================================================

/// Completion status of a block I/O request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioStatusChirho {
    /// Request has been created but not yet submitted.
    PendingChirho,
    /// Request is currently being processed by the device.
    InFlightChirho,
    /// Request completed successfully.
    CompleteChirho,
    /// Request failed with an error code.
    ErrorChirho(i64),
}

// ============================================================================
// BioRequestChirho — enhanced I/O request with status tracking
// ============================================================================

/// An enhanced block I/O request that wraps [`BioChirho`] with additional
/// metadata for queue management and completion tracking.
pub struct BioRequestChirho {
    /// The underlying bio request.
    pub bio_chirho: BioChirho,
    /// Device index in the registry this request targets.
    pub device_idx_chirho: usize,
    /// Current status of this request.
    pub status_chirho: BioStatusChirho,
    /// Monotonic request ID for ordering.
    pub request_id_chirho: u64,
}

impl BioRequestChirho {
    /// Create a new bio request targeting a specific device.
    pub fn new_chirho(
        device_idx_chirho: usize,
        sector_chirho: u64,
        count_chirho: u64,
        buffer_chirho: Vec<u8>,
        direction_chirho: BioDirectionChirho,
        request_id_chirho: u64,
    ) -> Self {
        Self {
            bio_chirho: BioChirho::new_chirho(
                sector_chirho,
                count_chirho,
                buffer_chirho,
                direction_chirho,
            ),
            device_idx_chirho,
            status_chirho: BioStatusChirho::PendingChirho,
            request_id_chirho,
        }
    }
}

// ============================================================================
// BioQueueChirho — FIFO request queue with status tracking
// ============================================================================

/// A simple FIFO request queue for block I/O.
///
/// Requests are submitted and drained in order (no elevator scheduling yet).
/// This is intentionally simple; a deadline or CFQ scheduler can be layered
/// on top later.
pub struct BioQueueChirho {
    /// Pending requests waiting to be dispatched.
    pending_chirho: Mutex<VecDeque<BioRequestChirho>>,
    /// Counter for assigning monotonic request IDs.
    next_id_chirho: Mutex<u64>,
}

impl BioQueueChirho {
    /// Create a new, empty bio queue.
    pub const fn new_chirho() -> Self {
        Self {
            pending_chirho: Mutex::new(VecDeque::new()),
            next_id_chirho: Mutex::new(0),
        }
    }

    /// Allocate the next monotonic request ID.
    fn next_request_id_chirho(&self) -> u64 {
        let mut id_chirho = self.next_id_chirho.lock();
        let current_chirho = *id_chirho;
        *id_chirho = current_chirho.wrapping_add(1);
        current_chirho
    }

    /// Submit a read request.
    pub fn submit_read_chirho(
        &self,
        device_idx_chirho: usize,
        sector_chirho: u64,
        count_chirho: u64,
    ) -> u64 {
        let id_chirho = self.next_request_id_chirho();
        let buf_size_chirho = (count_chirho as usize) * SECTOR_SIZE_CHIRHO;
        let request_chirho = BioRequestChirho::new_chirho(
            device_idx_chirho,
            sector_chirho,
            count_chirho,
            vec![0u8; buf_size_chirho],
            BioDirectionChirho::ReadChirho,
            id_chirho,
        );
        self.pending_chirho.lock().push_back(request_chirho);
        id_chirho
    }

    /// Submit a write request with data.
    pub fn submit_write_chirho(
        &self,
        device_idx_chirho: usize,
        sector_chirho: u64,
        data_chirho: Vec<u8>,
    ) -> u64 {
        let id_chirho = self.next_request_id_chirho();
        let count_chirho = (data_chirho.len() / SECTOR_SIZE_CHIRHO) as u64;
        let request_chirho = BioRequestChirho::new_chirho(
            device_idx_chirho,
            sector_chirho,
            count_chirho,
            data_chirho,
            BioDirectionChirho::WriteChirho,
            id_chirho,
        );
        self.pending_chirho.lock().push_back(request_chirho);
        id_chirho
    }

    /// Dequeue the next pending request, if any.
    pub fn dequeue_chirho(&self) -> Option<BioRequestChirho> {
        self.pending_chirho.lock().pop_front()
    }

    /// Number of pending requests.
    pub fn pending_count_chirho(&self) -> usize {
        self.pending_chirho.lock().len()
    }

    /// Whether the queue is empty.
    pub fn is_empty_chirho(&self) -> bool {
        self.pending_chirho.lock().is_empty()
    }
}

// ============================================================================
// MountBindingChirho — maps a path to a block device
// ============================================================================

/// A binding between a VFS mount-point path and a registered block device.
#[allow(dead_code)]
struct MountBindingChirho {
    /// The VFS mount path (e.g. "/", "/home", "/mnt/data").
    mount_path_chirho: String,
    /// Index into the `BioDeviceRegistryChirho` device list.
    device_idx_chirho: usize,
    /// Whether this mount is read-only.
    readonly_chirho: bool,
    /// Filesystem type name (e.g. "ext4", "fat32").
    fs_type_chirho: String,
}

// ============================================================================
// BioDeviceEntryChirho — enhanced device entry
// ============================================================================

/// An entry in the enhanced block device registry.
struct BioDeviceEntryChirho {
    /// Human-readable device name (e.g. "vda", "sda").
    name_chirho: String,
    /// The block device driver.
    device_chirho: Box<dyn BlockDeviceChirho>,
    /// Per-device request queue.
    queue_chirho: RequestQueueChirho,
    /// Whether this device is currently mounted.
    mounted_chirho: bool,
}

// ============================================================================
// BioDeviceRegistryChirho — enhanced block device registry
// ============================================================================

/// Enhanced block device registry with mount-point tracking and bio queue
/// integration.
///
/// This extends the basic `BlockDeviceRegistryChirho` with:
///   - Mount-point bindings (path -> device mapping)
///   - Synchronous read/write helpers that dispatch through the device trait
///   - Multi-sector bio request support
pub struct BioDeviceRegistryChirho {
    /// Registered block devices.
    devices_chirho: Mutex<Vec<BioDeviceEntryChirho>>,
    /// Mount-point bindings.
    mounts_chirho: Mutex<Vec<MountBindingChirho>>,
}

impl BioDeviceRegistryChirho {
    /// Create an empty registry.
    const fn new_chirho() -> Self {
        Self {
            devices_chirho: Mutex::new(Vec::new()),
            mounts_chirho: Mutex::new(Vec::new()),
        }
    }

    /// Register a new block device.
    ///
    /// Returns the device index on success, or a negative errno on failure.
    pub fn register_chirho(
        &self,
        name_chirho: String,
        device_chirho: Box<dyn BlockDeviceChirho>,
    ) -> Result<usize, i64> {
        let mut devices_chirho = self.devices_chirho.lock();
        if devices_chirho.len() >= MAX_BIO_DEVICES_CHIRHO {
            return Err(ENOSPC_CHIRHO);
        }
        let idx_chirho = devices_chirho.len();
        devices_chirho.push(BioDeviceEntryChirho {
            name_chirho,
            device_chirho,
            queue_chirho: RequestQueueChirho::new_chirho(),
            mounted_chirho: false,
        });
        Ok(idx_chirho)
    }

    /// Unregister a block device by index.
    ///
    /// Fails with `EBUSY` if the device is currently mounted.
    pub fn unregister_chirho(&self, idx_chirho: usize) -> Result<(), i64> {
        let mut devices_chirho = self.devices_chirho.lock();
        if idx_chirho >= devices_chirho.len() {
            return Err(ENODEV_CHIRHO);
        }
        if devices_chirho[idx_chirho].mounted_chirho {
            return Err(EBUSY_CHIRHO);
        }
        // Mark the slot as removed by replacing with a tombstone.
        // In a real kernel we'd use a slab allocator or Option<> slots.
        // For now just prevent double-use via the mounted flag.
        devices_chirho.remove(idx_chirho);
        Ok(())
    }

    /// Look up a device index by name.
    pub fn find_by_name_chirho(&self, name_chirho: &str) -> Option<usize> {
        let devices_chirho = self.devices_chirho.lock();
        devices_chirho
            .iter()
            .position(|entry_chirho| entry_chirho.name_chirho == name_chirho)
    }

    /// Return the number of registered devices.
    pub fn device_count_chirho(&self) -> usize {
        self.devices_chirho.lock().len()
    }

    /// Bind a mount-point path to a device index.
    ///
    /// This is called by the VFS `mount` path to associate a filesystem
    /// mount with its underlying block device.
    pub fn mount_chirho(
        &self,
        path_chirho: String,
        device_idx_chirho: usize,
        fs_type_chirho: String,
        readonly_chirho: bool,
    ) -> Result<(), i64> {
        // Validate the device index.
        {
            let mut devices_chirho = self.devices_chirho.lock();
            if device_idx_chirho >= devices_chirho.len() {
                return Err(ENODEV_CHIRHO);
            }
            if devices_chirho[device_idx_chirho].mounted_chirho {
                return Err(EBUSY_CHIRHO);
            }
            devices_chirho[device_idx_chirho].mounted_chirho = true;
        }

        let mut mounts_chirho = self.mounts_chirho.lock();
        if mounts_chirho.len() >= MAX_MOUNT_BINDINGS_CHIRHO {
            return Err(ENOSPC_CHIRHO);
        }
        mounts_chirho.push(MountBindingChirho {
            mount_path_chirho: path_chirho,
            device_idx_chirho,
            readonly_chirho,
            fs_type_chirho,
        });
        Ok(())
    }

    /// Unbind a mount-point, returning `Err` if not found.
    pub fn unmount_chirho(&self, path_chirho: &str) -> Result<(), i64> {
        let mut mounts_chirho = self.mounts_chirho.lock();
        let pos_chirho = mounts_chirho
            .iter()
            .position(|m_chirho| m_chirho.mount_path_chirho == path_chirho);

        match pos_chirho {
            Some(idx_chirho) => {
                let device_idx_chirho = mounts_chirho[idx_chirho].device_idx_chirho;
                mounts_chirho.remove(idx_chirho);
                // Mark device as no longer mounted.
                let mut devices_chirho = self.devices_chirho.lock();
                if device_idx_chirho < devices_chirho.len() {
                    devices_chirho[device_idx_chirho].mounted_chirho = false;
                }
                Ok(())
            }
            None => Err(EINVAL_CHIRHO),
        }
    }

    /// Find the device index for a given mount path.
    pub fn device_for_mount_chirho(&self, path_chirho: &str) -> Option<usize> {
        let mounts_chirho = self.mounts_chirho.lock();
        mounts_chirho
            .iter()
            .find(|m_chirho| m_chirho.mount_path_chirho == path_chirho)
            .map(|m_chirho| m_chirho.device_idx_chirho)
    }

    /// Synchronous sector read through the registered device.
    ///
    /// Reads `count_chirho` sectors starting at `sector_chirho` from device
    /// `device_idx_chirho` into the returned buffer.
    pub fn read_sectors_chirho(
        &self,
        device_idx_chirho: usize,
        sector_chirho: u64,
        count_chirho: u64,
    ) -> Result<Vec<u8>, i64> {
        if count_chirho == 0 || count_chirho > MAX_SECTORS_PER_BIO_CHIRHO {
            return Err(EINVAL_CHIRHO);
        }

        let devices_chirho = self.devices_chirho.lock();
        let entry_chirho = devices_chirho
            .get(device_idx_chirho)
            .ok_or(ENXIO_CHIRHO)?;

        let block_size_chirho = entry_chirho.device_chirho.block_size_chirho();
        let total_blocks_chirho = entry_chirho.device_chirho.total_blocks_chirho();

        if sector_chirho + count_chirho > total_blocks_chirho {
            return Err(EINVAL_CHIRHO);
        }

        let mut result_chirho = Vec::with_capacity((count_chirho as usize) * block_size_chirho);

        for i_chirho in 0..count_chirho {
            let mut sector_buf_chirho = vec![0u8; block_size_chirho];
            entry_chirho
                .device_chirho
                .read_block_chirho(sector_chirho + i_chirho, &mut sector_buf_chirho)
                .map_err(|_| EIO_CHIRHO)?;
            result_chirho.extend_from_slice(&sector_buf_chirho);
        }

        Ok(result_chirho)
    }

    /// Synchronous sector write through the registered device.
    ///
    /// Writes `data_chirho` starting at `sector_chirho` on device
    /// `device_idx_chirho`.  The data length must be a multiple of the
    /// device's block size.
    pub fn write_sectors_chirho(
        &self,
        device_idx_chirho: usize,
        sector_chirho: u64,
        data_chirho: &[u8],
    ) -> Result<(), i64> {
        let devices_chirho = self.devices_chirho.lock();
        let entry_chirho = devices_chirho
            .get(device_idx_chirho)
            .ok_or(ENXIO_CHIRHO)?;

        let block_size_chirho = entry_chirho.device_chirho.block_size_chirho();
        if data_chirho.len() % block_size_chirho != 0 {
            return Err(EINVAL_CHIRHO);
        }

        let count_chirho = (data_chirho.len() / block_size_chirho) as u64;
        if count_chirho > MAX_SECTORS_PER_BIO_CHIRHO {
            return Err(EINVAL_CHIRHO);
        }

        let total_blocks_chirho = entry_chirho.device_chirho.total_blocks_chirho();
        if sector_chirho + count_chirho > total_blocks_chirho {
            return Err(EINVAL_CHIRHO);
        }

        for i_chirho in 0..count_chirho {
            let offset_chirho = (i_chirho as usize) * block_size_chirho;
            let slice_chirho = &data_chirho[offset_chirho..offset_chirho + block_size_chirho];
            entry_chirho
                .device_chirho
                .write_block_chirho(sector_chirho + i_chirho, slice_chirho)
                .map_err(|_| EIO_CHIRHO)?;
        }

        Ok(())
    }

    /// Submit a bio request to the per-device request queue.
    ///
    /// The device driver is expected to drain the queue asynchronously (or
    /// a worker thread can call [`drain_queue_chirho`]).
    pub fn submit_bio_chirho(
        &self,
        device_idx_chirho: usize,
        bio_chirho: BioChirho,
    ) -> Result<(), i64> {
        let devices_chirho = self.devices_chirho.lock();
        let entry_chirho = devices_chirho
            .get(device_idx_chirho)
            .ok_or(ENXIO_CHIRHO)?;
        entry_chirho.queue_chirho.submit_chirho(bio_chirho);
        Ok(())
    }

    /// Drain and process all pending bio requests for a device.
    ///
    /// This is a synchronous helper intended for polling-mode drivers.
    /// Interrupt-driven drivers would process requests in their ISR.
    pub fn drain_queue_chirho(&self, device_idx_chirho: usize) -> Result<usize, i64> {
        let devices_chirho = self.devices_chirho.lock();
        let entry_chirho = devices_chirho
            .get(device_idx_chirho)
            .ok_or(ENXIO_CHIRHO)?;

        let mut processed_chirho: usize = 0;

        while let Some(mut bio_chirho) = entry_chirho.queue_chirho.next_chirho() {
            let result_chirho: Result<(), i64> = match bio_chirho.direction_chirho {
                BioDirectionChirho::ReadChirho => {
                    // Read each sector into the buffer.
                    for i_chirho in 0..bio_chirho.count_chirho {
                        let offset_chirho =
                            (i_chirho as usize) * entry_chirho.device_chirho.block_size_chirho();
                        let end_chirho =
                            offset_chirho + entry_chirho.device_chirho.block_size_chirho();
                        if end_chirho > bio_chirho.buffer_chirho.len() {
                            break;
                        }
                        let _ = entry_chirho.device_chirho.read_block_chirho(
                            bio_chirho.sector_chirho + i_chirho,
                            &mut bio_chirho.buffer_chirho[offset_chirho..end_chirho],
                        );
                    }
                    Ok(())
                }
                BioDirectionChirho::WriteChirho => {
                    for i_chirho in 0..bio_chirho.count_chirho {
                        let offset_chirho =
                            (i_chirho as usize) * entry_chirho.device_chirho.block_size_chirho();
                        let end_chirho =
                            offset_chirho + entry_chirho.device_chirho.block_size_chirho();
                        if end_chirho > bio_chirho.buffer_chirho.len() {
                            break;
                        }
                        let _ = entry_chirho.device_chirho.write_block_chirho(
                            bio_chirho.sector_chirho + i_chirho,
                            &bio_chirho.buffer_chirho[offset_chirho..end_chirho],
                        );
                    }
                    Ok(())
                }
            };

            if result_chirho.is_ok() {
                processed_chirho += 1;
            }
        }

        Ok(processed_chirho)
    }

    /// Return the number of active mount bindings.
    pub fn mount_count_chirho(&self) -> usize {
        self.mounts_chirho.lock().len()
    }
}

// ============================================================================
// Global instances
// ============================================================================

/// The global enhanced block device registry.
pub static BIO_REGISTRY_CHIRHO: BioDeviceRegistryChirho = BioDeviceRegistryChirho::new_chirho();

/// The global bio request queue (device-independent staging area).
pub static BIO_QUEUE_CHIRHO: BioQueueChirho = BioQueueChirho::new_chirho();

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the block I/O integration layer.
///
/// Called during kernel boot after the block device subsystem and VFS are
/// ready.  Currently just logs readiness; actual device registration happens
/// when drivers probe hardware.
#[allow(dead_code)]
pub fn init_bio_chirho() {
    crate::serial_debug_chirho!("BIO: block I/O integration layer initialized");
    crate::serial_debug_chirho!(
        "BIO: {} device(s) registered, {} mount(s) active",
        BIO_REGISTRY_CHIRHO.device_count_chirho(),
        BIO_REGISTRY_CHIRHO.mount_count_chirho(),
    );
}
