// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Loop device subsystem for the Lineluya kernel.
//!
//! Provides:
//! - `/dev/loop-control` (major 10, minor 237) — miscdevice for managing
//!   loop devices, with `LOOP_CTL_GET_FREE` ioctl.
//! - `/dev/loop0` through `/dev/loop7` (major 7, minor 0-7) — loop block
//!   device nodes with backing-file support.
//!
//! Implemented tasks:
//! - A2-LOOP-001: /dev/loop-control device node
//! - A2-LOOP-002: /dev/loop0..7 device nodes
//! - A2-LOOP-003: LOOP_SET_FD ioctl — bind backing file fd
//! - A2-LOOP-004: LOOP_CLR_FD ioctl — detach backing file
//! - A2-LOOP-005: LOOP_SET_STATUS64 ioctl — set lo_offset / lo_sizelimit
//! - A2-LOOP-006: LOOP_GET_STATUS64 ioctl — get stored parameters
//! - A2-LOOP-007: Loop device read/write — translate to backing file I/O

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::syscall_chirho::{EINVAL_CHIRHO, ENXIO_CHIRHO, ENOSYS_CHIRHO, EBADF_CHIRHO};
use crate::devtmpfs_chirho::DevNodeDataChirho;

// ============================================================================
// Loop device constants
// ============================================================================

/// Number of pre-created loop devices (loop0..loop7).
pub const LOOP_DEVICE_COUNT_CHIRHO: u8 = 8;

/// Major device number for loop block devices.
pub const LOOP_MAJOR_CHIRHO: u32 = 7;

/// Major device number for /dev/loop-control (misc device).
pub const LOOP_CONTROL_MAJOR_CHIRHO: u32 = 10;

/// Minor device number for /dev/loop-control.
pub const LOOP_CONTROL_MINOR_CHIRHO: u32 = 237;

// ============================================================================
// Loop ioctl command numbers
// ============================================================================

/// LOOP_SET_FD — associate a loop device with a file descriptor.
const LOOP_SET_FD_CHIRHO: u64 = 0x4C00;

/// LOOP_CLR_FD — disassociate a loop device from its file descriptor.
const LOOP_CLR_FD_CHIRHO: u64 = 0x4C01;

/// LOOP_SET_STATUS64 — set loop device parameters (64-bit).
const LOOP_SET_STATUS64_CHIRHO: u64 = 0x4C04;

/// LOOP_GET_STATUS64 — get loop device status (64-bit).
const LOOP_GET_STATUS64_CHIRHO: u64 = 0x4C05;

/// LOOP_CTL_ADD — add a new loop device.
#[allow(dead_code)]
const LOOP_CTL_ADD_CHIRHO: u64 = 0x4C80;

/// LOOP_CTL_REMOVE — remove a loop device.
#[allow(dead_code)]
const LOOP_CTL_REMOVE_CHIRHO: u64 = 0x4C81;

/// LOOP_CTL_GET_FREE — get the index of the first free loop device.
const LOOP_CTL_GET_FREE_CHIRHO: u64 = 0x4C82;

// ============================================================================
// Per-device loop state (A2-LOOP-003 through A2-LOOP-007)
// ============================================================================

/// Per-loop-device state: backing file fd, offset, size limit.
struct LoopStateChirho {
    /// File descriptor of the backing file, or -1 if unbound.
    backing_fd_chirho: i64,
    /// Byte offset into the backing file (from LOOP_SET_STATUS64).
    lo_offset_chirho: u64,
    /// Size limit in bytes (0 = no limit; from LOOP_SET_STATUS64).
    lo_sizelimit_chirho: u64,
}

impl LoopStateChirho {
    const fn new_chirho() -> Self {
        Self {
            backing_fd_chirho: -1,
            lo_offset_chirho: 0,
            lo_sizelimit_chirho: 0,
        }
    }

    fn is_bound_chirho(&self) -> bool {
        self.backing_fd_chirho >= 0
    }
}

