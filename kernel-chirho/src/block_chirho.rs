// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Block I/O layer for the Lineluya kernel (Phase 5).
//!
//! Provides the [`BlockDeviceChirho`] trait for abstracting block-oriented
//! storage devices, a [`BioChirho`] struct representing individual I/O
//! requests, a [`RequestQueueChirho`] for queuing those requests, and a
//! global [`BlockDeviceRegistryChirho`] for tracking registered devices.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Constants
// ============================================================================

/// Standard sector size in bytes (matching Linux's SECTOR_SIZE).
pub const SECTOR_SIZE_CHIRHO: usize = 512;

/// Maximum number of block devices that can be registered.
const MAX_BLOCK_DEVICES_CHIRHO: usize = 16;

// ============================================================================
// BioDirectionChirho -- direction of a block I/O request
// ============================================================================

/// Direction of a block I/O operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioDirectionChirho {
    /// Read data from the device into the buffer.
    ReadChirho,
    /// Write data from the buffer to the device.
    WriteChirho,
}

// ============================================================================
// BioChirho -- block I/O request
// ============================================================================

/// A single block I/O request.
///
/// Encapsulates the sector range, a data buffer, and the direction of the
/// transfer.  Callers push `BioChirho` instances into a [`RequestQueueChirho`]
/// for the device driver to process.
pub struct BioChirho {
    /// Starting sector number on the device.
    pub sector_chirho: u64,
    /// Number of sectors to transfer.
    pub count_chirho: u64,
    /// Data buffer.  For reads, the driver fills it; for writes, the driver
    /// reads from it.
    pub buffer_chirho: Vec<u8>,
    /// Direction of the transfer.
    pub direction_chirho: BioDirectionChirho,
}

impl BioChirho {
    /// Create a new block I/O request.
    pub fn new_chirho(
        sector_chirho: u64,
        count_chirho: u64,
        buffer_chirho: Vec<u8>,
        direction_chirho: BioDirectionChirho,
    ) -> Self {
        Self {
            sector_chirho,
            count_chirho,
            buffer_chirho,
            direction_chirho,
        }
    }

    /// Total number of bytes this request covers.
    pub fn byte_count_chirho(&self) -> usize {
        (self.count_chirho as usize) * SECTOR_SIZE_CHIRHO
    }
}

// ============================================================================
// BlockDeviceChirho -- trait for block storage devices
// ============================================================================

/// Trait that all block storage device drivers must implement.
///
/// Provides the minimal interface required by the VFS and block I/O layer to
/// perform sector-level reads and writes.
pub trait BlockDeviceChirho: Send + Sync {
    /// Read a single block (sector) from the device.
    ///
    /// `block_nr_chirho` is the zero-based sector index.
    /// `buf_chirho` must be at least [`SECTOR_SIZE_CHIRHO`] bytes long.
    ///
    /// Returns `Ok(())` on success or an errno-style negative error code.
    fn read_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &mut [u8],
    ) -> Result<(), i64>;

    /// Write a single block (sector) to the device.
    ///
    /// `block_nr_chirho` is the zero-based sector index.
    /// `buf_chirho` must be at least [`SECTOR_SIZE_CHIRHO`] bytes long.
    ///
    /// Returns `Ok(())` on success or an errno-style negative error code.
    fn write_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &[u8],
    ) -> Result<(), i64>;

    /// The size of one block (sector) in bytes.
    ///
    /// Typically [`SECTOR_SIZE_CHIRHO`] (512), but some devices use 4096.
    fn block_size_chirho(&self) -> usize;

    /// Total number of blocks (sectors) on the device.
    fn total_blocks_chirho(&self) -> u64;
}

// ============================================================================
// RequestQueueChirho -- queue of BioChirho requests
// ============================================================================

/// A FIFO queue of [`BioChirho`] requests for a block device.
///
/// Device drivers drain this queue to service pending I/O.
pub struct RequestQueueChirho {
    /// The pending requests, protected by a spin-lock for interrupt safety.
    queue_chirho: Mutex<VecDeque<BioChirho>>,
}

impl RequestQueueChirho {
    /// Create an empty request queue.
    pub const fn new_chirho() -> Self {
        Self {
            queue_chirho: Mutex::new(VecDeque::new()),
        }
    }

    /// Enqueue a new block I/O request.
    pub fn submit_chirho(&self, bio_chirho: BioChirho) {
        self.queue_chirho.lock().push_back(bio_chirho);
    }

    /// Dequeue the next pending request, if any.
    pub fn next_chirho(&self) -> Option<BioChirho> {
        self.queue_chirho.lock().pop_front()
    }

    /// Number of requests currently queued.
    pub fn len_chirho(&self) -> usize {
        self.queue_chirho.lock().len()
    }

    /// Whether the queue is empty.
    pub fn is_empty_chirho(&self) -> bool {
        self.queue_chirho.lock().is_empty()
    }
}

// ============================================================================
// BlockDeviceRegistryChirho -- global registry of block devices
// ============================================================================

/// Entry in the block device registry.
struct BlockDeviceEntryChirho {
    /// Human-readable name (e.g. "sda", "vda").
    name_chirho: String,
    /// The device driver.
    device_chirho: Box<dyn BlockDeviceChirho>,
    /// Per-device request queue.
    queue_chirho: RequestQueueChirho,
}

/// Global registry of block devices.
///
/// Device drivers call [`register_chirho`](BlockDeviceRegistryChirho::register_chirho)
/// during initialisation.  The VFS / filesystem code looks up devices by name
/// or index.
pub struct BlockDeviceRegistryChirho {
    devices_chirho: Mutex<Vec<BlockDeviceEntryChirho>>,
}

impl BlockDeviceRegistryChirho {
    /// Create an empty registry.
    const fn new_chirho() -> Self {
        Self {
            devices_chirho: Mutex::new(Vec::new()),
        }
    }

    /// Register a new block device.
    ///
    /// Returns an index that can be used with [`get_chirho`](Self::get_chirho),
    /// or `Err(-ENOMEM)` if the maximum number of devices has been reached.
    pub fn register_chirho(
        &self,
        name_chirho: String,
        device_chirho: Box<dyn BlockDeviceChirho>,
    ) -> Result<usize, i64> {
        let mut devices_chirho = self.devices_chirho.lock();
        if devices_chirho.len() >= MAX_BLOCK_DEVICES_CHIRHO {
            return Err(-12); // ENOMEM
        }
        let index_chirho = devices_chirho.len();
        devices_chirho.push(BlockDeviceEntryChirho {
            name_chirho,
            device_chirho,
            queue_chirho: RequestQueueChirho::new_chirho(),
        });
        Ok(index_chirho)
    }

    /// Return the number of registered devices.
    pub fn count_chirho(&self) -> usize {
        self.devices_chirho.lock().len()
    }
}

/// The single, global block device registry.
pub static BLOCK_REGISTRY_CHIRHO: BlockDeviceRegistryChirho =
    BlockDeviceRegistryChirho::new_chirho();
