// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! UART 16550 serial port driver for bare-metal x86_64.
//!
//! Provides formatted printing over COM1 (0x3F8) at 38400 baud.
//! Used as the primary debug output channel for the Lineluya kernel.

use core::fmt;
use spin::Mutex;
use x86_64::instructions::port::Port;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Base I/O port address for COM1.
const COM1_BASE_CHIRHO: u16 = 0x3F8;

/// Baud rate divisor for 38400 baud (115200 / 38400 = 3).
const BAUD_DIVISOR_LOW_CHIRHO: u8 = 0x03;
const BAUD_DIVISOR_HIGH_CHIRHO: u8 = 0x00;

/// Line control: 8 data bits, no parity, 1 stop bit.
const LINE_CONTROL_8N1_CHIRHO: u8 = 0x03;

/// FIFO control: enable FIFO, clear both buffers, 14-byte threshold.
const FIFO_CONTROL_CHIRHO: u8 = 0xC7;

/// Modem control: IRQs enabled, RTS and DSR set.
const MODEM_CONTROL_CHIRHO: u8 = 0x0B;

/// DLAB (Divisor Latch Access Bit) enable value for line control register.
const DLAB_ENABLE_CHIRHO: u8 = 0x80;

/// Interrupt enable: received-data-available interrupt only.
const INTERRUPT_ENABLE_RX_CHIRHO: u8 = 0x01;

/// Interrupt disable value (written during initialization).
const INTERRUPT_DISABLE_CHIRHO: u8 = 0x00;

/// Bit mask for the Transmitter Holding Register Empty flag in the Line
/// Status Register (bit 5).
const LSR_THRE_CHIRHO: u8 = 0x20;

// ---------------------------------------------------------------------------
// Register offset helpers
// ---------------------------------------------------------------------------

/// UART register offsets from the base address.
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum RegisterOffsetChirho {
    /// Data register / divisor latch low byte.
    DataChirho = 0,
    /// Interrupt enable / divisor latch high byte.
    InterruptEnableChirho = 1,
    /// FIFO control register.
    FifoControlChirho = 2,
    /// Line control register.
    LineControlChirho = 3,
    /// Modem control register.
    ModemControlChirho = 4,
    /// Line status register (read-only).
    LineStatusChirho = 5,
}

// ---------------------------------------------------------------------------
// SerialPortChirho
// ---------------------------------------------------------------------------

/// A UART 16550 serial port.
///
/// Wraps the six I/O-port registers that compose a single UART and provides
/// safe (where possible) methods for initialization and byte-level I/O.
pub struct SerialPortChirho {
    /// Data / divisor-latch-low register.
    data_port_chirho: Port<u8>,
    /// Interrupt-enable / divisor-latch-high register.
    interrupt_enable_port_chirho: Port<u8>,
    /// FIFO control register.
    fifo_control_port_chirho: Port<u8>,
    /// Line control register.
    line_control_port_chirho: Port<u8>,
    /// Modem control register.
    modem_control_port_chirho: Port<u8>,
    /// Line status register.
    line_status_port_chirho: Port<u8>,
}

