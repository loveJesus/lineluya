// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ACPI table parsing for the Lineluya kernel (A5-002).
//!
//! Provides structures and parsers for:
//! - RSDP (Root System Description Pointer) v1/v2
//! - RSDT (Root System Description Table, 32-bit pointers)
//! - XSDT (Extended System Description Table, 64-bit pointers)
//! - MADT (Multiple APIC Description Table) — APIC/IOAPIC enumeration
//! - FADT (Fixed ACPI Description Table) — power management, century reg
//! - HPET (High Precision Event Timer table)
//!
//! Reference: ACPI Specification 6.5 — <https://uefi.org/specs/ACPI/6.5/>

use core::ptr;

// ============================================================================
// RSDP — Root System Description Pointer
// ============================================================================

/// RSDP v1 / v2 descriptor found in BIOS/UEFI memory.
///
/// The first 20 bytes are the ACPI 1.0 RSDP; the remaining fields are
/// present only when `revision_chirho >= 2` (ACPI 2.0+).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct RsdpChirho {
    /// "RSD PTR " (8 bytes, not NUL-terminated).
    pub signature_chirho: [u8; 8],
    /// Checksum over the first 20 bytes.
    pub checksum_chirho: u8,
    /// OEM identifier (6 bytes).
    pub oem_id_chirho: [u8; 6],
    /// ACPI revision (0 = 1.0, 2 = 2.0+).
    pub revision_chirho: u8,
    /// Physical address of the RSDT (32-bit).
    pub rsdt_address_chirho: u32,

    // --- ACPI 2.0+ extended fields ---

    /// Length of the full RSDP structure.
    pub length_chirho: u32,
    /// Physical address of the XSDT (64-bit).
    pub xsdt_address_chirho: u64,
    /// Checksum over the entire structure (ACPI 2.0+).
    pub extended_checksum_chirho: u8,
    /// Reserved bytes.
    pub reserved_chirho: [u8; 3],
}

/// Expected RSDP signature: "RSD PTR ".
const RSDP_SIGNATURE_CHIRHO: [u8; 8] = *b"RSD PTR ";

impl RsdpChirho {
    /// Validate the RSDP v1 checksum (first 20 bytes must sum to 0).
    #[allow(dead_code)]
    pub fn validate_v1_checksum_chirho(&self) -> bool {
        let bytes_chirho =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, 20) };
        bytes_chirho.iter().fold(0u8, |acc_chirho, &b_chirho| acc_chirho.wrapping_add(b_chirho)) == 0
    }

    /// Validate the extended (v2) checksum over the full structure.
    #[allow(dead_code)]
    pub fn validate_v2_checksum_chirho(&self) -> bool {
        if self.revision_chirho < 2 {
            return true; // v1 has no extended checksum
        }
        let len_chirho = self.length_chirho as usize;
        let bytes_chirho =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, len_chirho) };
        bytes_chirho.iter().fold(0u8, |acc_chirho, &b_chirho| acc_chirho.wrapping_add(b_chirho)) == 0
    }

    /// Check if this is ACPI 2.0+ (has XSDT).
    #[allow(dead_code)]
    pub fn is_xsdt_available_chirho(&self) -> bool {
        self.revision_chirho >= 2 && self.xsdt_address_chirho != 0
    }
}

// ============================================================================
// Generic ACPI table header (SDT header)
// ============================================================================

/// Standard header that precedes every ACPI System Description Table.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct AcpiTableHeaderChirho {
    /// 4-byte ASCII signature (e.g. "APIC", "FACP", "HPET").
    pub signature_chirho: [u8; 4],
    /// Total length of the table including this header.
    pub length_chirho: u32,
    /// Revision of the table (table-specific meaning).
    pub revision_chirho: u8,
    /// Byte checksum — all bytes in the table must sum to zero.
    pub checksum_chirho: u8,
    /// OEM identifier.
    pub oem_id_chirho: [u8; 6],
    /// OEM table identifier.
    pub oem_table_id_chirho: [u8; 8],
    /// OEM revision.
    pub oem_revision_chirho: u32,
    /// Creator / ASL compiler ID.
    pub creator_id_chirho: u32,
    /// Creator revision.
    pub creator_revision_chirho: u32,
}

/// Size of the standard ACPI SDT header.
const SDT_HEADER_SIZE_CHIRHO: usize = core::mem::size_of::<AcpiTableHeaderChirho>();

impl AcpiTableHeaderChirho {
    /// Return the 4-byte signature as a `&str` (ASCII).
    #[allow(dead_code)]
    pub fn signature_str_chirho(&self) -> &str {
        core::str::from_utf8(&self.signature_chirho).unwrap_or("????")
    }

