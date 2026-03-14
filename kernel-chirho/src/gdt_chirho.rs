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

use spin::Lazy;
use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// IST index used for the double fault handler stack.
/// When a double fault occurs, the CPU switches to the stack at IST entry 0,
/// preventing a triple fault caused by a corrupted kernel stack.
pub const DOUBLE_FAULT_IST_INDEX_CHIRHO: u16 = 0;

/// Size in bytes of each IST / privilege-level stack.
/// 5 pages (20 KiB) is sufficient for interrupt and exception handling.
const STACK_SIZE_CHIRHO: usize = 4096 * 5;

/// Holds the GDT together with the segment selectors produced when adding
/// entries, so they can be loaded into the CPU registers after GDT installation.
struct GdtChirho {
    gdt_chirho: GlobalDescriptorTable,
    code_selector_chirho: SegmentSelector,
    data_selector_chirho: SegmentSelector,
    tss_selector_chirho: SegmentSelector,
}

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

/// Static GDT with code and TSS segment selectors, lazily initialised.
///
/// The GDT contains:
/// 1. A 64-bit kernel code segment descriptor.
/// 2. A TSS descriptor that points to [`TSS_CHIRHO`].
///
/// After loading the GDT with `lgdt`, the corresponding selectors must be
/// written into the CS register (`set_cs`) and the task register (`ltr`).
static GDT_CHIRHO: Lazy<GdtChirho> = Lazy::new(|| {
    let mut gdt_chirho = GlobalDescriptorTable::new();

    let code_selector_chirho = gdt_chirho.append(Descriptor::kernel_code_segment());
    let data_selector_chirho = gdt_chirho.append(Descriptor::kernel_data_segment());
    let tss_selector_chirho = gdt_chirho.append(Descriptor::tss_segment(&TSS_CHIRHO));

    GdtChirho {
        gdt_chirho,
        code_selector_chirho,
        data_selector_chirho,
        tss_selector_chirho,
    }
});

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
}
