// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Intel High Definition Audio controller support for Lineluya.
//!
//! This driver targets the QEMU `intel-hda` controller and implements:
//! - PCI probe and MMIO controller bring-up
//! - CORB/RIRB command-ring allocation and initialization
//! - Codec probing through the HDA immediate command interface
//! - Playback route discovery from an output pin back to an audio-out converter
//! - A single BDL-backed output DMA stream
//! - `/dev/dsp` OSS writes that copy PCM into the BDL pages and start playback

use alloc::vec::Vec;
use core::cmp::min;
use core::ptr::{copy_nonoverlapping, read_volatile, write_bytes, write_volatile};

use crate::pci_chirho::{pci_assign_bar_chirho, scan_bus_chirho, PciDeviceChirho};
use crate::syscall_chirho::{EINVAL_CHIRHO, EIO_CHIRHO, ENODEV_CHIRHO, ENOSYS_CHIRHO};
use crate::vfs_chirho::{FileChirho, FileOpsChirho};
use x86_64::structures::paging::{FrameAllocator, Size4KiB};

// ============================================================================
// PCI IDs and OSS ioctl constants
// ============================================================================

const INTEL_VENDOR_ID_CHIRHO: u16 = 0x8086;
const INTEL_HDA_DEVICE_ID_CHIRHO: u16 = 0x2668;
const PCI_CLASS_MULTIMEDIA_CHIRHO: u8 = 0x04;
const PCI_SUBCLASS_HDA_CHIRHO: u8 = 0x03;

const SNDCTL_DSP_RESET_CHIRHO: u64 = 0x0000_5000;
const SNDCTL_DSP_SPEED_CHIRHO: u64 = 0xC004_5002;
const SNDCTL_DSP_SETFMT_CHIRHO: u64 = 0xC004_5005;
const SNDCTL_DSP_CHANNELS_CHIRHO: u64 = 0xC004_5006;
const SNDCTL_DSP_GETFMTS_CHIRHO: u64 = 0x8004_500B;
const SNDCTL_DSP_GETBLKSIZE_CHIRHO: u64 = 0xC004_5004;
const SNDCTL_DSP_GETCAPS_CHIRHO: u64 = 0x8004_500F;
const SNDCTL_DSP_SYNC_CHIRHO: u64 = 0x0000_5001;

const AFMT_S16_LE_CHIRHO: i64 = 0x10;
const DSP_CAP_REALTIME_CHIRHO: i64 = 0x0000_0200;
const DSP_CAP_BATCH_CHIRHO: i64 = 0x0000_0040;

// ============================================================================
// Controller, codec, and DMA constants
// ============================================================================

const DEFAULT_SAMPLE_RATE_CHIRHO: u32 = 44_100;
const DEFAULT_CHANNELS_CHIRHO: u8 = 2;
const DEFAULT_SAMPLE_BITS_CHIRHO: u8 = 16;
const STREAM_TAG_CHIRHO: u8 = 1;

const HDA_MAX_CODECS_CHIRHO: u8 = 15;
const HDA_MAX_CONNECTIONS_CHIRHO: usize = 32;
const HDA_MAX_PATH_DEPTH_CHIRHO: usize = 16;
const HDA_MAX_CODEC_TREE_DEPTH_CHIRHO: usize = 8;
const HDA_BUFFER_PAGE_BYTES_CHIRHO: usize = 4096;
const HDA_BDL_ENTRY_COUNT_CHIRHO: usize = 4;
const HDA_BDL_BUFFER_BYTES_CHIRHO: usize =
    HDA_BUFFER_PAGE_BYTES_CHIRHO * HDA_BDL_ENTRY_COUNT_CHIRHO;
const HDA_RING_ENTRIES_CHIRHO: u16 = 256;
const HDA_RING_SIZE_SELECT_256_CHIRHO: u8 = 0x02;
const HDA_VERB_TIMEOUT_LOOPS_CHIRHO: usize = 250_000;
const HDA_MMIO_TIMEOUT_LOOPS_CHIRHO: usize = 250_000;
const HDA_PAUSE_GRANULARITY_CHIRHO: u32 = 64;
const HDA_RINTCNT_CHIRHO: u16 = 1;

// ============================================================================
// HDA register offsets and bits
// ============================================================================

const AZX_REG_GCAP_CHIRHO: u64 = 0x00;
const AZX_GCAP_ISS_CHIRHO: u16 = 15 << 8;
const AZX_GCAP_OSS_CHIRHO: u16 = 15 << 12;
const AZX_REG_GCTL_CHIRHO: u64 = 0x08;
const AZX_GCTL_RESET_CHIRHO: u32 = 1 << 0;
const AZX_REG_STATESTS_CHIRHO: u64 = 0x0E;
const AZX_REG_INTCTL_CHIRHO: u64 = 0x20;
const AZX_REG_WALLCLK_CHIRHO: u64 = 0x30;
const AZX_REG_CORBLBASE_CHIRHO: u64 = 0x40;
const AZX_REG_CORBUBASE_CHIRHO: u64 = 0x44;
const AZX_REG_CORBWP_CHIRHO: u64 = 0x48;
const AZX_REG_CORBRP_CHIRHO: u64 = 0x4A;
const AZX_CORBRP_RST_CHIRHO: u16 = 1 << 15;
const AZX_REG_CORBCTL_CHIRHO: u64 = 0x4C;
const AZX_CORBCTL_RUN_CHIRHO: u8 = 1 << 1;
const AZX_REG_CORBSTS_CHIRHO: u64 = 0x4D;
const AZX_CORBSTS_CMEI_CHIRHO: u8 = 1 << 0;
const AZX_REG_CORBSIZE_CHIRHO: u64 = 0x4E;
const AZX_REG_RIRBLBASE_CHIRHO: u64 = 0x50;
const AZX_REG_RIRBUBASE_CHIRHO: u64 = 0x54;
const AZX_REG_RIRBWP_CHIRHO: u64 = 0x58;
const AZX_RIRBWP_RST_CHIRHO: u16 = 1 << 15;
const AZX_REG_RINTCNT_CHIRHO: u64 = 0x5A;
const AZX_REG_RIRBCTL_CHIRHO: u64 = 0x5C;
const AZX_RBCTL_DMA_EN_CHIRHO: u8 = 1 << 1;
const AZX_REG_RIRBSTS_CHIRHO: u64 = 0x5D;
const AZX_RBSTS_IRQ_CHIRHO: u8 = 1 << 0;
const AZX_RBSTS_OVERRUN_CHIRHO: u8 = 1 << 2;
const AZX_REG_RIRBSIZE_CHIRHO: u64 = 0x5E;
const AZX_REG_IC_CHIRHO: u64 = 0x60;
const AZX_REG_IR_CHIRHO: u64 = 0x64;
const AZX_REG_IRS_CHIRHO: u64 = 0x68;
const AZX_IRS_VALID_CHIRHO: u16 = 1 << 1;
const AZX_IRS_BUSY_CHIRHO: u16 = 1 << 0;

const AZX_STREAM_BASE_CHIRHO: u64 = 0x80;
const AZX_STREAM_STRIDE_CHIRHO: u64 = 0x20;
const AZX_REG_SD_CTL_CHIRHO: u64 = 0x00;
const AZX_REG_SD_CTL_3B_CHIRHO: u64 = 0x02;
const AZX_REG_SD_STS_CHIRHO: u64 = 0x03;
const AZX_REG_SD_LPIB_CHIRHO: u64 = 0x04;
const AZX_REG_SD_CBL_CHIRHO: u64 = 0x08;
const AZX_REG_SD_LVI_CHIRHO: u64 = 0x0C;
const AZX_REG_SD_FORMAT_CHIRHO: u64 = 0x12;
const AZX_REG_SD_BDLPL_CHIRHO: u64 = 0x18;
const AZX_REG_SD_BDLPU_CHIRHO: u64 = 0x1C;
const SD_CTL_STREAM_RESET_CHIRHO: u16 = 0x01;
const SD_CTL_DMA_START_CHIRHO: u16 = 0x02;
const SD_INT_DESC_ERR_CHIRHO: u8 = 0x10;
const SD_INT_FIFO_ERR_CHIRHO: u8 = 0x08;
const SD_INT_COMPLETE_CHIRHO: u8 = 0x04;
const SD_INT_MASK_CHIRHO: u8 =
    SD_INT_DESC_ERR_CHIRHO | SD_INT_FIFO_ERR_CHIRHO | SD_INT_COMPLETE_CHIRHO;
