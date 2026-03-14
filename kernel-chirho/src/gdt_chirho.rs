// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Global Descriptor Table (GDT) module for the Lineluya kernel.
//!
//! The GDT defines memory segments and privilege levels for the CPU.
//! In 64-bit long mode, segmentation is largely vestigial, but the GDT is still
//! required for:
//!   - Setting the code segment (CS) to a valid 64-bit code descriptor
//!   - Loading a Task State Segment (TSS), which provides interrupt stack tables
//!     (IST) for handling double faults and other critical exceptions on a known-good
//!     stack, as well as privilege-level stack pointers for ring transitions.
//!   - Defining user-mode (ring 3) code and data segments for SYSCALL/SYSRET.
//!
//! ## GDT Layout (SYSRET-compatible, matches Linux convention)
//!
//! | Index | Selector | Description                | RPL |
//! |-------|----------|----------------------------|-----|
//! | 0     | 0x00     | Null descriptor            | -   |
//! | 1     | 0x08     | Kernel code segment (64b)  | 0   |
//! | 2     | 0x10     | Kernel data segment        | 0   |
//! | 3     | 0x18     | User code 32-bit (placeholder) | 3 |
//! | 4     | 0x20     | User data segment          | 3   |
//! | 5     | 0x28     | User code segment (64b)    | 3   |
//! | 6+7   | 0x30     | TSS (double-width)         | 0   |
//!
//! STAR MSR configuration for SYSCALL/SYSRET:
//! - STAR\[47:32\] = 0x08 (kernel CS). SYSCALL loads CS=0x08, SS=0x10.
//! - STAR\[63:48\] = 0x18 (user base). SYSRET64 loads CS=(0x18+16)|3=0x2B,
//!   SS=(0x18+8)|3=0x23.

use spin::Lazy;
use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, DescriptorFlags, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

// ============================================================================
// Public selector constants
// ============================================================================

/// Kernel code segment selector — GDT index 1, RPL 0.
/// Used by SYSCALL entry (loaded via STAR[47:32]).
pub const KERNEL_CS_CHIRHO: u16 = 0x08;

/// Kernel data segment selector — GDT index 2, RPL 0.
/// Loaded into SS on SYSCALL entry (STAR[47:32] + 8 = 0x10).
pub const KERNEL_DS_CHIRHO: u16 = 0x10;

/// User data segment selector — GDT index 4, RPL 3.
/// Loaded into SS on SYSRET (STAR[63:48] + 8 = 0x20, with RPL 3 = 0x23).
pub const USER_DS_CHIRHO: u16 = 0x20 | 3; // 0x23

/// User 64-bit code segment selector — GDT index 5, RPL 3.
/// Loaded into CS on SYSRET64 (STAR[63:48] + 16 = 0x28, with RPL 3 = 0x2B).
pub const USER_CS_CHIRHO: u16 = 0x28 | 3; // 0x2B

/// TSS selector — GDT index 6 (occupies slots 6+7 as a system descriptor).
pub const TSS_SEL_CHIRHO: u16 = 0x30;

/// Pre-computed STAR MSR value for SYSCALL/SYSRET.
/// - Bits 47:32 = kernel CS (0x08): SYSCALL loads CS=0x08, SS=0x10.
/// - Bits 63:48 = user base (0x18): SYSRET64 loads CS=0x28|3, SS=0x20|3.
pub const STAR_MSR_VALUE_CHIRHO: u64 = (0x18u64 << 48) | (0x08u64 << 32);

// ============================================================================
// IST / stack configuration
// ============================================================================

/// IST index used for the double fault handler stack.
/// When a double fault occurs, the CPU switches to the stack at IST entry 0,
/// preventing a triple fault caused by a corrupted kernel stack.
pub const DOUBLE_FAULT_IST_INDEX_CHIRHO: u16 = 0;

