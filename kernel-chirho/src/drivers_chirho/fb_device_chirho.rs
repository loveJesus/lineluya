// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Framebuffer device (`/dev/fb0`) for the Lineluya kernel.
//!
//! Exposes the UEFI framebuffer (physical address 0x80000000, 1280x800, 32bpp
//! BGR) as a Linux-compatible framebuffer device.  Supports:
//!
//! - `FBIOGET_VSCREENINFO` -- variable screen info (resolution, bpp, offsets)
//! - `FBIOGET_FSCREENINFO` -- fixed screen info (smem_start, line_length, type)
//! - `mmap` -- userspace direct framebuffer access (via the mm subsystem)
//! - `read`/`write` -- byte-level framebuffer access
//!
//! Needed by X11 (fbdev driver), DirectFB, and framebuffer console programs.

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::syscall_chirho::{
    EFAULT_CHIRHO, EINVAL_CHIRHO, ENOSYS_CHIRHO,
};

// ============================================================================
// Framebuffer constants
// ============================================================================

/// Default framebuffer physical address (UEFI GOP/QEMU VBE).
const FB_PHYS_ADDR_CHIRHO: u64 = 0x8000_0000;

/// Default width in pixels.
const FB_WIDTH_CHIRHO: u32 = 1280;

/// Default height in pixels.
const FB_HEIGHT_CHIRHO: u32 = 800;

/// Bits per pixel.
const FB_BPP_CHIRHO: u32 = 32;

/// Bytes per pixel.
const FB_BYTES_PER_PIXEL_CHIRHO: u32 = FB_BPP_CHIRHO / 8;

/// Stride (bytes per scan line). Typically width * bytes_per_pixel, but
/// can be larger if the GPU adds padding.
const FB_LINE_LENGTH_CHIRHO: u32 = FB_WIDTH_CHIRHO * FB_BYTES_PER_PIXEL_CHIRHO;

/// Total framebuffer size in bytes.
const FB_SIZE_CHIRHO: u64 = FB_LINE_LENGTH_CHIRHO as u64 * FB_HEIGHT_CHIRHO as u64;

/// FBIOGET_VSCREENINFO ioctl number.
const FBIOGET_VSCREENINFO_CHIRHO: u64 = 0x4600;

/// FBIOPUT_VSCREENINFO ioctl number.
const FBIOPUT_VSCREENINFO_CHIRHO: u64 = 0x4601;

/// FBIOGET_FSCREENINFO ioctl number.
const FBIOGET_FSCREENINFO_CHIRHO: u64 = 0x4602;

/// FBIOGETCMAP ioctl number.
const FBIOGETCMAP_CHIRHO: u64 = 0x4604;

/// FBIOPUTCMAP ioctl number.
const FBIOPUTCMAP_CHIRHO: u64 = 0x4605;

/// FBIOPAN_DISPLAY ioctl number — screen panning / double-buffering.
const FBIOPAN_DISPLAY_CHIRHO: u64 = 0x4606;

/// FBIOBLANK ioctl number.
const FBIOBLANK_CHIRHO: u64 = 0x4611;

// ============================================================================
// Global framebuffer state
// ============================================================================

/// Actual framebuffer physical address (updated from boot info if available).
static FB_ACTUAL_PHYS_CHIRHO: AtomicU64 = AtomicU64::new(FB_PHYS_ADDR_CHIRHO);

/// Actual width.
static FB_ACTUAL_WIDTH_CHIRHO: AtomicU32 = AtomicU32::new(FB_WIDTH_CHIRHO);

/// Actual height.
static FB_ACTUAL_HEIGHT_CHIRHO: AtomicU32 = AtomicU32::new(FB_HEIGHT_CHIRHO);

/// Actual stride (bytes per line).
static FB_ACTUAL_STRIDE_CHIRHO: AtomicU32 = AtomicU32::new(FB_LINE_LENGTH_CHIRHO);

/// Actual bytes per pixel.
static FB_ACTUAL_BPP_CHIRHO: AtomicU32 = AtomicU32::new(FB_BPP_CHIRHO);

/// Whether the format is BGR (true) or RGB (false).
static FB_IS_BGR_CHIRHO: AtomicBool = AtomicBool::new(true);

/// One-shot guard for the framebuffer serial screenshot dump.
static FB_DUMP_DONE_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Dump trigger at 60 seconds on the 1 kHz timer.
pub const FB_DUMP_TRIGGER_TICKS_CHIRHO: u64 = 1_500;