const SD_STS_FIFO_READY_CHIRHO: u8 = 0x20;

// ============================================================================
// HDA verb, parameter, and widget constants
// ============================================================================

const AC_WID_AUD_OUT_CHIRHO: u8 = 0x00;
const AC_WID_AUD_IN_CHIRHO: u8 = 0x01;
const AC_WID_AUD_MIX_CHIRHO: u8 = 0x02;
const AC_WID_AUD_SEL_CHIRHO: u8 = 0x03;
const AC_WID_PIN_CHIRHO: u8 = 0x04;
const AC_WID_VOL_KNB_CHIRHO: u8 = 0x06;

const AC_VERB_PARAMETERS_CHIRHO: u16 = 0x0F00;
const AC_VERB_GET_CONNECT_LIST_CHIRHO: u16 = 0x0F02;
const AC_VERB_GET_CONFIG_DEFAULT_CHIRHO: u16 = 0x0F1C;
const AC_VERB_SET_STREAM_FORMAT_CHIRHO: u16 = 0x0200;
const AC_VERB_SET_CONNECT_SEL_CHIRHO: u16 = 0x0701;
const AC_VERB_SET_POWER_STATE_CHIRHO: u16 = 0x0705;
const AC_VERB_SET_CHANNEL_STREAMID_CHIRHO: u16 = 0x0706;
const AC_VERB_SET_PIN_WIDGET_CONTROL_CHIRHO: u16 = 0x0707;
const AC_VERB_SET_EAPD_BTLENABLE_CHIRHO: u16 = 0x070C;
const AC_VERB_SET_CVT_CHAN_COUNT_CHIRHO: u16 = 0x072D;
const AC_VERB_SET_CODEC_RESET_CHIRHO: u16 = 0x07FF;

const AC_PAR_NODE_COUNT_CHIRHO: u16 = 0x04;
const AC_PAR_FUNCTION_TYPE_CHIRHO: u16 = 0x05;
const AC_PAR_AUDIO_WIDGET_CAP_CHIRHO: u16 = 0x09;
const AC_PAR_PCM_CHIRHO: u16 = 0x0A;
const AC_PAR_STREAM_CHIRHO: u16 = 0x0B;
const AC_PAR_PIN_CAP_CHIRHO: u16 = 0x0C;
const AC_PAR_CONNLIST_LEN_CHIRHO: u16 = 0x0E;

const AC_FGT_TYPE_CHIRHO: u32 = 0xFF;
const AC_GRP_AUDIO_FUNCTION_CHIRHO: u32 = 0x01;
const AC_NODE_COUNT_START_SHIFT_CHIRHO: u32 = 16;
const AC_NODE_COUNT_MASK_CHIRHO: u32 = 0x7FFF;

const AC_WCAP_FORMAT_OVRD_CHIRHO: u32 = 1 << 4;
const AC_WCAP_CONN_LIST_CHIRHO: u32 = 1 << 8;
const AC_WCAP_DIGITAL_CHIRHO: u32 = 1 << 9;
const AC_WCAP_TYPE_CHIRHO: u32 = 0x0F << 20;
const AC_WCAP_TYPE_SHIFT_CHIRHO: u32 = 20;
const AC_WCAP_STEREO_CHIRHO: u32 = 1 << 0;

const AC_SUPPCM_RATES_CHIRHO: u32 = 0x0FFF;
const AC_SUPPCM_BITS_16_CHIRHO: u32 = 1 << 17;
const AC_SUPFMT_PCM_CHIRHO: u32 = 1 << 0;

const AC_CLIST_LENGTH_CHIRHO: u32 = 0x7F;
const AC_CLIST_LONG_CHIRHO: u32 = 1 << 7;

const AC_PINCAP_OUT_CHIRHO: u32 = 1 << 4;
const AC_PINCAP_HDMI_CHIRHO: u32 = 1 << 7;
const AC_PINCAP_EAPD_CHIRHO: u32 = 1 << 16;
const AC_PINCAP_DP_CHIRHO: u32 = 1 << 24;
const AC_PINCAP_HP_DRV_CHIRHO: u32 = 1 << 3;

const AC_PINCTL_OUT_EN_CHIRHO: u8 = 1 << 6;
const AC_PINCTL_HP_EN_CHIRHO: u8 = 1 << 7;
const AC_EAPDBTL_EAPD_CHIRHO: u32 = 1 << 1;

const AC_DEFCFG_DEVICE_SHIFT_CHIRHO: u32 = 20;
const AC_DEFCFG_PORT_CONN_SHIFT_CHIRHO: u32 = 30;
const AC_JACK_LINE_OUT_CHIRHO: u32 = 0x0;
const AC_JACK_SPEAKER_CHIRHO: u32 = 0x1;
const AC_JACK_HP_OUT_CHIRHO: u32 = 0x2;
const AC_JACK_OTHER_CHIRHO: u32 = 0xF;
const AC_JACK_PORT_NONE_CHIRHO: u32 = 0x1;

const AC_FMT_BITS_16_CHIRHO: u16 = 1 << 4;
const AC_FMT_BASE_44K_CHIRHO: u16 = 1 << 14;
const AC_FMT_BASE_48K_CHIRHO: u16 = 0 << 14;

// ============================================================================
// DMA structures and playback metadata
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
struct BdlEntryChirho {
    address_low_chirho: u32,
    address_high_chirho: u32,
    length_chirho: u32,
    flags_chirho: u32,
}

#[derive(Clone, Copy)]
struct PathSelectChirho {
    node_nid_chirho: u8,
    connection_index_chirho: u8,
}

#[derive(Clone)]
struct CodecRouteChirho {
    codec_address_chirho: u8,
    function_group_nid_chirho: u8,
    pin_nid_chirho: u8,
    converter_nid_chirho: u8,
    score_chirho: u32,
    path_nodes_chirho: Vec<u8>,
    path_selects_chirho: Vec<PathSelectChirho>,
}

#[derive(Clone, Copy)]
struct RateEntryChirho {
    sample_rate_hz_chirho: u32,
    pcm_rate_bit_chirho: u32,
    stream_format_bits_chirho: u16,
}

const RATE_TABLE_CHIRHO: &[RateEntryChirho] = &[
    RateEntryChirho {
        sample_rate_hz_chirho: 8_000,
        pcm_rate_bit_chirho: 1 << 0,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO | (5 << 8),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 11_025,
        pcm_rate_bit_chirho: 1 << 1,
        stream_format_bits_chirho: AC_FMT_BASE_44K_CHIRHO | (3 << 8),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 16_000,
        pcm_rate_bit_chirho: 1 << 2,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO | (2 << 8),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 22_050,
        pcm_rate_bit_chirho: 1 << 3,
        stream_format_bits_chirho: AC_FMT_BASE_44K_CHIRHO | (1 << 8),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 32_000,
        pcm_rate_bit_chirho: 1 << 4,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO | (1 << 11) | (2 << 8),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 44_100,
        pcm_rate_bit_chirho: 1 << 5,
        stream_format_bits_chirho: AC_FMT_BASE_44K_CHIRHO,
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 48_000,
        pcm_rate_bit_chirho: 1 << 6,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO,
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 88_200,
        pcm_rate_bit_chirho: 1 << 7,
        stream_format_bits_chirho: AC_FMT_BASE_44K_CHIRHO | (1 << 11),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 96_000,
        pcm_rate_bit_chirho: 1 << 8,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO | (1 << 11),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 176_400,
        pcm_rate_bit_chirho: 1 << 9,
        stream_format_bits_chirho: AC_FMT_BASE_44K_CHIRHO | (3 << 11),
    },
    RateEntryChirho {
        sample_rate_hz_chirho: 192_000,
        pcm_rate_bit_chirho: 1 << 10,
        stream_format_bits_chirho: AC_FMT_BASE_48K_CHIRHO | (3 << 11),
    },
];

struct HdaControllerChirho {
    mmio_phys_base_chirho: u64,
    mmio_virt_base_chirho: u64,
    mmio_size_chirho: u64,
    output_stream_base_chirho: u64,
    output_stream_index_chirho: u8,
    bus_chirho: u8,
    device_chirho: u8,
    function_chirho: u8,
    codec_route_chirho: CodecRouteChirho,
    corb_phys_chirho: u64,
    corb_virt_chirho: u64,
    rirb_phys_chirho: u64,
    rirb_virt_chirho: u64,
    bdl_phys_chirho: u64,
    bdl_virt_chirho: u64,
    pcm_page_phys_addrs_chirho: [u64; HDA_BDL_ENTRY_COUNT_CHIRHO],
    pcm_page_virt_addrs_chirho: [u64; HDA_BDL_ENTRY_COUNT_CHIRHO],
    sample_rate_chirho: u32,
    channels_chirho: u8,
    sample_bits_chirho: u8,
    stream_format_chirho: u16,
}

