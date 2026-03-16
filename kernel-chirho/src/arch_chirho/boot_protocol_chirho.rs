// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux bzImage boot protocol structures for the Lineluya kernel (A5-001).
//!
//! Defines the `boot_params` and `setup_header` structures from the Linux
//! boot protocol (Documentation/x86/boot.rst), allowing GRUB / syslinux /
//! other Linux-compatible bootloaders to hand off control to our kernel.
//!
//! Reference: <https://www.kernel.org/doc/html/latest/x86/boot.html>

// ============================================================================
// Constants
// ============================================================================

/// Magic number in `setup_header.header` — must be "HdrS" (0x53726448).
pub const HDRS_MAGIC_CHIRHO: u32 = 0x5372_6448;

/// Boot protocol version we claim to support (2.15 = 0x020F).
pub const BOOT_PROTOCOL_VERSION_CHIRHO: u16 = 0x020F;

/// Linux boot flag indicating a loaded kernel.
pub const LOADED_HIGH_CHIRHO: u8 = 0x01;

/// Kernel type: bzImage (loaded high, above 1 MiB).
#[allow(dead_code)]
pub const SETUP_MOVE_SIZE_CHIRHO: u16 = 0x8000;

/// Boot loader ID for "undefined" / custom loader.
pub const BOOTLOADER_ID_UNDEFINED_CHIRHO: u8 = 0xFF;

/// Command-line maximum size.
pub const CMDLINE_SIZE_MAX_CHIRHO: u32 = 4096;

// ============================================================================
// E820 memory map
// ============================================================================

/// Maximum number of E820 entries Linux boot protocol supports (128).
pub const E820_MAX_ENTRIES_CHIRHO: usize = 128;

/// E820 memory types.
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum E820TypeChirho {
    /// Usable RAM.
    RamChirho = 1,
    /// Reserved / unusable.
    ReservedChirho = 2,
    /// ACPI reclaimable memory.
    AcpiChirho = 3,
    /// ACPI NVS (Non-Volatile Storage).
    AcpiNvsChirho = 4,
    /// Unusable / bad memory.
    UnusableChirho = 5,
}

/// A single E820 memory map entry.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct E820EntryChirho {
    /// Start of address range.
    pub addr_chirho: u64,
    /// Size of address range in bytes.
    pub size_chirho: u64,
    /// Type of address range (see [`E820TypeChirho`]).
    pub type_chirho: u32,
}

// ============================================================================
// setup_header — the kernel setup header (offset 0x1F1 in real-mode code)
// ============================================================================