/// Set the actual framebuffer parameters (called from boot init when
/// the framebuffer info is available from the bootloader).
pub fn set_fb_params_chirho(
    phys_addr_chirho: u64,
    width_chirho: u32,
    height_chirho: u32,
    stride_bytes_chirho: u32,
    bpp_chirho: u32,
    is_bgr_chirho: bool,
) {
    FB_ACTUAL_PHYS_CHIRHO.store(phys_addr_chirho, Ordering::SeqCst);
    FB_ACTUAL_WIDTH_CHIRHO.store(width_chirho, Ordering::SeqCst);
    FB_ACTUAL_HEIGHT_CHIRHO.store(height_chirho, Ordering::SeqCst);
    FB_ACTUAL_STRIDE_CHIRHO.store(stride_bytes_chirho, Ordering::SeqCst);
    FB_ACTUAL_BPP_CHIRHO.store(bpp_chirho, Ordering::SeqCst);
    FB_IS_BGR_CHIRHO.store(is_bgr_chirho, Ordering::SeqCst);
    crate::serial_println_chirho!(
        "[OK] /dev/fb0 configured: {}x{} {}bpp @ phys {:#x}",
        width_chirho, height_chirho, bpp_chirho, phys_addr_chirho,
    );
}

// ============================================================================
// FbVarScreenInfoChirho -- struct fb_var_screeninfo
// ============================================================================

/// Linux `struct fb_var_screeninfo` -- variable screen information.
///
/// This is a simplified version covering the fields that X11 fbdev
/// and DirectFB actually read.  Total size = 160 bytes on Linux.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FbVarScreenInfoChirho {
    pub xres_chirho: u32,
    pub yres_chirho: u32,
    pub xres_virtual_chirho: u32,
    pub yres_virtual_chirho: u32,
    pub xoffset_chirho: u32,
    pub yoffset_chirho: u32,
    pub bits_per_pixel_chirho: u32,
    pub grayscale_chirho: u32,
    // Bitfield offsets (RGB/BGR)
    pub red_offset_chirho: u32,
    pub red_length_chirho: u32,
    pub red_msb_right_chirho: u32,
    pub green_offset_chirho: u32,
    pub green_length_chirho: u32,
    pub green_msb_right_chirho: u32,
    pub blue_offset_chirho: u32,
    pub blue_length_chirho: u32,
    pub blue_msb_right_chirho: u32,
    pub transp_offset_chirho: u32,
    pub transp_length_chirho: u32,
    pub transp_msb_right_chirho: u32,
    pub nonstd_chirho: u32,
    pub activate_chirho: u32,
    pub height_chirho: u32,   // height in mm (0xFFFFFFFF = unknown)
    pub width_mm_chirho: u32, // width in mm
    pub accel_flags_chirho: u32,
    // Timing (not relevant for VESA/UEFI, zero-fill)
    pub pixclock_chirho: u32,
    pub left_margin_chirho: u32,
    pub right_margin_chirho: u32,
    pub upper_margin_chirho: u32,
    pub lower_margin_chirho: u32,
    pub hsync_len_chirho: u32,
    pub vsync_len_chirho: u32,
    pub sync_chirho: u32,
    pub vmode_chirho: u32,
    pub rotate_chirho: u32,
    pub colorspace_chirho: u32,
    pub reserved_chirho: [u32; 4],
}

impl FbVarScreenInfoChirho {
    fn new_chirho() -> Self {
        let width_chirho = FB_ACTUAL_WIDTH_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let bpp_chirho = FB_ACTUAL_BPP_CHIRHO.load(Ordering::Relaxed);
        let is_bgr_chirho = FB_IS_BGR_CHIRHO.load(Ordering::Relaxed);

        // BGR: blue=0, green=8, red=16
        // RGB: red=0, green=8, blue=16
        let (red_off_chirho, blue_off_chirho) = if is_bgr_chirho {
            (16u32, 0u32)
        } else {
            (0u32, 16u32)
        };

        Self {
            xres_chirho: width_chirho,
            yres_chirho: height_chirho,
            xres_virtual_chirho: width_chirho,
            yres_virtual_chirho: height_chirho,
            xoffset_chirho: 0,
            yoffset_chirho: 0,
            bits_per_pixel_chirho: bpp_chirho,
            grayscale_chirho: 0,
            red_offset_chirho: red_off_chirho,
            red_length_chirho: 8,
            red_msb_right_chirho: 0,
            green_offset_chirho: 8,
            green_length_chirho: 8,
            green_msb_right_chirho: 0,
            blue_offset_chirho: blue_off_chirho,
            blue_length_chirho: 8,
            blue_msb_right_chirho: 0,
            transp_offset_chirho: 24,
            transp_length_chirho: 8,
            transp_msb_right_chirho: 0,
            nonstd_chirho: 0,
            activate_chirho: 0,
            height_chirho: 0xFFFFFFFF,  // unknown
            width_mm_chirho: 0xFFFFFFFF,
            accel_flags_chirho: 0,
            pixclock_chirho: 0,
            left_margin_chirho: 0,
            right_margin_chirho: 0,
            upper_margin_chirho: 0,
            lower_margin_chirho: 0,
            hsync_len_chirho: 0,
            vsync_len_chirho: 0,
            sync_chirho: 0,
            vmode_chirho: 0,
            rotate_chirho: 0,
            colorspace_chirho: 0,
            reserved_chirho: [0; 4],
        }
    }
}

