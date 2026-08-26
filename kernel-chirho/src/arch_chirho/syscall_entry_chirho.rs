// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! SYSCALL entry trampoline for the Lineluya kernel (x86_64).
//!
//! This module provides the low-level assembly entry point that the CPU jumps
//! to when userspace executes the `SYSCALL` instruction.  The trampoline:
//!
//! 1. Switches from the user GS base to the kernel GS base (`swapgs`).
//! 2. Saves the user RSP into a scratch variable and loads the kernel stack.
//! 3. Pushes all relevant registers onto the kernel stack to form a
//!    [`crate::syscall_chirho::SyscallFrameChirho`].
//! 4. Calls [`crate::syscall_chirho::syscall_dispatch_chirho`] with a pointer
//!    to the frame.
//! 5. Restores all registers from the frame, switches GS back, and executes
//!    `sysretq` to return to userspace.
//!
//! # Single-CPU assumption
//!
//! The current implementation uses a single static scratch variable for the
//! user RSP and a single kernel syscall stack.  This is safe on a single CPU
//! but must be replaced with per-CPU storage before SMP support is added.

use core::arch::global_asm;

// ============================================================================
// Segment selector constants
// ============================================================================

/// User-mode data segment selector (GDT index 4, RPL 3).
pub const USER_DS_CHIRHO: u64 = 0x23;
/// User-mode code segment selector (GDT index 5, RPL 3, 64-bit).
pub const USER_CS_CHIRHO: u64 = 0x2B;

// ============================================================================
// Per-CPU syscall state (single-CPU for now)
// ============================================================================

/// Per-CPU syscall entry state.
///
/// Groups all `static mut` globals that the assembly trampoline reads/writes.
/// Currently single-CPU; for SMP this becomes an array indexed by CPU id
/// (or stored in the GS-base per-CPU area).
///
/// The fields are `#[no_mangle]` because the assembly trampoline accesses
/// them by symbol name. All Rust access goes through typed helpers below.
pub struct PerCpuSyscallStateChirho {
    /// Kernel stack pointer loaded on SYSCALL entry.
    pub kernel_stack_top_chirho: u64,
    /// Scratch slot for saving user RSP during SYSCALL entry.
    pub user_rsp_scratch_chirho: u64,
}

/// The assembly trampoline accesses these by raw symbol name, so they
/// must remain as `#[no_mangle] static mut` globals.
#[no_mangle]
pub static mut KERNEL_STACK_TOP_CHIRHO: u64 = 0;
#[no_mangle]
pub static mut USER_RSP_SCRATCH_CHIRHO: u64 = 0;

/// Safe read of the current kernel stack top.
pub fn kernel_stack_top_chirho() -> u64 {
    unsafe { KERNEL_STACK_TOP_CHIRHO }
}

/// Safe write of the kernel stack top (called during context switch).
///
/// # Safety contract
/// The caller must ensure `addr_chirho` points to a valid, mapped
/// kernel stack region with enough space for the syscall frame.
pub fn set_kernel_stack_top_chirho(addr_chirho: u64) {
    unsafe { KERNEL_STACK_TOP_CHIRHO = addr_chirho; }
}

// ============================================================================
// Syscall stack size
// ============================================================================

/// Size of the dedicated kernel stack used during SYSCALL handling (512 KiB).
/// Python3 loads 6MB libpython via ext4 which has deep Rust call chains.
const SYSCALL_STACK_SIZE_CHIRHO: usize = 512 * 1024;

// ============================================================================
// Assembly trampoline
// ============================================================================
//
// Register state on SYSCALL entry (set by the CPU):
//   RCX = user RIP (return address)
//   R11 = user RFLAGS
//   RSP = unchanged (still user RSP!)
//   CS  = kernel CS (from STAR MSR)
//
// We build a SyscallFrameChirho on the kernel stack.  The struct layout (from
// low address to high address) is:
//
//   offset  0: rax_chirho
//   offset  8: rdi_chirho
//   offset 16: rsi_chirho
//   offset 24: rdx_chirho
//   offset 32: r10_chirho
//   offset 40: r8_chirho
//   offset 48: r9_chirho
//   offset 56: rcx_chirho  (user RIP)
//   offset 64: r11_chirho  (user RFLAGS)
//   offset 72: rsp_chirho  (user RSP)
//
// Since pushq grows the stack *downward*, we push in reverse order:
//   rsp, r11, rcx, r9, r8, r10, rdx, rsi, rdi, rax
//
// After all pushes, RSP points to rax_chirho, which is the start of the
// SyscallFrameChirho.  We pass this pointer in RDI to the Rust dispatcher.

