// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Context switch assembly for the Lineluya kernel.
//!
//! This module provides the low-level [`switch_context_chirho`] routine written
//! in x86_64 assembly via [`core::arch::global_asm!`].  The function saves the
//! current task's callee-saved registers into the "old" [`CpuContextChirho`]
//! and restores them from the "new" [`CpuContextChirho`], then returns into the
//! new task's execution context.
//!
//! # CpuContextChirho field layout (`#[repr(C)]`)
//!
//! | Offset | Field          | Register |
//! |--------|----------------|----------|
//! |   0    | rsp_chirho     | RSP      |
//! |   8    | rbp_chirho     | RBP      |
//! |  16    | rbx_chirho     | RBX      |
//! |  24    | r12_chirho     | R12      |
//! |  32    | r13_chirho     | R13      |
//! |  40    | r14_chirho     | R14      |
//! |  48    | r15_chirho     | R15      |
//! |  56    | rip_chirho     | RIP      |
//! |  64    | rflags_chirho  | RFLAGS   |
//!
//! # Calling convention
//!
//! ```text
//! extern "C" fn switch_context_chirho(
//!     old_context_chirho: *mut CpuContextChirho,   // rdi
//!     new_context_chirho: *const CpuContextChirho, // rsi
//! )
//! ```
//!
//! # How first-time task dispatch works
//!
//! The stored `rsp_chirho` follows one rule:
//! it is the stack pointer value the resumed code should observe *after*
//! `switch_context_chirho` has returned.
//!
//! For a previously running task, that means the save path records `rsp + 8`
//! while the return address is still sitting at `[rsp]`.
//! For a brand-new task, `rsp_chirho` is initialized directly to the top of
//! its kernel stack and `rip_chirho` is the entry point. The restore path
//! then does `push saved_rip; ret`, which leaves `rsp` back at the stored
//! `rsp_chirho` in both cases.

use crate::task_chirho::CpuContextChirho;

// Force the type to be used so the import is not flagged as unused, while
// also providing a compile-time check that CpuContextChirho is available.
const _CONTEXT_SIZE_CHECK_CHIRHO: usize = core::mem::size_of::<CpuContextChirho>();

core::arch::global_asm!(
    r#"
// ---------------------------------------------------------------------------
// switch_context_chirho — x86_64 context switch (AT&T syntax)
//
// Arguments (System V AMD64 ABI):
//   %rdi = old_context_chirho: *mut CpuContextChirho
//   %rsi = new_context_chirho: *const CpuContextChirho
//
// Saves callee-saved registers into *old, restores from *new, then returns
// into the new task via `ret`.
// ---------------------------------------------------------------------------

.global switch_context_chirho
.type switch_context_chirho, @function

switch_context_chirho:
    // ---- Save old context (%rdi) ----

    // Save callee-saved registers into the old CpuContextChirho struct.
    //
    // CRITICAL: store the caller-visible post-return RSP, not the current RSP.
    // At function entry `%rsp` points at switch_context_chirho's own return
    // address. The restore path later does `push saved_rip; ret`, so to resume
    // the caller with the correct stack depth we must save `rsp + 8` here.
    leaq    8(%rsp), %rax
    movq    %rax,  0(%rdi)          // rsp_chirho  (offset  0)
    movq    %rbp,  8(%rdi)          // rbp_chirho  (offset  8)
    movq    %rbx, 16(%rdi)          // rbx_chirho  (offset 16)
    movq    %r12, 24(%rdi)          // r12_chirho  (offset 24)
    movq    %r13, 32(%rdi)          // r13_chirho  (offset 32)
    movq    %r14, 40(%rdi)          // r14_chirho  (offset 40)
    movq    %r15, 48(%rdi)          // r15_chirho  (offset 48)

    // Save the return address (RIP of the caller) so that when we switch back
    // to this task later, execution resumes right after the call site.
    // The return address is currently at the top of the stack (%rsp).
    movq    (%rsp), %rax
    movq    %rax, 56(%rdi)          // rip_chirho  (offset 56)

    // Save RFLAGS without touching the live task stack.
    //
    // `pushfq` always writes to [RSP-8].  If we do that on the task's real
    // stack, we risk corrupting data below the saved stack pointer.  Instead,
    // temporarily redirect RSP to scratch space in the context struct:
    //   scratch stack top = &old_context.rflags_chirho + 8
    // so the push lands exactly in old_context.rflags_chirho.
    movq    %rsp, %rax             // save live task RSP
    leaq    72(%rdi), %rsp         // temp stack top above rflags_chirho
    pushfq                         // writes FLAGS to 64(%rdi)
    popq    %rcx                   // read FLAGS back, RSP returns to 72(%rdi)
    movq    %rcx, 64(%rdi)         // rflags_chirho
    movq    %rax, %rsp             // restore live task RSP

    // ---- Restore new context (%rsi) ----

    // Restore callee-saved registers from the new CpuContextChirho struct.
    // CRITICAL: Restore RSP FIRST, then RFLAGS.  The old code did
    // pushq/popfq for RFLAGS BEFORE switching RSP, which wrote to the
    // OLD task's stack at [RSP-8] — corrupting the old task's saved
    // stack frame and causing GPF when it resumed.
    movq     0(%rsi), %rsp          // rsp_chirho  (offset  0) — NOW on new stack
    movq     8(%rsi), %rbp          // rbp_chirho  (offset  8)
    movq    16(%rsi), %rbx          // rbx_chirho  (offset 16)
    movq    24(%rsi), %r12          // r12_chirho  (offset 24)
    movq    32(%rsi), %r13          // r13_chirho  (offset 32)
    movq    40(%rsi), %r14          // r14_chirho  (offset 40)
    movq    48(%rsi), %r15          // r15_chirho  (offset 48)

    // Restore RFLAGS without touching the resumed task's live stack.
    // Use the new context struct as scratch space for push/popfq.
    movq    %rsp, %rax             // save resumed task RSP
    leaq    72(%rsi), %rsp         // temp stack top above rflags_chirho
    movq    64(%rsi), %rcx         // rflags_chirho
    pushq   %rcx
    popfq
    movq    %rax, %rsp             // restore resumed task RSP

    // Push the new task's saved RIP onto the (now-restored) stack so that
    // `ret` will jump to it.  For a brand-new task this is the entry point;
    // for a previously-running task it is the address right after the original
    // call to switch_context_chirho.
    movq    56(%rsi), %rax          // rip_chirho  (offset 56)
    pushq   %rax

    // Return into the new task.  `ret` pops the address we just pushed and
    // jumps there.
    ret

.size switch_context_chirho, . - switch_context_chirho
"#,
    options(att_syntax)
);
