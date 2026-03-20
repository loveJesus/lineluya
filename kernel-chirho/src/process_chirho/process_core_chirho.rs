// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Process creation and lifecycle syscalls for the Lineluya kernel.
//!
//! Provides:
//! - `sys_fork_chirho`   — vfork-style fork (shared page tables, child must execve)
//! - `sys_clone_chirho`  — clone with flag-driven sharing (CLONE_VM, CLONE_FILES, etc.)
//! - `sys_wait4_chirho`  — wait for child process termination and reap zombies
//! - `sys_execve_chirho` — real implementation that loads an ELF binary, sets up
//!   the user stack with argv/envp, and jumps to userspace via IRETQ.
//!
//! ## Fork design (vfork-style)
//!
//! Because Lineluya does not yet have per-process page tables, fork shares
//! the parent's address space with the child.  The child **must** call
//! `execve()` before touching user memory — identical to Linux `vfork()`.
//! The child's saved register context is a copy of the parent's syscall
//! frame with `rax = 0` (so `fork()` returns 0 in the child).

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::exec_chirho::{self, ExecErrorChirho, LoadedElfChirho, HELLO_ELF_CHIRHO};
use crate::dynlink_chirho;
use crate::fs_chirho;
use crate::syscall_chirho::{
    SyscallFrameChirho, E2BIG_CHIRHO, EAGAIN_CHIRHO, ECHILD_CHIRHO, EFAULT_CHIRHO,
    ENOENT_CHIRHO, ENOEXEC_CHIRHO, ENOMEM_CHIRHO, ENOSYS_CHIRHO,
};
use crate::task_chirho::{
    allocate_pid_chirho, current_task_chirho, register_task_chirho, CpuContextChirho, TaskChirho,
    TaskStateChirho, TASK_LIST_CHIRHO,
};
use crate::uaccess_chirho::{read_user_string_chirho, read_user_u64_chirho};

fn parse_proc_fd_exec_path_chirho(path_chirho: &str) -> Option<u64> {
    let current_pid_chirho = crate::task_chirho::current_task_chirho()
        .map(|task_arc_chirho| task_arc_chirho.lock().pid_chirho)
        .unwrap_or(1);
    let self_prefix_chirho = "/proc/self/fd/";
    let pid_prefix_chirho = format!("/proc/{}/fd/", current_pid_chirho);

    let fd_suffix_chirho = if let Some(suffix_chirho) = path_chirho.strip_prefix(self_prefix_chirho) {
        suffix_chirho
    } else if let Some(suffix_chirho) = path_chirho.strip_prefix(&pid_prefix_chirho) {
        suffix_chirho
    } else {
        return None;
    };

    if fd_suffix_chirho.is_empty() || fd_suffix_chirho.contains('/') {
        return None;
    }

    fd_suffix_chirho.parse::<u64>().ok()
}

fn try_read_file_from_fd_chirho(fd_chirho: u64) -> Option<Vec<u8>> {
    let file_arc_chirho = crate::fs_chirho::lookup_fd_chirho(fd_chirho)?;
    let file_size_chirho = {
        let file_guard_chirho = file_arc_chirho.lock();
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        inode_guard_chirho.size_chirho
    };
    crate::fs_chirho::read_file_data_at_offset_chirho(fd_chirho, 0, file_size_chirho)
}

fn find_socket_fds_in_current_task_chirho() -> Vec<u64> {
    let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() else {
        return Vec::new();
    };

    let task_guard_chirho = task_arc_chirho.lock();
    let Some(fd_table_chirho) = task_guard_chirho.fd_table_chirho.as_ref() else {
        return Vec::new();
    };

    let mut socket_fds_chirho = Vec::new();
    for (fd_index_chirho, slot_chirho) in fd_table_chirho.fds_chirho.iter().enumerate() {
        let Some(file_arc_chirho) = slot_chirho.as_ref() else {
            continue;
        };
        let file_guard_chirho = file_arc_chirho.lock();
        let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
        if inode_guard_chirho.mode_chirho & 0o170000 == 0o140000 {
            socket_fds_chirho.push(fd_index_chirho as u64);
        }
    }

    socket_fds_chirho
}

/// Flag set by execveat handler when AT_EMPTY_PATH (fexecve pattern).
/// Read by sys_execve_with_filename_chirho to preserve socket fds.
pub static IS_PROCFD_EXEC_FLAG_CHIRHO: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

fn resolve_exec_source_chirho(filename_chirho: &str) -> (String, Option<Vec<u8>>) {
    if let Some(fd_chirho) = parse_proc_fd_exec_path_chirho(filename_chirho) {
        // Set the procfd flag BEFORE resolving — sys_execve_with_filename
        // reads this to know it should preserve socket fds.
        IS_PROCFD_EXEC_FLAG_CHIRHO.store(true, core::sync::atomic::Ordering::Relaxed);
        // When resolving /proc/self/fd/N for fexecve (dropbear re-exec),
        // also set up stdin/stdout/stderr to the TCP socket. Dropbear
        // expects fexecve to fail so it can dup2 afterward — but since
        // our procfd resolution makes it succeed, the dup2 is skipped.
        // Dropbear can reshuffle descriptors before re-exec, so scan the
        // live fd table for socket-backed entries instead of assuming the
        // accepted TCP socket still lives in a small hardcoded range.
        let socket_fds_chirho = find_socket_fds_in_current_task_chirho();
        if !socket_fds_chirho.is_empty() {
            crate::serial_println_chirho!(
                "[PROCESS] procfd exec: found socket fds {:?}",
                socket_fds_chirho,
            );
            // DO NOT dup2 socket to fd 0/1/2 — that corrupts the SSH
            // stream (stderr log messages become plaintext in encrypted
            // channel) and breaks nfds calculation in dropbear's select.
        } else {
            crate::serial_println_chirho!(
                "[PROCESS] procfd exec: no socket fds present in current task table"
            );
        }

        if let Some(fd_path_chirho) = crate::fs_chirho::get_fd_path_chirho(fd_chirho) {
            crate::serial_debug_chirho!(
                "[PROCESS] execve: resolved {} -> {} via fd table",
                filename_chirho,
                fd_path_chirho,
            );
            let elf_data_chirho = try_read_file_chirho(&fd_path_chirho);
            return (fd_path_chirho, elf_data_chirho);
        }

        let elf_data_chirho = try_read_file_from_fd_chirho(fd_chirho);
        if elf_data_chirho.is_some() {
            crate::serial_debug_chirho!(
                "[PROCESS] execve: resolved {} via direct fd {} read",
                filename_chirho,
                fd_chirho,
            );
        }
        return (String::from(filename_chirho), elf_data_chirho);
    }

    (String::from(filename_chirho), try_read_file_chirho(filename_chirho))
}

// ---------------------------------------------------------------------------
// Clone flag constants (from Linux <linux/sched.h>)
// ---------------------------------------------------------------------------

/// Share the virtual memory space with the parent.
pub const CLONE_VM_CHIRHO: u64 = 0x0000_0100;
/// Share the filesystem information (cwd, root, umask).
pub const CLONE_FS_CHIRHO: u64 = 0x0000_0200;
/// Share the file descriptor table.
pub const CLONE_FILES_CHIRHO: u64 = 0x0000_0400;
/// Share signal handlers.
pub const CLONE_SIGHAND_CHIRHO: u64 = 0x0000_0800;
/// Create a new thread (same thread group).
pub const CLONE_THREAD_CHIRHO: u64 = 0x0001_0000;
/// Create a new mount namespace.
pub const CLONE_NEWNS_CHIRHO: u64 = 0x0002_0000;

// ---------------------------------------------------------------------------
// wait4 option constants
// ---------------------------------------------------------------------------

/// Return immediately if no child has exited.
pub const WNOHANG_CHIRHO: u32 = 1;
/// Also report stopped (not just terminated) children.
#[allow(dead_code)]
const WUNTRACED_CHIRHO: u32 = 2;

// ---------------------------------------------------------------------------
// Global wait queue for parents blocking in wait4
// ---------------------------------------------------------------------------

/// Global wait queue that parents sleep on while waiting for a child to exit.
///
/// When a child process calls `exit()` and transitions to `ZombieChirho`, the
/// exit path wakes all tasks sleeping on this queue via
/// [`wake_child_exit_waitqueue_chirho`].  This replaces the old poll-and-yield
/// loop in `sys_wait4_chirho` with an efficient sleep/wake mechanism.
pub static CHILD_EXIT_WAITQUEUE_CHIRHO: crate::waitqueue_chirho::WaitQueueChirho =
    crate::waitqueue_chirho::WaitQueueChirho::new_chirho();