global_asm!(r#"
.global syscall_entry_chirho
.type syscall_entry_chirho, @function
syscall_entry_chirho:
    // Step 1: Switch to kernel GS base (per-CPU data).
    swapgs

    // Step 2: Save user RSP into the scratch variable and load kernel stack.
    //         We cannot push anything yet because we are still on the user stack.
    movq    %rsp, USER_RSP_SCRATCH_CHIRHO(%rip)
    movq    KERNEL_STACK_TOP_CHIRHO(%rip), %rsp

    // Step 3: Push registers to build SyscallFrameChirho (reverse of struct order).
    //         Push from last field to first field.
    //         Callee-saved registers (rbx, rbp, r12-r15) are saved for fork.
    pushq   %r15                             // r15_chirho  (offset 120)
    pushq   %r14                             // r14_chirho  (offset 112)
    pushq   %r13                             // r13_chirho  (offset 104)
    pushq   %r12                             // r12_chirho  (offset  96)
    pushq   %rbp                             // rbp_chirho  (offset  88)
    pushq   %rbx                             // rbx_chirho  (offset  80)
    pushq   USER_RSP_SCRATCH_CHIRHO(%rip)   // rsp_chirho  (offset  72)
    pushq   %r11                             // r11_chirho  (offset  64) -- user RFLAGS
    pushq   %rcx                             // rcx_chirho  (offset  56) -- user RIP
    pushq   %r9                              // r9_chirho   (offset  48)
    pushq   %r8                              // r8_chirho   (offset  40)
    pushq   %r10                             // r10_chirho  (offset  32)
    pushq   %rdx                             // rdx_chirho  (offset  24)
    pushq   %rsi                             // rsi_chirho  (offset  16)
    pushq   %rdi                             // rdi_chirho  (offset   8)
    pushq   %rax                             // rax_chirho  (offset   0)

    // Step 4: First argument = pointer to SyscallFrameChirho (top of stack).
    movq    %rsp, %rdi

    // Step 5: Align stack to 16 bytes before the call (16 pushes = 128 bytes,
    //         which is 16-byte aligned if KERNEL_STACK_TOP_CHIRHO was aligned).
    andq    $-16, %rsp

    // Save the frame pointer in a callee-saved register so it survives
    // context switches inside syscall_dispatch (e.g. sched_yield from
    // the preemption trampoline). The old pushq/popq approach stored it
    // on the kernel stack, which can be corrupted when the scheduler
    // switches to another task and back. rbx is preserved by both the
    // SysV ABI and our context switch save/restore.
    pushq   %rbx
    movq    %rdi, %rbx

    // Call the Rust syscall dispatcher.
    call    syscall_dispatch_wrapper_chirho

    // Restore frame pointer from callee-saved register.
    movq    %rbx, %rdi
    popq    %rbx

    // Step 6: Move RSP back to the frame so we can pop registers.
    movq    %rdi, %rsp

    // Step 7: Write the return value (in RAX from the Rust call) into the
    //         frame's rax_chirho slot.  Actually, since we pop rax last and the
    //         Rust function returns the result in RAX, we overwrite the frame
    //         slot so the popq below picks up the return value.
    movq    %rax, (%rsp)

    // Step 8: Pop registers back (same order as pushes, reversed).
    popq    %rax                             // rax_chirho  (return value)
    popq    %rdi                             // rdi_chirho
    popq    %rsi                             // rsi_chirho
    popq    %rdx                             // rdx_chirho
    popq    %r10                             // r10_chirho
    popq    %r8                              // r8_chirho
    popq    %r9                              // r9_chirho
    popq    %rcx                             // rcx_chirho  (user RIP)
    popq    %r11                             // r11_chirho  (user RFLAGS)

    // Set IOPL=3 in user RFLAGS if KDENABIO was called.
    // This allows Xorg to use inb/outb for PCI config space.
    // Check the global flag (1 byte at USER_IOPL3_CHIRHO).
    cmpb    $0, USER_IOPL3_CHIRHO(%rip)
    je      3f
    orq     $0x3000, %r11                    // IOPL = 3
3:

    // Step 9: Build IRETQ frame for return to userspace.
    // IRETQ is used instead of SYSRET because QEMU TCG on ARM64 has
    // SYSRET emulation issues that cause #UD with CS=0x08 at user addresses.
    // IRETQ is slower but correct — atomically sets RIP, CS, RFLAGS, RSP, SS.
    //
    // Current register state:
    //   RCX = user RIP, R11 = user RFLAGS, RSP points to user_rsp slot
    //   RAX = syscall return value, RDI-R10 restored
    //   RBX/RBP/R12-R15 = user values (C ABI callee-saved)
    //
    // Read user RSP from the stack (next value to pop), save it to scratch.
    popq    %rsp                             // rsp_chirho = user RSP
    movq    %rsp, USER_RSP_SCRATCH_CHIRHO(%rip)  // save user RSP

    // Switch to kernel stack (well below the SyscallFrame to avoid overlap).
    movq    KERNEL_STACK_TOP_CHIRHO(%rip), %rsp
    subq    $256, %rsp

    // Switch GS base back to user before returning.
    swapgs

    // Build IRETQ frame on kernel stack (push in reverse order):
    pushq   $0x23                            // SS  = user data segment
    pushq   USER_RSP_SCRATCH_CHIRHO(%rip)    // RSP = user stack pointer
    pushq   %r11                             // RFLAGS = user flags
    pushq   $0x2B                            // CS  = user code segment (64-bit)
    pushq   %rcx                             // RIP = user return address

    // Return to userspace via IRETQ.
    iretq

.size syscall_entry_chirho, . - syscall_entry_chirho
"#,
    options(att_syntax)
);