    /// Validate that all bytes in the table sum to zero.
    #[allow(dead_code)]
    pub fn validate_checksum_chirho(&self) -> bool {
        let len_chirho = self.length_chirho as usize;
        let bytes_chirho =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, len_chirho) };
        bytes_chirho.iter().fold(0u8, |acc_chirho, &b_chirho| acc_chirho.wrapping_add(b_chirho)) == 0
    }
}

// ============================================================================
// RSDP search
// ============================================================================

/// Attempt to locate the RSDP in standard BIOS memory regions.
///
/// Searches:
///   1. The first KiB of the EBDA (Extended BIOS Data Area).
///   2. The BIOS ROM region `0x000E_0000 .. 0x0010_0000`.
///
/// # Safety
/// Requires that physical memory below 1 MiB is identity-mapped or
/// accessible via the provided offset.
#[allow(dead_code)]
pub unsafe fn find_rsdp_bios_chirho(phys_offset_chirho: u64) -> Option<&'static RsdpChirho> {
    // Helper: scan a physical region for the RSDP signature on 16-byte boundaries.
    let scan_region_chirho = |start_chirho: u64, len_chirho: u64| -> Option<&'static RsdpChirho> {
        let mut addr_chirho = start_chirho;
        while addr_chirho < start_chirho + len_chirho {
            let virt_chirho = (phys_offset_chirho + addr_chirho) as *const u8;
            let sig_chirho = unsafe { core::slice::from_raw_parts(virt_chirho, 8) };
            if sig_chirho == &RSDP_SIGNATURE_CHIRHO {
                let rsdp_chirho = unsafe { &*(virt_chirho as *const RsdpChirho) };
                if rsdp_chirho.validate_v1_checksum_chirho() {
                    crate::serial_println_chirho!(
                        "ACPI: found RSDP at phys {:#010x} rev={}",
                        addr_chirho,
                        rsdp_chirho.revision_chirho
                    );
                    return Some(rsdp_chirho);
                }
            }
            addr_chirho += 16;
        }
        None
    };

    // 1. Read the EBDA segment from BDA at 0x040E.
    let bda_ptr_chirho = (phys_offset_chirho + 0x040E) as *const u16;
    let ebda_seg_chirho = unsafe { ptr::read_unaligned(bda_ptr_chirho) } as u64;
    let ebda_base_chirho = ebda_seg_chirho << 4;
    if ebda_base_chirho != 0 {
        if let Some(rsdp_chirho) = scan_region_chirho(ebda_base_chirho, 1024) {
            return Some(rsdp_chirho);
        }
    }

    // 2. Scan BIOS ROM area 0xE0000 .. 0x100000.
    scan_region_chirho(0x000E_0000, 0x0002_0000)
}

/// Locate the RSDP. Wraps [`find_rsdp_bios_chirho`] with a fallback stub
/// for UEFI environments (where the RSDP is in the EFI system table).
#[allow(dead_code)]
pub fn find_rsdp_chirho() -> Option<&'static RsdpChirho> {
    crate::serial_println_chirho!("[STUB] ACPI: RSDP search — use find_rsdp_bios_chirho() with phys offset");
    None
}

// ============================================================================
// RSDT — Root System Description Table (32-bit pointers)
// ============================================================================

/// Parse the RSDT to get an array of 32-bit physical addresses of other
/// ACPI tables.
///
/// # Safety
/// `rsdt_phys_chirho` must be a valid physical address mapped at the
/// given offset.
#[allow(dead_code)]
pub unsafe fn parse_rsdt_chirho(
    rsdt_phys_chirho: u32,
    phys_offset_chirho: u64,
) -> Option<&'static [u32]> {
    let virt_chirho = (phys_offset_chirho + rsdt_phys_chirho as u64) as *const AcpiTableHeaderChirho;
    let header_chirho = unsafe { &*virt_chirho };

    if &header_chirho.signature_chirho != b"RSDT" {
        crate::serial_println_chirho!("ACPI: RSDT signature mismatch");
        return None;
    }

    if !header_chirho.validate_checksum_chirho() {
        crate::serial_println_chirho!("ACPI: RSDT checksum invalid");
        return None;
    }

    let total_len_chirho = header_chirho.length_chirho as usize;
    let entries_len_chirho = total_len_chirho - SDT_HEADER_SIZE_CHIRHO;
    let num_entries_chirho = entries_len_chirho / 4;

    let entries_ptr_chirho =
        (virt_chirho as *const u8).add(SDT_HEADER_SIZE_CHIRHO) as *const u32;
    let entries_chirho =
        unsafe { core::slice::from_raw_parts(entries_ptr_chirho, num_entries_chirho) };

    crate::serial_println_chirho!("ACPI: RSDT has {} table entries", num_entries_chirho);
    Some(entries_chirho)
}

// ============================================================================
// XSDT — Extended System Description Table (64-bit pointers)
// ============================================================================

