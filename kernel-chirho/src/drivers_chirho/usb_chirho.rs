// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! USB XHCI controller and HID keyboard driver stubs for the Lineluya kernel
//! (A5-014 / A5-015).
//!
//! Provides:
//! - XHCI controller discovery via PCI
//! - Transfer Ring Buffer (TRB) definitions
//! - Device context structures
//! - HID keyboard report parsing
//! - Keycode-to-ASCII scancode conversion
//!
//! Reference: xHCI Specification 1.2, USB HID Usage Tables 1.12

extern crate alloc;

use alloc::vec::Vec;

// ============================================================================
// XHCI PCI class/subclass
// ============================================================================

/// USB controller class code.
#[allow(dead_code)]
const USB_CLASS_CHIRHO: u8 = 0x0C;
/// USB controller subclass.
#[allow(dead_code)]
const USB_SUBCLASS_CHIRHO: u8 = 0x03;
/// XHCI programming interface.
#[allow(dead_code)]
const XHCI_PROG_IF_CHIRHO: u8 = 0x30;

// ============================================================================
// XHCI register offsets (Capability Registers)
// ============================================================================

/// Capability Register Length.
#[allow(dead_code)]
const XHCI_CAPLENGTH_CHIRHO: u32 = 0x00;
/// Host Controller Interface Version.
#[allow(dead_code)]
const XHCI_HCIVERSION_CHIRHO: u32 = 0x02;
/// Structural Parameters 1.
#[allow(dead_code)]
const XHCI_HCSPARAMS1_CHIRHO: u32 = 0x04;
/// Structural Parameters 2.
#[allow(dead_code)]
const XHCI_HCSPARAMS2_CHIRHO: u32 = 0x08;
/// Structural Parameters 3.
#[allow(dead_code)]
const XHCI_HCSPARAMS3_CHIRHO: u32 = 0x0C;
/// Capability Parameters 1.
#[allow(dead_code)]
const XHCI_HCCPARAMS1_CHIRHO: u32 = 0x10;
/// Doorbell Offset.
#[allow(dead_code)]
const XHCI_DBOFF_CHIRHO: u32 = 0x14;
/// Runtime Register Space Offset.
#[allow(dead_code)]
const XHCI_RTSOFF_CHIRHO: u32 = 0x18;

// ============================================================================
// XHCI Operational Register offsets (from cap_length)
// ============================================================================

/// USB Command Register.
#[allow(dead_code)]
const XHCI_USBCMD_CHIRHO: u32 = 0x00;
/// USB Status Register.
#[allow(dead_code)]
const XHCI_USBSTS_CHIRHO: u32 = 0x04;
/// Device Notification Control.
#[allow(dead_code)]
const XHCI_DNCTRL_CHIRHO: u32 = 0x14;
/// Command Ring Control.
#[allow(dead_code)]
const XHCI_CRCR_CHIRHO: u32 = 0x18;
/// Device Context Base Address Array Pointer.
#[allow(dead_code)]
const XHCI_DCBAAP_CHIRHO: u32 = 0x30;
/// Configure Register.
#[allow(dead_code)]
const XHCI_CONFIG_CHIRHO: u32 = 0x38;

// ============================================================================
// TRB (Transfer Request Block) — 16 bytes
// ============================================================================

/// Generic TRB structure (16 bytes, 4-byte aligned).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct TrbChirho {
    /// Parameter (meaning depends on TRB type).
    pub parameter_chirho: u64,
    /// Status / Transfer length.
    pub status_chirho: u32,
    /// Control (TRB type in bits 15:10, cycle bit in bit 0).
    pub control_chirho: u32,
}

/// TRB type values (bits 15:10 of control field).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TrbTypeChirho {
    NormalChirho = 1,
    SetupStageChirho = 2,
    DataStageChirho = 3,
    StatusStageChirho = 4,
    LinkChirho = 6,
    NoOpChirho = 8,
    EnableSlotChirho = 9,
    DisableSlotChirho = 10,
    AddressDeviceChirho = 11,
    ConfigureEndpointChirho = 12,
    TransferEventChirho = 32,
    CommandCompletionChirho = 33,
    PortStatusChangeChirho = 34,
}

