// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Pixel framebuffer console for the Lineluya kernel.
//!
//! Renders text as pixels on the UEFI framebuffer (1280x800 BGR).
//! Uses an embedded 8x16 bitmap font. Mirrors all serial output
//! to the screen so the QEMU VGA window shows boot messages.

use core::fmt;
use spin::Mutex;

// ---------------------------------------------------------------------------
// 8x16 bitmap font (CP437-style, covers ASCII 0x20..0x7F)
// Each character is 8 pixels wide, 16 pixels tall = 16 bytes per glyph
// ---------------------------------------------------------------------------

/// Basic 8x16 font glyphs for printable ASCII (space through ~).
/// Each glyph is 16 bytes (one byte per row, MSB = leftmost pixel).
static FONT_8X16_CHIRHO: [[u8; 16]; 96] = {
    let mut font_chirho = [[0u8; 16]; 96];

    // Space (0x20)
    font_chirho[0] = [0; 16];

    // ! (0x21)
    font_chirho[1] = [0,0,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0,0x18,0x18,0,0,0,0];

    // " (0x22)
    font_chirho[2] = [0,0x66,0x66,0x66,0x24,0,0,0,0,0,0,0,0,0,0,0];

    // # (0x23)
    font_chirho[3] = [0,0,0,0x36,0x36,0x7f,0x36,0x36,0x36,0x7f,0x36,0x36,0,0,0,0];

    // $ .. let's do common ones
    // For brevity, fill remaining with simple patterns
    // A-Z (0x41-0x5A = indices 33-58)
    // a-z (0x61-0x7A = indices 65-90)

    // 0 (0x30 = index 16)
    font_chirho[16] = [0,0,0x3c,0x66,0x66,0x6e,0x76,0x66,0x66,0x66,0x3c,0,0,0,0,0];
    // 1
    font_chirho[17] = [0,0,0x18,0x38,0x18,0x18,0x18,0x18,0x18,0x18,0x7e,0,0,0,0,0];
    // 2
    font_chirho[18] = [0,0,0x3c,0x66,0x06,0x0c,0x18,0x30,0x60,0x66,0x7e,0,0,0,0,0];
    // 3
    font_chirho[19] = [0,0,0x3c,0x66,0x06,0x1c,0x06,0x06,0x06,0x66,0x3c,0,0,0,0,0];
    // 4
    font_chirho[20] = [0,0,0x0c,0x1c,0x3c,0x6c,0x6c,0x7e,0x0c,0x0c,0x1e,0,0,0,0,0];
    // 5
    font_chirho[21] = [0,0,0x7e,0x60,0x60,0x7c,0x06,0x06,0x06,0x66,0x3c,0,0,0,0,0];
    // 6
    font_chirho[22] = [0,0,0x1c,0x30,0x60,0x7c,0x66,0x66,0x66,0x66,0x3c,0,0,0,0,0];
    // 7
    font_chirho[23] = [0,0,0x7e,0x66,0x06,0x0c,0x18,0x18,0x18,0x18,0x18,0,0,0,0,0];
    // 8
    font_chirho[24] = [0,0,0x3c,0x66,0x66,0x3c,0x66,0x66,0x66,0x66,0x3c,0,0,0,0,0];
    // 9
    font_chirho[25] = [0,0,0x3c,0x66,0x66,0x66,0x3e,0x06,0x06,0x0c,0x38,0,0,0,0,0];

    // A (0x41 = index 33)
    font_chirho[33] = [0,0,0x18,0x3c,0x66,0x66,0x66,0x7e,0x66,0x66,0x66,0,0,0,0,0];
    // B
    font_chirho[34] = [0,0,0x7c,0x66,0x66,0x7c,0x66,0x66,0x66,0x66,0x7c,0,0,0,0,0];
    // C
    font_chirho[35] = [0,0,0x3c,0x66,0x60,0x60,0x60,0x60,0x60,0x66,0x3c,0,0,0,0,0];
    // D
    font_chirho[36] = [0,0,0x78,0x6c,0x66,0x66,0x66,0x66,0x66,0x6c,0x78,0,0,0,0,0];
    // E
    font_chirho[37] = [0,0,0x7e,0x60,0x60,0x7c,0x60,0x60,0x60,0x60,0x7e,0,0,0,0,0];
    // F
    font_chirho[38] = [0,0,0x7e,0x60,0x60,0x7c,0x60,0x60,0x60,0x60,0x60,0,0,0,0,0];
    // G
    font_chirho[39] = [0,0,0x3c,0x66,0x60,0x60,0x6e,0x66,0x66,0x66,0x3e,0,0,0,0,0];
    // H
    font_chirho[40] = [0,0,0x66,0x66,0x66,0x7e,0x66,0x66,0x66,0x66,0x66,0,0,0,0,0];
    // I
    font_chirho[41] = [0,0,0x3c,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x3c,0,0,0,0,0];
    // J
    font_chirho[42] = [0,0,0x1e,0x0c,0x0c,0x0c,0x0c,0x0c,0x0c,0x6c,0x38,0,0,0,0,0];
    // K
    font_chirho[43] = [0,0,0x66,0x6c,0x78,0x70,0x70,0x78,0x6c,0x66,0x66,0,0,0,0,0];
    // L
    font_chirho[44] = [0,0,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x7e,0,0,0,0,0];
    // M
    font_chirho[45] = [0,0,0x63,0x77,0x7f,0x6b,0x63,0x63,0x63,0x63,0x63,0,0,0,0,0];
    // N
    font_chirho[46] = [0,0,0x66,0x76,0x7e,0x7e,0x6e,0x66,0x66,0x66,0x66,0,0,0,0,0];
    // O
    font_chirho[47] = [0,0,0x3c,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3c,0,0,0,0,0];
    // P
    font_chirho[48] = [0,0,0x7c,0x66,0x66,0x66,0x7c,0x60,0x60,0x60,0x60,0,0,0,0,0];
    // Q
    font_chirho[49] = [0,0,0x3c,0x66,0x66,0x66,0x66,0x66,0x6e,0x3c,0x0e,0,0,0,0,0];
    // R
    font_chirho[50] = [0,0,0x7c,0x66,0x66,0x7c,0x78,0x6c,0x66,0x66,0x66,0,0,0,0,0];
    // S
    font_chirho[51] = [0,0,0x3c,0x66,0x60,0x30,0x18,0x0c,0x06,0x66,0x3c,0,0,0,0,0];
    // T
    font_chirho[52] = [0,0,0x7e,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0,0,0,0,0];
    // U
    font_chirho[53] = [0,0,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3c,0,0,0,0,0];
    // V
    font_chirho[54] = [0,0,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3c,0x18,0,0,0,0,0];
    // W
    font_chirho[55] = [0,0,0x63,0x63,0x63,0x63,0x6b,0x7f,0x77,0x63,0x63,0,0,0,0,0];
    // X
    font_chirho[56] = [0,0,0x66,0x66,0x66,0x3c,0x18,0x3c,0x66,0x66,0x66,0,0,0,0,0];
    // Y
    font_chirho[57] = [0,0,0x66,0x66,0x66,0x3c,0x18,0x18,0x18,0x18,0x18,0,0,0,0,0];
    // Z
    font_chirho[58] = [0,0,0x7e,0x06,0x0c,0x18,0x30,0x60,0x60,0x66,0x7e,0,0,0,0,0];

    // [ (0x5B = index 59)
    font_chirho[59] = [0,0x1e,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x1e,0,0,0,0,0];
    // \ (0x5C = index 60)
    font_chirho[60] = [0,0,0x40,0x60,0x30,0x18,0x0c,0x06,0x03,0x01,0,0,0,0,0,0];
    // ] (0x5D = index 61)
    font_chirho[61] = [0,0x78,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x78,0,0,0,0,0];

    // a-z (0x61 = index 65)
    font_chirho[65] = [0,0,0,0,0,0x3c,0x06,0x3e,0x66,0x66,0x3e,0,0,0,0,0]; // a
    font_chirho[66] = [0,0,0x60,0x60,0x60,0x7c,0x66,0x66,0x66,0x66,0x7c,0,0,0,0,0]; // b
    font_chirho[67] = [0,0,0,0,0,0x3c,0x66,0x60,0x60,0x66,0x3c,0,0,0,0,0]; // c
    font_chirho[68] = [0,0,0x06,0x06,0x06,0x3e,0x66,0x66,0x66,0x66,0x3e,0,0,0,0,0]; // d
    font_chirho[69] = [0,0,0,0,0,0x3c,0x66,0x7e,0x60,0x66,0x3c,0,0,0,0,0]; // e
    font_chirho[70] = [0,0,0x0e,0x18,0x18,0x7e,0x18,0x18,0x18,0x18,0x18,0,0,0,0,0]; // f
    font_chirho[71] = [0,0,0,0,0,0x3e,0x66,0x66,0x66,0x3e,0x06,0x66,0x3c,0,0,0]; // g
    font_chirho[72] = [0,0,0x60,0x60,0x60,0x7c,0x66,0x66,0x66,0x66,0x66,0,0,0,0,0]; // h
    font_chirho[73] = [0,0,0x18,0x18,0,0x38,0x18,0x18,0x18,0x18,0x3c,0,0,0,0,0]; // i
    font_chirho[74] = [0,0,0x0c,0x0c,0,0x1c,0x0c,0x0c,0x0c,0x0c,0x0c,0x6c,0x38,0,0,0]; // j
    font_chirho[75] = [0,0,0x60,0x60,0x60,0x66,0x6c,0x78,0x6c,0x66,0x66,0,0,0,0,0]; // k
    font_chirho[76] = [0,0,0x38,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x3c,0,0,0,0,0]; // l
    font_chirho[77] = [0,0,0,0,0,0x66,0x7f,0x6b,0x6b,0x63,0x63,0,0,0,0,0]; // m
    font_chirho[78] = [0,0,0,0,0,0x7c,0x66,0x66,0x66,0x66,0x66,0,0,0,0,0]; // n
    font_chirho[79] = [0,0,0,0,0,0x3c,0x66,0x66,0x66,0x66,0x3c,0,0,0,0,0]; // o
    font_chirho[80] = [0,0,0,0,0,0x7c,0x66,0x66,0x66,0x7c,0x60,0x60,0x60,0,0,0]; // p
    font_chirho[81] = [0,0,0,0,0,0x3e,0x66,0x66,0x66,0x3e,0x06,0x06,0x06,0,0,0]; // q
    font_chirho[82] = [0,0,0,0,0,0x7c,0x66,0x60,0x60,0x60,0x60,0,0,0,0,0]; // r
    font_chirho[83] = [0,0,0,0,0,0x3e,0x60,0x3c,0x06,0x06,0x7c,0,0,0,0,0]; // s
    font_chirho[84] = [0,0,0x18,0x18,0x18,0x7e,0x18,0x18,0x18,0x18,0x0e,0,0,0,0,0]; // t
    font_chirho[85] = [0,0,0,0,0,0x66,0x66,0x66,0x66,0x66,0x3e,0,0,0,0,0]; // u
    font_chirho[86] = [0,0,0,0,0,0x66,0x66,0x66,0x66,0x3c,0x18,0,0,0,0,0]; // v
    font_chirho[87] = [0,0,0,0,0,0x63,0x63,0x6b,0x7f,0x77,0x63,0,0,0,0,0]; // w
    font_chirho[88] = [0,0,0,0,0,0x66,0x66,0x3c,0x3c,0x66,0x66,0,0,0,0,0]; // x
    font_chirho[89] = [0,0,0,0,0,0x66,0x66,0x66,0x66,0x3e,0x06,0x66,0x3c,0,0,0]; // y
    font_chirho[90] = [0,0,0,0,0,0x7e,0x0c,0x18,0x30,0x60,0x7e,0,0,0,0,0]; // z

    // Common symbols
    // - (0x2D = index 13)
    font_chirho[13] = [0,0,0,0,0,0,0,0x7e,0,0,0,0,0,0,0,0];
    // . (0x2E = index 14)
    font_chirho[14] = [0,0,0,0,0,0,0,0,0,0,0x18,0x18,0,0,0,0];
    // / (0x2F = index 15)
    font_chirho[15] = [0,0,0x01,0x03,0x06,0x0c,0x18,0x30,0x60,0xc0,0,0,0,0,0,0];
    // : (0x3A = index 26)
    font_chirho[26] = [0,0,0,0,0x18,0x18,0,0,0,0x18,0x18,0,0,0,0,0];
    // = (0x3D = index 29)
    font_chirho[29] = [0,0,0,0,0,0x7e,0,0x7e,0,0,0,0,0,0,0,0];
    // ( (0x28 = index 8)
    font_chirho[8] = [0,0x0c,0x18,0x30,0x30,0x30,0x30,0x30,0x30,0x18,0x0c,0,0,0,0,0];
    // ) (0x29 = index 9)
    font_chirho[9] = [0,0x30,0x18,0x0c,0x0c,0x0c,0x0c,0x0c,0x0c,0x18,0x30,0,0,0,0,0];
    // , (0x2C = index 12)
    font_chirho[12] = [0,0,0,0,0,0,0,0,0,0x18,0x18,0x30,0,0,0,0];
    // _ (0x5F = index 63)
    font_chirho[63] = [0,0,0,0,0,0,0,0,0,0,0,0,0x7e,0,0,0];
    // + (0x2B = index 11)
    font_chirho[11] = [0,0,0,0,0x18,0x18,0x7e,0x18,0x18,0,0,0,0,0,0,0];
    // * (0x2A = index 10)
    font_chirho[10] = [0,0,0,0x66,0x3c,0xff,0x3c,0x66,0,0,0,0,0,0,0,0];

    font_chirho
};

