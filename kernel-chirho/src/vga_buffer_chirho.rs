// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! VGA text-mode buffer driver for the Lineluya bare-metal kernel.
//!
//! Provides a writer that outputs characters to the VGA text buffer at
//! physical address 0xb8000. All writes use `core::ptr::write_volatile`
//! to ensure the compiler never elides stores to video memory.

use core::fmt;
use spin::{Lazy, Mutex};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Base physical address of the VGA text buffer.
const VGA_BUFFER_ADDRESS_CHIRHO: usize = 0xb8000;

/// Number of character columns in standard VGA text mode.
const BUFFER_WIDTH_CHIRHO: usize = 80;

/// Number of character rows in standard VGA text mode.
const BUFFER_HEIGHT_CHIRHO: usize = 25;

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
// BufferChirho — the raw 80×25 VGA framebuffer
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
/// New characters are always written to the **last** row.  When the row is
/// full or a newline is encountered the buffer scrolls up by one line.
pub struct WriterChirho {
    column_position_chirho: usize,
    color_code_chirho: ColorCodeChirho,
    buffer_chirho: &'static mut BufferChirho,
}

impl WriterChirho {
    /// Write a single byte to the VGA buffer.
    ///
    /// Printable ASCII bytes are placed at the current cursor position.
    /// A `\n` (newline) triggers a line advance.  Any other byte is replaced
    /// by `0xFE` (a small filled square on most VGA fonts).
    pub fn write_byte_chirho(&mut self, byte_chirho: u8) {
        match byte_chirho {
            b'\n' => self.new_line_chirho(),
            byte_chirho => {
                if self.column_position_chirho >= BUFFER_WIDTH_CHIRHO {
                    self.new_line_chirho();
                }

                let row_chirho = BUFFER_HEIGHT_CHIRHO - 1;
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
    /// Only bytes in the printable ASCII range (`0x20 ..= 0x7E`) and `\n` are
    /// output directly; everything else is replaced by `0xFE`.
    pub fn write_string_chirho(&mut self, string_chirho: &str) {
        for byte_chirho in string_chirho.bytes() {
            match byte_chirho {
                // Printable ASCII or newline — write as-is.
                0x20..=0x7E | b'\n' => self.write_byte_chirho(byte_chirho),
                // Non-printable / multibyte UTF-8 continuation — substitute.
                _ => self.write_byte_chirho(0xFE),
            }
        }
    }

    /// Scroll the entire buffer up by one row and clear the last row.
    fn new_line_chirho(&mut self) {
        // Move every row up by one.
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
        self.column_position_chirho = 0;
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
    Mutex::new(WriterChirho {
        column_position_chirho: 0,
        color_code_chirho: ColorCodeChirho::new_chirho(
            ColorChirho::YellowChirho,
            ColorChirho::BlackChirho,
        ),
        // SAFETY: 0xb8000 is the well-known VGA text buffer address
        // which is identity-mapped (or equivalently mapped) by the
        // bootloader before the kernel runs.
        buffer_chirho: unsafe { &mut *(VGA_BUFFER_ADDRESS_CHIRHO as *mut BufferChirho) },
    })
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
