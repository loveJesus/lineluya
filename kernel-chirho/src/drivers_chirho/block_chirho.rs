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
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Constants
// ============================================================================

/// Standard sector size in bytes (matching Linux's SECTOR_SIZE).
pub const SECTOR_SIZE_CHIRHO: usize = 512;

/// Maximum number of block devices that can be registered.
const MAX_BLOCK_DEVICES_CHIRHO: usize = 128;

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

/// Placeholder device used to fill sparse registry slots.
struct UnboundBlockDeviceChirho;

impl BlockDeviceChirho for UnboundBlockDeviceChirho {
    fn read_block_chirho(
        &self,
        _block_nr_chirho: u64,
        _buf_chirho: &mut [u8],
    ) -> Result<(), i64> {
        Err(-crate::syscall_chirho::ENODEV_CHIRHO)
    }

    fn write_block_chirho(
        &self,
        _block_nr_chirho: u64,
        _buf_chirho: &[u8],
    ) -> Result<(), i64> {
        Err(-crate::syscall_chirho::ENODEV_CHIRHO)
    }

    fn block_size_chirho(&self) -> usize { SECTOR_SIZE_CHIRHO }

    fn total_blocks_chirho(&self) -> u64 { 0 }
}

/// Block device adapter that forwards sector reads and writes through an open
/// loop-device file.
struct LoopFileBlockDeviceChirho {
    backing_file_chirho: Arc<Mutex<crate::vfs_chirho::FileChirho>>,
}

impl BlockDeviceChirho for LoopFileBlockDeviceChirho {
    fn read_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &mut [u8],
    ) -> Result<(), i64> {
        let mut file_guard_chirho = self.backing_file_chirho.lock();
        let saved_pos_chirho = file_guard_chirho.pos_chirho;
        file_guard_chirho.pos_chirho = block_nr_chirho * SECTOR_SIZE_CHIRHO as u64;

        let read_result_chirho =
            file_guard_chirho.ops_chirho.read_chirho(&mut file_guard_chirho, buf_chirho);
        file_guard_chirho.pos_chirho = saved_pos_chirho;

        match read_result_chirho {
            Ok(bytes_read_chirho) if bytes_read_chirho == buf_chirho.len() => Ok(()),
            Ok(bytes_read_chirho) => {
                crate::serial_println_chirho!(
                    "[BLOCK] loop read short: sector={} requested={} got={}",
                    block_nr_chirho,
                    buf_chirho.len(),
                    bytes_read_chirho
                );
                Err(-crate::syscall_chirho::EIO_CHIRHO)
            }
            Err(errno_chirho) => {
                crate::serial_println_chirho!(
                    "[BLOCK] loop read failed: sector={} len={} errno={}",
                    block_nr_chirho,
                    buf_chirho.len(),
                    errno_chirho
                );
                Err(errno_chirho)
            }
        }
    }

    fn write_block_chirho(
        &self,
        block_nr_chirho: u64,
        buf_chirho: &[u8],
    ) -> Result<(), i64> {
        let mut file_guard_chirho = self.backing_file_chirho.lock();
        let saved_pos_chirho = file_guard_chirho.pos_chirho;
        file_guard_chirho.pos_chirho = block_nr_chirho * SECTOR_SIZE_CHIRHO as u64;

        let write_result_chirho =
            file_guard_chirho.ops_chirho.write_chirho(&mut file_guard_chirho, buf_chirho);
        file_guard_chirho.pos_chirho = saved_pos_chirho;

        match write_result_chirho {
            Ok(bytes_written_chirho) if bytes_written_chirho == buf_chirho.len() => Ok(()),
            Ok(bytes_written_chirho) => {
                crate::serial_println_chirho!(
                    "[BLOCK] loop write short: sector={} requested={} got={}",
                    block_nr_chirho,
                    buf_chirho.len(),
                    bytes_written_chirho
                );
                Err(-crate::syscall_chirho::EIO_CHIRHO)
            }
            Err(errno_chirho) => {
                crate::serial_println_chirho!(
                    "[BLOCK] loop write failed: sector={} len={} errno={}",
                    block_nr_chirho,
                    buf_chirho.len(),
                    errno_chirho
                );
                Err(errno_chirho)
            }
        }
    }

    fn block_size_chirho(&self) -> usize { SECTOR_SIZE_CHIRHO }

    fn total_blocks_chirho(&self) -> u64 {
        let file_guard_chirho = self.backing_file_chirho.lock();
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        inode_guard_chirho.size_chirho / SECTOR_SIZE_CHIRHO as u64
    }
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
    device_chirho: alloc::sync::Arc<dyn BlockDeviceChirho + Send + Sync>,
    /// Per-device request queue.
    queue_chirho: RequestQueueChirho,
}