/// Global loop device state array, indexed by minor number (0..7).
/// Protected by a spinlock.
static LOOP_STATES_CHIRHO: spin::Mutex<[LoopStateChirho; 8]> = spin::Mutex::new([
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
    LoopStateChirho::new_chirho(),
]);

/// Linux `struct loop_info64` layout (used by LOOP_SET_STATUS64 / LOOP_GET_STATUS64).
///
/// We only care about `lo_offset` (at byte offset 0) and `lo_sizelimit` (at offset 8).
/// Total struct is 232 bytes on Linux; we only read/write the first 16 bytes.
#[repr(C)]
struct LoopInfo64Chirho {
    lo_device_chirho: u64,        // offset  0
    lo_inode_chirho: u64,         // offset  8
    lo_rdevice_chirho: u64,       // offset 16
    lo_offset_chirho: u64,        // offset 24
    lo_sizelimit_chirho: u64,     // offset 32
    lo_number_chirho: u32,        // offset 40
    lo_encrypt_type_chirho: u32,  // offset 44
    lo_encrypt_key_size_chirho: u32, // offset 48
    lo_flags_chirho: u32,         // offset 52
    lo_file_name_chirho: [u8; 64],// offset 56
    lo_crypt_name_chirho: [u8; 64],// offset 120
    lo_encrypt_key_chirho: [u8; 32],// offset 184
    lo_init_chirho: [u64; 2],     // offset 216
}

// ============================================================================
// Helper: extract minor number from FileChirho
// ============================================================================

/// Extract the minor device number from a loop device FileChirho.
///
/// Locks the inode, reads fs_data_chirho, downcasts to DevNodeDataChirho.
/// Returns None if the inode has no DevNodeDataChirho or minor >= 8.
fn get_minor_chirho(file_chirho: &FileChirho) -> Option<usize> {
    let inode_guard_chirho = file_chirho.inode_chirho.lock();
    if let Some(ref data_chirho) = inode_guard_chirho.fs_data_chirho {
        if let Some(dev_chirho) = data_chirho.downcast_ref::<DevNodeDataChirho>() {
            let minor_chirho = dev_chirho.minor_chirho as usize;
            if minor_chirho < LOOP_DEVICE_COUNT_CHIRHO as usize {
                return Some(minor_chirho);
            }
        }
    }
    None
}

// ============================================================================
// LoopControlOpsChirho — /dev/loop-control (major 10, minor 237)
// ============================================================================

/// File operations for `/dev/loop-control`.
///
/// - `read`/`write` return errors (not a data device).
/// - `ioctl` handles `LOOP_CTL_GET_FREE` returning the first unbound device.
pub struct LoopControlOpsChirho;

