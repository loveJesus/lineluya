// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Filesystem syscall implementation layer for Lineluya.
//!
//! Connects the syscall dispatch to the VFS layer.  Manages the root
//! filesystem, mount points, path resolution, and the per-process (currently
//! global) file descriptor table.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::vfs_chirho::{
    FdTableChirho, FileChirho, FileOpsChirho, InodeChirho,
    O_CREAT_CHIRHO, O_DIRECTORY_CHIRHO, O_RDONLY_CHIRHO, O_WRONLY_CHIRHO,
    S_IFCHR_CHIRHO, S_IFDIR_CHIRHO, SuperblockChirho,
};
use crate::syscall_chirho::{
    EBADF_CHIRHO, EFAULT_CHIRHO, ENOENT_CHIRHO, ENOTDIR_CHIRHO,
};
use crate::tmpfs_chirho::TMPFS_FILE_OPS_CHIRHO;
use crate::procfs_chirho::{PROC_FILE_OPS_CHIRHO as PROCFS_FILE_OPS_CHIRHO, PROC_DIR_OPS_CHIRHO as PROCFS_DIR_OPS_CHIRHO};
use crate::devtmpfs_chirho::{
    DevNodeDataChirho, DEV_CONSOLE_OPS_CHIRHO, DEV_NULL_OPS_CHIRHO,
    DEV_URANDOM_OPS_CHIRHO, DEV_ZERO_OPS_CHIRHO,
};
use crate::uaccess_chirho::{copy_from_user_chirho, copy_to_user_chirho, read_user_string_chirho};

// ============================================================================
// Mount point structure
// ============================================================================

/// A mount point binding a path to a superblock.
pub struct MountPointChirho {
    /// Absolute path where this filesystem is mounted (e.g. "/dev", "/proc").
    pub path_chirho: String,
    /// The superblock for the mounted filesystem.
    pub superblock_chirho: Arc<Mutex<SuperblockChirho>>,
}

// ============================================================================
// Global state
// ============================================================================

/// The root tmpfs superblock.
static ROOT_FS_CHIRHO: Mutex<Option<Arc<Mutex<SuperblockChirho>>>> = Mutex::new(None);

/// Table of mount points (checked during path resolution).
pub static MOUNT_TABLE_CHIRHO: Mutex<Vec<MountPointChirho>> = Mutex::new(Vec::new());

/// Global file descriptor table (single-process for now).
pub static GLOBAL_FD_TABLE_CHIRHO: Mutex<Option<FdTableChirho>> = Mutex::new(None);

/// Maximum number of file descriptors.
const MAX_FDS_CHIRHO: usize = 256;

/// AT_FDCWD sentinel value (Linux).
const AT_FDCWD_CHIRHO: i64 = -100;

// ============================================================================
// Initialisation
// ============================================================================

/// Initialise the filesystem layer.
///
/// - Creates the root tmpfs.
/// - Creates /dev, /proc, /tmp directories.
/// - Mounts devtmpfs on /dev and procfs on /proc.
/// - Sets up stdin/stdout/stderr (fd 0, 1, 2) pointing to /dev/console.
pub fn init_fs_chirho() {
    // 1. Create root tmpfs
    let root_sb_chirho = crate::tmpfs_chirho::mount_tmpfs_chirho();

    // 2. Create /dev, /proc, /tmp, /bin, /sbin directories in the root
    {
        let sb_guard_chirho = root_sb_chirho.lock();
        let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
        if let Some(ref root_inode_arc_chirho) = root_dentry_chirho.inode_chirho {
            let root_inode_chirho = root_inode_arc_chirho.lock();
            // Create standard directories on tmpfs root.
            // These take precedence over ext4 entries when the tmpfs
            // live walk finds them first, avoiding ext4 path resolution
            // overhead for common directories.
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "dev", 0o755);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "proc", 0o555);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "tmp", 0o1777);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "run", 0o755);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "var", 0o755);
            // Create /bin and /sbin for BusyBox applet lookups
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "bin", 0o755);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "sbin", 0o755);
            let _ = root_inode_chirho.ops_chirho.mkdir_chirho(&root_inode_chirho, "usr", 0o755);
        }
    }

    // 2b. Populate /bin with dummy executable entries for all BusyBox applets.
    //
    // When ash does stat("/bin/ls"), the VFS will find these entries and
    // return S_IFREG|0o755 (executable). When ash then does fork+execve
    // ("/bin/ls"), sys_execve_chirho recognises the applet name and loads
    // the embedded BusyBox binary with argv[0]="ls".
    //
    // We access the real /bin inode (with fs_data) through the tmpfs
    // directory entry list, since lookup_chirho returns a shallow copy
    // without fs_data.
    populate_bin_applets_chirho(&root_sb_chirho);

    // 3. Store root fs
    {
        let mut root_guard_chirho = ROOT_FS_CHIRHO.lock();
        *root_guard_chirho = Some(root_sb_chirho.clone());
    }

    // 4. Mount devtmpfs on /dev
    let dev_sb_chirho = crate::devtmpfs_chirho::mount_devtmpfs_chirho();
    {
        let mut mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        mounts_chirho.push(MountPointChirho {
            path_chirho: String::from("/dev"),
            superblock_chirho: dev_sb_chirho,
        });
    }

    // 5. Mount procfs on /proc
    let proc_sb_chirho = crate::procfs_chirho::mount_procfs_chirho();
    {
        let mut mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        mounts_chirho.push(MountPointChirho {
            path_chirho: String::from("/proc"),
            superblock_chirho: proc_sb_chirho,
        });
    }

    // 5b. Mount sysfs on /sys
    let sys_sb_chirho = crate::sysfs_chirho::mount_sysfs_chirho();
    {
        let mut mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        mounts_chirho.push(MountPointChirho {
            path_chirho: String::from("/sys"),
            superblock_chirho: sys_sb_chirho,
        });
    }

    // 5c. Mount tmpfs on /tmp, /run, /var (writable storage even when / is ext4 read-only)
    for dir_chirho in &["/tmp", "/run", "/var"] {
        let sb_chirho = crate::tmpfs_chirho::mount_tmpfs_chirho();
        let mut mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        mounts_chirho.push(MountPointChirho {
            path_chirho: String::from(*dir_chirho),
            superblock_chirho: sb_chirho,
        });
    }

    // 6. Initialise the FD table with stdin/stdout/stderr -> /dev/console
    {
        let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        let mut fd_table_chirho = FdTableChirho::new_chirho(MAX_FDS_CHIRHO);

        // Create a dummy console inode for stdin/stdout/stderr
        let console_inode_chirho = Arc::new(Mutex::new(InodeChirho {
            ino_chirho: 9999,
            mode_chirho: S_IFCHR_CHIRHO | 0o666,
            uid_chirho: 0,
            gid_chirho: 0,
            size_chirho: 0,
            nlink_chirho: 1,
            atime_chirho: 0,
            mtime_chirho: 0,
            ctime_chirho: 0,
            ops_chirho: &crate::tmpfs_chirho::TMPFS_INODE_OPS_CHIRHO,
            fs_data_chirho: None,
        }));

        // fd 0 = stdin  (console, read)
        let stdin_file_chirho = Arc::new(Mutex::new(FileChirho {
            inode_chirho: console_inode_chirho.clone(),
            pos_chirho: 0,
            flags_chirho: O_RDONLY_CHIRHO,
            ops_chirho: &DEV_CONSOLE_OPS_CHIRHO,
        }));
        fd_table_chirho.fds_chirho[0] = Some(stdin_file_chirho);

        // fd 1 = stdout (console, write)
        let stdout_file_chirho = Arc::new(Mutex::new(FileChirho {
            inode_chirho: console_inode_chirho.clone(),
            pos_chirho: 0,
            flags_chirho: O_WRONLY_CHIRHO,
            ops_chirho: &DEV_CONSOLE_OPS_CHIRHO,
        }));
        fd_table_chirho.fds_chirho[1] = Some(stdout_file_chirho);

        // fd 2 = stderr (console, write)
        let stderr_file_chirho = Arc::new(Mutex::new(FileChirho {
            inode_chirho: console_inode_chirho,
            pos_chirho: 0,
            flags_chirho: O_WRONLY_CHIRHO,
            ops_chirho: &DEV_CONSOLE_OPS_CHIRHO,
        }));
        fd_table_chirho.fds_chirho[2] = Some(stderr_file_chirho);

        *fd_table_guard_chirho = Some(fd_table_chirho);
    }

    crate::serial_println_chirho!("[OK] Filesystem layer initialized (root tmpfs + /dev + /proc + /bin applets)");
}