static HDA_CONTROLLER_CHIRHO: spin::Mutex<Option<HdaControllerChirho>> = spin::Mutex::new(None);

// ============================================================================
// MMIO and low-level helpers
// ============================================================================

#[inline(always)]
fn pause_short_chirho(iterations_chirho: u32) {
    for _ in 0..iterations_chirho {
        unsafe {
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

#[inline(always)]
fn mmio_read_u8_chirho(controller_chirho: &HdaControllerChirho, offset_chirho: u64) -> u8 {
    unsafe { read_volatile((controller_chirho.mmio_virt_base_chirho + offset_chirho) as *const u8) }
}

#[inline(always)]
fn mmio_read_u16_chirho(controller_chirho: &HdaControllerChirho, offset_chirho: u64) -> u16 {
    unsafe { read_volatile((controller_chirho.mmio_virt_base_chirho + offset_chirho) as *const u16) }
}

#[inline(always)]
fn mmio_read_u32_chirho(controller_chirho: &HdaControllerChirho, offset_chirho: u64) -> u32 {
    unsafe { read_volatile((controller_chirho.mmio_virt_base_chirho + offset_chirho) as *const u32) }
}

#[inline(always)]
fn mmio_write_u8_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    value_chirho: u8,
) {
    unsafe {
        write_volatile(
            (controller_chirho.mmio_virt_base_chirho + offset_chirho) as *mut u8,
            value_chirho,
        );
    }
}

#[inline(always)]
fn mmio_write_u16_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    value_chirho: u16,
) {
    unsafe {
        write_volatile(
            (controller_chirho.mmio_virt_base_chirho + offset_chirho) as *mut u16,
            value_chirho,
        );
    }
}

#[inline(always)]
fn mmio_write_u32_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    value_chirho: u32,
) {
    unsafe {
        write_volatile(
            (controller_chirho.mmio_virt_base_chirho + offset_chirho) as *mut u32,
            value_chirho,
        );
    }
}

fn wait_for_u32_mask_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    mask_chirho: u32,
    expected_chirho: u32,
    loop_budget_chirho: usize,
) -> bool {
    for _ in 0..loop_budget_chirho {
        if mmio_read_u32_chirho(controller_chirho, offset_chirho) & mask_chirho == expected_chirho {
            return true;
        }
        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }
    false
}

fn wait_for_u16_mask_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    mask_chirho: u16,
    expected_chirho: u16,
    loop_budget_chirho: usize,
) -> bool {
    for _ in 0..loop_budget_chirho {
        if mmio_read_u16_chirho(controller_chirho, offset_chirho) & mask_chirho == expected_chirho {
            return true;
        }
        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }
    false
}

fn wait_for_u8_mask_chirho(
    controller_chirho: &HdaControllerChirho,
    offset_chirho: u64,
    mask_chirho: u8,
    expected_chirho: u8,
    loop_budget_chirho: usize,
) -> bool {
    for _ in 0..loop_budget_chirho {
        if mmio_read_u8_chirho(controller_chirho, offset_chirho) & mask_chirho == expected_chirho {
            return true;
        }
        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }
    false
}

fn allocate_zeroed_dma_frame_chirho() -> Result<(u64, u64), i64> {
    let frame_chirho = {
        let mut alloc_guard_chirho = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();
        let Some(frame_allocator_chirho) = alloc_guard_chirho.as_mut() else {
            return Err(-ENODEV_CHIRHO);
        };
        frame_allocator_chirho
            .allocate_frame()
            .ok_or(-ENODEV_CHIRHO)?
    };

    let phys_addr_chirho = frame_chirho.start_address().as_u64();
    let virt_addr_chirho = phys_addr_chirho + crate::pagetable_chirho::phys_mem_offset_chirho();
    unsafe {
        write_bytes(
            virt_addr_chirho as *mut u8,
            0,
            HDA_BUFFER_PAGE_BYTES_CHIRHO,
        );
    }
    Ok((phys_addr_chirho, virt_addr_chirho))
}

fn compose_verb_chirho(
    codec_address_chirho: u8,
    node_nid_chirho: u8,
    verb_chirho: u16,
    payload_chirho: u16,
) -> u32 {
    ((codec_address_chirho as u32) << 28)
        | ((node_nid_chirho as u32) << 20)
        | ((verb_chirho as u32) << 8)
        | payload_chirho as u32
}

fn send_immediate_verb_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    node_nid_chirho: u8,
    verb_chirho: u16,
    payload_chirho: u16,
) -> Result<u32, i64> {
    let command_word_chirho =
        compose_verb_chirho(codec_address_chirho, node_nid_chirho, verb_chirho, payload_chirho);

    for _ in 0..HDA_VERB_TIMEOUT_LOOPS_CHIRHO {
        if mmio_read_u16_chirho(controller_chirho, AZX_REG_IRS_CHIRHO) & AZX_IRS_BUSY_CHIRHO == 0 {
            break;
        }
        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }

    mmio_write_u16_chirho(controller_chirho, AZX_REG_IRS_CHIRHO, 0);
    mmio_write_u32_chirho(controller_chirho, AZX_REG_IC_CHIRHO, command_word_chirho);
    mmio_write_u16_chirho(controller_chirho, AZX_REG_IRS_CHIRHO, AZX_IRS_BUSY_CHIRHO);

    for _ in 0..HDA_VERB_TIMEOUT_LOOPS_CHIRHO {
        let status_chirho = mmio_read_u16_chirho(controller_chirho, AZX_REG_IRS_CHIRHO);
        if status_chirho & AZX_IRS_BUSY_CHIRHO == 0
            && status_chirho & AZX_IRS_VALID_CHIRHO != 0
        {
            return Ok(mmio_read_u32_chirho(controller_chirho, AZX_REG_IR_CHIRHO));
        }
        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }

    crate::serial_println_chirho!(
        "HDA: immediate verb timeout codec={} nid={:#x} verb={:#x} payload={:#x}",
        codec_address_chirho,
        node_nid_chirho,
        verb_chirho,
        payload_chirho
    );
    Err(-EIO_CHIRHO)
}

fn get_parameter_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    node_nid_chirho: u8,
    parameter_id_chirho: u16,
) -> Result<u32, i64> {
    send_immediate_verb_chirho(
        controller_chirho,
        codec_address_chirho,
        node_nid_chirho,
        AC_VERB_PARAMETERS_CHIRHO,
        parameter_id_chirho,
    )
}

fn get_widget_caps_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    node_nid_chirho: u8,
) -> Result<u32, i64> {
    get_parameter_chirho(
        controller_chirho,
        codec_address_chirho,
        node_nid_chirho,
        AC_PAR_AUDIO_WIDGET_CAP_CHIRHO,
    )
}

fn get_subnodes_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    node_nid_chirho: u8,
) -> Result<(u8, u8), i64> {
    let node_count_value_chirho =
        get_parameter_chirho(controller_chirho, codec_address_chirho, node_nid_chirho, AC_PAR_NODE_COUNT_CHIRHO)?;
    let start_nid_chirho =
        ((node_count_value_chirho >> AC_NODE_COUNT_START_SHIFT_CHIRHO) & AC_NODE_COUNT_MASK_CHIRHO) as u8;
    let node_count_chirho = (node_count_value_chirho & AC_NODE_COUNT_MASK_CHIRHO) as u8;
    Ok((start_nid_chirho, node_count_chirho))
}

fn widget_type_chirho(widget_caps_chirho: u32) -> u8 {
    ((widget_caps_chirho & AC_WCAP_TYPE_CHIRHO) >> AC_WCAP_TYPE_SHIFT_CHIRHO) as u8
}

