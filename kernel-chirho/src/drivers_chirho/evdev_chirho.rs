// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! evdev input device driver (`/dev/input/event0`) for the Lineluya kernel.
//!
//! Provides a Linux-compatible evdev character device for keyboard input.
//! This is the infrastructure needed by X11, Wayland compositors, and libinput
//! to discover and read input devices.
//!
//! Supports:
//! - `read()`: returns queued `struct input_event` (16 bytes each) from the
//!   event ring buffer.
//! - `ioctl EVIOCGNAME`: returns the device name ("lineluya-keyboard").
//! - `ioctl EVIOCGID`: returns a valid `struct input_id`.
//! - `ioctl EVIOCGBIT(EV_KEY)`: returns capability bits for key events.
//! - `ioctl EVIOCGBIT(0)`: returns event type capability bits.
//! - `ioctl EVIOCGPROP`: returns device property bits (empty).
//!
//! Keyboard interrupt hookup is deferred -- for now this creates the device
//! infrastructure so that input subsystem discovery (udev, libinput) works.

extern crate alloc;

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::syscall_chirho::{
    EAGAIN_CHIRHO, EFAULT_CHIRHO, EINVAL_CHIRHO, ENOSYS_CHIRHO,
};

// ============================================================================
// Linux input event constants
// ============================================================================

/// Size of `struct input_event` on x86_64 Linux.
/// Layout: struct timeval (8+8=16 bytes on x86_64) + __u16 type + __u16 code + __s32 value
/// BUT: Linux evdev uses a "compat" struct that is always 16 bytes:
///   __u64 time_sec (or __kernel_ulong_t)
///   __u64 time_usec (or __kernel_ulong_t)
///   wait -- the actual Linux input_event on 64-bit is:
///   struct input_event {
///     struct timeval time; // 16 bytes on x86_64 (tv_sec:i64 + tv_usec:i64)
///     __u16 type;
///     __u16 code;
///     __s32 value;
///   };  // total = 24 bytes
///
/// However, modern kernels with INPUT_COMPAT use a fixed-size 24-byte struct.
const INPUT_EVENT_SIZE_CHIRHO: usize = 24;

/// EV_SYN -- synchronization events.
const EV_SYN_CHIRHO: u16 = 0x00;
/// EV_KEY -- key/button events.
const EV_KEY_CHIRHO: u16 = 0x01;
/// EV_REL -- relative axis events (mouse movement).
const EV_REL_CHIRHO: u16 = 0x02;
/// EV_ABS -- absolute axis events (touchscreen).
const EV_ABS_CHIRHO: u16 = 0x03;
/// EV_MSC -- miscellaneous events.
const EV_MSC_CHIRHO: u16 = 0x04;
/// EV_LED -- LED events.
const EV_LED_CHIRHO: u16 = 0x11;
/// EV_REP -- auto-repeat events.
const EV_REP_CHIRHO: u16 = 0x14;

/// Maximum event type value we report capability for.
const EV_MAX_CHIRHO: u16 = 0x1f;

/// Maximum key code (KEY_MAX in Linux = 0x2ff).
const KEY_MAX_CHIRHO: u16 = 0x2ff;

// ============================================================================
// IOCTL command numbers (from linux/input.h)
// ============================================================================

/// EVIOCGVERSION -- get driver version.
/// _IOR('E', 0x01, int) = 0x80044501
const EVIOCGVERSION_CHIRHO: u64 = 0x80044501;

/// EVIOCGID -- get device ID.
/// _IOR('E', 0x02, struct input_id) = 0x80084502
const EVIOCGID_CHIRHO: u64 = 0x80084502;

/// EVIOCGNAME -- get device name.
/// _IOC(_IOC_READ, 'E', 0x06, len) -- length-dependent, we match prefix.
/// The ioctl number encodes the buffer size in the upper bits.
/// We match on the base command: (cmd & 0xFFFF00FF) == 0x45060000 pattern
/// Actually: _IOC(_IOC_READ, 'E', 0x06, size) => high bits = 0x80004506 | (size << 16)
/// We match: (cmd & 0x0000FFFF) == 0x4506 (bottom 16 bits)
const EVIOCGNAME_BASE_CHIRHO: u64 = 0x4506;