/// Populate `/bin` with BusyBox applet symlinks.
///
/// TODO(fs-dyn-001): This creates hardcoded applet entries at boot time.
/// The proper approach is to have the BusyBox binary create these itself
/// during first boot (like `busybox --install -s /bin`), or to use an
/// initramfs/init script that runs the install command.
///
/// For now this is needed because our ext4 root doesn't have /bin/ls etc.
/// and the embedded BusyBox binary needs applet entries in the VFS.
///
/// Walks the tmpfs root directory entries to find the real `/bin` inode
/// (with its `fs_data_chirho` intact), then creates regular file entries
/// with mode 0o755 for each BusyBox applet name.
fn populate_bin_applets_chirho(root_sb_chirho: &Arc<Mutex<crate::vfs_chirho::SuperblockChirho>>) {
    use crate::tmpfs_chirho::TmpfsDataChirho;

    // BusyBox applet names — must match the list in process_chirho.rs
    let applets_chirho: &[&str] = &[
        "ls", "cat", "cp", "mv", "rm", "mkdir", "rmdir", "chmod",
        "chown", "ln", "touch", "head", "tail", "wc", "grep", "sed",
        "awk", "sort", "uniq", "tr", "cut", "find", "xargs", "tee",
        "du", "df", "mount", "umount", "ps", "kill", "sleep",
        "date", "uname", "id", "whoami", "hostname", "env",
        "printenv", "expr", "test", "true", "false", "yes",
        "sh", "ash", "busybox", "vi", "ping", "wget", "nc",
        "tar", "gzip", "gunzip", "dd", "hexdump", "od",
        "dmesg", "free", "uptime", "stat", "readlink",
        "basename", "dirname", "realpath", "seq", "printf",
        "echo", "clear", "reset", "stty", "tty",
    ];

    // Step 1: Get the root inode from the superblock.
    let sb_guard_chirho = root_sb_chirho.lock();
    let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
    let root_inode_arc_chirho = match root_dentry_chirho.inode_chirho {
        Some(ref arc_chirho) => arc_chirho.clone(),
        None => return,
    };
    drop(root_dentry_chirho);
    drop(sb_guard_chirho);

    // Step 2: Walk the root directory's tmpfs entries to find the real /bin
    // inode (the one stored in the entries vector, which has fs_data).
    let bin_inode_arc_chirho = {
        let root_locked_chirho = root_inode_arc_chirho.lock();
        let root_data_chirho = match root_locked_chirho
            .fs_data_chirho
            .as_ref()
            .and_then(|d_chirho| d_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>())
        {
            Some(d_chirho) => d_chirho,
            None => return,
        };
        let data_guard_chirho = root_data_chirho.lock();
        match &*data_guard_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                let mut found_chirho: Option<Arc<Mutex<crate::vfs_chirho::InodeChirho>>> = None;
                for (name_chirho, inode_arc_chirho) in entries_chirho.iter() {
                    if name_chirho == "bin" {
                        found_chirho = Some(inode_arc_chirho.clone());
                        break;
                    }
                }
                match found_chirho {
                    Some(arc_chirho) => arc_chirho,
                    None => return,
                }
            }
            _ => return,
        }
    };

    // Step 3: Create executable file entries inside /bin.
    // Lock the /bin inode and call create_chirho on it for each applet.
    let bin_locked_chirho = bin_inode_arc_chirho.lock();
    let mut count_chirho: usize = 0;
    for applet_name_chirho in applets_chirho {
        if bin_locked_chirho
            .ops_chirho
            .create_chirho(&bin_locked_chirho, applet_name_chirho, 0o755)
            .is_ok()
        {
            count_chirho += 1;
        }
    }

    crate::serial_println_chirho!(
        "[FS] Created {} BusyBox applet entries in /bin",
        count_chirho
    );
}

// ============================================================================
// Path resolution
// ============================================================================

/// Look up the file-operations vtable for a character device based on
/// major/minor numbers.
fn dev_file_ops_chirho(major_chirho: u32, minor_chirho: u32) -> &'static dyn FileOpsChirho {
    match (major_chirho, minor_chirho) {
        (1, 3) => &DEV_NULL_OPS_CHIRHO,
        (1, 5) => &DEV_ZERO_OPS_CHIRHO,
        (1, 9) => &DEV_URANDOM_OPS_CHIRHO,
        (5, 0) | (5, 1) => &DEV_CONSOLE_OPS_CHIRHO,
        (5, 2) => &crate::pty_chirho::PTMX_OPS_CHIRHO,       // /dev/ptmx
        (136, _) => &crate::pty_chirho::PTY_SLAVE_OPS_CHIRHO, // /dev/pts/N (major 136)
        (29, 0) => &crate::fb_device_chirho::FB_DEVICE_OPS_CHIRHO, // /dev/fb0
        _ => &TMPFS_FILE_OPS_CHIRHO, // fallback
    }
}