/// Parse the XSDT to get an array of 64-bit physical addresses of other
/// ACPI tables.
///
/// # Safety
/// `xsdt_phys_chirho` must be a valid physical address mapped at the
/// given offset.
#[allow(dead_code)]
pub unsafe fn parse_xsdt_chirho(
    xsdt_phys_chirho: u64,
    phys_offset_chirho: u64,
) -> Option<&'static [u64]> {
    let virt_chirho = (phys_offset_chirho + xsdt_phys_chirho) as *const AcpiTableHeaderChirho;
    let header_chirho = unsafe { &*virt_chirho };

    if &header_chirho.signature_chirho != b"XSDT" {
        crate::serial_println_chirho!("ACPI: XSDT signature mismatch");
        return None;
    }

    if !header_chirho.validate_checksum_chirho() {
        crate::serial_println_chirho!("ACPI: XSDT checksum invalid");
        return None;
    }

    let total_len_chirho = header_chirho.length_chirho as usize;
    let entries_len_chirho = total_len_chirho - SDT_HEADER_SIZE_CHIRHO;
    let num_entries_chirho = entries_len_chirho / 8;

    let entries_ptr_chirho =
        (virt_chirho as *const u8).add(SDT_HEADER_SIZE_CHIRHO) as *const u64;
    let entries_chirho =
        unsafe { core::slice::from_raw_parts(entries_ptr_chirho, num_entries_chirho) };

    crate::serial_println_chirho!("ACPI: XSDT has {} table entries", num_entries_chirho);
    Some(entries_chirho)
}

/// Find a table with a given 4-byte signature in the RSDT.
///
/// # Safety
/// Requires mapped physical memory.
#[allow(dead_code)]
pub unsafe fn find_table_rsdt_chirho(
    rsdt_phys_chirho: u32,
    phys_offset_chirho: u64,
    signature_chirho: &[u8; 4],
) -> Option<*const AcpiTableHeaderChirho> {
    let entries_chirho = unsafe { parse_rsdt_chirho(rsdt_phys_chirho, phys_offset_chirho)? };
    for &entry_phys_chirho in entries_chirho {
        let virt_chirho =
            (phys_offset_chirho + entry_phys_chirho as u64) as *const AcpiTableHeaderChirho;
        let hdr_chirho = unsafe { &*virt_chirho };
        if &hdr_chirho.signature_chirho == signature_chirho {
            return Some(virt_chirho);
        }
    }
    None
}

/// Find a table with a given 4-byte signature in the XSDT.
///
/// # Safety
/// Requires mapped physical memory.
#[allow(dead_code)]
pub unsafe fn find_table_xsdt_chirho(
    xsdt_phys_chirho: u64,
    phys_offset_chirho: u64,
    signature_chirho: &[u8; 4],
) -> Option<*const AcpiTableHeaderChirho> {
    let entries_chirho = unsafe { parse_xsdt_chirho(xsdt_phys_chirho, phys_offset_chirho)? };
    for &entry_phys_chirho in entries_chirho {
        let virt_chirho =
            (phys_offset_chirho + entry_phys_chirho) as *const AcpiTableHeaderChirho;
        let hdr_chirho = unsafe { &*virt_chirho };
        if &hdr_chirho.signature_chirho == signature_chirho {
            return Some(virt_chirho);
        }
    }
    None
}

// ============================================================================
// MADT — Multiple APIC Description Table (signature "APIC")
// ============================================================================

/// MADT header (after the standard SDT header).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtHeaderChirho {
    /// Standard ACPI header.
    pub header_chirho: AcpiTableHeaderChirho,
    /// Physical address of the Local APIC.
    pub local_apic_address_chirho: u32,
    /// Flags (bit 0 = PCAT_COMPAT — dual 8259 PICs installed).
    pub flags_chirho: u32,
}

/// MADT entry types.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MadtEntryTypeChirho {
    /// Processor Local APIC.
    LocalApicChirho = 0,
    /// I/O APIC.
    IoApicChirho = 1,
    /// Interrupt Source Override.
    InterruptOverrideChirho = 2,
    /// Non-Maskable Interrupt Source.
    NmiSourceChirho = 3,
    /// Local APIC NMI.
    LocalApicNmiChirho = 4,
    /// Local APIC Address Override (64-bit).
    LocalApicOverrideChirho = 5,
    /// Processor Local x2APIC.
    LocalX2ApicChirho = 9,
}

/// Generic MADT entry header (type + length).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtEntryHeaderChirho {
    pub entry_type_chirho: u8,
    pub length_chirho: u8,
}

/// MADT entry: Processor Local APIC (type 0).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtLocalApicChirho {
    pub header_chirho: MadtEntryHeaderChirho,
    /// ACPI Processor UID.
    pub acpi_processor_id_chirho: u8,
    /// Local APIC ID.
    pub apic_id_chirho: u8,
    /// Flags (bit 0 = enabled, bit 1 = online capable).
    pub flags_chirho: u32,
}

