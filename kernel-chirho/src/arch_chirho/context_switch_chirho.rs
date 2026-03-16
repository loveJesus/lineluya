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
//! When a new task is created, its `CpuContextChirho` is set up with:
//! - `rsp_chirho` pointing to the top of its kernel stack, where the entry
//!   point address has been pushed.
//! - `rip_chirho` set to the task's entry function address.
//!
//! On the first switch to this task, the assembly restores `rsp` from the
//! context, and the final `ret` instruction pops the entry point address off
//! the new stack, jumping directly into the task's entry function.

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
    movq    %rsp,  0(%rdi)          // rsp_chirho  (offset  0)
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

    // Save RFLAGS.
    pushfq
    popq    %rax
    movq    %rax, 64(%rdi)          // rflags_chirho (offset 64)

    // ---- Restore new context (%rsi) ----

    // Restore RFLAGS first (before touching the stack pointer).
    movq    64(%rsi), %rax          // rflags_chirho (offset 64)
    pushq   %rax
    popfq

    // Restore callee-saved registers from the new CpuContextChirho struct.
    movq     0(%rsi), %rsp          // rsp_chirho  (offset  0)
    movq     8(%rsi), %rbp          // rbp_chirho  (offset  8)
    movq    16(%rsi), %rbx          // rbx_chirho  (offset 16)
    movq    24(%rsi), %r12          // r12_chirho  (offset 24)
    movq    32(%rsi), %r13          // r13_chirho  (offset 32)
    movq    40(%rsi), %r14          // r14_chirho  (offset 40)
    movq    48(%rsi), %r15          // r15_chirho  (offset 48)

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
