// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! procfs — virtual filesystem exposing kernel and process information.
//!
//! Equivalent to Linux's `fs/proc/`.  Files are generated dynamically on each
//! read; there is no backing storage.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::format;
use spin::Mutex;

use crate::vfs_chirho::{
    DentryChirho, FileChirho, FileOpsChirho, InodeChirho, InodeOpsChirho,
    SuperOpsChirho, SuperblockChirho, StatfsChirho,
    S_IFDIR_CHIRHO, S_IFREG_CHIRHO, S_IFLNK_CHIRHO,
    SEEK_SET_CHIRHO, SEEK_CUR_CHIRHO, SEEK_END_CHIRHO,
};

// ---------------------------------------------------------------------------
// Errno constants (matching Linux values)
// ---------------------------------------------------------------------------

const ENOENT_CHIRHO: i64 = 2;
const EIO_CHIRHO: i64 = 5;
const ENOTDIR_CHIRHO: i64 = 20;
const EISDIR_CHIRHO: i64 = 21;
const EINVAL_CHIRHO: i64 = 22;
const EROFS_CHIRHO: i64 = 30;

// ---------------------------------------------------------------------------
// Proc magic number (matches Linux PROC_SUPER_MAGIC)
// ---------------------------------------------------------------------------

const PROC_SUPER_MAGIC_CHIRHO: u64 = 0x9fa0;

// ---------------------------------------------------------------------------
// Dynamic content generator stored in fs_data_chirho
// ---------------------------------------------------------------------------

/// Wrapper so we can store a `fn() -> String` inside `Box<dyn Any + Send>`.
#[derive(Clone)]
pub struct ProcGeneratorChirho {
    pub generate_chirho: fn() -> String,
}

/// Wrapper for symlink targets stored in `fs_data_chirho`.
struct ProcSymlinkTargetChirho {
    target_chirho: String,
}

// ---------------------------------------------------------------------------
// Content generators for each /proc file
// ---------------------------------------------------------------------------

fn gen_version_chirho() -> String {
    String::from("Linux version 0.1.0 (lineluya@rust) (gcc) #1 SMP\n")
}

fn gen_cpuinfo_chirho() -> String {
    String::from("processor\t: 0\nvendor_id\t: Lineluya\nmodel name\t: Lineluya Virtual CPU\n")
}

fn gen_meminfo_chirho() -> String {
    String::from("MemTotal:       524288 kB\nMemFree:        262144 kB\nMemAvailable:   262144 kB\n")
}

fn gen_uptime_chirho() -> String {
    let ticks_chirho = crate::scheduler_chirho::tick_count_chirho();
    let seconds_chirho = ticks_chirho / 100;
    let hundredths_chirho = ticks_chirho % 100;
    format!("{}.{:02} 0.00\n", seconds_chirho, hundredths_chirho)
}

fn gen_stat_chirho() -> String {
    String::from("cpu  0 0 0 0 0 0 0 0 0 0\n")
}

fn gen_filesystems_chirho() -> String {
    String::from("nodev\ttmpfs\nnodev\tproc\nnodev\tdevtmpfs\n")
}

fn gen_mounts_chirho() -> String {
    String::from("none / tmpfs rw 0 0\nproc /proc proc rw 0 0\n")
}

fn gen_cmdline_chirho() -> String {
    let raw_chirho = crate::cmdline_chirho::raw_cmdline_chirho();
    if raw_chirho.is_empty() {
        String::from("lineluya_chirho\n")
    } else {
        let mut result_chirho = raw_chirho;
        result_chirho.push('\n');
        result_chirho
    }
}

/// Generate `/proc/kmsg` — kernel log ring buffer (E1-015).
fn gen_kmsg_chirho() -> String {
    crate::dmesg_chirho::gen_kmsg_chirho()
}

fn gen_loadavg_chirho() -> String {
    String::from("0.00 0.00 0.00 1/1 1\n")
}

/// A2-015: Generate `/proc/modules` — list of loaded kernel modules.
fn gen_modules_chirho() -> String {
    crate::ko_loader_chirho::gen_proc_modules_chirho()
}

/// Generate `/proc/devices` — character and block device list (A6-010).
fn gen_devices_chirho() -> String {
    String::from("Character devices:\n  1 mem\n  4 tty\n  5 /dev/tty\n  5 /dev/console\n136 pts\n\nBlock devices:\n  8 sd\n259 blkext\n")
}

