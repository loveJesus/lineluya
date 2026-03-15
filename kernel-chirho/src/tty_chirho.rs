// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! TTY subsystem for the Lineluya kernel.
//!
//! This module implements a minimal terminal (TTY) layer with line discipline
//! support, wrapping the serial port to provide canonical-mode input buffering
//! and character echo.
//!
//! # Components
//!
//! - [`TermiosChirho`] -- terminal attributes (input/output/control/local flags
//!   and the control-character array), matching Linux's `struct termios`.
//! - [`LineDisciplineChirho`] -- canonical-mode line buffering and echo logic.
//! - [`TtyChirho`] -- the TTY device, tying a serial port to a line discipline.
//! - [`TtyFileOpsChirho`] -- [`FileOpsChirho`] implementation for read/write
//!   through the line discipline.
//! - TCGETS / TCSETS ioctl handling for termios get/set.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::waitqueue_chirho::WaitQueueChirho;

// ============================================================================
// termios flag constants
// ============================================================================

/// Local flag: enable echo of input characters.
pub const ECHO_CHIRHO: u32 = 0o10;
/// Local flag: canonical (line-buffered) mode.
pub const ICANON_CHIRHO: u32 = 0o2;
/// Local flag: enable signal generation (SIGINT on Ctrl-C, etc.).
pub const ISIG_CHIRHO: u32 = 0o1;

/// Output flag: post-process output.
pub const OPOST_CHIRHO: u32 = 0o1;
/// Output flag: map NL to CR-NL on output.
pub const ONLCR_CHIRHO: u32 = 0o4;

/// Size of the c_cc control-character array (matches Linux NCCS).
const NCCS_CHIRHO: usize = 32;

// ============================================================================
// TermiosChirho
// ============================================================================

/// Terminal I/O settings, modelled on Linux's `struct termios`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TermiosChirho {
    /// Input mode flags.
    pub c_iflag_chirho: u32,
    /// Output mode flags.
    pub c_oflag_chirho: u32,
    /// Control mode flags.
    pub c_cflag_chirho: u32,
    /// Local mode flags.
    pub c_lflag_chirho: u32,
    /// Line discipline (unused, kept for ABI compatibility).
    pub c_line_chirho: u8,
    /// Control characters.
    pub c_cc_chirho: [u8; NCCS_CHIRHO],
    /// Input baud rate (stored but unused).
    pub c_ispeed_chirho: u32,
    /// Output baud rate (stored but unused).
    pub c_ospeed_chirho: u32,
}

impl TermiosChirho {
    /// Return a default termios suitable for a serial console.
    pub const fn default_chirho() -> Self {
        Self {
            c_iflag_chirho: 0,
            c_oflag_chirho: OPOST_CHIRHO | ONLCR_CHIRHO,
            c_cflag_chirho: 0,
            c_lflag_chirho: ECHO_CHIRHO | ICANON_CHIRHO | ISIG_CHIRHO,
            c_line_chirho: 0,
            c_cc_chirho: [0u8; NCCS_CHIRHO],
            c_ispeed_chirho: 38400,
            c_ospeed_chirho: 38400,
        }
    }
}

// ============================================================================
// LineDisciplineChirho
// ============================================================================

/// Maximum line buffer length in canonical mode.
const LINE_BUF_SIZE_CHIRHO: usize = 4096;

/// Line discipline implementing canonical mode and echo.
///
/// In canonical mode, characters are buffered until a newline (`\n`) or EOF
/// is received.  When echo is enabled every input character is written back
/// to the output side of the TTY.
pub struct LineDisciplineChirho {
    /// Accumulated input buffer (canonical mode).
    buf_chirho: Vec<u8>,
    /// Completed lines ready for reading.
    ready_chirho: Vec<u8>,
    /// Current termios settings.
    termios_chirho: TermiosChirho,
}

impl LineDisciplineChirho {
    /// Create a new line discipline with default termios.
    pub fn new_chirho() -> Self {
        Self {
            buf_chirho: Vec::with_capacity(LINE_BUF_SIZE_CHIRHO),
            ready_chirho: Vec::new(),
            termios_chirho: TermiosChirho::default_chirho(),
        }
    }