// ============================================================================
// FbFixScreenInfoChirho -- struct fb_fix_screeninfo
// ============================================================================

/// Linux `struct fb_fix_screeninfo` -- fixed screen information.
///
/// Layout matches the Linux x86_64 ABI exactly (80 bytes with alignment):
///
/// ```text
///  offset  field           size
///  ------  -----           ----
///   0      id[16]          16     identification string (null-terminated)
///  16      smem_start      8      unsigned long — physical framebuffer addr
///  24      smem_len        4      __u32
///  28      type            4      __u32  (FB_TYPE_PACKED_PIXELS = 0)
///  32      type_aux        4      __u32
///  36      visual          4      __u32  (FB_VISUAL_TRUECOLOR = 2)
///  40      xpanstep        2      __u16
///  42      ypanstep        2      __u16
///  44      ywrapstep       2      __u16
///  46      [pad]           2      alignment padding
///  48      line_length     4      __u32  (bytes per scan line)
///  52      [pad]           4      alignment padding for mmio_start (u64)
///  56      mmio_start      8      unsigned long
///  64      mmio_len        4      __u32
///  68      accel           4      __u32  (FB_ACCEL_NONE = 0)
///  72      capabilities    2      __u16
///  74      reserved[2]     4      __u16 x 2
///  78      [pad]           2      struct tail alignment to 8 bytes
///  ------
///  total:  80 bytes
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FbFixScreenInfoChirho {
    pub id_chirho: [u8; 16],        // identification string
    pub smem_start_chirho: u64,     // start of frame buffer memory (phys)
    pub smem_len_chirho: u32,       // length of frame buffer memory
    pub fb_type_chirho: u32,        // FB_TYPE_PACKED_PIXELS = 0
    pub type_aux_chirho: u32,       // 0
    pub visual_chirho: u32,         // FB_VISUAL_TRUECOLOR = 2
    pub xpanstep_chirho: u16,
    pub ypanstep_chirho: u16,
    pub ywrapstep_chirho: u16,
    pub _pad0_chirho: u16,
    pub line_length_chirho: u32,    // bytes per scan line
    pub _pad1_chirho: u32,          // explicit alignment pad for mmio_start
    pub mmio_start_chirho: u64,     // start of MMIO (0 for us)
    pub mmio_len_chirho: u32,       // length of MMIO (0)
    pub accel_chirho: u32,          // FB_ACCEL_NONE = 0
    pub capabilities_chirho: u16,
    pub reserved_chirho: [u16; 2],
    pub _pad2_chirho: u16,          // struct tail alignment to 8 bytes
}

impl FbFixScreenInfoChirho {
    fn new_chirho() -> Self {
        let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
        let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let size_chirho = stride_chirho as u32 * height_chirho;

        // ID = "lineluya", null-terminated, zero-padded to 16 bytes.
        let mut id_chirho = [0u8; 16];
        let id_str_chirho = b"lineluya";
        let copy_len_chirho = id_str_chirho.len().min(15);
        id_chirho[..copy_len_chirho].copy_from_slice(&id_str_chirho[..copy_len_chirho]);

        Self {
            id_chirho,
            smem_start_chirho: phys_chirho,
            smem_len_chirho: size_chirho,
            fb_type_chirho: 0,   // FB_TYPE_PACKED_PIXELS
            type_aux_chirho: 0,
            visual_chirho: 2,    // FB_VISUAL_TRUECOLOR
            xpanstep_chirho: 0,
            ypanstep_chirho: 0,
            ywrapstep_chirho: 0,
            _pad0_chirho: 0,
            line_length_chirho: stride_chirho,
            _pad1_chirho: 0,
            mmio_start_chirho: 0,
            mmio_len_chirho: 0,
            accel_chirho: 0,     // FB_ACCEL_NONE
            capabilities_chirho: 0,
            reserved_chirho: [0; 2],
            _pad2_chirho: 0,
        }
    }
}

// ============================================================================
// FbDeviceOpsChirho -- /dev/fb0 file operations
// ============================================================================

/// File operations for `/dev/fb0`.
///
/// Supports read, write (byte-level framebuffer access), and ioctls
/// for screen info queries.  Mmap is handled at the syscall layer by
/// detecting the fb0 fd and mapping the physical framebuffer pages.
pub struct FbDeviceOpsChirho;