/// MADT entry: I/O APIC (type 1).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtIoApicChirho {
    pub header_chirho: MadtEntryHeaderChirho,
    /// I/O APIC ID.
    pub io_apic_id_chirho: u8,
    /// Reserved.
    pub reserved_chirho: u8,
    /// I/O APIC physical address.
    pub io_apic_address_chirho: u32,
    /// Global System Interrupt base.
    pub gsi_base_chirho: u32,
}

/// MADT entry: Interrupt Source Override (type 2).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtInterruptOverrideChirho {
    pub header_chirho: MadtEntryHeaderChirho,
    /// Bus (0 = ISA).
    pub bus_source_chirho: u8,
    /// IRQ source.
    pub irq_source_chirho: u8,
    /// Global System Interrupt this IRQ maps to.
    pub gsi_chirho: u32,
    /// Flags (polarity, trigger mode).
    pub flags_chirho: u16,
}

/// MADT entry: Local APIC NMI (type 4).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct MadtLocalApicNmiChirho {
    pub header_chirho: MadtEntryHeaderChirho,
    /// ACPI Processor UID (0xFF = all processors).
    pub acpi_processor_id_chirho: u8,
    /// Flags.
    pub flags_chirho: u16,
    /// Local APIC LINT# (0 or 1).
    pub lint_chirho: u8,
}

/// Results from parsing the MADT.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MadtInfoChirho {
    /// Physical base address of the Local APIC.
    pub local_apic_addr_chirho: u64,
    /// Number of processor Local APICs found.
    pub cpu_count_chirho: usize,
    /// Local APIC IDs of enabled processors.
    pub apic_ids_chirho: [u8; 256],
    /// Number of I/O APICs found.
    pub ioapic_count_chirho: usize,
    /// I/O APIC addresses.
    pub ioapic_addrs_chirho: [u32; 8],
    /// I/O APIC IDs.
    pub ioapic_ids_chirho: [u8; 8],
    /// I/O APIC GSI bases.
    pub ioapic_gsi_bases_chirho: [u32; 8],
}

impl MadtInfoChirho {
    /// Create an empty MADT info.
    #[allow(dead_code)]
    fn new_chirho() -> Self {
        Self {
            local_apic_addr_chirho: 0,
            cpu_count_chirho: 0,
            apic_ids_chirho: [0; 256],
            ioapic_count_chirho: 0,
            ioapic_addrs_chirho: [0; 8],
            ioapic_ids_chirho: [0; 8],
            ioapic_gsi_bases_chirho: [0; 8],
        }
    }
}

