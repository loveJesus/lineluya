// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! PTY (pseudo-terminal) subsystem for the Lineluya kernel.
//!
//! Implements the Unix 98 PTY model:
//! - `/dev/ptmx` -- opening creates a new master/slave pair
//! - `/dev/pts/N` -- slave side of PTY pair N
//! - TIOCGPTN ioctl -- get slave number
//! - TIOCSPTLCK ioctl -- unlock slave
//!
//! The master and slave are connected by a shared ring buffer:
//! writing to the master delivers data to the slave's read side,
//! and writing to the slave delivers data to the master's read side.
//!
//! Needed by SSH, screen, tmux, xterm, and any program that allocates
//! a pseudo-terminal.

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use spin::Mutex;

use crate::vfs_chirho::{
    FileChirho, FileOpsChirho,
};
use crate::syscall_chirho::{
    EAGAIN_CHIRHO, EFAULT_CHIRHO, EINVAL_CHIRHO,
    ENOSYS_CHIRHO, EIO_CHIRHO,
};
use crate::tty_chirho::TermiosChirho;
use crate::waitqueue_chirho::WaitQueueChirho;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of PTY pairs.
const MAX_PTYS_CHIRHO: usize = 256;

/// Size of each direction's ring buffer.
const PTY_BUF_SIZE_CHIRHO: usize = 4096;

/// TIOCGPTN -- get PTY number (ioctl).
const TIOCGPTN_CHIRHO: u64 = 0x80045430;

/// TIOCSPTLCK -- lock/unlock PTY (ioctl).
const TIOCSPTLCK_CHIRHO: u64 = 0x40045431;

/// TIOCGPTLCK -- get PTY lock state (ioctl).
const TIOCGPTLCK_CHIRHO: u64 = 0x80045439;

/// TCGETS -- get termios.
const TCGETS_CHIRHO: u64 = 0x5401;

/// TCSETS -- set termios.
const TCSETS_CHIRHO: u64 = 0x5402;

/// TIOCGWINSZ -- get window size.
const TIOCGWINSZ_CHIRHO: u64 = 0x5413;

/// TIOCSWINSZ -- set window size.
const TIOCSWINSZ_CHIRHO: u64 = 0x5414;

/// TIOCSCTTY -- become controlling terminal.
const TIOCSCTTY_CHIRHO: u64 = 0x540E;

/// TIOCGPGRP -- get foreground process group.
const TIOCGPGRP_CHIRHO: u64 = 0x540F;

/// TIOCSPGRP -- set foreground process group.
const TIOCSPGRP_CHIRHO: u64 = 0x5410;

/// Next PTY number to allocate.
static NEXT_PTY_NR_CHIRHO: AtomicU32 = AtomicU32::new(0);

// ============================================================================
// PtyPairChirho -- shared state between master and slave
// ============================================================================

/// Shared state for a PTY master/slave pair.
///
/// Data written to the master goes into `master_to_slave_chirho` (slave reads it).
/// Data written to the slave goes into `slave_to_master_chirho` (master reads it).
pub struct PtyPairChirho {
    /// PTY number (0, 1, 2, ...).
    pub pty_nr_chirho: u32,
    /// Whether the slave side is unlocked (TIOCSPTLCK).
    pub slave_unlocked_chirho: AtomicBool,
    /// Buffer: master writes -> slave reads.
    pub master_to_slave_chirho: Mutex<VecDeque<u8>>,
    /// Buffer: slave writes -> master reads.
    pub slave_to_master_chirho: Mutex<VecDeque<u8>>,
    /// Terminal attributes for this PTY.
    pub termios_chirho: Mutex<TermiosChirho>,
    /// Window size: (rows, cols).
    pub winsize_chirho: Mutex<(u16, u16)>,
    /// Wait queue for master read side.
    pub master_read_wait_chirho: WaitQueueChirho,
    /// Wait queue for slave read side.
    pub slave_read_wait_chirho: WaitQueueChirho,
    /// Whether master side is still open.
    pub master_open_chirho: AtomicBool,
    /// Whether slave side is still open.
    pub slave_open_chirho: AtomicBool,
    /// Foreground process group ID for this PTY.
    pub foreground_pgrp_chirho: AtomicU32,
}