impl FileOpsChirho for FbDeviceOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // Read from the linear framebuffer at the current file position.
        let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
        let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let total_size_chirho = (stride_chirho as u64) * (height_chirho as u64);

        let pos_chirho = file_chirho.pos_chirho;
        if pos_chirho >= total_size_chirho {
            return Ok(0); // EOF
        }

        let remaining_chirho = (total_size_chirho - pos_chirho) as usize;
        let to_read_chirho = buf_chirho.len().min(remaining_chirho);

        // Map through physical memory offset (kernel has all phys mapped)
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let virt_addr_chirho = phys_chirho + phys_offset_chirho + pos_chirho;

        // SAFETY: The framebuffer physical memory is mapped into the kernel's
        // address space via the bootloader's physical memory mapping.
        unsafe {
            core::ptr::copy_nonoverlapping(
                virt_addr_chirho as *const u8,
                buf_chirho.as_mut_ptr(),
                to_read_chirho,
            );
        }

        file_chirho.pos_chirho += to_read_chirho as u64;
        Ok(to_read_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
        let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let total_size_chirho = (stride_chirho as u64) * (height_chirho as u64);

        let pos_chirho = file_chirho.pos_chirho;
        if pos_chirho >= total_size_chirho {
            return Ok(0);
        }

        let remaining_chirho = (total_size_chirho - pos_chirho) as usize;
        let to_write_chirho = buf_chirho.len().min(remaining_chirho);

        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let virt_addr_chirho = phys_chirho + phys_offset_chirho + pos_chirho;

        unsafe {
            core::ptr::copy_nonoverlapping(
                buf_chirho.as_ptr(),
                virt_addr_chirho as *mut u8,
                to_write_chirho,
            );
        }

        file_chirho.pos_chirho += to_write_chirho as u64;
        Ok(to_write_chirho)
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let total_size_chirho = (stride_chirho as u64) * (height_chirho as u64);

        let new_pos_chirho = match whence_chirho {
            0 /* SEEK_SET */ => offset_chirho as u64,
            1 /* SEEK_CUR */ => {
                let p_chirho = file_chirho.pos_chirho as i64 + offset_chirho;
                if p_chirho < 0 { return Err(-EINVAL_CHIRHO); }
                p_chirho as u64
            }
            2 /* SEEK_END */ => {
                let p_chirho = total_size_chirho as i64 + offset_chirho;
                if p_chirho < 0 { return Err(-EINVAL_CHIRHO); }
                p_chirho as u64
            }
            _ => return Err(-EINVAL_CHIRHO),
        };

        file_chirho.pos_chirho = new_pos_chirho;
        Ok(new_pos_chirho)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        crate::serial_println_chirho!(
            "[FB-IOCTL] cmd={:#x} arg={:#x}", cmd_chirho, arg_chirho
        );
        match cmd_chirho {
            FBIOGET_VSCREENINFO_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EFAULT_CHIRHO);
                }
                let info_chirho = FbVarScreenInfoChirho::new_chirho();
                let size_chirho = core::mem::size_of::<FbVarScreenInfoChirho>();
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &info_chirho as *const FbVarScreenInfoChirho as *const u8,
                        size_chirho,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho,
                    src_chirho,
                    size_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            FBIOPUT_VSCREENINFO_CHIRHO => {
                // Accept but ignore mode changes
                Ok(0)
            }

            FBIOGET_FSCREENINFO_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EFAULT_CHIRHO);
                }
                let info_chirho = FbFixScreenInfoChirho::new_chirho();
                let size_chirho = core::mem::size_of::<FbFixScreenInfoChirho>();
                let src_chirho = unsafe {
                    core::slice::from_raw_parts(
                        &info_chirho as *const FbFixScreenInfoChirho as *const u8,
                        size_chirho,
                    )
                };
                if crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho,
                    src_chirho,
                    size_chirho,
                ).is_err() {
                    return Err(-EFAULT_CHIRHO);
                }
                Ok(0)
            }

            FBIOGETCMAP_CHIRHO | FBIOPUTCMAP_CHIRHO => {
                // Truecolor framebuffer doesn't use a colormap
                Ok(0)
            }

            FBIOBLANK_CHIRHO => {
                // Screen blanking: accept silently
                Ok(0)
            }

            FBIOPAN_DISPLAY_CHIRHO => {
                // Screen panning / double-buffering: accept silently
                // (fixed framebuffer, no panning hardware)
                Ok(0)
            }

            _ => Err(-EINVAL_CHIRHO),
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

/// Static instance of /dev/fb0 file operations.
pub static FB_DEVICE_OPS_CHIRHO: FbDeviceOpsChirho = FbDeviceOpsChirho;

/// Get the framebuffer physical address for mmap support.
pub fn fb_phys_addr_chirho() -> u64 {
    FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed)
}