/// Clone `fs_data_chirho` from an inode, handling all known filesystem types.
///
/// Supports: `ProcGeneratorChirho`, `ProcDirEntriesChirho`, `DevNodeDataChirho`,
/// and `Mutex<TmpfsDataChirho>`.  Returns `None` for unrecognised or absent data.
fn clone_fs_data_chirho(
    fs_data_chirho: &Option<Box<dyn core::any::Any + Send>>,
) -> Option<Box<dyn core::any::Any + Send>> {
    use crate::procfs_chirho::{ProcGeneratorChirho, ProcDirEntriesChirho};
    use crate::tmpfs_chirho::TmpfsDataChirho;

    let data_chirho = fs_data_chirho.as_ref()?;

    // ProcGeneratorChirho (proc regular files)
    if let Some(gen_chirho) = data_chirho.downcast_ref::<ProcGeneratorChirho>() {
        return Some(Box::new(gen_chirho.clone()) as Box<dyn core::any::Any + Send>);
    }

    // ProcDirEntriesChirho (proc directories)
    if let Some(entries_chirho) = data_chirho.downcast_ref::<ProcDirEntriesChirho>() {
        return Some(Box::new(entries_chirho.clone()) as Box<dyn core::any::Any + Send>);
    }

    // DevNodeDataChirho (device nodes)
    if let Some(dev_chirho) = data_chirho.downcast_ref::<DevNodeDataChirho>() {
        return Some(Box::new(DevNodeDataChirho {
            major_chirho: dev_chirho.major_chirho,
            minor_chirho: dev_chirho.minor_chirho,
        }) as Box<dyn core::any::Any + Send>);
    }

    // Ext4FsDataChirho (ext4 filesystem)
    if let Some(ext4_data_chirho) = data_chirho.downcast_ref::<crate::ext4_chirho::Ext4FsDataChirho>() {
        return Some(Box::new(crate::ext4_chirho::Ext4FsDataChirho {
            ino_chirho: ext4_data_chirho.ino_chirho,
            mount_chirho: ext4_data_chirho.mount_chirho.clone(),
        }) as Box<dyn core::any::Any + Send>);
    }

    // Mutex<TmpfsDataChirho> (tmpfs files/directories)
    if let Some(tmpfs_mutex_chirho) = data_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>() {
        let inner_chirho = tmpfs_mutex_chirho.lock();
        let cloned_chirho = match &*inner_chirho {
            TmpfsDataChirho::DirChirho(entries_chirho) => {
                TmpfsDataChirho::DirChirho(entries_chirho.clone())
            }
            TmpfsDataChirho::FileChirho(content_chirho) => {
                TmpfsDataChirho::FileChirho(content_chirho.clone())
            }
            TmpfsDataChirho::SymlinkChirho(target_chirho) => {
                TmpfsDataChirho::SymlinkChirho(target_chirho.clone())
            }
        };
        drop(inner_chirho);
        return Some(Box::new(Mutex::new(cloned_chirho)) as Box<dyn core::any::Any + Send>);
    }

    None
}

/// Resolve an absolute path to a `(inode, file_ops)` pair.
///
/// Walks the path component-by-component using `InodeOps::lookup`.
/// Checks mount points: if the accumulated path matches a mount point,
/// resolution continues from that filesystem's root inode.
pub fn resolve_path_chirho(
    path_chirho: &str,
) -> Result<(Arc<Mutex<InodeChirho>>, &'static dyn FileOpsChirho), i64> {
    resolve_path_depth_chirho(path_chirho, 0)
}

/// Linux ELOOP — too many levels of symbolic links.
const ELOOP_CHIRHO: i64 = 40;

/// Maximum symlink depth (matches Linux MAXSYMLINKS = 40).
const MAX_SYMLINK_DEPTH_CHIRHO: u32 = 40;