/// Global registry of block devices.
///
/// Device drivers call [`register_chirho`](BlockDeviceRegistryChirho::register_chirho)
/// during initialisation.  The VFS / filesystem code looks up devices by name
/// or index.
pub struct BlockDeviceRegistryChirho {
    pub devices_chirho: Mutex<Vec<BlockDeviceEntryChirho>>,
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
        device_chirho: alloc::sync::Arc<dyn BlockDeviceChirho + Send + Sync>,
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

    /// Read a single block (sector) from a registered device by index.
    ///
    /// Returns `Ok(())` on success, or an error if the device index is
    /// out of range or the read fails.
    pub fn read_block_chirho(
        &self,
        device_idx_chirho: usize,
        block_nr_chirho: u64,
        buf_chirho: &mut [u8],
    ) -> Result<(), i64> {
        // Clone the device Arc THEN drop the lock, so the read itself
        // doesn't hold the registry lock.  This prevents deadlock when
        // a loop-backed device reads from an ext4 file, which in turn
        // calls read_block_chirho on device 0 (VirtIO).
        let device_arc_chirho = {
            let devices_chirho = self.devices_chirho.lock();
            if device_idx_chirho >= devices_chirho.len() {
                return Err(-19); // ENODEV
            }
            devices_chirho[device_idx_chirho].device_chirho.clone()
        };
        // Lock is released — safe to call read which may re-enter the registry.
        if device_idx_chirho >= 1 {
            crate::serial_println_chirho!(
                "[BLOCK] read dev={} sector={} len={}",
                device_idx_chirho, block_nr_chirho, buf_chirho.len()
            );
        }
        let result_chirho = device_arc_chirho.read_block_chirho(block_nr_chirho, buf_chirho);
        if device_idx_chirho >= 1 {
            crate::serial_println_chirho!(
                "[BLOCK] read dev={} done: {:?}",
                device_idx_chirho, result_chirho.is_ok()
            );
        }
        result_chirho
    }

    /// Write a single block (sector) to a registered device by index.
    ///
    /// Used by the ext4 write path (write_block_chirho) for sector-level
    /// persistence through VirtIO-blk.
    pub fn write_block_chirho(
        &self,
        device_idx_chirho: usize,
        block_nr_chirho: u64,
        buf_chirho: &[u8],
    ) -> Result<(), i64> {
        let device_arc_chirho = {
            let devices_chirho = self.devices_chirho.lock();
            if device_idx_chirho >= devices_chirho.len() {
                return Err(-19); // ENODEV
            }
            devices_chirho[device_idx_chirho].device_chirho.clone()
        };
        device_arc_chirho.write_block_chirho(block_nr_chirho, buf_chirho)
    }
}

/// Register or replace a loop-backed block device at a specific registry slot.
pub fn register_loop_block_device_chirho(
    device_id_chirho: usize,
    backing_file_chirho: Arc<Mutex<crate::vfs_chirho::FileChirho>>,
) -> Result<(), i64> {
    if device_id_chirho >= MAX_BLOCK_DEVICES_CHIRHO {
        crate::serial_println_chirho!(
            "[BLOCK] loop registration rejected: device_id={} max={}",
            device_id_chirho,
            MAX_BLOCK_DEVICES_CHIRHO
        );
        return Err(-crate::syscall_chirho::ENOMEM_CHIRHO);
    }

    let mut devices_chirho = BLOCK_REGISTRY_CHIRHO.devices_chirho.lock();
    while devices_chirho.len() <= device_id_chirho {
        let placeholder_name_chirho = alloc::format!("unbound{}", devices_chirho.len());
        devices_chirho.push(BlockDeviceEntryChirho {
            name_chirho: placeholder_name_chirho,
            device_chirho: alloc::sync::Arc::new(UnboundBlockDeviceChirho),
            queue_chirho: RequestQueueChirho::new_chirho(),
        });
    }

    let loop_name_chirho = alloc::format!("loop{}", device_id_chirho);
    devices_chirho[device_id_chirho] = BlockDeviceEntryChirho {
        name_chirho: loop_name_chirho.clone(),
        device_chirho: alloc::sync::Arc::new(LoopFileBlockDeviceChirho { backing_file_chirho }),
        queue_chirho: RequestQueueChirho::new_chirho(),
    };

    crate::serial_println_chirho!(
        "[BLOCK] registered loop-backed block device '{}' at slot {}",
        loop_name_chirho,
        device_id_chirho
    );
    Ok(())
}

/// The single, global block device registry.
pub static BLOCK_REGISTRY_CHIRHO: BlockDeviceRegistryChirho =
    BlockDeviceRegistryChirho::new_chirho();
