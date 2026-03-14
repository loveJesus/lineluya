// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! WASM block storage — OPFS/IndexedDB as our disk drive.
//!
//! The Origin Private File System (OPFS) provides synchronous file
//! access in Web Workers, achieving 3-4x the performance of IndexedDB.
//! This replaces AHCI, NVMe, and VirtIO-blk drivers.

extern "C" {
    fn js_storage_read_chirho(offset_chirho: u64, buf_ptr_chirho: *mut u8, len_chirho: u32) -> i32;
    fn js_storage_write_chirho(offset_chirho: u64, buf_ptr_chirho: *const u8, len_chirho: u32) -> i32;
}

/// WASM block device backed by browser storage.
pub struct WasmStorageChirho {
    pub total_size_chirho: u64,
    pub block_size_chirho: u32,
}

impl WasmStorageChirho {
    pub const fn new_chirho(total_size_chirho: u64, block_size_chirho: u32) -> Self {
        Self {
            total_size_chirho,
            block_size_chirho,
        }
    }

    /// Read bytes from storage at the given offset.
    pub fn read_chirho(&self, offset_chirho: u64, buf_chirho: &mut [u8]) -> Result<usize, i32> {
        let result_chirho = unsafe {
            js_storage_read_chirho(offset_chirho, buf_chirho.as_mut_ptr(), buf_chirho.len() as u32)
        };
        if result_chirho < 0 {
            Err(result_chirho)
        } else {
            Ok(result_chirho as usize)
        }
    }

    /// Write bytes to storage at the given offset.
    pub fn write_chirho(&self, offset_chirho: u64, buf_chirho: &[u8]) -> Result<usize, i32> {
        let result_chirho = unsafe {
            js_storage_write_chirho(offset_chirho, buf_chirho.as_ptr(), buf_chirho.len() as u32)
        };
        if result_chirho < 0 {
            Err(result_chirho)
        } else {
            Ok(result_chirho as usize)
        }
    }
}