/// Size in bytes of each IST / privilege-level stack.
/// 5 pages (20 KiB) is sufficient for interrupt and exception handling.
const STACK_SIZE_CHIRHO: usize = 4096 * 5;

// ============================================================================
// GDT structure
// ============================================================================

/// Holds the GDT together with the segment selectors produced when adding
/// entries, so they can be loaded into the CPU registers after GDT installation.
struct GdtChirho {
    gdt_chirho: GlobalDescriptorTable,
    code_selector_chirho: SegmentSelector,
    data_selector_chirho: SegmentSelector,
    user_code32_selector_chirho: SegmentSelector,
    user_data_selector_chirho: SegmentSelector,
    user_code_selector_chirho: SegmentSelector,
    tss_selector_chirho: SegmentSelector,
}

// ============================================================================
// TSS
// ============================================================================

/// Static Task State Segment, lazily initialised.
///
/// The TSS is configured with:
/// - `interrupt_stack_table[0]`: a dedicated stack for the double-fault handler
///   (IST entry 0), so the CPU can switch to a known-good stack even when the
///   original kernel stack is corrupted or overflowed.
/// - `privilege_stack_table[0]`: the ring-0 stack pointer used on privilege
///   transitions (e.g., when a user-mode interrupt enters ring 0).
static TSS_CHIRHO: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss_chirho = TaskStateSegment::new();

    // -- Double-fault IST stack (IST index 0) --
    // SAFETY: This is a module-private static only accessed during TSS
    // initialisation, which happens exactly once via `Lazy`. No data races.
    let double_fault_stack_end_chirho = {
        static mut DOUBLE_FAULT_STACK_CHIRHO: [u8; STACK_SIZE_CHIRHO] = [0; STACK_SIZE_CHIRHO];
        #[allow(static_mut_refs)]
        let stack_start_chirho =
            VirtAddr::from_ptr(unsafe { &DOUBLE_FAULT_STACK_CHIRHO });
        // Stacks grow downward on x86_64, so the "end" (highest address) is the
        // initial stack pointer.
        stack_start_chirho + STACK_SIZE_CHIRHO as u64
    };
    tss_chirho.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX_CHIRHO as usize] =
        double_fault_stack_end_chirho;

    // -- Privilege stack for ring 0 --
    // SAFETY: Same reasoning as above — module-private, single init via `Lazy`.
    let privilege_stack_end_chirho = {
        static mut PRIVILEGE_STACK_CHIRHO: [u8; STACK_SIZE_CHIRHO] = [0; STACK_SIZE_CHIRHO];
        #[allow(static_mut_refs)]
        let stack_start_chirho =
            VirtAddr::from_ptr(unsafe { &PRIVILEGE_STACK_CHIRHO });
        stack_start_chirho + STACK_SIZE_CHIRHO as u64
    };
    tss_chirho.privilege_stack_table[0] = privilege_stack_end_chirho;

    tss_chirho
});

// ============================================================================
// GDT construction
// ============================================================================