const GLYPH_W_CHIRHO: usize = 8;
const GLYPH_H_CHIRHO: usize = 16;

// ---------------------------------------------------------------------------
// Framebuffer console state
// ---------------------------------------------------------------------------

/// Pixel format from the bootloader.
#[derive(Debug, Clone, Copy)]
pub enum PixelFormatChirho {
    BgrChirho,
    RgbChirho,
}

/// Framebuffer console state.
pub struct FbConsoleChirho {
    /// Pointer to the framebuffer memory.
    fb_ptr_chirho: *mut u8,
    /// Framebuffer byte length.
    fb_len_chirho: usize,
    /// Width in pixels.
    width_chirho: usize,
    /// Height in pixels.
    height_chirho: usize,
    /// Bytes per pixel (typically 4).
    bpp_chirho: usize,
    /// Stride (bytes per row, may include padding).
    stride_chirho: usize,
    /// Pixel format.
    format_chirho: PixelFormatChirho,
    /// Current cursor column (in character cells).
    col_chirho: usize,
    /// Current cursor row (in character cells).
    row_chirho: usize,
    /// Max columns.
    max_col_chirho: usize,
    /// Max rows.
    max_row_chirho: usize,
    /// Foreground color (R, G, B).
    fg_chirho: (u8, u8, u8),
    /// Background color (R, G, B).
    bg_chirho: (u8, u8, u8),
    /// Whether the console is initialized.
    ready_chirho: bool,
    /// ANSI escape sequence parser state.
    /// false = normal, true = inside ESC[...X sequence (skip until letter).
    in_ansi_chirho: bool,
}