/// Generate `/proc/interrupts` — IRQ counters (A6-010).
fn gen_interrupts_chirho() -> String {
    String::from("           CPU0\n  0:       0   IO-APIC  0-edge    timer\n  1:       0   IO-APIC  1-edge    i8042\n  8:       0   IO-APIC  8-edge    rtc0\n 14:       0   IO-APIC 14-edge    ata_piix\n")
}

/// Generate `/proc/diskstats` — disk I/O statistics (A6-010).
fn gen_diskstats_chirho() -> String {
    String::from("   8       0 sda 0 0 0 0 0 0 0 0 0 0 0\n")
}

/// Generate `/proc/vmstat` — virtual memory statistics (A6-010).
fn gen_vmstat_chirho() -> String {
    String::from("nr_free_pages 65536\nnr_active_anon 1024\nnr_inactive_anon 512\nnr_active_file 2048\nnr_inactive_file 1024\nnr_dirty 0\nnr_writeback 0\npgfault 0\npgmajfault 0\n")
}

/// Generate `/proc/sys/kernel/hostname` content (A6-010).
fn gen_hostname_chirho() -> String {
    String::from("lineluya\n")
}

/// Generate `/proc/sys/kernel/osrelease` content (A6-010).
fn gen_osrelease_chirho() -> String {
    String::from("0.2.0-lineluya-chirho\n")
}

/// Generate `/proc/net/tcp` — delegates to the networking subsystem.
fn gen_net_tcp_chirho() -> String {
    crate::net_chirho::gen_proc_net_tcp_chirho()
}

/// Generate `/proc/net/udp` — delegates to the networking subsystem.
fn gen_net_udp_chirho() -> String {
    crate::net_chirho::gen_proc_net_udp_chirho()
}

/// Generate `/proc/<pid>/maps` output — the virtual memory layout of the
/// current process, matching the Linux `/proc/PID/maps` format:
///
/// ```text
/// address           perms offset  dev   inode  pathname
/// 00400000-00452000 r-xp 00000000 00:00 0      [text]
/// ```
fn gen_maps_chirho() -> String {
    use crate::mm_chirho::{
        get_or_init_mm_chirho, PROT_READ_CHIRHO, PROT_WRITE_CHIRHO, PROT_EXEC_CHIRHO,
        MAP_PRIVATE_CHIRHO, MAP_SHARED_CHIRHO,
    };
    use core::fmt::Write;

    let mm_lock_chirho = get_or_init_mm_chirho();
    let mm_guard_chirho = mm_lock_chirho.lock();

    let mut output_chirho = String::new();

    if let Some(ref mm_chirho) = *mm_guard_chirho {
        for vma_chirho in &mm_chirho.vmas_chirho {
            // Permission string: r/w/x/p (or s for shared).
            let r_chirho = if vma_chirho.prot_chirho & PROT_READ_CHIRHO != 0 { 'r' } else { '-' };
            let w_chirho = if vma_chirho.prot_chirho & PROT_WRITE_CHIRHO != 0 { 'w' } else { '-' };
            let x_chirho = if vma_chirho.prot_chirho & PROT_EXEC_CHIRHO != 0 { 'x' } else { '-' };
            let p_chirho = if vma_chirho.flags_chirho & MAP_SHARED_CHIRHO != 0 { 's' } else { 'p' };

            // Guess a descriptive name based on the address range.
            let name_chirho = guess_vma_name_chirho(vma_chirho.start_chirho, vma_chirho.end_chirho, vma_chirho.prot_chirho);

            let _ = write!(
                output_chirho,
                "{:08x}-{:08x} {}{}{}{} {:08x} 00:00 0          {}\n",
                vma_chirho.start_chirho,
                vma_chirho.end_chirho,
                r_chirho,
                w_chirho,
                x_chirho,
                p_chirho,
                0u64, // offset (anonymous mappings have 0)
                name_chirho,
            );
        }
    }

    output_chirho
}

