// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM serial console — maps to browser console.log / DOM terminal.

use core::fmt;

extern "C" {
    fn js_console_write_chirho(ptr_chirho: *const u8, len_chirho: u32);
}

/// Write a string to the browser console via JS import.
pub fn write_str_chirho(s_chirho: &str) {
    unsafe {
        js_console_write_chirho(s_chirho.as_ptr(), s_chirho.len() as u32);
    }
}

/// Console writer implementing core::fmt::Write for format macros.
pub struct WasmConsoleChirho;

impl fmt::Write for WasmConsoleChirho {
    fn write_str(&mut self, s_chirho: &str) -> fmt::Result {
        write_str_chirho(s_chirho);
        Ok(())
    }
}
