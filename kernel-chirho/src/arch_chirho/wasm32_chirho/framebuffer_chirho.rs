// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM framebuffer — Canvas 2D is our VGA/GPU.
//!
//! The kernel writes RGBA pixels to WASM linear memory, then the JS
//! runtime copies them to a Canvas element via `putImageData`.
//! This replaces the VGA text buffer, DRM/KMS, and GPU drivers.

extern "C" {
    fn js_framebuffer_init_chirho(width_chirho: u32, height_chirho: u32) -> u32;
    fn js_framebuffer_flush_chirho();
}

/// Framebuffer state.
pub struct WasmFramebufferChirho {
    pub ptr_chirho: *mut u8,
    pub width_chirho: u32,
    pub height_chirho: u32,
    pub stride_chirho: u32, // bytes per row (width * 4 for RGBA)
}

impl WasmFramebufferChirho {
    /// Initialize a framebuffer with the given dimensions.
    pub fn init_chirho(width_chirho: u32, height_chirho: u32) -> Self {
        let ptr_raw_chirho = unsafe {
            js_framebuffer_init_chirho(width_chirho, height_chirho)
        };
        Self {
            ptr_chirho: ptr_raw_chirho as *mut u8,
            width_chirho,
            height_chirho,
            stride_chirho: width_chirho * 4,
        }
    }

    /// Set a pixel at (x, y) to the given RGBA color.
    pub fn set_pixel_chirho(&mut self, x_chirho: u32, y_chirho: u32, r_chirho: u8, g_chirho: u8, b_chirho: u8, a_chirho: u8) {
        if x_chirho >= self.width_chirho || y_chirho >= self.height_chirho {
            return;
        }
        let offset_chirho = ((y_chirho * self.stride_chirho) + (x_chirho * 4)) as isize;
        unsafe {
            *self.ptr_chirho.offset(offset_chirho) = r_chirho;
            *self.ptr_chirho.offset(offset_chirho + 1) = g_chirho;
            *self.ptr_chirho.offset(offset_chirho + 2) = b_chirho;
            *self.ptr_chirho.offset(offset_chirho + 3) = a_chirho;
        }
    }

    /// Flush the framebuffer to the Canvas element.
    pub fn flush_chirho(&self) {
        unsafe { js_framebuffer_flush_chirho(); }
    }

    /// Clear the framebuffer to a solid color.
    pub fn clear_chirho(&mut self, r_chirho: u8, g_chirho: u8, b_chirho: u8) {
        let total_pixels_chirho = self.width_chirho * self.height_chirho;
        for i_chirho in 0..total_pixels_chirho {
            let offset_chirho = (i_chirho * 4) as isize;
            unsafe {
                *self.ptr_chirho.offset(offset_chirho) = r_chirho;
                *self.ptr_chirho.offset(offset_chirho + 1) = g_chirho;
                *self.ptr_chirho.offset(offset_chirho + 2) = b_chirho;
                *self.ptr_chirho.offset(offset_chirho + 3) = 255;
            }
        }
    }
}