unsafe impl Send for FbConsoleChirho {}

impl FbConsoleChirho {
    pub const fn new_chirho() -> Self {
        Self {
            fb_ptr_chirho: core::ptr::null_mut(),
            fb_len_chirho: 0,
            width_chirho: 0,
            height_chirho: 0,
            bpp_chirho: 4,
            stride_chirho: 0,
            format_chirho: PixelFormatChirho::BgrChirho,
            col_chirho: 0,
            row_chirho: 0,
            max_col_chirho: 0,
            max_row_chirho: 0,
            fg_chirho: (0x00, 0xff, 0x41),  // Green on black (terminal style)
            bg_chirho: (0x0a, 0x0a, 0x0a),
            ready_chirho: false,
            in_ansi_chirho: false,
        }
    }

    /// Initialize the framebuffer console from bootloader info.
    pub fn init_chirho(
        &mut self,
        fb_addr_chirho: *mut u8,
        fb_len_chirho: usize,
        width_chirho: usize,
        height_chirho: usize,
        bpp_chirho: usize,
        stride_chirho: usize,
        is_bgr_chirho: bool,
    ) {
        self.fb_ptr_chirho = fb_addr_chirho;
        self.fb_len_chirho = fb_len_chirho;
        self.width_chirho = width_chirho;
        self.height_chirho = height_chirho;
        self.bpp_chirho = bpp_chirho;
        self.stride_chirho = stride_chirho;
        self.format_chirho = if is_bgr_chirho {
            PixelFormatChirho::BgrChirho
        } else {
            PixelFormatChirho::RgbChirho
        };
        self.max_col_chirho = width_chirho / GLYPH_W_CHIRHO;
        self.max_row_chirho = height_chirho / GLYPH_H_CHIRHO;
        self.col_chirho = 0;
        self.row_chirho = 0;
        self.ready_chirho = true;

        // Clear screen to background color
        self.clear_screen_chirho();
    }