/// Parse the MADT to discover Local APICs, I/O APICs, and overrides.
///
/// # Safety
/// `madt_ptr_chirho` must point to a valid, mapped MADT.
#[allow(dead_code)]
pub unsafe fn parse_madt_full_chirho(
    madt_ptr_chirho: *const AcpiTableHeaderChirho,
) -> MadtInfoChirho {
    let madt_chirho = unsafe { &*(madt_ptr_chirho as *const MadtHeaderChirho) };
    let mut info_chirho = MadtInfoChirho::new_chirho();
    let lapic_address_copy_chirho = { madt_chirho.local_apic_address_chirho };
    info_chirho.local_apic_addr_chirho = lapic_address_copy_chirho as u64;
    let madt_flags_copy_chirho = { madt_chirho.flags_chirho };
    crate::serial_println_chirho!(
        "ACPI MADT: Local APIC at {:#010x}, flags={:#x}",
        lapic_address_copy_chirho,
        madt_flags_copy_chirho
    );

    let total_len_chirho = madt_chirho.header_chirho.length_chirho as usize;
    let madt_base_chirho = madt_ptr_chirho as *const u8;
    let entries_start_chirho = core::mem::size_of::<MadtHeaderChirho>();
    let mut offset_chirho = entries_start_chirho;

    while offset_chirho + 2 <= total_len_chirho {
        let entry_hdr_chirho =
            unsafe { &*(madt_base_chirho.add(offset_chirho) as *const MadtEntryHeaderChirho) };
        let entry_len_chirho = entry_hdr_chirho.length_chirho as usize;
        if entry_len_chirho < 2 || offset_chirho + entry_len_chirho > total_len_chirho {
            break;
        }

        match entry_hdr_chirho.entry_type_chirho {
            0 => {
                // Local APIC
                let lapic_chirho =
                    unsafe { &*(madt_base_chirho.add(offset_chirho) as *const MadtLocalApicChirho) };
                let enabled_chirho = lapic_chirho.flags_chirho & 1 != 0;
                let online_capable_chirho = lapic_chirho.flags_chirho & 2 != 0;
                if enabled_chirho || online_capable_chirho {
                    if info_chirho.cpu_count_chirho < 256 {
                        info_chirho.apic_ids_chirho[info_chirho.cpu_count_chirho] =
                            lapic_chirho.apic_id_chirho;
                        info_chirho.cpu_count_chirho += 1;
                    }
                    crate::serial_println_chirho!(
                        "  MADT: CPU APIC_ID={} processor_uid={} enabled={}",
                        lapic_chirho.apic_id_chirho,
                        lapic_chirho.acpi_processor_id_chirho,
                        enabled_chirho
                    );
                }
            }
            1 => {
                // I/O APIC
                let ioapic_chirho =
                    unsafe { &*(madt_base_chirho.add(offset_chirho) as *const MadtIoApicChirho) };
                let ioapic_addr_copy_chirho = { ioapic_chirho.io_apic_address_chirho };
                let ioapic_gsi_copy_chirho = { ioapic_chirho.gsi_base_chirho };
                if info_chirho.ioapic_count_chirho < 8 {
                    let idx_chirho = info_chirho.ioapic_count_chirho;
                    info_chirho.ioapic_ids_chirho[idx_chirho] = ioapic_chirho.io_apic_id_chirho;
                    info_chirho.ioapic_addrs_chirho[idx_chirho] = ioapic_addr_copy_chirho;
                    info_chirho.ioapic_gsi_bases_chirho[idx_chirho] = ioapic_gsi_copy_chirho;
                    info_chirho.ioapic_count_chirho += 1;
                }
                crate::serial_println_chirho!(
                    "  MADT: IOAPIC id={} addr={:#010x} gsi_base={}",
                    ioapic_chirho.io_apic_id_chirho,
                    ioapic_addr_copy_chirho,
                    ioapic_gsi_copy_chirho
                );
            }
            2 => {
                // Interrupt Source Override
                let iso_chirho = unsafe {
                    &*(madt_base_chirho.add(offset_chirho)
                        as *const MadtInterruptOverrideChirho)
                };
                let iso_gsi_copy_chirho = { iso_chirho.gsi_chirho };
                let iso_flags_copy_chirho = { iso_chirho.flags_chirho };
                let iso_bus_copy_chirho = { iso_chirho.bus_source_chirho };
                let iso_irq_copy_chirho = { iso_chirho.irq_source_chirho };
                crate::serial_println_chirho!(
                    "  MADT: ISO bus={} irq={} -> gsi={} flags={:#06x}",
                    iso_bus_copy_chirho,
                    iso_irq_copy_chirho,
                    iso_gsi_copy_chirho,
                    iso_flags_copy_chirho
                );
            }
            4 => {
                // Local APIC NMI
                let nmi_chirho =
                    unsafe { &*(madt_base_chirho.add(offset_chirho) as *const MadtLocalApicNmiChirho) };
                crate::serial_println_chirho!(
                    "  MADT: LAPIC NMI processor={:#04x} lint={}",
                    nmi_chirho.acpi_processor_id_chirho,
                    nmi_chirho.lint_chirho
                );
            }
            5 => {
                // Local APIC Address Override (64-bit)
                let override_addr_chirho = unsafe {
                    ptr::read_unaligned(
                        madt_base_chirho.add(offset_chirho + 4) as *const u64,
                    )
                };
                info_chirho.local_apic_addr_chirho = override_addr_chirho;
                crate::serial_println_chirho!(
                    "  MADT: LAPIC addr override -> {:#018x}",
                    override_addr_chirho
                );
            }
            other_chirho => {
                crate::serial_println_chirho!(
                    "  MADT: unknown entry type={} len={}",
                    other_chirho,
                    entry_len_chirho
                );
            }
        }

        offset_chirho += entry_len_chirho;
    }

    crate::serial_println_chirho!(
        "ACPI MADT: {} CPUs, {} IOAPICs",
        info_chirho.cpu_count_chirho,
        info_chirho.ioapic_count_chirho
    );

    info_chirho
}

/// Backward-compatible stub that logs and returns.
#[allow(dead_code)]
pub fn parse_madt_chirho() {
    crate::serial_println_chirho!("[STUB] ACPI: MADT parsing — use parse_madt_full_chirho()");
}

/// A5-006: Global storage for parsed ACPI information.
///
/// Populated by [`init_acpi_chirho`] and read by other subsystems
/// (SMP startup, power management, timer init, etc.).
pub static ACPI_INFO_CHIRHO: spin::Mutex<AcpiInfoChirho> =
    spin::Mutex::new(AcpiInfoChirho::new_chirho());

