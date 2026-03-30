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
use crate::syscall_chirho::{EINVAL_CHIRHO, ENOSYS_CHIRHO, ENOTTY_CHIRHO};
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

/// TIOCGPGRP — get foreground process group of a terminal.
const TIOCGPGRP_CHIRHO: u64 = 0x5413;

/// TIOCSPGRP — set foreground process group of a terminal.
const TIOCSPGRP_CHIRHO: u64 = 0x5414;

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
        // If SB16 is available, send PCM data to it for real playback.
        if sb16_detected_chirho() {
            use core::sync::atomic::{AtomicU64, Ordering};
            static DSP_WRITE_BYTES_CHIRHO: AtomicU64 = AtomicU64::new(0);
            static DSP_WRITE_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
            let total_chirho = DSP_WRITE_BYTES_CHIRHO.fetch_add(buf_chirho.len() as u64, Ordering::Relaxed);
            let count_chirho = DSP_WRITE_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed);
            if count_chirho < 5 || (count_chirho % 100 == 0) {
                crate::serial_println_chirho!(
                    "[DSP-WRITE] #{} len={} total={}",
                    count_chirho, buf_chirho.len(), total_chirho + buf_chirho.len() as u64,
                );
            }
            sb16_write_pcm_chirho(buf_chirho);
            return Ok(buf_chirho.len());
        }

        // Fallback: drive PC speaker with PCM data for pitch-modulated audio.
        // Sample the PCM buffer at intervals and map amplitude → frequency.
        // This produces recognizable pitch contours from music.
        if buf_chirho.len() >= 2 {
            // Average several samples for a stable frequency estimate.
            // PCM is signed 16-bit LE stereo (4 bytes/frame at 44100Hz).
            let num_samples_chirho = buf_chirho.len() / 2;
            let step_chirho = if num_samples_chirho > 16 { num_samples_chirho / 16 } else { 1 };
            let mut sum_chirho: i64 = 0;
            let mut count_chirho: u32 = 0;
            let mut i_chirho = 0usize;
            while i_chirho + 1 < buf_chirho.len() && count_chirho < 32 {
                let s_chirho = i16::from_le_bytes([buf_chirho[i_chirho], buf_chirho[i_chirho + 1]]);
                sum_chirho += s_chirho.unsigned_abs() as i64;
                count_chirho += 1;
                i_chirho += step_chirho * 2;
            }
            if count_chirho > 0 {
                let avg_amp_chirho = (sum_chirho / count_chirho as i64) as u32;
                // Map amplitude (0–32767) to frequency (100–2000Hz).
                // Silent (<500 amplitude) → no beep.
                // Low amplitude → low pitch, high amplitude → high pitch.
                if avg_amp_chirho > 500 {
                    let freq_chirho = 100 + (avg_amp_chirho * 1900 / 32767).min(1900);
                    // Duration proportional to buffer size (~1ms per 88 bytes at 44100Hz stereo)
                    let duration_chirho = ((buf_chirho.len() as u32) / 176).max(1).min(50);
                    pc_speaker_beep_chirho(freq_chirho, duration_chirho);
                }
            }
        }
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
        // Mask to 32 bits — syscall args are sign-extended from i32
        let cmd_chirho = cmd_chirho & 0xFFFFFFFF;
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
            // SNDCTL_DSP_RESET (0x5000) — reset device to default state
            0x5000 => Ok(0),
            // SNDCTL_DSP_SYNC (0x5001) — flush buffers
            0x5001 => Ok(0),
            // SNDCTL_DSP_STEREO (0xC0045003) — set mono/stereo
            0xC0045003 => Ok(0),
            // SNDCTL_DSP_GETBLKSIZE (0xC0045004) — get fragment size
            0xC0045004 => {
                if arg_chirho != 0 {
                    unsafe { core::ptr::write_volatile(arg_chirho as *mut i32, 4096); }
                }
                Ok(4096)
            }
            // SNDCTL_DSP_SETFRAGMENT (0xC004500A) — set buffer fragments
            0xC004500A => Ok(0),
            // TIOCGPGRP (0x5413) / TIOCSPGRP (0x5414) — terminal ioctls.
            // /dev/dsp is not a terminal; return ENOTTY.
            TIOCGPGRP_CHIRHO | TIOCSPGRP_CHIRHO => Err(-ENOTTY_CHIRHO),
            // TCGETS (0x5401) — also not a terminal
            0x5401 => Err(-ENOTTY_CHIRHO),
            // SNDCTL_DSP_GETOSPACE (0x800C500C) — get output space
            0x800C500C => {
                if arg_chirho != 0 {
                    // Return audio_buf_info: fragments=8, fragstotal=8, fragsize=4096, bytes=32768
                    unsafe {
                        let p_chirho = arg_chirho as *mut [i32; 4];
                        core::ptr::write_volatile(p_chirho, [8, 8, 4096, 32768]);
                    }
                }
                Ok(0)
            }
            _ => {
                crate::serial_println_chirho!(
                    "SOUND: /dev/dsp unhandled ioctl cmd={:#x} arg={:#x}",
                    cmd_chirho,
                    arg_chirho
                );
                Ok(0) // Return success for unknown ioctls to keep mpg123 happy
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
// PC speaker beep via PIT channel 2 + speaker gate (port 0x61)
// ============================================================================

/// Drive the PC speaker at the given frequency for `duration_ms_chirho` milliseconds.
///
/// Uses PIT channel 2 (ports 0x42/0x43) to generate a square wave and
/// port 0x61 bits 0-1 to gate it to the speaker. A frequency of 0 silences
/// the speaker immediately.
pub fn pc_speaker_beep_chirho(freq_chirho: u32, duration_ms_chirho: u32) {
    use x86_64::instructions::port::Port;

    if freq_chirho == 0 || duration_ms_chirho == 0 {
        // Silence: clear speaker gate bits
        unsafe {
            let mut port61_chirho = Port::<u8>::new(0x61);
            let val_chirho = port61_chirho.read() & 0xFC; // clear bits 0-1
            port61_chirho.write(val_chirho);
        }
        return;
    }

    // PIT oscillator runs at 1,193,182 Hz
    let divisor_chirho: u32 = 1_193_182 / freq_chirho;
    let divisor_chirho = if divisor_chirho > 0xFFFF { 0xFFFF } else if divisor_chirho == 0 { 1 } else { divisor_chirho };

    unsafe {
        // Configure PIT channel 2 for square wave (mode 3), lo/hi byte
        let mut cmd_port_chirho = Port::<u8>::new(0x43);
        cmd_port_chirho.write(0xB6); // channel 2, lo/hi, mode 3, binary

        // Load frequency divisor (lo byte then hi byte)
        let mut ch2_port_chirho = Port::<u8>::new(0x42);
        ch2_port_chirho.write((divisor_chirho & 0xFF) as u8);
        ch2_port_chirho.write(((divisor_chirho >> 8) & 0xFF) as u8);

        // Enable speaker: set bits 0 (gate) and 1 (speaker data)
        let mut port61_chirho = Port::<u8>::new(0x61);
        let val_chirho = port61_chirho.read();
        port61_chirho.write(val_chirho | 0x03);
    }

    // Busy-wait for the requested duration using PIT channel 0 ticks.
    // Each PIT tick is ~838 ns. 1 ms ≈ 1193 ticks.
    // We read PIT ch0 counter to approximate elapsed time.
    // For simplicity, use a loop counter calibrated to ~1ms per iteration
    // on typical QEMU speeds (this is a stub, not precision timing).
    let loops_chirho = duration_ms_chirho * 1000;
    for _ in 0..loops_chirho {
        unsafe { core::arch::asm!("pause", options(nomem, nostack)); }
    }

    // Silence speaker after duration
    unsafe {
        let mut port61_chirho = Port::<u8>::new(0x61);
        let val_chirho = port61_chirho.read() & 0xFC;
        port61_chirho.write(val_chirho);
    }
}

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

// ============================================================================
// Sound Blaster 16 ISA driver (SB16)
// ============================================================================

/// SB16 standard I/O port base.
const SB16_BASE_CHIRHO: u16 = 0x220;
/// SB16 DSP reset port (base + 0x06).
const SB16_RESET_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x06;
/// SB16 DSP read data port (base + 0x0A).
const SB16_READ_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x0A;
/// SB16 DSP write data/command port (base + 0x0C).
const SB16_WRITE_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x0C;
/// SB16 DSP read-buffer status port (base + 0x0E).
const SB16_READ_STATUS_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x0E;
/// SB16 mixer address port (base + 0x04).
const SB16_MIXER_ADDR_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x04;
/// SB16 mixer data port (base + 0x05).
const SB16_MIXER_DATA_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x05;
/// SB16 16-bit IRQ ack port (base + 0x0F).
const SB16_IRQ_ACK_16_CHIRHO: u16 = SB16_BASE_CHIRHO + 0x0F;

/// DMA buffer physical address — must be below 16 MB for ISA DMA.
/// We reserve 64 KB at physical address 0x100000 (1 MB mark).
const SB16_DMA_PHYS_CHIRHO: u32 = 0x0010_0000;
/// DMA buffer size: 32 KB (half-buffer = 16 KB).
const SB16_DMA_SIZE_CHIRHO: usize = 32768;
/// Half-buffer size for double-buffering.
const SB16_HALF_SIZE_CHIRHO: usize = SB16_DMA_SIZE_CHIRHO / 2;

/// Whether SB16 was detected and initialized.
static SB16_DETECTED_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Ring buffer for PCM data from userspace writes.
static SB16_PCM_BUF_CHIRHO: spin::Mutex<PcmRingChirho> =
    spin::Mutex::new(PcmRingChirho::new_const_chirho());

/// Simple ring buffer for PCM data.
struct PcmRingChirho {
    buf_chirho: [u8; 65536],
    head_chirho: usize,
    tail_chirho: usize,
}

impl PcmRingChirho {
    const fn new_const_chirho() -> Self {
        Self {
            buf_chirho: [0u8; 65536],
            head_chirho: 0,
            tail_chirho: 0,
        }
    }

    fn len_chirho(&self) -> usize {
        self.head_chirho.wrapping_sub(self.tail_chirho) % 65536
    }

    fn push_chirho(&mut self, data_chirho: &[u8]) {
        for &b_chirho in data_chirho {
            let next_chirho = (self.head_chirho + 1) % 65536;
            if next_chirho == self.tail_chirho {
                // Buffer full — drop oldest
                self.tail_chirho = (self.tail_chirho + 1) % 65536;
            }
            self.buf_chirho[self.head_chirho] = b_chirho;
            self.head_chirho = next_chirho;
        }
    }

    fn pop_chirho(&mut self, out_chirho: &mut [u8]) -> usize {
        let mut count_chirho = 0;
        for byte_chirho in out_chirho.iter_mut() {
            if self.tail_chirho == self.head_chirho {
                break;
            }
            *byte_chirho = self.buf_chirho[self.tail_chirho];
            self.tail_chirho = (self.tail_chirho + 1) % 65536;
            count_chirho += 1;
        }
        count_chirho
    }
}

/// Write a command byte to the SB16 DSP.
unsafe fn sb16_dsp_write_chirho(val_chirho: u8) {
    use x86_64::instructions::port::Port;
    let mut status_port_chirho = Port::<u8>::new(SB16_WRITE_CHIRHO);
    // Wait for DSP to be ready (bit 7 clear)
    for _ in 0..10000u32 {
        if (status_port_chirho.read() & 0x80) == 0 {
            break;
        }
    }
    status_port_chirho.write(val_chirho);
}

/// Read a byte from the SB16 DSP.
unsafe fn sb16_dsp_read_chirho() -> u8 {
    use x86_64::instructions::port::Port;
    let mut status_port_chirho = Port::<u8>::new(SB16_READ_STATUS_CHIRHO);
    // Wait for data to be available (bit 7 set)
    for _ in 0..10000u32 {
        if (status_port_chirho.read() & 0x80) != 0 {
            break;
        }
    }
    Port::<u8>::new(SB16_READ_CHIRHO).read()
}

/// Probe for SB16 at ISA port 0x220, reset DSP, and configure for playback.
pub fn init_sb16_chirho() {
    use x86_64::instructions::port::Port;

    crate::serial_println_chirho!("[SB16] Probing ISA port {:#x}...", SB16_BASE_CHIRHO);

    // Step 1: Reset DSP
    unsafe {
        let mut reset_port_chirho = Port::<u8>::new(SB16_RESET_CHIRHO);
        reset_port_chirho.write(1);
        // Wait ~3 microseconds (busy loop)
        for _ in 0..1000u32 { core::arch::asm!("pause", options(nomem, nostack)); }
        reset_port_chirho.write(0);
    }

    // Wait for DSP ready (should return 0xAA)
    let mut detected_chirho = false;
    for _ in 0..100u32 {
        let status_chirho = unsafe { Port::<u8>::new(SB16_READ_STATUS_CHIRHO).read() };
        if (status_chirho & 0x80) != 0 {
            let val_chirho = unsafe { Port::<u8>::new(SB16_READ_CHIRHO).read() };
            if val_chirho == 0xAA {
                detected_chirho = true;
                break;
            }
        }
        for _ in 0..100u32 { unsafe { core::arch::asm!("pause", options(nomem, nostack)); } }
    }

    if !detected_chirho {
        crate::serial_println_chirho!("[SB16] DSP not detected at {:#x}", SB16_BASE_CHIRHO);
        return;
    }

    // Step 2: Get DSP version
    unsafe {
        sb16_dsp_write_chirho(0xE1); // Get DSP version
    }
    let major_chirho = unsafe { sb16_dsp_read_chirho() };
    let minor_chirho = unsafe { sb16_dsp_read_chirho() };
    crate::serial_println_chirho!(
        "[SB16] DSP version {}.{} detected at {:#x}",
        major_chirho, minor_chirho, SB16_BASE_CHIRHO,
    );

    // Step 3: Set master volume via mixer
    unsafe {
        Port::<u8>::new(SB16_MIXER_ADDR_CHIRHO).write(0x22); // Master volume
        Port::<u8>::new(SB16_MIXER_DATA_CHIRHO).write(0xCC); // ~80% left+right
        Port::<u8>::new(SB16_MIXER_ADDR_CHIRHO).write(0x04); // Voice (DAC) volume
        Port::<u8>::new(SB16_MIXER_DATA_CHIRHO).write(0xCC); // ~80%
    }

    // Step 4: Enable speaker output
    unsafe { sb16_dsp_write_chirho(0xD1); } // Turn on speaker

    // Step 5: Set sample rate (44100 Hz for output)
    unsafe {
        sb16_dsp_write_chirho(0x41); // Set output sample rate
        sb16_dsp_write_chirho((44100 >> 8) as u8);   // High byte
        sb16_dsp_write_chirho((44100 & 0xFF) as u8);  // Low byte
    }

    // Step 6: Set up ISA DMA channel 5 (16-bit) for auto-init playback
    // DMA channel 5 is the 16-bit counterpart of channel 1
    unsafe {
        // Mask DMA channel 5 (16-bit DMA uses ports 0xD4-0xDB)
        Port::<u8>::new(0xD4).write(0x05); // mask channel 5 (bit 0=channel, bit 2=mask)

        // Clear byte pointer flip-flop
        Port::<u8>::new(0xD8).write(0x00);

        // Set DMA mode: channel 5, auto-init, single mode, read (memory→device)
        // Mode byte: bits 1:0=channel(01), bit 4=auto-init(1), bits 7:6=single(01)
        Port::<u8>::new(0xD6).write(0x59); // channel 1 of 16-bit DMA, auto-init, read

        // Set DMA address (16-bit DMA uses word addresses, divided by 2)
        let word_addr_chirho = (SB16_DMA_PHYS_CHIRHO / 2) as u16;
        Port::<u8>::new(0xC4).write((word_addr_chirho & 0xFF) as u8);     // Low byte
        Port::<u8>::new(0xC4).write(((word_addr_chirho >> 8) & 0xFF) as u8); // High byte

        // Set page register for DMA channel 5 (port 0x8B)
        let page_chirho = ((SB16_DMA_PHYS_CHIRHO >> 16) & 0xFF) as u8;
        Port::<u8>::new(0x8B).write(page_chirho);

        // Set transfer count (in words, minus 1)
        let word_count_chirho = ((SB16_DMA_SIZE_CHIRHO / 2) - 1) as u16;
        Port::<u8>::new(0xC6).write((word_count_chirho & 0xFF) as u8);
        Port::<u8>::new(0xC6).write(((word_count_chirho >> 8) & 0xFF) as u8);

        // Unmask DMA channel 5
        Port::<u8>::new(0xD4).write(0x01); // unmask channel 5
    }

    // Step 7: Fill DMA buffer with silence (signed 16-bit = 0x0000)
    unsafe {
        let dma_ptr_chirho = (SB16_DMA_PHYS_CHIRHO as u64 + 0x18000000000u64) as *mut u8;
        core::ptr::write_bytes(dma_ptr_chirho, 0, SB16_DMA_SIZE_CHIRHO);
    }

    // Step 8: Start 16-bit auto-init DMA playback
    unsafe {
        sb16_dsp_write_chirho(0xB6); // 16-bit output, auto-init, signed stereo
        sb16_dsp_write_chirho(0x30); // signed stereo (bits: 5=signed, 4=stereo)
        // Transfer count per half-buffer (in samples, minus 1)
        let samples_chirho = ((SB16_HALF_SIZE_CHIRHO / 4) - 1) as u16; // /4 for stereo 16-bit
        sb16_dsp_write_chirho((samples_chirho & 0xFF) as u8);
        sb16_dsp_write_chirho(((samples_chirho >> 8) & 0xFF) as u8);
    }

    SB16_DETECTED_CHIRHO.store(true, core::sync::atomic::Ordering::Release);
    crate::serial_println_chirho!(
        "[SB16] Initialized: 44100Hz stereo 16-bit, DMA@{:#x} size={}",
        SB16_DMA_PHYS_CHIRHO, SB16_DMA_SIZE_CHIRHO,
    );
}

/// Returns true if SB16 was detected.
pub fn sb16_detected_chirho() -> bool {
    SB16_DETECTED_CHIRHO.load(core::sync::atomic::Ordering::Acquire)
}

/// Queue PCM data from userspace for SB16 playback.
/// Called from /dev/dsp write handler.
pub fn sb16_write_pcm_chirho(data_chirho: &[u8]) {
    if !sb16_detected_chirho() {
        return;
    }

    // Push data into the ring buffer
    let mut ring_chirho = SB16_PCM_BUF_CHIRHO.lock();
    ring_chirho.push_chirho(data_chirho);

    // Copy available data into the DMA buffer (simple memcpy approach).
    // In a proper driver we'd use double-buffering with IRQ-driven refill,
    // but for QEMU this direct approach produces audible output.
    let mut tmp_chirho = [0u8; SB16_DMA_SIZE_CHIRHO];
    let filled_chirho = ring_chirho.pop_chirho(&mut tmp_chirho);
    if filled_chirho > 0 {
        unsafe {
            let dma_ptr_chirho = (SB16_DMA_PHYS_CHIRHO as u64 + 0x18000000000u64) as *mut u8;
            core::ptr::copy_nonoverlapping(
                tmp_chirho.as_ptr(),
                dma_ptr_chirho,
                filled_chirho,
            );
        }
    }
}

/// Handle SB16 IRQ (IRQ 5). Acknowledge the interrupt and refill DMA buffer.
pub fn sb16_irq_handler_chirho() {
    if !sb16_detected_chirho() {
        return;
    }

    // Acknowledge 16-bit DSP interrupt
    unsafe {
        use x86_64::instructions::port::Port;
        let _ = Port::<u8>::new(SB16_IRQ_ACK_16_CHIRHO).read();
    }

    // Refill DMA buffer from ring
    let mut ring_chirho = SB16_PCM_BUF_CHIRHO.lock();
    let mut tmp_chirho = [0u8; SB16_HALF_SIZE_CHIRHO];
    let filled_chirho = ring_chirho.pop_chirho(&mut tmp_chirho);
    if filled_chirho > 0 {
        unsafe {
            let dma_ptr_chirho = (SB16_DMA_PHYS_CHIRHO as u64 + 0x18000000000u64) as *mut u8;
            core::ptr::copy_nonoverlapping(
                tmp_chirho.as_ptr(),
                dma_ptr_chirho,
                filled_chirho,
            );
        }
    }
}
