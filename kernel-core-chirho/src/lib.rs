// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! # Lineluya Kernel Core
//!
//! Architecture-independent kernel code shared between all targets:
//! x86_64, wasm32, aarch64, riscv64.

#![no_std]

extern crate alloc;

use core::sync::atomic::{AtomicPtr, Ordering};

/// Architecture abstraction — each target implements this.
pub trait ArchPortChirho {
    fn debug_print_chirho(&self, s: &str);
    fn timestamp_us_chirho(&self) -> u64;
    fn yield_cpu_chirho(&self);
    fn console_read_chirho(&self, buf: &mut [u8]) -> usize;
}

/// Global arch port pointer (set once during boot).
static ARCH_PORT_PTR_CHIRHO: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// VTable pointer for the arch port.
static ARCH_PORT_VTABLE_CHIRHO: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the architecture port (called once during boot).
pub fn set_arch_port_chirho(port_chirho: &'static dyn ArchPortChirho) {
    let fat_ptr_chirho: [usize; 2] = unsafe { core::mem::transmute(port_chirho) };
    ARCH_PORT_PTR_CHIRHO.store(fat_ptr_chirho[0] as *mut (), Ordering::Release);
    ARCH_PORT_VTABLE_CHIRHO.store(fat_ptr_chirho[1] as *mut (), Ordering::Release);
}

/// Get the architecture port.
pub fn arch_chirho() -> &'static dyn ArchPortChirho {
    let data_chirho = ARCH_PORT_PTR_CHIRHO.load(Ordering::Acquire);
    let vtable_chirho = ARCH_PORT_VTABLE_CHIRHO.load(Ordering::Acquire);
    assert!(!data_chirho.is_null(), "Architecture port not initialized");
    let fat_ptr_chirho: [usize; 2] = [data_chirho as usize, vtable_chirho as usize];
    unsafe { core::mem::transmute(fat_ptr_chirho) }
}

/// Print to debug console (architecture-independent).
pub fn kprint_chirho(s: &str) {
    let data_chirho = ARCH_PORT_PTR_CHIRHO.load(Ordering::Acquire);
    if !data_chirho.is_null() {
        arch_chirho().debug_print_chirho(s);
    }
}