/// Aggregated ACPI information parsed during boot.
#[derive(Debug)]
pub struct AcpiInfoChirho {
    /// Whether ACPI tables were successfully parsed.
    pub valid_chirho: bool,
    /// MADT info (CPU topology, IOAPIC addresses).
    pub madt_chirho: Option<MadtInfoChirho>,
    /// FADT PM1a control block address (for shutdown/reboot).
    pub pm1a_control_block_chirho: u32,
    /// FADT SCI interrupt number.
    pub sci_interrupt_chirho: u16,
    /// HPET base address (0 if not present).
    pub hpet_base_addr_chirho: u64,
    /// HPET number of timers.
    pub hpet_num_timers_chirho: u8,
    /// FADT reset register value / mechanism.
    pub fadt_flags_chirho: u32,
}

impl AcpiInfoChirho {
    /// Create an empty ACPI info structure.
    pub const fn new_chirho() -> Self {
        Self {
            valid_chirho: false,
            madt_chirho: None,
            pm1a_control_block_chirho: 0,
            sci_interrupt_chirho: 0,
            hpet_base_addr_chirho: 0,
            hpet_num_timers_chirho: 0,
            fadt_flags_chirho: 0,
        }
    }
}

// ============================================================================
// FADT — Fixed ACPI Description Table (signature "FACP")
// ============================================================================

/// Fixed ACPI Description Table.
///
/// Only the most critical fields are included; the full FADT is 276 bytes
/// in ACPI 6.x. Fields after `flags_chirho` are omitted for brevity.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct FadtChirho {
    pub header_chirho: AcpiTableHeaderChirho,
    /// Physical address of FACS (Firmware ACPI Control Structure).
    pub firmware_ctrl_chirho: u32,
    /// Physical address of DSDT.
    pub dsdt_chirho: u32,
    /// Reserved (ACPI 1.0 field).
    pub reserved1_chirho: u8,
    /// Preferred PM profile (0=unspec, 1=desktop, 2=mobile, ...).
    pub preferred_pm_profile_chirho: u8,
    /// SCI interrupt number.
    pub sci_interrupt_chirho: u16,
    /// SMI command port.
    pub smi_command_chirho: u32,
    /// Value to write to SMI_CMD to enable ACPI.
    pub acpi_enable_chirho: u8,
    /// Value to write to SMI_CMD to disable ACPI.
    pub acpi_disable_chirho: u8,
    /// Value to enter S4 BIOS state.
    pub s4bios_req_chirho: u8,
    /// Processor performance state control.
    pub pstate_control_chirho: u8,
    /// Port address of PM1a event block.
    pub pm1a_event_block_chirho: u32,
    /// Port address of PM1b event block (0 if not present).
    pub pm1b_event_block_chirho: u32,
    /// Port address of PM1a control block.
    pub pm1a_control_block_chirho: u32,
    /// Port address of PM1b control block (0 if not present).
    pub pm1b_control_block_chirho: u32,
    /// Port address of PM2 control block (0 if not present).
    pub pm2_control_block_chirho: u32,
    /// Port address of PM timer block.
    pub pm_timer_block_chirho: u32,
    /// Port address of GPE0 block.
    pub gpe0_block_chirho: u32,
    /// Port address of GPE1 block (0 if not present).
    pub gpe1_block_chirho: u32,
    /// Length of PM1 event registers (bytes).
    pub pm1_event_length_chirho: u8,
    /// Length of PM1 control registers.
    pub pm1_control_length_chirho: u8,
    /// Length of PM2 control register.
    pub pm2_control_length_chirho: u8,
    /// Length of PM timer register (4 bytes).
    pub pm_timer_length_chirho: u8,
    /// Length of GPE0 block (bytes).
    pub gpe0_block_length_chirho: u8,
    /// Length of GPE1 block (bytes).
    pub gpe1_block_length_chirho: u8,
    /// GPE1 base.
    pub gpe1_base_chirho: u8,
    /// C-state control.
    pub cstate_control_chirho: u8,
    /// Worst-case latency to enter/exit C2 state (microseconds).
    pub worst_c2_latency_chirho: u16,
    /// Worst-case latency to enter/exit C3 state (microseconds).
    pub worst_c3_latency_chirho: u16,
    /// Flush size (caches).
    pub flush_size_chirho: u16,
    /// Flush stride.
    pub flush_stride_chirho: u16,
    /// Duty cycle register bit offset.
    pub duty_offset_chirho: u8,
    /// Duty cycle register bit width.
    pub duty_width_chirho: u8,
    /// RTC CMOS day-alarm index.
    pub day_alarm_chirho: u8,
    /// RTC CMOS month-alarm index.
    pub month_alarm_chirho: u8,
    /// RTC CMOS century index.
    pub century_chirho: u8,
    /// IA-PC boot architecture flags (ACPI 2.0+).
    pub iapc_boot_arch_chirho: u16,
    /// Reserved.
    pub reserved2_chirho: u8,
    /// Fixed feature flags.
    pub flags_chirho: u32,
    // -- ACPI 2.0+ Generic Address Structures and 64-bit fields follow --
    // (omitted for kernel-space brevity; extend when needed)
}