fn get_connections_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    node_nid_chirho: u8,
) -> Result<Vec<u8>, i64> {
    let widget_caps_chirho =
        get_widget_caps_chirho(controller_chirho, codec_address_chirho, node_nid_chirho)?;
    if widget_type_chirho(widget_caps_chirho) == AC_WID_VOL_KNB_CHIRHO {
        return Ok(Vec::new());
    }
    if widget_caps_chirho & AC_WCAP_CONN_LIST_CHIRHO == 0 {
        return Ok(Vec::new());
    }

    let conn_len_raw_chirho =
        get_parameter_chirho(controller_chirho, codec_address_chirho, node_nid_chirho, AC_PAR_CONNLIST_LEN_CHIRHO)?;
    let is_long_form_chirho = conn_len_raw_chirho & AC_CLIST_LONG_CHIRHO != 0;
    let conn_declared_count_chirho = (conn_len_raw_chirho & AC_CLIST_LENGTH_CHIRHO) as usize;
    if conn_declared_count_chirho == 0 {
        return Ok(Vec::new());
    }

    let element_shift_chirho = if is_long_form_chirho { 16 } else { 8 };
    let elements_per_verb_chirho = if is_long_form_chirho { 2 } else { 4 };
    let range_flag_mask_chirho = 1u32 << (element_shift_chirho - 1);
    let element_mask_chirho = (1u32 << (element_shift_chirho - 1)) - 1;

    let mut connections_chirho = Vec::new();
    let mut previous_nid_chirho: u8 = 0;

    for connection_index_chirho in 0..conn_declared_count_chirho {
        let mut verb_result_chirho = 0u32;
        if connection_index_chirho % elements_per_verb_chirho == 0 {
            verb_result_chirho = send_immediate_verb_chirho(
                controller_chirho,
                codec_address_chirho,
                node_nid_chirho,
                AC_VERB_GET_CONNECT_LIST_CHIRHO,
                connection_index_chirho as u16,
            )?;
        } else {
            verb_result_chirho = send_immediate_verb_chirho(
                controller_chirho,
                codec_address_chirho,
                node_nid_chirho,
                AC_VERB_GET_CONNECT_LIST_CHIRHO,
                (connection_index_chirho - (connection_index_chirho % elements_per_verb_chirho)) as u16,
            )?;
            for _ in 0..(connection_index_chirho % elements_per_verb_chirho) {
                verb_result_chirho >>= element_shift_chirho;
            }
        }

        let has_range_chirho = verb_result_chirho & range_flag_mask_chirho != 0;
        let connection_nid_chirho = (verb_result_chirho & element_mask_chirho) as u8;
        if connection_nid_chirho == 0 {
            continue;
        }

        if has_range_chirho && previous_nid_chirho != 0 && previous_nid_chirho < connection_nid_chirho {
            for expanded_nid_chirho in (previous_nid_chirho + 1)..=connection_nid_chirho {
                if connections_chirho.len() >= HDA_MAX_CONNECTIONS_CHIRHO {
                    break;
                }
                connections_chirho.push(expanded_nid_chirho);
            }
        } else if connections_chirho.len() < HDA_MAX_CONNECTIONS_CHIRHO {
            connections_chirho.push(connection_nid_chirho);
        }

        previous_nid_chirho = connection_nid_chirho;
    }

    Ok(connections_chirho)
}

fn is_selector_like_widget_chirho(widget_type_chirho: u8) -> bool {
    matches!(widget_type_chirho, AC_WID_PIN_CHIRHO | AC_WID_AUD_SEL_CHIRHO)
}

fn score_pin_widget_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    pin_nid_chirho: u8,
) -> Result<Option<u32>, i64> {
    let widget_caps_chirho = get_widget_caps_chirho(controller_chirho, codec_address_chirho, pin_nid_chirho)?;
    if widget_type_chirho(widget_caps_chirho) != AC_WID_PIN_CHIRHO {
        return Ok(None);
    }
    if widget_caps_chirho & AC_WCAP_DIGITAL_CHIRHO != 0 {
        return Ok(None);
    }

    let pin_caps_chirho =
        get_parameter_chirho(controller_chirho, codec_address_chirho, pin_nid_chirho, AC_PAR_PIN_CAP_CHIRHO)?;
    if pin_caps_chirho & AC_PINCAP_OUT_CHIRHO == 0 {
        return Ok(None);
    }
    if pin_caps_chirho & (AC_PINCAP_HDMI_CHIRHO | AC_PINCAP_DP_CHIRHO) != 0 {
        return Ok(None);
    }

    let default_config_chirho = send_immediate_verb_chirho(
        controller_chirho,
        codec_address_chirho,
        pin_nid_chirho,
        AC_VERB_GET_CONFIG_DEFAULT_CHIRHO,
        0,
    )?;
    let port_connection_chirho = (default_config_chirho >> AC_DEFCFG_PORT_CONN_SHIFT_CHIRHO) & 0x3;
    if port_connection_chirho == AC_JACK_PORT_NONE_CHIRHO {
        return Ok(None);
    }

    let device_type_chirho = (default_config_chirho >> AC_DEFCFG_DEVICE_SHIFT_CHIRHO) & 0xF;
    let mut score_chirho = 100u32;
    score_chirho += match device_type_chirho {
        AC_JACK_LINE_OUT_CHIRHO => 400,
        AC_JACK_SPEAKER_CHIRHO => 320,
        AC_JACK_HP_OUT_CHIRHO => 280,
        AC_JACK_OTHER_CHIRHO => 40,
        _ => 120,
    };
    if pin_caps_chirho & AC_PINCAP_EAPD_CHIRHO != 0 {
        score_chirho += 10;
    }
    Ok(Some(score_chirho))
}

fn find_route_from_node_chirho(
    controller_chirho: &mut HdaControllerChirho,
    codec_address_chirho: u8,
    current_nid_chirho: u8,
    visited_nodes_chirho: &mut Vec<u8>,
    path_nodes_chirho: &mut Vec<u8>,
    path_selects_chirho: &mut Vec<PathSelectChirho>,
    recursion_depth_chirho: usize,
) -> Result<bool, i64> {
    if recursion_depth_chirho >= HDA_MAX_CODEC_TREE_DEPTH_CHIRHO {
        return Ok(false);
    }
    if visited_nodes_chirho.contains(&current_nid_chirho) {
        return Ok(false);
    }
    if path_nodes_chirho.len() >= HDA_MAX_PATH_DEPTH_CHIRHO {
        return Ok(false);
    }

    visited_nodes_chirho.push(current_nid_chirho);
    path_nodes_chirho.push(current_nid_chirho);

    let widget_caps_chirho =
        get_widget_caps_chirho(controller_chirho, codec_address_chirho, current_nid_chirho)?;
    let widget_type_chirho = widget_type_chirho(widget_caps_chirho);
    if widget_type_chirho == AC_WID_AUD_OUT_CHIRHO {
        return Ok(true);
    }
    if widget_type_chirho == AC_WID_AUD_IN_CHIRHO {
        path_nodes_chirho.pop();
        visited_nodes_chirho.pop();
        return Ok(false);
    }

    let connections_chirho =
        get_connections_chirho(controller_chirho, codec_address_chirho, current_nid_chirho)?;
    let should_track_select_chirho =
        is_selector_like_widget_chirho(widget_type_chirho) && connections_chirho.len() > 1;

    for (connection_index_chirho, upstream_nid_chirho) in connections_chirho.iter().enumerate() {
        if should_track_select_chirho {
            path_selects_chirho.push(PathSelectChirho {
                node_nid_chirho: current_nid_chirho,
                connection_index_chirho: connection_index_chirho as u8,
            });
        }

        if find_route_from_node_chirho(
            controller_chirho,
            codec_address_chirho,
            *upstream_nid_chirho,
            visited_nodes_chirho,
            path_nodes_chirho,
            path_selects_chirho,
            recursion_depth_chirho + 1,
        )? {
            return Ok(true);
        }

        if should_track_select_chirho {
            path_selects_chirho.pop();
        }
    }

    path_nodes_chirho.pop();
    visited_nodes_chirho.pop();
    Ok(false)
}