/// Heuristically guess a descriptive name for a VMA based on its address
/// range, matching the labels Linux uses in `/proc/PID/maps`.
fn guess_vma_name_chirho(start_chirho: u64, _end_chirho: u64, prot_chirho: u32) -> &'static str {
    use crate::mm_chirho::{PROT_EXEC_CHIRHO, PROT_WRITE_CHIRHO};

    // Stack region: typically near 0x7FFF_FFFF_xxxx.
    if start_chirho >= 0x7FFF_F000_0000 {
        return "[stack]";
    }

    // mmap region: near 0x7F00_xxxx_xxxx.
    if start_chirho >= 0x7F00_0000_0000 {
        return "";
    }

    // Heap / brk region: above the typical ELF load addresses, below mmap.
    // We check if it's above ~16MB (typical ELF top) and below mmap.
    if start_chirho >= 0x0100_0000 && start_chirho < 0x7F00_0000_0000 {
        if prot_chirho & PROT_EXEC_CHIRHO != 0 {
            return "";
        }
        if prot_chirho & PROT_WRITE_CHIRHO != 0 {
            return "[heap]";
        }
        return "";
    }

    // Low addresses: likely ELF text/data segments.
    if prot_chirho & PROT_EXEC_CHIRHO != 0 {
        return "";
    }

    ""
}

// ---------------------------------------------------------------------------
// Inode number allocation
// ---------------------------------------------------------------------------

static NEXT_INO_CHIRHO: spin::Mutex<u64> = spin::Mutex::new(1);

fn alloc_ino_chirho() -> u64 {
    let mut ino_chirho = NEXT_INO_CHIRHO.lock();
    let val_chirho = *ino_chirho;
    *ino_chirho += 1;
    val_chirho
}

// ---------------------------------------------------------------------------
// ProcFileOpsChirho — file operations for /proc regular files
// ---------------------------------------------------------------------------

/// File operations for proc entries that generate content dynamically.
pub struct ProcFileOpsChirho;

pub static PROC_FILE_OPS_CHIRHO: ProcFileOpsChirho = ProcFileOpsChirho;

impl FileOpsChirho for ProcFileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // Generate the content dynamically from the inode's fs_data_chirho.
        let content_chirho = {
            let inode_chirho = file_chirho.inode_chirho.lock();
            if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
                if let Some(gen_chirho) = data_chirho.downcast_ref::<ProcGeneratorChirho>() {
                    (gen_chirho.generate_chirho)()
                } else {
                    return Err(-EIO_CHIRHO);
                }
            } else {
                return Err(-EIO_CHIRHO);
            }
        };

        let bytes_chirho = content_chirho.as_bytes();
        let pos_chirho = file_chirho.pos_chirho as usize;

        if pos_chirho >= bytes_chirho.len() {
            return Ok(0); // EOF
        }

        let remaining_chirho = &bytes_chirho[pos_chirho..];
        let to_copy_chirho = remaining_chirho.len().min(buf_chirho.len());
        buf_chirho[..to_copy_chirho].copy_from_slice(&remaining_chirho[..to_copy_chirho]);
        file_chirho.pos_chirho += to_copy_chirho as u64;

        Ok(to_copy_chirho)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-EROFS_CHIRHO) // procfs is read-only
    }

    fn seek_chirho(
        &self,
        file_chirho: &mut FileChirho,
        offset_chirho: i64,
        whence_chirho: u32,
    ) -> Result<u64, i64> {
        // Generate content to know the size.
        let size_chirho = {
            let inode_chirho = file_chirho.inode_chirho.lock();
            if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
                if let Some(gen_chirho) = data_chirho.downcast_ref::<ProcGeneratorChirho>() {
                    (gen_chirho.generate_chirho)().len() as i64
                } else {
                    0i64
                }
            } else {
                0i64
            }
        };

        let new_pos_chirho = match whence_chirho {
            SEEK_SET_CHIRHO => offset_chirho,
            SEEK_CUR_CHIRHO => file_chirho.pos_chirho as i64 + offset_chirho,
            SEEK_END_CHIRHO => size_chirho + offset_chirho,
            _ => return Err(-EINVAL_CHIRHO),
        };

        if new_pos_chirho < 0 {
            return Err(-EINVAL_CHIRHO);
        }

        file_chirho.pos_chirho = new_pos_chirho as u64;
        Ok(file_chirho.pos_chirho)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-ENOTDIR_CHIRHO)
    }
}