/// Track the fd number that was opened for /dev/fb0 (set by open handler).
static FB_OPEN_FD_CHIRHO: core::sync::atomic::AtomicI64 =
    core::sync::atomic::AtomicI64::new(-1);

/// Record which fd was assigned to /dev/fb0 (called from open).
pub fn set_fb_fd_chirho(fd_chirho: u64) {
    FB_OPEN_FD_CHIRHO.store(fd_chirho as i64, core::sync::atomic::Ordering::Relaxed);
}

/// Check if a given fd is the framebuffer device (for mmap special-casing).
/// Looks up the fd's inode to check major=29, minor=0.
pub fn is_fb_fd_chirho(fd_chirho: u64) -> bool {
    if let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        let file_guard_chirho = file_arc_chirho.lock();
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        if let Some(ref data_chirho) = inode_guard_chirho.fs_data_chirho {
            if let Some(dev_chirho) = data_chirho.downcast_ref::<crate::devtmpfs_chirho::DevNodeDataChirho>() {
                return dev_chirho.major_chirho == 29 && dev_chirho.minor_chirho == 0;
            }
        }
    }
    false
}

/// Get the framebuffer total size for mmap support.
pub fn fb_size_chirho() -> u64 {
    let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
    let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
    (stride_chirho as u64) * (height_chirho as u64)
}

const BASE64_TABLE_CHIRHO: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const BASE64_WRAP_COLUMNS_CHIRHO: usize = 256;
const BASE64_OUTPUT_BUFFER_BYTES_CHIRHO: usize = 2048;
const FB_RGB_CHUNK_PIXELS_CHIRHO: usize = 256;

struct Base64StreamEncoderChirho {
    carry_chirho: [u8; 3],
    carry_len_chirho: usize,
    output_buf_chirho: [u8; BASE64_OUTPUT_BUFFER_BYTES_CHIRHO],
    output_len_chirho: usize,
    line_col_chirho: usize,
}

impl Base64StreamEncoderChirho {
    fn new_chirho() -> Self {
        Self {
            carry_chirho: [0; 3],
            carry_len_chirho: 0,
            output_buf_chirho: [0; BASE64_OUTPUT_BUFFER_BYTES_CHIRHO],
            output_len_chirho: 0,
            line_col_chirho: 0,
        }
    }

    fn flush_output_chirho(&mut self) {
        if self.output_len_chirho == 0 {
            return;
        }
        crate::serial_chirho::serial_write_bytes_chirho(
            &self.output_buf_chirho[..self.output_len_chirho],
        );
        self.output_len_chirho = 0;
    }

    fn push_raw_byte_chirho(&mut self, byte_chirho: u8) {
        if self.output_len_chirho >= self.output_buf_chirho.len() {
            self.flush_output_chirho();
        }
        self.output_buf_chirho[self.output_len_chirho] = byte_chirho;
        self.output_len_chirho += 1;
    }

    fn push_base64_byte_chirho(&mut self, byte_chirho: u8) {
        if self.line_col_chirho >= BASE64_WRAP_COLUMNS_CHIRHO {
            self.push_raw_byte_chirho(b'\n');
            self.line_col_chirho = 0;
        }
        self.push_raw_byte_chirho(byte_chirho);
        self.line_col_chirho += 1;
    }

    fn emit_triplet_chirho(&mut self, b0_chirho: u8, b1_chirho: u8, b2_chirho: u8) {
        let idx0_chirho = (b0_chirho >> 2) as usize;
        let idx1_chirho = (((b0_chirho & 0x03) << 4) | (b1_chirho >> 4)) as usize;
        let idx2_chirho = (((b1_chirho & 0x0f) << 2) | (b2_chirho >> 6)) as usize;
        let idx3_chirho = (b2_chirho & 0x3f) as usize;

        self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx0_chirho]);
        self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx1_chirho]);
        self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx2_chirho]);
        self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx3_chirho]);
    }

    fn feed_bytes_chirho(&mut self, bytes_chirho: &[u8]) {
        let mut offset_chirho = 0usize;

        if self.carry_len_chirho > 0 {
            while self.carry_len_chirho < 3 && offset_chirho < bytes_chirho.len() {
                self.carry_chirho[self.carry_len_chirho] = bytes_chirho[offset_chirho];
                self.carry_len_chirho += 1;
                offset_chirho += 1;
            }
            if self.carry_len_chirho == 3 {
                self.emit_triplet_chirho(
                    self.carry_chirho[0],
                    self.carry_chirho[1],
                    self.carry_chirho[2],
                );
                self.carry_len_chirho = 0;
            }
        }

        while offset_chirho + 3 <= bytes_chirho.len() {
            self.emit_triplet_chirho(
                bytes_chirho[offset_chirho],
                bytes_chirho[offset_chirho + 1],
                bytes_chirho[offset_chirho + 2],
            );
            offset_chirho += 3;
        }

        while offset_chirho < bytes_chirho.len() {
            self.carry_chirho[self.carry_len_chirho] = bytes_chirho[offset_chirho];
            self.carry_len_chirho += 1;
            offset_chirho += 1;
        }
    }

    fn finish_chirho(&mut self) {
        match self.carry_len_chirho {
            1 => {
                let b0_chirho = self.carry_chirho[0];
                let idx0_chirho = (b0_chirho >> 2) as usize;
                let idx1_chirho = ((b0_chirho & 0x03) << 4) as usize;
                self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx0_chirho]);
                self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx1_chirho]);
                self.push_base64_byte_chirho(b'=');
                self.push_base64_byte_chirho(b'=');
            }
            2 => {
                let b0_chirho = self.carry_chirho[0];
                let b1_chirho = self.carry_chirho[1];
                let idx0_chirho = (b0_chirho >> 2) as usize;
                let idx1_chirho = (((b0_chirho & 0x03) << 4) | (b1_chirho >> 4)) as usize;
                let idx2_chirho = ((b1_chirho & 0x0f) << 2) as usize;
                self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx0_chirho]);
                self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx1_chirho]);
                self.push_base64_byte_chirho(BASE64_TABLE_CHIRHO[idx2_chirho]);
                self.push_base64_byte_chirho(b'=');
            }
            _ => {}
        }

        self.carry_len_chirho = 0;
        if self.line_col_chirho != 0 {
            self.push_raw_byte_chirho(b'\n');
            self.line_col_chirho = 0;
        }
        self.flush_output_chirho();
    }
}

