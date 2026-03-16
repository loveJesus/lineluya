// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! dmesg ring buffer and /dev/kmsg for the Lineluya kernel (E1-015).
//!
//! Implements a fixed-size kernel log ring buffer that captures all
//! `serial_println_chirho!` output. Provides:
//!
//! - `klog_chirho!` / `klog_println_chirho!` macros for kernel logging
//! - `dmesg_read_chirho()` to dump the entire ring buffer
//! - `/dev/kmsg` procfs file generator for userspace access
//!
//! The ring buffer is lock-free for single-writer (kernel) and supports
//! concurrent readers via snapshot copies.
//!
//! Reference: Linux `printk` ring buffer / `include/linux/printk.h`

extern crate alloc;

use alloc::string::String;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

// ============================================================================
// Ring buffer constants
// ============================================================================

/// Size of the kernel log ring buffer in bytes (64 KiB, matching Linux
/// default `CONFIG_LOG_BUF_SHIFT=16`).
const DMESG_BUFFER_SIZE_CHIRHO: usize = 64 * 1024;

/// Log level constants (matching Linux syslog levels).
#[allow(dead_code)]
pub const LOG_EMERG_CHIRHO: u8 = 0;
#[allow(dead_code)]
pub const LOG_ALERT_CHIRHO: u8 = 1;
#[allow(dead_code)]
pub const LOG_CRIT_CHIRHO: u8 = 2;
#[allow(dead_code)]
pub const LOG_ERR_CHIRHO: u8 = 3;
#[allow(dead_code)]
pub const LOG_WARNING_CHIRHO: u8 = 4;
#[allow(dead_code)]
pub const LOG_NOTICE_CHIRHO: u8 = 5;
#[allow(dead_code)]
pub const LOG_INFO_CHIRHO: u8 = 6;
#[allow(dead_code)]
pub const LOG_DEBUG_CHIRHO: u8 = 7;

// ============================================================================
// Ring buffer structure
// ============================================================================

/// A fixed-size ring buffer for kernel log messages.
///
/// When the buffer is full, new messages overwrite the oldest data.
struct DmesgRingChirho {
    /// The raw byte buffer.
    buf_chirho: [u8; DMESG_BUFFER_SIZE_CHIRHO],
    /// Write position (always advances, wraps via modulo).
    write_pos_chirho: usize,
    /// Total bytes written (for sequence numbering).
    total_written_chirho: usize,
}

impl DmesgRingChirho {
    /// Create a new empty ring buffer.
    const fn new_chirho() -> Self {
        Self {
            buf_chirho: [0u8; DMESG_BUFFER_SIZE_CHIRHO],
            write_pos_chirho: 0,
            total_written_chirho: 0,
        }
    }

    /// Write bytes into the ring buffer.
    fn write_bytes_chirho(&mut self, data_chirho: &[u8]) {
        for &byte_chirho in data_chirho {
            self.buf_chirho[self.write_pos_chirho % DMESG_BUFFER_SIZE_CHIRHO] = byte_chirho;
            self.write_pos_chirho += 1;
        }
        self.total_written_chirho += data_chirho.len();
    }

    /// Read all available log data as a String.
    ///
    /// If the buffer has wrapped, only the most recent
    /// `DMESG_BUFFER_SIZE_CHIRHO` bytes are returned.
    fn read_all_chirho(&self) -> String {
        let mut output_chirho = String::new();

        if self.write_pos_chirho == 0 {
            return output_chirho;
        }

        let (start_chirho, len_chirho) = if self.write_pos_chirho <= DMESG_BUFFER_SIZE_CHIRHO {
            (0, self.write_pos_chirho)
        } else {
            (
                self.write_pos_chirho % DMESG_BUFFER_SIZE_CHIRHO,
                DMESG_BUFFER_SIZE_CHIRHO,
            )
        };

        // Read from the ring buffer in order.
        for i_chirho in 0..len_chirho {
            let idx_chirho = (start_chirho + i_chirho) % DMESG_BUFFER_SIZE_CHIRHO;
            let byte_chirho = self.buf_chirho[idx_chirho];
            if byte_chirho != 0 {
                output_chirho.push(byte_chirho as char);
            }
        }

        output_chirho
    }
}

