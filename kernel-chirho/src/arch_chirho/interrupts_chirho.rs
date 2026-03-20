// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Interrupt Descriptor Table (IDT) and interrupt handler module for the Lineluya kernel.
//!
//! This module sets up the IDT with handlers for CPU exceptions (breakpoint, double fault,
//! page fault, general protection fault) and hardware interrupts (timer, keyboard) via the
//! 8259 PIC. Keyboard scancodes are decoded using the `pc_keyboard` crate.

extern crate alloc;

use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::registers::rflags::RFlags;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;
use spin::Mutex;

// ---------------------------------------------------------------------------
// Inline PIC 8259 implementation (replaces pic8259 crate to avoid x86_64 version conflicts)
// ---------------------------------------------------------------------------

/// A pair of chained 8259 PICs (master + slave).
pub struct ChainedPicsChirho {
    master_cmd_chirho: u16,
    master_data_chirho: u16,
    slave_cmd_chirho: u16,
    slave_data_chirho: u16,
    offset1_chirho: u8,
    offset2_chirho: u8,
}

impl ChainedPicsChirho {
    pub const unsafe fn new(offset1_chirho: u8, offset2_chirho: u8) -> Self {
        Self {
            master_cmd_chirho: 0x20,
            master_data_chirho: 0x21,
            slave_cmd_chirho: 0xA0,
            slave_data_chirho: 0xA1,
            offset1_chirho,
            offset2_chirho,
        }
    }

    pub unsafe fn initialize(&mut self) {
        let wait_chirho = || { core::arch::asm!("nop"); };

        // Save masks
        let mask1_chirho: u8;
        let mask2_chirho: u8;
        core::arch::asm!("in al, dx", out("al") mask1_chirho, in("dx") self.master_data_chirho);
        core::arch::asm!("in al, dx", out("al") mask2_chirho, in("dx") self.slave_data_chirho);

        // ICW1: start init sequence, cascade mode
        Self::outb_chirho(self.master_cmd_chirho, 0x11); wait_chirho();
        Self::outb_chirho(self.slave_cmd_chirho, 0x11); wait_chirho();

        // ICW2: set vector offsets
        Self::outb_chirho(self.master_data_chirho, self.offset1_chirho); wait_chirho();
        Self::outb_chirho(self.slave_data_chirho, self.offset2_chirho); wait_chirho();

        // ICW3: master has slave on IRQ2, slave has cascade identity 2
        Self::outb_chirho(self.master_data_chirho, 4); wait_chirho();
        Self::outb_chirho(self.slave_data_chirho, 2); wait_chirho();

        // ICW4: 8086/88 mode
        Self::outb_chirho(self.master_data_chirho, 0x01); wait_chirho();
        Self::outb_chirho(self.slave_data_chirho, 0x01); wait_chirho();

        // Restore masks
        Self::outb_chirho(self.master_data_chirho, mask1_chirho);
        Self::outb_chirho(self.slave_data_chirho, mask2_chirho);
    }

    pub unsafe fn notify_end_of_interrupt(&mut self, irq_chirho: u8) {
        if irq_chirho >= self.offset2_chirho {
            Self::outb_chirho(self.slave_cmd_chirho, 0x20);
        }
        Self::outb_chirho(self.master_cmd_chirho, 0x20);
    }

    unsafe fn outb_chirho(port_chirho: u16, val_chirho: u8) {
        core::arch::asm!("out dx, al", in("dx") port_chirho, in("al") val_chirho);
    }
}

// ---------------------------------------------------------------------------
// LAPIC register typed access (A2-AUDIT-010)
// ---------------------------------------------------------------------------

/// LAPIC register offsets from base address 0xFEE0_0000.
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum LapicRegisterChirho {
    IdChirho = 0x020,
    VersionChirho = 0x030,
    TprChirho = 0x080,
    EoiChirho = 0x0B0,
    SpuriousChirho = 0x0F0,
    IcrLowChirho = 0x300,
    IcrHighChirho = 0x310,
    TimerLvtChirho = 0x320,
    TimerInitCountChirho = 0x380,
    TimerCurrentCountChirho = 0x390,
    TimerDivideChirho = 0x3E0,
}

const LAPIC_BASE_CHIRHO: u64 = 0xFEE0_0000;

/// Write a value to a LAPIC register, accounting for the physical memory
/// offset used by the bootloader's identity-mapped higher-half mapping.
#[inline(always)]
fn write_lapic_reg_chirho(phys_offset_chirho: u64, reg_chirho: LapicRegisterChirho, val_chirho: u32) {
    unsafe {
        core::ptr::write_volatile(
            (phys_offset_chirho + LAPIC_BASE_CHIRHO + reg_chirho as u64) as *mut u32,
            val_chirho,
        );
    }
}

/// Read a value from a LAPIC register, accounting for the physical memory
/// offset used by the bootloader's identity-mapped higher-half mapping.
#[inline(always)]
fn read_lapic_reg_chirho(phys_offset_chirho: u64, reg_chirho: LapicRegisterChirho) -> u32 {
    unsafe {
        core::ptr::read_volatile(
            (phys_offset_chirho + LAPIC_BASE_CHIRHO + reg_chirho as u64) as *const u32,
        )
    }
}

// ---------------------------------------------------------------------------
// PIC configuration constants
// ---------------------------------------------------------------------------

/// Offset for the primary PIC (PIC1). Hardware IRQs 0..7 are remapped to
/// interrupt vectors 32..39 so they do not collide with CPU exceptions 0..31.
pub const PIC_1_OFFSET_CHIRHO: u8 = 32;

/// Offset for the secondary PIC (PIC2). Hardware IRQs 8..15 are remapped to
/// interrupt vectors 40..47.
pub const PIC_2_OFFSET_CHIRHO: u8 = PIC_1_OFFSET_CHIRHO + 8;

// ---------------------------------------------------------------------------
// Hardware interrupt index enum
// ---------------------------------------------------------------------------

/// Maps hardware interrupt lines to their IDT vector numbers.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndexChirho {
    /// PIC timer interrupt (IRQ 0 -> vector 32).
    TimerChirho = PIC_1_OFFSET_CHIRHO,
    /// PS/2 keyboard interrupt (IRQ 1 -> vector 33).
    KeyboardChirho = PIC_1_OFFSET_CHIRHO + 1,
}