fn append_bytes_chirho(
    output_chirho: &mut [u8],
    output_len_chirho: &mut usize,
    bytes_chirho: &[u8],
) {
    let remaining_chirho = output_chirho.len().saturating_sub(*output_len_chirho);
    let copy_len_chirho = remaining_chirho.min(bytes_chirho.len());
    output_chirho[*output_len_chirho..*output_len_chirho + copy_len_chirho]
        .copy_from_slice(&bytes_chirho[..copy_len_chirho]);
    *output_len_chirho += copy_len_chirho;
}

fn append_u64_decimal_chirho(
    output_chirho: &mut [u8],
    output_len_chirho: &mut usize,
    value_chirho: u64,
) {
    let mut scratch_chirho = [0u8; 20];
    let mut digits_len_chirho = 0usize;
    let mut work_chirho = value_chirho;

    if work_chirho == 0 {
        scratch_chirho[0] = b'0';
        digits_len_chirho = 1;
    } else {
        while work_chirho > 0 && digits_len_chirho < scratch_chirho.len() {
            scratch_chirho[digits_len_chirho] = b'0' + (work_chirho % 10) as u8;
            digits_len_chirho += 1;
            work_chirho /= 10;
        }
        scratch_chirho[..digits_len_chirho].reverse();
    }

    append_bytes_chirho(
        output_chirho,
        output_len_chirho,
        &scratch_chirho[..digits_len_chirho],
    );
}

fn serial_write_line_chirho(bytes_chirho: &[u8]) {
    crate::serial_chirho::serial_write_bytes_chirho(bytes_chirho);
    crate::serial_chirho::serial_write_bytes_chirho(b"\r\n");
}

fn build_ppm_header_chirho(width_chirho: u32, height_chirho: u32) -> ([u8; 32], usize) {
    let mut header_buf_chirho = [0u8; 32];
    let mut header_len_chirho = 0usize;
    append_bytes_chirho(&mut header_buf_chirho, &mut header_len_chirho, b"P6\n");
    append_u64_decimal_chirho(
        &mut header_buf_chirho,
        &mut header_len_chirho,
        width_chirho as u64,
    );
    append_bytes_chirho(&mut header_buf_chirho, &mut header_len_chirho, b" ");
    append_u64_decimal_chirho(
        &mut header_buf_chirho,
        &mut header_len_chirho,
        height_chirho as u64,
    );
    append_bytes_chirho(&mut header_buf_chirho, &mut header_len_chirho, b"\n255\n");
    (header_buf_chirho, header_len_chirho)
}

/// One-shot timer-triggered framebuffer dump gate.
pub fn maybe_dump_framebuffer_after_tick_chirho(tick_count_chirho: u64) {
    if tick_count_chirho < FB_DUMP_TRIGGER_TICKS_CHIRHO {
        return;
    }
    if FB_DUMP_DONE_CHIRHO
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    dump_framebuffer_chirho();
}