impl SerialPortChirho {
    /// Create a new `SerialPortChirho` bound to the given base I/O address.
    ///
    /// # Safety
    ///
    /// The caller must ensure `base_chirho` is the correct I/O base address of
    /// a real (or emulated) 16550-compatible UART. Creating a port object
    /// itself is safe; the unsafety is deferred to `init_chirho` and the
    /// write/read methods.
    pub const fn new_chirho(base_chirho: u16) -> Self {
        Self {
            data_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::DataChirho as u16,
            ),
            interrupt_enable_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::InterruptEnableChirho as u16,
            ),
            fifo_control_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::FifoControlChirho as u16,
            ),
            line_control_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::LineControlChirho as u16,
            ),
            modem_control_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::ModemControlChirho as u16,
            ),
            line_status_port_chirho: Port::new(
                base_chirho + RegisterOffsetChirho::LineStatusChirho as u16,
            ),
        }
    }

    /// Initialize the UART with standard 38400-baud 8N1 settings.
    ///
    /// This follows the canonical 16550 init sequence:
    /// 1. Disable all interrupts.
    /// 2. Enable DLAB and set the baud-rate divisor.
    /// 3. Configure 8N1 line protocol.
    /// 4. Enable and clear FIFOs.
    /// 5. Set modem-control bits (IRQ enable, RTS, DSR).
    /// 6. Re-enable the received-data-available interrupt.
    ///
    /// # Safety
    ///
    /// Must only be called once, and only when the underlying I/O ports
    /// correspond to actual UART hardware (physical or emulated).
    pub unsafe fn init_chirho(&mut self) {
        // Step 1 -- Disable all interrupts.
        self.interrupt_enable_port_chirho
            .write(INTERRUPT_DISABLE_CHIRHO);

        // Step 2 -- Enable DLAB so we can set the baud-rate divisor.
        self.line_control_port_chirho.write(DLAB_ENABLE_CHIRHO);

        // Step 3 -- Write the divisor (38400 baud).
        self.data_port_chirho.write(BAUD_DIVISOR_LOW_CHIRHO); // low byte
        self.interrupt_enable_port_chirho
            .write(BAUD_DIVISOR_HIGH_CHIRHO); // high byte

        // Step 4 -- 8 data bits, no parity, one stop bit (clears DLAB).
        self.line_control_port_chirho
            .write(LINE_CONTROL_8N1_CHIRHO);

        // Step 5 -- Enable FIFO, clear TX/RX queues, 14-byte threshold.
        self.fifo_control_port_chirho.write(FIFO_CONTROL_CHIRHO);

        // Step 6 -- IRQs enabled, RTS/DSR set.
        self.modem_control_port_chirho.write(MODEM_CONTROL_CHIRHO);

        // Step 7 -- Enable received-data-available interrupt.
        self.interrupt_enable_port_chirho
            .write(INTERRUPT_ENABLE_RX_CHIRHO);
    }

    /// Returns `true` when the transmit-holding register is empty, meaning
    /// we can safely write the next byte.
    fn is_transmit_empty_chirho(&mut self) -> bool {
        unsafe { self.line_status_port_chirho.read() & LSR_THRE_CHIRHO != 0 }
    }

    /// Block until the transmitter is ready, then send a single byte.
    pub fn send_byte_chirho(&mut self, byte_chirho: u8) {
        // Busy-wait for the transmit-holding register to drain.
        while !self.is_transmit_empty_chirho() {
            core::hint::spin_loop();
        }
        unsafe {
            self.data_port_chirho.write(byte_chirho);
        }
    }
}

