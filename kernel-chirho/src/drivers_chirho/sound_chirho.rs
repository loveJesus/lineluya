// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Sound subsystem for the Lineluya kernel (A2-SOUND-001 / A2-SOUND-002).
//!
//! Provides:
//! - PCI detection of Intel AC97 (vendor 0x8086, device 0x2415) and
//!   Intel HDA (vendor 0x8086, device 0x2668) sound cards.
//! - `/dev/dsp` OSS device node (major 14, minor 3) with stub file
//!   operations that silently consume PCM write data and handle
//!   basic OSS ioctls (SNDCTL_DSP_SETFMT, SNDCTL_DSP_SPEED,
//!   SNDCTL_DSP_CHANNELS).
//!
//! - AC97 controller struct with PCI BAR discovery (`Ac97ControllerChirho`).
//!
//! The actual audio hardware DMA programming is not yet implemented;
//! this layer provides the device interface so userspace programs
//! that probe for `/dev/dsp` do not fail immediately.

use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use crate::syscall_chirho::{EINVAL_CHIRHO, ENOSYS_CHIRHO};
use crate::pci_chirho::{PciDeviceChirho, scan_bus_chirho};

// ============================================================================
// PCI vendor/device IDs for sound cards
// ============================================================================

/// Intel vendor ID.
const INTEL_VENDOR_ID_CHIRHO: u16 = 0x8086;

/// Intel AC97 audio controller device ID.
const INTEL_AC97_DEVICE_ID_CHIRHO: u16 = 0x2415;

/// Intel HDA (High Definition Audio) controller device ID.
const INTEL_HDA_DEVICE_ID_CHIRHO: u16 = 0x2668;

/// PCI multimedia audio class (class 0x04, subclass 0x01).
#[allow(dead_code)]
const PCI_CLASS_MULTIMEDIA_AUDIO_CHIRHO: u8 = 0x04;
#[allow(dead_code)]
const PCI_SUBCLASS_AUDIO_CHIRHO: u8 = 0x01;

// ============================================================================
// OSS ioctl command numbers
// ============================================================================

/// SNDCTL_DSP_SETFMT — set audio sample format.
const SNDCTL_DSP_SETFMT_CHIRHO: u64 = 0xC0045005;

/// SNDCTL_DSP_SPEED — set sample rate.
const SNDCTL_DSP_SPEED_CHIRHO: u64 = 0xC0045002;

/// SNDCTL_DSP_CHANNELS — set number of channels.
const SNDCTL_DSP_CHANNELS_CHIRHO: u64 = 0xC0045006;

/// SNDCTL_DSP_GETFMTS — query supported formats.
const SNDCTL_DSP_GETFMTS_CHIRHO: u64 = 0x8004500B;

/// SNDCTL_DSP_GETCAPS — query device capabilities.
const SNDCTL_DSP_GETCAPS_CHIRHO: u64 = 0x8004500F;

/// AFMT_S16_LE — signed 16-bit little-endian PCM.
const AFMT_S16_LE_CHIRHO: i64 = 0x10;

/// Default sample rate (44100 Hz CD quality).
const DEFAULT_SAMPLE_RATE_CHIRHO: i64 = 44100;

/// Default channel count (stereo).
const DEFAULT_CHANNELS_CHIRHO: i64 = 2;

// ============================================================================
// PCI sound card detection
// ============================================================================

