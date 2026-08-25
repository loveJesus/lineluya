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

    // IRQ 5 (vector 37) — SB16 ISA audio IRQ. Refill DMA buffer and ACK.
    idt_chirho[(PIC_1_OFFSET_CHIRHO + 5)]
        .set_handler_fn(sb16_audio_irq_handler_chirho);

    // IRQ 9 (vector 41) — ACPI / PCI steering. ACK and ignore.
    idt_chirho[(PIC_2_OFFSET_CHIRHO + 1)]
        .set_handler_fn(pci_audio_irq_handler_chirho);

    // IRQ 10 (vector 42) — PCI audio IRQ (AC97/HDA). ACK and ignore.
    idt_chirho[(PIC_2_OFFSET_CHIRHO + 2)]
        .set_handler_fn(pci_audio_irq_handler_chirho);

    // IRQ 3 (vector 35) — COM2 / PCI. ACK and ignore.
    idt_chirho[(PIC_1_OFFSET_CHIRHO + 3)]
        .set_handler_fn(pci_audio_irq_handler_chirho);

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
    // NOTE: this clobbers RAX but that's OK — most user code doesn't
    // depend on RAX across preemption (it's a scratch register).
    // The fork RAX issue is handled by the fork-yield mechanism.
    bytes_chirho[0] = 0xB8;
    bytes_chirho[1] = 24;
    bytes_chirho[2] = 0;
    bytes_chirho[3] = 0;
    bytes_chirho[4] = 0;
    // syscall — sched_yield handler restores original RIP via SYSRET
    bytes_chirho[5] = 0x0F;
    bytes_chirho[6] = 0x05;
    // ret — safety net (never reached, SYSRET returns to preempted_rip)
    bytes_chirho[7] = 0xC3;
    UserPreemptTrampolinePageChirho { bytes_chirho }
}

static USER_PREEMPT_TRAMPOLINE_PAGE_CHIRHO: UserPreemptTrampolinePageChirho =
    build_user_preempt_trampoline_page_chirho();
static USER_PREEMPT_TRAMPOLINE_READY_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Reset the READY flag so init_user_preempt_trampoline_chirho re-maps
/// the trampoline. Called after clear_user_pages removes the PTE.
pub fn reset_user_preempt_trampoline_ready_chirho() {
    USER_PREEMPT_TRAMPOLINE_READY_CHIRHO.store(false, Ordering::Release);
}

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
        // Slave PIC (0xA1): unmask all including IRQ 11 (VirtIO-net).
        // VirtIO interrupt handler now polls network + wakes waitqueues.
        x86_64::instructions::port::Port::<u8>::new(0xA1).write(0x00);

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

/// Saved user RBP from the most recent page fault (for backtrace).
static SAVED_USER_RBP_CHIRHO: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