impl PtyPairChirho {
    /// Create a new PTY pair with the given number.
    ///
    /// The slave is auto-unlocked on creation (Linux behaviour since 2.6.29 --
    /// most programs like dropbear call TIOCSPTLCK(0) anyway, and SSH login
    /// breaks if the slave stays locked).
    pub fn new_chirho(nr_chirho: u32) -> Self {
        Self {
            pty_nr_chirho: nr_chirho,
            // Auto-unlock: slave is immediately openable after ptmx open
            slave_unlocked_chirho: AtomicBool::new(true),
            master_to_slave_chirho: Mutex::new(VecDeque::with_capacity(PTY_BUF_SIZE_CHIRHO)),
            slave_to_master_chirho: Mutex::new(VecDeque::with_capacity(PTY_BUF_SIZE_CHIRHO)),
            termios_chirho: Mutex::new(TermiosChirho::default_chirho()),
            winsize_chirho: Mutex::new((24, 80)),
            master_read_wait_chirho: WaitQueueChirho::new_chirho(),
            slave_read_wait_chirho: WaitQueueChirho::new_chirho(),
            master_open_chirho: AtomicBool::new(false), // set true when ptmx open completes
            slave_open_chirho: AtomicBool::new(false),
            foreground_pgrp_chirho: AtomicU32::new(0),
        }
    }
}

// ============================================================================
// Global PTY registry
// ============================================================================

/// Global registry of active PTY pairs, indexed by PTY number.
static PTY_TABLE_CHIRHO: Mutex<Vec<Option<Arc<PtyPairChirho>>>> = Mutex::new(Vec::new());

/// Initialise the PTY table (called once at boot).
pub fn init_pty_chirho() {
    let mut table_chirho = PTY_TABLE_CHIRHO.lock();
    table_chirho.resize_with(MAX_PTYS_CHIRHO, || None);
    crate::serial_println_chirho!("[OK] PTY subsystem initialized ({} slots)", MAX_PTYS_CHIRHO);
}

/// Allocate a new PTY pair.  Returns the pair Arc and its number.
fn alloc_pty_pair_chirho() -> Result<Arc<PtyPairChirho>, i64> {
    let nr_chirho = NEXT_PTY_NR_CHIRHO.fetch_add(1, Ordering::SeqCst);
    if nr_chirho as usize >= MAX_PTYS_CHIRHO {
        NEXT_PTY_NR_CHIRHO.fetch_sub(1, Ordering::SeqCst);
        return Err(-ENOSYS_CHIRHO);
    }
    let pair_chirho = Arc::new(PtyPairChirho::new_chirho(nr_chirho));
    {
        let mut table_chirho = PTY_TABLE_CHIRHO.lock();
        table_chirho[nr_chirho as usize] = Some(pair_chirho.clone());
    }
    Ok(pair_chirho)
}

/// Look up a PTY pair by number.
pub fn get_pty_chirho(nr_chirho: u32) -> Option<Arc<PtyPairChirho>> {
    let table_chirho = PTY_TABLE_CHIRHO.lock();
    table_chirho.get(nr_chirho as usize).and_then(|slot_chirho| slot_chirho.clone())
}

/// Set the foreground process group on ALL allocated PTY pairs.
/// Used during shell re-exec to fix terminal ownership.
pub fn set_all_foreground_pgrp_chirho(pgrp_chirho: u32) {
    let table_chirho = PTY_TABLE_CHIRHO.lock();
    for slot_chirho in table_chirho.iter() {
        if let Some(pair_chirho) = slot_chirho {
            pair_chirho.foreground_pgrp_chirho.store(pgrp_chirho, core::sync::atomic::Ordering::SeqCst);
        }
    }
}

// ============================================================================
// PtmxFileOpsChirho -- /dev/ptmx file operations
// ============================================================================

/// File operations for `/dev/ptmx`.
///
/// Opening /dev/ptmx allocates a new PTY pair and returns the master fd.
/// The PTY number is stored as the inode number for later ioctl queries.
pub struct PtmxFileOpsChirho;

impl PtmxFileOpsChirho {
    /// Called when userspace opens /dev/ptmx.
    /// Allocates a PTY pair and returns a PtyMasterOps for the new pair.
    ///
    /// Sets `master_open_chirho = true` and auto-unlocks the slave so that
    /// `/dev/pts/N` can be opened immediately (Linux 2.6.29+ behaviour).
    pub fn open_ptmx_chirho() -> Result<(Arc<PtyPairChirho>, &'static dyn FileOpsChirho), i64> {
        let pair_chirho = alloc_pty_pair_chirho()?;
        // Mark master as open
        pair_chirho.master_open_chirho.store(true, Ordering::SeqCst);
        // Slave is already auto-unlocked in new_chirho(), but be explicit
        pair_chirho.slave_unlocked_chirho.store(true, Ordering::SeqCst);
        crate::serial_println_chirho!(
            "[PTY] opened ptmx -> pty_nr={}, slave auto-unlocked",
            pair_chirho.pty_nr_chirho
        );
        // We return a static ref to the master ops singleton; the pair
        // is looked up via the inode's ino_chirho field (set to pty_nr).
        Ok((pair_chirho, &PTY_MASTER_OPS_CHIRHO))
    }
}