// ---------------------------------------------------------------------------
// ProcDirOpsChirho — file operations for /proc directory
// ---------------------------------------------------------------------------

/// File operations for the /proc directory itself.
pub struct ProcDirOpsChirho;

pub static PROC_DIR_OPS_CHIRHO: ProcDirOpsChirho = ProcDirOpsChirho;

impl FileOpsChirho for ProcDirOpsChirho {
    fn read_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        Err(-EISDIR_CHIRHO)
    }

    fn write_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        Err(-EISDIR_CHIRHO)
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-EISDIR_CHIRHO)
    }

    fn ioctl_chirho(
        &self,
        _file_chirho: &FileChirho,
        _cmd_chirho: u64,
        _arg_chirho: u64,
    ) -> Result<i64, i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn readdir_chirho(
        &self,
        file_chirho: &mut FileChirho,
        callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        // Enumerate children of the /proc dentry stored in fs_data_chirho.
        let inode_chirho = file_chirho.inode_chirho.lock();
        if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
            if let Some(entries_chirho) = data_chirho.downcast_ref::<ProcDirEntriesChirho>() {
                let start_chirho = file_chirho.pos_chirho as usize;
                let mut count_chirho: usize = 0;

                const DT_DIR_CHIRHO: u8 = 4;
                const DT_REG_CHIRHO: u8 = 8;
                const DT_LNK_CHIRHO: u8 = 10;
                const S_IFMT_MASK_CHIRHO: u32 = 0o170000;
                const S_IFDIR_VAL_CHIRHO: u32 = 0o040000;
                const S_IFLNK_VAL_CHIRHO: u32 = 0o120000;

                // Emit "." and ".." first
                let ino_chirho = inode_chirho.ino_chirho;
                if start_chirho == 0 {
                    if !callback_chirho(".", ino_chirho, DT_DIR_CHIRHO) { return Ok(count_chirho); }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }
                if file_chirho.pos_chirho as usize == 1 {
                    if !callback_chirho("..", ino_chirho, DT_DIR_CHIRHO) { return Ok(count_chirho); }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }

                // Emit real entries, skipping already-read ones
                let entry_start_chirho = if start_chirho > 2 { start_chirho - 2 } else { 0 };
                for entry_chirho in entries_chirho.entries_chirho.iter().skip(entry_start_chirho) {
                    let dt_type_chirho = match entry_chirho.mode_chirho & S_IFMT_MASK_CHIRHO {
                        S_IFDIR_VAL_CHIRHO => DT_DIR_CHIRHO,
                        S_IFLNK_VAL_CHIRHO => DT_LNK_CHIRHO,
                        _ => DT_REG_CHIRHO,
                    };
                    if !callback_chirho(
                        &entry_chirho.name_chirho,
                        entry_chirho.ino_chirho,
                        dt_type_chirho,
                    ) {
                        break;
                    }
                    count_chirho += 1;
                    file_chirho.pos_chirho += 1;
                }
                return Ok(count_chirho);
            }
        }
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// ProcDirInodeOpsChirho — inode operations for /proc directory
// ---------------------------------------------------------------------------

/// Inode operations for the /proc directory (lookup, etc.).
struct ProcDirInodeOpsChirho;

static PROC_DIR_INODE_OPS_CHIRHO: ProcDirInodeOpsChirho = ProcDirInodeOpsChirho;

impl InodeOpsChirho for ProcDirInodeOpsChirho {
    fn lookup_chirho(
        &self,
        parent_chirho: &InodeChirho,
        name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        if let Some(ref data_chirho) = parent_chirho.fs_data_chirho {
            if let Some(entries_chirho) = data_chirho.downcast_ref::<ProcDirEntriesChirho>() {
                for entry_chirho in &entries_chirho.entries_chirho {
                    if entry_chirho.name_chirho == name_chirho {
                        return Ok(entry_chirho.inode_chirho.clone());
                    }
                }
            }
        }
        Err(-ENOENT_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EROFS_CHIRHO)
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EROFS_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EROFS_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EROFS_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

// ---------------------------------------------------------------------------
// ProcSymlinkInodeOpsChirho — inode operations for /proc symlinks
// ---------------------------------------------------------------------------

struct ProcSymlinkInodeOpsChirho;

static PROC_SYMLINK_INODE_OPS_CHIRHO: ProcSymlinkInodeOpsChirho = ProcSymlinkInodeOpsChirho;

impl InodeOpsChirho for ProcSymlinkInodeOpsChirho {
    fn lookup_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
            if let Some(target_chirho) = data_chirho.downcast_ref::<ProcSymlinkTargetChirho>() {
                return Ok(target_chirho.target_chirho.clone());
            }
        }
        Err(-EINVAL_CHIRHO)
    }
}