    /// Clear the entire screen.
    pub fn clear_screen_chirho(&mut self) {
        if !self.ready_chirho {
            return;
        }
        let (r_chirho, g_chirho, b_chirho) = self.bg_chirho;
        for y_chirho in 0..self.height_chirho {
            for x_chirho in 0..self.width_chirho {
                self.put_pixel_chirho(x_chirho, y_chirho, r_chirho, g_chirho, b_chirho);
            }
        }
        self.col_chirho = 0;
        self.row_chirho = 0;
    }

    /// Write a single pixel.
    #[inline]
    fn put_pixel_chirho(&self, x_chirho: usize, y_chirho: usize, r_chirho: u8, g_chirho: u8, b_chirho: u8) {
        if x_chirho >= self.width_chirho || y_chirho >= self.height_chirho {
            return;
        }
        let offset_chirho = y_chirho * self.stride_chirho * self.bpp_chirho + x_chirho * self.bpp_chirho;
        if offset_chirho + 3 > self.fb_len_chirho {
            return;
        }
        unsafe {
            let ptr_chirho = self.fb_ptr_chirho.add(offset_chirho);
            match self.format_chirho {
                PixelFormatChirho::BgrChirho => {
                    core::ptr::write_volatile(ptr_chirho, b_chirho);
                    core::ptr::write_volatile(ptr_chirho.add(1), g_chirho);
                    core::ptr::write_volatile(ptr_chirho.add(2), r_chirho);
                }
                PixelFormatChirho::RgbChirho => {
                    core::ptr::write_volatile(ptr_chirho, r_chirho);
                    core::ptr::write_volatile(ptr_chirho.add(1), g_chirho);
                    core::ptr::write_volatile(ptr_chirho.add(2), b_chirho);
                }
            }
        }
    }