// ============================================================================
// USB HID keyboard report
// ============================================================================

/// Standard 8-byte USB HID keyboard boot protocol report.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct HidKeyboardReportChirho {
    /// Modifier keys (Ctrl, Shift, Alt, GUI).
    pub modifiers_chirho: u8,
    /// Reserved byte.
    pub reserved_chirho: u8,
    /// Currently pressed keycodes (up to 6 simultaneous).
    pub keycodes_chirho: [u8; 6],
}

/// Modifier key bits.
#[allow(dead_code)]
pub const HID_MOD_LEFT_CTRL_CHIRHO: u8 = 1 << 0;
#[allow(dead_code)]
pub const HID_MOD_LEFT_SHIFT_CHIRHO: u8 = 1 << 1;
#[allow(dead_code)]
pub const HID_MOD_LEFT_ALT_CHIRHO: u8 = 1 << 2;
#[allow(dead_code)]
pub const HID_MOD_LEFT_GUI_CHIRHO: u8 = 1 << 3;
#[allow(dead_code)]
pub const HID_MOD_RIGHT_CTRL_CHIRHO: u8 = 1 << 4;
#[allow(dead_code)]
pub const HID_MOD_RIGHT_SHIFT_CHIRHO: u8 = 1 << 5;
#[allow(dead_code)]
pub const HID_MOD_RIGHT_ALT_CHIRHO: u8 = 1 << 6;
#[allow(dead_code)]
pub const HID_MOD_RIGHT_GUI_CHIRHO: u8 = 1 << 7;

/// Convert a USB HID keycode to an ASCII character (US layout, lowercase).
///
/// Returns `None` for non-printable keys.
#[allow(dead_code)]
pub fn hid_keycode_to_ascii_chirho(keycode_chirho: u8, shift_chirho: bool) -> Option<u8> {
    // USB HID Usage Table: Keyboard/Keypad page (0x07)
    let ascii_chirho = match keycode_chirho {
        0x04..=0x1D => {
            // a-z (0x04 = 'a', 0x1D = 'z')
            let base_chirho = b'a' + (keycode_chirho - 0x04);
            if shift_chirho { base_chirho - 32 } else { base_chirho }
        }
        0x1E..=0x26 => {
            // 1-9
            let digit_chirho = b'1' + (keycode_chirho - 0x1E);
            if shift_chirho {
                match keycode_chirho - 0x1E {
                    0 => b'!',
                    1 => b'@',
                    2 => b'#',
                    3 => b'$',
                    4 => b'%',
                    5 => b'^',
                    6 => b'&',
                    7 => b'*',
                    8 => b'(',
                    _ => digit_chirho,
                }
            } else {
                digit_chirho
            }
        }
        0x27 => if shift_chirho { b')' } else { b'0' },
        0x28 => b'\n',  // Enter
        0x29 => 0x1B,   // Escape
        0x2A => 0x08,   // Backspace
        0x2B => b'\t',  // Tab
        0x2C => b' ',   // Space
        0x2D => if shift_chirho { b'_' } else { b'-' },
        0x2E => if shift_chirho { b'+' } else { b'=' },
        0x2F => if shift_chirho { b'{' } else { b'[' },
        0x30 => if shift_chirho { b'}' } else { b']' },
        0x31 => if shift_chirho { b'|' } else { b'\\' },
        0x33 => if shift_chirho { b':' } else { b';' },
        0x34 => if shift_chirho { b'"' } else { b'\'' },
        0x35 => if shift_chirho { b'~' } else { b'`' },
        0x36 => if shift_chirho { b'<' } else { b',' },
        0x37 => if shift_chirho { b'>' } else { b'.' },
        0x38 => if shift_chirho { b'?' } else { b'/' },
        _ => return None,
    };
    Some(ascii_chirho)
}

