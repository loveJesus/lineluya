// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! VGA text-mode buffer driver for the Lineluya bare-metal kernel (E1-003).
//!
//! Provides a writer that outputs characters to the VGA text buffer at
//! physical address 0xb8000. All writes use `core::ptr::write_volatile`
//! to ensure the compiler never elides stores to video memory.
//!
//! Enhanced for real hardware boot:
//! - Hardware cursor position tracking via CRTC I/O ports
//! - Screen clearing with configurable colors
//! - Tab character support
//! - Backspace support
//! - Row/column tracking for both row dimensions

use core::fmt;
use spin::{Lazy, Mutex};
use x86_64::instructions::port::Port;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Base physical address of the VGA text buffer.
const VGA_BUFFER_ADDRESS_CHIRHO: usize = 0xb8000;

/// Number of character columns in standard VGA text mode.
const BUFFER_WIDTH_CHIRHO: usize = 80;

/// Number of character rows in standard VGA text mode.
const BUFFER_HEIGHT_CHIRHO: usize = 25;

/// Tab stop width.
const TAB_WIDTH_CHIRHO: usize = 8;

/// VGA CRTC index register (3D4h).
const CRTC_INDEX_PORT_CHIRHO: u16 = 0x3D4;

/// VGA CRTC data register (3D5h).
const CRTC_DATA_PORT_CHIRHO: u16 = 0x3D5;

/// CRTC register index for cursor location high byte.
const CRTC_CURSOR_HIGH_CHIRHO: u8 = 0x0E;

/// CRTC register index for cursor location low byte.
const CRTC_CURSOR_LOW_CHIRHO: u8 = 0x0F;

/// CRTC register index for cursor start scanline.
const CRTC_CURSOR_START_CHIRHO: u8 = 0x0A;

/// CRTC register index for cursor end scanline.
const CRTC_CURSOR_END_CHIRHO: u8 = 0x0B;

// ---------------------------------------------------------------------------
// ColorChirho — standard VGA 4-bit palette
// ---------------------------------------------------------------------------

/// The 16 standard VGA text-mode colors.
///
/// Each variant maps directly to the hardware color index (0x0 – 0xF).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColorChirho {
    BlackChirho      = 0x0,
    BlueChirho       = 0x1,
    GreenChirho      = 0x2,
    CyanChirho       = 0x3,
    RedChirho        = 0x4,
    MagentaChirho    = 0x5,
    BrownChirho      = 0x6,
    LightGrayChirho  = 0x7,
    DarkGrayChirho   = 0x8,
    LightBlueChirho  = 0x9,
    LightGreenChirho = 0xA,
    LightCyanChirho  = 0xB,
    LightRedChirho   = 0xC,
    PinkChirho       = 0xD,
    YellowChirho     = 0xE,
    WhiteChirho      = 0xF,
}

// ---------------------------------------------------------------------------
// ColorCodeChirho — packed foreground + background
// ---------------------------------------------------------------------------

/// A packed VGA color byte: bits 0..3 = foreground, bits 4..6 = background.
///
/// Bit 7 controls blinking and is left unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCodeChirho(u8);

impl ColorCodeChirho {
    /// Create a new color code from a foreground and background color.
    pub const fn new_chirho(
        foreground_chirho: ColorChirho,
        background_chirho: ColorChirho,
    ) -> Self {
        Self((background_chirho as u8) << 4 | (foreground_chirho as u8))
    }
}

// ---------------------------------------------------------------------------
// ScreenCharChirho — a single character cell
// ---------------------------------------------------------------------------

/// One character cell in the VGA text buffer (2 bytes: character + color).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ScreenCharChirho {
    pub ascii_character_chirho: u8,
    pub color_code_chirho: ColorCodeChirho,
}

// ---------------------------------------------------------------------------
// BufferChirho — the raw 80x25 VGA framebuffer
// ---------------------------------------------------------------------------

/// The raw VGA text framebuffer laid out as `BUFFER_HEIGHT_CHIRHO` rows of
/// `BUFFER_WIDTH_CHIRHO` character cells.
///
/// All reads and writes to this structure **must** go through volatile
/// operations so that the compiler cannot optimise them away.
#[repr(transparent)]
pub struct BufferChirho {
    chars_chirho: [[ScreenCharChirho; BUFFER_WIDTH_CHIRHO]; BUFFER_HEIGHT_CHIRHO],
}