impl InterruptIndexChirho {
    /// Return the interrupt vector number as a `u8`.
    fn as_u8_chirho(self) -> u8 {
        self as u8
    }

    /// Return the interrupt vector number as a `usize` (for IDT indexing).
    fn as_usize_chirho(self) -> usize {
        usize::from(self.as_u8_chirho())
    }
}

// ---------------------------------------------------------------------------
// Static PIC instance
// ---------------------------------------------------------------------------

/// Global chained PIC instance protected by a spinlock.
///
/// # Safety
/// The PIC offsets must not overlap with CPU exception vectors (0..31).
/// We use offsets 32 and 40 which are safe.
pub static PICS_CHIRHO: Mutex<ChainedPicsChirho> =
    Mutex::new(unsafe { ChainedPicsChirho::new(PIC_1_OFFSET_CHIRHO, PIC_2_OFFSET_CHIRHO) });

// ---------------------------------------------------------------------------
// Interrupt Descriptor Table (lazily initialized)
// ---------------------------------------------------------------------------

/// The global IDT, initialized once via `spin::Lazy`.
static IDT_CHIRHO: spin::Lazy<InterruptDescriptorTable> = spin::Lazy::new(|| {
    let mut idt_chirho = InterruptDescriptorTable::new();

    // --- CPU exception handlers ---

    idt_chirho.breakpoint.set_handler_fn(breakpoint_handler_chirho);

    // The double-fault handler runs on a separate stack (IST entry) to handle
    // kernel stack overflow scenarios where pushing the exception frame onto
    // the current (overflowed) stack would triple-fault.
    //
    // SAFETY: `DOUBLE_FAULT_IST_INDEX_CHIRHO` refers to a valid IST entry that
    // is configured in the TSS by the `gdt_chirho` module.
    unsafe {
        idt_chirho
            .double_fault
            .set_handler_fn(double_fault_handler_chirho)
            .set_stack_index(crate::gdt_chirho::DOUBLE_FAULT_IST_INDEX_CHIRHO);
    }

    unsafe {
        idt_chirho.page_fault
            .set_handler_fn(page_fault_handler_chirho)
            .set_stack_index(crate::gdt_chirho::PAGE_FAULT_IST_INDEX_CHIRHO);
    }
    idt_chirho.general_protection_fault.set_handler_fn(general_protection_fault_handler_chirho);
    idt_chirho.segment_not_present.set_handler_fn(segment_not_present_handler_chirho);
    idt_chirho.invalid_opcode.set_handler_fn(invalid_opcode_handler_chirho);
    idt_chirho.overflow.set_handler_fn(overflow_handler_chirho);
    idt_chirho.bound_range_exceeded.set_handler_fn(bound_range_handler_chirho);
    idt_chirho.device_not_available.set_handler_fn(device_not_available_handler_chirho);
    idt_chirho.invalid_tss.set_handler_fn(invalid_tss_handler_chirho);
    idt_chirho.stack_segment_fault.set_handler_fn(stack_segment_handler_chirho);
    idt_chirho.x87_floating_point.set_handler_fn(x87_fp_handler_chirho);
    idt_chirho.alignment_check.set_handler_fn(alignment_check_handler_chirho);
    idt_chirho.simd_floating_point.set_handler_fn(simd_fp_handler_chirho);

    // --- Hardware interrupt handlers ---

    idt_chirho[InterruptIndexChirho::TimerChirho.as_u8_chirho()]
        .set_handler_fn(timer_interrupt_handler_chirho);

    idt_chirho[InterruptIndexChirho::KeyboardChirho.as_u8_chirho()]
        .set_handler_fn(keyboard_interrupt_handler_chirho);

    // Serial port COM1 (IRQ 4 = vector 36) — fires when serial data arrives
    idt_chirho[(PIC_1_OFFSET_CHIRHO + 4)]
        .set_handler_fn(serial_interrupt_handler_chirho);

    // IRQ 11 (vector 43) — VirtIO PCI interrupt. Just ACK and return.
    idt_chirho[(PIC_1_OFFSET_CHIRHO + 11)]
        .set_handler_fn(virtio_interrupt_handler_chirho);

    idt_chirho
});

// ---------------------------------------------------------------------------
// Static keyboard decoder
// ---------------------------------------------------------------------------

/// Lazily-initialized PS/2 keyboard decoder protected by a spinlock.
/// Uses Scancode Set 1 (the default for most x86 hardware / emulators)
/// with `MapKeys104Us` layout and `HandleControl::Ignore` so control
/// characters are passed through without special treatment.
static KEYBOARD_CHIRHO: spin::Lazy<Mutex<pc_keyboard::Keyboard<pc_keyboard::layouts::Us104Key, pc_keyboard::ScancodeSet1>>> =
    spin::Lazy::new(|| {
        Mutex::new(pc_keyboard::Keyboard::new(
            pc_keyboard::ScancodeSet1::new(),
            pc_keyboard::layouts::Us104Key,
            pc_keyboard::HandleControl::Ignore,
        ))
    });

// ---------------------------------------------------------------------------
// User-mode preemption trampoline
// ---------------------------------------------------------------------------

/// One page below the fixed user stack region.
///
/// The timer interrupt rewrites the user interrupt frame to land here,
/// with the interrupted RIP pushed onto the user stack. The trampoline
/// does `sched_yield` through the normal syscall path and then `ret`s
/// back to the interrupted instruction stream.
const USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO: u64 = 0x7FFF_FF7F_E000;

#[repr(align(4096))]
struct UserPreemptTrampolinePageChirho {
    bytes_chirho: [u8; 4096],
}

const fn build_user_preempt_trampoline_page_chirho() -> UserPreemptTrampolinePageChirho {
    let mut bytes_chirho = [0xCC; 4096];
    // mov eax, 24  ; SYS_sched_yield_chirho
    bytes_chirho[0] = 0xB8;
    bytes_chirho[1] = 24;
    bytes_chirho[2] = 0;
    bytes_chirho[3] = 0;
    bytes_chirho[4] = 0;
    // syscall — sched_yield handler restores original RIP via SYSRET
    bytes_chirho[5] = 0x0F;
    bytes_chirho[6] = 0x05;
    // jmp -7 (loop back to mov eax) — safety net in case SYSRET lands here
    bytes_chirho[7] = 0xC3;
    UserPreemptTrampolinePageChirho { bytes_chirho }
}

