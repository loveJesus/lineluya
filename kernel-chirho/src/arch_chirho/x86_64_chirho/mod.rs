// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! x86_64 architecture implementation for Lineluya.
//!
//! This is the traditional bare-metal target using real hardware:
//! GDT, IDT, PIC/APIC, page tables, SYSCALL/SYSRET, serial UART, etc.
//!
//! The existing kernel modules (gdt_chirho, interrupts_chirho, memory_chirho,
//! etc.) serve as the x86_64 arch implementation. This module re-exports
//! the architecture trait implementation.

pub fn init_chirho() {
    // x86_64 init is handled by the existing boot sequence in main_chirho.rs
}

pub fn halt_chirho() {
    x86_64::instructions::hlt();
}

pub fn disable_interrupts_chirho() {
    x86_64::instructions::interrupts::disable();
}

pub fn enable_interrupts_chirho() {
    x86_64::instructions::interrupts::enable();
}

pub fn timestamp_chirho() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

pub fn debug_print_chirho(s_chirho: &str) {
    // Uses the serial_chirho module's _print_chirho
    crate::serial_chirho::_print_chirho(format_args!("{}", s_chirho));
}