/// EVIOCGPHYS -- get physical location string.
const EVIOCGPHYS_BASE_CHIRHO: u64 = 0x4507;

/// EVIOCGUNIQ -- get unique identifier string.
const EVIOCGUNIQ_BASE_CHIRHO: u64 = 0x4508;

/// EVIOCGPROP -- get device properties.
/// _IOC(_IOC_READ, 'E', 0x09, len)
const EVIOCGPROP_BASE_CHIRHO: u64 = 0x4509;

/// EVIOCGBIT -- get event/key/etc. bits.
/// _IOC(_IOC_READ, 'E', 0x20 + ev_type, len)
/// Base for EV type 0: 0x4520, EV_KEY(1): 0x4521, etc.
const EVIOCGBIT_BASE_CHIRHO: u64 = 0x4520;

/// EVIOCGKEY -- get key state.
const EVIOCGKEY_BASE_CHIRHO: u64 = 0x4518;

/// EVIOCGABS -- get abs axis info.
const EVIOCGABS_BASE_CHIRHO: u64 = 0x4540;

// ============================================================================
// Linux struct input_id (8 bytes)
// ============================================================================

/// Linux `struct input_id` -- device identification.
#[repr(C)]
#[derive(Clone, Copy)]
struct InputIdChirho {
    bustype_chirho: u16,  // BUS_I8042 = 0x11 for PS/2 keyboard
    vendor_chirho: u16,
    product_chirho: u16,
    version_chirho: u16,
}

/// BUS_I8042 -- PS/2 keyboard bus type.
const BUS_I8042_CHIRHO: u16 = 0x11;

// ============================================================================
// Linux struct input_event (24 bytes on x86_64)
// ============================================================================

/// Linux `struct input_event` -- a single input event.
///
/// On x86_64 Linux:
/// ```c
/// struct input_event {
///     struct timeval time;  // tv_sec: i64, tv_usec: i64 = 16 bytes
///     __u16 type;
///     __u16 code;
///     __s32 value;
/// };
/// ```
/// Total: 24 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputEventChirho {
    pub tv_sec_chirho: i64,
    pub tv_usec_chirho: i64,
    pub type_chirho: u16,
    pub code_chirho: u16,
    pub value_chirho: i32,
}

// ============================================================================
// Event ring buffer
// ============================================================================

/// Maximum number of events in the ring buffer.
const EVENT_RING_SIZE_CHIRHO: usize = 64;

/// Ring buffer for input events, protected by a spinlock.
/// This is a global buffer -- when keyboard interrupts are hooked up,
/// they will push events into this buffer, and userspace reads will
/// drain it.
pub static EVENT_RING_CHIRHO: spin::Mutex<EventRingChirho> =
    spin::Mutex::new(EventRingChirho::new_chirho());

/// Fixed-size ring buffer for input events.
pub struct EventRingChirho {
    buf_chirho: [InputEventChirho; EVENT_RING_SIZE_CHIRHO],
    head_chirho: usize,  // next write position
    tail_chirho: usize,  // next read position
    count_chirho: usize, // number of events in buffer
}

impl EventRingChirho {
    /// Create an empty ring buffer.
    const fn new_chirho() -> Self {
        Self {
            buf_chirho: [InputEventChirho {
                tv_sec_chirho: 0,
                tv_usec_chirho: 0,
                type_chirho: 0,
                code_chirho: 0,
                value_chirho: 0,
            }; EVENT_RING_SIZE_CHIRHO],
            head_chirho: 0,
            tail_chirho: 0,
            count_chirho: 0,
        }
    }

    /// Push an event into the ring buffer. Drops oldest if full.
    pub fn push_chirho(&mut self, event_chirho: InputEventChirho) {
        self.buf_chirho[self.head_chirho] = event_chirho;
        self.head_chirho = (self.head_chirho + 1) % EVENT_RING_SIZE_CHIRHO;
        if self.count_chirho == EVENT_RING_SIZE_CHIRHO {
            // Buffer full -- drop oldest event
            self.tail_chirho = (self.tail_chirho + 1) % EVENT_RING_SIZE_CHIRHO;
        } else {
            self.count_chirho += 1;
        }
    }