static USER_PREEMPT_TRAMPOLINE_PAGE_CHIRHO: UserPreemptTrampolinePageChirho =
    build_user_preempt_trampoline_page_chirho();
static USER_PREEMPT_TRAMPOLINE_READY_CHIRHO: AtomicBool = AtomicBool::new(false);

pub fn init_user_preempt_trampoline_chirho() {
    if USER_PREEMPT_TRAMPOLINE_READY_CHIRHO.load(Ordering::Acquire) {
        return;
    }

    use x86_64::instructions::tlb;
    use x86_64::structures::paging::PageTableFlags;

    let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
    if boot_pml4_chirho.as_u64() == 0 {
        crate::serial_println_chirho!(
            "[PREEMPT-TRAMP] boot PML4 unavailable; trampoline not mapped"
        );
        return;
    }

    let trampoline_kernel_vaddr_chirho =
        &USER_PREEMPT_TRAMPOLINE_PAGE_CHIRHO as *const _ as u64;
    let Some((trampoline_phys_chirho, _kernel_flags_chirho)) =
        crate::pagetable_chirho::lookup_in_boot_pt_chirho(trampoline_kernel_vaddr_chirho)
    else {
        crate::serial_println_chirho!(
            "[PREEMPT-TRAMP] failed to resolve trampoline kernel page phys addr"
        );
        return;
    };

    let user_flags_chirho = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
    if crate::pagetable_chirho::map_page_in_pt_chirho(
        boot_pml4_chirho,
        USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO,
        trampoline_phys_chirho & !0xFFF,
        user_flags_chirho,
    ).is_err()
    {
        crate::serial_println_chirho!(
            "[PREEMPT-TRAMP] failed to map user trampoline page at {:#x}",
            USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO,
        );
        return;
    }

    tlb::flush(VirtAddr::new(USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO));
    USER_PREEMPT_TRAMPOLINE_READY_CHIRHO.store(true, Ordering::Release);
    crate::serial_println_chirho!(
        "[PREEMPT-TRAMP] mapped user trampoline at {:#x}",
        USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO,
    );
}

// ---------------------------------------------------------------------------
// Public initialisation functions
// ---------------------------------------------------------------------------

/// Load the IDT into the CPU's IDTR register.
///
/// Must be called before enabling interrupts.
pub fn init_idt_chirho() {
    IDT_CHIRHO.load();
}

/// Initialize the 8259 chained PICs and unmask all IRQ lines.
///
/// # Safety
/// This function performs port I/O to program the PIC hardware. It must only
/// be called once during early kernel init while interrupts are still disabled.
pub fn init_pics_chirho() {
    // SAFETY: We are programming the PIC during early boot with interrupts
    // disabled. The offsets (32, 40) do not conflict with CPU exception vectors.
    unsafe {
        PICS_CHIRHO.lock().initialize();

        // Unmask PIC IRQs EXCEPT IRQ 11 (VirtIO-net PCI interrupt).
        // IRQ 11 = slave PIC bit 3. Unmasked NIC interrupts during fork
        // cause a ~30-40% hang on KVM even with proper EOI handling,
        // because the interrupt fires during the fork trampoline and
        // corrupts the child's kernel stack or context.
        // Master PIC (0x21): unmask all (timer=0, kbd=1, cascade=2, serial=4)
        x86_64::instructions::port::Port::<u8>::new(0x21).write(0x00);
        // Slave PIC (0xA1): mask IRQ 11 (bit 3) — VirtIO-net
        x86_64::instructions::port::Port::<u8>::new(0xA1).write(0x08);

        // PS/2 keyboard controller re-enable removed — it was causing
        // keyboard interrupt issues. The bootloader leaves the PS/2
        // controller in a working state.
    }
}

// ---------------------------------------------------------------------------
// CPU exception handlers
// ---------------------------------------------------------------------------

/// INT3 breakpoint handler. Logs the exception and returns (breakpoints are
/// recoverable).
extern "x86-interrupt" fn breakpoint_handler_chirho(stack_frame_chirho: InterruptStackFrame) {
    crate::serial_println_chirho!(
        "[EXCEPTION] BREAKPOINT\n{:#?}",
        stack_frame_chirho
    );
}

/// Double-fault handler. This is a diverging handler because double faults
/// are not recoverable.
extern "x86-interrupt" fn double_fault_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
    _error_code_chirho: u64,
) -> ! {
    crate::serial_println_chirho!(
        "[EXCEPTION] DOUBLE FAULT (last syscall={})\n{:#?}",
        crate::syscall_chirho::LAST_SYSCALL_NR_CHIRHO.load(core::sync::atomic::Ordering::Relaxed),
        stack_frame_chirho
    );
    loop {
        x86_64::instructions::hlt();
    }
}

/// Page-fault handler. Checks for COW faults first; if the fault is not
/// COW-resolvable, prints the faulting address and error code, then halts.
/// Fault disposition — what action to take after analyzing a page fault.
/// Replaces ad-hoc boolean branching with an explicit decision type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultDispositionChirho {
    /// Page successfully mapped — resume the faulting instruction.
    ResolvedChirho,
    /// User-mode fault that can't be resolved — kill the task.
    KillTaskChirho,
    /// Kernel-mode fault that can't be resolved — panic.
    PanicKernelChirho,
    /// Fault requires deferred repair (e.g., COW page copy, demand paging).
    /// The repair will be done outside the fault handler context.
    QueueDeferredRepairChirho,
}

/// Fault source classification for clearer dispatch.
#[derive(Debug, Clone, Copy)]
struct FaultContextChirho {
    addr_chirho: u64,
    is_user_chirho: bool,
    is_write_chirho: bool,
    is_present_chirho: bool,
}