fn find_best_codec_route_chirho(
    controller_chirho: &mut HdaControllerChirho,
    state_status_chirho: u16,
) -> Result<CodecRouteChirho, i64> {
    let mut best_route_chirho: Option<CodecRouteChirho> = None;

    for codec_address_chirho in 0..HDA_MAX_CODECS_CHIRHO {
        if state_status_chirho != 0 && (state_status_chirho & (1 << codec_address_chirho)) == 0 {
            continue;
        }

        let Ok((root_start_nid_chirho, root_node_count_chirho)) =
            get_subnodes_chirho(controller_chirho, codec_address_chirho, 0)
        else {
            continue;
        };

        for function_group_offset_chirho in 0..root_node_count_chirho {
            let function_group_nid_chirho = root_start_nid_chirho.wrapping_add(function_group_offset_chirho);
            let function_group_type_chirho = match get_parameter_chirho(
                controller_chirho,
                codec_address_chirho,
                function_group_nid_chirho,
                AC_PAR_FUNCTION_TYPE_CHIRHO,
            ) {
                Ok(value_chirho) => value_chirho,
                Err(_) => continue,
            };
            if function_group_type_chirho & AC_FGT_TYPE_CHIRHO != AC_GRP_AUDIO_FUNCTION_CHIRHO {
                continue;
            }

            let (widget_start_nid_chirho, widget_count_chirho) = match get_subnodes_chirho(
                controller_chirho,
                codec_address_chirho,
                function_group_nid_chirho,
            ) {
                Ok(result_chirho) => result_chirho,
                Err(_) => continue,
            };

            for widget_offset_chirho in 0..widget_count_chirho {
                let pin_nid_chirho = widget_start_nid_chirho.wrapping_add(widget_offset_chirho);
                let Some(score_chirho) =
                    score_pin_widget_chirho(controller_chirho, codec_address_chirho, pin_nid_chirho)?
                else {
                    continue;
                };

                let mut visited_nodes_chirho = Vec::new();
                let mut path_nodes_chirho = Vec::new();
                let mut path_selects_chirho = Vec::new();

                if !find_route_from_node_chirho(
                    controller_chirho,
                    codec_address_chirho,
                    pin_nid_chirho,
                    &mut visited_nodes_chirho,
                    &mut path_nodes_chirho,
                    &mut path_selects_chirho,
                    0,
                )? {
                    continue;
                }

                let Some(&converter_nid_chirho) = path_nodes_chirho.last() else {
                    continue;
                };
                let route_chirho = CodecRouteChirho {
                    codec_address_chirho,
                    function_group_nid_chirho,
                    pin_nid_chirho,
                    converter_nid_chirho,
                    score_chirho,
                    path_nodes_chirho,
                    path_selects_chirho,
                };

                let should_replace_chirho = best_route_chirho
                    .as_ref()
                    .map(|current_route_chirho| route_chirho.score_chirho > current_route_chirho.score_chirho)
                    .unwrap_or(true);
                if should_replace_chirho {
                    best_route_chirho = Some(route_chirho);
                }
            }
        }
    }

    best_route_chirho.ok_or(-ENODEV_CHIRHO)
}

fn build_stream_format_chirho(
    sample_rate_chirho: u32,
    channels_chirho: u8,
    sample_bits_chirho: u8,
) -> Option<u16> {
    if channels_chirho == 0 || channels_chirho > 8 {
        return None;
    }
    let sample_bits_field_chirho = match sample_bits_chirho {
        16 => AC_FMT_BITS_16_CHIRHO,
        _ => return None,
    };

    let base_rate_bits_chirho = RATE_TABLE_CHIRHO
        .iter()
        .find(|entry_chirho| entry_chirho.sample_rate_hz_chirho == sample_rate_chirho)?
        .stream_format_bits_chirho;

    Some(base_rate_bits_chirho | sample_bits_field_chirho | (channels_chirho as u16 - 1))
}

fn choose_supported_rate_chirho(desired_rate_chirho: u32, pcm_caps_chirho: u32) -> u32 {
    let mut best_rate_chirho = DEFAULT_SAMPLE_RATE_CHIRHO;
    let mut best_delta_chirho = u32::MAX;

    for rate_entry_chirho in RATE_TABLE_CHIRHO {
        if pcm_caps_chirho & rate_entry_chirho.pcm_rate_bit_chirho == 0 {
            continue;
        }
        let delta_chirho = desired_rate_chirho.abs_diff(rate_entry_chirho.sample_rate_hz_chirho);
        if delta_chirho < best_delta_chirho {
            best_delta_chirho = delta_chirho;
            best_rate_chirho = rate_entry_chirho.sample_rate_hz_chirho;
        }
    }

    if best_delta_chirho == u32::MAX {
        DEFAULT_SAMPLE_RATE_CHIRHO
    } else {
        best_rate_chirho
    }
}

fn configure_route_power_and_pins_chirho(controller_chirho: &mut HdaControllerChirho) -> Result<(), i64> {
    let route_chirho = controller_chirho.codec_route_chirho.clone();

    let _ = send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.function_group_nid_chirho,
        AC_VERB_SET_CODEC_RESET_CHIRHO,
        0,
    );
    let _ = send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.function_group_nid_chirho,
        AC_VERB_SET_POWER_STATE_CHIRHO,
        0,
    );

    for path_select_chirho in &route_chirho.path_selects_chirho {
        let _ = send_immediate_verb_chirho(
            controller_chirho,
            route_chirho.codec_address_chirho,
            path_select_chirho.node_nid_chirho,
            AC_VERB_SET_CONNECT_SEL_CHIRHO,
            path_select_chirho.connection_index_chirho as u16,
        )?;
    }

    for node_nid_chirho in &route_chirho.path_nodes_chirho {
        let _ = send_immediate_verb_chirho(
            controller_chirho,
            route_chirho.codec_address_chirho,
            *node_nid_chirho,
            AC_VERB_SET_POWER_STATE_CHIRHO,
            0,
        );
    }

    let pin_caps_chirho = get_parameter_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.pin_nid_chirho,
        AC_PAR_PIN_CAP_CHIRHO,
    )?;

    let mut pin_control_chirho = AC_PINCTL_OUT_EN_CHIRHO;
    if pin_caps_chirho & AC_PINCAP_HP_DRV_CHIRHO != 0 {
        pin_control_chirho |= AC_PINCTL_HP_EN_CHIRHO;
    }

    send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.pin_nid_chirho,
        AC_VERB_SET_PIN_WIDGET_CONTROL_CHIRHO,
        pin_control_chirho as u16,
    )?;

    if pin_caps_chirho & AC_PINCAP_EAPD_CHIRHO != 0 {
        let _ = send_immediate_verb_chirho(
            controller_chirho,
            route_chirho.codec_address_chirho,
            route_chirho.pin_nid_chirho,
            AC_VERB_SET_EAPD_BTLENABLE_CHIRHO,
            AC_EAPDBTL_EAPD_CHIRHO as u16,
        );
    }

    Ok(())
}

fn select_playback_format_chirho(controller_chirho: &mut HdaControllerChirho) -> Result<(), i64> {
    let route_chirho = controller_chirho.codec_route_chirho.clone();
    let converter_caps_chirho = get_widget_caps_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.converter_nid_chirho,
    )?;

    let caps_source_nid_chirho = if converter_caps_chirho & AC_WCAP_FORMAT_OVRD_CHIRHO != 0 {
        route_chirho.converter_nid_chirho
    } else {
        route_chirho.function_group_nid_chirho
    };

    let pcm_caps_chirho = get_parameter_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        caps_source_nid_chirho,
        AC_PAR_PCM_CHIRHO,
    )?;
    let stream_caps_chirho = get_parameter_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        caps_source_nid_chirho,
        AC_PAR_STREAM_CHIRHO,
    )?;

    if stream_caps_chirho & AC_SUPFMT_PCM_CHIRHO == 0 {
        return Err(-ENODEV_CHIRHO);
    }
    if pcm_caps_chirho & AC_SUPPCM_BITS_16_CHIRHO == 0 {
        return Err(-ENODEV_CHIRHO);
    }

    let stereo_supported_chirho = converter_caps_chirho & AC_WCAP_STEREO_CHIRHO != 0;
    let desired_channels_chirho = if controller_chirho.channels_chirho > 1 && stereo_supported_chirho {
        2
    } else {
        1
    };
    let chosen_rate_chirho = choose_supported_rate_chirho(controller_chirho.sample_rate_chirho, pcm_caps_chirho & AC_SUPPCM_RATES_CHIRHO);
    let stream_format_chirho = build_stream_format_chirho(
        chosen_rate_chirho,
        desired_channels_chirho,
        controller_chirho.sample_bits_chirho,
    )
    .ok_or(-EINVAL_CHIRHO)?;

    controller_chirho.sample_rate_chirho = chosen_rate_chirho;
    controller_chirho.channels_chirho = desired_channels_chirho;
    controller_chirho.stream_format_chirho = stream_format_chirho;
    Ok(())
}

