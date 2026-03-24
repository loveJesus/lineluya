// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16
//
// ============================================================================
// RBP-chain Frame Pointer Walker for Lineluya Page Fault Handler
// ============================================================================
//
// This is a PATCH to add to the NULL-deref guard in page_fault_handler_chirho
// (interrupts_chirho.rs, around line 625, inside the `if page_vaddr_chirho < 0x100000` block).
//
// IMPORTANT CAVEAT:
//   Both Xorg and musl are compiled WITHOUT -fno-omit-frame-pointer.
//   Only ~4 functions in musl and very few in Xorg use the classic
//   push %rbp / mov %rsp,%rbp prologue. This means RBP-chain walking
//   will produce PARTIAL results — it will follow whatever chain exists
//   but may miss frames or stop early.
//
//   The GDB approach (gdb-xorg-crash-chirho.gdb) is superior because GDB
//   uses .eh_frame DWARF CFI data for unwinding, which works even without
//   frame pointers.
//
//   However, this kernel-side walker is useful as a FALLBACK when:
//   - GDB is not available or cannot connect
//   - You need the backtrace in the serial log without external tools
//   - The crash happens during early boot before GDB can attach
//
// HOW TO USE:
//   Replace the existing stack dump block in the NULL-deref guard
//   (the `for frame_idx_chirho in 0..8u64` loop at ~line 635) with
//   the function call below, or integrate the function into the file.
//
// ============================================================================

