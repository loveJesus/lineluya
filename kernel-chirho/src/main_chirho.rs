// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]

extern crate alloc;

// Phase 1: Bare metal boot
mod serial_chirho;
mod vga_buffer_chirho;
mod gdt_chirho;
mod interrupts_chirho;
mod memory_chirho;
mod allocator_chirho;

// Phase 3: Virtual Filesystem Switch (VFS)
mod vfs_chirho;
mod tmpfs_chirho;
mod devtmpfs_chirho;
mod procfs_chirho;

// Phase 2: Process management & Linux syscall ABI
mod syscall_chirho;
mod syscall_entry_chirho;
mod task_chirho;
mod elf_chirho;
mod scheduler_chirho;
mod context_switch_chirho;
mod uaccess_chirho;
mod mm_chirho;
mod exec_chirho;
mod signal_chirho;
mod pipe_chirho;
mod process_chirho;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping;
use core::panic::PanicInfo;

/// Bootloader configuration: map all physical memory so the kernel can access it.
pub static BOOTLOADER_CONFIG_CHIRHO: BootloaderConfig = {
    let mut config_chirho = BootloaderConfig::new_default();
    config_chirho.mappings.physical_memory = Some(Mapping::Dynamic);
    config_chirho
};

entry_point!(kernel_main_chirho, config = &BOOTLOADER_CONFIG_CHIRHO);