// ============================================================================
// Rust-side wrapper callable from assembly
// ============================================================================

/// Wrapper function with C calling convention that the assembly trampoline
/// calls.  This bridges the gap between the assembly and the Rust dispatcher
/// which takes `&mut SyscallFrameChirho`.
///
/// # Safety
///
/// `frame_ptr_chirho` must point to a valid, properly aligned
/// [`crate::syscall_chirho::SyscallFrameChirho`] on the kernel stack.
#[no_mangle]
pub unsafe extern "C" fn syscall_dispatch_wrapper_chirho(
    frame_ptr_chirho: *mut crate::syscall_chirho::SyscallFrameChirho,
) -> i64 {
    let frame_chirho = &mut *frame_ptr_chirho;
    let saved_rcx_chirho = frame_chirho.rcx_chirho;

    // Save the syscall number before dispatch overwrites rax with the result.
    let syscall_nr_chirho = frame_chirho.rax_chirho;

    // Syscall counter — print every 1000th to show progress without flooding serial
    {
        use core::sync::atomic::{AtomicU64, Ordering};
        static SC_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
        let count_chirho = SC_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed);
        if count_chirho % 1000 == 0 {
            crate::serial_println_chirho!("[SC#{}] nr={}", count_chirho, syscall_nr_chirho);
        }
    }

    // GPT-directed debug: verify KERNEL_STACK_TOP matches current task
    {
        use core::sync::atomic::{AtomicBool, Ordering};
        static CHECKED_CHIRHO: AtomicBool = AtomicBool::new(false);
        let current_kstack_top_chirho = kernel_stack_top_chirho();
        if let Some(task_chirho) = crate::task_chirho::current_task_chirho() {
            let expected_chirho = task_chirho.lock().kernel_stack_chirho;
            if current_kstack_top_chirho != expected_chirho && !CHECKED_CHIRHO.swap(true, Ordering::Relaxed) {
                let pid_chirho = task_chirho.lock().pid_chirho;
                crate::serial_println_chirho!(
                    "[KSTACK-MISMATCH] pid={} KERNEL_STACK_TOP={:#x} task.kernel_stack={:#x} nr={}",
                    pid_chirho, current_kstack_top_chirho, expected_chirho, syscall_nr_chirho,
                );
            }
        }
    }

    let result_chirho = crate::syscall_chirho::syscall_dispatch_chirho(frame_chirho);

    // Validate that RCX (user return address) wasn't corrupted by the dispatch.
    // sched_yield (nr=24) INTENTIONALLY modifies RCX to restore the preempted
    // user RIP (saved by the timer trampoline). Do NOT revert that change —
    // reverting would cause SYSRET to return to the trampoline offset past
    // the SYSCALL instruction, executing garbage bytes and crashing.
    if frame_chirho.rcx_chirho != saved_rcx_chirho {
        if syscall_nr_chirho != 24 && syscall_nr_chirho != 15 { // allow sched_yield + rt_sigreturn
            crate::serial_println_chirho!(
                "[SYSRET-GUARD] RCX corrupted! was={:#x} now={:#x} sc={}",
                saved_rcx_chirho, frame_chirho.rcx_chirho,
                frame_chirho.rax_chirho,
            );
            // Restore the correct RCX to prevent SYSRET to garbage.
            frame_chirho.rcx_chirho = saved_rcx_chirho;
        }
    }

    // A2-PROC-005: Check for pending signals on syscall return.
    let is_exit_syscall_chirho = syscall_nr_chirho == 60 || syscall_nr_chirho == 231;
    let is_lifecycle_syscall_chirho = syscall_nr_chirho == 57   // fork
        || syscall_nr_chirho == 58   // vfork
        || syscall_nr_chirho == 56   // clone
        || syscall_nr_chirho == 59;  // execve
    // Skip signal delivery for sched_yield (nr=24) from the preemption
    // trampoline — the yield handler restores frame.rcx to the preempted
    // RIP, and signal delivery would clobber it with a handler address,
    // causing GPF at non-canonical addresses like 0x800000000000.
    let is_preempt_yield_chirho = syscall_nr_chirho == 24;
    if !is_exit_syscall_chirho && !is_lifecycle_syscall_chirho && !is_preempt_yield_chirho {
        // Deliver user signal handlers (enables dropbear SIGCHLD self-pipe).
        crate::signal_chirho::deliver_one_signal_on_return_chirho(frame_chirho);
        // Check for fatal pending signals.
        crate::signal_chirho::check_fatal_signals_on_return_chirho();
    }

    // Safe reschedule point: the syscall body has fully returned, the
    // continuation now lives in this compact syscall wrapper frame, and the
    // task's own SyscallFrameChirho is still on its kernel stack.
    // Direct schedule/yield from deep inside syscall handlers can lose the
    // caller continuation because those handlers suspend arbitrary kernel
    // frames mid-call. Do the actual context switch here instead.
    let allow_sysret_schedule_chirho = !is_exit_syscall_chirho && syscall_nr_chirho != 59;
    if allow_sysret_schedule_chirho && crate::scheduler_chirho::need_resched_chirho() {
        crate::scheduler_chirho::schedule_chirho();
    }

    // GPT-directed fix: restore user FS/GS base at the SYSCALL RETURN
    // boundary, immediately before the assembly IRETQ path. This is the
    // authoritative point for user TLS state. The scheduler's FS/GS write
    // happens too early — the resumed task runs more kernel code (wait_event,
    // select, signal checks) before IRETQ, and other tasks' FS bases can
    // overwrite the MSR during that kernel-mode execution.
    if let Some(task_chirho) = crate::task_chirho::current_task_chirho() {
        let tg_chirho = task_chirho.lock();
        let fs_chirho = tg_chirho.fs_base_chirho;
        let gs_chirho = tg_chirho.gs_base_chirho;
        drop(tg_chirho);
        unsafe {
            use x86_64::registers::model_specific::Msr;
            Msr::new(0xC000_0100).write(fs_chirho);
            Msr::new(0xC000_0102).write(gs_chirho);
        }
    }

    result_chirho
}