    /// Pop an event from the ring buffer. Returns None if empty.
    pub fn pop_chirho(&mut self) -> Option<InputEventChirho> {
        if self.count_chirho == 0 {
            return None;
        }
        let event_chirho = self.buf_chirho[self.tail_chirho];
        self.tail_chirho = (self.tail_chirho + 1) % EVENT_RING_SIZE_CHIRHO;
        self.count_chirho -= 1;
        Some(event_chirho)
    }

    /// Check if the ring buffer has any events.
    pub fn has_events_chirho(&self) -> bool {
        self.count_chirho > 0
    }
}

// ============================================================================
// EvdevOpsChirho -- /dev/input/event0 file operations
// ============================================================================

/// File operations for `/dev/input/event0` (keyboard evdev device).
pub struct EvdevOpsChirho;

impl FileOpsChirho for EvdevOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // Each read must return complete input_event structs (24 bytes each).
        // If buffer is too small for even one event, return EINVAL.
        if buf_chirho.len() < INPUT_EVENT_SIZE_CHIRHO {
            return Err(-EINVAL_CHIRHO);
        }

        let nonblock_chirho = (file_chirho.flags_chirho
            & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;

        let mut ring_chirho = EVENT_RING_CHIRHO.lock();

        if !ring_chirho.has_events_chirho() {
            if nonblock_chirho {
                return Err(-EAGAIN_CHIRHO);
            }
            // Blocking mode: for now, return 0 (no data).
            // When keyboard interrupts are hooked up, this would block
            // until events are available.
            return Ok(0);
        }

        // Read as many complete events as fit in the buffer.
        let max_events_chirho = buf_chirho.len() / INPUT_EVENT_SIZE_CHIRHO;
        let mut bytes_written_chirho = 0usize;

        for _ in 0..max_events_chirho {
            if let Some(event_chirho) = ring_chirho.pop_chirho() {
                let event_bytes_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &event_chirho as *const InputEventChirho as *const u8,
                        INPUT_EVENT_SIZE_CHIRHO,
                    )
                };
                buf_chirho[bytes_written_chirho..bytes_written_chirho + INPUT_EVENT_SIZE_CHIRHO]
                    .copy_from_slice(event_bytes_chirho);
                bytes_written_chirho += INPUT_EVENT_SIZE_CHIRHO;
            } else {
                break;
            }
        }

        Ok(bytes_written_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // evdev write is used for force-feedback; accept and ignore.
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE -- event devices are not seekable
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        // Extract the base ioctl (lower 16 bits) for variable-length ioctls.
        let base_cmd_chirho = cmd_chirho & 0x0000FFFF;
        // Extract the size from the ioctl encoding: bits 29:16 = size.
        let ioctl_size_chirho = ((cmd_chirho >> 16) & 0x3FFF) as usize;

        match cmd_chirho {
            // EVIOCGVERSION -- return evdev protocol version (0x010001 = 1.0.1).
            EVIOCGVERSION_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EFAULT_CHIRHO);
                }
                let version_chirho: i32 = 0x010001; // EV_VERSION
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &version_chirho as *const i32 as *const u8,
                        4,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(arg_chirho, src_chirho, 4).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            // EVIOCGID -- return device identification.
            EVIOCGID_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EFAULT_CHIRHO);
                }
                let id_chirho = InputIdChirho {
                    bustype_chirho: BUS_I8042_CHIRHO,
                    vendor_chirho: 0x4C49,  // "LI" for Lineluya
                    product_chirho: 0x4B42, // "KB" for keyboard
                    version_chirho: 0x0001,
                };
                let size_chirho = core::mem::size_of::<InputIdChirho>();
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &id_chirho as *const InputIdChirho as *const u8,
                        size_chirho,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, src_chirho, size_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            _ => {
                // Variable-length ioctls -- match on base command.
                match base_cmd_chirho {
                    // EVIOCGNAME -- return device name string.
                    x_chirho if x_chirho == EVIOCGNAME_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        let name_chirho = b"lineluya-keyboard\0";
                        let copy_len_chirho = name_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &name_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGPHYS -- return physical location string.
                    x_chirho if x_chirho == EVIOCGPHYS_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        let phys_chirho = b"isa0060/serio0/input0\0";
                        let copy_len_chirho = phys_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &phys_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGUNIQ -- return unique identifier (empty).
                    x_chirho if x_chirho == EVIOCGUNIQ_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        let uniq_chirho = b"\0";
                        let copy_len_chirho = uniq_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &uniq_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGPROP -- return device properties (none).
                    x_chirho if x_chirho == EVIOCGPROP_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        // Zero-fill the property bits buffer (no properties set).
                        let zero_buf_chirho = [0u8; 32];
                        let copy_len_chirho = zero_buf_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(0)
                    }

                    // EVIOCGBIT -- return capability bits.
                    x_chirho if x_chirho >= EVIOCGBIT_BASE_CHIRHO as u64
                             && x_chirho <= (EVIOCGBIT_BASE_CHIRHO as u64 + EV_MAX_CHIRHO as u64) =>
                    {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        let ev_type_chirho = (x_chirho - EVIOCGBIT_BASE_CHIRHO as u64) as u16;
                        self.handle_eviocgbit_chirho(ev_type_chirho, arg_chirho, ioctl_size_chirho)
                    }

                    // EVIOCGKEY -- return current key state (all keys up).
                    x_chirho if x_chirho == EVIOCGKEY_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 {
                            return Err(-EFAULT_CHIRHO);
                        }
                        // All keys reported as "up" (zero bits).
                        let zero_buf_chirho = [0u8; 96]; // KEY_MAX/8 + 1 ≈ 96 bytes
                        let copy_len_chirho = zero_buf_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(0)
                    }

                    _ => Err(-EINVAL_CHIRHO),
                }
            }
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

