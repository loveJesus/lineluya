// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Interrupt Descriptor Table (IDT) and interrupt handler module for the Lineluya kernel.
//!
//! This module sets up the IDT with handlers for CPU exceptions (breakpoint, double fault,
//! page fault, general protection fault) and hardware interrupts (timer, keyboard) via the
//! 8259 PIC. Keyboard scancodes are decoded using the `pc_keyboard` crate.

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use pic8259::ChainedPics;
use spin::Mutex;

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
pub static PICS_CHIRHO: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET_CHIRHO, PIC_2_OFFSET_CHIRHO) });

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

    // --- Hardware interrupt handlers ---

    idt_chirho[InterruptIndexChirho::TimerChirho.as_u8_chirho()]
        .set_handler_fn(timer_interrupt_handler_chirho);

    idt_chirho[InterruptIndexChirho::KeyboardChirho.as_u8_chirho()]
        .set_handler_fn(keyboard_interrupt_handler_chirho);

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

        // Unmask all PIC IRQs. Both PIC and IOAPIC can coexist — the PIC
        // handles interrupts in BIOS mode, IOAPIC in UEFI mode.
        x86_64::instructions::port::Port::<u8>::new(0x21).write(0x00);
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
extern "x86-interrupt" fn page_fault_handler_chirho(
    _stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    // For user-mode page faults: map the page directly using raw
    // page table operations. CANNOT use mm_chirho.mmap (would deadlock
    // if the mm locks are already held by the faulting syscall).
    let is_user_chirho = error_code_chirho.contains(PageFaultErrorCode::USER_MODE);

    if is_user_chirho {
        if let Ok(fault_addr_chirho) = Cr2::read() {
            // Map the faulting page directly via the global mapper.
            // Use try_lock to avoid deadlock — if the lock is held,
            // we can't map the page and must halt.
            use x86_64::structures::paging::{
                FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
            };
            use x86_64::VirtAddr;

            let page_chirho: Page<Size4KiB> =
                Page::containing_address(fault_addr_chirho);

            let flags_chirho = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;

            // Try to get mapper and allocator without blocking.
            // If locks are held, we can't map — fall through to halt.
            if let (Some(mut mg_chirho), Some(mut ag_chirho)) = (
                crate::mm_chirho::GLOBAL_MAPPER_CHIRHO.try_lock(),
                crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.try_lock(),
            ) {
                if let (Some(mapper_chirho), Some(alloc_chirho)) = (mg_chirho.as_mut(), ag_chirho.as_mut()) {
                    if let Some(frame_chirho) = alloc_chirho.allocate_frame() {
                        let map_result_chirho = unsafe {
                            mapper_chirho.map_to(page_chirho, frame_chirho, flags_chirho, alloc_chirho)
                        };
                        if let Ok(flush_chirho) = map_result_chirho {
                            flush_chirho.flush();
                            // Zero the page
                            unsafe {
                                core::ptr::write_bytes(
                                    (page_chirho.start_address().as_u64()) as *mut u8,
                                    0,
                                    4096,
                                );
                            }
                            return; // Retry the faulting instruction
                        }
                    }
                }
            }
        }
    }

    // Could not handle the page fault — halt.
    x86_64::instructions::interrupts::disable();
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
    crate::serial_println_chirho!(
        "[EXCEPTION] GENERAL PROTECTION FAULT (error code: {})\n{:#?}",
        error_code_chirho,
        stack_frame_chirho
    );
    // GPFs in kernel space are non-recoverable in this simple kernel.
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
    _stack_frame_chirho: InterruptStackFrame,
) {
    // Notify the scheduler of a timer tick.
    crate::scheduler_chirho::schedule_tick_chirho();

    // Send End-Of-Interrupt to both PIC and LAPIC.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(InterruptIndexChirho::TimerChirho.as_u8_chirho());
        // LAPIC EOI
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let lapic_eoi_chirho = (phys_offset_chirho + 0xFEE0_00B0u64) as *mut u32;
        core::ptr::write_volatile(lapic_eoi_chirho, 0);
    }
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
                    // Feed the character into the TTY line discipline.
                    // The TTY handles echo, canonical-mode buffering, and
                    // waking any tasks blocked on read().
                    let tty_chirho = crate::tty_chirho::tty0_chirho();
                    tty_chirho.input_char_chirho(character_chirho as u8);
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
        let lapic_eoi_chirho = (phys_offset_chirho + 0xFEE0_00B0u64) as *mut u32;
        core::ptr::write_volatile(lapic_eoi_chirho, 0);
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
    let lapic_base_chirho = phys_offset_chirho + 0xFEE0_0000u64;

    unsafe {
        let svr_chirho = (lapic_base_chirho + 0xF0) as *mut u32;
        let current_chirho = core::ptr::read_volatile(svr_chirho);
        // Set bit 8 (enable) and set spurious vector to 0xFF
        core::ptr::write_volatile(svr_chirho, current_chirho | 0x1FF);

        // Set Task Priority Register to 0 (accept all interrupts)
        let tpr_chirho = (lapic_base_chirho + 0x80) as *mut u32;
        core::ptr::write_volatile(tpr_chirho, 0);

        crate::serial_println_chirho!(
            "[LAPIC] Enabled (SVR={:#x})",
            core::ptr::read_volatile(svr_chirho)
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