fn controller_reset_chirho(controller_chirho: &HdaControllerChirho) -> Result<(), i64> {
    mmio_write_u32_chirho(controller_chirho, AZX_REG_INTCTL_CHIRHO, 0);
    mmio_write_u32_chirho(controller_chirho, AZX_REG_GCTL_CHIRHO, 0);
    if !wait_for_u32_mask_chirho(
        controller_chirho,
        AZX_REG_GCTL_CHIRHO,
        AZX_GCTL_RESET_CHIRHO,
        0,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO,
    ) {
        return Err(-EIO_CHIRHO);
    }

    pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO * 32);
    mmio_write_u32_chirho(controller_chirho, AZX_REG_GCTL_CHIRHO, AZX_GCTL_RESET_CHIRHO);
    if !wait_for_u32_mask_chirho(
        controller_chirho,
        AZX_REG_GCTL_CHIRHO,
        AZX_GCTL_RESET_CHIRHO,
        AZX_GCTL_RESET_CHIRHO,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO,
    ) {
        return Err(-EIO_CHIRHO);
    }

    pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO * 64);
    Ok(())
}

fn init_corb_rirb_chirho(controller_chirho: &HdaControllerChirho) -> Result<(), i64> {
    mmio_write_u8_chirho(controller_chirho, AZX_REG_CORBCTL_CHIRHO, 0);
    mmio_write_u8_chirho(controller_chirho, AZX_REG_RIRBCTL_CHIRHO, 0);
    mmio_write_u8_chirho(controller_chirho, AZX_REG_CORBSTS_CHIRHO, AZX_CORBSTS_CMEI_CHIRHO);
    mmio_write_u8_chirho(
        controller_chirho,
        AZX_REG_RIRBSTS_CHIRHO,
        AZX_RBSTS_IRQ_CHIRHO | AZX_RBSTS_OVERRUN_CHIRHO,
    );

    mmio_write_u16_chirho(controller_chirho, AZX_REG_CORBRP_CHIRHO, AZX_CORBRP_RST_CHIRHO);
    pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO * 2);
    mmio_write_u16_chirho(controller_chirho, AZX_REG_CORBRP_CHIRHO, 0);
    mmio_write_u16_chirho(controller_chirho, AZX_REG_RIRBWP_CHIRHO, AZX_RIRBWP_RST_CHIRHO);
    pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO * 2);

    mmio_write_u8_chirho(
        controller_chirho,
        AZX_REG_CORBSIZE_CHIRHO,
        HDA_RING_SIZE_SELECT_256_CHIRHO,
    );
    mmio_write_u8_chirho(
        controller_chirho,
        AZX_REG_RIRBSIZE_CHIRHO,
        HDA_RING_SIZE_SELECT_256_CHIRHO,
    );

    mmio_write_u32_chirho(
        controller_chirho,
        AZX_REG_CORBLBASE_CHIRHO,
        controller_chirho.corb_phys_chirho as u32,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        AZX_REG_CORBUBASE_CHIRHO,
        (controller_chirho.corb_phys_chirho >> 32) as u32,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        AZX_REG_RIRBLBASE_CHIRHO,
        controller_chirho.rirb_phys_chirho as u32,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        AZX_REG_RIRBUBASE_CHIRHO,
        (controller_chirho.rirb_phys_chirho >> 32) as u32,
    );
    mmio_write_u16_chirho(controller_chirho, AZX_REG_CORBWP_CHIRHO, 0);
    mmio_write_u16_chirho(controller_chirho, AZX_REG_RINTCNT_CHIRHO, HDA_RINTCNT_CHIRHO);
    mmio_write_u8_chirho(controller_chirho, AZX_REG_CORBCTL_CHIRHO, AZX_CORBCTL_RUN_CHIRHO);
    mmio_write_u8_chirho(controller_chirho, AZX_REG_RIRBCTL_CHIRHO, AZX_RBCTL_DMA_EN_CHIRHO);

    if !wait_for_u8_mask_chirho(
        controller_chirho,
        AZX_REG_CORBCTL_CHIRHO,
        AZX_CORBCTL_RUN_CHIRHO,
        AZX_CORBCTL_RUN_CHIRHO,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO / 4,
    ) {
        return Err(-EIO_CHIRHO);
    }
    if !wait_for_u8_mask_chirho(
        controller_chirho,
        AZX_REG_RIRBCTL_CHIRHO,
        AZX_RBCTL_DMA_EN_CHIRHO,
        AZX_RBCTL_DMA_EN_CHIRHO,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO / 4,
    ) {
        return Err(-EIO_CHIRHO);
    }

    Ok(())
}

fn stop_stream_dma_chirho(controller_chirho: &HdaControllerChirho) {
    let stream_offset_chirho = controller_chirho.output_stream_base_chirho;
    let stream_control_chirho =
        mmio_read_u16_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO);
    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        stream_control_chirho & !SD_CTL_DMA_START_CHIRHO,
    );
    let _ = wait_for_u16_mask_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        SD_CTL_DMA_START_CHIRHO,
        0,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO / 8,
    );
    mmio_write_u8_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_STS_CHIRHO,
        SD_INT_MASK_CHIRHO,
    );
}

fn reset_output_stream_chirho(controller_chirho: &HdaControllerChirho) -> Result<(), i64> {
    let stream_offset_chirho = controller_chirho.output_stream_base_chirho;
    stop_stream_dma_chirho(controller_chirho);

    let stream_control_chirho =
        mmio_read_u16_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO);
    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        stream_control_chirho | SD_CTL_STREAM_RESET_CHIRHO,
    );
    if !wait_for_u16_mask_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        SD_CTL_STREAM_RESET_CHIRHO,
        SD_CTL_STREAM_RESET_CHIRHO,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO / 8,
    ) {
        return Err(-EIO_CHIRHO);
    }

    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        stream_control_chirho & !SD_CTL_STREAM_RESET_CHIRHO,
    );
    if !wait_for_u16_mask_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        SD_CTL_STREAM_RESET_CHIRHO,
        0,
        HDA_MMIO_TIMEOUT_LOOPS_CHIRHO / 8,
    ) {
        return Err(-EIO_CHIRHO);
    }

    Ok(())
}

fn program_bdl_entries_chirho(controller_chirho: &HdaControllerChirho) {
    let bdl_ptr_chirho = controller_chirho.bdl_virt_chirho as *mut BdlEntryChirho;
    unsafe {
        write_bytes(
            controller_chirho.bdl_virt_chirho as *mut u8,
            0,
            HDA_BUFFER_PAGE_BYTES_CHIRHO,
        );
        for descriptor_index_chirho in 0..HDA_BDL_ENTRY_COUNT_CHIRHO {
            let descriptor_flags_chirho = if descriptor_index_chirho + 1 == HDA_BDL_ENTRY_COUNT_CHIRHO {
                0x01
            } else {
                0
            };
            write_volatile(
                bdl_ptr_chirho.add(descriptor_index_chirho),
                BdlEntryChirho {
                    address_low_chirho: controller_chirho.pcm_page_phys_addrs_chirho
                        [descriptor_index_chirho] as u32,
                    address_high_chirho: (controller_chirho.pcm_page_phys_addrs_chirho
                        [descriptor_index_chirho]
                        >> 32) as u32,
                    length_chirho: HDA_BUFFER_PAGE_BYTES_CHIRHO as u32,
                    flags_chirho: descriptor_flags_chirho,
                },
            );
        }
    }
}

fn program_stream_registers_chirho(controller_chirho: &mut HdaControllerChirho) -> Result<(), i64> {
    reset_output_stream_chirho(controller_chirho)?;
    let route_chirho = controller_chirho.codec_route_chirho.clone();
    let stream_offset_chirho = controller_chirho.output_stream_base_chirho;
    program_bdl_entries_chirho(controller_chirho);

    send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.converter_nid_chirho,
        AC_VERB_SET_CVT_CHAN_COUNT_CHIRHO,
        (controller_chirho.channels_chirho - 1) as u16,
    )?;
    send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.converter_nid_chirho,
        AC_VERB_SET_STREAM_FORMAT_CHIRHO,
        controller_chirho.stream_format_chirho,
    )?;
    send_immediate_verb_chirho(
        controller_chirho,
        route_chirho.codec_address_chirho,
        route_chirho.converter_nid_chirho,
        AC_VERB_SET_CHANNEL_STREAMID_CHIRHO,
        ((STREAM_TAG_CHIRHO as u16) << 4) | 0,
    )?;

    mmio_write_u8_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_3B_CHIRHO,
        STREAM_TAG_CHIRHO << 4,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CBL_CHIRHO,
        HDA_BDL_BUFFER_BYTES_CHIRHO as u32,
    );
    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_LVI_CHIRHO,
        (HDA_BDL_ENTRY_COUNT_CHIRHO as u16) - 1,
    );
    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_FORMAT_CHIRHO,
        controller_chirho.stream_format_chirho,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_BDLPL_CHIRHO,
        controller_chirho.bdl_phys_chirho as u32,
    );
    mmio_write_u32_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_BDLPU_CHIRHO,
        (controller_chirho.bdl_phys_chirho >> 32) as u32,
    );
    mmio_write_u8_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_STS_CHIRHO,
        SD_INT_MASK_CHIRHO,
    );

    Ok(())
}