extern "x86-interrupt" fn page_fault_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    let is_user_chirho = error_code_chirho.contains(PageFaultErrorCode::USER_MODE);
    let is_write_chirho = error_code_chirho.contains(PageFaultErrorCode::CAUSED_BY_WRITE);
    let is_present_chirho = error_code_chirho.contains(PageFaultErrorCode::PROTECTION_VIOLATION);

    // COW (Copy-on-Write) handling: write fault on present page with COW bit.
    // This must be checked BEFORE lazy migration — both user and kernel mode.
    if is_write_chirho && is_present_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            if crate::pagetable_chirho::handle_cow_fault_chirho(fault_addr_chirho) {
                return; // COW resolved — retry the write instruction
            }
        }
    }

    // Handle kernel-mode faults on user-space addresses.
    // The kernel legitimately accesses user memory during ELF loading
    // (copy_nonoverlapping), stack setup, and data copying. Treat these
    // the same as user-mode faults: lazy migration + allocation.
    if !is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            let page_vaddr_chirho = fault_addr_chirho.as_u64() & !0xFFF;
            if page_vaddr_chirho < 0x8000_0000_0000 {
                use x86_64::structures::paging::{
                    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
                };

                let current_pml4_chirho = crate::pagetable_chirho::get_current_pml4_phys_chirho();
                let rw_flags_chirho = PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE;

                // Try lazy migration from boot PT first.
                if let Some((phys_chirho, _boot_flags_chirho)) =
                    crate::pagetable_chirho::lookup_in_boot_pt_chirho(page_vaddr_chirho)
                {
                    // If it was a write-protection fault, also update boot PT flags.
                    if is_present_chirho {
                        let page_chirho: Page<Size4KiB> = Page::containing_address(fault_addr_chirho);
                        if let Some(ref mut mapper_chirho) = *crate::mm_chirho::GLOBAL_MAPPER_CHIRHO.lock() {
                            match unsafe { mapper_chirho.update_flags(page_chirho, rw_flags_chirho) } {
                                Ok(flush_chirho) => flush_chirho.flush(),
                                Err(map_error_chirho) => {
                                    crate::serial_println_chirho!(
                                        "[PF] update_flags failed for {:#x}: {:?}",
                                        page_vaddr_chirho,
                                        map_error_chirho
                                    );
                                }
                            }
                        }
                    }
                    if let Err(map_error_chirho) = crate::pagetable_chirho::map_page_in_pt_chirho(
                        current_pml4_chirho, page_vaddr_chirho, phys_chirho, rw_flags_chirho,
                    ) {
                        crate::serial_println_chirho!(
                            "[PF] lazy migrate failed for {:#x}: {:?}",
                            page_vaddr_chirho,
                            map_error_chirho
                        );
                    }
                    x86_64::instructions::tlb::flush(fault_addr_chirho);
                    return;
                }

                // Not in boot PT — allocate a new frame.
                let page_chirho: Page<Size4KiB> = Page::containing_address(fault_addr_chirho);
                let mut mg_chirho = loop {
                    if let Some(g_chirho) = crate::mm_chirho::GLOBAL_MAPPER_CHIRHO.try_lock() { break g_chirho; }
                    core::hint::spin_loop();
                };
                let mut ag_chirho = loop {
                    if let Some(g_chirho) = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() { break g_chirho; }
                    core::hint::spin_loop();
                };
                if let (Some(mapper_chirho), Some(alloc_chirho)) = (mg_chirho.as_mut(), ag_chirho.as_mut()) {
                    if let Some(frame_chirho) = alloc_chirho.allocate_frame() {
                        let phys_chirho = frame_chirho.start_address().as_u64();
                        match unsafe { mapper_chirho.map_to(page_chirho, frame_chirho, rw_flags_chirho, alloc_chirho) } {
                            Ok(flush_chirho) => flush_chirho.flush(),
                            Err(map_error_chirho) => {
                                crate::serial_println_chirho!(
                                    "[PF] boot map_to failed for {:#x}: {:?}",
                                    page_vaddr_chirho,
                                    map_error_chirho
                                );
                            }
                        }
                        if let Err(map_error_chirho) = crate::pagetable_chirho::map_page_in_pt_chirho(
                            current_pml4_chirho, page_vaddr_chirho, phys_chirho, rw_flags_chirho,
                        ) {
                            crate::serial_println_chirho!(
                                "[PF] per-process map failed for {:#x}: {:?}",
                                page_vaddr_chirho,
                                map_error_chirho
                            );
                        }
                        x86_64::instructions::tlb::flush(fault_addr_chirho);
                        unsafe { core::ptr::write_bytes(page_vaddr_chirho as *mut u8, 0, 4096); }
                        return;
                    }
                }
            }
        }
    }

    if is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            use x86_64::structures::paging::{
                FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
            };
            use x86_64::VirtAddr;

            let page_vaddr_chirho = fault_addr_chirho.as_u64() & !0xFFF;

            // --- Lazy page migration from boot PT ---
            let current_pml4_chirho = crate::pagetable_chirho::get_current_pml4_phys_chirho();
            if let Some((phys_chirho, boot_flags_chirho)) =
                crate::pagetable_chirho::lookup_in_boot_pt_chirho(page_vaddr_chirho)
            {
                if crate::pagetable_chirho::map_page_in_pt_chirho(
                    current_pml4_chirho,
                    page_vaddr_chirho,
                    phys_chirho,
                    boot_flags_chirho,
                ).is_ok() {
                    x86_64::instructions::tlb::flush(fault_addr_chirho);
                    return; // Retry — page now mapped from boot PT
                }
            }

            // --- Normal page fault: allocate a new frame ---
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(fault_addr_chirho);

            let flags_chirho = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;

            // Spin-wait for mapper and allocator locks.
            let mut mg_chirho = loop {
                if let Some(g_chirho) = crate::mm_chirho::GLOBAL_MAPPER_CHIRHO.try_lock() {
                    break g_chirho;
                }
                core::hint::spin_loop();
            };
            let mut ag_chirho = loop {
                if let Some(g_chirho) = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock() {
                    break g_chirho;
                }
                core::hint::spin_loop();
            };
            if let (Some(mapper_chirho), Some(alloc_chirho)) = (mg_chirho.as_mut(), ag_chirho.as_mut()) {
                if let Some(frame_chirho) = alloc_chirho.allocate_frame() {
                    let frame_phys_chirho = frame_chirho.start_address().as_u64();

                    // Map in the boot PML4 (via global mapper).
                    let map_result_chirho = unsafe {
                        mapper_chirho.map_to(page_chirho, frame_chirho, flags_chirho, alloc_chirho)
                    };
                    if let Ok(flush_chirho) = map_result_chirho {
                        flush_chirho.flush();
                    }

                    // ALSO map in the current (per-process) PML4.
                    // Without this, the page exists in the boot PML4 but
                    // not the current PML4, causing infinite page faults.
                    if let Err(map_error_chirho) = crate::pagetable_chirho::map_page_in_pt_chirho(
                        current_pml4_chirho,
                        page_vaddr_chirho,
                        frame_phys_chirho,
                        flags_chirho,
                    ) {
                        crate::serial_println_chirho!(
                            "[PF] user per-process map failed for {:#x}: {:?}",
                            page_vaddr_chirho,
                            map_error_chirho
                        );
                    }
                    x86_64::instructions::tlb::flush(fault_addr_chirho);

                    // Zero the page.
                    unsafe {
                        core::ptr::write_bytes(
                            (page_chirho.start_address().as_u64()) as *mut u8,
                            0,
                            4096,
                        );
                    }
                    return;
                }
            }
        }
    }

    // Could not handle the page fault.
    // If it's a user-mode fault, recover by re-launching the shell.
    if is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            crate::serial_println_chirho!(
                "[EXCEPTION] Unrecoverable user page fault at {:#x} — terminating process",
                fault_addr_chirho.as_u64(),
            );
        }
        crate::process_chirho::kill_and_respawn_shell_chirho("unrecoverable page fault");
    }

    // Kernel-mode page fault — log and halt.
    x86_64::instructions::interrupts::disable();
    {
        use x86_64::registers::control::Cr2;
        let fault_addr_chirho = Cr2::read().unwrap_or(x86_64::VirtAddr::zero());
        // Use raw serial port write to avoid mutex deadlock
        let msg_chirho = b"\r\n!!! PAGE FAULT - UNHANDLED !!!\r\n";
        for &b_chirho in msg_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        // Print fault address as hex
        let addr_val_chirho = fault_addr_chirho.as_u64();
        let hex_chars_chirho = b"0123456789abcdef";
        let prefix_chirho = b"addr=0x";
        for &b_chirho in prefix_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        for shift_chirho in (0..16).rev() {
            let nibble_chirho = ((addr_val_chirho >> (shift_chirho * 4)) & 0xF) as usize;
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(hex_chars_chirho[nibble_chirho]);
            }
        }
        // Print error code
        let err_prefix_chirho = b" err=";
        for &b_chirho in err_prefix_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        let err_val_chirho = error_code_chirho.bits() as u64;
        for shift_chirho in (0..2).rev() {
            let nibble_chirho = ((err_val_chirho >> (shift_chirho * 4)) & 0xF) as usize;
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(hex_chars_chirho[nibble_chirho]);
            }
        }
        let user_str_chirho = if is_user_chirho { b" USER" } else { b" KERN" };
        for &b_chirho in user_str_chirho.iter() {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        let nl_chirho = b"\r\n";
        for &b_chirho in nl_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}

