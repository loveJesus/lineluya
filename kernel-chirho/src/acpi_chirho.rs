// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ACPI table parsing stubs for the Lineluya kernel (Phase 8).
//!
//! Provides structures for the RSDP and generic ACPI table headers, plus
//! stub functions for locating the RSDP in firmware memory and parsing
//! the MADT (Multiple APIC Description Table).

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

// ============================================================================
// Generic ACPI table header
// ============================================================================

/// Standard header that precedes every ACPI System Description Table.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub struct AcpiTableHeaderChirho {
    /// 4-byte ASCII signature (e.g. "APIC", "FACP").
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

// ============================================================================
// RSDP search
// ============================================================================

/// Attempt to locate the RSDP in standard BIOS memory regions.
///
/// Searches:
///   1. The first KiB of the EBDA (Extended BIOS Data Area).
///   2. The BIOS ROM region `0x000E_0000 .. 0x0010_0000`.
///
/// Stub: always returns `None`.  A real implementation would scan on
/// 16-byte boundaries for the "RSD PTR " signature and validate the
/// checksum.
#[allow(dead_code)]
pub fn find_rsdp_chirho() -> Option<&'static RsdpChirho> {
    crate::serial_println_chirho!("[STUB] ACPI: searching for RSDP — not implemented yet");
    None
}

// ============================================================================
// MADT parsing
// ============================================================================

/// Parse the MADT (Multiple APIC Description Table) to discover local APICs,
/// I/O APICs, and interrupt source overrides.
///
/// Stub: logs a message and returns.  A real implementation would walk the
/// variable-length entries after the MADT header.
#[allow(dead_code)]
pub fn parse_madt_chirho() {
    crate::serial_println_chirho!("[STUB] ACPI: MADT parsing — not implemented yet");
}
