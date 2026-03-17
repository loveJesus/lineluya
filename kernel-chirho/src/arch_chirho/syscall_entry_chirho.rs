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
// Static scratch storage (single-CPU only)
// ============================================================================

/// Kernel stack top for the SYSCALL trampoline.
///
/// Set by [`init_syscall_entry_chirho`] during boot.  The assembly stub loads
/// this into RSP on entry.
#[no_mangle]
pub static mut KERNEL_STACK_TOP_CHIRHO: u64 = 0;

/// Scratch space to temporarily save the user RSP during the SYSCALL entry
/// sequence, before we have switched to the kernel stack.
#[no_mangle]
pub static mut USER_RSP_SCRATCH_CHIRHO: u64 = 0;

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

    // Save the frame pointer so we can restore it after the call.
    pushq   %rdi

    // Call the Rust syscall dispatcher.
    call    syscall_dispatch_wrapper_chirho

    // Restore frame pointer from stack.
    popq    %rdi

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
    popq    %rcx                             // rcx_chirho  (user RIP for sysretq)
    popq    %r11                             // r11_chirho  (user RFLAGS for sysretq)

    // Step 9: Restore user RSP.
    popq    %rsp                             // rsp_chirho
    // Callee-saved registers (rbx-r15) remain on the kernel stack
    // but are preserved by the C ABI across the Rust call.

    // Step 10: Switch GS base back to user before returning.
    swapgs

    // Step 11: Return to userspace via SYSRET.
    sysretq

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
    let result_chirho = crate::syscall_chirho::syscall_dispatch_chirho(frame_chirho);

    // Validate that RCX (user return address) wasn't corrupted by the dispatch.
    if frame_chirho.rcx_chirho != saved_rcx_chirho {
        crate::serial_println_chirho!(
            "[SYSRET-GUARD] RCX corrupted! was={:#x} now={:#x} sc={}",
            saved_rcx_chirho, frame_chirho.rcx_chirho,
            frame_chirho.rax_chirho,
        );
        // Restore the correct RCX to prevent SYSRET to garbage.
        frame_chirho.rcx_chirho = saved_rcx_chirho;
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
    // We use a Vec<u8> and leak it so the memory lives forever.
    let mut stack_vec_chirho: Vec<u8> = Vec::with_capacity(SYSCALL_STACK_SIZE_CHIRHO);
    // Zero-fill for safety.
    stack_vec_chirho.resize(SYSCALL_STACK_SIZE_CHIRHO, 0);
    let stack_base_chirho = stack_vec_chirho.as_ptr() as u64;
    // Leak the vector so it is never freed.
    core::mem::forget(stack_vec_chirho);

    // The stack grows downward, so "top" = base + size.
    // Align to 16 bytes for ABI compliance.
    let stack_top_chirho = (stack_base_chirho + SYSCALL_STACK_SIZE_CHIRHO as u64) & !0xF;
    KERNEL_STACK_TOP_CHIRHO = stack_top_chirho;

    crate::serial_println_chirho!(
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

        // Write directly to the TSS memory. The TSS is a Lazy<TaskStateSegment>
        // which is already initialized. We write via raw pointer to update RSP0.
        unsafe {
            let tss_ptr_chirho = &crate::gdt_chirho::TSS_CHIRHO as *const _
                as *const x86_64::structures::tss::TaskStateSegment
                as *mut x86_64::structures::tss::TaskStateSegment;
            (*tss_ptr_chirho).privilege_stack_table[0] =
                x86_64::VirtAddr::new(rsp0_top_chirho);
            // Also update IST[1] (page fault stack) to heap memory
            (*tss_ptr_chirho).interrupt_stack_table[1] =
                x86_64::VirtAddr::new(rsp0_top_chirho - 16384);
        }

        crate::serial_println_chirho!(
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

    crate::serial_println_chirho!(
        "[SYSCALL-ENTRY] LSTAR updated to assembly trampoline at 0x{:x}",
        entry_addr_chirho,
    );
}

extern crate alloc;
