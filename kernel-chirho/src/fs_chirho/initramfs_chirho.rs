// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Initramfs (initial ramdisk) loader and CPIO archive parser for the
//! Lineluya kernel (A5 subsystem).
//!
//! Supports:
//! - newc (SVR4) CPIO archive format parsing
//! - Populating the root tmpfs from the initramfs image
//! - Optional gzip decompression (placeholder — requires inflate impl)
//!
//! The initramfs is typically embedded in the kernel image or provided
//! by the bootloader at a known physical address.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

// ============================================================================
// CPIO newc header (110 bytes ASCII)
// ============================================================================

/// Magic string for SVR4 "newc" CPIO archives.
const CPIO_NEWC_MAGIC_CHIRHO: &[u8; 6] = b"070701";

/// A parsed CPIO file entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CpioEntryChirho {
    /// File path (relative, no leading /).
    pub name_chirho: String,
    /// File mode (includes type bits: S_IFREG, S_IFDIR, etc.).
    pub mode_chirho: u32,
    /// File size in bytes.
    pub filesize_chirho: u32,
    /// Owner UID.
    pub uid_chirho: u32,
    /// Owner GID.
    pub gid_chirho: u32,
    /// Number of hard links.
    pub nlink_chirho: u32,
    /// Modification time (Unix timestamp).
    pub mtime_chirho: u32,
    /// Inode number.
    pub ino_chirho: u32,
    /// Device major number.
    pub dev_major_chirho: u32,
    /// Device minor number.
    pub dev_minor_chirho: u32,
    /// Rdev major number (for device nodes).
    pub rdev_major_chirho: u32,
    /// Rdev minor number.
    pub rdev_minor_chirho: u32,
    /// Offset into the archive where the file data begins.
    pub data_offset_chirho: usize,
}

/// File type bits from mode.
#[allow(dead_code)]
const S_IFMT_CHIRHO: u32 = 0o170000;
#[allow(dead_code)]
const S_IFDIR_CHIRHO: u32 = 0o040000;
#[allow(dead_code)]
const S_IFREG_CHIRHO: u32 = 0o100000;
#[allow(dead_code)]
const S_IFLNK_CHIRHO: u32 = 0o120000;
#[allow(dead_code)]
const S_IFCHR_CHIRHO: u32 = 0o020000;
#[allow(dead_code)]
const S_IFBLK_CHIRHO: u32 = 0o060000;

// ============================================================================
// Hex parsing helper
// ============================================================================

/// Parse an 8-character ASCII hex string into a u32.
fn parse_hex8_chirho(bytes_chirho: &[u8]) -> u32 {
    let mut val_chirho = 0u32;
    for &b_chirho in bytes_chirho.iter().take(8) {
        val_chirho <<= 4;
        val_chirho |= match b_chirho {
            b'0'..=b'9' => (b_chirho - b'0') as u32,
            b'a'..=b'f' => (b_chirho - b'a' + 10) as u32,
            b'A'..=b'F' => (b_chirho - b'A' + 10) as u32,
            _ => 0,
        };
    }
    val_chirho
}

/// Align a value up to the next 4-byte boundary.
fn align4_chirho(val_chirho: usize) -> usize {
    (val_chirho + 3) & !3
}

// ============================================================================
// CPIO parser
// ============================================================================