/// Implement `core::fmt::Write` so that `write!` / `writeln!` macros work
/// with `SerialPortChirho` directly.
impl fmt::Write for SerialPortChirho {
    fn write_str(&mut self, s_chirho: &str) -> fmt::Result {
        for byte_chirho in s_chirho.bytes() {
            // Serial terminals expect `\r\n` for a proper line break.
            if byte_chirho == b'\n' {
                self.send_byte_chirho(b'\r');
            }
            self.send_byte_chirho(byte_chirho);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Global static serial port
// ---------------------------------------------------------------------------

/// Global COM1 serial port, protected by a spin-lock mutex.
///
/// Initialized lazily on first use via `spin::Once`.
static SERIAL1_ONCE_CHIRHO: spin::Once<Mutex<SerialPortChirho>> = spin::Once::new();

/// Obtain a reference to the global COM1 `Mutex<SerialPortChirho>`,
/// initializing it on first call.
fn serial1_chirho() -> &'static Mutex<SerialPortChirho> {
    SERIAL1_ONCE_CHIRHO.call_once(|| {
        let mut serial_port_chirho = SerialPortChirho::new_chirho(COM1_BASE_CHIRHO);
        unsafe {
            serial_port_chirho.init_chirho();
        }
        Mutex::new(serial_port_chirho)
    })
}

// ---------------------------------------------------------------------------
// Public initialization entry point
// ---------------------------------------------------------------------------

/// Explicitly initialize COM1.
///
/// This is safe to call multiple times; the underlying `spin::Once` ensures
/// the hardware init sequence runs exactly once.
pub fn init_chirho() {
    // Touching the Once triggers initialization if it hasn't happened yet.
    let _serial_chirho = serial1_chirho();
}

// ---------------------------------------------------------------------------
// Print helper used by macros
// ---------------------------------------------------------------------------

/// Low-level print function that writes formatted arguments to COM1.
///
/// This is **not** intended to be called directly -- use the
/// [`serial_print_chirho!`] or [`serial_println_chirho!`] macros instead.
#[doc(hidden)]
pub fn _print_chirho(args_chirho: fmt::Arguments) {
    use fmt::Write;
    // Disable interrupts while holding the serial lock to prevent deadlocks
    // if an interrupt handler also tries to print.
    x86_64::instructions::interrupts::without_interrupts(|| {
        serial1_chirho()
            .lock()
            .write_fmt(args_chirho)
            .expect("Serial write failed");
    });
}

// ---------------------------------------------------------------------------
// Macros
// ---------------------------------------------------------------------------

/// Print formatted text to the serial port (COM1) without a trailing newline.
///
/// Usage is identical to the standard `print!` macro.
#[macro_export]
macro_rules! serial_print_chirho {
    ($($arg_chirho:tt)*) => {
        $crate::serial_chirho::_print_chirho(format_args!($($arg_chirho)*))
    };
}

/// Print formatted text to the serial port (COM1) with a trailing newline.
///
/// Usage is identical to the standard `println!` macro.  Calling with no
/// arguments prints a bare newline.
#[macro_export]
macro_rules! serial_println_chirho {
    () => {
        $crate::serial_print_chirho!("\n")
    };
    ($($arg_chirho:tt)*) => {
        $crate::serial_print_chirho!("{}\n", format_args!($($arg_chirho)*))
    };
}

// ---------------------------------------------------------------------------
// IRQ-safe byte output (no locks, no formatting, no allocation)
// ---------------------------------------------------------------------------

/// Write a static string to serial port without formatting.
/// Safe to call from interrupt handlers — no locks, no allocation.
///
/// Directly polls the Line Status Register and writes bytes one at a time
/// to the COM1 data port, bypassing the `Mutex<SerialPortChirho>` entirely.
/// This avoids any risk of deadlock when an interrupt fires while the
/// serial lock is already held.
#[inline(always)]
pub fn serial_write_bytes_chirho(bytes_chirho: &[u8]) {
    for &b_chirho in bytes_chirho {
        unsafe {
            // Spin-wait for transmit buffer empty (LSR bit 5)
            while x86_64::instructions::port::Port::<u8>::new(
                COM1_BASE_CHIRHO + RegisterOffsetChirho::LineStatusChirho as u16,
            )
            .read()
                & LSR_THRE_CHIRHO
                == 0
            {}
            x86_64::instructions::port::Port::<u8>::new(
                COM1_BASE_CHIRHO + RegisterOffsetChirho::DataChirho as u16,
            )
            .write(b_chirho);
        }
    }
}

/// IRQ-safe log macro — outputs a static string with no formatting.
/// For use in interrupt handlers where deadlock-free output is critical.
///
/// Unlike [`serial_println_chirho!`], this macro bypasses the global serial
/// mutex and the `core::fmt` machinery.  It writes raw bytes directly to
/// the COM1 data port, making it safe to call even when the serial lock is
/// held by the interrupted code path.
///
/// # Example
///
/// ```ignore
/// log_irq_chirho!("timer tick");
/// ```
#[macro_export]
macro_rules! log_irq_chirho {
    ($msg_chirho:expr) => {
        $crate::serial_chirho::serial_write_bytes_chirho($msg_chirho.as_bytes());
        $crate::serial_chirho::serial_write_bytes_chirho(b"\r\n");
    };
}

// ---------------------------------------------------------------------------
// Debug macros
// ---------------------------------------------------------------------------

/// Debug-only serial print. Compiles to nothing unless the `debug_serial`
/// feature is enabled. Rust optimizes the empty branch away entirely.
///
/// Usage: `serial_debug_chirho!("[SUBSYS] debug info: {}", value);`
///
/// Enable with: `cargo build --features debug_serial`
#[macro_export]
macro_rules! serial_debug_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!($($arg_chirho)*);
        }
    };
}

/// Subsystem-tagged debug macros. Each compiles to nothing unless
/// the `debug_serial` feature is enabled. Tags help filter kernel
/// log output by subsystem.
///
/// Usage: `log_net_chirho!("TCP state: {:?}", state);`
/// Output: `[NET] TCP state: Established`

#[macro_export]
macro_rules! log_net_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[NET] {}", format_args!($($arg_chirho)*));
        }
    };
}

#[macro_export]
macro_rules! log_fs_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[FS] {}", format_args!($($arg_chirho)*));
        }
    };
}

#[macro_export]
macro_rules! log_mm_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[MM] {}", format_args!($($arg_chirho)*));
        }
    };
}

#[macro_export]
macro_rules! log_sched_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[SCHED] {}", format_args!($($arg_chirho)*));
        }
    };
}

#[macro_export]
macro_rules! log_proc_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[PROC] {}", format_args!($($arg_chirho)*));
        }
    };
}

#[macro_export]
macro_rules! log_drv_chirho {
    ($($arg_chirho:tt)*) => {
        #[cfg(feature = "debug_serial")]
        {
            $crate::serial_println_chirho!("[DRV] {}", format_args!($($arg_chirho)*));
        }
    };
}