/// Main kernel entry point. Called by the bootloader after setting up long mode,
/// paging, and providing boot information.
fn kernel_main_chirho(boot_info_chirho: &'static mut BootInfo) -> ! {
    serial_println_chirho!("Lineluya kernel booting...");
    serial_println_chirho!("For God so loved the world that he gave his only begotten Son,");
    serial_println_chirho!("that whoever believes in him should not perish but have eternal life.");
    serial_println_chirho!("- John 3:16");
    serial_println_chirho!();

    // Initialize the Global Descriptor Table
    gdt_chirho::init_chirho();
    serial_println_chirho!("[OK] GDT initialized");

    // Initialize the Interrupt Descriptor Table
    interrupts_chirho::init_idt_chirho();
    serial_println_chirho!("[OK] IDT initialized");

    // Initialize the PICs (Programmable Interrupt Controllers)
    interrupts_chirho::init_pics_chirho();
    serial_println_chirho!("[OK] PICs initialized");

    // Initialize memory management
    let physical_memory_offset_chirho = boot_info_chirho
        .physical_memory_offset
        .into_option()
        .expect("Physical memory offset not provided by bootloader");

    let memory_regions_chirho = &boot_info_chirho.memory_regions;

    // Initialize the frame allocator with the memory map from the bootloader
    let mut frame_allocator_chirho =
        memory_chirho::BootInfoFrameAllocatorChirho::init_chirho(memory_regions_chirho);
    serial_println_chirho!("[OK] Frame allocator initialized");

    // Initialize the page mapper
    let mut mapper_chirho =
        unsafe { memory_chirho::init_mapper_chirho(physical_memory_offset_chirho) };
    serial_println_chirho!("[OK] Page mapper initialized");

    // Initialize the kernel heap
    allocator_chirho::init_heap_chirho(&mut mapper_chirho, &mut frame_allocator_chirho)
        .expect("Heap initialization failed");
    serial_println_chirho!("[OK] Heap allocator initialized");

    // Initialize the mm subsystem with a second mapper and a frame allocator
    // that starts where the boot allocator left off (to avoid double-allocating
    // frames used for the heap and page tables).
    {
        let mm_mapper_chirho =
            unsafe { memory_chirho::init_mapper_chirho(physical_memory_offset_chirho) };
        let mm_frame_alloc_chirho = mm_chirho::GlobalFrameAllocatorChirho::new_chirho(
            frame_allocator_chirho.memory_regions_chirho(),
            frame_allocator_chirho.next_index_chirho(),
        );
        unsafe {
            mm_chirho::init_mm_chirho(mm_mapper_chirho, mm_frame_alloc_chirho);
        }
    }

    // Phase 2: Initialize process management
    task_chirho::init_tasking_chirho();
    serial_println_chirho!("[OK] Task system initialized");

    scheduler_chirho::init_scheduler_chirho();
    serial_println_chirho!("[OK] Scheduler initialized");

    // Initialize syscall MSRs (SYSCALL/SYSRET mechanism)
    // SAFETY: Called once during early boot to set up SYSCALL MSRs.
    unsafe { syscall_chirho::init_syscalls_chirho() };
    serial_println_chirho!("[OK] Syscall interface initialized");

    // Initialize the SYSCALL assembly entry trampoline.
    // This allocates a dedicated kernel syscall stack and updates LSTAR
    // to point to the real assembly stub instead of the placeholder.
    // SAFETY: Called once after heap init; before userspace code runs.
    unsafe { syscall_entry_chirho::init_syscall_entry_chirho() };
    serial_println_chirho!("[OK] Syscall entry trampoline initialized");

    // Enable interrupts
    x86_64::instructions::interrupts::enable();
    serial_println_chirho!("[OK] Interrupts enabled");

    serial_println_chirho!();
    serial_println_chirho!("=== Lineluya Kernel v0.1.0 ===");
    serial_println_chirho!("Linux-compatible kernel written in Rust");
    serial_println_chirho!("All subsystems initialized.");
    serial_println_chirho!();

    // Test heap allocation
    {
        use alloc::boxed::Box;
        use alloc::vec;
        use alloc::string::String;

        let heap_value_chirho = Box::new(42);
        serial_println_chirho!("[TEST] Heap allocation: Box<i32> = {}", heap_value_chirho);

        let vec_chirho = vec![1, 2, 3, 4, 5];
        serial_println_chirho!("[TEST] Vec allocation: {:?}", vec_chirho);

        let string_chirho = String::from("Hallelujah! Lineluya kernel is alive!");
        serial_println_chirho!("[TEST] String allocation: {}", string_chirho);
    }

    // Test syscall dispatch (kernel-mode test)
    {
        let mut test_frame_chirho = syscall_chirho::SyscallFrameChirho::zeroed_chirho();
        // Test sys_uname (syscall 63)
        test_frame_chirho.rax_chirho = 63;
        let result_chirho = syscall_chirho::syscall_dispatch_chirho(&mut test_frame_chirho);
        serial_println_chirho!("[TEST] sys_uname dispatch returned: {}", result_chirho);

        // Test sys_getpid (syscall 39)
        test_frame_chirho.rax_chirho = 39;
        let result_chirho = syscall_chirho::syscall_dispatch_chirho(&mut test_frame_chirho);
        serial_println_chirho!("[TEST] sys_getpid returned: {}", result_chirho);
    }

    // Load and execute the hello world binary
    serial_println_chirho!();
    serial_println_chirho!("Loading hello world ELF into userspace...");
    exec_chirho::exec_init_chirho();
    // If we return here, userspace exited or failed to load
    serial_println_chirho!("Returned from exec_init_chirho (userspace exited or load failed).");

    serial_println_chirho!();
    serial_println_chirho!("Entering idle loop. Press keys to test keyboard interrupt.");

    // Enter the idle loop - halt until next interrupt
    hlt_loop_chirho();
}

/// Halt the CPU until the next interrupt, in a loop.
/// This is more power-efficient than a busy spin loop.
pub fn hlt_loop_chirho() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler - prints the panic info to serial and halts.
#[panic_handler]
fn panic_handler_chirho(info_chirho: &PanicInfo) -> ! {
    serial_println_chirho!("!!! KERNEL PANIC !!!");
    serial_println_chirho!("{}", info_chirho);
    loop {
        x86_64::instructions::hlt();
    }
}