// ---------------------------------------------------------------------------
// NullInodeOpsChirho — no-op inode ops for regular proc files
// ---------------------------------------------------------------------------

struct NullInodeOpsChirho;

static NULL_INODE_OPS_CHIRHO: NullInodeOpsChirho = NullInodeOpsChirho;

impl InodeOpsChirho for NullInodeOpsChirho {
    fn lookup_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-ENOTDIR_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EROFS_CHIRHO)
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EROFS_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EROFS_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EROFS_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<String, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

// ---------------------------------------------------------------------------
// ProcDirEntriesChirho — stored in the directory inode's fs_data_chirho
// ---------------------------------------------------------------------------

/// Holds the list of child entries for a proc directory inode.
#[derive(Clone)]
pub struct ProcDirEntriesChirho {
    entries_chirho: Vec<ProcEntryChirho>,
}

/// One entry in a proc directory.
#[derive(Clone)]
struct ProcEntryChirho {
    name_chirho: String,
    ino_chirho: u64,
    mode_chirho: u32,
    inode_chirho: Arc<InodeChirho>,
}

// ---------------------------------------------------------------------------
// ProcSuperOpsChirho — superblock operations for procfs
// ---------------------------------------------------------------------------

struct ProcSuperOpsChirho;

static PROC_SUPER_OPS_CHIRHO: ProcSuperOpsChirho = ProcSuperOpsChirho;

impl SuperOpsChirho for ProcSuperOpsChirho {
    fn alloc_inode_chirho(&self) -> Arc<InodeChirho> {
        Arc::new(InodeChirho {
            ino_chirho: alloc_ino_chirho(),
            mode_chirho: S_IFREG_CHIRHO | 0o444,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &NULL_INODE_OPS_CHIRHO,
            fs_data_chirho: None,
        })
    }