/// The Linux kernel setup header, located at offset 0x1F1 in the boot sector.
///
/// Fields correspond to the documented Linux boot protocol. All multi-byte
/// fields are little-endian, matching x86 native order.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct SetupHeaderChirho {
    /// Number of setup sectors (0 means 4).
    pub setup_sects_chirho: u8,
    /// Deprecated root_flags.
    pub root_flags_chirho: u16,
    /// Size of the 32-bit (protected-mode) code in 16-byte paragraphs.
    pub syssize_chirho: u32,
    /// Deprecated RAM size.
    pub ram_size_chirho: u16,
    /// Video mode.
    pub vid_mode_chirho: u16,
    /// Default root device number.
    pub root_dev_chirho: u16,
    /// 0xAA55 boot signature.
    pub boot_flag_chirho: u16,
    /// Jump instruction (short jump + NOP).
    pub jump_chirho: u16,
    /// Magic "HdrS" = 0x53726448.
    pub header_chirho: u32,
    /// Boot protocol version (e.g. 0x020F for 2.15).
    pub version_chirho: u16,
    /// Hook for real-mode kernel (deprecated).
    pub realmode_swtch_chirho: u32,
    /// Deprecated start_sys_seg.
    pub start_sys_seg_chirho: u16,
    /// Pointer to kernel version string (relative to setup start).
    pub kernel_version_chirho: u16,
    /// Boot loader identifier (set by bootloader).
    pub type_of_loader_chirho: u8,
    /// Boot protocol option flags (bit 0 = LOADED_HIGH).
    pub loadflags_chirho: u8,
    /// Move size for real-mode code.
    pub setup_move_size_chirho: u16,
    /// Protected-mode kernel entry point (code32_start).
    pub code32_start_chirho: u32,
    /// Physical address of the initial ramdisk.
    pub ramdisk_image_chirho: u32,
    /// Size of the initial ramdisk.
    pub ramdisk_size_chirho: u32,
    /// Deprecated bootsect_kludge.
    pub bootsect_kludge_chirho: u32,
    /// Free memory after setup end.
    pub heap_end_ptr_chirho: u16,
    /// Extended boot loader version.
    pub ext_loader_ver_chirho: u8,
    /// Extended boot loader type.
    pub ext_loader_type_chirho: u8,
    /// Physical address of the kernel command line.
    pub cmd_line_ptr_chirho: u32,
    /// Highest legal initrd address.
    pub initrd_addr_max_chirho: u32,
    /// Physical address alignment required for kernel.
    pub kernel_alignment_chirho: u32,
    /// Whether kernel is relocatable.
    pub relocatable_kernel_chirho: u8,
    /// Minimum alignment (power of 2) as bit shift.
    pub min_alignment_chirho: u8,
    /// Xload flags.
    pub xloadflags_chirho: u16,
    /// Maximum command-line size.
    pub cmdline_size_chirho: u32,
    /// Hardware subarchitecture (0 = PC).
    pub hardware_subarch_chirho: u32,
    /// Subarchitecture-specific data.
    pub hardware_subarch_data_chirho: u64,
    /// Offset of compressed payload.
    pub payload_offset_chirho: u32,
    /// Length of compressed payload.
    pub payload_length_chirho: u32,
    /// 64-bit setup_data linked list.
    pub setup_data_chirho: u64,
    /// Preferred loading address.
    pub pref_address_chirho: u64,
    /// Amount of linear memory required.
    pub init_size_chirho: u32,
    /// Handover protocol offset.
    pub handover_offset_chirho: u32,
    /// 64-bit physical address of the kernel command line.
    pub kernel_info_offset_chirho: u32,
}

// ============================================================================
// screen_info — video information passed by bootloader
// ============================================================================

/// Video/screen information from the bootloader.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct ScreenInfoChirho {
    pub orig_x_chirho: u8,
    pub orig_y_chirho: u8,
    pub ext_mem_k_chirho: u16,
    pub orig_video_page_chirho: u16,
    pub orig_video_mode_chirho: u8,
    pub orig_video_cols_chirho: u8,
    pub flags_chirho: u8,
    pub unused2_chirho: u8,
    pub orig_video_ega_bx_chirho: u16,
    pub unused3_chirho: u16,
    pub orig_video_lines_chirho: u8,
    pub orig_video_is_vga_chirho: u8,
    pub orig_video_points_chirho: u16,

    // VESA fields
    pub lfb_width_chirho: u16,
    pub lfb_height_chirho: u16,
    pub lfb_depth_chirho: u16,
    pub lfb_base_chirho: u32,
    pub lfb_size_chirho: u32,
    pub cl_magic_chirho: u16,
    pub cl_offset_chirho: u16,
    pub lfb_linelength_chirho: u16,
    pub red_size_chirho: u8,
    pub red_pos_chirho: u8,
    pub green_size_chirho: u8,
    pub green_pos_chirho: u8,
    pub blue_size_chirho: u8,
    pub blue_pos_chirho: u8,
    pub rsvd_size_chirho: u8,
    pub rsvd_pos_chirho: u8,
    pub vesapm_seg_chirho: u16,
    pub vesapm_off_chirho: u16,
    pub pages_chirho: u16,
    pub vesa_attributes_chirho: u16,
    pub capabilities_chirho: u32,
    pub ext_lfb_base_chirho: u32,
    pub reserved_chirho: [u8; 2],
}

// ============================================================================
// boot_params — the master structure passed at kernel entry
// ============================================================================

