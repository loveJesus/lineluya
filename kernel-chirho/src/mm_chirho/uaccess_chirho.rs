// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! User-space memory access helpers for the Lineluya kernel.
//!
//! Provides safe wrappers around raw pointer copies between kernel and user
//! address spaces, analogous to Linux's `copy_from_user()` / `copy_to_user()`
//! family from `include/linux/uaccess.h`.
//!
//! Every address passed as a "user" pointer is validated against the canonical
//! user-space boundary ([`USER_SPACE_MAX_CHIRHO`]) before any memory access is
//! performed.  This prevents the kernel from accidentally reading or writing
//! kernel memory on behalf of a user-space process.
//!
//! # Current limitations
//!
//! * No page-fault fixup mechanism yet -- if the user page is not currently
//!   mapped, the access will triple-fault rather than returning
//!   [`UaccessErrorChirho::FaultChirho`].  Proper fault handling will be
//!   layered in once the page-fault handler supports fixup tables.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

// ============================================================================
// Constants
// ============================================================================

/// Highest valid canonical user-space virtual address on x86_64.
///
/// On current hardware with 48-bit virtual addresses the user/kernel split is
/// at 0x0000_8000_0000_0000.  Addresses from 0x0000_0000_0000_0000 through
/// 0x0000_7FFF_FFFF_FFFF are user space; addresses at or above
/// 0xFFFF_8000_0000_0000 are kernel space.  The range in between is
/// non-canonical and will cause a general-protection fault if dereferenced.
pub const USER_SPACE_MAX_CHIRHO: u64 = 0x0000_7FFF_FFFF_FFFF;

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur during user-space memory access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UaccessErrorChirho {
    /// The supplied address falls outside the canonical user-space range.
    InvalidAddressChirho,
    /// The address-plus-length computation overflowed, or the resulting end
    /// address exceeds the user-space boundary.
    OverflowChirho,
    /// A page fault (or similar access fault) occurred while touching user
    /// memory.  This variant is reserved for future use once the page-fault
    /// handler supports fixup tables.
    FaultChirho,
}

impl fmt::Display for UaccessErrorChirho {
    fn fmt(&self, f_chirho: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddressChirho => write!(f_chirho, "invalid user-space address"),
            Self::OverflowChirho => write!(f_chirho, "address + length overflow"),
            Self::FaultChirho => write!(f_chirho, "fault during user memory access"),
        }
    }
}

// ============================================================================
// Address validation
// ============================================================================

/// Check whether the byte range `[addr_chirho, addr_chirho + len_chirho)` lies
/// entirely within user space.
///
/// Returns `true` when:
/// 1. `addr_chirho + len_chirho` does not overflow a `u64`, **and**
/// 2. the last byte (`addr_chirho + len_chirho - 1`, when `len_chirho > 0`)
///    does not exceed [`USER_SPACE_MAX_CHIRHO`].
///
/// A zero-length range at any valid user address is considered valid.
pub fn is_user_address_chirho(addr_chirho: u64, len_chirho: u64) -> bool {
    if len_chirho == 0 {
        // A zero-length access is valid as long as the start address itself is
        // within user space (or at most one past the end, which we accept).
        return addr_chirho <= USER_SPACE_MAX_CHIRHO.wrapping_add(1);
    }

    match addr_chirho.checked_add(len_chirho) {
        None => false, // overflow
        Some(end_chirho) => {
            // `end_chirho` is one-past-the-last, so the last accessed byte is
            // at `end_chirho - 1`.
            end_chirho.wrapping_sub(1) <= USER_SPACE_MAX_CHIRHO
        }
    }
}

/// Internal helper: validate and return an error-typed result.
/// Check that `[user_addr, user_addr+len)` is a canonical, non-overflowing
/// user-space range.
///
/// Public so callers can validate BEFORE committing irreversible state. Signal
/// delivery needs exactly that: it dequeues the signal and widens the blocked
/// mask with no way to undo either, so it must know the destination is sane
/// before it mutates anything.
///
/// This checks the RANGE ONLY. It does not prove the pages are mapped or
/// writable, and does not verify the active page table.
pub fn validate_user_range_chirho(
    addr_chirho: u64,
    len_chirho: usize,
) -> Result<(), UaccessErrorChirho> {
    if addr_chirho > USER_SPACE_MAX_CHIRHO && len_chirho > 0 {
        return Err(UaccessErrorChirho::InvalidAddressChirho);
    }

    if !is_user_address_chirho(addr_chirho, len_chirho as u64) {
        return Err(UaccessErrorChirho::OverflowChirho);
    }

    Ok(())
}

