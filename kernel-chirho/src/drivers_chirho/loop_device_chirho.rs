// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Loop device subsystem for the Lineluya kernel (A2-LOOP-001 / A2-LOOP-002).
//!
//! Provides:
//! - `/dev/loop-control` (major 10, minor 237) — miscdevice for managing
//!   loop devices, with `LOOP_CTL_GET_FREE` ioctl returning 0.
//! - `/dev/loop0` through `/dev/loop7` (major 7, minor 0-7) — loop
//!   block device nodes with stub file operations.
//!
//! These stubs allow userspace programs (e.g. `losetup`, `mount -o loop`)
//! to probe for loop device support without crashing. Actual backing-file
//! association is not yet implemented.

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::syscall_chirho::{EINVAL_CHIRHO, ENXIO_CHIRHO, ENOSYS_CHIRHO};

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

/// LOOP_CTL_ADD — add a new loop device.
#[allow(dead_code)]
const LOOP_CTL_ADD_CHIRHO: u64 = 0x4C80;

/// LOOP_CTL_REMOVE — remove a loop device.
#[allow(dead_code)]
const LOOP_CTL_REMOVE_CHIRHO: u64 = 0x4C81;

/// LOOP_CTL_GET_FREE — get the index of the first free loop device.
const LOOP_CTL_GET_FREE_CHIRHO: u64 = 0x4C82;

/// LOOP_SET_FD — associate a loop device with a file descriptor.
#[allow(dead_code)]
const LOOP_SET_FD_CHIRHO: u64 = 0x4C00;

/// LOOP_CLR_FD — disassociate a loop device from its file descriptor.
#[allow(dead_code)]
const LOOP_CLR_FD_CHIRHO: u64 = 0x4C01;

/// LOOP_GET_STATUS64 — get loop device status (64-bit).
#[allow(dead_code)]
const LOOP_GET_STATUS64_CHIRHO: u64 = 0x4C05;

// ============================================================================
// LoopControlOpsChirho — /dev/loop-control (major 10, minor 237)
// ============================================================================

/// File operations for `/dev/loop-control`.
///
/// - `read`/`write` return errors (not a data device).
/// - `ioctl` handles `LOOP_CTL_GET_FREE` returning 0 (first loop device).
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
                // Return 0 — first loop device is always "free" in our stub.
                crate::serial_println_chirho!(
                    "LOOP: LOOP_CTL_GET_FREE -> returning 0"
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
/// - `read` returns -ENXIO (no backing file associated).
/// - `write` returns -ENXIO (no backing file associated).
/// - `ioctl` handles LOOP_SET_FD and LOOP_GET_STATUS64 with stub errors.
pub struct LoopDeviceOpsChirho;

impl FileOpsChirho for LoopDeviceOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // No backing file — return ENXIO.
        Err(-ENXIO_CHIRHO)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // No backing file — return ENXIO.
        Err(-ENXIO_CHIRHO)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-ENXIO_CHIRHO)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        match cmd_chirho {
            LOOP_SET_FD_CHIRHO => {
                crate::serial_println_chirho!(
                    "LOOP: LOOP_SET_FD (stub, returning -ENOSYS)"
                );
                Err(-ENOSYS_CHIRHO)
            }
            LOOP_CLR_FD_CHIRHO => {
                crate::serial_println_chirho!(
                    "LOOP: LOOP_CLR_FD (stub, returning -ENOSYS)"
                );
                Err(-ENOSYS_CHIRHO)
            }
            LOOP_GET_STATUS64_CHIRHO => {
                crate::serial_println_chirho!(
                    "LOOP: LOOP_GET_STATUS64 (stub, returning -ENXIO)"
                );
                Err(-ENXIO_CHIRHO)
            }
            _ => {
                crate::serial_println_chirho!(
                    "LOOP: /dev/loopN unhandled ioctl cmd={:#x}",
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