/// The Linux `struct boot_params` — the master data block that a
/// Linux-compatible bootloader fills in before jumping to the kernel's
/// 32-bit or 64-bit entry point.
///
/// The structure is exactly 4096 bytes (one page), zero-padded.
#[repr(C, packed)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub struct BootParamsChirho {
    /// Screen/video information (0x000).
    pub screen_info_chirho: ScreenInfoChirho,
    /// APM BIOS info (0x040) — 20 bytes, not parsed here.
    pub apm_bios_info_chirho: [u8; 20],
    /// Padding (0x054).
    pub pad2_chirho: [u8; 4],
    /// Tboot shared page address (0x058).
    pub tboot_addr_chirho: u64,
    /// IST info (0x060) — 16 bytes.
    pub ist_info_chirho: [u8; 16],
    /// Padding (0x070).
    pub pad3_chirho: [u8; 16],
    /// HD info (0x080) — 16 bytes.
    pub hd0_info_chirho: [u8; 16],
    /// HD info (0x090) — 16 bytes.
    pub hd1_info_chirho: [u8; 16],
    /// SYS_DESC_TABLE info (0x0A0) — 16 bytes.
    pub sys_desc_table_chirho: [u8; 16],
    /// OLPC OFW header (0x0B0) — 16 bytes.
    pub olpc_ofw_header_chirho: [u8; 16],
    /// Extension of EXT_RAMDISK_IMAGE (0x0C0).
    pub ext_ramdisk_image_chirho: u32,
    /// Extension of EXT_RAMDISK_SIZE (0x0C4).
    pub ext_ramdisk_size_chirho: u32,
    /// Extension of EXT_CMD_LINE_PTR (0x0C8).
    pub ext_cmd_line_ptr_chirho: u32,
    /// Padding (0x0CC).
    pub pad4_chirho: [u8; 116],
    /// EFI info (0x140) — 32 bytes.
    pub efi_info_chirho: [u8; 32],
    /// Alternative mem check (0x160).
    pub alt_mem_k_chirho: u32,
    /// Scratch field (0x164).
    pub scratch_chirho: u32,
    /// Number of E820 entries (0x1E8).
    pub e820_entries_chirho: u8,
    /// EDDBUF entries count (0x1E9).
    pub eddbuf_entries_chirho: u8,
    /// EDD MBR signature buffer entries (0x1EA).
    pub edd_mbr_sig_buf_entries_chirho: u8,
    /// Keyboard status (0x1EB).
    pub kbd_status_chirho: u8,
    /// Secure boot flag (0x1EC).
    pub secure_boot_chirho: u8,
    /// Padding (0x1ED).
    pub pad5_chirho: [u8; 2],
    /// Sentinel value (0x1EF) — must be zero.
    pub sentinel_chirho: u8,
    /// Padding to reach setup_header at 0x1F1.
    pub pad6_chirho: [u8; 1],
    /// The setup header (0x1F1).
    pub hdr_chirho: SetupHeaderChirho,
    /// Padding between setup_header end and E820 map.
    pub pad7_chirho: [u8; 40],
    /// EDD MBR signatures (0x290) — up to 16 entries.
    pub edd_mbr_sig_buffer_chirho: [u32; 16],
    /// E820 memory map (0x2D0).
    pub e820_table_chirho: [E820EntryChirho; E820_MAX_ENTRIES_CHIRHO],
    /// Remaining padding to fill the page to 4096 bytes.
    /// (The exact tail layout is version-dependent; we pad to page size.)
    pub pad_tail_chirho: [u8; 48],
}

// ============================================================================
// Parsing helpers
// ============================================================================