impl EvdevOpsChirho {
    /// Handle EVIOCGBIT ioctl -- return capability bit arrays for each event type.
    fn handle_eviocgbit_chirho(
        &self,
        ev_type_chirho: u16,
        arg_chirho: u64,
        max_size_chirho: usize,
    ) -> Result<i64, i64> {
        match ev_type_chirho {
            // EV type 0: which event types this device supports.
            // We support: EV_SYN (0), EV_KEY (1), EV_MSC (4), EV_LED (17), EV_REP (20).
            0 => {
                // Bit array: bit N set means EV type N is supported.
                // EV_SYN=0, EV_KEY=1, EV_MSC=4, EV_LED=0x11, EV_REP=0x14
                // Byte 0: bits 0,1,4 set = 0b0001_0011 = 0x13
                // Byte 2: bit 1 (LED=0x11, 0x11/8=2, 0x11%8=1) = 0x02
                // Byte 2: bit 4 (REP=0x14, 0x14/8=2, 0x14%8=4) = 0x10
                // So byte 2 = 0x12
                let mut bits_chirho = [0u8; 4]; // 32 event types / 8 = 4 bytes
                bits_chirho[0] = 0x13; // EV_SYN + EV_KEY + EV_MSC
                bits_chirho[2] = 0x12; // EV_LED + EV_REP
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_KEY (1): which keys this device can generate.
            // For a standard keyboard, we set bits for keys 1..127
            // (ESC through DEL, covering the full AT keyboard range).
            1 => {
                // KEY_MAX = 0x2ff => need (0x2ff/8)+1 = 96 bytes
                let mut bits_chirho = [0u8; 96];
                // Set bits for keys 1 (KEY_ESC) through 127 (KEY_COMPOSE).
                // This covers the standard PC keyboard range.
                for key_code_chirho in 1u16..=127 {
                    let byte_idx_chirho = (key_code_chirho / 8) as usize;
                    let bit_idx_chirho = (key_code_chirho % 8) as u8;
                    if byte_idx_chirho < bits_chirho.len() {
                        bits_chirho[byte_idx_chirho] |= 1 << bit_idx_chirho;
                    }
                }
                // Also set function keys and numpad (183-194 range).
                for key_code_chirho in 183u16..=194 {
                    let byte_idx_chirho = (key_code_chirho / 8) as usize;
                    let bit_idx_chirho = (key_code_chirho % 8) as u8;
                    if byte_idx_chirho < bits_chirho.len() {
                        bits_chirho[byte_idx_chirho] |= 1 << bit_idx_chirho;
                    }
                }
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_MSC (4): miscellaneous event bits.
            4 => {
                // MSC_SCAN = 4 => set bit 4
                let mut bits_chirho = [0u8; 1];
                bits_chirho[0] = 0x10; // bit 4 = MSC_SCAN
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_LED (0x11): LED capability bits.
            0x11 => {
                // LED_NUML=0, LED_CAPSL=1, LED_SCROLLL=2
                let mut bits_chirho = [0u8; 1];
                bits_chirho[0] = 0x07; // bits 0,1,2
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_REP (0x14): auto-repeat capability.
            0x14 => {
                // REP_DELAY=0, REP_PERIOD=1
                let mut bits_chirho = [0u8; 1];
                bits_chirho[0] = 0x03; // bits 0,1
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // All other event types: return zero bits.
            _ => {
                let zero_buf_chirho = [0u8; 32];
                let copy_len_chirho = zero_buf_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }
        }
    }
}

/// Static instance of /dev/input/event0 file operations.
pub static EVDEV_OPS_CHIRHO: EvdevOpsChirho = EvdevOpsChirho;

/// Push a key event into the evdev ring buffer.
///
/// Called from keyboard interrupt handler (future) or poll loop.
/// Generates both the key event and a SYN_REPORT event.
pub fn push_key_event_chirho(code_chirho: u16, value_chirho: i32) {
    let mut ring_chirho = EVENT_RING_CHIRHO.lock();

    // Key event
    ring_chirho.push_chirho(InputEventChirho {
        tv_sec_chirho: 0,   // TODO: fill from system clock
        tv_usec_chirho: 0,
        type_chirho: EV_KEY_CHIRHO,
        code_chirho,
        value_chirho,
    });

    // SYN_REPORT to mark end of this event batch
    ring_chirho.push_chirho(InputEventChirho {
        tv_sec_chirho: 0,
        tv_usec_chirho: 0,
        type_chirho: EV_SYN_CHIRHO,
        code_chirho: 0, // SYN_REPORT
        value_chirho: 0,
    });
}

// ============================================================================
// A2-X11-005: EvdevMouseOpsChirho — /dev/input/event1 (mouse) file operations
// ============================================================================

/// REL_X — relative X axis.
const REL_X_CHIRHO: u16 = 0x00;
/// REL_Y — relative Y axis.
const REL_Y_CHIRHO: u16 = 0x01;
/// REL_WHEEL — scroll wheel.
const REL_WHEEL_CHIRHO: u16 = 0x08;

/// BTN_LEFT — left mouse button.
const BTN_LEFT_CHIRHO: u16 = 0x110;
/// BTN_RIGHT — right mouse button.
const BTN_RIGHT_CHIRHO: u16 = 0x111;
/// BTN_MIDDLE — middle mouse button.
const BTN_MIDDLE_CHIRHO: u16 = 0x112;

/// BUS_USB for mouse device identification.
const BUS_USB_CHIRHO: u16 = 0x03;

/// Mouse event ring buffer (separate from keyboard).
pub static MOUSE_EVENT_RING_CHIRHO: spin::Mutex<EventRingChirho> =
    spin::Mutex::new(EventRingChirho::new_chirho());

/// File operations for `/dev/input/event1` (mouse evdev device).
///
/// Reports EV_REL (relative motion) and EV_KEY (mouse buttons) capabilities.
/// X11 and libinput use this to discover and read mouse input.
pub struct EvdevMouseOpsChirho;

impl FileOpsChirho for EvdevMouseOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        if buf_chirho.len() < INPUT_EVENT_SIZE_CHIRHO {
            return Err(-EINVAL_CHIRHO);
        }

        let nonblock_chirho = (file_chirho.flags_chirho
            & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;

        let mut ring_chirho = MOUSE_EVENT_RING_CHIRHO.lock();

        if !ring_chirho.has_events_chirho() {
            if nonblock_chirho {
                return Err(-EAGAIN_CHIRHO);
            }
            return Ok(0);
        }

        let max_events_chirho = buf_chirho.len() / INPUT_EVENT_SIZE_CHIRHO;
        let mut bytes_written_chirho = 0usize;

        for _ in 0..max_events_chirho {
            if let Some(event_chirho) = ring_chirho.pop_chirho() {
                let event_bytes_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &event_chirho as *const InputEventChirho as *const u8,
                        INPUT_EVENT_SIZE_CHIRHO,
                    )
                };
                buf_chirho[bytes_written_chirho..bytes_written_chirho + INPUT_EVENT_SIZE_CHIRHO]
                    .copy_from_slice(event_bytes_chirho);
                bytes_written_chirho += INPUT_EVENT_SIZE_CHIRHO;
            } else {
                break;
            }
        }

        Ok(bytes_written_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Ok(buf_chirho.len())
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
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        let base_cmd_chirho = cmd_chirho & 0x0000FFFF;
        let ioctl_size_chirho = ((cmd_chirho >> 16) & 0x3FFF) as usize;

        match cmd_chirho {
            // EVIOCGVERSION
            EVIOCGVERSION_CHIRHO => {
                if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                let version_chirho: i32 = 0x010001;
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &version_chirho as *const i32 as *const u8, 4,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(arg_chirho, src_chirho, 4).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            // EVIOCGID — mouse device identification
            EVIOCGID_CHIRHO => {
                if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                let id_chirho = InputIdChirho {
                    bustype_chirho: BUS_USB_CHIRHO,
                    vendor_chirho: 0x4C49,  // "LI" for Lineluya
                    product_chirho: 0x4D53, // "MS" for mouse
                    version_chirho: 0x0001,
                };
                let size_chirho = core::mem::size_of::<InputIdChirho>();
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &id_chirho as *const InputIdChirho as *const u8,
                        size_chirho,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, src_chirho, size_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            _ => {
                match base_cmd_chirho {
                    // EVIOCGNAME — "lineluya-mouse"
                    x_chirho if x_chirho == EVIOCGNAME_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let name_chirho = b"lineluya-mouse\0";
                        let copy_len_chirho = name_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &name_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGPHYS
                    x_chirho if x_chirho == EVIOCGPHYS_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let phys_chirho = b"usb-0000:00:01.0-1/input0\0";
                        let copy_len_chirho = phys_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &phys_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGUNIQ
                    x_chirho if x_chirho == EVIOCGUNIQ_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let uniq_chirho = b"\0";
                        let copy_len_chirho = uniq_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &uniq_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(copy_len_chirho as i64)
                    }

                    // EVIOCGPROP — no properties
                    x_chirho if x_chirho == EVIOCGPROP_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let zero_buf_chirho = [0u8; 32];
                        let copy_len_chirho = zero_buf_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(0)
                    }

                    // EVIOCGBIT — capability bits for mouse
                    x_chirho if x_chirho >= EVIOCGBIT_BASE_CHIRHO as u64
                             && x_chirho <= (EVIOCGBIT_BASE_CHIRHO as u64 + EV_MAX_CHIRHO as u64) =>
                    {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let ev_type_chirho = (x_chirho - EVIOCGBIT_BASE_CHIRHO as u64) as u16;
                        self.handle_mouse_eviocgbit_chirho(ev_type_chirho, arg_chirho, ioctl_size_chirho)
                    }

                    // EVIOCGKEY — all buttons up
                    x_chirho if x_chirho == EVIOCGKEY_BASE_CHIRHO as u64 => {
                        if arg_chirho == 0 { return Err(-EFAULT_CHIRHO); }
                        let zero_buf_chirho = [0u8; 96];
                        let copy_len_chirho = zero_buf_chirho.len().min(ioctl_size_chirho);
                        if crate::uaccess_chirho::copy_to_user_chirho(
                            arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                        ).is_err() {
                            return Err(-EFAULT_CHIRHO);
                        }
                        Ok(0)
                    }

                    _ => Err(-EINVAL_CHIRHO),
                }
            }
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

impl EvdevMouseOpsChirho {
    /// Handle EVIOCGBIT for mouse device — reports EV_REL, EV_KEY, EV_SYN capabilities.
    fn handle_mouse_eviocgbit_chirho(
        &self,
        ev_type_chirho: u16,
        arg_chirho: u64,
        max_size_chirho: usize,
    ) -> Result<i64, i64> {
        match ev_type_chirho {
            // EV type 0: which event types this device supports.
            // Mouse: EV_SYN (0), EV_KEY (1), EV_REL (2)
            0 => {
                let mut bits_chirho = [0u8; 4];
                // Bit 0 = EV_SYN, Bit 1 = EV_KEY, Bit 2 = EV_REL
                bits_chirho[0] = 0x07; // 0b0000_0111
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_KEY (1): mouse button bits.
            // BTN_LEFT=0x110, BTN_RIGHT=0x111, BTN_MIDDLE=0x112
            1 => {
                let mut bits_chirho = [0u8; 96]; // KEY_MAX/8 + 1
                // BTN_LEFT=0x110: byte 0x110/8=34, bit 0x110%8=0
                bits_chirho[34] |= 0x01; // BTN_LEFT
                // BTN_RIGHT=0x111: byte 34, bit 1
                bits_chirho[34] |= 0x02; // BTN_RIGHT
                // BTN_MIDDLE=0x112: byte 34, bit 2
                bits_chirho[34] |= 0x04; // BTN_MIDDLE
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // EV_REL (2): relative axis bits.
            // REL_X=0, REL_Y=1, REL_WHEEL=8
            2 => {
                let mut bits_chirho = [0u8; 2]; // need at least 2 bytes for bit 8
                bits_chirho[0] = 0x03; // REL_X (bit 0) + REL_Y (bit 1)
                bits_chirho[1] = 0x01; // REL_WHEEL (bit 8 => byte 1, bit 0)
                let copy_len_chirho = bits_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &bits_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(copy_len_chirho as i64)
            }

            // All other event types: return zero bits.
            _ => {
                let zero_buf_chirho = [0u8; 32];
                let copy_len_chirho = zero_buf_chirho.len().min(max_size_chirho);
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho, &zero_buf_chirho[..copy_len_chirho], copy_len_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }
        }
    }
}

/// Static instance of /dev/input/event1 (mouse) file operations.
pub static EVDEV_MOUSE_OPS_CHIRHO: EvdevMouseOpsChirho = EvdevMouseOpsChirho;

/// Push a mouse motion event into the mouse evdev ring buffer.
///
/// Generates REL_X and/or REL_Y events plus SYN_REPORT.
#[allow(dead_code)]
pub fn push_mouse_rel_event_chirho(dx_chirho: i32, dy_chirho: i32) {
    let mut ring_chirho = MOUSE_EVENT_RING_CHIRHO.lock();

    if dx_chirho != 0 {
        ring_chirho.push_chirho(InputEventChirho {
            tv_sec_chirho: 0,
            tv_usec_chirho: 0,
            type_chirho: EV_REL_CHIRHO,
            code_chirho: REL_X_CHIRHO,
            value_chirho: dx_chirho,
        });
    }
    if dy_chirho != 0 {
        ring_chirho.push_chirho(InputEventChirho {
            tv_sec_chirho: 0,
            tv_usec_chirho: 0,
            type_chirho: EV_REL_CHIRHO,
            code_chirho: REL_Y_CHIRHO,
            value_chirho: dy_chirho,
        });
    }

    ring_chirho.push_chirho(InputEventChirho {
        tv_sec_chirho: 0,
        tv_usec_chirho: 0,
        type_chirho: EV_SYN_CHIRHO,
        code_chirho: 0,
        value_chirho: 0,
    });
}

/// Push a mouse button event into the mouse evdev ring buffer.
///
/// `button_chirho`: BTN_LEFT, BTN_RIGHT, or BTN_MIDDLE code.
/// `pressed_chirho`: 1 for press, 0 for release.
#[allow(dead_code)]
pub fn push_mouse_button_event_chirho(button_chirho: u16, pressed_chirho: i32) {
    let mut ring_chirho = MOUSE_EVENT_RING_CHIRHO.lock();

    ring_chirho.push_chirho(InputEventChirho {
        tv_sec_chirho: 0,
        tv_usec_chirho: 0,
        type_chirho: EV_KEY_CHIRHO,
        code_chirho: button_chirho,
        value_chirho: pressed_chirho,
    });

    ring_chirho.push_chirho(InputEventChirho {
        tv_sec_chirho: 0,
        tv_usec_chirho: 0,
        type_chirho: EV_SYN_CHIRHO,
        code_chirho: 0,
        value_chirho: 0,
    });
}