/// Wake all parents sleeping on the child-exit wait queue.
///
/// Called from the `exit()` / `exit_group()` syscall path after a child has
/// been marked as `ZombieChirho`.  Each woken parent will re-evaluate its
/// wait4 condition (checking for a matching zombie child) and either reap a
/// child or go back to sleep.
pub fn wake_child_exit_waitqueue_chirho() {
    crate::waitqueue_chirho::wake_up_chirho(&CHILD_EXIT_WAITQUEUE_CHIRHO);
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default kernel stack size (must match task_chirho.rs).
/// Kernel stack size — re-exported from task_chirho to avoid shadowing.
use crate::task_chirho::KERNEL_STACK_SIZE_CHIRHO as DEFAULT_KERNEL_STACK_SIZE_CHIRHO;

/// Maximum length of a filename path from userspace.
const MAX_PATH_LEN_CHIRHO: usize = 4096;

/// Maximum number of argv/envp entries.
const MAX_ARG_COUNT_CHIRHO: usize = 256;

/// Maximum length of a single argv/envp string.
const MAX_ARG_LEN_CHIRHO: usize = 4096;

/// Kernel RFLAGS for the fork/clone child trampoline.
///
/// The child first resumes in `fork_child_return_chirho`, which is a short
/// ring-0 trampoline that rebuilds an `iretq` frame and immediately returns to
/// user mode. Keep interrupts masked in that transient kernel context so the
/// timer cannot preempt the child while the frame is only partially rebuilt.
const FORK_TRAMPOLINE_RFLAGS_CHIRHO: u64 = 0x2;

fn debug_log_fork_frame_chirho(
    kind_chirho: &str,
    child_pid_chirho: u64,
    frame_chirho: &SyscallFrameChirho,
    child_pt_root_chirho: Option<x86_64::PhysAddr>,
) {
    let child_pt_phys_chirho = child_pt_root_chirho.map(|pt_chirho| pt_chirho.as_u64()).unwrap_or(0);
    crate::serial_debug_chirho!(
        "[PROCESS] {} child PID={} user_rip={:#x} user_rsp={:#x} user_rflags={:#x} child_pt={:#x}",
        kind_chirho,
        child_pid_chirho,
        frame_chirho.rcx_chirho,
        frame_chirho.rsp_chirho,
        frame_chirho.r11_chirho,
        child_pt_phys_chirho,
    );
}

// ===========================================================================
// sys_fork_chirho — REAL IMPLEMENTATION
// ===========================================================================

/// `fork()` / `vfork()` — create a child process.
///
/// Fork clones the parent's address space using per-process page tables and
/// copy-on-write sharing for writable user pages.
///
/// The child's saved register context is a copy of the parent's syscall
/// frame with `rax = 0` (so `fork()` returns 0 in the child).  When the
/// scheduler picks the child, it context-switches into a trampoline that
/// restores the syscall frame and returns to userspace via SYSRET.
///
/// Returns the child's PID to the parent.
pub fn sys_fork_chirho(frame_chirho: &SyscallFrameChirho) -> i64 {
    // --- 1. Get the current (parent) task ---
    let parent_arc_chirho = match current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => {
            crate::serial_println_chirho!(
                "[PROCESS] sys_fork: no current task"
            );
            return -EAGAIN_CHIRHO;
        }
    };

    // --- 2. Allocate a new PID ---
    let child_pid_chirho = allocate_pid_chirho();

    // --- 3. Allocate a kernel stack for the child ---
    let child_kstack_base_chirho = allocate_kernel_stack_chirho(DEFAULT_KERNEL_STACK_SIZE_CHIRHO);
    let child_kstack_top_chirho =
        child_kstack_base_chirho + DEFAULT_KERNEL_STACK_SIZE_CHIRHO as u64;

    // --- 4. Build the child task ---
    let child_task_chirho = {
        let parent_chirho = parent_arc_chirho.lock();
        let parent_pid_chirho = parent_chirho.pid_chirho;

        // Copy the parent's syscall frame onto the child's kernel stack.
        // The frame sits at the top of the stack (highest address, growing
        // down).
        let frame_size_chirho = core::mem::size_of::<SyscallFrameChirho>() as u64;
        let frame_dst_chirho = child_kstack_top_chirho - frame_size_chirho;

        // SAFETY: We just allocated this stack; writing the frame is safe.
        unsafe {
            let dst_ptr_chirho = frame_dst_chirho as *mut SyscallFrameChirho;
            // Copy parent's syscall frame.
            core::ptr::write(dst_ptr_chirho, *frame_chirho);
            // Set rax=0 in the child's copy so fork() returns 0 to the child.
            (*dst_ptr_chirho).rax_chirho = 0;
            // CRITICAL: Set IF=1 in the child's user RFLAGS (r11 field).
            // The parent's r11 has IF=0 because FMASK clears it on SYSCALL
            // entry. Without this fix, the child enters userspace with
            // interrupts disabled and can never be preempted or receive
            // signals, causing a silent hang.
            (*dst_ptr_chirho).r11_chirho |= 0x200; // IF flag
        }

        // The child's CpuContextChirho: when switch_context_chirho restores
        // these registers, rip will be fork_child_return_chirho and rsp will
        // point to the SyscallFrameChirho we placed on the stack.
        let mut child_ctx_chirho = CpuContextChirho::zero_chirho();
        // The very first child instructions in BusyBox after fork touch
        // FS-relative TLS immediately. Carry the intended FS base in a
        // scratch callee-saved register so the trampoline can explicitly
        // re-program IA32_FS_BASE right before iretq.
        child_ctx_chirho.rbx_chirho = parent_chirho.fs_base_chirho;
        child_ctx_chirho.rip_chirho = fork_child_return_chirho as *const () as u64;
        child_ctx_chirho.rsp_chirho = frame_dst_chirho;
        child_ctx_chirho.rflags_chirho = FORK_TRAMPOLINE_RFLAGS_CHIRHO;

        // A2-PROC-003: Clone the parent's per-process fd table.  The
        // per-process table is now the authoritative source of truth
        // (no more relying on GLOBAL swap during context switch).
        // Fall back to GLOBAL only if the parent has no per-process table.
        let child_fd_table_chirho = if let Some(ref parent_fdt_chirho) = parent_chirho.fd_table_chirho {
            Some(parent_fdt_chirho.clone_table_chirho())
        } else {
            let global_fd_chirho = crate::fs_chirho::GLOBAL_FD_TABLE_CHIRHO.lock();
            global_fd_chirho.as_ref().map(|t_chirho| t_chirho.clone_table_chirho())
        };

        // Create a per-process page table for the child.
        // If parent has one, clone it with COW. Otherwise create a fresh
        // PT (copies boot PML4 entries) — the child gets its own address
        // space so the parent's post-fork stack writes don't corrupt it.
        let child_pt_root_chirho = match parent_chirho.page_table_root_chirho {
            Some(parent_pml4_chirho) => {
                // Mark parent's writable user pages as COW before cloning.
                // Without this, parent and child share physical frames
                // WITHOUT COW protection — child writes corrupt parent's
                // musl linker GOT/data, causing GPF in the parent.
                let cow_count_chirho = crate::pagetable_chirho::mark_user_pages_cow_chirho(parent_pml4_chirho);
                crate::serial_debug_chirho!(
                    "[FORK] Marked {} per-process PT pages as COW for child",
                    cow_count_chirho,
                );
                crate::pagetable_chirho::clone_page_table_chirho(parent_pml4_chirho)
            }
            None => {
                // Parent runs on boot PML4. Use COW to protect shared pages:
                // 1. Mark writable user pages in boot PML4 as read-only + COW
                // 2. Clone the boot PML4 for the child (clone_page_table_chirho
                //    creates its own PDPT/PD/PT frames, sharing leaf physical
                //    frames marked as COW)
                // 3. When either parent or child writes, the page fault handler
                //    calls handle_cow_fault_chirho to copy the page
                let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
                let cow_count_chirho = crate::pagetable_chirho::mark_user_pages_cow_chirho(boot_pml4_chirho);
                crate::serial_println_chirho!(
                    "[FORK] Marked {} boot PML4 user pages as COW, cloning for child",
                    cow_count_chirho,
                );
                crate::pagetable_chirho::clone_page_table_chirho(boot_pml4_chirho)
            }
        };

        debug_log_fork_frame_chirho(
            "fork",
            child_pid_chirho,
            frame_chirho,
            child_pt_root_chirho,
        );

        TaskChirho {
            pid_chirho: child_pid_chirho,
            tgid_chirho: child_pid_chirho,
            ppid_chirho: parent_pid_chirho,
            state_chirho: TaskStateChirho::ReadyChirho,
            exit_code_chirho: 0,
            context_chirho: child_ctx_chirho,
            kernel_stack_chirho: child_kstack_top_chirho,
            kernel_stack_size_chirho: DEFAULT_KERNEL_STACK_SIZE_CHIRHO,
            user_rsp_chirho: frame_chirho.rsp_chirho,
            preempted_rip_chirho: 0,
            page_table_root_chirho: child_pt_root_chirho,
            next_fd_chirho: parent_chirho.next_fd_chirho,
            fd_table_chirho: child_fd_table_chirho,
            priority_chirho: parent_chirho.priority_chirho,
            time_slice_chirho: parent_chirho.time_slice_chirho,
            uid_chirho: parent_chirho.uid_chirho,
            gid_chirho: parent_chirho.gid_chirho,
            euid_chirho: parent_chirho.euid_chirho,
            egid_chirho: parent_chirho.egid_chirho,
            saved_uid_chirho: parent_chirho.saved_uid_chirho,
            saved_gid_chirho: parent_chirho.saved_gid_chirho,
            supplementary_groups_chirho: parent_chirho.supplementary_groups_chirho.clone(),
            comm_chirho: parent_chirho.comm_chirho,
            fs_base_chirho: parent_chirho.fs_base_chirho,
            gs_base_chirho: parent_chirho.gs_base_chirho,
            signal_mask_chirho: parent_chirho.signal_mask_chirho,
            pending_signals_chirho: 0, // pending signals are not inherited
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: parent_chirho.brk_chirho,
            brk_start_chirho: parent_chirho.brk_start_chirho,
            cwd_chirho: parent_chirho.cwd_chirho.clone(),
            sid_chirho: parent_chirho.sid_chirho,
            pgid_chirho: parent_chirho.pgid_chirho,
            controlling_tty_chirho: parent_chirho.controlling_tty_chirho,
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] fork: parent PID={} -> child PID={}",
        child_task_chirho.ppid_chirho,
        child_pid_chirho
    );

    // --- 5. Sync context to static slot BEFORE moving into Arc ---
    crate::task_chirho::sync_context_to_slot_chirho(
        child_pid_chirho,
        &child_task_chirho.context_chirho,
    );

    // --- 6. Register the child in the global task list ---
    let child_arc_chirho = Arc::new(Mutex::new(child_task_chirho));
    register_task_chirho(Arc::clone(&child_arc_chirho));

    // --- 7. Add the child to the scheduler run queue ---
    crate::scheduler_chirho::add_task_chirho(child_pid_chirho);

    // --- 8. Real fork with IRETQ return ---
    child_pid_chirho as i64
}