/// General Protection Fault handler. Logs the error code and stack frame,
/// then halts.
extern "x86-interrupt" fn general_protection_fault_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: u64,
) {
    // Check if this GPF occurred in user mode (CS RPL == 3).
    let cs_chirho = stack_frame_chirho.code_segment.0;
    let is_user_chirho = (cs_chirho & 0x3) == 3;

    if is_user_chirho {
        // User-mode GPF — terminate the process gracefully (like SIGSEGV)
        // and re-launch the shell. This prevents the kernel from halting
        // when a user program executes an invalid instruction.
        let gpf_rip_chirho = stack_frame_chirho.instruction_pointer.as_u64();
        let gpf_rsp_chirho = stack_frame_chirho.stack_pointer.as_u64();
        crate::serial_println_chirho!(
            "[EXCEPTION] User-mode GPF at {:#x} (error_code={}) rsp={:#x} — terminating process",
            gpf_rip_chirho, error_code_chirho, gpf_rsp_chirho,
        );

        // GPT-directed: dump page content at GPF address to determine
        // if page is corrupted or if instruction faults on bad operand.
        {
            let pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(0);
            let (cr3_raw_chirho, _) = x86_64::registers::control::Cr3::read();
            let pt_root_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().page_table_root_chirho)
                .unwrap_or(None);
            let fs_base_chirho = unsafe {
                x86_64::registers::model_specific::Msr::new(0xC000_0100).read()
            };
            let expected_fs_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| t.lock().fs_base_chirho)
                .unwrap_or(0);
            crate::serial_println_chirho!(
                "[GPF-DIAG] pid={} CR3={:#x} pt_root={:?} FS={:#x} task.fs={:#x} match={}",
                pid_chirho, cr3_raw_chirho.start_address().as_u64(),
                pt_root_chirho.map(|p| p.as_u64()),
                fs_base_chirho, expected_fs_chirho,
                fs_base_chirho == expected_fs_chirho,
            );
            // Dump bytes at the faulting RIP
            let bytes_chirho: [u8; 16] = unsafe {
                let ptr_chirho = gpf_rip_chirho as *const [u8; 16];
                core::ptr::read_volatile(ptr_chirho)
            };
            crate::serial_println_chirho!(
                "[GPF-DIAG] bytes@{:#x}: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                gpf_rip_chirho,
                bytes_chirho[0], bytes_chirho[1], bytes_chirho[2], bytes_chirho[3],
                bytes_chirho[4], bytes_chirho[5], bytes_chirho[6], bytes_chirho[7],
                bytes_chirho[8], bytes_chirho[9], bytes_chirho[10], bytes_chirho[11],
                bytes_chirho[12], bytes_chirho[13], bytes_chirho[14], bytes_chirho[15],
            );
        }

        // Dump full stack backtrace at GPF
        if gpf_rip_chirho == 0x7f0000163040 {
            // Key offsets: vfprintf ret=0x198, vsnprintf ret=0x2b8,
            // dropbear log ret=0x508 (= 0x2b8 + 0x248 + 8)
            // GPT: expected return addr at [C+0x508] should be PIE+0x33d0
            // or PIE+0x347a. Dump fine-grained around that slot.
            let offsets_chirho: [u64; 10] = [
                0x198, 0x2b8,
                0x4f8, 0x500, 0x508, 0x510, 0x518,
                0x928, 0x930, 0x938,
            ];
            for off_chirho in offsets_chirho {
                let addr_chirho = gpf_rsp_chirho + off_chirho;
                if addr_chirho > 0x7fff00000000 && addr_chirho < 0x800000000000 {
                    let val_chirho = unsafe {
                        core::ptr::read_volatile(addr_chirho as *const u64)
                    };
                    crate::serial_println_chirho!(
                        "[GPF-STACK] [rsp+{:#x}]={:#x}",
                        off_chirho, val_chirho,
                    );
                }
            }
        }

        // Remove current task from scheduler.
        crate::process_chirho::kill_and_respawn_shell_chirho("user-mode GPF");
    }

    crate::serial_println_chirho!(
        "[EXCEPTION] GENERAL PROTECTION FAULT (error code: {})\n{:#?}",
        error_code_chirho,
        stack_frame_chirho
    );
    // Kernel-mode GPF — non-recoverable.
    loop {
        x86_64::instructions::hlt();
    }
}