/// Walk the user-mode RBP chain to produce a backtrace.
///
/// # Arguments
/// * `rbp_chirho`   — Initial RBP value from the faulting context
/// * `rsp_chirho`   — RSP at the time of the fault
/// * `rip_chirho`   — RIP at the time of the fault (crash instruction)
/// * `pid_chirho`   — Process ID for log tagging
///
/// # Safety
/// Reads user-space memory via raw pointer dereference. The caller must ensure
/// the pages containing the stack are mapped (they should be, since we're in
/// the page fault handler after the fault but the stack pages themselves are
/// valid — it's a data access that faulted, not a stack access).
///
/// # Address Ranges (Lineluya conventions)
/// * Xorg code:  0x555555550000 .. 0x555555800000  (PIE base + ~2.5MB text)
/// * musl code:  0x7F0000100000 .. 0x7F0000200000  (interp base + ~1MB)
/// * User stack: 0x7FFFFF000000 .. 0x800000000000  (grows down from top)
///
/// # Frame Layout (when frame pointers are used)
/// ```text
///   [rbp + 8] = return address
///   [rbp + 0] = saved previous rbp (caller's frame pointer)
/// ```
pub unsafe fn walk_user_backtrace_chirho(
    rbp_chirho: u64,
    rsp_chirho: u64,
    rip_chirho: u64,
    pid_chirho: u64,
) {
    // Address range checks
    const XORG_LO_CHIRHO: u64 = 0x555555550000;
    const XORG_HI_CHIRHO: u64 = 0x555555800000;
    const MUSL_LO_CHIRHO: u64 = 0x7F0000100000;
    const MUSL_HI_CHIRHO: u64 = 0x7F0000200000;
    const STACK_LO_CHIRHO: u64 = 0x7FFF00000000;
    const STACK_HI_CHIRHO: u64 = 0x800000000000;
    const MAX_FRAMES_CHIRHO: usize = 32;

    crate::serial_println_chirho!(
        "[BT-PID{}] === User Backtrace (RBP chain + stack scan) ===",
        pid_chirho,
    );
    crate::serial_println_chirho!(
        "[BT-PID{}] #0  RIP={:#018x} (crash point)",
        pid_chirho, rip_chirho,
    );

    // Classify an address as Xorg, musl, or unknown
    let classify_chirho = |addr_chirho: u64| -> &'static str {
        if addr_chirho >= XORG_LO_CHIRHO && addr_chirho < XORG_HI_CHIRHO {
            "Xorg"
        } else if addr_chirho >= MUSL_LO_CHIRHO && addr_chirho < MUSL_HI_CHIRHO {
            "musl"
        } else {
            "????"
        }
    };

    let is_code_addr_chirho = |addr_chirho: u64| -> bool {
        (addr_chirho >= XORG_LO_CHIRHO && addr_chirho < XORG_HI_CHIRHO)
            || (addr_chirho >= MUSL_LO_CHIRHO && addr_chirho < MUSL_HI_CHIRHO)
    };

    let is_stack_addr_chirho = |addr_chirho: u64| -> bool {
        addr_chirho >= STACK_LO_CHIRHO && addr_chirho < STACK_HI_CHIRHO
    };

    // -----------------------------------------------------------------------
    // Phase 1: RBP chain walk (best-effort, may stop early)
    // -----------------------------------------------------------------------
    let mut frame_count_chirho: usize = 1;
    let mut current_rbp_chirho = rbp_chirho;
    let mut rbp_walk_succeeded_chirho = false;

    if is_stack_addr_chirho(current_rbp_chirho) && (current_rbp_chirho & 0x7) == 0 {
        crate::serial_println_chirho!(
            "[BT-PID{}] --- RBP chain walk (RBP={:#018x}) ---",
            pid_chirho, current_rbp_chirho,
        );

        for _ in 0..MAX_FRAMES_CHIRHO {
            // Read saved RBP (previous frame pointer)
            let saved_rbp_chirho = core::ptr::read_volatile(current_rbp_chirho as *const u64);
            // Read return address (RBP + 8)
            let ret_addr_chirho = core::ptr::read_volatile(
                (current_rbp_chirho + 8) as *const u64,
            );

            if is_code_addr_chirho(ret_addr_chirho) {
                let module_chirho = classify_chirho(ret_addr_chirho);
                let offset_chirho = if ret_addr_chirho >= XORG_LO_CHIRHO
                    && ret_addr_chirho < XORG_HI_CHIRHO
                {
                    ret_addr_chirho - XORG_LO_CHIRHO
                } else {
                    ret_addr_chirho - MUSL_LO_CHIRHO
                };
                crate::serial_println_chirho!(
                    "[BT-PID{}] #{:<2} {:#018x}  [{}+{:#x}]  rbp={:#x}",
                    pid_chirho, frame_count_chirho, ret_addr_chirho,
                    module_chirho, offset_chirho, current_rbp_chirho,
                );
                frame_count_chirho += 1;
                rbp_walk_succeeded_chirho = true;
            }

            // Validate next RBP: must be on the stack, aligned, and > current
            if !is_stack_addr_chirho(saved_rbp_chirho)
                || (saved_rbp_chirho & 0x7) != 0
                || saved_rbp_chirho <= current_rbp_chirho
            {
                break;
            }
            current_rbp_chirho = saved_rbp_chirho;
        }
    }

    if !rbp_walk_succeeded_chirho {
        crate::serial_println_chirho!(
            "[BT-PID{}] RBP chain walk failed (RBP={:#x} not a valid frame pointer)",
            pid_chirho, rbp_chirho,
        );
    }

    // -----------------------------------------------------------------------
    // Phase 2: Stack scan (heuristic — scan for return addresses)
    // -----------------------------------------------------------------------
    // When frame pointers are omitted, the stack still contains return
    // addresses from CALL instructions. We scan the stack for values that
    // look like code addresses. This produces false positives (.rela.plt
    // data, saved registers, etc.) but is better than nothing.
    //
    // To filter noise: a return address should be preceded by a CALL
    // instruction (5 bytes for near call: E8 xx xx xx xx, or 2+ bytes for
    // indirect call: FF 1x/FF 2x). We check the byte before the return
    // address in the code segment.
    crate::serial_println_chirho!(
        "[BT-PID{}] --- Stack scan (RSP={:#018x}, 256 bytes) ---",
        pid_chirho, rsp_chirho,
    );

    if is_stack_addr_chirho(rsp_chirho) {
        let scan_count_chirho = 256 / 8; // 32 qwords
        let mut found_chirho = 0usize;

        for idx_chirho in 0..scan_count_chirho {
            let stack_addr_chirho = rsp_chirho + (idx_chirho as u64) * 8;
            if !is_stack_addr_chirho(stack_addr_chirho) {
                break;
            }

            let val_chirho = core::ptr::read_volatile(stack_addr_chirho as *const u64);

            if is_code_addr_chirho(val_chirho) {
                // Heuristic: check if this looks like a return address.
                // A CALL instruction (E8 xx xx xx xx) is 5 bytes, so the
                // return address is 5 bytes past the call. Check if the
                // byte at (val - 5) is 0xE8 (near relative call).
                // Also accept FF /2 (call [reg]) at (val - 2).
                let mut is_ret_addr_chirho = false;

                // Check for E8 (near call, 5 bytes)
                let call_site_chirho = val_chirho.wrapping_sub(5);
                if is_code_addr_chirho(call_site_chirho) {
                    let opcode_chirho = core::ptr::read_volatile(
                        call_site_chirho as *const u8,
                    );
                    if opcode_chirho == 0xE8 {
                        is_ret_addr_chirho = true;
                    }
                }
                // Check for FF /2 (indirect call, 2+ bytes)
                if !is_ret_addr_chirho {
                    let call_site2_chirho = val_chirho.wrapping_sub(2);
                    if is_code_addr_chirho(call_site2_chirho) {
                        let opcode2_chirho = core::ptr::read_volatile(
                            call_site2_chirho as *const u8,
                        );
                        let modrm_chirho = core::ptr::read_volatile(
                            (call_site2_chirho + 1) as *const u8,
                        );
                        // FF /2 = FF with modrm reg field = 2 (010)
                        if opcode2_chirho == 0xFF && (modrm_chirho & 0x38) == 0x10 {
                            is_ret_addr_chirho = true;
                        }
                    }
                }

                let tag_chirho = if is_ret_addr_chirho { "CALL" } else { "data" };
                let module_chirho = classify_chirho(val_chirho);
                let offset_chirho = if val_chirho >= XORG_LO_CHIRHO
                    && val_chirho < XORG_HI_CHIRHO
                {
                    val_chirho - XORG_LO_CHIRHO
                } else {
                    val_chirho - MUSL_LO_CHIRHO
                };

                crate::serial_println_chirho!(
                    "[BT-PID{}]   [rsp+{:#04x}] = {:#018x}  [{}+{:#x}]  <{}>",
                    pid_chirho, idx_chirho * 8, val_chirho,
                    module_chirho, offset_chirho, tag_chirho,
                );
                found_chirho += 1;
            }
        }

        if found_chirho == 0 {
            crate::serial_println_chirho!(
                "[BT-PID{}] No code addresses found in stack scan.",
                pid_chirho,
            );
        }
    }

    crate::serial_println_chirho!(
        "[BT-PID{}] === End Backtrace ({} frames from RBP, stack scan above) ===",
        pid_chirho, frame_count_chirho - 1,
    );

    // -----------------------------------------------------------------------
    // Phase 3: Dump key registers via stack frame (IST context)
    // -----------------------------------------------------------------------
    // In an x86-interrupt handler, the InterruptStackFrame gives us
    // RIP, CS, RFLAGS, RSP, SS. For GPRs we need to read from the
    // IST stack where the CPU pushed them. This is best-effort.
    crate::serial_println_chirho!(
        "[BT-PID{}] === Address Legend ===",
        pid_chirho,
    );
    crate::serial_println_chirho!(
        "[BT-PID{}]   Xorg .text:  0x555555571{:>4}  (file offset 0x218c0, subtract 0x555555550000 for file addr)",
        pid_chirho, "8c0+",
    );
    crate::serial_println_chirho!(
        "[BT-PID{}]   musl .text:  0x7f00001140{:>2}  (file offset 0x14000, subtract 0x7F0000100000 for file addr)",
        pid_chirho, "00+",
    );
    crate::serial_println_chirho!(
        "[BT-PID{}] To resolve: objdump -d --start-address=0x<file_offset> Xorg | head -20",
        pid_chirho,
    );
}