    /// Process an incoming character from the serial port.
    ///
    /// In canonical mode the character is appended to the line buffer; when a
    /// newline arrives the entire line (including the newline) is moved to the
    /// ready queue.  If echo is enabled the character is sent back out.
    pub fn receive_char_chirho(&mut self, ch_chirho: u8) {
        let canonical_chirho =
            self.termios_chirho.c_lflag_chirho & ICANON_CHIRHO != 0;
        let echo_chirho =
            self.termios_chirho.c_lflag_chirho & ECHO_CHIRHO != 0;

        if echo_chirho {
            // Echo the character to the serial console.
            if ch_chirho == b'\r' || ch_chirho == b'\n' {
                crate::serial_print_chirho!("\r\n");
            } else if ch_chirho == 0x7f || ch_chirho == 0x08 {
                // Backspace: erase the previous character on screen.
                crate::serial_print_chirho!("\x08 \x08");
            } else {
                crate::serial_print_chirho!("{}", ch_chirho as char);
            }
        }

        if canonical_chirho {
            // Handle backspace in canonical mode.
            if ch_chirho == 0x7f || ch_chirho == 0x08 {
                self.buf_chirho.pop();
                return;
            }

            // Treat CR as newline.
            let effective_chirho = if ch_chirho == b'\r' { b'\n' } else { ch_chirho };

            self.buf_chirho.push(effective_chirho);

            if effective_chirho == b'\n'
                || self.buf_chirho.len() >= LINE_BUF_SIZE_CHIRHO
            {
                // Flush the line buffer to the ready queue.
                self.ready_chirho.extend_from_slice(&self.buf_chirho);
                self.buf_chirho.clear();
            }
        } else {
            // Raw mode: every character is immediately available.
            self.ready_chirho.push(ch_chirho);
        }
    }

    /// Read up to `count_chirho` bytes from the ready queue.
    ///
    /// Returns a `Vec` of the bytes consumed (may be fewer than requested if
    /// not enough data is available).
    pub fn read_chirho(&mut self, count_chirho: usize) -> Vec<u8> {
        let available_chirho = count_chirho.min(self.ready_chirho.len());
        let data_chirho: Vec<u8> =
            self.ready_chirho.drain(..available_chirho).collect();
        data_chirho
    }

    /// Return `true` if there is data ready to be read.
    pub fn has_data_chirho(&self) -> bool {
        !self.ready_chirho.is_empty()
    }

    /// Get a reference to the current termios.
    pub fn termios_chirho(&self) -> &TermiosChirho {
        &self.termios_chirho
    }

    /// Set new termios settings.
    pub fn set_termios_chirho(&mut self, termios_chirho: TermiosChirho) {
        self.termios_chirho = termios_chirho;
    }
}

// ============================================================================
// TtyChirho
// ============================================================================

/// A TTY device wrapping a serial port with a line discipline.
pub struct TtyChirho {
    /// The line discipline that processes input/output.
    pub ldisc_chirho: Mutex<LineDisciplineChirho>,
    /// Wait queue for tasks blocked on read.
    pub read_wait_chirho: WaitQueueChirho,
}

impl TtyChirho {
    /// Create a new TTY device with default settings.
    pub fn new_chirho() -> Self {
        Self {
            ldisc_chirho: Mutex::new(LineDisciplineChirho::new_chirho()),
            read_wait_chirho: WaitQueueChirho::new_chirho(),
        }
    }

    /// Feed a character into the TTY (called from the keyboard/serial
    /// interrupt handler).
    ///
    /// In canonical mode, waiters are only woken when a complete line becomes
    /// available (i.e. when Enter / newline is received or the line buffer is
    /// full).  In raw mode every character wakes waiters immediately.
    pub fn input_char_chirho(&self, ch_chirho: u8) {
        let mut ldisc_chirho = self.ldisc_chirho.lock();
        ldisc_chirho.receive_char_chirho(ch_chirho);
        // Only wake blocked readers when the line discipline actually has
        // data ready for consumption.  In canonical mode this is only true
        // after a newline (or buffer overflow); in raw mode every char
        // produces ready data.
        let has_data_chirho = ldisc_chirho.has_data_chirho();
        drop(ldisc_chirho);

        if has_data_chirho {
            crate::waitqueue_chirho::wake_up_chirho(&self.read_wait_chirho);
        }
    }

    /// Write output bytes, applying output post-processing (OPOST/ONLCR).
    pub fn write_output_chirho(&self, data_chirho: &[u8]) {
        let ldisc_chirho = self.ldisc_chirho.lock();
        let opost_chirho =
            ldisc_chirho.termios_chirho.c_oflag_chirho & OPOST_CHIRHO != 0;
        let onlcr_chirho =
            ldisc_chirho.termios_chirho.c_oflag_chirho & ONLCR_CHIRHO != 0;
        drop(ldisc_chirho);

        for &byte_chirho in data_chirho {
            if opost_chirho && onlcr_chirho && byte_chirho == b'\n' {
                crate::serial_print_chirho!("\r\n");
            } else {
                crate::serial_print_chirho!("{}", byte_chirho as char);
            }
        }
    }
}

/// Global TTY0 instance.
static TTY0_CHIRHO: spin::Once<Arc<TtyChirho>> = spin::Once::new();

/// Get (or initialise) the global TTY0.
pub fn tty0_chirho() -> Arc<TtyChirho> {
    TTY0_CHIRHO
        .call_once(|| Arc::new(TtyChirho::new_chirho()))
        .clone()
}

// ============================================================================
// TtyFileOpsChirho -- VFS integration
// ============================================================================

/// File operations implementation for TTY devices.
pub struct TtyFileOpsChirho {
    /// Reference to the TTY this file operates on.
    tty_chirho: Arc<TtyChirho>,
}