// ============================================================================
// XHCI Controller
// ============================================================================

/// XHCI controller state.
#[allow(dead_code)]
pub struct XhciControllerChirho {
    /// Virtual BAR0 base address.
    pub bar0_virt_chirho: u64,
    /// Capability register length (offset to operational regs).
    pub cap_length_chirho: u8,
    /// Max device slots.
    pub max_slots_chirho: u8,
    /// Max interrupters.
    pub max_intrs_chirho: u16,
    /// Max ports.
    pub max_ports_chirho: u8,
    /// PCI bus/device/function.
    pub pci_bdf_chirho: (u8, u8, u8),
}

impl XhciControllerChirho {
    /// Create a new XHCI controller descriptor.
    #[allow(dead_code)]
    pub fn new_chirho(
        bus_chirho: u8,
        dev_chirho: u8,
        func_chirho: u8,
        bar0_virt_chirho: u64,
    ) -> Self {
        Self {
            bar0_virt_chirho,
            cap_length_chirho: 0,
            max_slots_chirho: 0,
            max_intrs_chirho: 0,
            max_ports_chirho: 0,
            pci_bdf_chirho: (bus_chirho, dev_chirho, func_chirho),
        }
    }

    /// Read capability registers and populate controller info.
    ///
    /// # Safety
    /// BAR0 must be mapped.
    #[allow(dead_code)]
    pub unsafe fn read_caps_chirho(&mut self) {
        let cap_chirho = unsafe {
            core::ptr::read_volatile(
                (self.bar0_virt_chirho + XHCI_CAPLENGTH_CHIRHO as u64) as *const u32,
            )
        };
        self.cap_length_chirho = (cap_chirho & 0xFF) as u8;

        let hcsparams1_chirho = unsafe {
            core::ptr::read_volatile(
                (self.bar0_virt_chirho + XHCI_HCSPARAMS1_CHIRHO as u64) as *const u32,
            )
        };
        self.max_slots_chirho = (hcsparams1_chirho & 0xFF) as u8;
        self.max_intrs_chirho = ((hcsparams1_chirho >> 8) & 0x7FF) as u16;
        self.max_ports_chirho = ((hcsparams1_chirho >> 24) & 0xFF) as u8;
    }
}

// ============================================================================
// Discovery
// ============================================================================

/// Scan PCI bus for XHCI USB controllers.
#[allow(dead_code)]
pub fn discover_xhci_chirho(phys_offset_chirho: u64) -> Vec<XhciControllerChirho> {
    let mut controllers_chirho = Vec::new();

    let devices_chirho = unsafe { crate::pci_chirho::scan_bus_chirho(0) };
    for dev_chirho in &devices_chirho {
        if dev_chirho.class_code_chirho == USB_CLASS_CHIRHO
            && dev_chirho.subclass_chirho == USB_SUBCLASS_CHIRHO
            && dev_chirho.prog_if_chirho == XHCI_PROG_IF_CHIRHO
        {
            if let Some(bar_chirho) = unsafe { dev_chirho.read_bar_chirho(0) } {
                let bar0_virt_chirho = phys_offset_chirho + bar_chirho.base_address_chirho;
                let ctrl_chirho = XhciControllerChirho::new_chirho(
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                    bar0_virt_chirho,
                );
                crate::serial_debug_chirho!(
                    "[USB] Found XHCI controller at PCI {:02x}:{:02x}.{}",
                    dev_chirho.bus_chirho,
                    dev_chirho.device_chirho,
                    dev_chirho.function_chirho,
                );
                controllers_chirho.push(ctrl_chirho);
            }
        }
    }

    if controllers_chirho.is_empty() {
        crate::serial_debug_chirho!("[USB] No XHCI controllers found");
    }

    controllers_chirho
}