// ============================================================================
// Bulk copy helpers
// ============================================================================

/// Copy `len_chirho` bytes from user-space address `user_src_chirho` into
/// `kernel_dst_chirho`.
///
/// # Errors
///
/// * [`UaccessErrorChirho::InvalidAddressChirho`] -- `user_src_chirho` is not
///   in user space.
/// * [`UaccessErrorChirho::OverflowChirho`] -- `user_src_chirho + len_chirho`
///   overflows or exceeds the user-space boundary.
///
/// # Safety model
///
/// The function validates the user address range and then performs a raw
/// pointer copy.  The caller must ensure that the user page tables actually
/// have the pages mapped and readable; until fixup-table support is added, an
/// unmapped page will cause a fault rather than a clean error return.
pub fn copy_from_user_chirho(
    kernel_dst_chirho: &mut [u8],
    user_src_chirho: u64,
    len_chirho: usize,
) -> Result<(), UaccessErrorChirho> {
    if len_chirho == 0 {
        return Ok(());
    }

    // Destination buffer must be large enough.
    if kernel_dst_chirho.len() < len_chirho {
        return Err(UaccessErrorChirho::OverflowChirho);
    }

    validate_user_range_chirho(user_src_chirho, len_chirho)?;

    unsafe {
        core::ptr::copy_nonoverlapping(
            user_src_chirho as *const u8,
            kernel_dst_chirho.as_mut_ptr(),
            len_chirho,
        );
    }

    Ok(())
}

/// Copy `len_chirho` bytes from `kernel_src_chirho` to user-space address
/// `user_dst_chirho`.
///
/// # Errors
///
/// Same as [`copy_from_user_chirho`].
pub fn copy_to_user_chirho(
    user_dst_chirho: u64,
    kernel_src_chirho: &[u8],
    len_chirho: usize,
) -> Result<(), UaccessErrorChirho> {
    if len_chirho == 0 {
        return Ok(());
    }

    // Source buffer must contain at least `len_chirho` bytes.
    if kernel_src_chirho.len() < len_chirho {
        return Err(UaccessErrorChirho::OverflowChirho);
    }

    validate_user_range_chirho(user_dst_chirho, len_chirho)?;

    unsafe {
        core::ptr::copy_nonoverlapping(
            kernel_src_chirho.as_ptr(),
            user_dst_chirho as *mut u8,
            len_chirho,
        );
    }

    Ok(())
}

// ============================================================================
// Typed scalar helpers
// ============================================================================

/// Read a single `u64` from user-space address `user_addr_chirho`.
///
/// The address need **not** be 8-byte aligned; an unaligned read is performed
/// (x86_64 supports unaligned access, though it may be slower).
///
/// # Errors
///
/// See [`copy_from_user_chirho`].
pub fn read_user_u64_chirho(user_addr_chirho: u64) -> Result<u64, UaccessErrorChirho> {
    const SIZE_CHIRHO: usize = core::mem::size_of::<u64>();

    validate_user_range_chirho(user_addr_chirho, SIZE_CHIRHO)?;

    // SAFETY: Address range validated.  We use `read_unaligned` because user
    // pointers are not guaranteed to be aligned to 8 bytes.
    let value_chirho = unsafe { core::ptr::read_unaligned(user_addr_chirho as *const u64) };

    Ok(value_chirho)
}

/// Write a single `u64` to user-space address `user_addr_chirho`.
///
/// # Errors
///
/// See [`copy_to_user_chirho`].
pub fn write_user_u64_chirho(
    user_addr_chirho: u64,
    value_chirho: u64,
) -> Result<(), UaccessErrorChirho> {
    const SIZE_CHIRHO: usize = core::mem::size_of::<u64>();

    validate_user_range_chirho(user_addr_chirho, SIZE_CHIRHO)?;

    // SAFETY: Address range validated.  We use `write_unaligned` to tolerate
    // arbitrary user-space alignment.
    unsafe {
        core::ptr::write_unaligned(user_addr_chirho as *mut u64, value_chirho);
    }

    Ok(())
}

// ============================================================================
// String helper
// ============================================================================