    fn statfs_chirho(&self) -> Result<StatfsChirho, i64> {
        Ok(StatfsChirho {
            f_type_chirho: PROC_SUPER_MAGIC_CHIRHO,
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

// ---------------------------------------------------------------------------
// Helper: create a regular proc file inode
// ---------------------------------------------------------------------------

fn make_proc_file_chirho(generator_chirho: fn() -> String) -> Arc<InodeChirho> {
    Arc::new(InodeChirho {
        ino_chirho: alloc_ino_chirho(),
        mode_chirho: S_IFREG_CHIRHO | 0o444,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 1,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &NULL_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(ProcGeneratorChirho {
            generate_chirho: generator_chirho,
        })),
    })
}

/// Create a symlink inode pointing to `target_chirho`.
fn make_proc_symlink_chirho(target_chirho: &str) -> Arc<InodeChirho> {
    Arc::new(InodeChirho {
        ino_chirho: alloc_ino_chirho(),
        mode_chirho: S_IFLNK_CHIRHO | 0o777,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: target_chirho.len() as u64,
        nlink_chirho: 1,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &PROC_SYMLINK_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(ProcSymlinkTargetChirho {
            target_chirho: String::from(target_chirho),
        })),
    })
}

// ---------------------------------------------------------------------------
// mount_procfs_chirho — creates and returns the /proc superblock
// ---------------------------------------------------------------------------

/// Mount the procfs virtual filesystem, returning a `SuperblockChirho`
/// with all pre-populated entries.
pub fn mount_procfs_chirho() -> Arc<Mutex<SuperblockChirho>> {
    // Build the individual proc file inodes.
    let version_inode_chirho = make_proc_file_chirho(gen_version_chirho);
    let cpuinfo_inode_chirho = make_proc_file_chirho(gen_cpuinfo_chirho);
    let meminfo_inode_chirho = make_proc_file_chirho(gen_meminfo_chirho);
    let uptime_inode_chirho = make_proc_file_chirho(gen_uptime_chirho);
    let stat_inode_chirho = make_proc_file_chirho(gen_stat_chirho);
    let filesystems_inode_chirho = make_proc_file_chirho(gen_filesystems_chirho);
    let mounts_inode_chirho = make_proc_file_chirho(gen_mounts_chirho);
    let cmdline_inode_chirho = make_proc_file_chirho(gen_cmdline_chirho);
    let loadavg_inode_chirho = make_proc_file_chirho(gen_loadavg_chirho);
    let modules_inode_chirho = make_proc_file_chirho(gen_modules_chirho);
    let devices_inode_chirho = make_proc_file_chirho(gen_devices_chirho);
    let interrupts_inode_chirho = make_proc_file_chirho(gen_interrupts_chirho);
    let diskstats_inode_chirho = make_proc_file_chirho(gen_diskstats_chirho);
    let vmstat_inode_chirho = make_proc_file_chirho(gen_vmstat_chirho);
    let kmsg_inode_chirho = make_proc_file_chirho(gen_kmsg_chirho);
    let self_inode_chirho = make_proc_symlink_chirho("/proc/1");

    // -- /proc/net/ directory (A3-015) --
    let net_tcp_inode_chirho = make_proc_file_chirho(gen_net_tcp_chirho);
    let net_udp_inode_chirho = make_proc_file_chirho(gen_net_udp_chirho);

    let net_entries_chirho = alloc::vec![
        ProcEntryChirho {
            name_chirho: String::from("tcp"),
            ino_chirho: net_tcp_inode_chirho.ino_chirho,
            mode_chirho: net_tcp_inode_chirho.mode_chirho,
            inode_chirho: net_tcp_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("udp"),
            ino_chirho: net_udp_inode_chirho.ino_chirho,
            mode_chirho: net_udp_inode_chirho.mode_chirho,
            inode_chirho: net_udp_inode_chirho.clone(),
        },
    ];

    let net_dir_ino_chirho = alloc_ino_chirho();
    let net_dir_inode_chirho = Arc::new(InodeChirho {
        ino_chirho: net_dir_ino_chirho,
        mode_chirho: S_IFDIR_CHIRHO | 0o555,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &PROC_DIR_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(ProcDirEntriesChirho {
            entries_chirho: net_entries_chirho,
        })),
    });

    // -- /proc/1/ directory (PID 1 process directory) --
    // Contains: maps, status (future), etc.
    let pid1_maps_inode_chirho = make_proc_file_chirho(gen_maps_chirho);

    let pid1_entries_chirho = alloc::vec![
        ProcEntryChirho {
            name_chirho: String::from("maps"),
            ino_chirho: pid1_maps_inode_chirho.ino_chirho,
            mode_chirho: pid1_maps_inode_chirho.mode_chirho,
            inode_chirho: pid1_maps_inode_chirho.clone(),
        },
    ];

    let pid1_ino_chirho = alloc_ino_chirho();
    let pid1_dir_inode_chirho = Arc::new(InodeChirho {
        ino_chirho: pid1_ino_chirho,
        mode_chirho: S_IFDIR_CHIRHO | 0o555,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &PROC_DIR_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(ProcDirEntriesChirho {
            entries_chirho: pid1_entries_chirho,
        })),
    });