/// Dump the live framebuffer as a base64-encoded PPM image over serial.
///
/// This is intentionally a heavy, one-shot diagnostic path. It reads the
/// QEMU-visible BGRA framebuffer, converts it to RGB PPM (`P6`), base64-encodes
/// the stream without heap allocation, and emits it with begin/end markers.
pub fn dump_framebuffer_chirho() {
    let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
    let width_chirho = FB_ACTUAL_WIDTH_CHIRHO.load(Ordering::Relaxed);
    let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
    let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
    let bpp_chirho = FB_ACTUAL_BPP_CHIRHO.load(Ordering::Relaxed);
    let is_bgr_chirho = FB_IS_BGR_CHIRHO.load(Ordering::Relaxed);

    if phys_chirho == 0 || width_chirho == 0 || height_chirho == 0 {
        serial_write_line_chirho(b"[FB-DUMP-ERROR] framebuffer-not-configured");
        return;
    }

    if bpp_chirho != 32 {
        let mut error_buf_chirho = [0u8; 96];
        let mut error_len_chirho = 0usize;
        append_bytes_chirho(
            &mut error_buf_chirho,
            &mut error_len_chirho,
            b"[FB-DUMP-ERROR] unsupported-bpp=",
        );
        append_u64_decimal_chirho(
            &mut error_buf_chirho,
            &mut error_len_chirho,
            bpp_chirho as u64,
        );
        serial_write_line_chirho(&error_buf_chirho[..error_len_chirho]);
        return;
    }

    // Downsample to thumbnail for faster serial transfer
    // 160x100 = 48KB PPM = 64KB base64 = ~6 seconds at 115200 baud
    let thumb_w_chirho: u32 = 160;
    let thumb_h_chirho: u32 = 100;
    let scale_x_chirho = width_chirho / thumb_w_chirho;
    let scale_y_chirho = height_chirho / thumb_h_chirho;
    let (ppm_header_chirho, ppm_header_len_chirho) =
        build_ppm_header_chirho(thumb_w_chirho, thumb_h_chirho);
    let ppm_payload_bytes_chirho =
        ppm_header_len_chirho as u64 + thumb_w_chirho as u64 * thumb_h_chirho as u64 * 3;
    let ppm_base64_bytes_chirho = ((ppm_payload_bytes_chirho + 2) / 3) * 4;

    let mut begin_buf_chirho = [0u8; 192];
    let mut begin_len_chirho = 0usize;
    append_bytes_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        b"[FB-DUMP-BEGIN] format=ppm;base64 width=",
    );
    append_u64_decimal_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        thumb_w_chirho as u64,
    );
    append_bytes_chirho(&mut begin_buf_chirho, &mut begin_len_chirho, b" height=");
    append_u64_decimal_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        thumb_h_chirho as u64,
    );
    append_bytes_chirho(&mut begin_buf_chirho, &mut begin_len_chirho, b" bpp=");
    append_u64_decimal_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        bpp_chirho as u64,
    );
    append_bytes_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        b" ppm_bytes=",
    );
    append_u64_decimal_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        ppm_payload_bytes_chirho,
    );
    append_bytes_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        b" base64_bytes=",
    );
    append_u64_decimal_chirho(
        &mut begin_buf_chirho,
        &mut begin_len_chirho,
        ppm_base64_bytes_chirho,
    );
    serial_write_line_chirho(&begin_buf_chirho[..begin_len_chirho]);

    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let base_ptr_chirho = (phys_chirho + phys_offset_chirho) as *const u8;
    let width_usize_chirho = width_chirho as usize;
    let height_usize_chirho = height_chirho as usize;
    let stride_usize_chirho = stride_chirho as usize;

    let mut encoder_chirho = Base64StreamEncoderChirho::new_chirho();
    encoder_chirho.feed_bytes_chirho(&ppm_header_chirho[..ppm_header_len_chirho]);

    let mut rgb_chunk_chirho = [0u8; FB_RGB_CHUNK_PIXELS_CHIRHO * 3];
    let thumb_w_usize_chirho = thumb_w_chirho as usize;
    let thumb_h_usize_chirho = thumb_h_chirho as usize;
    let scale_x_usize_chirho = scale_x_chirho as usize;
    let scale_y_usize_chirho = scale_y_chirho as usize;
    for ty_chirho in 0..thumb_h_usize_chirho {
        let y_chirho = ty_chirho * scale_y_usize_chirho;
        let row_offset_chirho = y_chirho * stride_usize_chirho;
        let mut tx_chirho = 0usize;
        while tx_chirho < thumb_w_usize_chirho {
            let chunk_pixels_chirho =
                core::cmp::min(FB_RGB_CHUNK_PIXELS_CHIRHO, thumb_w_usize_chirho - tx_chirho);
            for pixel_index_chirho in 0..chunk_pixels_chirho {
                let x_chirho = (tx_chirho + pixel_index_chirho) * scale_x_usize_chirho;
                let pixel_offset_chirho = row_offset_chirho + x_chirho * 4;
                let pixel_value_chirho = unsafe {
                    core::ptr::read_volatile(
                        base_ptr_chirho.add(pixel_offset_chirho) as *const u32
                    )
                };

                let byte0_chirho = (pixel_value_chirho & 0xff) as u8;
                let byte1_chirho = ((pixel_value_chirho >> 8) & 0xff) as u8;
                let byte2_chirho = ((pixel_value_chirho >> 16) & 0xff) as u8;
                let out_offset_chirho = pixel_index_chirho * 3;

                if is_bgr_chirho {
                    rgb_chunk_chirho[out_offset_chirho] = byte2_chirho;
                    rgb_chunk_chirho[out_offset_chirho + 1] = byte1_chirho;
                    rgb_chunk_chirho[out_offset_chirho + 2] = byte0_chirho;
                } else {
                    rgb_chunk_chirho[out_offset_chirho] = byte0_chirho;
                    rgb_chunk_chirho[out_offset_chirho + 1] = byte1_chirho;
                    rgb_chunk_chirho[out_offset_chirho + 2] = byte2_chirho;
                }
            }

            encoder_chirho.feed_bytes_chirho(&rgb_chunk_chirho[..chunk_pixels_chirho * 3]);
            tx_chirho += chunk_pixels_chirho;
        }
    }

    encoder_chirho.finish_chirho();

    let mut end_buf_chirho = [0u8; 96];
    let mut end_len_chirho = 0usize;
    append_bytes_chirho(
        &mut end_buf_chirho,
        &mut end_len_chirho,
        b"[FB-DUMP-END] width=",
    );
    append_u64_decimal_chirho(
        &mut end_buf_chirho,
        &mut end_len_chirho,
        width_chirho as u64,
    );
    append_bytes_chirho(&mut end_buf_chirho, &mut end_len_chirho, b" height=");
    append_u64_decimal_chirho(
        &mut end_buf_chirho,
        &mut end_len_chirho,
        height_chirho as u64,
    );
    serial_write_line_chirho(&end_buf_chirho[..end_len_chirho]);
}