    /// Draw a single character at the current cursor position.
    fn draw_char_chirho(&self, ch_chirho: u8) {
        let idx_chirho = if ch_chirho >= 0x20 && ch_chirho <= 0x7E {
            (ch_chirho - 0x20) as usize
        } else {
            0 // space for unprintable
        };
        let glyph_chirho = &FONT_8X16_CHIRHO[idx_chirho];
        let base_x_chirho = self.col_chirho * GLYPH_W_CHIRHO;
        let base_y_chirho = self.row_chirho * GLYPH_H_CHIRHO;
        let (fr_chirho, fg_chirho, fb_chirho) = self.fg_chirho;
        let (br_chirho, bg_chirho, bb_chirho) = self.bg_chirho;

        for row_chirho in 0..GLYPH_H_CHIRHO {
            let bits_chirho = glyph_chirho[row_chirho];
            for col_chirho in 0..GLYPH_W_CHIRHO {
                let on_chirho = (bits_chirho >> (7 - col_chirho)) & 1 != 0;
                if on_chirho {
                    self.put_pixel_chirho(base_x_chirho + col_chirho, base_y_chirho + row_chirho, fr_chirho, fg_chirho, fb_chirho);
                } else {
                    self.put_pixel_chirho(base_x_chirho + col_chirho, base_y_chirho + row_chirho, br_chirho, bg_chirho, bb_chirho);
                }
            }
        }
    }