/// Scan PCI bus 0 for known sound cards and log detections.
///
/// This is called during kernel init to detect Intel AC97 and HDA
/// controllers. No hardware initialization is performed yet.
pub fn detect_sound_cards_chirho() {
    crate::serial_debug_chirho!("SOUND: scanning PCI bus 0 for audio controllers...");

    let devices_chirho = unsafe { scan_bus_chirho(0) };
    let mut found_count_chirho: u32 = 0;

    for dev_chirho in &devices_chirho {
        // Check for Intel AC97
        if dev_chirho.vendor_id_chirho == INTEL_VENDOR_ID_CHIRHO
            && dev_chirho.device_id_chirho == INTEL_AC97_DEVICE_ID_CHIRHO
        {
            crate::serial_println_chirho!(
                "SOUND: Intel AC97 audio controller detected at PCI {:02x}:{:02x}.{} (vendor={:#06x} device={:#06x})",
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                dev_chirho.vendor_id_chirho,
                dev_chirho.device_id_chirho,
            );
            found_count_chirho += 1;
        }

        // Check for Intel HDA
        if dev_chirho.vendor_id_chirho == INTEL_VENDOR_ID_CHIRHO
            && dev_chirho.device_id_chirho == INTEL_HDA_DEVICE_ID_CHIRHO
        {
            crate::serial_println_chirho!(
                "SOUND: Intel HDA audio controller detected at PCI {:02x}:{:02x}.{} (vendor={:#06x} device={:#06x})",
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                dev_chirho.vendor_id_chirho,
                dev_chirho.device_id_chirho,
            );
            found_count_chirho += 1;
        }

        // Also detect by class code (multimedia audio = 0x04:0x01)
        if dev_chirho.class_code_chirho == PCI_CLASS_MULTIMEDIA_AUDIO_CHIRHO
            && dev_chirho.subclass_chirho == PCI_SUBCLASS_AUDIO_CHIRHO
            && !(dev_chirho.vendor_id_chirho == INTEL_VENDOR_ID_CHIRHO
                && (dev_chirho.device_id_chirho == INTEL_AC97_DEVICE_ID_CHIRHO
                    || dev_chirho.device_id_chirho == INTEL_HDA_DEVICE_ID_CHIRHO))
        {
            crate::serial_println_chirho!(
                "SOUND: Unknown audio controller at PCI {:02x}:{:02x}.{} (vendor={:#06x} device={:#06x} class=04:01)",
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                dev_chirho.vendor_id_chirho,
                dev_chirho.device_id_chirho,
            );
            found_count_chirho += 1;
        }
    }

    if found_count_chirho == 0 {
        crate::serial_println_chirho!("SOUND: no audio controllers found on PCI bus 0");
    } else {
        crate::serial_println_chirho!(
            "SOUND: {} audio controller(s) detected (stub /dev/dsp available)",
            found_count_chirho
        );
    }
}

// ============================================================================
// DevDspOpsChirho — /dev/dsp (major 14, minor 3) — OSS audio device
// ============================================================================

/// File operations for `/dev/dsp` (OSS audio device).
///
/// - `read` returns 0 (silence / EOF — capture not supported).
/// - `write` silently consumes PCM data (playback stub).
/// - `ioctl` handles `SNDCTL_DSP_SETFMT`, `SNDCTL_DSP_SPEED`,
///   `SNDCTL_DSP_CHANNELS`, returning sane defaults.
pub struct DevDspOpsChirho;