// ---------------------------------------------------------------------------
// Hardware interrupt handlers
// ---------------------------------------------------------------------------

/// PIC timer interrupt handler (IRQ 0).
///
/// Drives the scheduler tick — decrements the current task's time slice and
/// sets the reschedule flag when exhausted.
extern "x86-interrupt" fn timer_interrupt_handler_chirho(
    mut _stack_frame_chirho: InterruptStackFrame,
) {
    // Notify the scheduler of a timer tick.
    crate::scheduler_chirho::schedule_tick_chirho();

    // Send End-Of-Interrupt to both PIC and LAPIC.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(InterruptIndexChirho::TimerChirho.as_u8_chirho());
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }

    // Drive the polled network RX path from the periodic timer.  VirtIO-net
    // IRQ 11 is currently masked/ack-only during the SSH work, so blocked
    // tasks sleeping on SOCKET_DATA_WAITQUEUE_CHIRHO still need a periodic
    // kernel entry point that drains device RX rings and wakes them.
    // Try to poll network — skip if NET_DEVICES lock is held (avoids
    // deadlock when timer fires while a task is mid-poll_network).
    crate::net_chirho::try_poll_network_chirho();

    // Deferred user-mode preemption:
    // Rewrite the IRETQ frame so the interrupted task returns to a small
    // user trampoline that performs `sched_yield` via the normal syscall
    // path, then `ret`s back to the original user RIP that we pushed on
    // the user stack below.
    let interrupted_cs_chirho = _stack_frame_chirho.code_segment.0;
    let was_user_mode_chirho = (interrupted_cs_chirho & 0x3) == 3;

    if was_user_mode_chirho
        && crate::scheduler_chirho::need_resched_chirho()
        && USER_PREEMPT_TRAMPOLINE_READY_CHIRHO.load(Ordering::Acquire)
    {
        let current_pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(0);
        if current_pid_chirho >= 4 {
            let user_rip_chirho = _stack_frame_chirho.instruction_pointer.as_u64();
            let user_rsp_chirho = _stack_frame_chirho.stack_pointer.as_u64();

            // One-shot debug: log call context when PID 4+ is at a low boot address
            // (below user binary region at 0x400000 — indicates corrupted jump target)
            if current_pid_chirho >= 4 && user_rip_chirho < 0x400000 && user_rip_chirho > 0x1000 {
                use core::sync::atomic::{AtomicBool, Ordering};
                static LOGGED_CHIRHO: AtomicBool = AtomicBool::new(false);
                if !LOGGED_CHIRHO.swap(true, Ordering::Relaxed) {
                    crate::serial_println_chirho!(
                        "[TRAP-34B] pid={} rip={:#x} rsp={:#x}",
                        current_pid_chirho, user_rip_chirho, user_rsp_chirho,
                    );
                    // Read return addresses from user stack
                    for i_chirho in 0..4u64 {
                        let addr_chirho = user_rsp_chirho + i_chirho * 8;
                        if addr_chirho > 0x7fff00000000 && addr_chirho < 0x800000000000 {
                            let val_chirho = unsafe {
                                core::ptr::read_volatile(addr_chirho as *const u64)
                            };
                            crate::serial_println_chirho!(
                                "[TRAP-34B]   [rsp+{}]={:#x}",
                                i_chirho * 8, val_chirho,
                            );
                        }
                    }
                }
            }

            // Skip if already IN the trampoline page (avoid recursive push).
            let in_trampoline_chirho = user_rip_chirho >= USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO
                && user_rip_chirho < USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO + 8;
            if !in_trampoline_chirho {

            // Save the interrupted RIP in the task struct (kernel-side).
            // The trampoline does SYSCALL → sched_yield. The sched_yield
            // handler reads preempted_rip and sets the syscall frame's
            // RCX (return address for SYSRET) to the original RIP.
            // This avoids writing to the user stack (which may be COW
            // and can't be resolved in interrupt context).
            if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
                task_arc_chirho.lock().preempted_rip_chirho = user_rip_chirho;
            }
            unsafe {
                let mut frame_mut_chirho = _stack_frame_chirho.as_mut();
                frame_mut_chirho.update(|frame_value_chirho| {
                    frame_value_chirho.instruction_pointer =
                        VirtAddr::new(USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO);
                    frame_value_chirho.cpu_flags |= RFlags::INTERRUPT_FLAG;
                    // DON'T modify RSP — keep the original user stack
                });
            }
            } // else: not already in trampoline
        }
    }

    // GPT-directed watchpoint: check for stack corruption before timer IRET
    if was_user_mode_chirho {
        crate::syscall_entry_chirho::check_stack_watch_chirho("timer-iret");
    }

    // GPT-directed: restore user FS/GS base before returning from interrupt
    // to user mode. Without this, the timer IRET returns with whatever stale
    // FS base was live (e.g., PID 1's static BusyBox TLS 0x713198 instead
    // of PID 2's dropbear musl TLS 0x7f00001a4b28). This is the interrupt-
    // return counterpart to the syscall-return FS restore.
    if was_user_mode_chirho {
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
    }
}