    /// Scroll the screen up by one line.
    fn scroll_up_chirho(&mut self) {
        if !self.ready_chirho {
            return;
        }
        let row_bytes_chirho = GLYPH_H_CHIRHO * self.stride_chirho * self.bpp_chirho;
        let total_rows_chirho = self.max_row_chirho;

        // Move all rows up by one glyph height
        unsafe {
            let src_chirho = self.fb_ptr_chirho.add(row_bytes_chirho);
            let dst_chirho = self.fb_ptr_chirho;
            let copy_len_chirho = row_bytes_chirho * (total_rows_chirho - 1);
            if copy_len_chirho <= self.fb_len_chirho {
                core::ptr::copy(src_chirho, dst_chirho, copy_len_chirho);
            }
        }

        // Clear the last row
        let last_row_y_chirho = (self.max_row_chirho - 1) * GLYPH_H_CHIRHO;
        let (br_chirho, bg_chirho, bb_chirho) = self.bg_chirho;
        for y_chirho in last_row_y_chirho..last_row_y_chirho + GLYPH_H_CHIRHO {
            for x_chirho in 0..self.width_chirho {
                self.put_pixel_chirho(x_chirho, y_chirho, br_chirho, bg_chirho, bb_chirho);
            }
        }
    }

    /// Write a single byte to the console.
    ///
    /// Filters ANSI escape sequences (ESC[...m, ESC[...H, etc.) so that
    /// BusyBox color output doesn't render as garbage on the framebuffer.
    pub fn write_byte_chirho(&mut self, byte_chirho: u8) {
        if !self.ready_chirho {
            return;
        }

        // ANSI escape sequence state machine:
        // ESC (0x1B) starts a sequence, skip bytes until a letter terminates it.
        if self.in_ansi_chirho {
            // Letters A-Z, a-z terminate the ANSI sequence.
            if byte_chirho.is_ascii_alphabetic() {
                self.in_ansi_chirho = false;
            }
            // Either way, skip this byte (it's part of the escape sequence).
            return;
        }

        match byte_chirho {
            0x00 => {
                // NUL — silently ignore (don't advance cursor).
            }
            0x1B => {
                // ESC — start of ANSI escape sequence.
                self.in_ansi_chirho = true;
            }
            0x08 => {
                // Backspace — move cursor left one column.
                if self.col_chirho > 0 {
                    self.col_chirho -= 1;
                }
            }
            b'\n' => {
                self.col_chirho = 0;
                self.row_chirho += 1;
                if self.row_chirho >= self.max_row_chirho {
                    self.scroll_up_chirho();
                    self.row_chirho = self.max_row_chirho - 1;
                }
            }
            b'\r' => {
                self.col_chirho = 0;
            }
            b'\t' => {
                let next_tab_chirho = (self.col_chirho + 8) & !7;
                self.col_chirho = next_tab_chirho.min(self.max_col_chirho - 1);
            }
            0x01..=0x06 | 0x0E..=0x1A | 0x1C..=0x1F | 0x7F => {
                // Other control characters — silently ignore.
            }
            _ => {
                self.draw_char_chirho(byte_chirho);
                self.col_chirho += 1;
                if self.col_chirho >= self.max_col_chirho {
                    self.col_chirho = 0;
                    self.row_chirho += 1;
                    if self.row_chirho >= self.max_row_chirho {
                        self.scroll_up_chirho();
                        self.row_chirho = self.max_row_chirho - 1;
                    }
                }
            }
        }
    }