fn resolve_path_depth_chirho(
    path_chirho: &str,
    symlink_depth_chirho: u32,
) -> Result<(Arc<Mutex<InodeChirho>>, &'static dyn FileOpsChirho), i64> {
    use crate::tmpfs_chirho::TmpfsDataChirho;

    // Symlink loop detection — prevent unbounded recursion that causes
    // the 64MB OOM (each recursive call builds a String that doubles).
    if symlink_depth_chirho > MAX_SYMLINK_DEPTH_CHIRHO {
        return Err(-ELOOP_CHIRHO);
    }

    // Only absolute paths for now
    if !path_chirho.starts_with('/') {
        return Err(-ENOENT_CHIRHO);
    }

    // Split path into components, filtering empty strings
    let components_chirho: Vec<&str> = path_chirho
        .split('/')
        .filter(|s_chirho| !s_chirho.is_empty())
        .collect();

    // Check mount points -- find the longest matching mount
    let mut mount_prefix_len_chirho: usize = 0;
    let mut current_sb_chirho: Option<Arc<Mutex<SuperblockChirho>>> = None;

    {
        let mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        for mount_chirho in mounts_chirho.iter() {
            let mount_path_chirho = &mount_chirho.path_chirho;
            // Special case: "/" matches all paths but is the shortest
            // prefix. Only use it if no longer prefix has been found yet.
            if mount_path_chirho == "/" {
                if mount_prefix_len_chirho <= 1 {
                    mount_prefix_len_chirho = 1;
                    current_sb_chirho = Some(mount_chirho.superblock_chirho.clone());
                }
                continue;
            }
            // Normal mount: path must start with mount_path and be
            // followed by '/' or be an exact match
            if path_chirho.starts_with(mount_path_chirho.as_str())
                && mount_path_chirho.len() > mount_prefix_len_chirho
                && (path_chirho.len() == mount_path_chirho.len()
                    || path_chirho.as_bytes().get(mount_path_chirho.len()) == Some(&b'/'))
            {
                mount_prefix_len_chirho = mount_path_chirho.len();
                current_sb_chirho = Some(mount_chirho.superblock_chirho.clone());
            }
        }
    }

    // Determine starting inode and which components to walk
    let (start_inode_chirho, remaining_components_chirho) = if let Some(sb_chirho) = current_sb_chirho
    {
        // Start from the mount's root inode
        let sb_guard_chirho = sb_chirho.lock();
        let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
        let inode_arc_chirho = root_dentry_chirho
            .inode_chirho
            .clone()
            .ok_or(-ENOENT_CHIRHO)?;

        // Compute remaining path after mount prefix
        let remaining_path_chirho = &path_chirho[mount_prefix_len_chirho..];
        let remaining_chirho: Vec<&str> = remaining_path_chirho
            .split('/')
            .filter(|s_chirho| !s_chirho.is_empty())
            .collect();
        (inode_arc_chirho, remaining_chirho)
    } else {
        // Start from root fs
        let root_guard_chirho = ROOT_FS_CHIRHO.lock();
        let root_sb_chirho = root_guard_chirho.as_ref().ok_or(-ENOENT_CHIRHO)?;
        let sb_guard_chirho = root_sb_chirho.lock();
        let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
        let inode_arc_chirho = root_dentry_chirho
            .inode_chirho
            .clone()
            .ok_or(-ENOENT_CHIRHO)?;
        (inode_arc_chirho, components_chirho)
    };

    // Helper: determine the correct FileOps for an inode based on its fs_data
    fn detect_file_ops_chirho(inode_chirho: &InodeChirho) -> &'static dyn FileOpsChirho {
        if inode_chirho.mode_chirho & S_IFCHR_CHIRHO == S_IFCHR_CHIRHO {
            if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
                if let Some(dev_data_chirho) = data_chirho.downcast_ref::<DevNodeDataChirho>() {
                    return dev_file_ops_chirho(dev_data_chirho.major_chirho, dev_data_chirho.minor_chirho);
                }
            }
            return &TMPFS_FILE_OPS_CHIRHO;
        }
        if let Some(ref data_chirho) = inode_chirho.fs_data_chirho {
            if data_chirho.downcast_ref::<crate::procfs_chirho::ProcGeneratorChirho>().is_some() {
                return &PROCFS_FILE_OPS_CHIRHO;
            }
            if data_chirho.downcast_ref::<crate::procfs_chirho::ProcDirEntriesChirho>().is_some() {
                return &PROCFS_DIR_OPS_CHIRHO;
            }
            // P2-004: ext4 filesystem data
            if data_chirho.downcast_ref::<crate::ext4_chirho::Ext4FsDataChirho>().is_some() {
                if inode_chirho.mode_chirho & S_IFDIR_CHIRHO == S_IFDIR_CHIRHO {
                    return &crate::ext4_chirho::EXT4_DIR_OPS_CHIRHO;
                }
                return &crate::ext4_chirho::EXT4_FILE_OPS_CHIRHO;
            }
        }
        &TMPFS_FILE_OPS_CHIRHO
    }

    // If no more components, return the root/mount-root inode with correct file_ops
    if remaining_components_chirho.is_empty() {
        let file_ops_chirho = {
            let inode_guard_chirho = start_inode_chirho.lock();
            detect_file_ops_chirho(&inode_guard_chirho)
        };
        return Ok((start_inode_chirho, file_ops_chirho));
    }

    // Walk each component using the **live** tree when possible.
    //
    // For tmpfs inodes (which store children as `Arc<Mutex<InodeChirho>>`
    // inside `Mutex<TmpfsDataChirho::DirChirho>`), we walk the live entry
    // vector so that the returned inode is the actual tree node — not a
    // clone.  This ensures that readdir sees entries created by mkdir.
    //
    // For non-tmpfs inodes (procfs, devtmpfs), we fall back to
    // `InodeOps::lookup_chirho` which may return static Arc copies.
    let mut current_inode_chirho = start_inode_chirho;

    for (idx_chirho, component_chirho) in remaining_components_chirho.iter().enumerate() {
        let is_last_chirho = idx_chirho == remaining_components_chirho.len() - 1;

        // Try live tmpfs walk first
        let live_child_chirho: Option<Arc<Mutex<InodeChirho>>> = {
            let inode_guard_chirho = current_inode_chirho.lock();
            if let Some(ref fs_data_box_chirho) = inode_guard_chirho.fs_data_chirho {
                if let Some(tmpfs_mutex_chirho) = fs_data_box_chirho.downcast_ref::<Mutex<TmpfsDataChirho>>() {
                    let data_chirho = tmpfs_mutex_chirho.lock();
                    match &*data_chirho {
                        TmpfsDataChirho::DirChirho(entries_chirho) => {
                            let mut found_chirho: Option<Arc<Mutex<InodeChirho>>> = None;
                            for (name_chirho, child_arc_chirho) in entries_chirho.iter() {
                                if name_chirho.as_str() == *component_chirho {
                                    found_chirho = Some(child_arc_chirho.clone());
                                    break;
                                }
                            }
                            found_chirho
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(child_arc_chirho) = live_child_chirho {
            // Check if this child is a mount point
            if !is_last_chirho {
                let mut child_path_chirho = String::from("/");
                // Build the accumulated path from the original components (not remaining)
                // accounting for the mount prefix that was already consumed.
                // remaining_components_chirho[..=idx_chirho] gives us the path
                // relative to where we started walking.
                for (i_chirho, c_chirho) in remaining_components_chirho[..=idx_chirho].iter().enumerate() {
                    if i_chirho > 0 {
                        child_path_chirho.push('/');
                    }
                    child_path_chirho.push_str(c_chirho);
                }
                // If mount_prefix_len > 0, prepend the mount prefix
                if mount_prefix_len_chirho > 0 {
                    let mut full_accumulated_chirho = String::from(&path_chirho[..mount_prefix_len_chirho]);
                    full_accumulated_chirho.push('/');
                    for (i_chirho, c_chirho) in remaining_components_chirho[..=idx_chirho].iter().enumerate() {
                        if i_chirho > 0 {
                            full_accumulated_chirho.push('/');
                        }
                        full_accumulated_chirho.push_str(c_chirho);
                    }
                    child_path_chirho = full_accumulated_chirho;
                }

                // Check if this accumulated path is a mount point
                let mount_match_chirho: Option<Arc<Mutex<SuperblockChirho>>> = {
                    let mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
                    let mut found_chirho: Option<Arc<Mutex<SuperblockChirho>>> = None;
                    for mount_chirho in mounts_chirho.iter() {
                        if mount_chirho.path_chirho == child_path_chirho {
                            found_chirho = Some(mount_chirho.superblock_chirho.clone());
                            break;
                        }
                    }
                    found_chirho
                };

                if let Some(mount_sb_chirho) = mount_match_chirho {
                    // Switch to the mount's root inode
                    let sb_guard_chirho = mount_sb_chirho.lock();
                    let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
                    current_inode_chirho = root_dentry_chirho.inode_chirho.clone().ok_or(-ENOENT_CHIRHO)?;
                    continue;
                }
            }

            // Symlink following for tmpfs inodes.
            // Check if the child is a symlink — if so, follow it.
            {
                let child_guard_chirho = child_arc_chirho.lock();
                if child_guard_chirho.mode_chirho & 0xF000 == 0xA000 {
                    // S_IFLNK — follow the symlink
                    if let Ok(target_chirho) = child_guard_chirho.ops_chirho.readlink_chirho(&child_guard_chirho) {
                        drop(child_guard_chirho); // Release lock before recursion
                        let resolved_target_chirho = if target_chirho.starts_with('/') {
                            target_chirho
                        } else {
                            let parent_path_chirho = path_chirho.rsplit_once('/').map(|(p, _)| p).unwrap_or("/");
                            let mut full_chirho = String::from(parent_path_chirho);
                            full_chirho.push('/');
                            full_chirho.push_str(&target_chirho);
                            full_chirho
                        };
                        let mut remaining_path_chirho = resolved_target_chirho;
                        for rest_idx_chirho in (idx_chirho + 1)..remaining_components_chirho.len() {
                            remaining_path_chirho.push('/');
                            remaining_path_chirho.push_str(remaining_components_chirho[rest_idx_chirho]);
                        }
                        return resolve_path_depth_chirho(&remaining_path_chirho, symlink_depth_chirho + 1);
                    }
                }
            }

            if is_last_chirho {
                let file_ops_chirho = {
                    let guard_chirho = child_arc_chirho.lock();
                    detect_file_ops_chirho(&guard_chirho)
                };
                return Ok((child_arc_chirho, file_ops_chirho));
            }
            current_inode_chirho = child_arc_chirho;
        } else {
            // Non-tmpfs: use InodeOps::lookup_chirho (procfs, devtmpfs, etc.)
            let lookup_result_chirho = {
                let inode_guard_chirho = current_inode_chirho.lock();
                inode_guard_chirho.ops_chirho.lookup_chirho(&inode_guard_chirho, component_chirho)
            };

            match lookup_result_chirho {
                Ok(child_inode_chirho) => {
                    // Symlink following: if the inode is a symlink, read
                    // the target and recursively resolve it.
                    if child_inode_chirho.mode_chirho & 0xF000 == 0xA000 {
                        // S_IFLNK
                        if let Ok(target_chirho) = child_inode_chirho.ops_chirho.readlink_chirho(&child_inode_chirho) {
                            // Build the resolved path: if target is relative,
                            // prepend the current directory; if absolute, use as-is.
                            let resolved_target_chirho = if target_chirho.starts_with('/') {
                                target_chirho
                            } else {
                                // Get parent path
                                let parent_path_chirho = path_chirho.rsplit_once('/').map(|(p, _)| p).unwrap_or("/");
                                let mut full_chirho = String::from(parent_path_chirho);
                                full_chirho.push('/');
                                full_chirho.push_str(&target_chirho);
                                full_chirho
                            };
                            // Append remaining path components after the symlink
                            let mut remaining_path_chirho = resolved_target_chirho;
                            for rest_idx_chirho in (idx_chirho + 1)..remaining_components_chirho.len() {
                                remaining_path_chirho.push('/');
                                remaining_path_chirho.push_str(remaining_components_chirho[rest_idx_chirho]);
                            }
                            return resolve_path_depth_chirho(&remaining_path_chirho, symlink_depth_chirho + 1);
                        }
                    }

                    if is_last_chirho {
                        let file_ops_chirho = detect_file_ops_chirho(&child_inode_chirho);
                        let fs_data_clone_chirho = clone_fs_data_chirho(&child_inode_chirho.fs_data_chirho);
                        let owned_inode_chirho = InodeChirho {
                            ino_chirho: child_inode_chirho.ino_chirho,
                            mode_chirho: child_inode_chirho.mode_chirho,
                            uid_chirho: child_inode_chirho.uid_chirho,
                            gid_chirho: child_inode_chirho.gid_chirho,
                            size_chirho: child_inode_chirho.size_chirho,
                            nlink_chirho: child_inode_chirho.nlink_chirho,
                            atime_chirho: child_inode_chirho.atime_chirho,
                            mtime_chirho: child_inode_chirho.mtime_chirho,
                            ctime_chirho: child_inode_chirho.ctime_chirho,
                            ops_chirho: child_inode_chirho.ops_chirho,
                            fs_data_chirho: fs_data_clone_chirho,
                        };
                        return Ok((Arc::new(Mutex::new(owned_inode_chirho)), file_ops_chirho));
                    }

                    // Intermediate non-tmpfs component: wrap and continue
                    let intermediate_fs_data_chirho = clone_fs_data_chirho(&child_inode_chirho.fs_data_chirho);
                    current_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                        ino_chirho: child_inode_chirho.ino_chirho,
                        mode_chirho: child_inode_chirho.mode_chirho,
                        uid_chirho: child_inode_chirho.uid_chirho,
                        gid_chirho: child_inode_chirho.gid_chirho,
                        size_chirho: child_inode_chirho.size_chirho,
                        nlink_chirho: child_inode_chirho.nlink_chirho,
                        atime_chirho: child_inode_chirho.atime_chirho,
                        mtime_chirho: child_inode_chirho.mtime_chirho,
                        ctime_chirho: child_inode_chirho.ctime_chirho,
                        ops_chirho: child_inode_chirho.ops_chirho,
                        fs_data_chirho: intermediate_fs_data_chirho,
                    }));
                }
                Err(errno_chirho) => return Err(errno_chirho),
            }
        }
    }

    // Should not reach here, but just in case
    let file_ops_chirho = {
        let guard_chirho = current_inode_chirho.lock();
        detect_file_ops_chirho(&guard_chirho)
    };
    Ok((current_inode_chirho, file_ops_chirho))
}

/// Resolve an absolute path to the **live** parent `Arc<Mutex<InodeChirho>>`
/// and the final component name.
///
/// Unlike `resolve_path_chirho`, this returns the actual `Arc` stored in the
/// tmpfs entry vector (not a clone), so mutations via `InodeOps` (mkdir,
/// create, unlink, etc.) affect the live filesystem tree.
///
/// Respects mount points: if the path crosses into a mounted filesystem,
/// the walk continues from that mount's root inode.
pub fn resolve_parent_live_chirho(
    path_chirho: &str,
) -> Result<(Arc<Mutex<InodeChirho>>, alloc::string::String), i64> {
    use crate::tmpfs_chirho::TmpfsDataChirho;

    if !path_chirho.starts_with('/') {
        return Err(-ENOENT_CHIRHO);
    }

    let components_chirho: Vec<&str> = path_chirho
        .split('/')
        .filter(|s_chirho| !s_chirho.is_empty())
        .collect();

    if components_chirho.is_empty() {
        return Err(-ENOENT_CHIRHO); // can't mkdir "/"
    }

    // .last() is guaranteed Some because we checked is_empty() above, but
    // use ok_or to avoid a panic path in the generated binary.
    let final_name_chirho = alloc::string::String::from(
        *components_chirho.last().ok_or(-ENOENT_CHIRHO)?
    );
    let parent_components_chirho = &components_chirho[..components_chirho.len() - 1];

    // Build the parent path to check mount points
    let mut parent_path_chirho = alloc::string::String::from("/");
    for (i_chirho, c_chirho) in parent_components_chirho.iter().enumerate() {
        parent_path_chirho.push_str(c_chirho);
        if i_chirho < parent_components_chirho.len() - 1 {
            parent_path_chirho.push('/');
        }
    }

    // Check mount points for the parent path
    let mut mount_prefix_len_chirho: usize = 0;
    let mut current_sb_chirho: Option<Arc<Mutex<SuperblockChirho>>> = None;
    {
        let mounts_chirho = MOUNT_TABLE_CHIRHO.lock();
        for mount_chirho in mounts_chirho.iter() {
            let mp_chirho = &mount_chirho.path_chirho;
            // "/" matches all paths but only if no longer mount was found
            if mp_chirho == "/" {
                if mount_prefix_len_chirho <= 1 {
                    mount_prefix_len_chirho = 1;
                    current_sb_chirho = Some(mount_chirho.superblock_chirho.clone());
                }
                continue;
            }
            if path_chirho.starts_with(mp_chirho.as_str())
                && mp_chirho.len() > mount_prefix_len_chirho
                && (path_chirho.len() == mp_chirho.len()
                    || path_chirho.as_bytes().get(mp_chirho.len()) == Some(&b'/'))
            {
                mount_prefix_len_chirho = mp_chirho.len();
                current_sb_chirho = Some(mount_chirho.superblock_chirho.clone());
            }
        }
    }

    // Get the starting live inode
    let (start_inode_chirho, walk_components_chirho) = if let Some(sb_chirho) = current_sb_chirho {
        let sb_guard_chirho = sb_chirho.lock();
        let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
        let inode_arc_chirho = root_dentry_chirho.inode_chirho.clone().ok_or(-ENOENT_CHIRHO)?;
        let remaining_path_chirho = &path_chirho[mount_prefix_len_chirho..];
        let remaining_chirho: Vec<&str> = remaining_path_chirho
            .split('/')
            .filter(|s_chirho| !s_chirho.is_empty())
            .collect();
        // Parent components = all except last
        let parent_rem_chirho: Vec<&str> = if remaining_chirho.len() > 1 {
            remaining_chirho[..remaining_chirho.len() - 1].to_vec()
        } else {
            Vec::new()
        };
        (inode_arc_chirho, parent_rem_chirho)
    } else {
        let root_guard_chirho = ROOT_FS_CHIRHO.lock();
        let root_sb_chirho = root_guard_chirho.as_ref().ok_or(-ENOENT_CHIRHO)?;
        let sb_guard_chirho = root_sb_chirho.lock();
        let root_dentry_chirho = sb_guard_chirho.root_chirho.lock();
        let inode_arc_chirho = root_dentry_chirho.inode_chirho.clone().ok_or(-ENOENT_CHIRHO)?;
        (inode_arc_chirho, parent_components_chirho.to_vec())
    };

    // Walk the parent components through the live tmpfs tree
    let mut current_chirho = start_inode_chirho;
    for comp_chirho in walk_components_chirho.iter() {
        // Use the generic VFS lookup (works for both tmpfs and ext4)
        let next_chirho = {
            let inode_guard_chirho = current_chirho.lock();
            // Check if directory
            if inode_guard_chirho.mode_chirho & S_IFDIR_CHIRHO != S_IFDIR_CHIRHO {
                return Err(-ENOTDIR_CHIRHO);
            }
            let child_result_chirho = inode_guard_chirho.ops_chirho.lookup_chirho(
                &inode_guard_chirho,
                comp_chirho,
            );
            match child_result_chirho {
                Ok(child_inode_arc_chirho) => {
                    // Wrap Arc<InodeChirho> into Arc<Mutex<InodeChirho>>
                    let i_chirho = &*child_inode_arc_chirho;
                    Arc::new(Mutex::new(crate::vfs_chirho::InodeChirho {
                        ino_chirho: i_chirho.ino_chirho,
                        mode_chirho: i_chirho.mode_chirho,
                        uid_chirho: i_chirho.uid_chirho,
                        gid_chirho: i_chirho.gid_chirho,
                        size_chirho: i_chirho.size_chirho,
                        nlink_chirho: i_chirho.nlink_chirho,
                        atime_chirho: i_chirho.atime_chirho,
                        mtime_chirho: i_chirho.mtime_chirho,
                        ctime_chirho: i_chirho.ctime_chirho,
                        ops_chirho: i_chirho.ops_chirho,
                        // Reconstruct fs_data for ext4 inodes
                        fs_data_chirho: {
                            if let Some(ref d_chirho) = i_chirho.fs_data_chirho {
                                if let Some(ed_chirho) = d_chirho.downcast_ref::<crate::ext4_chirho::Ext4FsDataChirho>() {
                                    Some(alloc::boxed::Box::new(crate::ext4_chirho::Ext4FsDataChirho {
                                        ino_chirho: ed_chirho.ino_chirho,
                                        mount_chirho: ed_chirho.mount_chirho.clone(),
                                    }))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                    }))
                }
                Err(e_chirho) => return Err(e_chirho),
            }
        };

        // Check if accumulated path so far crosses a mount point
        // (handle paths like /mnt/sub where /mnt is a mount)
        current_chirho = next_chirho;
    }

    Ok((current_chirho, final_name_chirho))
}

/// Create a new file at the given path using the parent directory's create op.
/// Used when open() is called with O_CREAT on a non-existent path.
fn create_file_at_path_chirho(
    path_chirho: &str,
    mode_chirho: u32,
) -> Result<(Arc<Mutex<InodeChirho>>, &'static dyn FileOpsChirho), i64> {
    crate::serial_println_chirho!("[FS-CREATE] creating '{}'", path_chirho);
    // Create the file in the parent directory
    {
        let (parent_inode_chirho, name_chirho) = match resolve_parent_live_chirho(path_chirho) {
            Ok(result_chirho) => result_chirho,
            Err(e_chirho) => {
                return Err(e_chirho);
            }
        };
        let parent_guard_chirho = parent_inode_chirho.lock();
        let _new_chirho = parent_guard_chirho.ops_chirho.create_chirho(
            &parent_guard_chirho,
            &name_chirho,
            mode_chirho | 0o100000, // S_IFREG
        )?;
    }
    // Now resolve the newly created file through the normal VFS path
    resolve_path_chirho(path_chirho)
}

// ============================================================================
// Syscall implementations
// ============================================================================

/// `openat(2)` -- open a file relative to a directory fd.
///
/// Currently ignores `dirfd_chirho` for absolute paths.
pub fn sys_openat_chirho(
    dirfd_chirho: i64,
    pathname_addr_chirho: u64,
    flags_chirho: u32,
    mode_chirho: u32,
) -> i64 {
    // Read the pathname from user space
    let raw_pathname_chirho = match read_user_string_chirho(pathname_addr_chirho, 4096) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -EFAULT_CHIRHO,
    };

    // Handle relative paths: resolve against dirfd or CWD.
    let pathname_chirho = if !raw_pathname_chirho.starts_with('/') {
        if dirfd_chirho == AT_FDCWD_CHIRHO {
            // AT_FDCWD: resolve relative to current working directory.
            let cwd_chirho = if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                let t_chirho = task_arc_chirho.lock();
                if !t_chirho.cwd_chirho.is_empty() {
                    t_chirho.cwd_chirho.clone()
                } else {
                    alloc::string::String::from("/")
                }
            } else {
                alloc::string::String::from("/")
            };
            let mut full_path_chirho = cwd_chirho;
            if !full_path_chirho.ends_with('/') {
                full_path_chirho.push('/');
            }
            full_path_chirho.push_str(&raw_pathname_chirho);
            full_path_chirho
        } else {
            // Resolve relative to the directory referenced by dirfd.
            // Look up the dirfd's path in the fd table.
            let dir_path_chirho = get_fd_path_chirho(dirfd_chirho as u64);
            if let Some(dp_chirho) = dir_path_chirho {
                let mut full_path_chirho = dp_chirho;
                if !full_path_chirho.ends_with('/') {
                    full_path_chirho.push('/');
                }
                full_path_chirho.push_str(&raw_pathname_chirho);
                crate::log_fs_chirho!(
                    "openat: dirfd={} resolved to '{}' + '{}'",
                    dirfd_chirho, full_path_chirho, raw_pathname_chirho
                );
                full_path_chirho
            } else {
                // Can't resolve dirfd — try as absolute from CWD
                crate::log_fs_chirho!(
                    "openat: dirfd={} unknown, treating '{}' as relative to /",
                    dirfd_chirho, raw_pathname_chirho
                );
                let mut full_path_chirho = alloc::string::String::from("/");
                full_path_chirho.push_str(&raw_pathname_chirho);
                full_path_chirho
            }
        }
    } else {
        raw_pathname_chirho
    };

    // Log which file is being opened.
    crate::log_fs_chirho!("OPEN {}", &pathname_chirho);

    // Special case: /dev/pts/N -- PTY slave devices are created dynamically
    // and don't exist in the VFS tree.  We detect this pattern and create
    // the slave file directly.
    if pathname_chirho.starts_with("/dev/pts/") {
        let num_str_chirho = &pathname_chirho["/dev/pts/".len()..];
        if let Ok(pty_nr_chirho) = num_str_chirho.parse::<u32>() {
            if let Some(pair_chirho) = crate::pty_chirho::get_pty_chirho(pty_nr_chirho) {
                if !pair_chirho.slave_unlocked_chirho.load(core::sync::atomic::Ordering::SeqCst) {
                    return -crate::syscall_chirho::EACCES_CHIRHO;
                }
                let slave_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                    ino_chirho: pty_nr_chirho as u64,
                    mode_chirho: S_IFCHR_CHIRHO | 0o666,
                    uid_chirho: 0,
                    gid_chirho: 0,
                    size_chirho: 0,
                    nlink_chirho: 1,
                    atime_chirho: 0,
                    mtime_chirho: 0,
                    ctime_chirho: 0,
                    ops_chirho: &crate::tmpfs_chirho::TMPFS_INODE_OPS_CHIRHO,
                    fs_data_chirho: None,
                }));
                let file_chirho = Arc::new(Mutex::new(FileChirho {
                    inode_chirho: slave_inode_chirho,
                    pos_chirho: 0,
                    flags_chirho,
                    ops_chirho: &crate::pty_chirho::PTY_SLAVE_OPS_CHIRHO,
                }));
                let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
                let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
                    Some(t_chirho) => t_chirho,
                    None => return -EBADF_CHIRHO,
                };
                let fd_chirho = match fd_table_chirho.alloc_fd_chirho() {
                    Ok(fd_chirho) => fd_chirho,
                    Err(errno_chirho) => return errno_chirho,
                };
                fd_table_chirho.fds_chirho[fd_chirho] = Some(file_chirho);
                pair_chirho.slave_open_chirho.store(true, core::sync::atomic::Ordering::SeqCst);
                return fd_chirho as i64;
            } else {
                return -crate::syscall_chirho::ENOENT_CHIRHO;
            }
        }
    }

    // Resolve the path
    let (inode_chirho, file_ops_chirho) = match resolve_path_chirho(&pathname_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(errno_chirho) => {
            // If O_CREAT is set and the file doesn't exist, create it
            if flags_chirho & O_CREAT_CHIRHO != 0 && errno_chirho == -ENOENT_CHIRHO {
                match create_file_at_path_chirho(&pathname_chirho, mode_chirho) {
                    Ok(result_chirho) => result_chirho,
                    Err(e_chirho) => return e_chirho,
                }
            } else {
                return errno_chirho;
            }
        }
    };

    // Check O_DIRECTORY: if set, the inode must be a directory
    if flags_chirho & O_DIRECTORY_CHIRHO != 0 {
        let inode_guard_chirho = inode_chirho.lock();
        if inode_guard_chirho.mode_chirho & S_IFDIR_CHIRHO != S_IFDIR_CHIRHO {
            return -ENOTDIR_CHIRHO;
        }
    }

    // Special handling for /dev/ptmx: allocate a new PTY pair.
    // The PTY master ops use the inode's ino_chirho field to identify
    // which PTY pair they belong to.
    let (final_inode_chirho, final_ops_chirho) = {
        let is_ptmx_chirho = {
            let ig_chirho = inode_chirho.lock();
            if let Some(ref data_chirho) = ig_chirho.fs_data_chirho {
                if let Some(dev_data_chirho) = data_chirho.downcast_ref::<DevNodeDataChirho>() {
                    dev_data_chirho.major_chirho == 5 && dev_data_chirho.minor_chirho == 2
                } else {
                    false
                }
            } else {
                false
            }
        };

        if is_ptmx_chirho {
            // Allocate a new PTY pair
            match crate::pty_chirho::PtmxFileOpsChirho::open_ptmx_chirho() {
                Ok((pair_chirho, master_ops_chirho)) => {
                    // Create a fresh inode with ino = pty_nr so the master
                    // ops can find the pair.
                    let pty_inode_chirho = Arc::new(Mutex::new(InodeChirho {
                        ino_chirho: pair_chirho.pty_nr_chirho as u64,
                        mode_chirho: S_IFCHR_CHIRHO | 0o666,
                        uid_chirho: 0,
                        gid_chirho: 0,
                        size_chirho: 0,
                        nlink_chirho: 1,
                        atime_chirho: 0,
                        mtime_chirho: 0,
                        ctime_chirho: 0,
                        ops_chirho: &crate::tmpfs_chirho::TMPFS_INODE_OPS_CHIRHO,
                        fs_data_chirho: None,
                    }));
                    (pty_inode_chirho, master_ops_chirho)
                }
                Err(errno_chirho) => return errno_chirho,
            }
        } else {
            (inode_chirho, file_ops_chirho)
        }
    };

    // Create the FileChirho
    let file_chirho = Arc::new(Mutex::new(FileChirho {
        inode_chirho: final_inode_chirho,
        pos_chirho: 0,
        flags_chirho,
        ops_chirho: final_ops_chirho,
    }));

    // Allocate an fd
    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
        Some(t_chirho) => t_chirho,
        None => return -EBADF_CHIRHO,
    };

    let fd_chirho = match fd_table_chirho.alloc_fd_chirho() {
        Ok(fd_chirho) => fd_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    fd_table_chirho.fds_chirho[fd_chirho] = Some(file_chirho);
    // Store the path for dirfd resolution in future openat calls.
    if fd_chirho < fd_table_chirho.paths_chirho.len() {
        fd_table_chirho.paths_chirho[fd_chirho] = Some(pathname_chirho.clone());
    }
    fd_chirho as i64
}