impl FileOpsChirho for PtmxFileOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // Reading /dev/ptmx itself is not meaningful -- the real read
        // goes through PtyMasterOpsChirho after open returns a master fd.
        Err(-EIO_CHIRHO)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-EIO_CHIRHO)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        // Positive errno in Err() -- dispatch layer negates it
        Err(EINVAL_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

/// Static instance of /dev/ptmx operations.
pub static PTMX_OPS_CHIRHO: PtmxFileOpsChirho = PtmxFileOpsChirho;

// ============================================================================
// PtyMasterOpsChirho -- master side of a PTY pair
// ============================================================================

/// File operations for the master side of a PTY pair.
///
/// The PTY pair is identified by the inode's `ino_chirho` field, which
/// stores the PTY number.
pub struct PtyMasterOpsChirho;

impl PtyMasterOpsChirho {
    /// Helper: get the PTY pair from a file's inode.
    fn get_pair_chirho(file_chirho: &FileChirho) -> Option<Arc<PtyPairChirho>> {
        let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
        get_pty_chirho(ino_chirho as u32)
    }
}

impl FileOpsChirho for PtyMasterOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        if buf_chirho.is_empty() {
            return Ok(0);
        }
        let pair_chirho = Self::get_pair_chirho(file_chirho).ok_or(-EIO_CHIRHO)?;

        // Read from slave_to_master buffer (what the slave wrote).
        let mut ring_chirho = pair_chirho.slave_to_master_chirho.lock();
        if ring_chirho.is_empty() {
            // Non-blocking: check file flags
            let nonblock_chirho = (file_chirho.flags_chirho
                & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;
            if nonblock_chirho {
                return Err(-EAGAIN_CHIRHO);
            }
            // For blocking reads in our simple implementation, return 0
            // (the scheduler would need to block here in a real impl).
            return Ok(0);
        }

        let count_chirho = buf_chirho.len().min(ring_chirho.len());
        let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
        for i_chirho in 0..count_chirho {
            let byte_chirho = match ring_chirho.pop_front() {
                Some(byte_chirho) => byte_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[PTY] master read underflow on inode {}",
                        ino_chirho
                    );
                    return Ok(i_chirho);
                }
            };
            buf_chirho[i_chirho] = byte_chirho;
        }
        Ok(count_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let pair_chirho = Self::get_pair_chirho(file_chirho).ok_or(-EIO_CHIRHO)?;

        // One-shot diagnostic
        use core::sync::atomic::{AtomicU64, Ordering};
        static PTY_WR_CNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
        let pwc_chirho = PTY_WR_CNT_CHIRHO.fetch_add(1, Ordering::Relaxed);
        if pwc_chirho < 5 {
            crate::serial_println_chirho!(
                "[PTY-WRITE] #{} pty_nr={} len={}",
                pwc_chirho, pair_chirho.pty_nr_chirho, buf_chirho.len(),
            );
        }

        // Write to master_to_slave buffer (slave will read it).
        let mut ring_chirho = pair_chirho.master_to_slave_chirho.lock();
        for &byte_chirho in buf_chirho {
            if ring_chirho.len() < PTY_BUF_SIZE_CHIRHO {
                ring_chirho.push_back(byte_chirho);
            } else {
                break;
            }
        }
        drop(ring_chirho);
        // Wake any slave readers
        crate::waitqueue_chirho::wake_up_chirho(&pair_chirho.slave_read_wait_chirho);
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE
    }

    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        let pair_chirho = {
            let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
            get_pty_chirho(ino_chirho as u32).ok_or(-EIO_CHIRHO)?
        };
        pty_ioctl_chirho(&pair_chirho, cmd_chirho, arg_chirho)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

/// Static instance of PTY master operations.
pub static PTY_MASTER_OPS_CHIRHO: PtyMasterOpsChirho = PtyMasterOpsChirho;

// ============================================================================
// PtySlaveOpsChirho -- slave side of a PTY pair
// ============================================================================