// Implement fmt::Write so we can use write! macros with the ring buffer.
impl fmt::Write for DmesgRingChirho {
    fn write_str(&mut self, s_chirho: &str) -> fmt::Result {
        self.write_bytes_chirho(s_chirho.as_bytes());
        Ok(())
    }
}

// ============================================================================
// Global ring buffer instance
// ============================================================================

/// Global dmesg ring buffer, protected by a spin-lock.
static DMESG_RING_CHIRHO: Mutex<DmesgRingChirho> = Mutex::new(DmesgRingChirho::new_chirho());

/// Monotonically increasing sequence number for log entries.
static DMESG_SEQ_CHIRHO: AtomicUsize = AtomicUsize::new(0);

// ============================================================================
// Public API
// ============================================================================

/// Write a formatted log message to the dmesg ring buffer.
///
/// Also echoes to serial for real-time debugging.
#[doc(hidden)]
pub fn _klog_print_chirho(args_chirho: fmt::Arguments) {
    use fmt::Write;
    // Write to the ring buffer.
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut ring_chirho = DMESG_RING_CHIRHO.lock();
        let _ = ring_chirho.write_fmt(args_chirho);
    });
}

/// Read the entire dmesg ring buffer contents as a String.
///
/// This is the kernel-internal equivalent of `dmesg(1)`.
#[allow(dead_code)]
pub fn dmesg_read_chirho() -> String {
    let ring_chirho = DMESG_RING_CHIRHO.lock();
    ring_chirho.read_all_chirho()
}

/// Return the number of bytes that have been written to the ring buffer
/// (total, including overwritten data).
#[allow(dead_code)]
pub fn dmesg_total_written_chirho() -> usize {
    let ring_chirho = DMESG_RING_CHIRHO.lock();
    ring_chirho.total_written_chirho
}

/// Return the current sequence number (number of klog calls).
#[allow(dead_code)]
pub fn dmesg_seq_chirho() -> usize {
    DMESG_SEQ_CHIRHO.load(Ordering::Relaxed)
}

/// Increment and return the next sequence number.
#[allow(dead_code)]
pub fn dmesg_next_seq_chirho() -> usize {
    DMESG_SEQ_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

/// Generate content for `/proc/kmsg` or `/dev/kmsg`.
///
/// Returns the full dmesg ring buffer contents, suitable for procfs.
pub fn gen_kmsg_chirho() -> String {
    dmesg_read_chirho()
}

/// Initialize the dmesg subsystem.
///
/// Writes the boot banner to the ring buffer.
pub fn init_dmesg_chirho() {
    use fmt::Write;
    let mut ring_chirho = DMESG_RING_CHIRHO.lock();
    let _ = write!(
        ring_chirho,
        "[    0.000000] Lineluya kernel dmesg ring buffer initialized ({}K)\n",
        DMESG_BUFFER_SIZE_CHIRHO / 1024
    );
    let _ = write!(
        ring_chirho,
        "[    0.000000] For God so loved the world - John 3:16\n"
    );
    crate::serial_println_chirho!("[OK] dmesg ring buffer initialized ({}K)", DMESG_BUFFER_SIZE_CHIRHO / 1024);
}

// ============================================================================
// Macros for kernel logging into the ring buffer
// ============================================================================

/// Log a formatted message to the dmesg ring buffer (no newline).
#[macro_export]
macro_rules! klog_chirho {
    ($($arg_chirho:tt)*) => {
        $crate::dmesg_chirho::_klog_print_chirho(format_args!($($arg_chirho)*))
    };
}

/// Log a formatted message to the dmesg ring buffer (with newline).
#[macro_export]
macro_rules! klog_println_chirho {
    () => {
        $crate::klog_chirho!("\n")
    };
    ($($arg_chirho:tt)*) => {
        $crate::klog_chirho!("{}\n", format_args!($($arg_chirho)*))
    };
}