/// Parse a newc CPIO archive from a byte slice.
///
/// Returns a list of entries. The trailer entry "TRAILER!!!" is excluded.
/// File data can be read from the original slice using
/// `entry.data_offset_chirho..entry.data_offset_chirho + entry.filesize_chirho`.
#[allow(dead_code)]
pub fn parse_cpio_chirho(data_chirho: &[u8]) -> Vec<CpioEntryChirho> {
    let mut entries_chirho = Vec::new();
    let mut offset_chirho = 0usize;

    while offset_chirho + 110 <= data_chirho.len() {
        let header_chirho = &data_chirho[offset_chirho..offset_chirho + 110];

        // Check magic
        if &header_chirho[0..6] != CPIO_NEWC_MAGIC_CHIRHO {
            crate::serial_println_chirho!(
                "[INITRAMFS] Bad CPIO magic at offset {:#x}",
                offset_chirho
            );
            break;
        }

        // Parse header fields (all 8-char hex)
        let ino_chirho = parse_hex8_chirho(&header_chirho[6..14]);
        let mode_chirho = parse_hex8_chirho(&header_chirho[14..22]);
        let uid_chirho = parse_hex8_chirho(&header_chirho[22..30]);
        let gid_chirho = parse_hex8_chirho(&header_chirho[30..38]);
        let nlink_chirho = parse_hex8_chirho(&header_chirho[38..46]);
        let mtime_chirho = parse_hex8_chirho(&header_chirho[46..54]);
        let filesize_chirho = parse_hex8_chirho(&header_chirho[54..62]);
        let dev_major_chirho = parse_hex8_chirho(&header_chirho[62..70]);
        let dev_minor_chirho = parse_hex8_chirho(&header_chirho[70..78]);
        let rdev_major_chirho = parse_hex8_chirho(&header_chirho[78..86]);
        let rdev_minor_chirho = parse_hex8_chirho(&header_chirho[86..94]);
        let namesize_chirho = parse_hex8_chirho(&header_chirho[94..102]) as usize;
        // check_chirho at 102..110 (unused)

        // Extract filename
        let name_start_chirho = offset_chirho + 110;
        let name_end_chirho = name_start_chirho + namesize_chirho;
        if name_end_chirho > data_chirho.len() {
            break;
        }

        // Name includes NUL terminator
        let name_bytes_chirho = &data_chirho[name_start_chirho..name_end_chirho - 1];
        let name_chirho =
            core::str::from_utf8(name_bytes_chirho).unwrap_or("???");

        // Check for trailer
        if name_chirho == "TRAILER!!!" {
            break;
        }

        // Data starts after header + name, aligned to 4
        let data_start_chirho = align4_chirho(name_end_chirho);
        let data_end_chirho = data_start_chirho + filesize_chirho as usize;

        entries_chirho.push(CpioEntryChirho {
            name_chirho: String::from(name_chirho),
            mode_chirho,
            filesize_chirho,
            uid_chirho,
            gid_chirho,
            nlink_chirho,
            mtime_chirho,
            ino_chirho,
            dev_major_chirho,
            dev_minor_chirho,
            rdev_major_chirho,
            rdev_minor_chirho,
            data_offset_chirho: data_start_chirho,
        });

        // Next entry starts after data, aligned to 4
        offset_chirho = align4_chirho(data_end_chirho);
    }

    crate::serial_println_chirho!(
        "[INITRAMFS] Parsed {} CPIO entries",
        entries_chirho.len()
    );

    entries_chirho
}

/// Check if a byte slice starts with a gzip magic header (1f 8b).
#[allow(dead_code)]
pub fn is_gzip_chirho(data_chirho: &[u8]) -> bool {
    data_chirho.len() >= 2 && data_chirho[0] == 0x1F && data_chirho[1] == 0x8B
}

/// Determine the type of an initramfs entry based on its mode bits.
#[allow(dead_code)]
pub fn file_type_str_chirho(mode_chirho: u32) -> &'static str {
    match mode_chirho & S_IFMT_CHIRHO {
        S_IFDIR_CHIRHO => "dir",
        S_IFREG_CHIRHO => "file",
        S_IFLNK_CHIRHO => "symlink",
        S_IFCHR_CHIRHO => "chardev",
        S_IFBLK_CHIRHO => "blkdev",
        _ => "unknown",
    }
}

/// Load and extract an initramfs CPIO image into the root tmpfs.
///
/// `initramfs_data_chirho` is a byte slice containing the (possibly gzip'd)
/// CPIO archive.
#[allow(dead_code)]
pub fn load_initramfs_chirho(initramfs_data_chirho: &[u8]) {
    if initramfs_data_chirho.is_empty() {
        crate::serial_println_chirho!("[INITRAMFS] No initramfs provided");
        return;
    }

    if is_gzip_chirho(initramfs_data_chirho) {
        crate::serial_println_chirho!(
            "[INITRAMFS] Gzip-compressed initramfs detected ({} bytes) — decompression not yet implemented",
            initramfs_data_chirho.len()
        );
        return;
    }

    let entries_chirho = parse_cpio_chirho(initramfs_data_chirho);

    for entry_chirho in &entries_chirho {
        let ftype_chirho = file_type_str_chirho(entry_chirho.mode_chirho);
        crate::serial_println_chirho!(
            "[INITRAMFS] {} {} ({} bytes, mode={:#o})",
            ftype_chirho,
            entry_chirho.name_chirho,
            entry_chirho.filesize_chirho,
            entry_chirho.mode_chirho & 0o7777,
        );
    }

    crate::serial_println_chirho!(
        "[INITRAMFS] Loaded {} entries (VFS population is TODO)",
        entries_chirho.len()
    );
}