/// `open(2)` -- wrapper around openat with AT_FDCWD.
/// Get the path associated with a file descriptor (for openat dirfd resolution).
pub fn get_fd_path_chirho(fd_chirho: u64) -> Option<alloc::string::String> {
    let fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    if let Some(ref fd_table_chirho) = *fd_table_guard_chirho {
        if let Some(Some(ref path_chirho)) = fd_table_chirho.paths_chirho.get(fd_chirho as usize) {
            return Some(path_chirho.clone());
        }
    }
    None
}

/// Store the path for a file descriptor.
pub fn set_fd_path_chirho(fd_chirho: usize, path_chirho: &str) {
    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    if let Some(ref mut fd_table_chirho) = *fd_table_guard_chirho {
        if fd_chirho < fd_table_chirho.paths_chirho.len() {
            fd_table_chirho.paths_chirho[fd_chirho] = Some(alloc::string::String::from(path_chirho));
        }
    }
}

pub fn sys_open_chirho(
    pathname_addr_chirho: u64,
    flags_chirho: u32,
    mode_chirho: u32,
) -> i64 {
    sys_openat_chirho(AT_FDCWD_CHIRHO, pathname_addr_chirho, flags_chirho, mode_chirho)
}

/// Look up a file from an fd, checking GLOBAL table first,
/// then the current task's per-process table as fallback.
///
/// This is needed because the global fd table may be stale
/// after a context switch (parent closed fds that child still has).
pub fn lookup_fd_chirho(fd_chirho: u64) -> Option<alloc::sync::Arc<spin::Mutex<FileChirho>>> {
    // Try global first.
    {
        let fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        if let Some(ref fd_table_chirho) = *fd_table_guard_chirho {
            if let Some(file_arc_chirho) = fd_table_chirho.get_chirho(fd_chirho as usize) {
                return Some(file_arc_chirho);
            }
        }
    }
    // Fallback: current task's fd table.
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let task_guard_chirho = task_arc_chirho.lock();
        if let Some(ref fd_table_chirho) = task_guard_chirho.fd_table_chirho {
            if let Some(file_arc_chirho) = fd_table_chirho.get_chirho(fd_chirho as usize) {
                return Some(file_arc_chirho);
            }
        }
    }
    None
}

