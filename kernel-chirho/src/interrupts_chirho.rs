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

    idt_chirho.page_fault.set_handler_fn(page_fault_handler_chirho);
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
    error_code_chirho: u64,
) -> ! {
    crate::serial_println_chirho!(
        "[EXCEPTION] DOUBLE FAULT (error code: {})\n{:#?}",
        error_code_chirho,
        stack_frame_chirho
    );
    // A double fault is not recoverable. Halt the CPU forever.
    loop {
        x86_64::instructions::hlt();
    }
}

/// Page-fault handler. Checks for COW faults first; if the fault is not
/// COW-resolvable, prints the faulting address and error code, then halts.
extern "x86-interrupt" fn page_fault_handler_chirho(
    stack_frame_chirho: InterruptStackFrame,
    error_code_chirho: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    let faulting_address_chirho = Cr2::read();

    // Check if this is a write fault (bit 1 of error code = CAUSED_BY_WRITE).
    // If so, attempt to resolve it as a COW (copy-on-write) fault.
    let is_write_fault_chirho = error_code_chirho.contains(PageFaultErrorCode::CAUSED_BY_WRITE);
    let is_present_chirho = error_code_chirho.contains(PageFaultErrorCode::PROTECTION_VIOLATION);

    if is_write_fault_chirho && is_present_chirho {
        // The page is present but not writable — could be COW.
        if let Ok(faulting_virt_chirho) = faulting_address_chirho {
            if crate::pagetable_chirho::handle_cow_fault_chirho(faulting_virt_chirho) {
                // COW fault resolved — return to the faulting instruction
                // which will now succeed with the writable page.
                return;
            }
        }
    }

    crate::serial_println_chirho!(
        "[EXCEPTION] PAGE FAULT"
    );
    crate::serial_println_chirho!(
        "  Accessed address: {:?}",
        faulting_address_chirho
    );
    crate::serial_println_chirho!(
        "  Error code:       {:?}",
        error_code_chirho
    );
    crate::serial_println_chirho!("{:#?}", stack_frame_chirho);

    // Page faults during kernel execution are fatal in this simple kernel.
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

    // Send End-Of-Interrupt to the PIC so it knows we handled the IRQ.
    //
    // SAFETY: We are notifying the PIC that the timer interrupt has been
    // serviced. The interrupt index is correct for IRQ 0.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(InterruptIndexChirho::TimerChirho.as_u8_chirho());
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

    // Send End-Of-Interrupt to the PIC.
    //
    // SAFETY: We are notifying the PIC that the keyboard interrupt has been
    // serviced. The interrupt index is correct for IRQ 1.
    unsafe {
        PICS_CHIRHO
            .lock()
            .notify_end_of_interrupt(InterruptIndexChirho::KeyboardChirho.as_u8_chirho());
    }
}