impl TtyFileOpsChirho {
    /// Create a new `TtyFileOpsChirho` for the given TTY.
    pub fn new_chirho(tty_chirho: Arc<TtyChirho>) -> Self {
        Self { tty_chirho }
    }
}

impl FileOpsChirho for TtyFileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        if buf_chirho.is_empty() {
            return Ok(0);
        }

        // If O_NONBLOCK is set, return -EAGAIN when no data is available.
        let nonblock_chirho = (file_chirho.flags_chirho
            & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;

        if nonblock_chirho {
            let mut ldisc_chirho = self.tty_chirho.ldisc_chirho.lock();
            if !ldisc_chirho.has_data_chirho() {
                return Err(crate::syscall_chirho::EAGAIN_CHIRHO);
            }
            let data_chirho = ldisc_chirho.read_chirho(buf_chirho.len());
            let n_chirho = data_chirho.len();
            buf_chirho[..n_chirho].copy_from_slice(&data_chirho);
            return Ok(n_chirho);
        }

        // Blocking read: sleep on the TTY wait queue until data is available.
        let tty_ref_chirho = self.tty_chirho.clone();
        crate::waitqueue_chirho::wait_event_chirho(
            &self.tty_chirho.read_wait_chirho,
            || tty_ref_chirho.ldisc_chirho.lock().has_data_chirho(),
        );

        // Data is now available — read it.
        let mut ldisc_chirho = self.tty_chirho.ldisc_chirho.lock();
        let data_chirho = ldisc_chirho.read_chirho(buf_chirho.len());
        let n_chirho = data_chirho.len();
        buf_chirho[..n_chirho].copy_from_slice(&data_chirho);
        Ok(n_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        self.tty_chirho.write_output_chirho(buf_chirho);
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(29) // ESPIPE -- TTYs are not seekable
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        // TCGETS (0x5401) -- get termios
        const TCGETS_CHIRHO: u64 = 0x5401;
        // TCSETS (0x5402) -- set termios
        const TCSETS_CHIRHO: u64 = 0x5402;
        // TIOCGWINSZ (0x5413) -- get window size
        const TIOCGWINSZ_CHIRHO: u64 = 0x5413;

        match cmd_chirho {
            TCGETS_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(14); // EFAULT
                }
                let ldisc_chirho = self.tty_chirho.ldisc_chirho.lock();
                let termios_chirho = ldisc_chirho.termios_chirho();
                let size_chirho = core::mem::size_of::<TermiosChirho>();
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        termios_chirho as *const TermiosChirho as *const u8,
                        size_chirho,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho,
                    src_chirho,
                    size_chirho,
                )
                .is_err()
                {
                    return Err(14); // EFAULT
                }
                Ok(0)
            }
            TCSETS_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(14); // EFAULT
                }
                let size_chirho = core::mem::size_of::<TermiosChirho>();
                let mut buf_chirho = [0u8; 128]; // TermiosChirho is well under 128 bytes
                if crate::uaccess_chirho::copy_from_user_chirho(
                    &mut buf_chirho[..size_chirho],
                    arg_chirho,
                    size_chirho,
                )
                .is_err()
                {
                    return Err(14); // EFAULT
                }
                let termios_chirho = unsafe {
                    core::ptr::read(buf_chirho.as_ptr() as *const TermiosChirho)
                };
                self.tty_chirho
                    .ldisc_chirho
                    .lock()
                    .set_termios_chirho(termios_chirho);
                Ok(0)
            }
            TIOCGWINSZ_CHIRHO => {
                // Return a default 80x24 window size.
                if arg_chirho == 0 {
                    return Err(14); // EFAULT
                }
                let winsize_chirho: [u8; 8] = {
                    let row_chirho: u16 = 24;
                    let col_chirho: u16 = 80;
                    let xpix_chirho: u16 = 0;
                    let ypix_chirho: u16 = 0;
                    let mut buf_chirho = [0u8; 8];
                    buf_chirho[0..2]
                        .copy_from_slice(&row_chirho.to_ne_bytes());
                    buf_chirho[2..4]
                        .copy_from_slice(&col_chirho.to_ne_bytes());
                    buf_chirho[4..6]
                        .copy_from_slice(&xpix_chirho.to_ne_bytes());
                    buf_chirho[6..8]
                        .copy_from_slice(&ypix_chirho.to_ne_bytes());
                    buf_chirho
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho,
                    &winsize_chirho,
                    8,
                )
                .is_err()
                {
                    return Err(14); // EFAULT
                }
                Ok(0)
            }
            // TIOCGPGRP (0x540F) — return foreground process group
            0x540F => {
                if arg_chirho != 0 {
                    let pgid_chirho: i32 = 0; // current process group
                    let _ = crate::uaccess_chirho::copy_to_user_chirho(
                        arg_chirho,
                        &pgid_chirho.to_ne_bytes(),
                        4,
                    );
                }
                Ok(0)
            }
            // TIOCSPGRP (0x5410) — set foreground process group
            0x5410 => Ok(0), // silently accept
            _ => Err(25), // ENOTTY (positive errno)
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(20) // ENOTDIR
    }
}
