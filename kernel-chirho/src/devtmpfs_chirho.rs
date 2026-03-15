// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! devtmpfs — auto-populated device-node filesystem for Lineluya.
//!
//! Built on top of tmpfs, this filesystem is pre-populated with essential
//! device nodes (`/dev/null`, `/dev/zero`, `/dev/urandom`, `/dev/console`,
//! `/dev/tty`) at mount time.  Each device has its own `FileOpsChirho`
//! implementation that provides the expected Linux behaviour.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::vfs_chirho::{
    DentryChirho, FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho,
    S_IFCHR_CHIRHO, S_IFDIR_CHIRHO, SEEK_CUR_CHIRHO, SEEK_END_CHIRHO, SEEK_SET_CHIRHO,
    StatfsChirho, SuperOpsChirho, SuperblockChirho,
};
use crate::syscall_chirho::{
    EINVAL_CHIRHO, ENOSYS_CHIRHO, PRNG_STATE_CHIRHO, xorshift64_chirho,
};
use crate::tmpfs_chirho::{
    TmpfsDataChirho, TmpfsInodeOpsChirho, TMPFS_INODE_OPS_CHIRHO,
};

// ---------------------------------------------------------------------------
// Inode counter (separate from tmpfs to avoid confusion)
// ---------------------------------------------------------------------------

static DEVTMPFS_NEXT_INO_CHIRHO: AtomicU64 = AtomicU64::new(1000);