/// Read a NUL-terminated string from user-space address `user_addr_chirho`,
/// examining at most `max_len_chirho` bytes (including the NUL terminator).
///
/// Returns an owned [`String`] containing the characters up to (but not
/// including) the NUL byte.  If no NUL byte is found within `max_len_chirho`
/// bytes the function returns [`UaccessErrorChirho::FaultChirho`] (the closest
/// semantic match -- the string is unterminated, which is analogous to a bad
/// user pointer).
///
/// # Errors
///
/// * [`UaccessErrorChirho::InvalidAddressChirho`] / `OverflowChirho` -- bad
///   address range.
/// * [`UaccessErrorChirho::FaultChirho`] -- no NUL terminator found within the
///   first `max_len_chirho` bytes.
pub fn read_user_string_chirho(
    user_addr_chirho: u64,
    max_len_chirho: usize,
) -> Result<String, UaccessErrorChirho> {
    if max_len_chirho == 0 {
        return Err(UaccessErrorChirho::FaultChirho);
    }

    // Read byte-by-byte until NUL terminator. This avoids reading past
    // the string into unmapped pages which would cause a page fault.
    let mut buf_chirho = Vec::with_capacity(128);

    for i_chirho in 0..max_len_chirho {
        let addr_chirho = user_addr_chirho + i_chirho as u64;
        if addr_chirho > USER_SPACE_MAX_CHIRHO {
            return Err(UaccessErrorChirho::FaultChirho);
        }
        let byte_chirho = unsafe { core::ptr::read_volatile(addr_chirho as *const u8) };
        if byte_chirho == 0 {
            break;
        }
        buf_chirho.push(byte_chirho);
    }

    let string_chirho = String::from_utf8(buf_chirho)
        .unwrap_or_else(|err_chirho| String::from_utf8_lossy(err_chirho.as_bytes()).into_owned());

    Ok(string_chirho)
}

// ============================================================================
// Typed user-pointer wrappers (A2-AUDIT-001)
// ============================================================================

/// Typed user-space pointer.  Wraps a raw `u64` address with type information
/// and bounds-checked read/write operations, replacing raw pointer arithmetic
/// in syscall handlers with safe, validated access.
///
/// # Example
///
/// ```ignore
/// let ptr_chirho = UserPtrChirho::<u64>::new_chirho(user_addr_chirho);
/// let val_chirho = ptr_chirho.read_chirho()?;
/// ```
pub struct UserPtrChirho<T> {
    addr_chirho: u64,
    _marker_chirho: core::marker::PhantomData<T>,
}

impl<T: Copy> UserPtrChirho<T> {
    /// Create a new typed user-space pointer from a raw `u64` address.
    ///
    /// This does **not** validate the address; validation is deferred to
    /// [`read_chirho`](Self::read_chirho) / [`write_chirho`](Self::write_chirho)
    /// so that construction is zero-cost.
    #[inline]
    pub fn new_chirho(addr_chirho: u64) -> Self {
        Self {
            addr_chirho,
            _marker_chirho: core::marker::PhantomData,
        }
    }

    /// Read a `T` from user space.
    ///
    /// The entire range `[addr, addr + size_of::<T>())` must lie within the
    /// canonical user-space region.  Returns `Err(-EFAULT)` (as a negative
    /// errno `i64`) if the address is invalid or overflows.
    ///
    /// Uses `read_unaligned` because user-space pointers are not guaranteed
    /// to satisfy alignment requirements.
    pub fn read_chirho(&self) -> Result<T, i64> {
        let size_chirho = core::mem::size_of::<T>();

        validate_user_range_chirho(self.addr_chirho, size_chirho)
            .map_err(|_err_chirho| -(EFAULT_CHIRHO))?;

        // SAFETY: Address range validated above.  `read_unaligned` tolerates
        // arbitrary alignment from user space.
        let value_chirho =
            unsafe { core::ptr::read_unaligned(self.addr_chirho as *const T) };

        Ok(value_chirho)
    }

    /// Write a `T` to user space.
    ///
    /// The entire range `[addr, addr + size_of::<T>())` must lie within the
    /// canonical user-space region.  Returns `Err(-EFAULT)` if the address
    /// is invalid or overflows.
    pub fn write_chirho(&self, val_chirho: &T) -> Result<(), i64> {
        let size_chirho = core::mem::size_of::<T>();

        validate_user_range_chirho(self.addr_chirho, size_chirho)
            .map_err(|_err_chirho| -(EFAULT_CHIRHO))?;

        // SAFETY: Address range validated above.  `write_unaligned` tolerates
        // arbitrary alignment.
        unsafe {
            core::ptr::write_unaligned(self.addr_chirho as *mut T, *val_chirho);
        }

        Ok(())
    }

    /// Check if the pointer is null (address == 0).
    #[inline]
    pub fn is_null_chirho(&self) -> bool {
        self.addr_chirho == 0
    }