/// File operations for the slave side of a PTY pair (`/dev/pts/N`).
pub struct PtySlaveOpsChirho;

impl PtySlaveOpsChirho {
    /// Helper: get the PTY pair from a file's inode.
    fn get_pair_chirho(file_chirho: &FileChirho) -> Option<Arc<PtyPairChirho>> {
        let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
        get_pty_chirho(ino_chirho as u32)
    }
}

impl FileOpsChirho for PtySlaveOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        if buf_chirho.is_empty() {
            return Ok(0);
        }
        let pair_chirho = Self::get_pair_chirho(file_chirho).ok_or(-EIO_CHIRHO)?;

        // Read from master_to_slave buffer (what the master wrote).
        let mut ring_chirho = pair_chirho.master_to_slave_chirho.lock();
        if ring_chirho.is_empty() {
            let nonblock_chirho = (file_chirho.flags_chirho
                & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0;
            if nonblock_chirho {
                return Err(-EAGAIN_CHIRHO);
            }
            // Blocking: return 0 for now (scheduler integration needed).
            return Ok(0);
        }

        let count_chirho = buf_chirho.len().min(ring_chirho.len());
        let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
        for i_chirho in 0..count_chirho {
            let byte_chirho = match ring_chirho.pop_front() {
                Some(byte_chirho) => byte_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[PTY] slave read underflow on inode {}",
                        ino_chirho
                    );
                    return Ok(i_chirho);
                }
            };
            buf_chirho[i_chirho] = byte_chirho;
        }
        Ok(count_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let pair_chirho = Self::get_pair_chirho(file_chirho).ok_or(-EIO_CHIRHO)?;

        // Write to slave_to_master buffer (master will read it).
        let mut ring_chirho = pair_chirho.slave_to_master_chirho.lock();
        for &byte_chirho in buf_chirho {
            if ring_chirho.len() < PTY_BUF_SIZE_CHIRHO {
                ring_chirho.push_back(byte_chirho);
            } else {
                break;
            }
        }
        drop(ring_chirho);
        // Wake any master readers
        crate::waitqueue_chirho::wake_up_chirho(&pair_chirho.master_read_wait_chirho);
        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-29) // ESPIPE
    }

    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        let pair_chirho = {
            let ino_chirho = file_chirho.inode_chirho.lock().ino_chirho;
            get_pty_chirho(ino_chirho as u32).ok_or(-EIO_CHIRHO)?
        };
        pty_ioctl_chirho(&pair_chirho, cmd_chirho, arg_chirho)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-20) // ENOTDIR
    }
}

/// Static instance of PTY slave operations.
pub static PTY_SLAVE_OPS_CHIRHO: PtySlaveOpsChirho = PtySlaveOpsChirho;

// ============================================================================
// PtsDir file operations -- /dev/pts directory listing
// ============================================================================

/// File operations for the /dev/pts directory.
pub struct PtsDirOpsChirho;

impl FileOpsChirho for PtsDirOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Err(-21) // EISDIR
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-21) // EISDIR
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Ok(0)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        // Positive errno in Err() -- dispatch layer negates it
        Err(EINVAL_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        let mut count_chirho = 0usize;
        // Emit . and ..
        if !callback_chirho(".", 1, 4 /* DT_DIR */) {
            return Ok(count_chirho);
        }
        count_chirho += 1;
        if !callback_chirho("..", 1, 4) {
            return Ok(count_chirho);
        }
        count_chirho += 1;

        // Emit active PTY slave entries
        let table_chirho = PTY_TABLE_CHIRHO.lock();
        for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
            if slot_chirho.is_some() {
                use alloc::format;
                let name_chirho = format!("{}", idx_chirho);
                if !callback_chirho(&name_chirho, (idx_chirho + 2) as u64, 2 /* DT_CHR */) {
                    break;
                }
                count_chirho += 1;
            }
        }
        Ok(count_chirho)
    }
}

/// Static instance of /dev/pts directory operations.
pub static PTS_DIR_OPS_CHIRHO: PtsDirOpsChirho = PtsDirOpsChirho;

// ============================================================================
// Shared ioctl handler
// ============================================================================