fn alloc_dev_ino_chirho() -> u64 {
    DEVTMPFS_NEXT_INO_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// DevNullOpsChirho — /dev/null (major 1, minor 3)
// ---------------------------------------------------------------------------

/// File operations for `/dev/null`.
/// - read returns 0 bytes (EOF).
/// - write succeeds silently, discarding all data.
pub struct DevNullOpsChirho;

impl FileOpsChirho for DevNullOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Ok(0) // EOF
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Ok(buf_chirho.len()) // silently discard
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        // /dev/null supports seek but always stays at 0
        let _ = (offset_chirho, whence_chirho);
        file_chirho.pos_chirho = 0;
        Ok(0)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/null file operations.
pub static DEV_NULL_OPS_CHIRHO: DevNullOpsChirho = DevNullOpsChirho;

// ---------------------------------------------------------------------------
// DevZeroOpsChirho — /dev/zero (major 1, minor 5)
// ---------------------------------------------------------------------------

/// File operations for `/dev/zero`.
/// - read fills the buffer with zero bytes.
/// - write succeeds silently, discarding all data.
pub struct DevZeroOpsChirho;

impl FileOpsChirho for DevZeroOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        for byte_chirho in buf_chirho.iter_mut() {
            *byte_chirho = 0;
        }
        Ok(buf_chirho.len())
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Ok(buf_chirho.len()) // silently discard
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        let _ = (offset_chirho, whence_chirho);
        file_chirho.pos_chirho = 0;
        Ok(0)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/zero file operations.
pub static DEV_ZERO_OPS_CHIRHO: DevZeroOpsChirho = DevZeroOpsChirho;

// ---------------------------------------------------------------------------
// DevUrandomOpsChirho — /dev/urandom (major 1, minor 9)
// ---------------------------------------------------------------------------

/// File operations for `/dev/urandom`.
/// - read returns pseudo-random bytes via xorshift64.
/// - write succeeds silently (entropy contribution, ignored here).
pub struct DevUrandomOpsChirho;

impl FileOpsChirho for DevUrandomOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        let mut state_chirho = PRNG_STATE_CHIRHO.load(Ordering::Relaxed);

        // Seed from TSC if not yet initialised
        if state_chirho == 0 {
            let lo_chirho: u32;
            let hi_chirho: u32;
            unsafe {
                core::arch::asm!("rdtsc", out("eax") lo_chirho, out("edx") hi_chirho);
            }
            state_chirho = ((hi_chirho as u64) << 32) | (lo_chirho as u64);
            if state_chirho == 0 {
                state_chirho = 0xDEAD_BEEF_CAFE_BABE;
            }
        }

        let mut offset_chirho: usize = 0;
        while offset_chirho < buf_chirho.len() {
            state_chirho = xorshift64_chirho(state_chirho);
            let bytes_chirho = state_chirho.to_ne_bytes();
            let remaining_chirho = buf_chirho.len() - offset_chirho;
            let chunk_chirho = if remaining_chirho < 8 { remaining_chirho } else { 8 };
            buf_chirho[offset_chirho..offset_chirho + chunk_chirho]
                .copy_from_slice(&bytes_chirho[..chunk_chirho]);
            offset_chirho += chunk_chirho;
        }

        PRNG_STATE_CHIRHO.store(state_chirho, Ordering::Relaxed);
        Ok(buf_chirho.len())
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Ok(buf_chirho.len()) // accept entropy contribution, ignore
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Ok(0) // not seekable, always "position 0"
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-ENOSYS_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of /dev/urandom file operations.
pub static DEV_URANDOM_OPS_CHIRHO: DevUrandomOpsChirho = DevUrandomOpsChirho;

// ---------------------------------------------------------------------------
// DevConsoleOpsChirho — /dev/console (major 5, minor 1) and /dev/tty (5, 0)
// ---------------------------------------------------------------------------

/// File operations for `/dev/console` and `/dev/tty`.
///
/// Delegates read to the global TTY0 line discipline (keyboard input buffer)
/// and write to the TTY0 output path (serial port with OPOST/ONLCR processing).
pub struct DevConsoleOpsChirho;

impl FileOpsChirho for DevConsoleOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        if buf_chirho.is_empty() {
            return Ok(0);
        }

        let tty_chirho = crate::tty_chirho::tty0_chirho();

        // If O_NONBLOCK is set, return -EAGAIN when no data is available.
        let nonblock_chirho = (file_chirho.flags_chirho
            & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;

        if nonblock_chirho {
            let mut ldisc_chirho = tty_chirho.ldisc_chirho.lock();
            if !ldisc_chirho.has_data_chirho() {
                return Err(-crate::syscall_chirho::EAGAIN_CHIRHO);
            }
            let data_chirho = ldisc_chirho.read_chirho(buf_chirho.len());
            let n_chirho = data_chirho.len();
            buf_chirho[..n_chirho].copy_from_slice(&data_chirho);
            return Ok(n_chirho);
        }

        // Blocking read: poll PS/2 keyboard port directly.
        // The keyboard interrupt (IRQ1) doesn't reliably fire in all
        // boot modes, so we poll port 0x60 directly in a spin loop.
        loop {
            if tty_chirho.ldisc_chirho.lock().has_data_chirho() {
                break;
            }
            // Poll PS/2 status port — bit 0 means data available
            let status_chirho: u8 = unsafe {
                x86_64::instructions::port::Port::<u8>::new(0x64).read()
            };
            if status_chirho & 0x01 != 0 {
                // Read the scancode
                let scancode_chirho: u8 = unsafe {
                    x86_64::instructions::port::Port::<u8>::new(0x60).read()
                };
                // Decode using the keyboard state machine
                use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
                // Use a local decoder since we can't easily access the global one
                static POLL_KBD_CHIRHO: spin::Mutex<Option<Keyboard<layouts::Us104Key, ScancodeSet1>>> =
                    spin::Mutex::new(None);
                let mut kbd_guard_chirho = POLL_KBD_CHIRHO.lock();
                let kbd_chirho = kbd_guard_chirho.get_or_insert_with(|| {
                    Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore)
                });
                if let Ok(Some(key_event_chirho)) = kbd_chirho.add_byte(scancode_chirho) {
                    if let Some(key_chirho) = kbd_chirho.process_keyevent(key_event_chirho) {
                        match key_chirho {
                            DecodedKey::Unicode(ch_chirho) => {
                                tty_chirho.input_char_chirho(ch_chirho as u8);
                            }
                            DecodedKey::RawKey(_) => {}
                        }
                    }
                }
            }
            // Brief yield to avoid burning CPU
            core::hint::spin_loop();
        }

        // Data is now available — read it.
        let mut ldisc_chirho = tty_chirho.ldisc_chirho.lock();
        let data_chirho = ldisc_chirho.read_chirho(buf_chirho.len());
        let n_chirho = data_chirho.len();
        buf_chirho[..n_chirho].copy_from_slice(&data_chirho);
        Ok(n_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        // Write through the TTY output path (applies OPOST/ONLCR).
        let tty_chirho = crate::tty_chirho::tty0_chirho();
        tty_chirho.write_output_chirho(buf_chirho);
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE — TTYs are not seekable
    }

    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        // Delegate ioctl to the TTY file ops (TCGETS, TCSETS, TIOCGWINSZ).
        let tty_chirho = crate::tty_chirho::tty0_chirho();
        let tty_ops_chirho = crate::tty_chirho::TtyFileOpsChirho::new_chirho(tty_chirho);
        tty_ops_chirho.ioctl_chirho(file_chirho, cmd_chirho, arg_chirho)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

/// Static instance of /dev/console file operations.
pub static DEV_CONSOLE_OPS_CHIRHO: DevConsoleOpsChirho = DevConsoleOpsChirho;

// ---------------------------------------------------------------------------
// DevtmpfsSuperOpsChirho
// ---------------------------------------------------------------------------

/// Superblock operations for devtmpfs.
pub struct DevtmpfsSuperOpsChirho;

/// devtmpfs magic number.
const DEVTMPFS_MAGIC_CHIRHO: u64 = 0x9FA0; // TMPFS_MAGIC variant

impl SuperOpsChirho for DevtmpfsSuperOpsChirho {
    fn alloc_inode_chirho(&self) -> Arc<InodeChirho> {
        Arc::new(InodeChirho {
            ino_chirho: alloc_dev_ino_chirho(),
            mode_chirho: S_IFCHR_CHIRHO | 0o666,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
            fs_data_chirho: None,
        })
    }

    fn statfs_chirho(&self) -> Result<StatfsChirho, i64> {
        Ok(StatfsChirho {
            f_type_chirho: DEVTMPFS_MAGIC_CHIRHO,
            f_bsize_chirho: 4096,
            f_blocks_chirho: 0,
            f_bfree_chirho: 0,
            f_bavail_chirho: 0,
            f_files_chirho: 0,
            f_ffree_chirho: 0,
            f_namelen_chirho: 255,
        })
    }
}

/// Static instance of devtmpfs superblock operations.
static DEVTMPFS_SUPER_OPS_CHIRHO: DevtmpfsSuperOpsChirho = DevtmpfsSuperOpsChirho;

// ---------------------------------------------------------------------------
// Device node descriptor
// ---------------------------------------------------------------------------

/// Describes a device node to be created at mount time.
struct DevNodeChirho {
    name_chirho: &'static str,
    major_chirho: u32,
    minor_chirho: u32,
    ops_chirho: &'static dyn FileOpsChirho,
}

/// Essential device nodes pre-populated on mount.
static DEV_NODES_CHIRHO: &[DevNodeChirho] = &[
    DevNodeChirho {
        name_chirho: "null",
        major_chirho: 1,
        minor_chirho: 3,
        ops_chirho: &DEV_NULL_OPS_CHIRHO,
    },
    DevNodeChirho {
        name_chirho: "zero",
        major_chirho: 1,
        minor_chirho: 5,
        ops_chirho: &DEV_ZERO_OPS_CHIRHO,
    },
    DevNodeChirho {
        name_chirho: "urandom",
        major_chirho: 1,
        minor_chirho: 9,
        ops_chirho: &DEV_URANDOM_OPS_CHIRHO,
    },
    DevNodeChirho {
        name_chirho: "console",
        major_chirho: 5,
        minor_chirho: 1,
        ops_chirho: &DEV_CONSOLE_OPS_CHIRHO,
    },
    DevNodeChirho {
        name_chirho: "tty",
        major_chirho: 5,
        minor_chirho: 0,
        ops_chirho: &DEV_CONSOLE_OPS_CHIRHO, // same behaviour as console for now
    },
];

// ---------------------------------------------------------------------------
// DevNodeDataChirho — per-device-node private data
// ---------------------------------------------------------------------------

/// Filesystem-private data for a devtmpfs character device node.
/// Stores the major/minor pair and a reference to the device file ops.
pub struct DevNodeDataChirho {
    pub major_chirho: u32,
    pub minor_chirho: u32,
}

// ---------------------------------------------------------------------------
// Mount function
// ---------------------------------------------------------------------------

/// Create and return a mounted devtmpfs instance, pre-populated with
/// the essential device nodes.
pub fn mount_devtmpfs_chirho() -> Arc<Mutex<SuperblockChirho>> {
    // Build the root directory entries
    let mut entries_chirho: Vec<(String, Arc<Mutex<InodeChirho>>)> = Vec::new();

    for node_chirho in DEV_NODES_CHIRHO {
        let dev_chirho = (node_chirho.major_chirho as u64) << 8
            | (node_chirho.minor_chirho as u64);
        let _ = dev_chirho; // stored conceptually; major/minor in DevNodeDataChirho

        let inode_chirho = Arc::new(Mutex::new(InodeChirho {
            ino_chirho: alloc_dev_ino_chirho(),
            mode_chirho: S_IFCHR_CHIRHO | 0o666,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
            fs_data_chirho: Some(Box::new(DevNodeDataChirho {
                major_chirho: node_chirho.major_chirho,
                minor_chirho: node_chirho.minor_chirho,
            })),
        }));

        entries_chirho.push((String::from(node_chirho.name_chirho), inode_chirho));
    }

    let root_inode_chirho = Arc::new(Mutex::new(InodeChirho {
        ino_chirho: alloc_dev_ino_chirho(),
        mode_chirho: S_IFDIR_CHIRHO | 0o755,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &TMPFS_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(Mutex::new(TmpfsDataChirho::DirChirho(entries_chirho)))),
    }));

    let root_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("/dev"),
        inode_chirho: Some(root_inode_chirho),
        parent_chirho: None,
        children_chirho: Vec::new(),
    }));

    Arc::new(Mutex::new(SuperblockChirho {
        fs_type_chirho: "devtmpfs",
        root_chirho: root_dentry_chirho,
        flags_chirho: 0,
        ops_chirho: &DEVTMPFS_SUPER_OPS_CHIRHO,
    }))
}