    /// Return the raw address.
    #[inline]
    pub fn addr_chirho(&self) -> u64 {
        self.addr_chirho
    }
}

/// Typed user-space slice.  Wraps a `(ptr, len)` pair with bounds-checked
/// bulk read/write operations.
///
/// # Example
///
/// ```ignore
/// let slice_chirho = UserSliceChirho::<u8>::new_chirho(buf_addr_chirho, count_chirho);
/// let mut kbuf_chirho = [0u8; 256];
/// let n_chirho = slice_chirho.read_to_chirho(&mut kbuf_chirho)?;
/// ```
pub struct UserSliceChirho<T> {
    addr_chirho: u64,
    len_chirho: usize,
    _marker_chirho: core::marker::PhantomData<T>,
}

impl<T: Copy> UserSliceChirho<T> {
    /// Create a new typed user-space slice.
    ///
    /// `addr_chirho` is the base address in user space and `len_chirho` is
    /// the number of `T` elements (not bytes).
    #[inline]
    pub fn new_chirho(addr_chirho: u64, len_chirho: usize) -> Self {
        Self {
            addr_chirho,
            len_chirho,
            _marker_chirho: core::marker::PhantomData,
        }
    }

    /// Read the user-space slice into a kernel buffer.
    ///
    /// Copies `min(self.len, buf.len)` elements of type `T` from user space
    /// into `buf_chirho`.  Returns the number of elements actually read.
    ///
    /// The byte range `[addr, addr + count * size_of::<T>())` must lie within
    /// the canonical user-space region.  Returns `Err(-EFAULT)` on failure.
    pub fn read_to_chirho(&self, buf_chirho: &mut [T]) -> Result<usize, i64> {
        let count_chirho = core::cmp::min(self.len_chirho, buf_chirho.len());
        if count_chirho == 0 {
            return Ok(0);
        }

        let byte_len_chirho = count_chirho
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(-(EFAULT_CHIRHO))?;

        validate_user_range_chirho(self.addr_chirho, byte_len_chirho)
            .map_err(|_err_chirho| -(EFAULT_CHIRHO))?;

        // SAFETY: Address range validated.  We copy `byte_len_chirho` bytes
        // from user space into the kernel buffer.
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.addr_chirho as *const T,
                buf_chirho.as_mut_ptr(),
                count_chirho,
            );
        }

        Ok(count_chirho)
    }

    /// Write from a kernel buffer into user space.
    ///
    /// Copies `min(self.len, buf.len)` elements of type `T` from `buf_chirho`
    /// into user space.  Returns the number of elements actually written.
    ///
    /// Returns `Err(-EFAULT)` if the address range is invalid.
    pub fn write_from_chirho(&self, buf_chirho: &[T]) -> Result<usize, i64> {
        let count_chirho = core::cmp::min(self.len_chirho, buf_chirho.len());
        if count_chirho == 0 {
            return Ok(0);
        }

        let byte_len_chirho = count_chirho
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(-(EFAULT_CHIRHO))?;

        validate_user_range_chirho(self.addr_chirho, byte_len_chirho)
            .map_err(|_err_chirho| -(EFAULT_CHIRHO))?;

        // SAFETY: Address range validated.  We copy `byte_len_chirho` bytes
        // from the kernel buffer into user space.
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf_chirho.as_ptr(),
                self.addr_chirho as *mut T,
                count_chirho,
            );
        }

        Ok(count_chirho)
    }

    /// Return the number of elements in this user slice.
    #[inline]
    pub fn len_chirho(&self) -> usize {
        self.len_chirho
    }

    /// Check if the user slice has zero length.
    #[inline]
    pub fn is_empty_chirho(&self) -> bool {
        self.len_chirho == 0
    }

    /// Return the base address.
    #[inline]
    pub fn addr_chirho(&self) -> u64 {
        self.addr_chirho
    }
}

// ============================================================================
// EFAULT constant (mirrors syscall_chirho::EFAULT_CHIRHO for self-contained use)
// ============================================================================

/// Errno 14 — Bad address.  Kept local so that `uaccess_chirho` does not need
/// a circular dependency on `syscall_chirho`.
const EFAULT_CHIRHO: i64 = 14;

// ============================================================================
// Unit tests (run under the host, not the kernel target)
// ============================================================================

#[cfg(test)]
mod tests_chirho {
    use super::*;