/// Parse and log key fields from the FADT.
///
/// # Safety
/// `fadt_ptr_chirho` must point to a valid, mapped FADT.
#[allow(dead_code)]
pub unsafe fn parse_fadt_chirho(fadt_ptr_chirho: *const AcpiTableHeaderChirho) {
    let fadt_chirho = unsafe { &*(fadt_ptr_chirho as *const FadtChirho) };
    let sci_irq_copy_chirho = { fadt_chirho.sci_interrupt_chirho };
    let smi_cmd_copy_chirho = { fadt_chirho.smi_command_chirho };
    let pm_timer_copy_chirho = { fadt_chirho.pm_timer_block_chirho };
    let dsdt_copy_chirho = { fadt_chirho.dsdt_chirho };
    let fadt_flags_copy_chirho = { fadt_chirho.flags_chirho };
    crate::serial_println_chirho!("ACPI FADT:");
    crate::serial_println_chirho!("  SCI IRQ:     {}", sci_irq_copy_chirho);
    crate::serial_println_chirho!("  SMI CMD:     {:#010x}", smi_cmd_copy_chirho);
    crate::serial_println_chirho!("  PM timer:    {:#010x}", pm_timer_copy_chirho);
    crate::serial_println_chirho!("  PM profile:  {}", fadt_chirho.preferred_pm_profile_chirho);
    crate::serial_println_chirho!("  DSDT phys:   {:#010x}", dsdt_copy_chirho);
    crate::serial_println_chirho!("  Century reg: {}", fadt_chirho.century_chirho);
    crate::serial_println_chirho!("  Flags:       {:#010x}", fadt_flags_copy_chirho);
}

// ============================================================================
// HPET — High Precision Event Timer (signature "HPET")
// ============================================================================

/// ACPI Generic Address Structure (GAS) — 12 bytes.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct GenericAddressChirho {
    /// Address space ID (0=system memory, 1=system I/O).
    pub address_space_id_chirho: u8,
    /// Register bit width.
    pub register_bit_width_chirho: u8,
    /// Register bit offset.
    pub register_bit_offset_chirho: u8,
    /// Access size (0=undefined, 1=byte, 2=word, 3=dword, 4=qword).
    pub access_size_chirho: u8,
    /// Address (physical for memory, port for I/O).
    pub address_chirho: u64,
}

/// HPET description table.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct HpetTableChirho {
    pub header_chirho: AcpiTableHeaderChirho,
    /// Hardware revision ID.
    pub hardware_rev_id_chirho: u8,
    /// Number of comparators in the first timer block (bits 4:0),
    /// counter size (bit 5), legacy replacement IRQ capable (bit 6).
    pub comparator_count_chirho: u8,
    /// PCI vendor ID of the first timer block.
    pub pci_vendor_id_chirho: u16,
    /// Base address of the HPET registers.
    pub address_chirho: GenericAddressChirho,
    /// HPET sequence number.
    pub hpet_number_chirho: u8,
    /// Minimum tick count for periodic mode.
    pub minimum_tick_chirho: u16,
    /// Page protection attribute.
    pub page_protection_chirho: u8,
}

/// Parse and log the HPET table.
///
/// # Safety
/// `hpet_ptr_chirho` must point to a valid, mapped HPET table.
#[allow(dead_code)]
pub unsafe fn parse_hpet_chirho(hpet_ptr_chirho: *const AcpiTableHeaderChirho) {
    let hpet_chirho = unsafe { &*(hpet_ptr_chirho as *const HpetTableChirho) };
    let num_timers_chirho = (hpet_chirho.comparator_count_chirho & 0x1F) + 1;
    let counter_64_chirho = (hpet_chirho.comparator_count_chirho >> 5) & 1 != 0;

    let hpet_base_addr_copy_chirho = { hpet_chirho.address_chirho.address_chirho };
    let hpet_min_tick_copy_chirho = { hpet_chirho.minimum_tick_chirho };
    crate::serial_println_chirho!("ACPI HPET:");
    crate::serial_println_chirho!(
        "  Base address: {:#018x} (space={})",
        hpet_base_addr_copy_chirho,
        hpet_chirho.address_chirho.address_space_id_chirho
    );
    crate::serial_println_chirho!("  Timers:       {}", num_timers_chirho);
    crate::serial_println_chirho!("  64-bit:       {}", counter_64_chirho);
    crate::serial_println_chirho!("  Min tick:     {}", hpet_min_tick_copy_chirho);
    crate::serial_println_chirho!("  HW rev:       {}", hpet_chirho.hardware_rev_id_chirho);
}

// ============================================================================
// Top-level ACPI init
// ============================================================================