/// Sample the live framebuffer contents and log when the sparse signature changes.
///
/// This is a low-overhead probe for proving that Xorg/ShadowFB eventually
/// pushes pixels into the QEMU-visible framebuffer, which in turn is what VNC
/// displays.
pub fn sample_fb_signature_chirho(reason_chirho: &str) {
    use core::sync::atomic::{AtomicU64, Ordering as FbOrderingChirho};

    static LAST_HASH_CHIRHO: AtomicU64 = AtomicU64::new(0);
    static LAST_NONZERO_CHIRHO: AtomicU64 = AtomicU64::new(0);
    static TRACE_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);

    let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
    let total_size_chirho = fb_size_chirho() as usize;
    if phys_chirho == 0 || total_size_chirho == 0 {
        return;
    }

    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let base_ptr_chirho = (phys_chirho + phys_offset_chirho) as *const u8;
    let sample_slots_chirho = 1024usize;
    let sample_step_chirho = core::cmp::max(1usize, total_size_chirho / sample_slots_chirho);

    let mut hash_chirho: u64 = 0xcbf2_9ce4_8422_2325;
    let mut nonzero_samples_chirho: u64 = 0;
    let mut sample_count_chirho: u64 = 0;
    let mut offset_chirho: usize = 0;
    while offset_chirho < total_size_chirho {
        let byte_chirho = unsafe { core::ptr::read_volatile(base_ptr_chirho.add(offset_chirho)) };
        hash_chirho ^= byte_chirho as u64;
        hash_chirho = hash_chirho.wrapping_mul(0x1000_0000_01b3);
        if byte_chirho != 0 {
            nonzero_samples_chirho = nonzero_samples_chirho.saturating_add(1);
        }
        sample_count_chirho = sample_count_chirho.saturating_add(1);
        offset_chirho = offset_chirho.saturating_add(sample_step_chirho);
    }

    let last_hash_chirho = LAST_HASH_CHIRHO.swap(hash_chirho, FbOrderingChirho::Relaxed);
    let last_nonzero_chirho =
        LAST_NONZERO_CHIRHO.swap(nonzero_samples_chirho, FbOrderingChirho::Relaxed);
    if last_hash_chirho != hash_chirho || last_nonzero_chirho != nonzero_samples_chirho {
        let trace_index_chirho = TRACE_COUNT_CHIRHO.fetch_add(1, FbOrderingChirho::Relaxed);
        if trace_index_chirho < 256 {
            crate::serial_println_chirho!(
                "[FB-SIG] #{} reason='{}' hash={:#x} nonzero_samples={} sampled={} fb_size={} phys={:#x}",
                trace_index_chirho,
                reason_chirho,
                hash_chirho,
                nonzero_samples_chirho,
                sample_count_chirho,
                total_size_chirho,
                phys_chirho,
            );
        }
    }
}