// ============================================================================
// Extern declaration for the assembly symbol
// ============================================================================

extern "C" {
    /// The assembly-defined SYSCALL entry point.
    ///
    /// This symbol is defined by the `global_asm!` block above.  We declare it
    /// here so that Rust code (e.g. [`init_syscall_entry_chirho`]) can obtain
    /// its address.
    pub fn syscall_entry_chirho();
}

// ============================================================================
// Initialisation
// ============================================================================

/// Initialise the SYSCALL entry trampoline.
///
/// This function:
/// 1. Allocates a dedicated kernel stack for syscall handling.
/// 2. Sets [`KERNEL_STACK_TOP_CHIRHO`] to the top of that stack.
/// 3. Updates the IA32_LSTAR MSR to point to [`syscall_entry_chirho`] so that
///    future SYSCALL instructions enter the assembly trampoline instead of the
///    placeholder stub.
///
/// # Safety
///
/// Must be called after the heap allocator is initialised and before userspace
/// code is allowed to execute.  Must be called exactly once.
pub unsafe fn init_syscall_entry_chirho() {
    use alloc::vec::Vec;
    use x86_64::registers::model_specific::Msr;

    // -- Allocate syscall kernel stack --
    // Allocate a zeroed buffer from the heap and leak it into a &'static [u8].
    // This is intentional — the syscall stack must live for the entire kernel
    // lifetime. Box::leak is the idiomatic Rust pattern for this.
    let stack_slice_chirho: &'static mut [u8] = alloc::boxed::Box::leak(
        alloc::vec![0u8; SYSCALL_STACK_SIZE_CHIRHO].into_boxed_slice()
    );
    let stack_base_chirho = stack_slice_chirho.as_ptr() as u64;

    // The stack grows downward, so "top" = base + size.
    // Align to 16 bytes for ABI compliance.
    let stack_top_chirho = (stack_base_chirho + SYSCALL_STACK_SIZE_CHIRHO as u64) & !0xF;
    KERNEL_STACK_TOP_CHIRHO = stack_top_chirho;

    crate::serial_debug_chirho!(
        "[SYSCALL-ENTRY] Kernel syscall stack allocated: base=0x{:x}, top=0x{:x}",
        stack_base_chirho,
        stack_top_chirho,
    );

    // Update TSS.RSP0 to a heap-allocated stack so ring 3→0 exception
    // transitions work. The BSS-based stack set during GDT init might
    // be at an address that's not accessible during page faults.
    {
        let mut rsp0_stack_chirho: Vec<u8> = Vec::with_capacity(32 * 1024);
        rsp0_stack_chirho.resize(32 * 1024, 0);
        let rsp0_top_chirho = (rsp0_stack_chirho.as_ptr() as u64 + 32 * 1024) & !0xF;
        core::mem::forget(rsp0_stack_chirho);

        // The GDT module owns the Lazy<TSS> interior-mutation boundary. Going
        // through its setters addresses the initialized TaskStateSegment, not
        // the Lazy wrapper header.
        unsafe {
            crate::gdt_chirho::set_tss_rsp0_chirho(rsp0_top_chirho);
            crate::gdt_chirho::set_page_fault_ist_chirho(rsp0_top_chirho - 16384);
        }

        crate::serial_debug_chirho!(
            "[SYSCALL-ENTRY] TSS.RSP0={:#x}, IST[1]={:#x} (heap)",
            rsp0_top_chirho,
            rsp0_top_chirho - 16384
        );
    }

    // -- Update IA32_LSTAR to point to the assembly trampoline --
    const IA32_LSTAR_CHIRHO: u32 = 0xC000_0082;
    let entry_addr_chirho = syscall_entry_chirho as *const () as u64;
    let mut lstar_msr_chirho = Msr::new(IA32_LSTAR_CHIRHO);
    lstar_msr_chirho.write(entry_addr_chirho);

    crate::serial_debug_chirho!(
        "[SYSCALL-ENTRY] LSTAR updated to assembly trampoline at 0x{:x}",
        entry_addr_chirho,
    );
}

extern crate alloc;