impl BootParamsChirho {
    /// Interpret a raw pointer (physical address provided by bootloader in
    /// `%esi` / `RSI`) as a reference to `BootParamsChirho`.
    ///
    /// # Safety
    /// The caller must ensure `ptr_chirho` points to a valid, page-aligned
    /// `boot_params` structure in mapped memory.
    #[allow(dead_code)]
    pub unsafe fn from_ptr_chirho(ptr_chirho: *const u8) -> &'static Self {
        &*(ptr_chirho as *const Self)
    }

    /// Validate that the setup header contains the "HdrS" magic.
    #[allow(dead_code)]
    pub fn validate_header_chirho(&self) -> bool {
        let magic_chirho = self.hdr_chirho.header_chirho;
        magic_chirho == HDRS_MAGIC_CHIRHO
    }

    /// Return the boot protocol version from the setup header.
    #[allow(dead_code)]
    pub fn protocol_version_chirho(&self) -> u16 {
        self.hdr_chirho.version_chirho
    }

    /// Return the command-line physical address (32-bit).
    #[allow(dead_code)]
    pub fn cmdline_ptr_chirho(&self) -> u32 {
        self.hdr_chirho.cmd_line_ptr_chirho
    }

    /// Return the boot loader identifier byte.
    #[allow(dead_code)]
    pub fn loader_type_chirho(&self) -> u8 {
        self.hdr_chirho.type_of_loader_chirho
    }

    /// Return ramdisk (initrd) physical address.
    #[allow(dead_code)]
    pub fn ramdisk_addr_chirho(&self) -> u32 {
        self.hdr_chirho.ramdisk_image_chirho
    }

    /// Return ramdisk (initrd) size in bytes.
    #[allow(dead_code)]
    pub fn ramdisk_len_chirho(&self) -> u32 {
        self.hdr_chirho.ramdisk_size_chirho
    }

    /// Return the number of E820 entries.
    #[allow(dead_code)]
    pub fn e820_count_chirho(&self) -> usize {
        self.e820_entries_chirho as usize
    }

    /// Iterate over valid E820 entries.
    #[allow(dead_code)]
    pub fn e820_iter_chirho(&self) -> &[E820EntryChirho] {
        let count_chirho = self.e820_count_chirho().min(E820_MAX_ENTRIES_CHIRHO);
        &self.e820_table_chirho[..count_chirho]
    }

    /// Log all boot parameters to the serial console for debugging.
    #[allow(dead_code)]
    pub fn dump_chirho(&self) {
        crate::serial_println_chirho!("=== Boot Parameters (bzImage protocol) ===");
        let hdr_magic_copy_chirho = { self.hdr_chirho.header_chirho };
        crate::serial_println_chirho!(
            "  Header magic: {:#010x} (valid={})",
            hdr_magic_copy_chirho,
            self.validate_header_chirho()
        );
        crate::serial_println_chirho!(
            "  Protocol version: {:#06x}",
            self.protocol_version_chirho()
        );
        crate::serial_println_chirho!(
            "  Loader type: {:#04x}",
            self.loader_type_chirho()
        );
        crate::serial_println_chirho!(
            "  Cmdline ptr: {:#010x}",
            self.cmdline_ptr_chirho()
        );
        crate::serial_println_chirho!(
            "  Ramdisk: addr={:#010x} size={:#x}",
            self.ramdisk_addr_chirho(),
            self.ramdisk_len_chirho()
        );
        crate::serial_println_chirho!(
            "  E820 entries: {}",
            self.e820_count_chirho()
        );
        for (idx_chirho, entry_chirho) in self.e820_iter_chirho().iter().enumerate() {
            let e820_addr_copy_chirho = { entry_chirho.addr_chirho };
            let e820_size_copy_chirho = { entry_chirho.size_chirho };
            let e820_type_copy_chirho = { entry_chirho.type_chirho };
            crate::serial_println_chirho!(
                "    E820[{}]: addr={:#018x} size={:#018x} type={}",
                idx_chirho,
                e820_addr_copy_chirho,
                e820_size_copy_chirho,
                e820_type_copy_chirho
            );
        }
    }
}

/// Parse boot_params from a raw physical address.
///
/// This is the main entry point for A5-001: a bootloader (GRUB, syslinux)
/// passes the physical address of boot_params in RSI. After mapping it,
/// we call this function to parse and validate.
///
/// # Safety
/// The caller must ensure `boot_params_phys_chirho` points to valid memory.
#[allow(dead_code)]
pub unsafe fn parse_boot_params_chirho(
    boot_params_phys_chirho: *const u8,
) -> Option<&'static BootParamsChirho> {
    let params_chirho = BootParamsChirho::from_ptr_chirho(boot_params_phys_chirho);

    if !params_chirho.validate_header_chirho() {
        let hdr_magic_copy2_chirho = { params_chirho.hdr_chirho.header_chirho };
        crate::serial_println_chirho!(
            "boot_protocol: invalid header magic {:#010x}",
            hdr_magic_copy2_chirho
        );
        return None;
    }

    crate::serial_println_chirho!(
        "boot_protocol: valid bzImage header, protocol v{}.{}",
        params_chirho.protocol_version_chirho() >> 8,
        params_chirho.protocol_version_chirho() & 0xFF
    );

    Some(params_chirho)
}