fn expected_wallclk_ticks_chirho(controller_chirho: &HdaControllerChirho) -> u32 {
    let bytes_per_second_chirho =
        controller_chirho.sample_rate_chirho as u64
            * controller_chirho.channels_chirho as u64
            * (controller_chirho.sample_bits_chirho as u64 / 8);
    if bytes_per_second_chirho == 0 {
        return 24_000_000 / 10;
    }

    let base_ticks_chirho =
        (HDA_BDL_BUFFER_BYTES_CHIRHO as u64 * 24_000_000u64) / bytes_per_second_chirho;
    min(base_ticks_chirho.saturating_mul(3), u32::MAX as u64) as u32
}

fn copy_pcm_to_dma_pages_chirho(controller_chirho: &HdaControllerChirho, pcm_bytes_chirho: &[u8]) {
    let mut remaining_bytes_chirho = pcm_bytes_chirho;

    for page_index_chirho in 0..HDA_BDL_ENTRY_COUNT_CHIRHO {
        let page_virt_chirho = controller_chirho.pcm_page_virt_addrs_chirho[page_index_chirho];
        unsafe {
            write_bytes(
                page_virt_chirho as *mut u8,
                0,
                HDA_BUFFER_PAGE_BYTES_CHIRHO,
            );
        }

        if remaining_bytes_chirho.is_empty() {
            continue;
        }

        let chunk_len_chirho = min(remaining_bytes_chirho.len(), HDA_BUFFER_PAGE_BYTES_CHIRHO);
        unsafe {
            copy_nonoverlapping(
                remaining_bytes_chirho.as_ptr(),
                page_virt_chirho as *mut u8,
                chunk_len_chirho,
            );
        }
        remaining_bytes_chirho = &remaining_bytes_chirho[chunk_len_chirho..];
    }
}

fn play_pcm_chunk_chirho(
    controller_chirho: &mut HdaControllerChirho,
    pcm_bytes_chirho: &[u8],
) -> Result<usize, i64> {
    let copy_len_chirho = min(pcm_bytes_chirho.len(), HDA_BDL_BUFFER_BYTES_CHIRHO);
    if copy_len_chirho == 0 {
        return Ok(0);
    }

    copy_pcm_to_dma_pages_chirho(controller_chirho, &pcm_bytes_chirho[..copy_len_chirho]);
    program_stream_registers_chirho(controller_chirho)?;

    let stream_offset_chirho = controller_chirho.output_stream_base_chirho;
    let stream_control_chirho =
        mmio_read_u16_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO);
    mmio_write_u16_chirho(
        controller_chirho,
        stream_offset_chirho + AZX_REG_SD_CTL_CHIRHO,
        stream_control_chirho | SD_CTL_DMA_START_CHIRHO,
    );

    let playback_deadline_chirho = mmio_read_u32_chirho(controller_chirho, AZX_REG_WALLCLK_CHIRHO)
        .wrapping_add(expected_wallclk_ticks_chirho(controller_chirho));
    let mut saw_fifo_ready_chirho = false;

    for _ in 0..HDA_MMIO_TIMEOUT_LOOPS_CHIRHO {
        let stream_status_chirho =
            mmio_read_u8_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_STS_CHIRHO);
        if stream_status_chirho & SD_STS_FIFO_READY_CHIRHO != 0 {
            saw_fifo_ready_chirho = true;
        }
        if stream_status_chirho & SD_INT_COMPLETE_CHIRHO != 0 {
            break;
        }

        let wallclk_now_chirho = mmio_read_u32_chirho(controller_chirho, AZX_REG_WALLCLK_CHIRHO);
        if saw_fifo_ready_chirho
            && wallclk_now_chirho.wrapping_sub(playback_deadline_chirho) < (u32::MAX / 2)
        {
            break;
        }

        pause_short_chirho(HDA_PAUSE_GRANULARITY_CHIRHO);
    }

    stop_stream_dma_chirho(controller_chirho);
    Ok(copy_len_chirho)
}

fn current_stream_status_summary_chirho(controller_chirho: &HdaControllerChirho) -> (u8, u32) {
    let stream_offset_chirho = controller_chirho.output_stream_base_chirho;
    let stream_status_chirho =
        mmio_read_u8_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_STS_CHIRHO);
    let stream_position_chirho =
        mmio_read_u32_chirho(controller_chirho, stream_offset_chirho + AZX_REG_SD_LPIB_CHIRHO);
    (stream_status_chirho, stream_position_chirho)
}

fn is_hda_controller_chirho(device_chirho: &PciDeviceChirho) -> bool {
    (device_chirho.vendor_id_chirho == INTEL_VENDOR_ID_CHIRHO
        && device_chirho.device_id_chirho == INTEL_HDA_DEVICE_ID_CHIRHO)
        || (device_chirho.class_code_chirho == PCI_CLASS_MULTIMEDIA_CHIRHO
            && device_chirho.subclass_chirho == PCI_SUBCLASS_HDA_CHIRHO)
}

fn init_controller_from_pci_chirho(device_chirho: &PciDeviceChirho) -> Result<HdaControllerChirho, i64> {
    unsafe {
        device_chirho.enable_bus_master_chirho();
    }

    let mut mmio_bar_chirho = unsafe { device_chirho.read_bar_chirho(0) };
    if mmio_bar_chirho
        .as_ref()
        .map(|bar_chirho| !bar_chirho.is_memory_chirho || bar_chirho.base_address_chirho == 0)
        .unwrap_or(true)
    {
        let _ = unsafe { pci_assign_bar_chirho(device_chirho, 0) };
        mmio_bar_chirho = unsafe { device_chirho.read_bar_chirho(0) };
    }

    let mmio_bar_chirho = mmio_bar_chirho.ok_or(-ENODEV_CHIRHO)?;
    if !mmio_bar_chirho.is_memory_chirho {
        return Err(-ENODEV_CHIRHO);
    }

    let (corb_phys_chirho, corb_virt_chirho) = allocate_zeroed_dma_frame_chirho()?;
    let (rirb_phys_chirho, rirb_virt_chirho) = allocate_zeroed_dma_frame_chirho()?;
    let (bdl_phys_chirho, bdl_virt_chirho) = allocate_zeroed_dma_frame_chirho()?;

    let mut pcm_page_phys_addrs_chirho = [0u64; HDA_BDL_ENTRY_COUNT_CHIRHO];
    let mut pcm_page_virt_addrs_chirho = [0u64; HDA_BDL_ENTRY_COUNT_CHIRHO];
    for descriptor_index_chirho in 0..HDA_BDL_ENTRY_COUNT_CHIRHO {
        let (page_phys_chirho, page_virt_chirho) = allocate_zeroed_dma_frame_chirho()?;
        pcm_page_phys_addrs_chirho[descriptor_index_chirho] = page_phys_chirho;
        pcm_page_virt_addrs_chirho[descriptor_index_chirho] = page_virt_chirho;
    }

    let mut controller_chirho = HdaControllerChirho {
        mmio_phys_base_chirho: mmio_bar_chirho.base_address_chirho,
        mmio_virt_base_chirho: mmio_bar_chirho.base_address_chirho + crate::pagetable_chirho::phys_mem_offset_chirho(),
        mmio_size_chirho: mmio_bar_chirho.size_chirho,
        output_stream_base_chirho: 0,
        output_stream_index_chirho: 0,
        bus_chirho: device_chirho.bus_chirho,
        device_chirho: device_chirho.device_chirho,
        function_chirho: device_chirho.function_chirho,
        codec_route_chirho: CodecRouteChirho {
            codec_address_chirho: 0,
            function_group_nid_chirho: 0,
            pin_nid_chirho: 0,
            converter_nid_chirho: 0,
            score_chirho: 0,
            path_nodes_chirho: Vec::new(),
            path_selects_chirho: Vec::new(),
        },
        corb_phys_chirho,
        corb_virt_chirho,
        rirb_phys_chirho,
        rirb_virt_chirho,
        bdl_phys_chirho,
        bdl_virt_chirho,
        pcm_page_phys_addrs_chirho,
        pcm_page_virt_addrs_chirho,
        sample_rate_chirho: DEFAULT_SAMPLE_RATE_CHIRHO,
        channels_chirho: DEFAULT_CHANNELS_CHIRHO,
        sample_bits_chirho: DEFAULT_SAMPLE_BITS_CHIRHO,
        stream_format_chirho: 0,
    };

    controller_reset_chirho(&controller_chirho)?;
    init_corb_rirb_chirho(&controller_chirho)?;

    let gcap_chirho = mmio_read_u16_chirho(&controller_chirho, AZX_REG_GCAP_CHIRHO);
    let input_stream_count_chirho = ((gcap_chirho & AZX_GCAP_ISS_CHIRHO) >> 8) as u8;
    let output_stream_count_chirho = ((gcap_chirho & AZX_GCAP_OSS_CHIRHO) >> 12) as u8;
    if output_stream_count_chirho == 0 {
        return Err(-ENODEV_CHIRHO);
    }

    controller_chirho.output_stream_base_chirho =
        AZX_STREAM_BASE_CHIRHO + (input_stream_count_chirho as u64 * AZX_STREAM_STRIDE_CHIRHO);
    controller_chirho.output_stream_index_chirho = 0;

    let state_status_chirho = mmio_read_u16_chirho(&controller_chirho, AZX_REG_STATESTS_CHIRHO);
    controller_chirho.codec_route_chirho = find_best_codec_route_chirho(&mut controller_chirho, state_status_chirho)?;
    mmio_write_u16_chirho(&controller_chirho, AZX_REG_STATESTS_CHIRHO, state_status_chirho);

    configure_route_power_and_pins_chirho(&mut controller_chirho)?;
    select_playback_format_chirho(&mut controller_chirho)?;
    program_stream_registers_chirho(&mut controller_chirho)?;

    Ok(controller_chirho)
}