/// Write LAPIC End-Of-Interrupt register via typed enum (A2-AUDIT-010).
#[inline(always)]
unsafe fn write_lapic_eoi_chirho(phys_offset_chirho: u64) {
    write_lapic_reg_chirho(phys_offset_chirho, LapicRegisterChirho::EoiChirho, 0);
}

/// PS/2 keyboard interrupt handler (IRQ 1).
///
/// Reads the scancode from I/O port 0x60, feeds it to the `pc_keyboard`
/// decoder, and prints any resulting character to the serial console.
extern "x86-interrupt" fn keyboard_interrupt_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    use x86_64::instructions::port::Port;

    // Read the scancode from the PS/2 data port.
    //
    // SAFETY: Port 0x60 is the standard PS/2 keyboard data port. Reading it
    // during the keyboard IRQ is expected and safe.
    let scancode_chirho: u8 = unsafe { Port::new(0x60).read() };

    // Decode the scancode into a key event, then into a character.
    let mut keyboard_chirho = KEYBOARD_CHIRHO.lock();
    if let Ok(Some(key_event_chirho)) = keyboard_chirho.add_byte(scancode_chirho) {
        if let Some(key_chirho) = keyboard_chirho.process_keyevent(key_event_chirho) {
            match key_chirho {
                pc_keyboard::DecodedKey::Unicode(character_chirho) => {
                    // Feed into TTY line discipline
                    let tty_chirho = crate::tty_chirho::tty0_chirho();
                    tty_chirho.input_char_chirho(character_chirho as u8);
                    // Feed into lock-free keyboard buffer for sys_read
                    crate::fbconsole_chirho::KB_INPUT_CHIRHO.push_chirho(character_chirho as u8);
                    // Also push to serial port so BusyBox sees it
                    unsafe {
                        while Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                        Port::<u8>::new(0x3F8).write(character_chirho as u8);
                    }
                }
                pc_keyboard::DecodedKey::RawKey(key_raw_chirho) => {
                    // Non-Unicode keys (arrows, function keys, etc.) are
                    // logged but not forwarded to the TTY input buffer.
                    crate::serial_println_chirho!("[KBD] {:?}", key_raw_chirho);
                }
            }
        }
    }

    // Send End-Of-Interrupt to both PIC and Local APIC.
    // PIC EOI is needed for BIOS mode, LAPIC EOI for UEFI/IOAPIC mode.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(InterruptIndexChirho::KeyboardChirho.as_u8_chirho());

        // LAPIC EOI: write 0 to the EOI register at LAPIC base + 0xB0
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}

// ---------------------------------------------------------------------------
// IOAPIC keyboard routing (for UEFI mode)
// ---------------------------------------------------------------------------

/// Initialize the IOAPIC to route IRQ1 (keyboard) to vector 33.
/// The IOAPIC is at physical address 0xFEC00000 (standard location).
/// In UEFI mode, the PIC doesn't deliver keyboard interrupts — the
/// IOAPIC must be programmed to route them.
pub fn init_local_apic_chirho() {
    // Enable the Local APIC by setting bit 8 (APIC Software Enable) in the
    // Spurious Interrupt Vector Register (SVR) at offset 0xF0.
    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();

    unsafe {
        // Read current SVR, set bit 8 (enable) and spurious vector to 0xFF
        let current_chirho = read_lapic_reg_chirho(phys_offset_chirho, LapicRegisterChirho::SpuriousChirho);
        write_lapic_reg_chirho(phys_offset_chirho, LapicRegisterChirho::SpuriousChirho, current_chirho | 0x1FF);

        // Set Task Priority Register to 0 (accept all interrupts)
        write_lapic_reg_chirho(phys_offset_chirho, LapicRegisterChirho::TprChirho, 0);

        crate::serial_println_chirho!(
            "[LAPIC] Enabled (SVR={:#x})",
            read_lapic_reg_chirho(phys_offset_chirho, LapicRegisterChirho::SpuriousChirho)
        );
    }
}

pub fn init_ioapic_keyboard_chirho() {
    // The IOAPIC is memory-mapped. We need to access it via the
    // physical memory offset provided by the bootloader.
    let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
    let ioapic_base_chirho = phys_offset_chirho + 0xFEC0_0000u64;

    unsafe {
        let ioregsel_chirho = ioapic_base_chirho as *mut u32;
        let iowin_chirho = (ioapic_base_chirho + 0x10) as *mut u32;

        // Read IOAPIC version to verify it's accessible
        core::ptr::write_volatile(ioregsel_chirho, 0x01); // IOAPICVER
        let ver_chirho = core::ptr::read_volatile(iowin_chirho);
        let max_redir_chirho = ((ver_chirho >> 16) & 0xFF) as u8;

        crate::serial_println_chirho!(
            "[IOAPIC] Version: {:#x}, max redirections: {}",
            ver_chirho & 0xFF,
            max_redir_chirho
        );

        // Program redirection table entry 1 (IRQ1 = keyboard)
        // Each entry is 64 bits: low 32 bits at 0x10+2*n, high 32 bits at 0x11+2*n
        let redir_low_reg_chirho = 0x10 + 2 * 1; // Entry 1, low
        let redir_high_reg_chirho = 0x10 + 2 * 1 + 1; // Entry 1, high

        // Low 32 bits:
        //   bits 7:0  = vector (33 = PIC_1_OFFSET + 1)
        //   bit 8     = delivery mode (0 = Fixed)
        //   bit 11    = destination mode (0 = physical)
        //   bit 13    = pin polarity (0 = active high)
        //   bit 15    = trigger mode (0 = edge)
        //   bit 16    = mask (0 = enabled)
        let vector_chirho: u32 = (PIC_1_OFFSET_CHIRHO as u32) + 1; // 33
        let low_chirho: u32 = vector_chirho; // All other bits 0 = fixed, physical, active-high, edge, unmasked

        // High 32 bits:
        //   bits 27:24 = destination APIC ID (0 = BSP)
        let high_chirho: u32 = 0; // Deliver to APIC ID 0 (BSP)

        // Write high first (it's safe, entry is masked until low is written)
        core::ptr::write_volatile(ioregsel_chirho, redir_high_reg_chirho);
        core::ptr::write_volatile(iowin_chirho, high_chirho);

        // Write low (this unmasks the entry)
        core::ptr::write_volatile(ioregsel_chirho, redir_low_reg_chirho);
        core::ptr::write_volatile(iowin_chirho, low_chirho);

        // Also route IRQ0 (timer) to vector 32 via IOAPIC
        let timer_low_reg_chirho = 0x10; // Entry 0, low
        let timer_high_reg_chirho = 0x11; // Entry 0, high
        let timer_vector_chirho: u32 = PIC_1_OFFSET_CHIRHO as u32; // 32
        core::ptr::write_volatile(ioregsel_chirho, timer_high_reg_chirho);
        core::ptr::write_volatile(iowin_chirho, 0u32); // dest APIC 0
        core::ptr::write_volatile(ioregsel_chirho, timer_low_reg_chirho);
        core::ptr::write_volatile(iowin_chirho, timer_vector_chirho);

        crate::serial_println_chirho!(
            "[IOAPIC] IRQ0 (timer) -> vec {}, IRQ1 (keyboard) -> vec {}",
            timer_vector_chirho,
            vector_chirho
        );
    }
}