impl FileOpsChirho for LoopControlOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO) // not a data device
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO) // not a data device
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        match cmd_chirho {
            LOOP_CTL_GET_FREE_CHIRHO => {
                // Return the first unbound loop device index.
                let states_chirho = LOOP_STATES_CHIRHO.lock();
                for (idx_chirho, state_chirho) in states_chirho.iter().enumerate() {
                    if !state_chirho.is_bound_chirho() {
                        crate::serial_println_chirho!(
                            "LOOP: LOOP_CTL_GET_FREE -> returning {}",
                            idx_chirho
                        );
                        return Ok(idx_chirho as i64);
                    }
                }
                // All bound — return 0 as fallback (Linux would create a new one).
                crate::serial_println_chirho!(
                    "LOOP: LOOP_CTL_GET_FREE -> all bound, returning 0"
                );
                Ok(0)
            }
            LOOP_CTL_ADD_CHIRHO => {
                crate::serial_println_chirho!(
                    "LOOP: LOOP_CTL_ADD (stub, returning -ENOSYS)"
                );
                Err(-ENOSYS_CHIRHO)
            }
            LOOP_CTL_REMOVE_CHIRHO => {
                crate::serial_println_chirho!(
                    "LOOP: LOOP_CTL_REMOVE (stub, returning -ENOSYS)"
                );
                Err(-ENOSYS_CHIRHO)
            }
            _ => {
                crate::serial_println_chirho!(
                    "LOOP: /dev/loop-control unhandled ioctl cmd={:#x}",
                    cmd_chirho
                );
                Err(-ENOSYS_CHIRHO)
            }
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/loop-control file operations.
pub static LOOP_CONTROL_OPS_CHIRHO: LoopControlOpsChirho = LoopControlOpsChirho;

// ============================================================================
// LoopDeviceOpsChirho — /dev/loop0..7 (major 7, minor 0-7)
// ============================================================================

/// File operations for `/dev/loopN` devices.
///
/// When a backing file is bound via `LOOP_SET_FD`, read/write operations
/// are forwarded to the backing file using pread/pwrite semantics.
/// When no backing file is bound, read/write return -ENXIO.
pub struct LoopDeviceOpsChirho;

impl FileOpsChirho for LoopDeviceOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // A2-LOOP-007: Forward reads to backing file.
        let minor_chirho = get_minor_chirho(file_chirho).ok_or(-ENXIO_CHIRHO)?;

        let (backing_fd_chirho, lo_offset_chirho) = {
            let states_chirho = LOOP_STATES_CHIRHO.lock();
            let state_chirho = &states_chirho[minor_chirho];
            if !state_chirho.is_bound_chirho() {
                return Err(-ENXIO_CHIRHO);
            }
            (state_chirho.backing_fd_chirho, state_chirho.lo_offset_chirho)
        };

        // Look up the backing file descriptor and pread from it.
        let file_arc_chirho = crate::fs_chirho::lookup_fd_chirho(backing_fd_chirho as u64)
            .ok_or(-EBADF_CHIRHO)?;

        let mut backing_file_chirho = file_arc_chirho.lock();
        // Calculate the position: loop device pos + lo_offset.
        let read_pos_chirho = file_chirho.pos_chirho + lo_offset_chirho;

        // Save backing file position, seek, read, restore.
        let saved_pos_chirho = backing_file_chirho.pos_chirho;
        backing_file_chirho.pos_chirho = read_pos_chirho;
        let result_chirho = backing_file_chirho.ops_chirho.read_chirho(
            &mut backing_file_chirho,
            buf_chirho,
        );
        // Restore the backing file's original position.
        backing_file_chirho.pos_chirho = saved_pos_chirho;

        match result_chirho {
            Ok(bytes_read_chirho) => {
                // Advance the loop device's own position.
                file_chirho.pos_chirho += bytes_read_chirho as u64;
                crate::serial_println_chirho!(
                    "LOOP: loop{} read {} bytes at offset {}",
                    minor_chirho,
                    bytes_read_chirho,
                    read_pos_chirho
                );
                Ok(bytes_read_chirho)
            }
            Err(e_chirho) => Err(e_chirho),
        }
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // A2-LOOP-007: Forward writes to backing file.
        let minor_chirho = get_minor_chirho(file_chirho).ok_or(-ENXIO_CHIRHO)?;

        let (backing_fd_chirho, lo_offset_chirho) = {
            let states_chirho = LOOP_STATES_CHIRHO.lock();
            let state_chirho = &states_chirho[minor_chirho];
            if !state_chirho.is_bound_chirho() {
                return Err(-ENXIO_CHIRHO);
            }
            (state_chirho.backing_fd_chirho, state_chirho.lo_offset_chirho)
        };

        // Look up the backing file descriptor and pwrite to it.
        let file_arc_chirho = crate::fs_chirho::lookup_fd_chirho(backing_fd_chirho as u64)
            .ok_or(-EBADF_CHIRHO)?;

        let mut backing_file_chirho = file_arc_chirho.lock();
        let write_pos_chirho = file_chirho.pos_chirho + lo_offset_chirho;

        // Save, seek, write, restore.
        let saved_pos_chirho = backing_file_chirho.pos_chirho;
        backing_file_chirho.pos_chirho = write_pos_chirho;
        let result_chirho = backing_file_chirho.ops_chirho.write_chirho(
            &mut backing_file_chirho,
            buf_chirho,
        );
        backing_file_chirho.pos_chirho = saved_pos_chirho;

        match result_chirho {
            Ok(bytes_written_chirho) => {
                file_chirho.pos_chirho += bytes_written_chirho as u64;
                crate::serial_println_chirho!(
                    "LOOP: loop{} wrote {} bytes at offset {}",
                    minor_chirho,
                    bytes_written_chirho,
                    write_pos_chirho
                );
                Ok(bytes_written_chirho)
            }
            Err(e_chirho) => Err(e_chirho),
        }
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        // Basic seek on the loop device's own position.
        let minor_chirho = get_minor_chirho(file_chirho).ok_or(-ENXIO_CHIRHO)?;
        let _bound_chirho = {
            let states_chirho = LOOP_STATES_CHIRHO.lock();
            states_chirho[minor_chirho].is_bound_chirho()
        };

        let new_pos_chirho = match whence_chirho {
            0 => {
                // SEEK_SET
                if offset_chirho < 0 {
                    return Err(-EINVAL_CHIRHO);
                }
                offset_chirho as u64
            }
            1 => {
                // SEEK_CUR
                let cur_chirho = file_chirho.pos_chirho as i64;
                let new_chirho = cur_chirho + offset_chirho;
                if new_chirho < 0 {
                    return Err(-EINVAL_CHIRHO);
                }
                new_chirho as u64
            }
            _ => return Err(-EINVAL_CHIRHO),
        };
        file_chirho.pos_chirho = new_pos_chirho;
        Ok(new_pos_chirho)
    }

    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        let minor_chirho = get_minor_chirho(file_chirho).unwrap_or(0);

        match cmd_chirho {
            // =================================================================
            // A2-LOOP-003: LOOP_SET_FD — bind backing file to loop device
            // =================================================================
            LOOP_SET_FD_CHIRHO => {
                let fd_chirho = arg_chirho as i64;
                // Validate the backing fd exists.
                if crate::fs_chirho::lookup_fd_chirho(fd_chirho as u64).is_none() {
                    crate::serial_println_chirho!(
                        "LOOP: loop{} LOOP_SET_FD fd={} — EBADF",
                        minor_chirho,
                        fd_chirho
                    );
                    return Err(-EBADF_CHIRHO);
                }

                let mut states_chirho = LOOP_STATES_CHIRHO.lock();
                let state_chirho = &mut states_chirho[minor_chirho];

                if state_chirho.is_bound_chirho() {
                    crate::serial_println_chirho!(
                        "LOOP: loop{} LOOP_SET_FD — already bound to fd={}",
                        minor_chirho,
                        state_chirho.backing_fd_chirho
                    );
                    // Linux returns -EBUSY, use EINVAL for simplicity.
                    return Err(-EINVAL_CHIRHO);
                }

                state_chirho.backing_fd_chirho = fd_chirho;
                state_chirho.lo_offset_chirho = 0;
                state_chirho.lo_sizelimit_chirho = 0;

                crate::serial_println_chirho!(
                    "LOOP: loop{} LOOP_SET_FD fd={} — bound successfully",
                    minor_chirho,
                    fd_chirho
                );
                Ok(0)
            }

            // =================================================================
            // A2-LOOP-004: LOOP_CLR_FD — detach backing file
            // =================================================================
            LOOP_CLR_FD_CHIRHO => {
                let mut states_chirho = LOOP_STATES_CHIRHO.lock();
                let state_chirho = &mut states_chirho[minor_chirho];

                if !state_chirho.is_bound_chirho() {
                    crate::serial_println_chirho!(
                        "LOOP: loop{} LOOP_CLR_FD — not bound, returning -ENXIO",
                        minor_chirho
                    );
                    return Err(-ENXIO_CHIRHO);
                }

                let old_fd_chirho = state_chirho.backing_fd_chirho;
                state_chirho.backing_fd_chirho = -1;
                state_chirho.lo_offset_chirho = 0;
                state_chirho.lo_sizelimit_chirho = 0;

                crate::serial_println_chirho!(
                    "LOOP: loop{} LOOP_CLR_FD — detached from fd={}",
                    minor_chirho,
                    old_fd_chirho
                );
                Ok(0)
            }

            // =================================================================
            // A2-LOOP-005: LOOP_SET_STATUS64 — set loop device parameters
            // =================================================================
            LOOP_SET_STATUS64_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EINVAL_CHIRHO);
                }

                // Read lo_offset and lo_sizelimit from the userspace struct.
                // In Linux's loop_info64: lo_offset is at byte 24, lo_sizelimit at 32.
                let lo_offset_chirho = unsafe {
                    core::ptr::read_volatile((arg_chirho + 24) as *const u64)
                };
                let lo_sizelimit_chirho = unsafe {
                    core::ptr::read_volatile((arg_chirho + 32) as *const u64)
                };

                let mut states_chirho = LOOP_STATES_CHIRHO.lock();
                let state_chirho = &mut states_chirho[minor_chirho];

                state_chirho.lo_offset_chirho = lo_offset_chirho;
                state_chirho.lo_sizelimit_chirho = lo_sizelimit_chirho;

                crate::serial_println_chirho!(
                    "LOOP: loop{} LOOP_SET_STATUS64 offset={} sizelimit={}",
                    minor_chirho,
                    lo_offset_chirho,
                    lo_sizelimit_chirho
                );
                Ok(0)
            }

            // =================================================================
            // A2-LOOP-006: LOOP_GET_STATUS64 — get loop device parameters
            // =================================================================
            LOOP_GET_STATUS64_CHIRHO => {
                let states_chirho = LOOP_STATES_CHIRHO.lock();
                let state_chirho = &states_chirho[minor_chirho];

                if !state_chirho.is_bound_chirho() {
                    crate::serial_println_chirho!(
                        "LOOP: loop{} LOOP_GET_STATUS64 — not bound, returning -ENXIO",
                        minor_chirho
                    );
                    return Err(-ENXIO_CHIRHO);
                }

                if arg_chirho == 0 {
                    return Err(-EINVAL_CHIRHO);
                }

                // Zero the entire struct first (232 bytes for loop_info64).
                unsafe {
                    core::ptr::write_bytes(arg_chirho as *mut u8, 0, 232);
                }

                // Write lo_offset at byte 24 and lo_sizelimit at byte 32.
                unsafe {
                    core::ptr::write_volatile(
                        (arg_chirho + 24) as *mut u64,
                        state_chirho.lo_offset_chirho,
                    );
                    core::ptr::write_volatile(
                        (arg_chirho + 32) as *mut u64,
                        state_chirho.lo_sizelimit_chirho,
                    );
                    // Write lo_number at byte 40.
                    core::ptr::write_volatile(
                        (arg_chirho + 40) as *mut u32,
                        minor_chirho as u32,
                    );
                }

                crate::serial_println_chirho!(
                    "LOOP: loop{} LOOP_GET_STATUS64 offset={} sizelimit={}",
                    minor_chirho,
                    state_chirho.lo_offset_chirho,
                    state_chirho.lo_sizelimit_chirho
                );
                Ok(0)
            }

            _ => {
                crate::serial_println_chirho!(
                    "LOOP: /dev/loop{} unhandled ioctl cmd={:#x}",
                    minor_chirho,
                    cmd_chirho
                );
                Err(-ENOSYS_CHIRHO)
            }
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/loopN file operations.
pub static LOOP_DEVICE_OPS_CHIRHO: LoopDeviceOpsChirho = LoopDeviceOpsChirho;
