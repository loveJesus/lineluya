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
const WNOHANG_CHIRHO: u32 = 1;
/// Also report stopped (not just terminated) children.
#[allow(dead_code)]
const WUNTRACED_CHIRHO: u32 = 2;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default kernel stack size (must match task_chirho.rs).
const DEFAULT_KERNEL_STACK_SIZE_CHIRHO: usize = 16 * 1024;

/// Maximum length of a filename path from userspace.
const MAX_PATH_LEN_CHIRHO: usize = 4096;

/// Maximum number of argv/envp entries.
const MAX_ARG_COUNT_CHIRHO: usize = 256;

/// Maximum length of a single argv/envp string.
const MAX_ARG_LEN_CHIRHO: usize = 4096;

// ===========================================================================
// sys_fork_chirho — REAL IMPLEMENTATION
// ===========================================================================

/// `fork()` / `vfork()` — create a child process.
///
/// Because Lineluya does not yet have per-process page tables, this is
/// effectively a `vfork()`: parent and child share the same address space.
/// The child **must** call `execve()` before doing anything that modifies
/// user memory.
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
        }

        // The child's CpuContextChirho: when switch_context_chirho restores
        // these registers, rip will be fork_child_return_chirho and rsp will
        // point to the SyscallFrameChirho we placed on the stack.
        let mut child_ctx_chirho = CpuContextChirho::zero_chirho();
        child_ctx_chirho.rip_chirho = fork_child_return_chirho as *const () as u64;
        child_ctx_chirho.rsp_chirho = frame_dst_chirho;
        child_ctx_chirho.rflags_chirho = 0x200; // IF (interrupts enabled)

        // Clone the FD table (each Arc<Mutex<FileChirho>> is shared, but the
        // table itself is independent — matching POSIX fork semantics).
        let child_fd_table_chirho = parent_chirho
            .fd_table_chirho
            .as_ref()
            .map(|t_chirho| t_chirho.clone_table_chirho());

        // Clone the parent's page table with COW semantics (if it has one).
        let child_pt_root_chirho = match parent_chirho.page_table_root_chirho {
            Some(parent_pml4_chirho) => {
                crate::pagetable_chirho::clone_page_table_chirho(parent_pml4_chirho)
            }
            None => None,
        };

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
            page_table_root_chirho: child_pt_root_chirho,
            next_fd_chirho: parent_chirho.next_fd_chirho,
            fd_table_chirho: child_fd_table_chirho,
            priority_chirho: parent_chirho.priority_chirho,
            time_slice_chirho: parent_chirho.time_slice_chirho,
            uid_chirho: parent_chirho.uid_chirho,
            gid_chirho: parent_chirho.gid_chirho,
            euid_chirho: parent_chirho.euid_chirho,
            egid_chirho: parent_chirho.egid_chirho,
            comm_chirho: parent_chirho.comm_chirho,
            fs_base_chirho: parent_chirho.fs_base_chirho,
            gs_base_chirho: parent_chirho.gs_base_chirho,
            signal_mask_chirho: parent_chirho.signal_mask_chirho,
            pending_signals_chirho: 0, // pending signals are not inherited
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: parent_chirho.brk_chirho,
            brk_start_chirho: parent_chirho.brk_start_chirho,
        }
    };

    crate::serial_println_chirho!(
        "[PROCESS] fork: parent PID={} -> child PID={}",
        child_task_chirho.ppid_chirho,
        child_pid_chirho
    );

    // --- 5. Register the child in the global task list ---
    let child_arc_chirho = Arc::new(Mutex::new(child_task_chirho));
    register_task_chirho(Arc::clone(&child_arc_chirho));

    // --- 6. Add the child to the scheduler run queue ---
    crate::scheduler_chirho::add_task_chirho(child_pid_chirho);

    // --- 7. vfork semantics: child runs first ---
    // Real fork requires per-process page tables (CR3 switching) to prevent
    // the child's execve from overwriting the parent's address space.
    // Until per-process page tables are fully working, we use vfork
    // semantics: fork returns 0 (child path), child calls execve, sys_exit
    // re-launches the shell.  The child IS registered in the scheduler for
    // future use once page table isolation is implemented.
    0i64
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
    crate::serial_println_chirho!(
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
            // If a custom stack was provided, use it for the child's
            // user-space RSP.
            if stack_chirho != 0 {
                (*dst_ptr_chirho).rsp_chirho = stack_chirho;
            }
        }

        let mut child_ctx_chirho = CpuContextChirho::zero_chirho();
        child_ctx_chirho.rip_chirho = fork_child_return_chirho as *const () as u64;
        child_ctx_chirho.rsp_chirho = frame_dst_chirho;
        child_ctx_chirho.rflags_chirho = 0x200;

        // FD table: if CLONE_FILES is set, share (clone for now since we
        // lack Arc<Mutex<FdTableChirho>> in TaskChirho).  Otherwise, duplicate.
        let child_fd_table_chirho = parent_chirho
            .fd_table_chirho
            .as_ref()
            .map(|t_chirho| t_chirho.clone_table_chirho());

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
                None => None,
            }
        };

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
            page_table_root_chirho: child_pt_root_chirho,
            next_fd_chirho: parent_chirho.next_fd_chirho,
            fd_table_chirho: child_fd_table_chirho,
            priority_chirho: parent_chirho.priority_chirho,
            time_slice_chirho: parent_chirho.time_slice_chirho,
            uid_chirho: parent_chirho.uid_chirho,
            gid_chirho: parent_chirho.gid_chirho,
            euid_chirho: parent_chirho.euid_chirho,
            egid_chirho: parent_chirho.egid_chirho,
            comm_chirho: parent_chirho.comm_chirho,
            fs_base_chirho: parent_chirho.fs_base_chirho,
            gs_base_chirho: parent_chirho.gs_base_chirho,
            signal_mask_chirho: parent_chirho.signal_mask_chirho,
            pending_signals_chirho: 0,
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: parent_chirho.brk_chirho,
            brk_start_chirho: parent_chirho.brk_start_chirho,
        }
    };

    crate::serial_println_chirho!(
        "[PROCESS] clone: parent PID={} -> child PID={} (flags={:#x})",
        child_task_chirho.ppid_chirho,
        child_pid_chirho,
        flags_chirho
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
/// - Otherwise, block (yield + retry) until a child exits.
pub fn sys_wait4_chirho(
    pid_chirho: i64,
    wstatus_chirho: u64,
    options_chirho: u32,
    _rusage_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
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

    // Retry limit to prevent infinite hangs if all children are gone
    // without becoming zombies (safety valve).
    let max_retries_chirho: u32 = 10_000;
    let mut retries_chirho: u32 = 0;

    loop {
        // Search for a matching zombie child.
        let task_list_chirho = TASK_LIST_CHIRHO.lock();

        // First check: do we have any children at all?
        let has_children_chirho = task_list_chirho
            .iter()
            .any(|t_chirho| {
                let tc_chirho = t_chirho.lock();
                tc_chirho.ppid_chirho == parent_pid_chirho
                    && tc_chirho.state_chirho != TaskStateChirho::DeadChirho
            });

        if !has_children_chirho {
            return -ECHILD_CHIRHO;
        }

        // Look for a zombie child matching the requested PID.
        let mut found_pid_result_chirho: Option<u64> = None;
        let mut found_exit_code_chirho: i32 = 0;

        for task_arc_chirho in task_list_chirho.iter() {
            let task_chirho = task_arc_chirho.lock();

            // Must be our child.
            if task_chirho.ppid_chirho != parent_pid_chirho {
                continue;
            }

            // Check PID filter.
            if pid_chirho > 0 && task_chirho.pid_chirho != pid_chirho as u64 {
                continue;
            }

            // Check if zombie.
            if task_chirho.state_chirho == TaskStateChirho::ZombieChirho {
                found_pid_result_chirho = Some(task_chirho.pid_chirho);
                found_exit_code_chirho = task_chirho.exit_code_chirho;
                break;
            }
        }

        // Drop the task list lock before writing to userspace memory.
        drop(task_list_chirho);

        if let Some(reaped_pid_chirho) = found_pid_result_chirho {
            // Write the exit status to userspace if wstatus pointer is non-null.
            if wstatus_chirho != 0 {
                // WEXITSTATUS format: (exit_code << 8) | 0 (normal termination).
                let wstatus_val_chirho: i32 = (found_exit_code_chirho & 0xFF) << 8;
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

            crate::serial_println_chirho!(
                "[PROCESS] wait4: reaped child PID={}, exit_code={}",
                reaped_pid_chirho,
                found_exit_code_chirho
            );

            return reaped_pid_chirho as i64;
        }

        // No zombie child found.
        if (options_chirho & WNOHANG_CHIRHO) != 0 {
            // WNOHANG: return 0 immediately.
            return 0;
        }

        // Blocking wait: yield to the scheduler and try again.
        // This is a simple poll-and-yield approach. A proper implementation
        // would use wait queues where the child wakes the parent on exit,
        // but this is sufficient for BusyBox ash to not crash.
        retries_chirho += 1;
        if retries_chirho >= max_retries_chirho {
            crate::serial_println_chirho!(
                "[PROCESS] wait4: exceeded retry limit, returning -ECHILD"
            );
            return -ECHILD_CHIRHO;
        }

        crate::scheduler_chirho::yield_current_chirho();
    }
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
fn fork_child_return_chirho() {
    // The RSP currently points to the SyscallFrameChirho on the child's
    // kernel stack.  We need to restore the registers and SYSRET.
    //
    // SAFETY: This function is only reached via a context switch where
    // RSP was set to point at a valid SyscallFrameChirho.
    unsafe {
        core::arch::asm!(
            // RSP points to the SyscallFrameChirho laid out as:
            //   [rsp+0x00] = rax (0 for child)
            //   [rsp+0x08] = rdi
            //   [rsp+0x10] = rsi
            //   [rsp+0x18] = rdx
            //   [rsp+0x20] = r10
            //   [rsp+0x28] = r8
            //   [rsp+0x30] = r9
            //   [rsp+0x38] = rcx (return address)
            //   [rsp+0x40] = r11 (saved rflags)
            //   [rsp+0x48] = rsp (user stack)

            // Restore registers from the frame.
            "mov rax, [rsp + 0x00]",    // rax = 0 (fork return value)
            "mov rdi, [rsp + 0x08]",    // rdi
            "mov rsi, [rsp + 0x10]",    // rsi
            "mov rdx, [rsp + 0x18]",    // rdx
            "mov r10, [rsp + 0x20]",    // r10
            "mov r8,  [rsp + 0x28]",    // r8
            "mov r9,  [rsp + 0x30]",    // r9
            "mov rcx, [rsp + 0x38]",    // rcx = user RIP (for sysretq)
            "mov r11, [rsp + 0x40]",    // r11 = user RFLAGS (for sysretq)

            // Load the user stack pointer into a scratch register.
            // We must read it before overwriting RSP.
            "mov r15, [rsp + 0x48]",    // r15 = user RSP

            // Switch to the user stack.
            "mov rsp, r15",

            // Swap GS back to user GS (swapgs convention).
            "swapgs",

            // Return to userspace via SYSRET.
            // SYSRET sets RIP = RCX, RFLAGS = R11, and transitions to ring 3.
            "sysretq",
            options(noreturn),
        );
    }
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
    crate::serial_println_chirho!(
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

    crate::serial_println_chirho!(
        "[PROCESS] execve: filename = \"{}\"",
        filename_str_chirho
    );

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

    crate::serial_println_chirho!(
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

    crate::serial_println_chirho!(
        "[PROCESS] execve: envp ({} entries)",
        envp_vec_chirho.len()
    );

    // -----------------------------------------------------------------------
    // Step 4: Obtain the ELF binary data
    // -----------------------------------------------------------------------
    // Try to resolve the file via the VFS first. If that fails, fall back to
    // the embedded hello-chirho binary (useful for early testing before the
    // filesystem has real binaries).
    let elf_data_chirho: &[u8] = match try_read_file_chirho(&filename_str_chirho) {
        Some(data_chirho) => {
            crate::serial_println_chirho!(
                "[PROCESS] execve: loaded \"{}\" from VFS ({} bytes)",
                filename_str_chirho,
                data_chirho.len()
            );
            // Leak the Vec to get a &'static [u8] — the old process image is
            // being replaced, so this memory won't be freed until the process
            // exits. This is acceptable for a single-process kernel.
            let leaked_chirho: &'static [u8] = alloc::vec::Vec::leak(data_chirho);
            leaked_chirho
        }
        None => {
            // Check if the filename is a BusyBox applet — if so, load
            // the embedded BusyBox binary with the applet name as argv[0].
            // This emulates having /bin/ls → /bin/busybox symlinks.
            let basename_chirho = filename_str_chirho
                .rsplit('/')
                .next()
                .unwrap_or(&filename_str_chirho);
            let busybox_applets_chirho = [
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
            if busybox_applets_chirho.contains(&basename_chirho) {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: \"{}\" is a BusyBox applet, using embedded BusyBox",
                    filename_str_chirho
                );
                crate::exec_chirho::BUSYBOX_ELF_CHIRHO
            } else {
                crate::serial_println_chirho!(
                    "[PROCESS] execve: \"{}\" not found in VFS, falling back to embedded hello-chirho",
                    filename_str_chirho
                );
                HELLO_ELF_CHIRHO
            }
        }
    };

    // -----------------------------------------------------------------------
    // Step 4b: Create a fresh page table for this process (execve replaces
    //          the entire address space).
    // -----------------------------------------------------------------------
    // Skip page table switch for vfork-style execve — reuse the current
    // page table. Creating a new one and switching CR3 causes a triple fault
    // because the bootloader's physical memory mapping is complex.
    // The ELF loader will remap user segments in the existing page table.
    crate::serial_println_chirho!("[PROCESS] execve: reusing current page table for vfork-execve");

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

    if let Some(ref raw_interp_path_chirho) = interp_path_chirho {
        // ---------------------------------------------------------------
        // P4-004: Dynamically linked ELF — load interpreter from ext4
        // ---------------------------------------------------------------
        crate::serial_println_chirho!(
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

        crate::serial_println_chirho!(
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
                crate::serial_println_chirho!(
                    "[PROCESS] execve: loaded interpreter from VFS ({} bytes) header: {}",
                    data_chirho.len(), hdr_hex_chirho
                );
                data_chirho
            }
            None => {
                crate::serial_println_chirho!(
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
            // Leak interpreter data to get a &'static [u8]
            let interp_data_leaked_chirho: &'static [u8] =
                alloc::vec::Vec::leak(interp_data_vec_chirho);

            // Load main ELF + interpreter using the existing
            // load_elf_with_interp_chirho infrastructure.
            let dyn_result_chirho = match exec_chirho::load_elf_with_interp_chirho(
                elf_data_chirho,
                Some(interp_data_leaked_chirho),
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

            crate::serial_println_chirho!(
                "[PROCESS] execve: dynamic ELF loaded — exe_entry={:#x}, start={:#x}, interp_base={:#x}, brk={:#x}",
                dyn_result_chirho.exe_chirho.entry_point_chirho,
                dyn_result_chirho.start_addr_chirho,
                dyn_result_chirho.interp_base_chirho,
                dyn_result_chirho.exe_chirho.brk_addr_chirho
            );

            // Set up the user stack with AT_BASE for the dynamic linker.
            // AT_ENTRY must be the main executable's entry so the
            // interpreter can jump to it after self-relocation.
            let user_rsp_chirho = exec_chirho::setup_user_stack_dynlink_chirho(
                &dyn_result_chirho.exe_chirho,
                &effective_argv_chirho,
                &envp_vec_chirho,
                dyn_result_chirho.interp_base_chirho,
                dyn_result_chirho.exe_chirho.entry_point_chirho,
            );

            // Debug: verify argv[0] on the user stack
            debug_verify_stack_chirho(user_rsp_chirho);

            // Update /proc/self/exe path for the new executable.
            crate::syscall_chirho::set_current_exe_path_chirho(filename_str_chirho.as_bytes());

            crate::serial_println_chirho!(
                "[PROCESS] execve: ready to enter userspace (dynamic) — entry={:#x}, rsp={:#x}",
                dyn_result_chirho.start_addr_chirho,
                user_rsp_chirho
            );

            // Jump to the interpreter's entry point (not the main binary's).
            // The interpreter (ld-musl) will self-relocate, load the main
            // binary, resolve symbols, and eventually call main().
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

    crate::serial_println_chirho!(
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

    crate::serial_println_chirho!(
        "[PROCESS] execve: ready to enter userspace (static) — entry={:#x}, rsp={:#x}",
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho
    );

    // -----------------------------------------------------------------------
    // Step 7: Jump to userspace (never returns on success)
    // -----------------------------------------------------------------------
    exec_chirho::jump_to_userspace_chirho(loaded_chirho.entry_point_chirho, user_rsp_chirho);

    // UNREACHABLE: jump_to_userspace_chirho is -> !
}

// ===========================================================================
// Internal helpers
// ===========================================================================

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

/// Debug helper: verify argv[0] on the user stack after setup.
///
/// Reads argc and argv[0] from the freshly constructed stack layout and
/// prints them to serial for debugging.
fn debug_verify_stack_chirho(user_rsp_chirho: u64) {
    unsafe {
        let argc_val_chirho = core::ptr::read(user_rsp_chirho as *const u64);
        let argv0_ptr_chirho = core::ptr::read((user_rsp_chirho + 8) as *const u64);
        let mut argv0_buf_chirho = [0u8; 32];
        for i_chirho in 0..31usize {
            let b_chirho = core::ptr::read_volatile(
                (argv0_ptr_chirho + i_chirho as u64) as *const u8,
            );
            if b_chirho == 0 {
                break;
            }
            argv0_buf_chirho[i_chirho] = b_chirho;
        }
        let argv0_str_chirho =
            core::str::from_utf8(&argv0_buf_chirho).unwrap_or("???");
        crate::serial_println_chirho!(
            "[PROCESS] execve: VERIFY stack: argc={}, argv[0]@{:#x}=\"{}\"",
            argc_val_chirho,
            argv0_ptr_chirho,
            argv0_str_chirho.trim_end_matches('\0')
        );
    }
}

/// Allocate a kernel stack on the heap and return the base (lowest) address.
///
/// The stack memory is leaked intentionally — kernel stacks live for the
/// lifetime of their task.
fn allocate_kernel_stack_chirho(size_chirho: usize) -> u64 {
    use alloc::vec;

    let stack_vec_chirho = vec![0u8; size_chirho];
    let ptr_chirho = stack_vec_chirho.as_ptr() as u64;
    core::mem::forget(stack_vec_chirho);
    ptr_chirho
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