// ===========================================================================
// sys_clone_chirho — REAL IMPLEMENTATION
// ===========================================================================

/// `clone(flags, stack, parent_tid, child_tid, tls)` — create a child
/// process/thread with fine-grained sharing control.
///
/// If `CLONE_VM` is set, the child shares the parent's address space
/// (thread-like).  Otherwise, this behaves like `fork()`.
///
/// If a non-zero `stack_chirho` is provided, the child uses that as its
/// user-space stack pointer; otherwise it inherits the parent's.
pub fn sys_clone_chirho(
    flags_chirho: u64,
    stack_chirho: u64,
    _parent_tid_chirho: u64,
    _child_tid_chirho: u64,
    _tls_chirho: u64,
    frame_chirho: &SyscallFrameChirho,
) -> i64 {
    crate::serial_debug_chirho!(
        "[PROCESS] sys_clone(flags={:#x}, stack={:#x})",
        flags_chirho,
        stack_chirho
    );

    // For now, we treat all clone calls as fork (vfork-style).
    // CLONE_VM (share memory) is the default anyway since we share page
    // tables.  CLONE_FILES is handled by sharing vs. duplicating the FD
    // table.

    let parent_arc_chirho = match current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return -EAGAIN_CHIRHO,
    };

    let child_pid_chirho = allocate_pid_chirho();
    let child_kstack_base_chirho = allocate_kernel_stack_chirho(DEFAULT_KERNEL_STACK_SIZE_CHIRHO);
    let child_kstack_top_chirho =
        child_kstack_base_chirho + DEFAULT_KERNEL_STACK_SIZE_CHIRHO as u64;

    let child_task_chirho = {
        let parent_chirho = parent_arc_chirho.lock();
        let parent_pid_chirho = parent_chirho.pid_chirho;

        // Copy syscall frame to child's kernel stack with rax=0.
        let frame_size_chirho = core::mem::size_of::<SyscallFrameChirho>() as u64;
        let frame_dst_chirho = child_kstack_top_chirho - frame_size_chirho;

        unsafe {
            let dst_ptr_chirho = frame_dst_chirho as *mut SyscallFrameChirho;
            core::ptr::write(dst_ptr_chirho, *frame_chirho);
            (*dst_ptr_chirho).rax_chirho = 0;
            // Set IF=1 in child's user RFLAGS (parent's r11 has IF=0 from FMASK)
            (*dst_ptr_chirho).r11_chirho |= 0x200;
            // If a custom stack was provided, use it for the child's
            // user-space RSP.
            if stack_chirho != 0 {
                (*dst_ptr_chirho).rsp_chirho = stack_chirho;
            }
        }

        let mut child_ctx_chirho = CpuContextChirho::zero_chirho();
        child_ctx_chirho.rbx_chirho = parent_chirho.fs_base_chirho;
        child_ctx_chirho.rip_chirho = fork_child_return_chirho as *const () as u64;
        child_ctx_chirho.rsp_chirho = frame_dst_chirho;
        child_ctx_chirho.rflags_chirho = FORK_TRAMPOLINE_RFLAGS_CHIRHO;

        // A2-PROC-003: Clone the parent's per-process fd table (authoritative).
        // Fall back to GLOBAL only if the parent has no per-process table.
        let child_fd_table_chirho = if let Some(ref parent_fdt_chirho) = parent_chirho.fd_table_chirho {
            Some(parent_fdt_chirho.clone_table_chirho())
        } else {
            crate::fs_chirho::GLOBAL_FD_TABLE_CHIRHO.lock()
                .as_ref()
                .map(|t_chirho| t_chirho.clone_table_chirho())
        };

        let child_user_rsp_chirho = if stack_chirho != 0 {
            stack_chirho
        } else {
            frame_chirho.rsp_chirho
        };

        // If CLONE_THREAD is set, share the thread group ID.
        let child_tgid_chirho = if (flags_chirho & CLONE_THREAD_CHIRHO) != 0 {
            parent_chirho.tgid_chirho
        } else {
            child_pid_chirho
        };

        // Clone page table: CLONE_VM shares the address space (thread),
        // otherwise clone with COW.
        let child_pt_root_chirho = if (flags_chirho & CLONE_VM_CHIRHO) != 0 {
            // Threads share the parent's page table.
            parent_chirho.page_table_root_chirho
        } else {
            // Fork: clone with COW.
            match parent_chirho.page_table_root_chirho {
                Some(parent_pml4_chirho) => {
                    crate::pagetable_chirho::clone_page_table_chirho(parent_pml4_chirho)
                }
                None => {
                    let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
                    let _ = crate::pagetable_chirho::mark_user_pages_cow_chirho(boot_pml4_chirho);
                    crate::pagetable_chirho::clone_page_table_chirho(boot_pml4_chirho)
                }
            }
        };

        debug_log_fork_frame_chirho(
            "clone",
            child_pid_chirho,
            frame_chirho,
            child_pt_root_chirho,
        );

        TaskChirho {
            pid_chirho: child_pid_chirho,
            tgid_chirho: child_tgid_chirho,
            ppid_chirho: parent_pid_chirho,
            state_chirho: TaskStateChirho::ReadyChirho,
            exit_code_chirho: 0,
            context_chirho: child_ctx_chirho,
            kernel_stack_chirho: child_kstack_top_chirho,
            kernel_stack_size_chirho: DEFAULT_KERNEL_STACK_SIZE_CHIRHO,
            user_rsp_chirho: child_user_rsp_chirho,
            preempted_rip_chirho: 0,
            page_table_root_chirho: child_pt_root_chirho,
            next_fd_chirho: parent_chirho.next_fd_chirho,
            fd_table_chirho: child_fd_table_chirho,
            priority_chirho: parent_chirho.priority_chirho,
            time_slice_chirho: parent_chirho.time_slice_chirho,
            uid_chirho: parent_chirho.uid_chirho,
            gid_chirho: parent_chirho.gid_chirho,
            euid_chirho: parent_chirho.euid_chirho,
            egid_chirho: parent_chirho.egid_chirho,
            saved_uid_chirho: parent_chirho.saved_uid_chirho,
            saved_gid_chirho: parent_chirho.saved_gid_chirho,
            supplementary_groups_chirho: parent_chirho.supplementary_groups_chirho.clone(),
            comm_chirho: parent_chirho.comm_chirho,
            fs_base_chirho: parent_chirho.fs_base_chirho,
            gs_base_chirho: parent_chirho.gs_base_chirho,
            signal_mask_chirho: parent_chirho.signal_mask_chirho,
            pending_signals_chirho: 0,
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: parent_chirho.brk_chirho,
            brk_start_chirho: parent_chirho.brk_start_chirho,
            cwd_chirho: parent_chirho.cwd_chirho.clone(),
            sid_chirho: parent_chirho.sid_chirho,
            pgid_chirho: parent_chirho.pgid_chirho,
            controlling_tty_chirho: parent_chirho.controlling_tty_chirho,
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] clone: parent PID={} -> child PID={} (flags={:#x})",
        child_task_chirho.ppid_chirho,
        child_pid_chirho,
        flags_chirho
    );

    // Sync context to static slot BEFORE moving into Arc
    crate::task_chirho::sync_context_to_slot_chirho(
        child_pid_chirho,
        &child_task_chirho.context_chirho,
    );

    let child_arc_chirho = Arc::new(Mutex::new(child_task_chirho));
    register_task_chirho(Arc::clone(&child_arc_chirho));
    crate::scheduler_chirho::add_task_chirho(child_pid_chirho);

    child_pid_chirho as i64
}

// ===========================================================================
// sys_wait4_chirho — REAL IMPLEMENTATION
// ===========================================================================

/// `wait4(pid, wstatus, options, rusage)` — wait for a child process.
///
/// Searches for zombie children of the calling process:
/// - If `pid_chirho > 0`, wait for the specific child.
/// - If `pid_chirho == -1`, wait for any child.
/// - If a zombie child is found, reap it: write the exit status to
///   `wstatus_chirho` (if non-null) in `WEXITSTATUS` format (code << 8),
///   mark the child as Dead, and return the child's PID.
/// - If `WNOHANG` is set and no child is a zombie, return 0.
/// - Otherwise, sleep on `CHILD_EXIT_WAITQUEUE_CHIRHO` until a child exits.
pub fn sys_wait4_chirho(
    pid_chirho: i64,
    wstatus_chirho: u64,
    options_chirho: u32,
    _rusage_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!(
        "[PROCESS] sys_wait4(pid={}, wstatus={:#x}, options={:#x})",
        pid_chirho,
        wstatus_chirho,
        options_chirho,
    );

    // Get the current task's PID (the parent).
    let parent_pid_chirho = match current_task_chirho() {
        Some(t_chirho) => t_chirho.lock().pid_chirho,
        None => return -ECHILD_CHIRHO,
    };

    // -----------------------------------------------------------------------
    // Helper closure: scan the task list for a matching zombie child.
    // Returns (zombie_pid, exit_code) or None.  Also returns -ECHILD sentinel
    // via a separate flag if there are no living children at all.
    // -----------------------------------------------------------------------
    let find_zombie_chirho = |parent_pid_arg_chirho: u64, pid_filter_chirho: i64|
        -> Result<(u64, i32), i64>
    {
        let task_list_chirho = TASK_LIST_CHIRHO.lock();

        // First: do we have any non-dead children?
        let has_children_chirho = task_list_chirho
            .iter()
            .any(|t_chirho| {
                let tc_chirho = t_chirho.lock();
                tc_chirho.ppid_chirho == parent_pid_arg_chirho
                    && tc_chirho.state_chirho != TaskStateChirho::DeadChirho
            });

        if !has_children_chirho {
            return Err(-ECHILD_CHIRHO);
        }

        // Look for a zombie child matching the PID filter.
        for task_arc_chirho in task_list_chirho.iter() {
            let task_chirho = task_arc_chirho.lock();

            // Must be our child.
            if task_chirho.ppid_chirho != parent_pid_arg_chirho {
                continue;
            }

            // Check PID filter.
            if pid_filter_chirho > 0
                && task_chirho.pid_chirho != pid_filter_chirho as u64
            {
                continue;
            }

            // Check if zombie.
            if task_chirho.state_chirho == TaskStateChirho::ZombieChirho {
                return Ok((task_chirho.pid_chirho, task_chirho.exit_code_chirho));
            }
        }

        // Children exist but none are zombies yet.
        Err(0)
    };

    // -----------------------------------------------------------------------
    // Main wait4 logic
    // -----------------------------------------------------------------------

    // First attempt (before potentially sleeping).
    match find_zombie_chirho(parent_pid_chirho, pid_chirho) {
        Err(code_chirho) if code_chirho == -ECHILD_CHIRHO => return -ECHILD_CHIRHO,
        Ok((reaped_pid_chirho, exit_code_chirho)) => {
            return reap_child_chirho(
                reaped_pid_chirho,
                exit_code_chirho,
                wstatus_chirho,
            );
        }
        _ => {} // no zombie yet — fall through
    }

    // WNOHANG: promote the child to front of the scheduler queue so it
    // gets CPU via the preemption trampoline. Don't yield here — let the
    // caller return to userspace and run its event loop first.
    if (options_chirho & WNOHANG_CHIRHO) != 0 {
        // Find the highest-PID living child and promote it
        let child_pid_chirho = {
            let task_list_chirho = TASK_LIST_CHIRHO.lock();
            task_list_chirho.iter()
                .filter_map(|t_chirho| {
                    let tc_chirho = t_chirho.lock();
                    if tc_chirho.ppid_chirho == parent_pid_chirho
                        && tc_chirho.state_chirho != TaskStateChirho::ZombieChirho
                        && tc_chirho.state_chirho != TaskStateChirho::DeadChirho {
                        Some(tc_chirho.pid_chirho)
                    } else { None }
                })
                .max()
        };
        if let Some(cp_chirho) = child_pid_chirho {
            crate::scheduler_chirho::promote_task_chirho(cp_chirho);
        }
        return 0;
    }

    // Block until a child exits using a proper waitqueue. The task is
    // REMOVED from the run queue (giving PID 4 full CPU) and woken by
    // wake_child_exit_waitqueue_chirho when any child calls exit_group.
    crate::waitqueue_chirho::wait_event_chirho(
        &CHILD_EXIT_WAITQUEUE_CHIRHO,
        || find_zombie_chirho(parent_pid_chirho, pid_chirho).is_ok(),
    );

    // After being woken, re-check and reap
    {
        // dummy block to match the old code structure
        let caller_pid_chirho = parent_pid_chirho;
        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        for task_arc_chirho in list_chirho.iter() {
            let task_chirho = task_arc_chirho.lock();
            if task_chirho.ppid_chirho == caller_pid_chirho
                && task_chirho.is_exited_chirho()
                && (pid_chirho == -1 || pid_chirho == task_chirho.pid_chirho as i64)
            {
                let reaped_pid_chirho = task_chirho.pid_chirho;
                let exit_code_chirho = task_chirho.exit_code_chirho;
                drop(task_chirho);
                drop(list_chirho);
                return reap_child_chirho(
                    reaped_pid_chirho,
                    exit_code_chirho,
                    wstatus_chirho,
                );
            }
        }
    }
    -ECHILD_CHIRHO
}

// Dead code removed — wait4 polling is now the main path above.

/// Reap a zombie child: write exit status, mark Dead, remove from scheduler.
///
/// Shared helper used by the initial scan and the wait-queue wakeup path in
/// [`sys_wait4_chirho`].
fn reap_child_chirho(
    reaped_pid_chirho: u64,
    exit_code_chirho: i32,
    wstatus_chirho: u64,
) -> i64 {
    // Write the exit status to userspace if wstatus pointer is non-null.
    if wstatus_chirho != 0 {
        // WEXITSTATUS format: (exit_code << 8) | 0 (normal termination).
        let wstatus_val_chirho: i32 = (exit_code_chirho & 0xFF) << 8;
        let wstatus_ptr_chirho = wstatus_chirho as *mut i32;
        if crate::uaccess_chirho::is_user_address_chirho(
            wstatus_ptr_chirho as u64,
            core::mem::size_of::<i32>() as u64,
        ) {
            unsafe {
                core::ptr::write(wstatus_ptr_chirho, wstatus_val_chirho);
            }
        }
    }

    // Mark the child as Dead (fully reaped).
    {
        let task_list_chirho = TASK_LIST_CHIRHO.lock();
        for task_arc_chirho in task_list_chirho.iter() {
            let mut task_chirho = task_arc_chirho.lock();
            if task_chirho.pid_chirho == reaped_pid_chirho {
                task_chirho.state_chirho = TaskStateChirho::DeadChirho;
                break;
            }
        }
    }

    // Remove the dead child from the scheduler (should already be
    // gone, but be safe).
    crate::scheduler_chirho::remove_task_chirho(reaped_pid_chirho);

    crate::serial_debug_chirho!(
        "[PROCESS] wait4: reaped child PID={}, exit_code={}",
        reaped_pid_chirho,
        exit_code_chirho
    );

    // Force-exit the SSH session handler after its exec'd child exits
    // and the TCP connection is closed/closing. Without this, PID 3's
    // select/read loop never detects EOF because is_socket_fd(0) returns
    // false (fd table desync between per-process and global tables after
    // the child's exec overwrote the global table).
    {
        let caller_pid_chirho = crate::task_chirho::current_task_chirho()
            .map(|t| t.lock().pid_chirho).unwrap_or(0);
        if caller_pid_chirho >= 3
            && crate::net_chirho::has_closewait_tcp_chirho(2222)
        {
            // CloseWait not yet — client hasn't sent FIN.
            // The force-exit is handled in select's timeout fallthrough.
        }
    }

    // Return the reaped PID to the parent.
    reaped_pid_chirho as i64
}

// ===========================================================================
// Fork child return trampoline
// ===========================================================================

/// Trampoline function that the child task "returns" into after the first
/// context switch.
///
/// When `switch_context_chirho` restores the child's `CpuContextChirho`,
/// `rip` points here and `rsp` points to the `SyscallFrameChirho` we placed
/// on the child's kernel stack.  This function pops the frame and does
/// SYSRET back to userspace.
///
/// The SyscallFrameChirho has `rax=0`, so the child sees `fork()` returning 0.
/// Naked trampoline — NO Rust prologue.  The context switch sets RSP to
/// point at the SyscallFrameChirho on the child's kernel stack.
/// We read the frame, restore registers, and SYSRET to userspace.
#[unsafe(naked)]
unsafe extern "C" fn fork_child_return_chirho() {
    core::arch::naked_asm!(
        // The child starts in a transient kernel trampoline, not at a normal
        // syscall entry. Keep interrupts masked until `iretq` publishes the
        // final user RIP/RSP/RFLAGS atomically.
        "cli",

        // Explicitly re-program IA32_FS_BASE from the child context before
        // userspace runs. BusyBox's child path after fork dereferences FS:0
        // almost immediately, so this tests whether the scheduler's earlier
        // FS restore is sometimes getting lost before the child hits userspace.
        //
        // On first entry to this trampoline, RBX carries the child's intended
        // FS base from CpuContextChirho. We are free to clobber it here
        // because the real user RBX is restored later from the syscall frame.
        "mov ecx, 0xC0000100",
        "mov eax, ebx",
        "shr rbx, 32",
        "mov edx, ebx",
        "wrmsr",

        // RSP points to the SyscallFrameChirho on the child's kernel stack.
        // Layout (offsets from RSP):
        //   0x00: rax (= 0, fork return value)
        //   0x08: rdi     0x10: rsi     0x18: rdx
        //   0x20: r10     0x28: r8      0x30: r9
        //   0x38: rcx (user RIP)
        //   0x40: r11 (user RFLAGS)
        //   0x48: rsp (user RSP)
        //   0x50: rbx     0x58: rbp
        //   0x60: r12     0x68: r13     0x70: r14     0x78: r15
        //
        // Strategy: save frame pointer, read user RIP/RFLAGS/RSP into
        // scratch registers, restore all GPRs, then build IRETQ frame
        // on a clean area of the kernel stack.

        // Step 1: Save the frame base pointer so we can read it after
        //         restoring all registers.  Use r15 as temporary (we
        //         restore it last).
        "mov r15, rsp",                     // r15 = frame base

        // Step 2: Read the three IRETQ inputs into callee-saved regs
        //         that we will restore AFTER building the IRETQ frame.
        //         r12 = user RIP,  r13 = user RFLAGS,  r14 = user RSP
        "mov r12, [r15 + 0x38]",            // user RIP  (was in rcx)
        "mov r13, [r15 + 0x40]",            // user RFLAGS (was in r11)
        "mov r14, [r15 + 0x48]",            // user RSP

        // Step 3: Restore all GPRs from the frame EXCEPT r12-r15
        //         (we're using those as scratch for IRETQ values).
        "xor eax, eax",                     // rax = 0 (fork return)
        "mov rdi, [r15 + 0x08]",
        "mov rsi, [r15 + 0x10]",
        "mov rdx, [r15 + 0x18]",
        "mov r10, [r15 + 0x20]",
        "mov r8,  [r15 + 0x28]",
        "mov r9,  [r15 + 0x30]",
        "mov rcx, [r15 + 0x38]",            // rcx = user RIP (also in r12)
        "mov r11, [r15 + 0x40]",            // r11 = user RFLAGS (also in r13)
        "mov rbx, [r15 + 0x50]",
        "mov rbp, [r15 + 0x58]",

        // Step 4: Build IRETQ frame DEEP in the kernel stack so the
        //         leftover IRETQ values (user-mode SS/RSP/RFLAGS/CS/RIP)
        //         don't interfere with future syscall frames at the stack top.
        //         The kernel stack is 64KB; placing the IRETQ frame 32KB down
        //         leaves plenty of room for syscall call chains above.
        "lea rsp, [r15 - 32768]",

        // Push IRETQ frame (reverse order: SS, RSP, RFLAGS, CS, RIP)
        "push 0x23",                         // SS  = user data segment
        "push r14",                          // RSP = user stack pointer
        "push r13",                          // RFLAGS = user flags
        "push 0x2B",                         // CS  = user code segment (64-bit)
        "push r12",                          // RIP = user return address

        // Step 5: Restore the remaining callee-saved regs (r12-r15)
        //         from the frame.  We use r15 (frame base) to read them.
        "mov r12, [r15 + 0x60]",
        "mov r13, [r15 + 0x68]",
        "mov r14, [r15 + 0x70]",
        "mov r15, [r15 + 0x78]",            // last use of frame pointer

        // Step 7: Switch GS base from kernel to user before IRETQ.
        // Even though our SYSCALL entry doesn't use swapgs, the CPU
        // requires proper GS state for exceptions in user mode.
        "swapgs",

        // Step 7: Return to userspace.  IRETQ pops RIP, CS, RFLAGS,
        //         RSP, SS — atomically switching to ring 3.
        "iretq",
    );
}

// ===========================================================================
// sys_execve_chirho — REAL IMPLEMENTATION
// ===========================================================================

/// `execve(filename, argv, envp)` — execute a new program.
///
/// Replaces the current process image with a new program loaded from an ELF
/// binary. On success, this function never returns — it jumps to the new
/// program's entry point via IRETQ. On failure, returns a negative errno.
///
/// # Steps
///
/// 1. Read the filename string from userspace.
/// 2. Read argv and envp pointer arrays from userspace.
/// 3. Look up the file in the VFS (or fall back to the embedded hello-chirho
///    binary for testing).
/// 4. Parse and load the ELF binary into user memory.
/// 5. Set up the user stack with argv/envp in the standard Linux layout.
/// 6. Jump to userspace via IRETQ (never returns on success).
pub fn sys_execve_chirho(
    filename_chirho: u64,
    argv_chirho: u64,
    envp_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!(
        "[PROCESS] sys_execve called (filename={:#x}, argv={:#x}, envp={:#x})",
        filename_chirho,
        argv_chirho,
        envp_chirho,
    );

    // -----------------------------------------------------------------------
    // Step 1: Read filename from userspace
    // -----------------------------------------------------------------------
    let filename_str_chirho = match read_user_string_chirho(filename_chirho, MAX_PATH_LEN_CHIRHO) {
        Ok(s_chirho) => s_chirho,
        Err(err_chirho) => {
            crate::serial_println_chirho!(
                "[PROCESS] execve: failed to read filename from userspace: {:?}",
                err_chirho
            );
            return -EFAULT_CHIRHO;
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] execve: filename = \"{}\"",
        filename_str_chirho
    );

    sys_execve_with_filename_chirho(filename_str_chirho, argv_chirho, envp_chirho)
}

/// `execve()` helper when the kernel already has a resolved filename string.
///
/// This is used by plain `execve`, procfd fallbacks, and `execveat`
/// path resolution so the actual ELF-loading path stays shared.
pub fn sys_execve_with_filename_chirho(
    filename_str_chirho: String,
    argv_chirho: u64,
    envp_chirho: u64,
) -> i64 {
    // Track whether this is a procfd (fexecve) exec so we can preserve
    // socket fds across exec for dropbear's `-2 N` connection passing.
    // Check BOTH the current filename AND the resolve_exec_source result
    // (the filename might already be resolved to the actual binary path
    // by the execveat handler before we're called).
    let is_procfd_exec_chirho = filename_str_chirho.contains("/proc/self/fd/")
        || IS_PROCFD_EXEC_FLAG_CHIRHO.swap(false, core::sync::atomic::Ordering::Relaxed);

    // -----------------------------------------------------------------------
    // Step 2: Read argv array from userspace
    // -----------------------------------------------------------------------
    let argv_vec_chirho = match read_user_string_array_chirho(argv_chirho) {
        Ok(v_chirho) => v_chirho,
        Err(errno_chirho) => {
            crate::serial_println_chirho!(
                "[PROCESS] execve: failed to read argv: errno={}",
                errno_chirho
            );
            return errno_chirho;
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] execve: argv ({} entries): {:?}",
        argv_vec_chirho.len(),
        argv_vec_chirho
    );

    // -----------------------------------------------------------------------
    // Step 3: Read envp array from userspace
    // -----------------------------------------------------------------------
    let envp_vec_chirho = match read_user_string_array_chirho(envp_chirho) {
        Ok(v_chirho) => v_chirho,
        Err(errno_chirho) => {
            crate::serial_println_chirho!(
                "[PROCESS] execve: failed to read envp: errno={}",
                errno_chirho
            );
            return errno_chirho;
        }
    };

    crate::serial_debug_chirho!(
        "[PROCESS] execve: envp ({} entries)",
        envp_vec_chirho.len()
    );

    // NOTE: No COW page cleanup needed here. The fork marks boot PML4 pages
    // as COW. The COW handler resolves them on write (allocating new frames).
    // After exec, the old process's pages are abandoned — the new binary gets
    // fresh page mappings via GLOBAL_MAPPER. Any COW pages that haven't been
    // resolved will be resolved on first write (lazy COW).

    // -----------------------------------------------------------------------
    // Step 4: Obtain the ELF binary data
    // -----------------------------------------------------------------------
    // Resolve procfd exec paths used by libc fallbacks when execveat is
    // missing. This is enough for Dropbear's "/proc/self/fd/N" re-exec path
    // without needing a full /proc/<pid>/fd implementation.
    let (resolved_filename_chirho, elf_data_owned_chirho) =
        resolve_exec_source_chirho(&filename_str_chirho);

    // Try to resolve the file via the VFS first. If that fails, fall back to
    // the embedded hello-chirho binary (useful for early testing before the
    // filesystem has real binaries).
    // Read the ELF into a Vec — we'll drop it after mapping into user pages.
    // Try to load the binary from the VFS (Alpine rootfs on ext4).
    // If not found, fall back to embedded BusyBox for known applets.
    // A real kernel doesn't embed BusyBox — the embedded copy is only
    // a fallback for early boot before the disk is mounted.
    let basename_chirho = filename_str_chirho
        .rsplit('/')
        .next()
        .unwrap_or(&filename_str_chirho);

    let elf_data_chirho: &[u8] = match &elf_data_owned_chirho {
        Some(data_chirho) => {
            data_chirho.as_slice()
        }
        None => {
            // Not on disk — check if it's a BusyBox applet
            if crate::busybox_chirho::is_busybox_applet_chirho(basename_chirho) {
                crate::exec_chirho::BUSYBOX_ELF_CHIRHO
            } else {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: \"{}\" not found in VFS — returning ENOENT",
                    filename_str_chirho
                );
                return -ENOENT_CHIRHO;
            }
        }
    };

    // -----------------------------------------------------------------------
    // Step 4b: Per-process page tables — record for future use.
    // -----------------------------------------------------------------------
    // We DON'T switch CR3 here because the mapper (OffsetPageTable) is bound
    // to the current PML4. Switching CR3 before loading the ELF would map
    // segments into the old page table while the CPU uses the new one.
    // Instead, we store the new PML4 in the task descriptor. The scheduler
    // will switch CR3 when it picks this task.
    //
    // NOTE: For vfork semantics (current model), all processes share the
    // same address space anyway. Per-process page tables become useful once
    // real fork + preemptive scheduling are enabled.
    // Create a new per-process PT for all exec except embedded static BusyBox.
    let is_embedded_static_exec_chirho = !is_procfd_exec_chirho
        && crate::task_chirho::current_task_chirho()
            .map(|t| t.lock().pid_chirho >= 4)
            .unwrap_or(false);
    if !is_embedded_static_exec_chirho {
        let _new_pt_chirho = crate::pagetable_chirho::create_user_page_table_chirho();
        if let Some(pt_root_chirho) = _new_pt_chirho {
            crate::serial_debug_chirho!(
                "[PROCESS] execve: created per-process page table PML4={:#x}",
                pt_root_chirho.as_u64(),
            );
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                task_arc_chirho.lock().page_table_root_chirho = Some(pt_root_chirho);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step 4b: Switch to boot PML4 and clear stale user pages.
    //
    // After fork, this process runs on its per-process PT. The exec path
    // loads ELF segments via mmap/GLOBAL_MAPPER onto the boot PML4.
    // We must clear stale user pages from boot PML4 so the new binary
    // doesn't inherit old mappings.
    //
    // IMPORTANT: We save and restore boot PML4 user entries because
    // OTHER processes (e.g., parent PID 2) may have pages lazily backed
    // by boot PML4 entries. Clearing them all causes those processes to
    // page fault when they access their mmap'd pages.
    // -----------------------------------------------------------------------
    {
        let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
        // Switch to boot PML4 for ALL exec EXCEPT embedded static BusyBox
        // (PID >= 4 non-procfd). Static BusyBox loads onto the fork-inherited
        // per-process PT to avoid corrupting parent's musl library pages.
        let is_embedded_static_chirho = !is_procfd_exec_chirho
            && crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().pid_chirho >= 4)
                .unwrap_or(false);
        if !is_embedded_static_chirho && boot_pml4_chirho.as_u64() != 0 {
            unsafe {
                crate::pagetable_chirho::switch_page_table_chirho(boot_pml4_chirho);
            }
        }
        // For procfd exec (dropbear re-exec): clear ALL stale user pages.
        // Stale pages from previous sessions' shared libraries (libutmps)
        // corrupt the new session's library loading ("Exec format error").
        // For normal exec (BusyBox child): only restore COW to writable.
        // Clearing all pages would destroy the parent session's library
        // pages, causing page faults during pipe relay.
        if is_procfd_exec_chirho {
            let cleared_chirho = crate::pagetable_chirho::clear_user_pages_chirho(boot_pml4_chirho);
            // Re-map the user preemption trampoline — it was cleared along
            // with all other user pages. Reset the READY flag first so init
            // doesn't skip the re-mapping.
            crate::interrupts_chirho::reset_user_preempt_trampoline_ready_chirho();
            crate::interrupts_chirho::init_user_preempt_trampoline_chirho();
            crate::serial_debug_chirho!(
                "[PROCESS] execve: procfd — cleared {} stale user pages + re-mapped trampoline",
                cleared_chirho,
            );
        } else if is_embedded_static_chirho {
            // Embedded static BusyBox: DON'T touch boot PML4.
            // Load ELF directly onto the fork-inherited per-process PT.
            // BusyBox at 0x400000 doesn't conflict with parent's dropbear
            // at 0x555555550000 or musl at 0x7f0000100000.
            crate::serial_debug_chirho!(
                "[PROCESS] execve: embedded static — loading onto fork PT",
            );
        } else {
            // Normal non-procfd exec (PID 0/1 shell, PID 2 initial dropbear):
            // restore COW to writable on boot PML4.
            let restored_chirho = crate::pagetable_chirho::restore_cow_to_writable_chirho(boot_pml4_chirho);
            crate::serial_debug_chirho!(
                "[PROCESS] execve: normal — restored {} COW pages to writable",
                restored_chirho,
            );
        }
    }

    // -----------------------------------------------------------------------
    // Step 5: Check for PT_INTERP (dynamic linking) and load the ELF
    // -----------------------------------------------------------------------
    // If argv is empty, use the filename as argv[0] (standard behaviour).
    let effective_argv_chirho = if argv_vec_chirho.is_empty() {
        alloc::vec![filename_str_chirho.clone()]
    } else {
        argv_vec_chirho
    };

    // Check whether this ELF has a PT_INTERP segment (dynamically linked).
    let interp_path_chirho = dynlink_chirho::find_interp_in_phdrs_chirho(elf_data_chirho);

    // DIAGNOSTIC: For PID >= 4 (SSH exec children), use the embedded
    // static BusyBox to bypass dynamic linking issues. This lets us
    // verify the SSH pipeline works before fixing dynlink.
    let current_pid_for_exec_chirho = crate::task_chirho::current_task_chirho()
        .map(|t| t.lock().pid_chirho)
        .unwrap_or(0);
    let (elf_data_chirho, interp_path_chirho) = if interp_path_chirho.is_some()
        && current_pid_for_exec_chirho >= 4
        && crate::busybox_chirho::is_busybox_applet_chirho(basename_chirho)
    {
        crate::serial_println_chirho!(
            "[PROCESS] execve: PID {} using embedded static BusyBox for '{}'",
            current_pid_for_exec_chirho, basename_chirho,
        );
        (crate::exec_chirho::BUSYBOX_ELF_CHIRHO, None)
    } else {
        (elf_data_chirho, interp_path_chirho)
    };

    if let Some(ref raw_interp_path_chirho) = interp_path_chirho {
        // ---------------------------------------------------------------
        // P4-004: Dynamically linked ELF — load interpreter from ext4
        // ---------------------------------------------------------------
        crate::serial_debug_chirho!(
            "[PROCESS] execve: PT_INTERP detected: \"{}\"",
            raw_interp_path_chirho
        );

        // Resolve the interpreter path. The ELF says e.g.
        // "/lib/ld-musl-x86_64.so.1" but the Alpine rootfs is mounted
        // at /mnt, so the actual file is /mnt/lib/ld-musl-x86_64.so.1.
        // If the filename itself starts with /mnt, the rootfs is /mnt.
        // Otherwise try the raw path first, then /mnt-prefixed.
        let interp_resolved_path_chirho = resolve_interp_path_chirho(
            raw_interp_path_chirho,
            &filename_str_chirho,
        );

        crate::serial_debug_chirho!(
            "[PROCESS] execve: resolved interpreter path: \"{}\"",
            interp_resolved_path_chirho
        );

        // Read the interpreter binary from the VFS (ext4).
        let interp_data_vec_chirho = match try_read_file_chirho(&interp_resolved_path_chirho) {
            Some(data_chirho) => {
                // Log first 32 bytes to verify ELF header integrity
                let hdr_hex_chirho: alloc::string::String = data_chirho.iter().take(32)
                    .map(|b| alloc::format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                crate::serial_debug_chirho!(
                    "[PROCESS] execve: loaded interpreter from VFS ({} bytes) header: {}",
                    data_chirho.len(), hdr_hex_chirho
                );
                data_chirho
            }
            None => {
                crate::serial_debug_chirho!(
                    "[PROCESS] execve: interpreter \"{}\" not found, falling back to static load",
                    interp_resolved_path_chirho
                );
                // Fall through to static loading below by using
                // load_elf_with_interp_chirho with no interpreter data.
                Vec::new()
            }
        };

        let has_interp_data_chirho = !interp_data_vec_chirho.is_empty();

        if has_interp_data_chirho {
            // Keep interpreter data alive (borrowed) during ELF loading.
            // It will be dropped when interp_data_vec_chirho goes out of scope.
            let interp_data_ref_chirho: &[u8] = &interp_data_vec_chirho;

            let dyn_result_chirho = match exec_chirho::load_elf_with_interp_chirho(
                elf_data_chirho,
                Some(interp_data_ref_chirho),
            ) {
                Ok(result_chirho) => result_chirho,
                Err(err_chirho) => {
                    crate::serial_println_chirho!(
                        "[PROCESS] execve: dynamic ELF load failed: {:?}",
                        err_chirho
                    );
                    return -ENOEXEC_CHIRHO;
                }
            };

            crate::serial_debug_chirho!(
                "[PROCESS] execve: dynamic ELF loaded — exe_entry={:#x}, start={:#x}, interp_base={:#x}, brk={:#x}",
                dyn_result_chirho.exe_chirho.entry_point_chirho,
                dyn_result_chirho.start_addr_chirho,
                dyn_result_chirho.interp_base_chirho,
                dyn_result_chirho.exe_chirho.brk_addr_chirho
            );

            // Boot PML4 switch already done in step 4b — ELF segments
            // are mapped there. Stack writes also go into boot PML4.

            // Set up the user stack with AT_BASE for the dynamic linker.
            let user_rsp_chirho = exec_chirho::setup_user_stack_dynlink_chirho(
                &dyn_result_chirho.exe_chirho,
                &effective_argv_chirho,
                &envp_vec_chirho,
                dyn_result_chirho.interp_base_chirho,
                dyn_result_chirho.exe_chirho.entry_point_chirho,
            );

            debug_verify_stack_chirho(user_rsp_chirho);

            crate::syscall_chirho::set_current_exe_path_chirho(filename_str_chirho.as_bytes());
            crate::serial_println_chirho!("[EXEC-TRACE] about to preserve_fd (procfd={})", is_procfd_exec_chirho);
            preserve_fd_table_across_exec_chirho_impl(is_procfd_exec_chirho);
            crate::serial_println_chirho!("[EXEC-TRACE] preserve_fd done");

            crate::serial_debug_chirho!(
                "[PROCESS] execve: ready to enter userspace (dynamic) — entry={:#x}, rsp={:#x}",
                dyn_result_chirho.start_addr_chirho,
                user_rsp_chirho
            );

            // Drop ELF data BEFORE jumping — frees heap memory that was
            // previously leaked via Vec::leak on every execve.
            drop(interp_data_vec_chirho);
            drop(elf_data_owned_chirho);

            // Activate per-process page table for address space isolation.
            // Without this, the dynamic binary's pages are in the shared
            // boot PML4 and get overwritten by other processes' exec.
            activate_per_process_pt_chirho();

            // Jump to the interpreter's entry point.
            exec_chirho::jump_to_userspace_chirho(
                dyn_result_chirho.start_addr_chirho,
                user_rsp_chirho,
            );
            // UNREACHABLE
        }
        // If interpreter data was empty, fall through to static loading.
    }

    // -----------------------------------------------------------------------
    // Step 5b: Static ELF — no interpreter needed
    // -----------------------------------------------------------------------
    let loaded_chirho: LoadedElfChirho =
        match exec_chirho::load_elf_into_memory_chirho(elf_data_chirho) {
            Ok(info_chirho) => info_chirho,
            Err(err_chirho) => {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: ELF load failed: {:?}",
                    err_chirho
                );
                return -ENOEXEC_CHIRHO;
            }
        };

    crate::serial_debug_chirho!(
        "[PROCESS] execve: ELF loaded (static) — entry={:#x}, phdr={:#x}, brk={:#x}",
        loaded_chirho.entry_point_chirho,
        loaded_chirho.phdr_addr_chirho,
        loaded_chirho.brk_addr_chirho
    );

    // -----------------------------------------------------------------------
    // Step 6: Set up the user stack with argv/envp
    // -----------------------------------------------------------------------
    let user_rsp_chirho = exec_chirho::setup_user_stack_with_args_chirho(
        &loaded_chirho,
        &effective_argv_chirho,
        &envp_vec_chirho,
    );

    // Debug: verify argv[0] on the user stack
    debug_verify_stack_chirho(user_rsp_chirho);

    // Update /proc/self/exe path for the new executable.
    crate::syscall_chirho::set_current_exe_path_chirho(filename_str_chirho.as_bytes());
    preserve_fd_table_across_exec_chirho_impl(is_procfd_exec_chirho);

    crate::serial_debug_chirho!(
        "[PROCESS] execve: ready to enter userspace (static) — entry={:#x}, rsp={:#x}",
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho
    );

    // Mirror user-space mappings into per-process page table and switch CR3.
    // Skip for embedded static BusyBox — ELF was loaded directly onto fork PT.
    if !is_embedded_static_exec_chirho {
        activate_per_process_pt_chirho();
    }

    // -----------------------------------------------------------------------
    // Step 7: Free ELF data and jump to userspace (never returns)
    // -----------------------------------------------------------------------
    // Drop the ELF binary data — it's already mapped into user pages.
    // Previously leaked via Vec::leak, wasting heap on every execve.
    drop(elf_data_owned_chirho);
    exec_chirho::jump_to_userspace_chirho(loaded_chirho.entry_point_chirho, user_rsp_chirho);

    // UNREACHABLE: jump_to_userspace_chirho is -> !
}

/// Mirror user-space mappings from the current (shared) page table into
/// the task's per-process page table, then switch CR3.
///
/// Called right before jumping to userspace in execve. This allows the
/// ELF loader to use the global mapper (which maps into the current CR3)
/// and then transfer all user mappings to an isolated address space.
fn activate_per_process_pt_chirho() {
    // Eagerly mirror ALL user mappings from the boot PML4 to the
    // per-process page table, then switch CR3. This gives the process
    // its own isolated address space — critical for preventing shared
    // .data segment corruption between processes.
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return,
    };
    let pt_root_chirho = task_arc_chirho.lock().page_table_root_chirho;

    if let Some(pml4_phys_chirho) = pt_root_chirho {
        // Ensure preemption trampoline is in boot PML4 before mirroring.
        crate::interrupts_chirho::init_user_preempt_trampoline_chirho();
        // Mirror ALL user-space pages from boot PML4 → per-process PT.
        let count_chirho = crate::pagetable_chirho::mirror_user_mappings_chirho(
            pml4_phys_chirho,
        );
        crate::serial_debug_chirho!(
            "[PROCESS] Mirrored {} user pages to per-process PT {:#x}",
            count_chirho,
            pml4_phys_chirho.as_u64(),
        );
        unsafe {
            crate::pagetable_chirho::switch_page_table_chirho(pml4_phys_chirho);
        }
    }
}