    // Build the directory entries list for readdir / lookup.
    let entries_chirho = alloc::vec![
        ProcEntryChirho {
            name_chirho: String::from("version"),
            ino_chirho: version_inode_chirho.ino_chirho,
            mode_chirho: version_inode_chirho.mode_chirho,
            inode_chirho: version_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("cpuinfo"),
            ino_chirho: cpuinfo_inode_chirho.ino_chirho,
            mode_chirho: cpuinfo_inode_chirho.mode_chirho,
            inode_chirho: cpuinfo_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("meminfo"),
            ino_chirho: meminfo_inode_chirho.ino_chirho,
            mode_chirho: meminfo_inode_chirho.mode_chirho,
            inode_chirho: meminfo_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("uptime"),
            ino_chirho: uptime_inode_chirho.ino_chirho,
            mode_chirho: uptime_inode_chirho.mode_chirho,
            inode_chirho: uptime_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("stat"),
            ino_chirho: stat_inode_chirho.ino_chirho,
            mode_chirho: stat_inode_chirho.mode_chirho,
            inode_chirho: stat_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("filesystems"),
            ino_chirho: filesystems_inode_chirho.ino_chirho,
            mode_chirho: filesystems_inode_chirho.mode_chirho,
            inode_chirho: filesystems_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("mounts"),
            ino_chirho: mounts_inode_chirho.ino_chirho,
            mode_chirho: mounts_inode_chirho.mode_chirho,
            inode_chirho: mounts_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("cmdline"),
            ino_chirho: cmdline_inode_chirho.ino_chirho,
            mode_chirho: cmdline_inode_chirho.mode_chirho,
            inode_chirho: cmdline_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("loadavg"),
            ino_chirho: loadavg_inode_chirho.ino_chirho,
            mode_chirho: loadavg_inode_chirho.mode_chirho,
            inode_chirho: loadavg_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("modules"),
            ino_chirho: modules_inode_chirho.ino_chirho,
            mode_chirho: modules_inode_chirho.mode_chirho,
            inode_chirho: modules_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("devices"),
            ino_chirho: devices_inode_chirho.ino_chirho,
            mode_chirho: devices_inode_chirho.mode_chirho,
            inode_chirho: devices_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("interrupts"),
            ino_chirho: interrupts_inode_chirho.ino_chirho,
            mode_chirho: interrupts_inode_chirho.mode_chirho,
            inode_chirho: interrupts_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("diskstats"),
            ino_chirho: diskstats_inode_chirho.ino_chirho,
            mode_chirho: diskstats_inode_chirho.mode_chirho,
            inode_chirho: diskstats_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("vmstat"),
            ino_chirho: vmstat_inode_chirho.ino_chirho,
            mode_chirho: vmstat_inode_chirho.mode_chirho,
            inode_chirho: vmstat_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("kmsg"),
            ino_chirho: kmsg_inode_chirho.ino_chirho,
            mode_chirho: kmsg_inode_chirho.mode_chirho,
            inode_chirho: kmsg_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("self"),
            ino_chirho: self_inode_chirho.ino_chirho,
            mode_chirho: self_inode_chirho.mode_chirho,
            inode_chirho: self_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("net"),
            ino_chirho: net_dir_inode_chirho.ino_chirho,
            mode_chirho: net_dir_inode_chirho.mode_chirho,
            inode_chirho: net_dir_inode_chirho.clone(),
        },
        ProcEntryChirho {
            name_chirho: String::from("1"),
            ino_chirho: pid1_dir_inode_chirho.ino_chirho,
            mode_chirho: pid1_dir_inode_chirho.mode_chirho,
            inode_chirho: pid1_dir_inode_chirho.clone(),
        },
    ];

    // Build the root directory inode for /proc.
    let root_ino_chirho = alloc_ino_chirho();
    let root_inode_chirho = Arc::new(Mutex::new(InodeChirho {
        ino_chirho: root_ino_chirho,
        mode_chirho: S_IFDIR_CHIRHO | 0o555,
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 2,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &PROC_DIR_INODE_OPS_CHIRHO,
        fs_data_chirho: Some(Box::new(ProcDirEntriesChirho { entries_chirho })),
    }));

    // Build child dentries for the dcache tree.
    let mut children_chirho: Vec<Arc<Mutex<DentryChirho>>> = Vec::new();

    let file_inodes_chirho: Vec<(&str, Arc<InodeChirho>)> = alloc::vec![
        ("version", version_inode_chirho),
        ("cpuinfo", cpuinfo_inode_chirho),
        ("meminfo", meminfo_inode_chirho),
        ("uptime", uptime_inode_chirho),
        ("stat", stat_inode_chirho),
        ("filesystems", filesystems_inode_chirho),
        ("mounts", mounts_inode_chirho),
        ("cmdline", cmdline_inode_chirho),
        ("loadavg", loadavg_inode_chirho),
        ("devices", devices_inode_chirho),
        ("interrupts", interrupts_inode_chirho),
        ("diskstats", diskstats_inode_chirho),
        ("vmstat", vmstat_inode_chirho),
        ("modules", modules_inode_chirho),
        ("kmsg", kmsg_inode_chirho),
        ("self", self_inode_chirho),
    ];

