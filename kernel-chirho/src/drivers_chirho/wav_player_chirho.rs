// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Minimal WAV playback helper for Lineluya.
//!
//! This module intentionally supports the canonical 44-byte PCM WAV layout:
//! - RIFF/WAVE header
//! - `fmt ` chunk at offset 12
//! - `data` chunk at offset 36
//! - PCM payload beginning at byte 44
//!
//! That is enough for quick audio bring-up and `/dev/dsp` validation without
//! pulling in a full userspace media stack.

extern crate alloc;

use alloc::vec::Vec;

use crate::process_chirho::try_read_file_pub_chirho;
use crate::vfs_chirho::FileChirho;

const WAV_HEADER_BYTES_CHIRHO: usize = 44;
const WAV_PCM_FORMAT_CHIRHO: u16 = 0x0001;
const SNDCTL_DSP_SPEED_CHIRHO: u64 = 0xC004_5002;
const SNDCTL_DSP_SETFMT_CHIRHO: u64 = 0xC004_5005;
const SNDCTL_DSP_CHANNELS_CHIRHO: u64 = 0xC004_5006;
const AFMT_S16_LE_CHIRHO: i32 = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavPlayerErrorChirho {
    FileReadFailedChirho,
    HeaderTooShortChirho,
    InvalidRiffHeaderChirho,
    UnsupportedLayoutChirho,
    UnsupportedFormatChirho,
    TruncatedDataChirho,
    AudioDeviceErrorChirho(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavHeaderChirho {
    pub channel_count_chirho: u16,
    pub sample_rate_hz_chirho: u32,
    pub byte_rate_chirho: u32,
    pub block_align_chirho: u16,
    pub sample_bits_chirho: u16,
    pub pcm_data_offset_chirho: usize,
    pub pcm_data_len_chirho: usize,
}

#[inline]
fn read_u16_le_chirho(bytes_chirho: &[u8], offset_chirho: usize) -> Result<u16, WavPlayerErrorChirho> {
    let end_chirho = offset_chirho
        .checked_add(2)
        .ok_or(WavPlayerErrorChirho::HeaderTooShortChirho)?;
    let slice_chirho = bytes_chirho
        .get(offset_chirho..end_chirho)
        .ok_or(WavPlayerErrorChirho::HeaderTooShortChirho)?;
    Ok(u16::from_le_bytes([slice_chirho[0], slice_chirho[1]]))
}

#[inline]
fn read_u32_le_chirho(bytes_chirho: &[u8], offset_chirho: usize) -> Result<u32, WavPlayerErrorChirho> {
    let end_chirho = offset_chirho
        .checked_add(4)
        .ok_or(WavPlayerErrorChirho::HeaderTooShortChirho)?;
    let slice_chirho = bytes_chirho
        .get(offset_chirho..end_chirho)
        .ok_or(WavPlayerErrorChirho::HeaderTooShortChirho)?;
    Ok(u32::from_le_bytes([
        slice_chirho[0],
        slice_chirho[1],
        slice_chirho[2],
        slice_chirho[3],
    ]))
}

pub fn parse_wav_header_chirho(wav_bytes_chirho: &[u8]) -> Result<WavHeaderChirho, WavPlayerErrorChirho> {
    if wav_bytes_chirho.len() < WAV_HEADER_BYTES_CHIRHO {
        return Err(WavPlayerErrorChirho::HeaderTooShortChirho);
    }

    if &wav_bytes_chirho[0..4] != b"RIFF" || &wav_bytes_chirho[8..12] != b"WAVE" {
        return Err(WavPlayerErrorChirho::InvalidRiffHeaderChirho);
    }

    if &wav_bytes_chirho[12..16] != b"fmt " || &wav_bytes_chirho[36..40] != b"data" {
        return Err(WavPlayerErrorChirho::UnsupportedLayoutChirho);
    }

    let fmt_chunk_size_chirho = read_u32_le_chirho(wav_bytes_chirho, 16)?;
    if fmt_chunk_size_chirho != 16 {
        return Err(WavPlayerErrorChirho::UnsupportedLayoutChirho);
    }

    let audio_format_chirho = read_u16_le_chirho(wav_bytes_chirho, 20)?;
    if audio_format_chirho != WAV_PCM_FORMAT_CHIRHO {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    let channel_count_chirho = read_u16_le_chirho(wav_bytes_chirho, 22)?;
    let sample_rate_hz_chirho = read_u32_le_chirho(wav_bytes_chirho, 24)?;
    let byte_rate_chirho = read_u32_le_chirho(wav_bytes_chirho, 28)?;
    let block_align_chirho = read_u16_le_chirho(wav_bytes_chirho, 32)?;
    let sample_bits_chirho = read_u16_le_chirho(wav_bytes_chirho, 34)?;
    let pcm_data_declared_len_chirho = read_u32_le_chirho(wav_bytes_chirho, 40)? as usize;

    if !(1..=2).contains(&channel_count_chirho) || sample_rate_hz_chirho == 0 {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    if sample_bits_chirho != 16 {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    let expected_block_align_chirho = channel_count_chirho
        .checked_mul((sample_bits_chirho / 8).max(1))
        .ok_or(WavPlayerErrorChirho::UnsupportedFormatChirho)?;
    if block_align_chirho != expected_block_align_chirho {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    let expected_byte_rate_chirho = sample_rate_hz_chirho
        .checked_mul(block_align_chirho as u32)
        .ok_or(WavPlayerErrorChirho::UnsupportedFormatChirho)?;
    if byte_rate_chirho != expected_byte_rate_chirho {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    let available_pcm_len_chirho = wav_bytes_chirho
        .len()
        .checked_sub(WAV_HEADER_BYTES_CHIRHO)
        .ok_or(WavPlayerErrorChirho::TruncatedDataChirho)?;
    if pcm_data_declared_len_chirho > available_pcm_len_chirho {
        return Err(WavPlayerErrorChirho::TruncatedDataChirho);
    }

    Ok(WavHeaderChirho {
        channel_count_chirho,
        sample_rate_hz_chirho,
        byte_rate_chirho,
        block_align_chirho,
        sample_bits_chirho,
        pcm_data_offset_chirho: WAV_HEADER_BYTES_CHIRHO,
        pcm_data_len_chirho: pcm_data_declared_len_chirho,
    })
}

fn open_dev_dsp_file_chirho() -> Result<FileChirho, WavPlayerErrorChirho> {
    let (inode_chirho, file_ops_chirho) = crate::fs_chirho::resolve_path_chirho("/dev/dsp")
        .map_err(WavPlayerErrorChirho::AudioDeviceErrorChirho)?;

    Ok(FileChirho {
        inode_chirho,
        pos_chirho: 0,
        flags_chirho: 0,
        ops_chirho: file_ops_chirho,
    })
}

fn configure_dev_dsp_for_wav_chirho(
    dsp_file_chirho: &mut FileChirho,
    wav_header_chirho: &WavHeaderChirho,
) -> Result<(), WavPlayerErrorChirho> {
    let mut sample_format_chirho = AFMT_S16_LE_CHIRHO;
    dsp_file_chirho
        .ops_chirho
        .ioctl_chirho(
            dsp_file_chirho,
            SNDCTL_DSP_SETFMT_CHIRHO,
            (&mut sample_format_chirho as *mut i32) as u64,
        )
        .map_err(WavPlayerErrorChirho::AudioDeviceErrorChirho)?;
    if sample_format_chirho != AFMT_S16_LE_CHIRHO {
        return Err(WavPlayerErrorChirho::UnsupportedFormatChirho);
    }

    let mut requested_rate_chirho = wav_header_chirho.sample_rate_hz_chirho as i32;
    let chosen_rate_chirho = dsp_file_chirho
        .ops_chirho
        .ioctl_chirho(
            dsp_file_chirho,
            SNDCTL_DSP_SPEED_CHIRHO,
            (&mut requested_rate_chirho as *mut i32) as u64,
        )
        .map_err(WavPlayerErrorChirho::AudioDeviceErrorChirho)?;
    if chosen_rate_chirho <= 0 {
        return Err(WavPlayerErrorChirho::AudioDeviceErrorChirho(chosen_rate_chirho));
    }

    let mut requested_channels_chirho = wav_header_chirho.channel_count_chirho as i32;
    let chosen_channels_chirho = dsp_file_chirho
        .ops_chirho
        .ioctl_chirho(
            dsp_file_chirho,
            SNDCTL_DSP_CHANNELS_CHIRHO,
            (&mut requested_channels_chirho as *mut i32) as u64,
        )
        .map_err(WavPlayerErrorChirho::AudioDeviceErrorChirho)?;
    if chosen_channels_chirho <= 0 {
        return Err(WavPlayerErrorChirho::AudioDeviceErrorChirho(
            chosen_channels_chirho,
        ));
    }

    Ok(())
}

pub fn play_wav_bytes_chirho(wav_bytes_chirho: &[u8]) -> Result<usize, WavPlayerErrorChirho> {
    let wav_header_chirho = parse_wav_header_chirho(wav_bytes_chirho)?;
    let pcm_end_chirho = wav_header_chirho
        .pcm_data_offset_chirho
        .checked_add(wav_header_chirho.pcm_data_len_chirho)
        .ok_or(WavPlayerErrorChirho::TruncatedDataChirho)?;
    let pcm_payload_chirho = wav_bytes_chirho
        .get(wav_header_chirho.pcm_data_offset_chirho..pcm_end_chirho)
        .ok_or(WavPlayerErrorChirho::TruncatedDataChirho)?;

    let mut dsp_file_chirho = open_dev_dsp_file_chirho()?;
    configure_dev_dsp_for_wav_chirho(&mut dsp_file_chirho, &wav_header_chirho)?;

    crate::serial_println_chirho!(
        "WAV: playback start rate={}Hz channels={} bits={} bytes={}",
        wav_header_chirho.sample_rate_hz_chirho,
        wav_header_chirho.channel_count_chirho,
        wav_header_chirho.sample_bits_chirho,
        pcm_payload_chirho.len(),
    );

    dsp_file_chirho
        .ops_chirho
        .write_chirho(&mut dsp_file_chirho, pcm_payload_chirho)
        .map_err(WavPlayerErrorChirho::AudioDeviceErrorChirho)
}

pub fn play_wav_file_chirho(path_chirho: &str) -> Result<usize, WavPlayerErrorChirho> {
    let wav_bytes_chirho =
        try_read_file_pub_chirho(path_chirho).ok_or(WavPlayerErrorChirho::FileReadFailedChirho)?;
    play_wav_bytes_chirho(&wav_bytes_chirho)
}

pub fn build_test_tone_wav_chirho(
    duration_millis_chirho: u32,
    frequency_hz_chirho: u32,
    sample_rate_hz_chirho: u32,
) -> Vec<u8> {
    const SINE_LUT_CHIRHO: [i16; 32] = [
        0, 6393, 12539, 18204, 23170, 27244, 30273, 32138, 32767, 32138, 30273, 27244, 23170,
        18204, 12539, 6393, 0, -6393, -12539, -18204, -23170, -27244, -30273, -32138, -32767,
        -32138, -30273, -27244, -23170, -18204, -12539, -6393,
    ];

    let sample_count_chirho =
        ((sample_rate_hz_chirho as u64 * duration_millis_chirho as u64) / 1000) as usize;
    let channel_count_chirho = 2u16;
    let sample_bits_chirho = 16u16;
    let block_align_chirho = channel_count_chirho * (sample_bits_chirho / 8);
    let byte_rate_chirho = sample_rate_hz_chirho * block_align_chirho as u32;
    let pcm_bytes_len_chirho = sample_count_chirho * block_align_chirho as usize;
    let mut wav_bytes_chirho = Vec::with_capacity(WAV_HEADER_BYTES_CHIRHO + pcm_bytes_len_chirho);

    wav_bytes_chirho.extend_from_slice(b"RIFF");
    wav_bytes_chirho.extend_from_slice(&(36u32 + pcm_bytes_len_chirho as u32).to_le_bytes());
    wav_bytes_chirho.extend_from_slice(b"WAVE");
    wav_bytes_chirho.extend_from_slice(b"fmt ");
    wav_bytes_chirho.extend_from_slice(&16u32.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&WAV_PCM_FORMAT_CHIRHO.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&channel_count_chirho.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&sample_rate_hz_chirho.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&byte_rate_chirho.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&block_align_chirho.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(&sample_bits_chirho.to_le_bytes());
    wav_bytes_chirho.extend_from_slice(b"data");
    wav_bytes_chirho.extend_from_slice(&(pcm_bytes_len_chirho as u32).to_le_bytes());

    let table_size_chirho = SINE_LUT_CHIRHO.len() as u64;
    let phase_step_chirho = ((frequency_hz_chirho as u64) << 32) / sample_rate_hz_chirho as u64;
    let mut phase_accumulator_chirho = 0u64;

    for _ in 0..sample_count_chirho {
        let table_index_chirho =
            (((phase_accumulator_chirho >> 32) * table_size_chirho) & 0xFFFF_FFFF) as usize
                % SINE_LUT_CHIRHO.len();
        let sample_value_chirho = SINE_LUT_CHIRHO[table_index_chirho];
        wav_bytes_chirho.extend_from_slice(&sample_value_chirho.to_le_bytes());
        wav_bytes_chirho.extend_from_slice(&sample_value_chirho.to_le_bytes());
        phase_accumulator_chirho = phase_accumulator_chirho.wrapping_add(phase_step_chirho);
    }

    wav_bytes_chirho
}