// ===========================================================================
// Shell re-launch (used by sys_exit and fault handlers)
// ===========================================================================

/// Kill the current task and respawn the shell.
///
/// Called from fault handlers (#UD, #GP, page fault) when a user-mode task
/// crashes unrecoverably. Removes the task from the scheduler, delivers
/// SIGCHLD to the parent, and respawns the shell.
///
/// This function never returns.
/// Re-exec the shell in the current task context.
///
/// Used by wait4 after reaping a child — avoids returning through the
/// corrupted parent stack frame by directly re-executing the shell binary.
pub fn relaunch_shell_chirho() -> ! {
    crate::serial_println_chirho!("[PROCESS] relaunch_shell: re-exec /bin/sh");
    let argv_chirho = [alloc::string::String::from("sh")];
    let envp_chirho = [alloc::string::String::from("HOME=/root")];
    exec_shell_with_args_chirho(&argv_chirho, &envp_chirho)
}

pub fn kill_and_respawn_shell_chirho(reason_chirho: &str) -> ! {
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let pid_chirho = task_arc_chirho.lock().pid_chirho;
        let ppid_chirho = task_arc_chirho.lock().ppid_chirho;
        crate::serial_println_chirho!(
            "[PROCESS] kill_and_respawn: PID {} killed ({}), respawning shell",
            pid_chirho, reason_chirho
        );
        crate::scheduler_chirho::remove_task_chirho(pid_chirho);
        crate::signal_chirho::deliver_sigchld_chirho(ppid_chirho, pid_chirho);

        // For daemon children (PID >= 3 — dropbear, SSH exec'd processes),
        // do NOT respawn a shell. Just mark as zombie and let the parent
        // reap via wait4. Respawning would overwrite argv (losing -c flag).
        if pid_chirho >= 3 {
            crate::serial_println_chirho!(
                "[PROCESS] PID {} is a daemon child — not respawning shell",
                pid_chirho
            );
            // Mark as zombie with exit code 128 + signal
            if let Some(task_arc_chirho) = crate::task_chirho::find_task_by_pid_chirho(pid_chirho) {
                task_arc_chirho.lock().state_chirho = crate::task_chirho::TaskStateChirho::ZombieChirho;
                task_arc_chirho.lock().exit_code_chirho = 139; // SIGSEGV
            }
            // Yield to let the parent run
            crate::scheduler_chirho::yield_current_chirho();
            // Halt this task — it should never run again
            loop { x86_64::instructions::hlt(); }
        }
    }
    let argv_chirho = [alloc::string::String::from("sh")];
    let envp_chirho = [alloc::string::String::from("HOME=/root")];
    exec_shell_with_args_chirho(&argv_chirho, &envp_chirho)
}