/// Full ACPI initialization: find RSDP, parse RSDT/XSDT, enumerate tables.
///
/// # Safety
/// Requires physical memory to be mapped at `phys_offset_chirho`.
#[allow(dead_code)]
pub unsafe fn init_acpi_chirho(phys_offset_chirho: u64) {
    crate::serial_println_chirho!("ACPI: searching for RSDP...");

    let rsdp_chirho = match unsafe { find_rsdp_bios_chirho(phys_offset_chirho) } {
        Some(r_chirho) => r_chirho,
        None => {
            crate::serial_println_chirho!("ACPI: RSDP not found");
            return;
        }
    };

    let mut acpi_info_chirho = ACPI_INFO_CHIRHO.lock();

    // Prefer XSDT if available (64-bit pointers), fall back to RSDT.
    if rsdp_chirho.is_xsdt_available_chirho() {
        let xsdt_addr_copy_chirho = { rsdp_chirho.xsdt_address_chirho };
        crate::serial_println_chirho!(
            "ACPI: using XSDT at {:#018x}",
            xsdt_addr_copy_chirho
        );

        // Find and parse MADT
        if let Some(madt_ptr_chirho) = unsafe {
            find_table_xsdt_chirho(
                xsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"APIC",
            )
        } {
            let madt_info_chirho = unsafe { parse_madt_full_chirho(madt_ptr_chirho) };
            acpi_info_chirho.madt_chirho = Some(madt_info_chirho);
        }

        // Find and parse FADT
        if let Some(fadt_ptr_chirho) = unsafe {
            find_table_xsdt_chirho(
                xsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"FACP",
            )
        } {
            unsafe { parse_fadt_chirho(fadt_ptr_chirho) };
            let fadt_chirho = unsafe { &*(fadt_ptr_chirho as *const FadtChirho) };
            acpi_info_chirho.pm1a_control_block_chirho = { fadt_chirho.pm1a_control_block_chirho };
            acpi_info_chirho.sci_interrupt_chirho = { fadt_chirho.sci_interrupt_chirho };
            acpi_info_chirho.fadt_flags_chirho = { fadt_chirho.flags_chirho };
        }

        // Find and parse HPET
        if let Some(hpet_ptr_chirho) = unsafe {
            find_table_xsdt_chirho(
                xsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"HPET",
            )
        } {
            unsafe { parse_hpet_chirho(hpet_ptr_chirho) };
            let hpet_chirho = unsafe { &*(hpet_ptr_chirho as *const HpetTableChirho) };
            acpi_info_chirho.hpet_base_addr_chirho = { hpet_chirho.address_chirho.address_chirho };
            acpi_info_chirho.hpet_num_timers_chirho = (hpet_chirho.comparator_count_chirho & 0x1F) + 1;
        }
    } else {
        let rsdt_addr_copy_chirho = { rsdp_chirho.rsdt_address_chirho };
        crate::serial_println_chirho!(
            "ACPI: using RSDT at {:#010x}",
            rsdt_addr_copy_chirho
        );

        // Find and parse MADT
        if let Some(madt_ptr_chirho) = unsafe {
            find_table_rsdt_chirho(
                rsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"APIC",
            )
        } {
            let madt_info_chirho = unsafe { parse_madt_full_chirho(madt_ptr_chirho) };
            acpi_info_chirho.madt_chirho = Some(madt_info_chirho);
        }

        // Find and parse FADT
        if let Some(fadt_ptr_chirho) = unsafe {
            find_table_rsdt_chirho(
                rsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"FACP",
            )
        } {
            unsafe { parse_fadt_chirho(fadt_ptr_chirho) };
            let fadt_chirho = unsafe { &*(fadt_ptr_chirho as *const FadtChirho) };
            acpi_info_chirho.pm1a_control_block_chirho = { fadt_chirho.pm1a_control_block_chirho };
            acpi_info_chirho.sci_interrupt_chirho = { fadt_chirho.sci_interrupt_chirho };
            acpi_info_chirho.fadt_flags_chirho = { fadt_chirho.flags_chirho };
        }

        // Find and parse HPET
        if let Some(hpet_ptr_chirho) = unsafe {
            find_table_rsdt_chirho(
                rsdt_addr_copy_chirho,
                phys_offset_chirho,
                b"HPET",
            )
        } {
            unsafe { parse_hpet_chirho(hpet_ptr_chirho) };
            let hpet_chirho = unsafe { &*(hpet_ptr_chirho as *const HpetTableChirho) };
            acpi_info_chirho.hpet_base_addr_chirho = { hpet_chirho.address_chirho.address_chirho };
            acpi_info_chirho.hpet_num_timers_chirho = (hpet_chirho.comparator_count_chirho & 0x1F) + 1;
        }
    }

    acpi_info_chirho.valid_chirho = true;
    crate::serial_println_chirho!("ACPI: initialization complete, results stored globally");
}
