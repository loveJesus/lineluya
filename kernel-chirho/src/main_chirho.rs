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
mod fbconsole_chirho;
mod gdt_chirho;
mod interrupts_chirho;
mod memory_chirho;
mod allocator_chirho;

// Phase 3: Virtual Filesystem Switch (VFS)
mod vfs_chirho;
mod tmpfs_chirho;
mod devtmpfs_chirho;
mod procfs_chirho;
mod sysfs_chirho;
mod fs_chirho;

// Phase 5: Block I/O layer
mod block_chirho;
mod bio_chirho;

// Phase A4: VirtIO device drivers, GPT, ext4
mod virtio_chirho;
mod gpt_chirho;
mod ext4_chirho;

// Phase 6: Networking socket stubs
mod net_chirho;

// Phase 8: Hardware support — APIC, ACPI, PCI, SMP
mod apic_chirho;
mod acpi_chirho;
mod pci_chirho;
mod smp_chirho;

// Phase A5: Real hardware boot — bzImage, AHCI, NVMe, e1000, USB, timer, MSI
mod boot_protocol_chirho;
mod ahci_chirho;
mod hpet_chirho;
mod nvme_chirho;
mod e1000_chirho;
mod usb_chirho;
mod initramfs_chirho;
mod cmdline_chirho;
mod msi_chirho;
mod random_chirho;

// Phase E1: Real hardware boot — Multiboot2, dmesg
mod multiboot2_header_chirho;
mod dmesg_chirho;

// Phase 2: Process management & Linux syscall ABI
mod syscall_chirho;
mod syscall_entry_chirho;
mod task_chirho;
mod elf_chirho;
mod scheduler_chirho;
mod context_switch_chirho;
mod uaccess_chirho;
mod mm_chirho;
mod pagetable_chirho;
mod exec_chirho;
mod dynlink_chirho;
mod signal_chirho;
mod pipe_chirho;
mod process_chirho;
mod waitqueue_chirho;
mod futex_chirho;
mod tty_chirho;
mod pty_chirho;
mod fb_device_chirho;

// Phase 9: Advanced subsystem stubs
mod io_uring_chirho;
mod bpf_chirho;
mod vdso_chirho;
mod epoll_chirho;
mod cgroup_chirho;
mod namespace_chirho;
mod seccomp_chirho;
mod capability_chirho;
mod module_chirho;
mod ko_loader_chirho;
mod power_chirho;
mod trace_chirho;
mod eventfd_chirho;
mod inotify_chirho;

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

    // Enable SSE/SSE2 — required by musl and all modern x86_64 code.
    // Clear CR0.EM, set CR0.MP, set CR4.OSFXSR + CR4.OSXMMEXCPT.
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB", // clear EM (bit 2)
            "or ax, 0x2",     // set MP (bit 1)
            "mov cr0, rax",
            "mov rax, cr4",
            "or ax, 0x600",   // set OSFXSR (bit 9) + OSXMMEXCPT (bit 10)
            "mov cr4, rax",
            out("rax") _,
            options(nomem, nostack)
        );
    }
    serial_println_chirho!("[OK] SSE/SSE2 enabled");

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

    // Store the physical memory offset globally for the page table module.
    pagetable_chirho::set_phys_mem_offset_chirho(physical_memory_offset_chirho);

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

    // Initialize the framebuffer console (pixel-based text on UEFI screen)
    if let Some(fb_chirho) = boot_info_chirho.framebuffer.as_mut() {
        let fb_info_chirho = fb_chirho.info();
        let fb_buf_chirho = fb_chirho.buffer_mut();
        let is_bgr_chirho = matches!(fb_info_chirho.pixel_format, bootloader_api::info::PixelFormat::Bgr);
        fbconsole_chirho::FB_CONSOLE_CHIRHO.lock().init_chirho(
            fb_buf_chirho.as_mut_ptr(),
            fb_buf_chirho.len(),
            fb_info_chirho.width,
            fb_info_chirho.height,
            fb_info_chirho.bytes_per_pixel,
            fb_info_chirho.stride,
            is_bgr_chirho,
        );
        fb_println_chirho!("Lineluya kernel booting...");
        fb_println_chirho!("For God so loved the world that he gave his only begotten Son,");
        fb_println_chirho!("that whoever believes in him should not perish but have eternal life.");
        fb_println_chirho!("- John 3:16");
        fb_println_chirho!();
        fb_println_chirho!("[OK] Framebuffer console initialized ({}x{}, {}bpp)",
            fb_info_chirho.width, fb_info_chirho.height, fb_info_chirho.bytes_per_pixel * 8);

        // Configure /dev/fb0 device with actual framebuffer parameters
        // The physical address is derived from the virtual buffer pointer
        // and the physical memory offset.
        let fb_virt_addr_chirho = fb_buf_chirho.as_ptr() as u64;
        let fb_phys_addr_chirho = fb_virt_addr_chirho.wrapping_sub(physical_memory_offset_chirho);
        fb_device_chirho::set_fb_params_chirho(
            fb_phys_addr_chirho,
            fb_info_chirho.width as u32,
            fb_info_chirho.height as u32,
            (fb_info_chirho.stride * fb_info_chirho.bytes_per_pixel) as u32,
            (fb_info_chirho.bytes_per_pixel * 8) as u32,
            is_bgr_chirho,
        );
    }

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

    // Initialize kernel command line from bootloader (E1-014).
    cmdline_chirho::init_cmdline_from_bootinfo_chirho();

    // Initialize dmesg ring buffer (E1-015).
    dmesg_chirho::init_dmesg_chirho();

    // Phase 2: Initialize process management
    task_chirho::init_tasking_chirho();
    fb_println_chirho!("[OK] Task system initialized");

    scheduler_chirho::init_scheduler_chirho();
    fb_println_chirho!("[OK] Scheduler initialized");

    unsafe { syscall_chirho::init_syscalls_chirho() };
    fb_println_chirho!("[OK] Syscall interface initialized");

    unsafe { syscall_entry_chirho::init_syscall_entry_chirho() };
    fb_println_chirho!("[OK] Syscall entry trampoline initialized");

    fs_chirho::init_fs_chirho();
    fb_println_chirho!("[OK] Filesystem layer initialized");

    // Initialize PTY subsystem (needed for SSH, screen, tmux, xterm)
    pty_chirho::init_pty_chirho();
    fb_println_chirho!("[OK] PTY subsystem initialized");

    // Initialize kernel symbol table for .ko module loading
    ko_loader_chirho::init_kernel_symbols_chirho();

    // Phase A4: VirtIO device discovery — scan PCI bus, probe any VirtIO-blk,
    // and attempt to read sector 0 as a smoke test (P2-001 / P2-002).
    virtio_chirho::init_virtio_chirho();
    fb_println_chirho!("[OK] VirtIO subsystem initialized");

    net_chirho::init_networking_chirho();
    fb_println_chirho!("[OK] Networking initialized");

    x86_64::instructions::interrupts::enable();
    fb_println_chirho!("[OK] Interrupts enabled");

    fb_println_chirho!();
    fb_println_chirho!("=== Lineluya Kernel v2.0.0 ===");
    fb_println_chirho!("Linux-compatible kernel written in Rust");
    fb_println_chirho!("All subsystems initialized.");
    fb_println_chirho!();

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
        x86_64::instructions::interrupts::enable_and_hlt();
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