/// Re-launch the BusyBox shell after a process exits or crashes.
///
/// This function never returns — it loads BusyBox and jumps to userspace.
/// Called from sys_exit (vfork child exit) and from user-mode fault handlers
/// (GPF, illegal instruction, etc.) to recover gracefully.
pub fn exec_shell_with_args_chirho(
    _argv_chirho: &[alloc::string::String],
    _envp_chirho: &[alloc::string::String],
) -> ! {
    let shell_argv_chirho = [alloc::string::String::from("sh")];
    let shell_envp_chirho = [
        alloc::string::String::from("HOME=/root"),
        alloc::string::String::from("PATH=/bin:/sbin:/usr/bin:/usr/sbin"),
        alloc::string::String::from("TERM=linux"),
        alloc::string::String::from("PS1=lineluya# "),
        alloc::string::String::from("LD_LIBRARY_PATH=/lib:/usr/lib"),
        alloc::string::String::from("SHELL=/bin/sh"),
        alloc::string::String::from("PYTHONDONTWRITEBYTECODE=1"),
        alloc::string::String::from("PYTHONHOME=/usr"),
        alloc::string::String::from("PYTHONPATH=/usr/lib/python3.12"),
    ];

    let loaded_chirho = crate::exec_chirho::load_elf_into_memory_chirho(
        crate::exec_chirho::BUSYBOX_ELF_CHIRHO,
    )
    .expect("Failed to reload shell");

    crate::syscall_chirho::set_brk_chirho(loaded_chirho.brk_addr_chirho);

    let user_rsp_chirho = crate::exec_chirho::setup_user_stack_with_args_chirho(
        &loaded_chirho,
        &shell_argv_chirho,
        &shell_envp_chirho,
    );

    crate::exec_chirho::jump_to_userspace_chirho(
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho,
    );
}