/// `read(2)` -- read from a file descriptor using the VFS.
pub fn sys_read_real_chirho(fd_chirho: u64, buf_addr_chirho: u64, count_chirho: usize) -> i64 {
    if count_chirho == 0 {
        return 0;
    }

    // Get the file from the fd table (global or per-task fallback).
    let file_arc_chirho = match lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Read into a stack-based kernel buffer (avoid heap allocation
    // which can trigger page faults in the allocator during syscalls).
    let capped_count_chirho = core::cmp::min(count_chirho, 4096);
    let mut kernel_buf_storage_chirho = [0u8; 4096];
    let kernel_buf_chirho = &mut kernel_buf_storage_chirho[..capped_count_chirho];
    let bytes_read_chirho = {
        let mut file_guard_chirho = file_arc_chirho.lock();
        match file_guard_chirho.ops_chirho.read_chirho(&mut file_guard_chirho, kernel_buf_chirho) {
            Ok(n_chirho) => n_chirho,
            Err(errno_chirho) => return errno_chirho,
        }
    };

    // Copy to user space
    if bytes_read_chirho > 0 {
        if let Err(_) =
            copy_to_user_chirho(buf_addr_chirho, &kernel_buf_chirho[..bytes_read_chirho], bytes_read_chirho)
        {
            return -EFAULT_CHIRHO;
        }
    }

    bytes_read_chirho as i64
}

/// `write(2)` -- write to a file descriptor using the VFS.
pub fn sys_write_real_chirho(fd_chirho: u64, buf_addr_chirho: u64, count_chirho: usize) -> i64 {
    if count_chirho == 0 {
        return 0;
    }

    // Get the file from the fd table (global or per-task fallback).
    let file_arc_chirho = match lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Copy from user space into stack-based kernel buffer
    let capped_write_chirho = core::cmp::min(count_chirho, 4096);
    let mut kernel_buf_storage2_chirho = [0u8; 4096];
    let kernel_buf_chirho = &mut kernel_buf_storage2_chirho[..capped_write_chirho];
    if let Err(_) =
        copy_from_user_chirho(kernel_buf_chirho, buf_addr_chirho, capped_write_chirho)
    {
        return -EFAULT_CHIRHO;
    }

    // Write through the file ops
    let bytes_written_chirho = {
        let mut file_guard_chirho = file_arc_chirho.lock();

        // O_APPEND: seek to end of file before writing
        if file_guard_chirho.flags_chirho & crate::vfs_chirho::O_APPEND_CHIRHO != 0 {
            let size_chirho = file_guard_chirho.inode_chirho.lock().size_chirho;
            file_guard_chirho.pos_chirho = size_chirho;
        }

        match file_guard_chirho.ops_chirho.write_chirho(&mut file_guard_chirho, &kernel_buf_chirho) {
            Ok(n_chirho) => n_chirho,
            Err(errno_chirho) => return errno_chirho,
        }
    };

    bytes_written_chirho as i64
}

/// `close(2)` -- close a file descriptor.
/// Read file data from an fd at a specific offset, returning a Vec<u8>.
/// Used by mmap to bulk-read file contents instead of the O(n²) 4K loop.
pub fn read_file_data_at_offset_chirho(
    fd_chirho: u64,
    offset_chirho: u64,
    max_len_chirho: u64,
) -> Option<alloc::vec::Vec<u8>> {
    let file_arc_chirho = {
        let fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        let fd_table_chirho = fd_table_guard_chirho.as_ref()?;
        fd_table_chirho.get_chirho(fd_chirho as usize)?
    };

    let mut file_guard_chirho = file_arc_chirho.lock();

    // Get file size
    let file_size_chirho = {
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        inode_guard_chirho.size_chirho
    };

    // Calculate how much to read
    let start_chirho = offset_chirho as usize;
    if start_chirho as u64 >= file_size_chirho {
        return Some(alloc::vec::Vec::new());
    }
    let available_chirho = (file_size_chirho as usize) - start_chirho;
    let to_read_chirho = core::cmp::min(available_chirho, max_len_chirho as usize);

    // Save position, seek to offset, read, restore
    let saved_pos_chirho = file_guard_chirho.pos_chirho;
    file_guard_chirho.pos_chirho = offset_chirho;

    let mut buf_chirho = alloc::vec![0u8; to_read_chirho];
    match file_guard_chirho.ops_chirho.read_chirho(&mut file_guard_chirho, &mut buf_chirho) {
        Ok(n_chirho) => {
            buf_chirho.truncate(n_chirho);
            file_guard_chirho.pos_chirho = saved_pos_chirho;
            Some(buf_chirho)
        }
        Err(_) => {
            file_guard_chirho.pos_chirho = saved_pos_chirho;
            None
        }
    }
}

pub fn sys_close_real_chirho(fd_chirho: u64) -> i64 {
    let result_chirho = {
        let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
            Some(t_chirho) => t_chirho,
            None => return -EBADF_CHIRHO,
        };
        match fd_table_chirho.close_chirho(fd_chirho as usize) {
            Ok(()) => 0i64,
            Err(errno_chirho) => errno_chirho,
        }
    };
    // Drop the fd_table lock BEFORE returning — if dealloc triggers
    // ext4 inode drop which tries to use serial (mutex), the lock
    // ordering could cause issues.
    result_chirho
}