impl FileOpsChirho for DevDspOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // No audio capture support yet — return 0 (EOF).
        Ok(0)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // Silently consume PCM data — playback is a stub.
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        // Audio devices are not seekable.
        Err(-29) // ESPIPE
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        match cmd_chirho {
            SNDCTL_DSP_SETFMT_CHIRHO => {
                // Return AFMT_S16_LE regardless of what was requested.
                if arg_chirho != 0 {
                    unsafe {
                        let ptr_chirho = arg_chirho as *mut i32;
                        if !ptr_chirho.is_null() {
                            core::ptr::write_volatile(ptr_chirho, AFMT_S16_LE_CHIRHO as i32);
                        }
                    }
                }
                Ok(AFMT_S16_LE_CHIRHO)
            }
            SNDCTL_DSP_SPEED_CHIRHO => {
                // Return 44100 Hz regardless.
                if arg_chirho != 0 {
                    unsafe {
                        let ptr_chirho = arg_chirho as *mut i32;
                        if !ptr_chirho.is_null() {
                            core::ptr::write_volatile(ptr_chirho, DEFAULT_SAMPLE_RATE_CHIRHO as i32);
                        }
                    }
                }
                Ok(DEFAULT_SAMPLE_RATE_CHIRHO)
            }
            SNDCTL_DSP_CHANNELS_CHIRHO => {
                // Return 2 (stereo) regardless.
                if arg_chirho != 0 {
                    unsafe {
                        let ptr_chirho = arg_chirho as *mut i32;
                        if !ptr_chirho.is_null() {
                            core::ptr::write_volatile(ptr_chirho, DEFAULT_CHANNELS_CHIRHO as i32);
                        }
                    }
                }
                Ok(DEFAULT_CHANNELS_CHIRHO)
            }
            SNDCTL_DSP_GETFMTS_CHIRHO => {
                // Report only S16_LE support.
                Ok(AFMT_S16_LE_CHIRHO)
            }
            SNDCTL_DSP_GETCAPS_CHIRHO => {
                // No special capabilities.
                Ok(0)
            }
            _ => {
                crate::serial_debug_chirho!(
                    "SOUND: /dev/dsp unhandled ioctl cmd={:#x} arg={:#x}",
                    cmd_chirho,
                    arg_chirho
                );
                Err(-ENOSYS_CHIRHO)
            }
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/dsp file operations.
pub static DEV_DSP_OPS_CHIRHO: DevDspOpsChirho = DevDspOpsChirho;

// ============================================================================
// AC97 controller driver (A2-SOUND-005)
// ============================================================================

/// AC97 controller state — holds PCI BARs and IRQ discovered during init.
///
/// The Intel ICH AC97 audio controller uses two I/O port regions:
/// - NAM BAR (BAR0): Audio Mixer register I/O (Native Audio Mixer).
/// - NABM BAR (BAR1): Bus Master register I/O (Native Audio Bus Master).
///
/// Full DMA buffer programming is not yet implemented — this struct records
/// the device location so that a future DMA path can use it.
#[allow(dead_code)]
pub struct Ac97ControllerChirho {
    /// Bus Master BAR I/O port base (NABM — BAR1).
    pub nabm_bar_chirho: u16,
    /// Mixer BAR I/O port base (NAM — BAR0).
    pub nam_bar_chirho: u16,
    /// PCI interrupt line.
    pub irq_chirho: u8,
    /// PCI bus number where the device was found.
    pub bus_chirho: u8,
    /// PCI device number where the device was found.
    pub device_chirho: u8,
    /// PCI function number where the device was found.
    pub function_chirho: u8,
}

/// Global AC97 controller instance (set during init if device found).
static AC97_CONTROLLER_CHIRHO: spin::Mutex<Option<Ac97ControllerChirho>> =
    spin::Mutex::new(None);

/// Probe PCI bus for an Intel AC97 controller and read its BARs.
///
/// If an AC97 device (vendor 0x8086, device 0x2415) is found, reads BAR0
/// (NAM — mixer I/O) and BAR1 (NABM — bus master I/O) from PCI config space,
/// along with the interrupt line. Stores the result in [`AC97_CONTROLLER_CHIRHO`].
///
/// Does NOT perform full codec reset or DMA buffer setup — that comes in a
/// later task when we actually want to push PCM samples to hardware.
pub fn init_ac97_chirho() {
    crate::serial_debug_chirho!("AC97: probing PCI bus 0 for Intel AC97 controller...");

    let devices_chirho = unsafe { scan_bus_chirho(0) };

    for dev_chirho in &devices_chirho {
        if dev_chirho.vendor_id_chirho != INTEL_VENDOR_ID_CHIRHO
            || dev_chirho.device_id_chirho != INTEL_AC97_DEVICE_ID_CHIRHO
        {
            continue;
        }

        // Found AC97 — read BAR0 (NAM) and BAR1 (NABM) from config space.
        let bar0_raw_chirho = unsafe {
            crate::pci_chirho::pci_config_read_u32_chirho(
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                0x10, // BAR0
            )
        };
        let bar1_raw_chirho = unsafe {
            crate::pci_chirho::pci_config_read_u32_chirho(
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                0x14, // BAR1
            )
        };
        let irq_raw_chirho = unsafe {
            crate::pci_chirho::pci_config_read_u8_chirho(
                dev_chirho.bus_chirho,
                dev_chirho.device_chirho,
                dev_chirho.function_chirho,
                0x3C, // Interrupt Line
            )
        };

        // AC97 BARs are I/O space (bit 0 set). Mask to get port base.
        let nam_port_chirho = (bar0_raw_chirho & 0xFFFC) as u16;
        let nabm_port_chirho = (bar1_raw_chirho & 0xFFFC) as u16;

        crate::serial_debug_chirho!(
            "AC97: found at PCI {:02x}:{:02x}.{} — NAM(BAR0)={:#06x} NABM(BAR1)={:#06x} IRQ={}",
            dev_chirho.bus_chirho,
            dev_chirho.device_chirho,
            dev_chirho.function_chirho,
            nam_port_chirho,
            nabm_port_chirho,
            irq_raw_chirho,
        );

        let controller_chirho = Ac97ControllerChirho {
            nabm_bar_chirho: nabm_port_chirho,
            nam_bar_chirho: nam_port_chirho,
            irq_chirho: irq_raw_chirho,
            bus_chirho: dev_chirho.bus_chirho,
            device_chirho: dev_chirho.device_chirho,
            function_chirho: dev_chirho.function_chirho,
        };

        *AC97_CONTROLLER_CHIRHO.lock() = Some(controller_chirho);
        crate::serial_debug_chirho!("AC97: controller registered (DMA not yet enabled)");
        return;
    }

    crate::serial_debug_chirho!("AC97: no Intel AC97 controller found on PCI bus 0");
}

/// Returns `true` if an AC97 controller was detected and initialized.
#[allow(dead_code)]
pub fn ac97_detected_chirho() -> bool {
    AC97_CONTROLLER_CHIRHO.lock().is_some()
}