// ===========================================================================
// Internal helpers
// ===========================================================================

fn preserve_fd_table_across_exec_chirho_impl(preserve_sockets_chirho: bool) {
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(task_arc_chirho) => task_arc_chirho,
        None => return,
    };

    let global_snapshot_chirho = {
        let global_guard_chirho = crate::fs_chirho::GLOBAL_FD_TABLE_CHIRHO.lock();
        global_guard_chirho.as_ref().map(|fd_table_chirho| fd_table_chirho.clone_table_chirho())
    };

    let global_mirror_chirho = {
        let mut task_guard_chirho = task_arc_chirho.lock();
        if task_guard_chirho.fd_table_chirho.is_none() {
            task_guard_chirho.fd_table_chirho = global_snapshot_chirho;
        }

        let ppid_chirho = task_guard_chirho.ppid_chirho;
        if let Some(ref mut fd_table_chirho) = task_guard_chirho.fd_table_chirho {
            let fd0_before_chirho = fd_table_chirho.fds_chirho.get(0).map(|s| s.is_some()).unwrap_or(false);
            let fd0_cloexec_chirho = fd_table_chirho.cloexec_chirho.get(0).copied().unwrap_or(false);
            // For procfd exec (dropbear fexecve): preserve ALL fds so the
            // connection socket on fd=8 (from `-2 8`) survives exec.
            // Without this, fd=8 is closed (O_CLOEXEC), dropbear falls
            // back to fd=0, computes nfds=1, and never reads pipe fds.
            if preserve_sockets_chirho {
                fd_table_chirho.clear_all_cloexec_flags_chirho();
            } else {
                fd_table_chirho.close_cloexec_fds_chirho();
            }
            let fd0_after_chirho = fd_table_chirho.fds_chirho.get(0).map(|s| s.is_some()).unwrap_or(false);
            if ppid_chirho != 0 {
                crate::serial_println_chirho!(
                    "[EXECVE-FD] fd0: before={} cloexec={} after={}",
                    fd0_before_chirho, fd0_cloexec_chirho, fd0_after_chirho,
                );
            }
            let next_fd_chirho = fd_table_chirho.next_free_fd_chirho();
            let mirror_table_chirho = fd_table_chirho.clone_table_chirho();
            task_guard_chirho.next_fd_chirho = next_fd_chirho;
            Some(mirror_table_chirho)
        } else {
            None
        }
    };

    // Only sync per-process → global for PID 0/1 (init shell).
    // For daemon children (PID >= 2), syncing overwrites the global table
    // with the child's fd layout, breaking other processes' fd lookups.
    // E.g., PID 4's exec copies its fd=0 (pipe) to global, overwriting
    // PID 3's fd=0 (socket), causing PID 3 to read from VFS instead of
    // recvfrom, preventing CloseWait EOF detection.
    // Only sync per-process → global for PID 0/1 (init shell).
    // For daemon PIDs (>= 2), syncing overwrites the global table with the
    // child's fd layout. Other processes that fall through to the global
    // table then see stale pipe fds, causing wait_event to spin (pipe scan
    // finds "ready" pipes that belong to a dead process).
    let current_pid_chirho = crate::task_chirho::current_task_chirho()
        .map(|t| t.lock().pid_chirho).unwrap_or(0);
    if current_pid_chirho <= 1 {
        if let Some(global_mirror_value_chirho) = global_mirror_chirho {
            let mut global_guard_chirho = crate::fs_chirho::GLOBAL_FD_TABLE_CHIRHO.lock();
            *global_guard_chirho = Some(global_mirror_value_chirho);
        }
    }
}