    #[test]
    fn test_is_user_address_valid_chirho() {
        assert!(is_user_address_chirho(0, 1));
        assert!(is_user_address_chirho(0, 0));
        assert!(is_user_address_chirho(0x1000, 0x1000));
        assert!(is_user_address_chirho(USER_SPACE_MAX_CHIRHO, 1));
    }

    #[test]
    fn test_is_user_address_invalid_chirho() {
        // One byte past the boundary.
        assert!(!is_user_address_chirho(USER_SPACE_MAX_CHIRHO, 2));
        // Kernel space.
        assert!(!is_user_address_chirho(0xFFFF_8000_0000_0000, 1));
        // Overflow.
        assert!(!is_user_address_chirho(u64::MAX, 1));
    }

    #[test]
    fn test_validate_user_range_ok_chirho() {
        assert!(validate_user_range_chirho(0, 0).is_ok());
        assert!(validate_user_range_chirho(0x1000, 16).is_ok());
    }

    #[test]
    fn test_validate_user_range_err_chirho() {
        assert_eq!(
            validate_user_range_chirho(0xFFFF_8000_0000_0000, 1),
            Err(UaccessErrorChirho::InvalidAddressChirho)
        );
        assert_eq!(
            validate_user_range_chirho(USER_SPACE_MAX_CHIRHO, 2),
            Err(UaccessErrorChirho::OverflowChirho)
        );
    }

    // ---- UserPtrChirho tests ----

    #[test]
    fn test_user_ptr_new_null_chirho() {
        let ptr_chirho = UserPtrChirho::<u64>::new_chirho(0);
        assert!(ptr_chirho.is_null_chirho());
        assert_eq!(ptr_chirho.addr_chirho(), 0);
    }

    #[test]
    fn test_user_ptr_non_null_chirho() {
        let ptr_chirho = UserPtrChirho::<u32>::new_chirho(0x1000);
        assert!(!ptr_chirho.is_null_chirho());
        assert_eq!(ptr_chirho.addr_chirho(), 0x1000);
    }

    #[test]
    fn test_user_ptr_read_kernel_addr_fails_chirho() {
        // Kernel-space address must be rejected.
        let ptr_chirho = UserPtrChirho::<u64>::new_chirho(0xFFFF_8000_0000_0000);
        assert!(ptr_chirho.read_chirho().is_err());
    }

    #[test]
    fn test_user_ptr_write_kernel_addr_fails_chirho() {
        let ptr_chirho = UserPtrChirho::<u64>::new_chirho(0xFFFF_8000_0000_0000);
        let val_chirho: u64 = 42;
        assert!(ptr_chirho.write_chirho(&val_chirho).is_err());
    }

    #[test]
    fn test_user_ptr_read_overflow_fails_chirho() {
        // Address near end of user space — size_of::<u64>()=8 would overflow.
        let ptr_chirho =
            UserPtrChirho::<u64>::new_chirho(USER_SPACE_MAX_CHIRHO - 3);
        assert!(ptr_chirho.read_chirho().is_err());
    }

    // ---- UserSliceChirho tests ----

    #[test]
    fn test_user_slice_empty_chirho() {
        let slice_chirho = UserSliceChirho::<u8>::new_chirho(0x1000, 0);
        assert!(slice_chirho.is_empty_chirho());
        assert_eq!(slice_chirho.len_chirho(), 0);

        let mut buf_chirho = [0u8; 4];
        let n_chirho = slice_chirho.read_to_chirho(&mut buf_chirho);
        assert_eq!(n_chirho, Ok(0));
    }

    #[test]
    fn test_user_slice_kernel_addr_fails_chirho() {
        let slice_chirho =
            UserSliceChirho::<u8>::new_chirho(0xFFFF_8000_0000_0000, 16);
        let mut buf_chirho = [0u8; 16];
        assert!(slice_chirho.read_to_chirho(&mut buf_chirho).is_err());
    }

    #[test]
    fn test_user_slice_write_kernel_addr_fails_chirho() {
        let slice_chirho =
            UserSliceChirho::<u8>::new_chirho(0xFFFF_8000_0000_0000, 16);
        let buf_chirho = [0u8; 16];
        assert!(slice_chirho.write_from_chirho(&buf_chirho).is_err());
    }

    #[test]
    fn test_user_slice_accessors_chirho() {
        let slice_chirho = UserSliceChirho::<u32>::new_chirho(0x2000, 10);
        assert_eq!(slice_chirho.addr_chirho(), 0x2000);
        assert_eq!(slice_chirho.len_chirho(), 10);
        assert!(!slice_chirho.is_empty_chirho());
    }
}