/// Common ioctl handler for both master and slave sides of a PTY.
fn pty_ioctl_chirho(
    pair_chirho: &PtyPairChirho,
    cmd_chirho: u64,
    arg_chirho: u64,
) -> Result<i64, i64> {
    match cmd_chirho {
        TIOCGPTN_CHIRHO => {
            // Return PTY number in the u32 at arg_chirho
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let nr_chirho = pair_chirho.pty_nr_chirho;
            if crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                &nr_chirho.to_ne_bytes(),
                4,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            Ok(0)
        }

        TIOCSPTLCK_CHIRHO => {
            // Lock/unlock slave: read int from arg
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let mut val_buf_chirho = [0u8; 4];
            if crate::uaccess_chirho::copy_from_user_chirho(
                &mut val_buf_chirho,
                arg_chirho,
                4,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            let lock_val_chirho = i32::from_ne_bytes(val_buf_chirho);
            pair_chirho.slave_unlocked_chirho.store(
                lock_val_chirho == 0,
                Ordering::SeqCst,
            );
            Ok(0)
        }

        TIOCGPTLCK_CHIRHO => {
            // Get lock state
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let locked_chirho: i32 = if pair_chirho.slave_unlocked_chirho.load(Ordering::SeqCst) {
                0
            } else {
                1
            };
            if crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                &locked_chirho.to_ne_bytes(),
                4,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            Ok(0)
        }

        TCGETS_CHIRHO => {
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let termios_chirho = pair_chirho.termios_chirho.lock();
            let size_chirho = core::mem::size_of::<TermiosChirho>();
            let src_chirho = unsafe {
                core::slice::from_raw_parts(
                    &*termios_chirho as *const TermiosChirho as *const u8,
                    size_chirho,
                )
            };
            if crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                src_chirho,
                size_chirho,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            Ok(0)
        }

        TCSETS_CHIRHO => {
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let size_chirho = core::mem::size_of::<TermiosChirho>();
            let mut buf_chirho = [0u8; 128];
            if crate::uaccess_chirho::copy_from_user_chirho(
                &mut buf_chirho[..size_chirho],
                arg_chirho,
                size_chirho,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            let new_termios_chirho = unsafe {
                core::ptr::read(buf_chirho.as_ptr() as *const TermiosChirho)
            };
            *pair_chirho.termios_chirho.lock() = new_termios_chirho;
            Ok(0)
        }

        TIOCGWINSZ_CHIRHO => {
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let (rows_chirho, cols_chirho) = *pair_chirho.winsize_chirho.lock();
            let mut buf_chirho = [0u8; 8];
            buf_chirho[0..2].copy_from_slice(&rows_chirho.to_ne_bytes());
            buf_chirho[2..4].copy_from_slice(&cols_chirho.to_ne_bytes());
            // xpixel, ypixel = 0
            if crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                &buf_chirho,
                8,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            Ok(0)
        }

        TIOCSWINSZ_CHIRHO => {
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let mut buf_chirho = [0u8; 8];
            if crate::uaccess_chirho::copy_from_user_chirho(
                &mut buf_chirho,
                arg_chirho,
                8,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            let rows_chirho = u16::from_ne_bytes([buf_chirho[0], buf_chirho[1]]);
            let cols_chirho = u16::from_ne_bytes([buf_chirho[2], buf_chirho[3]]);
            *pair_chirho.winsize_chirho.lock() = (rows_chirho, cols_chirho);
            Ok(0)
        }

        TIOCGPGRP_CHIRHO => {
            // Return the foreground process group stored on this PTY
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let pgid_chirho: i32 = pair_chirho.foreground_pgrp_chirho.load(Ordering::SeqCst) as i32;
            if crate::uaccess_chirho::copy_to_user_chirho(
                arg_chirho,
                &pgid_chirho.to_ne_bytes(),
                4,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            Ok(0)
        }

        TIOCSPGRP_CHIRHO => {
            // Read the u32 pgrp from userspace and store it
            if arg_chirho == 0 {
                return Err(EFAULT_CHIRHO);
            }
            let mut val_buf_chirho = [0u8; 4];
            if crate::uaccess_chirho::copy_from_user_chirho(
                &mut val_buf_chirho,
                arg_chirho,
                4,
            ).is_err() {
                return Err(EFAULT_CHIRHO);
            }
            let pgrp_val_chirho = u32::from_ne_bytes(val_buf_chirho);
            pair_chirho.foreground_pgrp_chirho.store(pgrp_val_chirho, Ordering::SeqCst);
            Ok(0)
        }

        TIOCSCTTY_CHIRHO => {
            // Become controlling TTY -- accept silently for SSH login.
            // In a full implementation this would associate the TTY with
            // the calling session, but for now success is sufficient.
            Ok(0)
        }

        _ => Err(EINVAL_CHIRHO),
    }
}