impl BufferChirho {
    /// Write a single `ScreenCharChirho` to row/column using a volatile store.
    #[inline]
    fn write_volatile_chirho(
        &mut self,
        row_chirho: usize,
        col_chirho: usize,
        value_chirho: ScreenCharChirho,
    ) {
        // SAFETY: The caller guarantees that `self` points to the VGA buffer
        // at 0xb8000 and that row/col are within bounds.
        unsafe {
            core::ptr::write_volatile(
                &mut self.chars_chirho[row_chirho][col_chirho] as *mut ScreenCharChirho,
                value_chirho,
            );
        }
    }

    /// Read a single `ScreenCharChirho` from row/column using a volatile load.
    #[inline]
    fn read_volatile_chirho(
        &self,
        row_chirho: usize,
        col_chirho: usize,
    ) -> ScreenCharChirho {
        // SAFETY: Same as `write_volatile_chirho`.
        unsafe {
            core::ptr::read_volatile(
                &self.chars_chirho[row_chirho][col_chirho] as *const ScreenCharChirho,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// WriterChirho — high-level character output
// ---------------------------------------------------------------------------

/// A writer that keeps track of the current cursor column and color, and
/// outputs characters to the VGA text buffer.
///
/// Enhanced for real hardware with:
/// - Current row tracking (not always last row)
/// - Hardware cursor updates via CRTC registers
/// - Tab and backspace support
pub struct WriterChirho {
    column_position_chirho: usize,
    row_position_chirho: usize,
    color_code_chirho: ColorCodeChirho,
    buffer_chirho: &'static mut BufferChirho,
}

impl WriterChirho {
    /// Write a single byte to the VGA buffer.
    ///
    /// Printable ASCII bytes are placed at the current cursor position.
    /// Special characters: `\n` (newline), `\t` (tab), `\x08` (backspace).
    /// Any other non-printable byte is replaced by `0xFE`.
    pub fn write_byte_chirho(&mut self, byte_chirho: u8) {
        match byte_chirho {
            b'\n' => self.new_line_chirho(),
            b'\t' => {
                // Tab: advance to next tab stop.
                let spaces_chirho = TAB_WIDTH_CHIRHO
                    - (self.column_position_chirho % TAB_WIDTH_CHIRHO);
                for _ in 0..spaces_chirho {
                    self.write_byte_chirho(b' ');
                }
            }
            0x08 => {
                // Backspace: move cursor back one position and clear.
                if self.column_position_chirho > 0 {
                    self.column_position_chirho -= 1;
                    let blank_chirho = ScreenCharChirho {
                        ascii_character_chirho: b' ',
                        color_code_chirho: self.color_code_chirho,
                    };
                    let row_chirho = self.row_position_chirho;
                    let col_chirho = self.column_position_chirho;
                    self.buffer_chirho
                        .write_volatile_chirho(row_chirho, col_chirho, blank_chirho);
                    self.update_hardware_cursor_chirho();
                }
            }
            byte_chirho => {
                if self.column_position_chirho >= BUFFER_WIDTH_CHIRHO {
                    self.new_line_chirho();
                }

                let row_chirho = self.row_position_chirho;
                let col_chirho = self.column_position_chirho;

                let screen_char_chirho = ScreenCharChirho {
                    ascii_character_chirho: byte_chirho,
                    color_code_chirho: self.color_code_chirho,
                };
                self.buffer_chirho
                    .write_volatile_chirho(row_chirho, col_chirho, screen_char_chirho);

                self.column_position_chirho += 1;
            }
        }
    }

    /// Write a string slice to the VGA buffer.
    ///
    /// Only bytes in the printable ASCII range (`0x20 ..= 0x7E`), `\n`,
    /// `\t`, and backspace are output directly; everything else is replaced
    /// by `0xFE`.
    pub fn write_string_chirho(&mut self, string_chirho: &str) {
        for byte_chirho in string_chirho.bytes() {
            match byte_chirho {
                // Printable ASCII, newline, tab, or backspace — write as-is.
                0x20..=0x7E | b'\n' | b'\t' | 0x08 => self.write_byte_chirho(byte_chirho),
                // Non-printable / multibyte UTF-8 continuation — substitute.
                _ => self.write_byte_chirho(0xFE),
            }
        }
        self.update_hardware_cursor_chirho();
    }

    /// Scroll the entire buffer up by one row and clear the last row.
    fn new_line_chirho(&mut self) {
        if self.row_position_chirho < BUFFER_HEIGHT_CHIRHO - 1 {
            // Still have room — just move to the next row.
            self.row_position_chirho += 1;
        } else {
            // At the bottom — scroll everything up by one.
            for row_chirho in 1..BUFFER_HEIGHT_CHIRHO {
                for col_chirho in 0..BUFFER_WIDTH_CHIRHO {
                    let character_chirho =
                        self.buffer_chirho.read_volatile_chirho(row_chirho, col_chirho);
                    self.buffer_chirho
                        .write_volatile_chirho(row_chirho - 1, col_chirho, character_chirho);
                }
            }
            // Clear the last row.
            self.clear_row_chirho(BUFFER_HEIGHT_CHIRHO - 1);
        }
        self.column_position_chirho = 0;
        self.update_hardware_cursor_chirho();
    }

    /// Overwrite an entire row with blank (space) characters.
    fn clear_row_chirho(&mut self, row_chirho: usize) {
        let blank_chirho = ScreenCharChirho {
            ascii_character_chirho: b' ',
            color_code_chirho: self.color_code_chirho,
        };
        for col_chirho in 0..BUFFER_WIDTH_CHIRHO {
            self.buffer_chirho
                .write_volatile_chirho(row_chirho, col_chirho, blank_chirho);
        }
    }

    /// Clear the entire screen and reset cursor to top-left.
    #[allow(dead_code)]
    pub fn clear_screen_chirho(&mut self) {
        for row_chirho in 0..BUFFER_HEIGHT_CHIRHO {
            self.clear_row_chirho(row_chirho);
        }
        self.column_position_chirho = 0;
        self.row_position_chirho = 0;
        self.update_hardware_cursor_chirho();
    }

    /// Set the foreground and background color.
    #[allow(dead_code)]
    pub fn set_color_chirho(
        &mut self,
        foreground_chirho: ColorChirho,
        background_chirho: ColorChirho,
    ) {
        self.color_code_chirho =
            ColorCodeChirho::new_chirho(foreground_chirho, background_chirho);
    }

    /// Update the hardware VGA cursor position via CRTC registers.
    ///
    /// On real hardware, this moves the blinking cursor to match the
    /// software cursor position. On emulators this is optional but
    /// provides a more accurate display.
    fn update_hardware_cursor_chirho(&self) {
        let pos_chirho: u16 =
            (self.row_position_chirho * BUFFER_WIDTH_CHIRHO + self.column_position_chirho) as u16;

        unsafe {
            let mut index_port_chirho: Port<u8> = Port::new(CRTC_INDEX_PORT_CHIRHO);
            let mut data_port_chirho: Port<u8> = Port::new(CRTC_DATA_PORT_CHIRHO);

            // Write cursor position high byte.
            index_port_chirho.write(CRTC_CURSOR_HIGH_CHIRHO);
            data_port_chirho.write((pos_chirho >> 8) as u8);

            // Write cursor position low byte.
            index_port_chirho.write(CRTC_CURSOR_LOW_CHIRHO);
            data_port_chirho.write(pos_chirho as u8);
        }
    }

    /// Enable the hardware text-mode cursor with given scanline range.
    ///
    /// Typical values: start=14, end=15 for an underline cursor;
    /// start=0, end=15 for a full-block cursor.
    #[allow(dead_code)]
    pub fn enable_cursor_chirho(&self, start_chirho: u8, end_chirho: u8) {
        unsafe {
            let mut index_port_chirho: Port<u8> = Port::new(CRTC_INDEX_PORT_CHIRHO);
            let mut data_port_chirho: Port<u8> = Port::new(CRTC_DATA_PORT_CHIRHO);

            index_port_chirho.write(CRTC_CURSOR_START_CHIRHO);
            // Bit 5 = cursor disable; clear it to enable.
            data_port_chirho.write(start_chirho & 0x1F);

            index_port_chirho.write(CRTC_CURSOR_END_CHIRHO);
            data_port_chirho.write(end_chirho & 0x1F);
        }
    }

    /// Disable the hardware cursor (hide it).
    #[allow(dead_code)]
    pub fn disable_cursor_chirho(&self) {
        unsafe {
            let mut index_port_chirho: Port<u8> = Port::new(CRTC_INDEX_PORT_CHIRHO);
            let mut data_port_chirho: Port<u8> = Port::new(CRTC_DATA_PORT_CHIRHO);

            index_port_chirho.write(CRTC_CURSOR_START_CHIRHO);
            // Bit 5 set = cursor disabled.
            data_port_chirho.write(0x20);
        }
    }
}

// ---------------------------------------------------------------------------
// core::fmt::Write implementation
// ---------------------------------------------------------------------------

impl fmt::Write for WriterChirho {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string_chirho(s);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Global static WRITER_CHIRHO
// ---------------------------------------------------------------------------

/// Global, mutex-protected VGA writer.
///
/// `spin::Lazy` defers initialisation until first access so that the
/// constructor does not run before the VGA buffer is mapped.
pub static WRITER_CHIRHO: Lazy<Mutex<WriterChirho>> = Lazy::new(|| {
    let mut writer_chirho = WriterChirho {
        column_position_chirho: 0,
        row_position_chirho: 0,
        color_code_chirho: ColorCodeChirho::new_chirho(
            ColorChirho::YellowChirho,
            ColorChirho::BlackChirho,
        ),
        // SAFETY: 0xb8000 is the well-known VGA text buffer address
        // which is identity-mapped (or equivalently mapped) by the
        // bootloader before the kernel runs.
        buffer_chirho: unsafe { &mut *(VGA_BUFFER_ADDRESS_CHIRHO as *mut BufferChirho) },
    };

    // Enable an underline cursor on real hardware.
    writer_chirho.enable_cursor_chirho(14, 15);

    Mutex::new(writer_chirho)
});

// ---------------------------------------------------------------------------
// Public helper: _print_chirho
// ---------------------------------------------------------------------------

/// Internal print function used by the `print_chirho!` and `println_chirho!`
/// macros.  Acquires the global writer lock, formats the arguments, and
/// releases the lock.
#[doc(hidden)]
pub fn _print_chirho(args_chirho: fmt::Arguments) {
    use core::fmt::Write;
    // Disable interrupts while holding the writer lock to prevent deadlocks
    // if an interrupt handler also tries to print.
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER_CHIRHO
            .lock()
            .write_fmt(args_chirho)
            .expect("Writing to VGA buffer failed");
    });
}

// ---------------------------------------------------------------------------
// Public utility functions
// ---------------------------------------------------------------------------

/// Clear the VGA screen from outside the module.
#[allow(dead_code)]
pub fn clear_screen_chirho() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER_CHIRHO.lock().clear_screen_chirho();
    });
}

/// Set the VGA text color from outside the module.
#[allow(dead_code)]
pub fn set_color_chirho(foreground_chirho: ColorChirho, background_chirho: ColorChirho) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER_CHIRHO
            .lock()
            .set_color_chirho(foreground_chirho, background_chirho);
    });
}

// ---------------------------------------------------------------------------
// Macros
// ---------------------------------------------------------------------------

/// Print formatted text to the VGA text buffer (no trailing newline).
///
/// Usage mirrors `print!` from the standard library.
#[macro_export]
macro_rules! print_chirho {
    ($($arg_chirho:tt)*) => {
        $crate::vga_buffer_chirho::_print_chirho(format_args!($($arg_chirho)*))
    };
}

/// Print formatted text to the VGA text buffer **with** a trailing newline.
///
/// Usage mirrors `println!` from the standard library.
#[macro_export]
macro_rules! println_chirho {
    () => {
        $crate::print_chirho!("\n")
    };
    ($($arg_chirho:tt)*) => {
        $crate::print_chirho!("{}\n", format_args!($($arg_chirho)*))
    };
}