/// Segment Not Present (#NP) handler — vector 11.
/// This fires when the CPU tries to use a segment descriptor that has the
/// Present bit clear. Usually caused by a bad SS/CS selector during SYSRET.
extern "x86-interrupt" fn segment_not_present_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: u64,
) {
    crate::serial_println_chirho!(
        "[EXCEPTION] SEGMENT NOT PRESENT (error code: {:#x})\n{:#?}",
        error_code_chirho,
        stack_frame_chirho
    );
    loop {
        x86_64::instructions::hlt();
    }
}

/// Invalid Opcode (#UD) handler. Dumps the faulting instruction bytes.
extern "x86-interrupt" fn invalid_opcode_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
) {
    let rip_chirho = stack_frame_chirho.instruction_pointer.as_u64();
    let cs_chirho = stack_frame_chirho.code_segment.0;
    let is_user_chirho = (cs_chirho & 0x3) == 3;

    crate::serial_println_chirho!(
        "[EXCEPTION] INVALID OPCODE (#UD) at {:#x} (CS={:#x} {})",
        rip_chirho, cs_chirho,
        if is_user_chirho { "user" } else { "kernel" },
    );
    crate::serial_println_chirho!(
        "  RSP={:#x} SS={:#x}",
        stack_frame_chirho.stack_pointer.as_u64(),
        stack_frame_chirho.stack_segment.0,
    );
    // Print bytes at the faulting address for diagnosis
    if rip_chirho > 0x1000 {
        let bytes_chirho = unsafe {
            core::slice::from_raw_parts(rip_chirho as *const u8, 8)
        };
        crate::serial_println_chirho!(
            "  Bytes at RIP: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
            bytes_chirho[0], bytes_chirho[1], bytes_chirho[2], bytes_chirho[3],
            bytes_chirho[4], bytes_chirho[5], bytes_chirho[6], bytes_chirho[7],
        );
    }

    if is_user_chirho {
        crate::process_chirho::kill_and_respawn_shell_chirho("invalid opcode (#UD)");
    }

    // Kernel-mode #UD — halt.
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn overflow_handler_chirho(_sf: InterruptStackFrame) {
    crate::serial_println_chirho!("[EXCEPTION] OVERFLOW");
}

extern "x86-interrupt" fn bound_range_handler_chirho(_sf: InterruptStackFrame) {
    crate::serial_println_chirho!("[EXCEPTION] BOUND RANGE");
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn device_not_available_handler_chirho(_sf: InterruptStackFrame) {
    // #NM — FPU/SSE not available. Enable FPU by setting CR0.TS=0.
    crate::serial_println_chirho!("[EXCEPTION] DEVICE NOT AVAILABLE (#NM) — enabling FPU");
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB", // clear CR0.EM (bit 2)
            "or ax, 0x2",     // set CR0.MP (bit 1)
            "mov cr0, rax",
            "mov rax, cr4",
            "or eax, 0x10600", // set CR4.OSFXSR(9) + CR4.OSXMMEXCPT(10) + CR4.FSGSBASE(16)
            "mov cr4, rax",
            out("rax") _,
            options(nomem, nostack)
        );
    }
}

extern "x86-interrupt" fn invalid_tss_handler_chirho(_sf: InterruptStackFrame, ec: u64) {
    crate::serial_println_chirho!("[EXCEPTION] INVALID TSS (error code: {:#x})", ec);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn stack_segment_handler_chirho(_sf: InterruptStackFrame, ec: u64) {
    crate::serial_println_chirho!("[EXCEPTION] STACK SEGMENT FAULT (error code: {:#x})", ec);
    loop { x86_64::instructions::hlt(); }
}

extern "x86-interrupt" fn x87_fp_handler_chirho(_sf: InterruptStackFrame) {
    crate::serial_println_chirho!("[EXCEPTION] x87 FP EXCEPTION");
}

extern "x86-interrupt" fn alignment_check_handler_chirho(_sf: InterruptStackFrame, ec: u64) {
    crate::serial_println_chirho!("[EXCEPTION] ALIGNMENT CHECK (error code: {:#x})", ec);
}

extern "x86-interrupt" fn simd_fp_handler_chirho(_sf: InterruptStackFrame) {
    crate::serial_println_chirho!("[EXCEPTION] SIMD FP EXCEPTION");
}

/// VirtIO PCI interrupt handler (IRQ 11, vector 43).
/// Reads the ISR register to acknowledge the device-side interrupt,
/// then sends EOI to the PIC.
extern "x86-interrupt" fn virtio_interrupt_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    // Read the VirtIO ISR status register to acknowledge the device
    // interrupt. Without this, the device won't deliver new interrupts.
    crate::virtio_chirho::ack_virtio_interrupt_chirho();

    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt((PIC_1_OFFSET_CHIRHO + 11) as u8);
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}

/// Serial port COM1 interrupt handler (IRQ 4, vector 36).
/// Fires when data arrives on the serial port. We just acknowledge
/// the interrupt — the actual data reading happens in the polling loop.
extern "x86-interrupt" fn serial_interrupt_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    // Don't read data here — the polling loop in sys_read_stdin handles it.
    // Just send EOI to acknowledge the interrupt.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt((PIC_1_OFFSET_CHIRHO + 4) as u8);
        // LAPIC EOI
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}