    // Build /proc/1/ child dentry for maps.
    let pid1_maps_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("maps"),
        inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
            ino_chirho: pid1_maps_inode_chirho.ino_chirho,
            mode_chirho: pid1_maps_inode_chirho.mode_chirho,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: pid1_maps_inode_chirho.ops_chirho,
            fs_data_chirho: None,
        }))),
        parent_chirho: None,
        children_chirho: Vec::new(),
    }));

    // Build /proc/1/ dentry.
    let pid1_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("1"),
        inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
            ino_chirho: pid1_dir_inode_chirho.ino_chirho,
            mode_chirho: pid1_dir_inode_chirho.mode_chirho,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 2,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: pid1_dir_inode_chirho.ops_chirho,
            fs_data_chirho: None,
        }))),
        parent_chirho: None,
        children_chirho: alloc::vec![pid1_maps_dentry_chirho],
    }));

    for (name_chirho, inode_chirho) in &file_inodes_chirho {
        children_chirho.push(Arc::new(Mutex::new(DentryChirho {
            name_chirho: String::from(*name_chirho),
            inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
                ino_chirho: inode_chirho.ino_chirho,
                mode_chirho: inode_chirho.mode_chirho,
                uid_chirho: 0,
                gid_chirho: 0,
                size_chirho: inode_chirho.size_chirho,
                nlink_chirho: 1,
                atime_chirho: 0,
                mtime_chirho: 0,
                ctime_chirho: 0,
                ops_chirho: inode_chirho.ops_chirho,
                fs_data_chirho: None, // Dentry inodes don't need fs_data
            }))),
            parent_chirho: None, // Will not be set here to avoid circular Arc
            children_chirho: Vec::new(),
        })));
    }

    // Add the /proc/net/ dentry with tcp and udp children (A3-015).
    let net_tcp_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("tcp"),
        inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
            ino_chirho: net_tcp_inode_chirho.ino_chirho,
            mode_chirho: net_tcp_inode_chirho.mode_chirho,
            uid_chirho: 0, gid_chirho: 0, size_chirho: 0, nlink_chirho: 1,
            atime_chirho: 0, mtime_chirho: 0, ctime_chirho: 0,
            ops_chirho: net_tcp_inode_chirho.ops_chirho, fs_data_chirho: None,
        }))),
        parent_chirho: None, children_chirho: Vec::new(),
    }));
    let net_udp_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("udp"),
        inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
            ino_chirho: net_udp_inode_chirho.ino_chirho,
            mode_chirho: net_udp_inode_chirho.mode_chirho,
            uid_chirho: 0, gid_chirho: 0, size_chirho: 0, nlink_chirho: 1,
            atime_chirho: 0, mtime_chirho: 0, ctime_chirho: 0,
            ops_chirho: net_udp_inode_chirho.ops_chirho, fs_data_chirho: None,
        }))),
        parent_chirho: None, children_chirho: Vec::new(),
    }));
    let net_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("net"),
        inode_chirho: Some(Arc::new(Mutex::new(InodeChirho {
            ino_chirho: net_dir_inode_chirho.ino_chirho,
            mode_chirho: net_dir_inode_chirho.mode_chirho,
            uid_chirho: 0, gid_chirho: 0, size_chirho: 0, nlink_chirho: 2,
            atime_chirho: 0, mtime_chirho: 0, ctime_chirho: 0,
            ops_chirho: net_dir_inode_chirho.ops_chirho, fs_data_chirho: None,
        }))),
        parent_chirho: None,
        children_chirho: alloc::vec![net_tcp_dentry_chirho, net_udp_dentry_chirho],
    }));
    children_chirho.push(net_dentry_chirho);

    // Add the /proc/1/ dentry to root children.
    children_chirho.push(pid1_dentry_chirho);

    // Build the root dentry.
    let root_dentry_chirho = Arc::new(Mutex::new(DentryChirho {
        name_chirho: String::from("/"),
        inode_chirho: Some(root_inode_chirho),
        parent_chirho: None,
        children_chirho,
    }));

    // Build and return the superblock.
    Arc::new(Mutex::new(SuperblockChirho {
        fs_type_chirho: "proc",
        root_chirho: root_dentry_chirho,
        flags_chirho: 0,
        ops_chirho: &PROC_SUPER_OPS_CHIRHO,
    }))
}