/// Static GDT with all segment selectors, lazily initialised.
///
/// The GDT is laid out for SYSCALL/SYSRET compatibility (see module docs):
///   [0] null, [1] kernel_code, [2] kernel_data, [3] user_code_32 (placeholder),
///   [4] user_data, [5] user_code_64, [6+7] TSS.
///
/// After loading the GDT with `lgdt`, the corresponding selectors must be
/// written into the CS register (`set_cs`) and the task register (`ltr`).
static GDT_CHIRHO: Lazy<GdtChirho> = Lazy::new(|| {
    let mut gdt_chirho = GlobalDescriptorTable::new();

    // [1] Kernel code segment — 64-bit, DPL 0
    let code_selector_chirho = gdt_chirho.append(Descriptor::kernel_code_segment());

    // [2] Kernel data segment — DPL 0
    let data_selector_chirho = gdt_chirho.append(Descriptor::kernel_data_segment());

    // [3] User code 32-bit placeholder — DPL 3
    // Required by the SYSRET convention: STAR[63:48] = 0x18 (this slot).
    // We only support 64-bit user mode, but the slot must exist so that
    // user_data lands at 0x20 and user_code_64 lands at 0x28.
    let user_code32_selector_chirho =
        gdt_chirho.append(Descriptor::UserSegment(DescriptorFlags::USER_CODE32.bits()));

    // [4] User data segment — DPL 3
    // SYSRET loads SS = (STAR[63:48] + 8) | 3 = 0x20 | 3 = 0x23
    let user_data_selector_chirho = gdt_chirho.append(Descriptor::user_data_segment());

    // [5] User code segment — 64-bit, DPL 3
    // SYSRET64 loads CS = (STAR[63:48] + 16) | 3 = 0x28 | 3 = 0x2B
    let user_code_selector_chirho = gdt_chirho.append(Descriptor::user_code_segment());

    // [6+7] TSS descriptor (occupies two GDT slots as a system segment)
    let tss_selector_chirho = gdt_chirho.append(Descriptor::tss_segment(&TSS_CHIRHO));

    GdtChirho {
        gdt_chirho,
        code_selector_chirho,
        data_selector_chirho,
        user_code32_selector_chirho,
        user_data_selector_chirho,
        user_code_selector_chirho,
        tss_selector_chirho,
    }
});

// ============================================================================
// Initialisation
// ============================================================================

/// Initialise the Global Descriptor Table and load it into the CPU.
///
/// This function:
/// 1. Installs the GDT via the `lgdt` instruction.
/// 2. Reloads the code-segment register (CS) so the CPU uses the new GDT entry
///    for code fetches.
/// 3. Loads the TSS into the task register so the CPU knows where to find
///    interrupt stacks and privilege-level stacks.
///
/// Must be called exactly once during early kernel initialisation, before
/// interrupts are enabled.
pub fn init_chirho() {
    GDT_CHIRHO.gdt_chirho.load();

    // SAFETY: `code_selector_chirho` was obtained from the GDT that was just
    // loaded, so it refers to a valid 64-bit code segment descriptor.
    unsafe {
        CS::set_reg(GDT_CHIRHO.code_selector_chirho);
    }

    // SAFETY: Set SS, DS, ES to the kernel data segment so interrupt frames
    // have a valid stack segment.  In 64-bit long mode these are largely
    // ignored, but the CPU still checks the selector on interrupt entry.
    unsafe {
        SS::set_reg(GDT_CHIRHO.data_selector_chirho);
        DS::set_reg(GDT_CHIRHO.data_selector_chirho);
        ES::set_reg(GDT_CHIRHO.data_selector_chirho);
    }

    // SAFETY: `tss_selector_chirho` was obtained from the GDT that was just
    // loaded, so it refers to a valid TSS descriptor pointing to `TSS_CHIRHO`.
    unsafe {
        load_tss(GDT_CHIRHO.tss_selector_chirho);
    }

    // Verify selector layout matches our public constants (debug-mode only).
    debug_assert_eq!(
        GDT_CHIRHO.code_selector_chirho.0, KERNEL_CS_CHIRHO,
        "Kernel CS selector mismatch"
    );
    debug_assert_eq!(
        GDT_CHIRHO.data_selector_chirho.0, KERNEL_DS_CHIRHO,
        "Kernel DS selector mismatch"
    );
    debug_assert_eq!(
        GDT_CHIRHO.user_data_selector_chirho.0, USER_DS_CHIRHO,
        "User DS selector mismatch"
    );
    debug_assert_eq!(
        GDT_CHIRHO.user_code_selector_chirho.0, USER_CS_CHIRHO,
        "User CS selector mismatch"
    );
    debug_assert_eq!(
        GDT_CHIRHO.tss_selector_chirho.0, TSS_SEL_CHIRHO,
        "TSS selector mismatch"
    );
}