/// `dup(2)` -- duplicate a file descriptor.
pub fn sys_dup_chirho(oldfd_chirho: u64) -> i64 {
    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
        Some(t_chirho) => t_chirho,
        None => return -EBADF_CHIRHO,
    };

    match fd_table_chirho.dup_chirho(oldfd_chirho as usize) {
        Ok(new_fd_chirho) => new_fd_chirho as i64,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `dup2(2)` -- duplicate a file descriptor to a specific number.
pub fn sys_dup2_chirho(oldfd_chirho: u64, newfd_chirho: u64) -> i64 {
    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
        Some(t_chirho) => t_chirho,
        None => return -EBADF_CHIRHO,
    };

    let old_chirho = oldfd_chirho as usize;
    let new_chirho = newfd_chirho as usize;

    // If oldfd == newfd, just check validity
    if old_chirho == new_chirho {
        return if fd_table_chirho.get_chirho(old_chirho).is_some() {
            new_chirho as i64
        } else {
            -EBADF_CHIRHO
        };
    }

    // Get the file for the old fd
    let file_chirho = match fd_table_chirho.get_chirho(old_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Ensure the new fd slot exists
    if new_chirho >= fd_table_chirho.fds_chirho.len() {
        return -EBADF_CHIRHO;
    }

    // Close whatever was at newfd (if anything), then place the dup there
    fd_table_chirho.fds_chirho[new_chirho] = Some(file_chirho);

    new_chirho as i64
}

/// `dup3(2)` -- duplicate a file descriptor with flags (e.g. O_CLOEXEC).
///
/// Like dup2, but if oldfd == newfd, returns -EINVAL (per Linux semantics).
/// The `flags_chirho` argument is recorded but not yet enforced (O_CLOEXEC
/// would matter once execve drops close-on-exec descriptors).
pub fn sys_dup3_chirho(oldfd_chirho: u64, newfd_chirho: u64, flags_chirho: u32) -> i64 {
    use crate::syscall_chirho::EINVAL_CHIRHO;

    let old_chirho = oldfd_chirho as usize;
    let new_chirho = newfd_chirho as usize;

    // dup3 differs from dup2: oldfd == newfd is an error
    if old_chirho == new_chirho {
        return -EINVAL_CHIRHO;
    }

    let mut fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
    let fd_table_chirho = match fd_table_guard_chirho.as_mut() {
        Some(t_chirho) => t_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Get the file for the old fd
    let file_chirho = match fd_table_chirho.get_chirho(old_chirho) {
        Some(f_chirho) => f_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Ensure the new fd slot exists
    if new_chirho >= fd_table_chirho.fds_chirho.len() {
        return -EBADF_CHIRHO;
    }

    // Close whatever was at newfd (if anything), then place the dup there
    fd_table_chirho.fds_chirho[new_chirho] = Some(file_chirho);

    // Note: O_CLOEXEC flag (flags_chirho) is accepted but not yet enforced
    // until execve implements close-on-exec descriptor cleanup.
    let _ = flags_chirho;

    new_chirho as i64
}

/// `lseek(2)` -- reposition read/write file offset.
pub fn sys_lseek_chirho(fd_chirho: u64, offset_chirho: i64, whence_chirho: u32) -> i64 {
    // Get the file from the fd table
    let file_arc_chirho = {
        let fd_table_guard_chirho = GLOBAL_FD_TABLE_CHIRHO.lock();
        let fd_table_chirho = match fd_table_guard_chirho.as_ref() {
            Some(t_chirho) => t_chirho,
            None => return -EBADF_CHIRHO,
        };
        match fd_table_chirho.get_chirho(fd_chirho as usize) {
            Some(f_chirho) => f_chirho,
            None => return -EBADF_CHIRHO,
        }
    };

    let mut file_guard_chirho = file_arc_chirho.lock();
    match file_guard_chirho
        .ops_chirho
        .seek_chirho(&mut file_guard_chirho, offset_chirho, whence_chirho)
    {
        Ok(new_pos_chirho) => new_pos_chirho as i64,
        Err(errno_chirho) => errno_chirho,
    }
}