/// Resolve an interpreter path from a PT_INTERP segment.
///
/// The ELF binary specifies an interpreter like "/lib/ld-musl-x86_64.so.1",
/// Resolve the ELF interpreter path. With ext4 mounted at "/", the
/// interpreter path from PT_INTERP (e.g., "/lib/ld-musl-x86_64.so.1")
/// should resolve directly via the VFS.
fn resolve_interp_path_chirho(
    interp_path_chirho: &str,
    _exe_path_chirho: &str,
) -> String {
    // Try the raw path first — with ext4 at "/" this should work directly.
    if fs_chirho::resolve_path_chirho(interp_path_chirho).is_ok() {
        return String::from(interp_path_chirho);
    }

    // Fall back to /mnt-prefixed path for backward compatibility.
    let mut resolved_chirho = String::from("/mnt");
    resolved_chirho.push_str(interp_path_chirho);
    resolved_chirho
}

/// Debug helper: verify the first few argv entries on the user stack after
/// setup.
///
/// Reads argc and up to argv[0..3] from the freshly constructed stack layout
/// and prints them to serial for debugging.
fn debug_verify_stack_chirho(user_rsp_chirho: u64) {
    unsafe {
        let argc_val_chirho = core::ptr::read(user_rsp_chirho as *const u64);
        crate::serial_debug_chirho!("[PROCESS] execve: VERIFY stack argc={}", argc_val_chirho);

        let argv_count_to_log_chirho = core::cmp::min(argc_val_chirho as usize, 4);
        for arg_index_chirho in 0..argv_count_to_log_chirho {
            let argv_ptr_addr_chirho =
                user_rsp_chirho + 8 + (arg_index_chirho as u64 * 8);
            let argv_ptr_chirho = core::ptr::read(argv_ptr_addr_chirho as *const u64);
            let mut argv_buf_chirho = [0u8; 96];
            for byte_index_chirho in 0..95usize {
                let byte_chirho = core::ptr::read_volatile(
                    (argv_ptr_chirho + byte_index_chirho as u64) as *const u8,
                );
                if byte_chirho == 0 {
                    break;
                }
                argv_buf_chirho[byte_index_chirho] = byte_chirho;
            }
            let argv_str_chirho = core::str::from_utf8(&argv_buf_chirho).unwrap_or("???");
            crate::serial_debug_chirho!(
                "[PROCESS] execve: VERIFY argv[{}]@{:#x}=\"{}\"",
                arg_index_chirho,
                argv_ptr_chirho,
                argv_str_chirho.trim_end_matches('\0')
            );
        }
    }
}