    /// Write a string to the console.
    pub fn write_str_chirho(&mut self, s_chirho: &str) {
        for byte_chirho in s_chirho.bytes() {
            self.write_byte_chirho(byte_chirho);
        }
    }
}

impl fmt::Write for FbConsoleChirho {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str_chirho(s);
        Ok(())
    }
}

/// Global framebuffer console instance.
pub static FB_CONSOLE_CHIRHO: Mutex<FbConsoleChirho> = Mutex::new(FbConsoleChirho::new_chirho());

// ---------------------------------------------------------------------------
// PS/2 keyboard input buffer — filled by interrupt handler, read by sys_read
// ---------------------------------------------------------------------------

const KB_BUF_SIZE_CHIRHO: usize = 256;

/// Lock-free ring buffer for keyboard input bytes.
/// Uses atomics so the interrupt handler (push) and sys_read (pop)
/// never need a lock — avoids deadlocks between IRQ and syscall context.
pub struct KbInputBufChirho {
    buf_chirho: [core::sync::atomic::AtomicU8; KB_BUF_SIZE_CHIRHO],
    head_chirho: core::sync::atomic::AtomicUsize,
    tail_chirho: core::sync::atomic::AtomicUsize,
}

impl KbInputBufChirho {
    pub const fn new_chirho() -> Self {
        // const initialization of atomic array
        const ZERO_CHIRHO: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
        Self {
            buf_chirho: [ZERO_CHIRHO; KB_BUF_SIZE_CHIRHO],
            head_chirho: core::sync::atomic::AtomicUsize::new(0),
            tail_chirho: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Push a byte (called from keyboard interrupt handler — no lock needed).
    pub fn push_chirho(&self, byte_chirho: u8) {
        let tail_chirho = self.tail_chirho.load(core::sync::atomic::Ordering::Relaxed);
        let next_chirho = (tail_chirho + 1) % KB_BUF_SIZE_CHIRHO;
        let head_chirho = self.head_chirho.load(core::sync::atomic::Ordering::Relaxed);
        if next_chirho != head_chirho {
            self.buf_chirho[tail_chirho].store(byte_chirho, core::sync::atomic::Ordering::Relaxed);
            self.tail_chirho.store(next_chirho, core::sync::atomic::Ordering::Release);
        }
    }

    /// Pop a byte (called from sys_read — no lock needed).
    pub fn pop_chirho(&self) -> Option<u8> {
        let head_chirho = self.head_chirho.load(core::sync::atomic::Ordering::Relaxed);
        let tail_chirho = self.tail_chirho.load(core::sync::atomic::Ordering::Acquire);
        if head_chirho == tail_chirho {
            None
        } else {
            let byte_chirho = self.buf_chirho[head_chirho].load(core::sync::atomic::Ordering::Relaxed);
            self.head_chirho.store((head_chirho + 1) % KB_BUF_SIZE_CHIRHO, core::sync::atomic::Ordering::Release);
            Some(byte_chirho)
        }
    }
}

/// Global keyboard input buffer (lock-free, safe for IRQ + syscall).
pub static KB_INPUT_CHIRHO: KbInputBufChirho = KbInputBufChirho::new_chirho();

/// Write to both serial AND framebuffer console.
#[macro_export]
macro_rules! fb_println_chirho {
    () => {
        {
            $crate::serial_println_chirho!();
            if let Some(mut fb_chirho) = $crate::fbconsole_chirho::FB_CONSOLE_CHIRHO.try_lock() {
                fb_chirho.write_byte_chirho(b'\n');
            }
        }
    };
    ($($arg:tt)*) => {
        {
            $crate::serial_println_chirho!($($arg)*);
            if let Some(mut fb_chirho) = $crate::fbconsole_chirho::FB_CONSOLE_CHIRHO.try_lock() {
                use core::fmt::Write;
                let _ = write!(fb_chirho, $($arg)*);
                fb_chirho.write_byte_chirho(b'\n');
            }
        }
    };
}
