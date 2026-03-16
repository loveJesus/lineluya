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

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};

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
static FB_IS_BGR_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

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
/// Total size = 68 bytes (Linux uses a padded 80-byte version, but the
/// first 68 bytes are what user-space fbdev actually reads).
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
    pub mmio_start_chirho: u64,     // start of MMIO (0 for us)
    pub mmio_len_chirho: u32,       // length of MMIO (0)
    pub accel_chirho: u32,          // FB_ACCEL_NONE = 0
    pub capabilities_chirho: u16,
    pub reserved_chirho: [u16; 2],
}

impl FbFixScreenInfoChirho {
    fn new_chirho() -> Self {
        let phys_chirho = FB_ACTUAL_PHYS_CHIRHO.load(Ordering::Relaxed);
        let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
        let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
        let size_chirho = stride_chirho as u32 * height_chirho;

        let mut id_chirho = [0u8; 16];
        let id_str_chirho = b"lineluya-fb0";
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
            mmio_start_chirho: 0,
            mmio_len_chirho: 0,
            accel_chirho: 0,     // FB_ACCEL_NONE
            capabilities_chirho: 0,
            reserved_chirho: [0; 2],
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
pub fn is_fb_fd_chirho(fd_chirho: u64) -> bool {
    let stored_chirho = FB_OPEN_FD_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
    stored_chirho >= 0 && stored_chirho == fd_chirho as i64
}

/// Get the framebuffer total size for mmap support.
pub fn fb_size_chirho() -> u64 {
    let stride_chirho = FB_ACTUAL_STRIDE_CHIRHO.load(Ordering::Relaxed);
    let height_chirho = FB_ACTUAL_HEIGHT_CHIRHO.load(Ordering::Relaxed);
    (stride_chirho as u64) * (height_chirho as u64)
}