/// Allocate a kernel stack on the heap and return the base (lowest) address.
///
/// The stack memory is leaked intentionally — kernel stacks live for the
/// lifetime of their task.
fn allocate_kernel_stack_chirho(size_chirho: usize) -> u64 {
    // Delegate to the task module's allocator which uses a bump allocator
    // with guard pages, preventing stack overlap. The previous heap-based
    // allocator (Vec + forget) placed adjacent stacks without gaps,
    // causing PID 3 and PID 4's stacks to overlap and corrupt each other.
    crate::task_chirho::allocate_kernel_stack_chirho(size_chirho)
}

/// Read a NULL-terminated array of string pointers from userspace.
///
/// Each entry in the array is a `u64` pointer to a NUL-terminated string.
/// The array itself is terminated by a NULL pointer (0).
///
/// Returns `Vec<String>` on success, or a negative errno on failure.
fn read_user_string_array_chirho(array_ptr_chirho: u64) -> Result<Vec<String>, i64> {
    // A NULL array pointer means an empty array (valid for envp).
    if array_ptr_chirho == 0 {
        return Ok(Vec::new());
    }

    let mut strings_chirho: Vec<String> = Vec::new();
    let mut offset_chirho: u64 = 0;

    loop {
        if strings_chirho.len() >= MAX_ARG_COUNT_CHIRHO {
            crate::serial_println_chirho!(
                "[PROCESS] execve: too many argv/envp entries (max={})",
                MAX_ARG_COUNT_CHIRHO
            );
            return Err(-E2BIG_CHIRHO);
        }

        // Read the pointer at array_ptr + offset
        let str_ptr_chirho = match read_user_u64_chirho(array_ptr_chirho + offset_chirho) {
            Ok(ptr_chirho) => ptr_chirho,
            Err(err_chirho) => {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: failed to read pointer at array offset {}: {:?}",
                    offset_chirho,
                    err_chirho
                );
                return Err(-EFAULT_CHIRHO);
            }
        };

        // NULL pointer terminates the array.
        if str_ptr_chirho == 0 {
            break;
        }

        // Read the string that the pointer points to.
        let string_chirho = match read_user_string_chirho(str_ptr_chirho, MAX_ARG_LEN_CHIRHO) {
            Ok(s_chirho) => s_chirho,
            Err(err_chirho) => {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: failed to read string at {:#x}: {:?}",
                    str_ptr_chirho,
                    err_chirho
                );
                return Err(-EFAULT_CHIRHO);
            }
        };

        strings_chirho.push(string_chirho);
        offset_chirho += 8; // sizeof(u64) — advance to next pointer
    }

    Ok(strings_chirho)
}

/// Try to read a file's contents from the VFS.
///
/// Returns `Some(Vec<u8>)` with the full file contents on success,
/// or `None` if the file cannot be found or read.
/// Public wrapper for reading a file from VFS (used by ko_loader modprobe).
pub fn try_read_file_pub_chirho(path_chirho: &str) -> Option<Vec<u8>> {
    try_read_file_chirho(path_chirho)
}

fn try_read_file_chirho(path_chirho: &str) -> Option<Vec<u8>> {
    use crate::vfs_chirho::FileChirho;

    // Resolve the path to an inode + file ops.
    let (inode_chirho, file_ops_chirho) = match fs_chirho::resolve_path_chirho(path_chirho) {
        Ok(result_chirho) => result_chirho,
        Err(_errno_chirho) => return None,
    };

    // Get the file size from the inode.
    let size_chirho = {
        let inode_guard_chirho = inode_chirho.lock();
        inode_guard_chirho.size_chirho as usize
    };

    if size_chirho == 0 {
        return None;
    }

    // Create a temporary File object to read through the file ops.
    let mut file_chirho = FileChirho {
        inode_chirho: inode_chirho.clone(),
        pos_chirho: 0,
        flags_chirho: 0, // O_RDONLY
        ops_chirho: file_ops_chirho,
    };

    // Read the entire file into a buffer.
    let mut buf_chirho = alloc::vec![0u8; size_chirho];
    match file_ops_chirho.read_chirho(&mut file_chirho, &mut buf_chirho) {
        Ok(bytes_read_chirho) => {
            buf_chirho.truncate(bytes_read_chirho);
            Some(buf_chirho)
        }
        Err(_errno_chirho) => None,
    }
}