// ============================================================================
// Public init entrypoint
// ============================================================================

pub fn init_hda_chirho() {
    if HDA_CONTROLLER_CHIRHO.lock().is_some() {
        return;
    }

    let devices_chirho = unsafe { scan_bus_chirho(0) };
    let Some(hda_device_chirho) = devices_chirho.iter().find(|device_chirho| is_hda_controller_chirho(device_chirho)).cloned() else {
        crate::serial_println_chirho!("HDA: no Intel HDA controller found on PCI bus 0");
        return;
    };

    match init_controller_from_pci_chirho(&hda_device_chirho) {
        Ok(controller_chirho) => {
            let (stream_status_chirho, stream_position_chirho) =
                current_stream_status_summary_chirho(&controller_chirho);
            crate::serial_println_chirho!(
                "HDA: ready at PCI {:02x}:{:02x}.{} BAR={:#x} route codec={} afg={:#x} pin={:#x} dac={:#x} rate={}Hz ch={} fmt={:#06x} sd_sts={:#04x} lpib={}",
                controller_chirho.bus_chirho,
                controller_chirho.device_chirho,
                controller_chirho.function_chirho,
                controller_chirho.mmio_phys_base_chirho,
                controller_chirho.codec_route_chirho.codec_address_chirho,
                controller_chirho.codec_route_chirho.function_group_nid_chirho,
                controller_chirho.codec_route_chirho.pin_nid_chirho,
                controller_chirho.codec_route_chirho.converter_nid_chirho,
                controller_chirho.sample_rate_chirho,
                controller_chirho.channels_chirho,
                controller_chirho.stream_format_chirho,
                stream_status_chirho,
                stream_position_chirho,
            );
            *HDA_CONTROLLER_CHIRHO.lock() = Some(controller_chirho);
        }
        Err(error_chirho) => {
            crate::serial_println_chirho!(
                "HDA: init failed for PCI {:02x}:{:02x}.{} with error {}",
                hda_device_chirho.bus_chirho,
                hda_device_chirho.device_chirho,
                hda_device_chirho.function_chirho,
                error_chirho,
            );
        }
    }
}

// ============================================================================
// /dev/dsp file operations
// ============================================================================

pub struct DevDspOpsChirho;

impl FileOpsChirho for DevDspOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Ok(0)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        if buf_chirho.is_empty() {
            return Ok(0);
        }

        let mut controller_guard_chirho = HDA_CONTROLLER_CHIRHO.lock();
        let Some(controller_chirho) = controller_guard_chirho.as_mut() else {
            return Ok(buf_chirho.len());
        };

        let mut total_written_chirho = 0usize;
        while total_written_chirho < buf_chirho.len() {
            let chunk_written_chirho =
                play_pcm_chunk_chirho(controller_chirho, &buf_chirho[total_written_chirho..])?;
            if chunk_written_chirho == 0 {
                break;
            }
            total_written_chirho += chunk_written_chirho;
        }

        Ok(total_written_chirho)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        let mut controller_guard_chirho = HDA_CONTROLLER_CHIRHO.lock();
        let controller_option_chirho = controller_guard_chirho.as_mut();

        let mut write_back_i32_chirho = |value_chirho: i32| {
            if arg_chirho != 0 {
                unsafe {
                    let value_ptr_chirho = arg_chirho as *mut i32;
                    if !value_ptr_chirho.is_null() {
                        write_volatile(value_ptr_chirho, value_chirho);
                    }
                }
            }
        };

        let read_i32_arg_chirho = || -> Option<i32> {
            if arg_chirho == 0 {
                return None;
            }
            unsafe {
                let value_ptr_chirho = arg_chirho as *const i32;
                if value_ptr_chirho.is_null() {
                    None
                } else {
                    Some(read_volatile(value_ptr_chirho))
                }
            }
        };

        match cmd_chirho {
            SNDCTL_DSP_RESET_CHIRHO | SNDCTL_DSP_SYNC_CHIRHO => Ok(0),
            SNDCTL_DSP_SETFMT_CHIRHO => {
                write_back_i32_chirho(AFMT_S16_LE_CHIRHO as i32);
                Ok(AFMT_S16_LE_CHIRHO)
            }
            SNDCTL_DSP_SPEED_CHIRHO => {
                let requested_rate_chirho = read_i32_arg_chirho()
                    .unwrap_or(DEFAULT_SAMPLE_RATE_CHIRHO as i32)
                    .max(8_000) as u32;

                let chosen_rate_chirho = if let Some(controller_chirho) = controller_option_chirho {
                    controller_chirho.sample_rate_chirho = requested_rate_chirho;
                    select_playback_format_chirho(controller_chirho)?;
                    controller_chirho.sample_rate_chirho
                } else {
                    choose_supported_rate_chirho(requested_rate_chirho, AC_SUPPCM_RATES_CHIRHO)
                };

                write_back_i32_chirho(chosen_rate_chirho as i32);
                Ok(chosen_rate_chirho as i64)
            }
            SNDCTL_DSP_CHANNELS_CHIRHO => {
                let requested_channels_chirho = read_i32_arg_chirho()
                    .unwrap_or(DEFAULT_CHANNELS_CHIRHO as i32)
                    .clamp(1, 2) as u8;

                let chosen_channels_chirho = if let Some(controller_chirho) = controller_option_chirho {
                    controller_chirho.channels_chirho = requested_channels_chirho;
                    select_playback_format_chirho(controller_chirho)?;
                    controller_chirho.channels_chirho
                } else {
                    requested_channels_chirho
                };

                write_back_i32_chirho(chosen_channels_chirho as i32);
                Ok(chosen_channels_chirho as i64)
            }
            SNDCTL_DSP_GETFMTS_CHIRHO => Ok(AFMT_S16_LE_CHIRHO),
            SNDCTL_DSP_GETBLKSIZE_CHIRHO => {
                write_back_i32_chirho(HDA_BDL_BUFFER_BYTES_CHIRHO as i32);
                Ok(HDA_BDL_BUFFER_BYTES_CHIRHO as i64)
            }
            SNDCTL_DSP_GETCAPS_CHIRHO => Ok(DSP_CAP_REALTIME_CHIRHO | DSP_CAP_BATCH_CHIRHO),
            _ => Err(-ENOSYS_CHIRHO),
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

pub static DEV_DSP_OPS_CHIRHO: DevDspOpsChirho = DevDspOpsChirho;