extern "x86-interrupt" fn page_fault_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    // Save RBP early — in x86-interrupt ABI, the compiler may clobber
    // callee-saved registers after the prologue. Capture RBP now before
    // any function calls. For user-mode faults, this is the user's RBP
    // (CPU doesn't modify GPRs on interrupt entry).
    let saved_rbp_chirho: u64;
    unsafe { core::arch::asm!("mov {}, rbp", out(reg) saved_rbp_chirho, options(nomem, nostack)); }
    // Note: by this point the compiler prologue already saved/restored RBP.
    // The value we read is the KERNEL frame pointer, not the user's.
    // To get user RBP, we'd need naked function + manual frame setup.
    // Store it anyway — it might be the user value on some code paths.
    SAVED_USER_RBP_CHIRHO.store(saved_rbp_chirho, core::sync::atomic::Ordering::Relaxed);

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
                    Mapper, Page, PageTableFlags, Size4KiB,
                };

                let current_pml4_chirho = crate::pagetable_chirho::get_current_pml4_phys_chirho();
                let rw_flags_chirho = PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE;

                // Lazy migration from boot PT: only for PID 0/1 (which share boot PML4).
                // For PID >= 2 with authoritative per-process PTs, lazy migration
                // from boot PML4 is WRONG — it re-injects PID 0's BusyBox/heap
                // pages into the clean fresh PT, causing cross-process memory
                // aliasing and heap corruption (musl free() assert).
                let lazy_pid_chirho = crate::task_chirho::current_task_chirho()
                    .and_then(|t| t.try_lock().map(|g| g.pid_chirho)).unwrap_or(0);
                let allow_lazy_chirho = lazy_pid_chirho <= 1;
                if allow_lazy_chirho {
                    if let Some((phys_chirho, _boot_flags_chirho)) =
                        crate::pagetable_chirho::lookup_in_boot_pt_chirho(page_vaddr_chirho)
                    {
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
                }

                // Not in boot PT — allocate and map without retaining the
                // frame-allocator guard across page-table-level allocation.
                let (cr3_pf_chirho, _) = x86_64::registers::control::Cr3::read();
                match crate::pagetable_chirho::map_zeroed_demand_page_chirho(
                    cr3_pf_chirho.start_address(),
                    page_vaddr_chirho,
                    rw_flags_chirho,
                ) {
                    Ok(_) => {
                        x86_64::instructions::tlb::flush(fault_addr_chirho);
                        return;
                    }
                    Err(map_error_chirho) => {
                        crate::serial_println_chirho!(
                            "[PF] kernel demand map failed for {:#x}: {:?}",
                            page_vaddr_chirho,
                            map_error_chirho,
                        );
                    }
                }
            }
        }
    }

    if is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            use x86_64::structures::paging::{
                PageTableFlags,
            };

            let page_vaddr_chirho = fault_addr_chirho.as_u64() & !0xFFF;
            // --- Lazy page migration from boot PT (PID 0/1 only) ---
            // For PID >= 2 with authoritative per-process PTs, do NOT
            // import boot PML4 mappings — they belong to PID 0's init shell.
            let current_pml4_chirho = crate::pagetable_chirho::get_current_pml4_phys_chirho();
            let user_fault_pid_chirho = crate::task_chirho::current_task_chirho()
                .and_then(|t| t.try_lock().map(|g| g.pid_chirho)).unwrap_or(0);
            if user_fault_pid_chirho <= 1 {
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
            }

            // --- Guard: reject faults in the NULL page guard zone ---
            // Accessing addresses below 64KB is almost certainly a NULL
            // pointer dereference (struct->field with NULL base).
            // Don't allocate pages — deliver SIGSEGV or kill the process.
            if page_vaddr_chirho < 0x100000 {
                let rip_chirho = _stack_frame_chirho.instruction_pointer.as_u64();
                let rsp_chirho = _stack_frame_chirho.stack_pointer.as_u64();
                // Dump user registers for crash analysis
                let saved_rbp_chirho: u64;
                unsafe { core::arch::asm!("mov {}, rbp", out(reg) saved_rbp_chirho); }
                crate::serial_println_chirho!(
                    "[PF] NULL deref: pid={} addr={:#x} rip={:#x} rsp={:#x} — killing",
                    user_fault_pid_chirho,
                    fault_addr_chirho.as_u64(), rip_chirho, rsp_chirho,
                );
                // Read user stack values to find RDI (corrupted pointer)
                {
                    let po_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
                    let (cr3_d_chirho, _) = x86_64::registers::control::Cr3::read();
                    // User's RSP has the return state. Walk stack for context.
                    for off_chirho in 0..8u64 {
                        let saddr_chirho = rsp_chirho + off_chirho * 8;
                        if let Some(pte_chirho) = crate::pagetable_chirho::walk_page_table_chirho(
                            cr3_d_chirho.start_address(),
                            x86_64::VirtAddr::new(saddr_chirho & !0xFFF),
                        ) {
                            let phys_chirho = unsafe { (*pte_chirho).addr().as_u64() };
                            if phys_chirho != 0 {
                                let val_chirho = unsafe {
                                    *((phys_chirho + po_chirho + (saddr_chirho & 0xFFF)) as *const u64)
                                };
                                crate::serial_println_chirho!(
                                    "[PF-STACK] rsp+{:#x} = {:#018x}",
                                    off_chirho * 8, val_chirho,
                                );
                            }
                        }
                    }
                }
                // User stack dump via page table walk (reads user pages
                // through physical memory offset)
                let phys_off_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
                let (cr3_pf_chirho, _) = x86_64::registers::control::Cr3::read();
                // Helper: read u64 from user virtual address via PT walk
                let read_user_u64_chirho = |uva_chirho: u64| -> Option<u64> {
                    let pte_chirho = crate::pagetable_chirho::walk_page_table_chirho(
                        cr3_pf_chirho.start_address(),
                        x86_64::VirtAddr::new(uva_chirho & !0xFFF),
                    )?;
                    let phys_chirho = unsafe { (*pte_chirho).addr().as_u64() };
                    let kva_chirho = phys_chirho + phys_off_chirho + (uva_chirho & 0xFFF);
                    Some(unsafe { core::ptr::read_volatile(kva_chirho as *const u64) })
                };
                // Dump 20 stack entries
                for i_chirho in 0..20u64 {
                    if let Some(val_chirho) = read_user_u64_chirho(rsp_chirho + i_chirho * 8) {
                        if val_chirho != 0 {
                            crate::serial_println_chirho!(
                                "[PF]   [rsp+{:#04x}] = {:#018x}",
                                i_chirho * 8, val_chirho,
                            );
                        }
                    }
                }
                // For Xorg crash debugging: halt ONLY for PID >= 5
                // (skip boot processes that also hit NULL deref)
                // GDB halt for Xorg crash debugging (enable with -s flag):
                // GDB halt + user stack dump (disabled for normal operation):
                // Enable by uncommenting when debugging with QEMU -s flag
                // if user_fault_pid_chirho >= 4 && fault_addr_chirho.as_u64() == 0x2b33d { ... }
                // Kill the process with SIGSEGV
                if let Some(task_chirho) = crate::task_chirho::current_task_chirho() {
                    crate::process_chirho::exit_task_with_deferred_descriptor_retirement_chirho(
                        &task_chirho,
                        139,
                    );
                }
                crate::scheduler_chirho::schedule_chirho();
                return;
            }

            // --- VMA validation: only demand-page addresses within valid VMAs ---
            // Without this check, accesses to munmap'd regions silently get
            // zero-filled pages instead of SIGSEGV. This caused the Xorg crash:
            // musl's realloc does mmap+memcpy+munmap (mremap=ENOSYS), then
            // accessing the old pointer after munmap read zeros, corrupting
            // struct pointers → NULL deref at 0x2b33d.
            {
                let mm_arc_chirho = crate::mm_chirho::get_current_mm_chirho();
                let (has_vma_chirho, is_prot_none_chirho) = if let Some(mm_guard_chirho) = mm_arc_chirho.try_lock() {
                    let in_vma_chirho = mm_guard_chirho.is_in_vma_chirho(page_vaddr_chirho);
                    let prot_none_chirho = mm_guard_chirho.is_prot_none_chirho(page_vaddr_chirho);
                    (in_vma_chirho, prot_none_chirho)
                } else {
                    (true, false) // Can't check — allow (avoid deadlock)
                };
                // PROT_NONE guard page: deliver SIGSEGV (don't demand-page)
                if is_prot_none_chirho {
                    crate::serial_println_chirho!(
                        "[PF] PROT_NONE guard: pid={} addr={:#x} rip={:#x}",
                        user_fault_pid_chirho, fault_addr_chirho.as_u64(),
                        _stack_frame_chirho.instruction_pointer.as_u64(),
                    );
                    if let Some(task_chirho) = crate::task_chirho::current_task_chirho() {
                        crate::process_chirho::exit_task_with_deferred_descriptor_retirement_chirho(
                            &task_chirho,
                            139,
                        );
                    }
                    crate::scheduler_chirho::schedule_chirho();
                    return;
                }
                if !has_vma_chirho {
                    crate::serial_println_chirho!(
                        "[PF] SIGSEGV: pid={} addr={:#x} rip={:#x} — no VMA",
                        user_fault_pid_chirho, fault_addr_chirho.as_u64(),
                        _stack_frame_chirho.instruction_pointer.as_u64(),
                    );
                    if let Some(task_chirho) = crate::task_chirho::current_task_chirho() {
                        crate::process_chirho::exit_task_with_deferred_descriptor_retirement_chirho(
                            &task_chirho,
                            139,
                        );
                    }
                    crate::scheduler_chirho::schedule_chirho();
                    return;
                }
            }

            // --- Normal page fault: allocate and map a zero-filled frame ---
            let flags_chirho = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;

            // The helper releases the leaf-frame allocator guard before
            // map_page_in_pt_chirho can allocate intermediate table levels.
            let (cr3_upf_chirho, _) = x86_64::registers::control::Cr3::read();
            match crate::pagetable_chirho::map_zeroed_demand_page_chirho(
                cr3_upf_chirho.start_address(),
                page_vaddr_chirho,
                flags_chirho,
            ) {
                Ok(_) => {
                    x86_64::instructions::tlb::flush(fault_addr_chirho);
                    return;
                }
                Err(map_error_chirho) => {
                    crate::serial_println_chirho!(
                        "[PF] user demand map failed for {:#x}: {:?}",
                        page_vaddr_chirho,
                        map_error_chirho,
                    );
                }
            }
        }
    }

    // Could not handle the page fault.
    // If it's a user-mode fault, recover by re-launching the shell.
    if is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            let fault_va_chirho = fault_addr_chirho.as_u64();
            let (cr3_frame_chirho, _) = x86_64::registers::control::Cr3::read();
            let cr3_phys_chirho = cr3_frame_chirho.start_address();
            // Check if page exists in current PT vs boot PML4
            let in_current_chirho = crate::pagetable_chirho::lookup_in_pt_chirho(cr3_phys_chirho, fault_va_chirho).is_some();
            let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
            let pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(99);
            crate::serial_println_chirho!(
                "[PF-DIAG] pid={} fault={:#x} cr3={:#x} boot={:#x} in_mapper={} err={:#x}",
                pid_chirho, fault_va_chirho, cr3_phys_chirho.as_u64(),
                boot_pml4_chirho.as_u64(), in_current_chirho,
                error_code_chirho.bits(),
            );
            crate::serial_println_chirho!(
                "[EXCEPTION] Unrecoverable user page fault at {:#x} — terminating process",
                fault_va_chirho,
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
        // Print RIP from the stack frame
        let rip_prefix_chirho = b" rip=0x";
        for &b_chirho in rip_prefix_chirho {
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b_chirho);
            }
        }
        let rip_val_chirho = _stack_frame_chirho.instruction_pointer.as_u64();
        for shift_chirho in (0..16).rev() {
            let nibble_chirho = ((rip_val_chirho >> (shift_chirho * 4)) & 0xF) as usize;
            unsafe {
                while x86_64::instructions::port::Port::<u8>::new(0x3FD).read() & 0x20 == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(hex_chars_chirho[nibble_chirho]);
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

// Saved caller-saved registers from GPF — set by inline asm at handler entry.
static GPF_SAVED_RAX_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static GPF_SAVED_RCX_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static GPF_SAVED_RDI_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static GPF_SAVED_RSI_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// General Protection Fault handler. Logs the error code and stack frame,
/// then halts.
extern "x86-interrupt" fn general_protection_fault_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: u64,
) {
    // Save caller-saved registers before the compiler clobbers them.
    // NOTE: With extern "x86-interrupt", the compiler may have already
    // modified some of these. This is best-effort.
    {
        let rax_val_chirho: u64;
        let rcx_val_chirho: u64;
        let rdi_val_chirho: u64;
        let rsi_val_chirho: u64;
        unsafe {
            core::arch::asm!(
                "",
                out("rax") rax_val_chirho,
                out("rcx") rcx_val_chirho,
                out("rdi") rdi_val_chirho,
                out("rsi") rsi_val_chirho,
                options(nomem, nostack, preserves_flags),
            );
        }
        GPF_SAVED_RAX_CHIRHO.store(rax_val_chirho, core::sync::atomic::Ordering::Relaxed);
        GPF_SAVED_RCX_CHIRHO.store(rcx_val_chirho, core::sync::atomic::Ordering::Relaxed);
        GPF_SAVED_RDI_CHIRHO.store(rdi_val_chirho, core::sync::atomic::Ordering::Relaxed);
        GPF_SAVED_RSI_CHIRHO.store(rsi_val_chirho, core::sync::atomic::Ordering::Relaxed);
    }

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

        // Dump saved GPRs (best-effort — compiler may have clobbered some).
        {
            let saved_rax_chirho = GPF_SAVED_RAX_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
            let saved_rcx_chirho = GPF_SAVED_RCX_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
            let saved_rdi_chirho = GPF_SAVED_RDI_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
            let saved_rsi_chirho = GPF_SAVED_RSI_CHIRHO.load(core::sync::atomic::Ordering::Relaxed);
            crate::serial_println_chirho!(
                "[GPF-GPRS] rax={:#x} rcx={:#x} rdi={:#x} rsi={:#x} (may be compiler-clobbered)",
                saved_rax_chirho, saved_rcx_chirho, saved_rdi_chirho, saved_rsi_chirho,
            );
        }

        // Dump user-space register state at GPF via saved context.
        // The interrupt saves RSP/RIP in the frame; for GPRs we read from
        // the user stack and nearby memory to reconstruct the call context.
        // For the musl free() GPF at cmp [rax+0x10],rcx:
        //   rdi = chunk being freed (passed to free)
        //   rax = prev_chunk ptr (read from [rdi-0x10])
        // We can infer these from the stack frame.
        if gpf_rip_chirho >= 0x7f0000100000 && gpf_rip_chirho < 0x7f0000200000 {
            // Read the 16 bytes below the user RSP to see saved registers
            let user_rsp_chirho = gpf_rsp_chirho;
            crate::serial_println_chirho!(
                "[GPF-REGS] pid={} rip={:#x} user_rsp={:#x}",
                crate::scheduler_chirho::current_pid_chirho().unwrap_or(0),
                gpf_rip_chirho, user_rsp_chirho,
            );
            // Dump 128 bytes above and below rsp for register forensics
            for off_chirho in (0..128).step_by(8) {
                let addr_chirho = user_rsp_chirho.wrapping_sub(64).wrapping_add(off_chirho);
                if addr_chirho > 0x7fff00000000 && addr_chirho < 0x800000000000 {
                    let val_chirho = unsafe {
                        core::ptr::read_volatile(addr_chirho as *const u64)
                    };
                    let marker_chirho = if addr_chirho == user_rsp_chirho { " <-- RSP" } else { "" };
                    crate::serial_println_chirho!(
                        "[GPF-REGS]   [{:#x}] = {:#018x}{}",
                        addr_chirho, val_chirho, marker_chirho,
                    );
                }
            }
        }

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
            // Dump user stack to trace the call chain
            if gpf_rsp_chirho > 0x7fff00000000 && gpf_rsp_chirho < 0x800000000000 {
                crate::serial_println_chirho!("[GPF-STACK] RSP={:#x} stack dump:", gpf_rsp_chirho);
                for i_chirho in 0..8u64 {
                    let addr_chirho = gpf_rsp_chirho + i_chirho * 8;
                    if addr_chirho < 0x800000000000 {
                        let val_chirho = unsafe {
                            core::ptr::read_volatile(addr_chirho as *const u64)
                        };
                        crate::serial_println_chirho!(
                            "[GPF-STACK]   [rsp+{:#x}] = {:#018x}",
                            i_chirho * 8, val_chirho,
                        );
                    }
                }
            }
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

        // TEMPORARY: For musl's HLT assertions (a_crash), skip the HLT
        // instruction instead of killing the process. This allows us to
        // diagnose whether the heap corruption is fatal or recoverable.
        // musl uses HLT (0xf4) as a crash assertion — in ring 3, HLT causes GPF.
        {
            let first_byte_chirho = unsafe { core::ptr::read_volatile(gpf_rip_chirho as *const u8) };
            if first_byte_chirho == 0xf4 {
                // HLT instruction — skip it (1 byte) and continue execution.
                // This is a TEMPORARY diagnostic workaround, not a fix.
                use core::sync::atomic::{AtomicU64, Ordering};
                static HLT_SKIP_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
                let skip_n_chirho = HLT_SKIP_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed);
                if skip_n_chirho < 100 {
                    crate::serial_println_chirho!(
                        "[GPF-HLT-SKIP] pid={} skipping HLT at {:#x} (skip #{})",
                        crate::scheduler_chirho::current_pid_chirho().unwrap_or(0),
                        gpf_rip_chirho,
                        skip_n_chirho,
                    );
                    unsafe {
                        let frame_ptr_chirho = &stack_frame_chirho as *const InterruptStackFrame
                            as *mut InterruptStackFrame;
                        (*frame_ptr_chirho).as_mut().update(|f_chirho| {
                            f_chirho.instruction_pointer = x86_64::VirtAddr::new(gpf_rip_chirho + 1);
                        });
                    }
                    return;
                }
                // After 5 skips, fall through to kill — heap is truly corrupt
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

    // Drive the polled network RX path from the periodic timer.
    crate::net_chirho::try_poll_network_chirho();

    // One-shot framebuffer screenshot dump request after 60 seconds of boot.
    crate::fb_device_chirho::maybe_dump_framebuffer_after_tick_chirho(
        crate::scheduler_chirho::tick_count_chirho(),
    );

    // Deferred user-mode preemption:
    // Rewrite the IRETQ frame so the interrupted task returns to a small
    // user trampoline that performs `sched_yield` via the normal syscall
    // path, then `ret`s back to the original user RIP that we pushed on
    // the user stack below.
    let interrupted_cs_chirho = _stack_frame_chirho.code_segment.0;
    let was_user_mode_chirho = (interrupted_cs_chirho & 0x3) == 3;

    // Log PID 5 timer state for preemption debugging
    {
        let dbg_any_pid_chirho =
            crate::scheduler_chirho::try_current_pid_chirho().unwrap_or(0);
        if dbg_any_pid_chirho == 5 {
            use core::sync::atomic::{AtomicU64, Ordering as KOrd};
            static P5_ANY_CNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
            let acnt_chirho = P5_ANY_CNT_CHIRHO.fetch_add(1, KOrd::Relaxed);
            if acnt_chirho % 200 == 0 {
                crate::serial_println_chirho!(
                    "[P5-ANY] tick={} user={} rip={:#x}",
                    acnt_chirho, was_user_mode_chirho,
                    _stack_frame_chirho.instruction_pointer.as_u64(),
                );
            }
        }
    }
    if was_user_mode_chirho {
        let dbg_pid_chirho =
            crate::scheduler_chirho::try_current_pid_chirho().unwrap_or(0);
        if dbg_pid_chirho == 5 {
            use core::sync::atomic::{AtomicU64, Ordering as DebugOrd};
            static P5_TIMER_CNT_CHIRHO: AtomicU64 = AtomicU64::new(0);
            let tcnt_chirho = P5_TIMER_CNT_CHIRHO.fetch_add(1, DebugOrd::Relaxed);
            if tcnt_chirho % 200 == 0 {
                let nr_chirho = crate::scheduler_chirho::need_resched_chirho();
                let pr_chirho = crate::task_chirho::current_task_chirho()
                    .map(|t| t.lock().preempted_rip_chirho).unwrap_or(0);
                let rip_chirho = _stack_frame_chirho.instruction_pointer.as_u64();
                crate::serial_println_chirho!(
                    "[P5-TIMER] tick={} need_resched={} preempted_rip={:#x} rip={:#x}",
                    tcnt_chirho, nr_chirho, pr_chirho, rip_chirho,
                );
            }
        }
    }

    // User-mode preemption trampoline: saves user RAX in task struct
    // (not on user stack) to avoid corrupting user stack state.
    // PID 5-only trampoline preemption. Full preemption for all PIDs
    // causes RSP corruption (page fault at -8). PID 5 (dropbear) is the
    // main CPU hog; preempting it lets Xorg and other processes run.
    // The is_task_runnable fix (Sleeping excluded) ensures the scheduler
    // picks Ready fork children instead of cycling through Sleeping PIDs.
    // DISABLED: yield_current in select HLT loop handles preemption.
    // The trampoline's user-mode window is too small for the 1ms timer.
    if false && was_user_mode_chirho
        && crate::scheduler_chirho::need_resched_chirho()
        && USER_PREEMPT_TRAMPOLINE_READY_CHIRHO.load(Ordering::Acquire)
    {
        let current_pid_chirho =
            crate::scheduler_chirho::try_current_pid_chirho().unwrap_or(0);
        // Only preempt PIDs >= 5 (daemons). PIDs 2-4 are boot processes
        // where RAX clobber from the trampoline breaks fork() return.
        if current_pid_chirho >= 5 {
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
                && user_rip_chirho < USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO + 9;
            if !in_trampoline_chirho {

            // Guard: skip if already preempted (preempted_rip != 0).
            // Re-preemption before sched_yield processes the first one
            // corrupts the saved RIP (overwrites real RIP with trampoline
            // or post-trampoline address), causing GPF at non-canonical
            // addresses like 0x800000000000.
            let already_preempted_chirho = crate::task_chirho::current_task_chirho()
                .map(|t| {
                    let mut tg_chirho = t.lock();
                    if tg_chirho.preempted_rip_chirho != 0 {
                        // Safety valve: if preempted_rip has been set for too
                        // long (the trampoline/sched_yield never ran), force-clear
                        // it to prevent permanent preemption stall.
                        tg_chirho.preempt_stale_chirho += 1;
                        if tg_chirho.preempt_stale_chirho > 3 {
                            tg_chirho.preempted_rip_chirho = 0;
                            tg_chirho.preempt_stale_chirho = 0;
                            false // allow new preemption
                        } else {
                            true // still pending
                        }
                    } else {
                        tg_chirho.preempt_stale_chirho = 0;
                        false
                    }
                })
                .unwrap_or(false);
            if already_preempted_chirho {
                // Skip — let the pending preemption complete first
            } else {

            // Diagnostic: verify trampoline bytes before redirecting
            if current_pid_chirho == 4 {
                use core::sync::atomic::{AtomicBool, Ordering as DiagOrd};
                static DIAG_DONE_CHIRHO: AtomicBool = AtomicBool::new(false);
                if !DIAG_DONE_CHIRHO.swap(true, DiagOrd::Relaxed) {
                    // Read first 8 bytes at trampoline address
                    // Expected: b8 18 00 00 00 0f 05 c3 (mov eax,24; syscall; ret)
                    let tramp_ptr_chirho = USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO as *const u64;
                    let tramp_bytes_chirho = unsafe { core::ptr::read_volatile(tramp_ptr_chirho) };
                    crate::serial_println_chirho!(
                        "[TRAMP-DIAG] pid=4 rip={:#x} trampoline@{:#x} bytes={:#018x} (expect 0xc3050f000018b8)",
                        user_rip_chirho,
                        USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO,
                        tramp_bytes_chirho,
                    );
                }
            }

            // Verify trampoline page is mapped in current PT before redirecting.
            // After fork/exec, the per-process PT might not have the trampoline.
            {
                let (cr3_tramp_chirho, _) = x86_64::registers::control::Cr3::read();
                let tramp_present_chirho = crate::pagetable_chirho::walk_page_table_chirho(
                    cr3_tramp_chirho.start_address(),
                    VirtAddr::new(USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO),
                ).map(|pte_chirho| unsafe {
                    (*pte_chirho).flags().contains(
                        x86_64::structures::paging::PageTableFlags::PRESENT
                    )
                }).unwrap_or(false);
                if !tramp_present_chirho {
                    // Re-map the trampoline into this process's page table
                    let tramp_kernel_vaddr_chirho =
                        &USER_PREEMPT_TRAMPOLINE_PAGE_CHIRHO as *const _ as u64;
                    if let Some((tramp_phys_chirho, _)) =
                        crate::pagetable_chirho::lookup_in_boot_pt_chirho(tramp_kernel_vaddr_chirho)
                    {
                        let user_flags_chirho = x86_64::structures::paging::PageTableFlags::PRESENT
                            | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;
                        let _ = crate::pagetable_chirho::map_page_in_pt_chirho(
                            cr3_tramp_chirho.start_address(),
                            USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO,
                            tramp_phys_chirho & !0xFFF,
                            user_flags_chirho,
                        );
                        x86_64::instructions::tlb::flush(
                            VirtAddr::new(USER_PREEMPT_TRAMPOLINE_VADDR_CHIRHO)
                        );
                    }
                }
            }

            // Save the interrupted RIP in the task struct (kernel-side).
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
            } // close !already_preempted
            } // close !in_trampoline
        }
    }

    // Timer signal delivery DISABLED — corrupts processes by writing
    // sigframes to user stacks during interrupt context. Signal delivery
    // on syscall return (in syscall_dispatch) handles most cases.
    if false && was_user_mode_chirho {
        if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
            let has_sig_chirho = {
                let tg_chirho = task_arc_chirho.lock();
                let deliverable_chirho = tg_chirho.pending_signals_chirho
                    & !tg_chirho.signal_state_chirho.blocked_chirho;
                deliverable_chirho != 0
            };
            if has_sig_chirho {
                // Build a minimal SyscallFrame from the interrupt frame to
                // reuse the existing signal delivery mechanism.
                let user_rip_chirho = _stack_frame_chirho.instruction_pointer.as_u64();
                let user_rsp_chirho = _stack_frame_chirho.stack_pointer.as_u64();
                let mut fake_frame_chirho = crate::syscall_chirho::SyscallFrameChirho {
                    rax_chirho: 0,
                    rdi_chirho: 0, rsi_chirho: 0, rdx_chirho: 0,
                    r10_chirho: 0, r8_chirho: 0, r9_chirho: 0,
                    rcx_chirho: user_rip_chirho,  // return address
                    r11_chirho: _stack_frame_chirho.cpu_flags.bits(),
                    rsp_chirho: user_rsp_chirho,
                    rbx_chirho: 0, rbp_chirho: 0,
                    r12_chirho: 0, r13_chirho: 0, r14_chirho: 0, r15_chirho: 0,
                };
                if crate::signal_chirho::deliver_one_signal_on_return_chirho(&mut fake_frame_chirho) {
                    // Signal was delivered — update the interrupt frame to jump
                    // to the signal handler instead of the original user RIP.
                    unsafe {
                        let mut frame_mut_chirho = _stack_frame_chirho.as_mut();
                        frame_mut_chirho.update(|f_chirho| {
                            f_chirho.instruction_pointer =
                                VirtAddr::new(fake_frame_chirho.rcx_chirho);
                            f_chirho.stack_pointer =
                                VirtAddr::new(fake_frame_chirho.rsp_chirho);
                        });
                    }
                    let pid_chirho = task_arc_chirho.lock().pid_chirho;
                    crate::serial_println_chirho!(
                        "[TIMER-SIG] pid={} delivering signal, new_rip={:#x}",
                        pid_chirho, fake_frame_chirho.rcx_chirho,
                    );
                }
            }
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

        // Initialize PIT channel 0 to 1000 Hz (1ms tick)
        // PIT oscillator: 1,193,182 Hz. Divisor for 1000 Hz = 1193.
        {
            let divisor_chirho: u16 = 1193; // 1193182 / 1000 ≈ 1193
            let mut cmd_port_chirho = x86_64::instructions::port::Port::<u8>::new(0x43);
            let mut ch0_port_chirho = x86_64::instructions::port::Port::<u8>::new(0x40);
            cmd_port_chirho.write(0x36); // channel 0, lo/hi, mode 3 (square wave)
            ch0_port_chirho.write((divisor_chirho & 0xFF) as u8); // low byte
            ch0_port_chirho.write((divisor_chirho >> 8) as u8);   // high byte
            crate::serial_println_chirho!("[PIT] Timer initialized: 1000 Hz (1ms tick)");
        }

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

    // Poll network on VirtIO interrupt — this is the primary RX path.
    // The timer also polls, but VirtIO interrupts arrive immediately
    // when new packets arrive, giving much lower latency.
    crate::net_chirho::try_poll_network_chirho();
}

/// PCI audio device IRQ handler (AC97/HDA — IRQ 3/5/9/10).
/// Just ACK the PIC so the device doesn't lock up the interrupt line.
extern "x86-interrupt" fn pci_audio_irq_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    unsafe {
        // ACK both PICs (safe even if only master fired)
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(PIC_2_OFFSET_CHIRHO + 2);
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}

/// SB16 ISA audio interrupt handler (IRQ 5, vector 37).
/// Acknowledges the DSP interrupt, refills the DMA buffer, then sends EOI.
extern "x86-interrupt" fn sb16_audio_irq_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    crate::sound_chirho::sb16_irq_handler_chirho();
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt((PIC_1_OFFSET_CHIRHO + 5) as u8);
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}

/// Serial port COM1 interrupt handler (IRQ 4, vector 36).
///
/// Drains the UART receive register into the TTY line discipline, mirroring
/// the keyboard handler. `input_char_chirho` drops the ldisc guard before
/// waking `read_wait_chirho`, so no lock is held across the wake.
///
/// This used to be a bare EOI with the comment "the actual data reading
/// happens in the polling loop". That was load-bearing in the worst way: it
/// forced the console read to busy-poll, which is what made a blocking read
/// monopolise the CPU. It also left the UART asserting its receive condition,
/// since EOI without reading RBR does not clear the source.
///
/// Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md
extern "x86-interrupt" fn serial_interrupt_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
) {
    // Drain every byte the UART has buffered. Reading RBR (0x3F8) is what
    // actually clears the interrupt condition; EOI alone does not.
    unsafe {
        let tty_chirho = crate::tty_chirho::tty0_chirho();
        loop {
            let line_status_chirho: u8 =
                x86_64::instructions::port::Port::<u8>::new(0x3FD).read();
            if line_status_chirho & 0x01 == 0 {
                break; // receive buffer empty
            }
            let byte_chirho: u8 =
                x86_64::instructions::port::Port::<u8>::new(0x3F8).read();
            // Terminals send CR for Enter; the line discipline expects LF.
            let ch_chirho = if byte_chirho == b'\r' { b'\n' } else { byte_chirho };
            tty_chirho.input_char_chirho(ch_chirho);
        }
    }

    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt((PIC_1_OFFSET_CHIRHO + 4) as u8);
        // LAPIC EOI
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        write_lapic_eoi_chirho(phys_offset_chirho);
    }
}