// ============================================================================
// Integration Point
// ============================================================================
//
// In page_fault_handler_chirho (interrupts_chirho.rs), inside the
//   `if page_vaddr_chirho < 0x100000 { ... }` block (~line 625),
// REPLACE the existing stack dump loop:
//
//   ```rust
//   // Dump user stack to find return addresses (backtrace)
//   if rsp_chirho > 0x7f0000000000 && rsp_chirho < 0x800000000000 {
//       for frame_idx_chirho in 0..8u64 { ... }
//   }
//   ```
//
// WITH:
//
//   ```rust
//   // Walk user backtrace via RBP chain + stack scan
//   {
//       let rbp_val_chirho: u64;
//       unsafe { core::arch::asm!("mov {}, rbp", out(reg) rbp_val_chirho); }
//       // NOTE: This rbp_val_chirho is the KERNEL's RBP, not the user's.
//       // For the user's RBP, we need to read it from the IST exception
//       // frame. In x86-interrupt ABI, the CPU saves RSP/RIP/CS/RFLAGS/SS
//       // but NOT general-purpose registers. The compiler saves GPRs on the
//       // IST stack, so we can't easily get user RBP from here.
//       //
//       // WORKAROUND: If you add a naked wrapper that saves all GPRs before
//       // calling the Rust handler, you can pass user RBP through.
//       // For now, we use the stack scan (Phase 2) which doesn't need RBP.
//       //
//       // Pass 0 for RBP to skip Phase 1 and go straight to stack scanning:
//       unsafe {
//           walk_user_backtrace_chirho(0, rsp_chirho, rip_chirho, user_fault_pid_chirho as u64);
//       }
//   }
//   ```
//
// To get the REAL user RBP, add save/restore GPR wrappers around the
// page fault handler. This requires a naked function entry point.
// See the GDB approach (gdb-xorg-crash-chirho.gdb) for the reliable method.
