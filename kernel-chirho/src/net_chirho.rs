// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Network socket stubs for the Lineluya kernel (Phase 6).
//!
//! Provides stub implementations for the Linux socket API.  All functions
//! return minimal plausible values so that userspace libraries that probe
//! for socket support do not crash the kernel.

use crate::syscall_chirho::{
    EAGAIN_CHIRHO, ECONNREFUSED_CHIRHO, ENOSYS_CHIRHO, ENOTSOCK_CHIRHO,
};

// ============================================================================
// Address family constants
// ============================================================================

/// Address family: local / Unix domain sockets.
#[allow(dead_code)]
pub const AF_UNIX_CHIRHO: u64 = 1;
/// Address family: IPv4.
#[allow(dead_code)]
pub const AF_INET_CHIRHO: u64 = 2;
/// Address family: IPv6.
#[allow(dead_code)]
pub const AF_INET6_CHIRHO: u64 = 10;
/// Address family: Netlink (kernel/userspace messaging).
#[allow(dead_code)]
pub const AF_NETLINK_CHIRHO: u64 = 16;

// ============================================================================
// AddressFamilyChirho enum
// ============================================================================

/// Supported address families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum AddressFamilyChirho {
    /// Unix domain socket.
    AfUnixChirho = AF_UNIX_CHIRHO,
    /// IPv4.
    AfInetChirho = AF_INET_CHIRHO,
    /// IPv6.
    AfInet6Chirho = AF_INET6_CHIRHO,
    /// Netlink.
    AfNetlinkChirho = AF_NETLINK_CHIRHO,
}

impl AddressFamilyChirho {
    /// Try to convert from a raw u64 value.
    pub fn from_raw_chirho(raw_chirho: u64) -> Option<Self> {
        match raw_chirho {
            AF_UNIX_CHIRHO => Some(Self::AfUnixChirho),
            AF_INET_CHIRHO => Some(Self::AfInetChirho),
            AF_INET6_CHIRHO => Some(Self::AfInet6Chirho),
            AF_NETLINK_CHIRHO => Some(Self::AfNetlinkChirho),
            _ => None,
        }
    }
}

// ============================================================================
// SocketTypeChirho enum
// ============================================================================

/// Socket types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum SocketTypeChirho {
    /// Stream socket (TCP-like).
    SockStreamChirho = 1,
    /// Datagram socket (UDP-like).
    SockDgramChirho = 2,
    /// Raw socket.
    SockRawChirho = 3,
}

impl SocketTypeChirho {
    /// Try to convert from a raw u64 value (masking out SOCK_NONBLOCK/SOCK_CLOEXEC flags).
    pub fn from_raw_chirho(raw_chirho: u64) -> Option<Self> {
        // Linux defines SOCK_NONBLOCK = 0o4000, SOCK_CLOEXEC = 0o2000000.
        // Mask those out to get the base type.
        let base_chirho = raw_chirho & 0xF;
        match base_chirho {
            1 => Some(Self::SockStreamChirho),
            2 => Some(Self::SockDgramChirho),
            3 => Some(Self::SockRawChirho),
            _ => None,
        }
    }
}

// ============================================================================
// SocketStateChirho enum
// ============================================================================

/// Socket state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketStateChirho {
    /// Freshly created, not bound or connected.
    UnconnectedChirho,
    /// Bound to a local address.
    BoundChirho,
    /// Listening for incoming connections.
    ListeningChirho,
    /// Connected to a peer.
    ConnectedChirho,
    /// Closed.
    ClosedChirho,
}

// ============================================================================
// SocketChirho struct
// ============================================================================

/// Represents a single socket instance.
#[derive(Debug)]
pub struct SocketChirho {
    /// Address family.
    pub family_chirho: u64,
    /// Socket type (SOCK_STREAM, SOCK_DGRAM, SOCK_RAW).
    pub sock_type_chirho: u64,
    /// Protocol number (usually 0 = default for the family/type).
    pub protocol_chirho: u64,
    /// Current socket state.
    pub state_chirho: SocketStateChirho,
}

impl SocketChirho {
    /// Create a new socket with the given parameters.
    pub fn new_chirho(
        family_chirho: u64,
        sock_type_chirho: u64,
        protocol_chirho: u64,
    ) -> Self {
        Self {
            family_chirho,
            sock_type_chirho,
            protocol_chirho,
            state_chirho: SocketStateChirho::UnconnectedChirho,
        }
    }
}

// ============================================================================
// Fake fd counter for socket stubs
// ============================================================================

use core::sync::atomic::{AtomicU64, Ordering};

/// Next fake file descriptor for sockets (starts above typical fd range).
static NEXT_SOCK_FD_CHIRHO: AtomicU64 = AtomicU64::new(100);

// ============================================================================
// Syscall stub implementations
// ============================================================================

/// `socket(2)` stub -- returns a fake fd for recognised address families.
pub fn sys_socket_chirho(
    domain_chirho: u64,
    type_chirho: u64,
    protocol_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[NET] sys_socket(domain={}, type={}, proto={})",
        domain_chirho,
        type_chirho,
        protocol_chirho,
    );

    // Validate address family
    if AddressFamilyChirho::from_raw_chirho(domain_chirho).is_none() {
        return -ENOSYS_CHIRHO;
    }

    // Return a fake fd
    let fd_chirho = NEXT_SOCK_FD_CHIRHO.fetch_add(1, Ordering::Relaxed);
    fd_chirho as i64
}

/// `bind(2)` stub -- always succeeds.
pub fn sys_bind_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_bind(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `listen(2)` stub -- always succeeds.
pub fn sys_listen_chirho(_sockfd_chirho: u64, _backlog_chirho: u64) -> i64 {
    crate::serial_println_chirho!("[NET] sys_listen(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `accept(2)` stub -- returns -EAGAIN (no pending connections).
pub fn sys_accept_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_accept(fd={}) -> -EAGAIN (stub)", _sockfd_chirho);
    -EAGAIN_CHIRHO
}

/// `accept4(2)` stub -- returns -EAGAIN.
pub fn sys_accept4_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_accept4(fd={}) -> -EAGAIN (stub)", _sockfd_chirho);
    -EAGAIN_CHIRHO
}

/// `connect(2)` stub -- returns -ECONNREFUSED.
pub fn sys_connect_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_connect(fd={}) -> -ECONNREFUSED (stub)", _sockfd_chirho);
    -ECONNREFUSED_CHIRHO
}

/// `sendto(2)` stub -- pretends all bytes were sent.
pub fn sys_sendto_chirho(
    _sockfd_chirho: u64,
    _buf_chirho: u64,
    len_chirho: u64,
    _flags_chirho: u64,
    _dest_addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[NET] sys_sendto(fd={}, len={}) -> {} (stub)",
        _sockfd_chirho,
        len_chirho,
        len_chirho,
    );
    len_chirho as i64
}

/// `recvfrom(2)` stub -- returns 0 (no data available).
pub fn sys_recvfrom_chirho(
    _sockfd_chirho: u64,
    _buf_chirho: u64,
    _len_chirho: u64,
    _flags_chirho: u64,
    _src_addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_recvfrom(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `sendmsg(2)` stub -- returns 0 (pretend no data sent).
pub fn sys_sendmsg_chirho(
    _sockfd_chirho: u64,
    _msg_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_sendmsg(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `recvmsg(2)` stub -- returns 0 (no data).
pub fn sys_recvmsg_chirho(
    _sockfd_chirho: u64,
    _msg_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[NET] sys_recvmsg(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `setsockopt(2)` stub -- always succeeds.
pub fn sys_setsockopt_chirho(
    _sockfd_chirho: u64,
    _level_chirho: u64,
    _optname_chirho: u64,
    _optval_chirho: u64,
    _optlen_chirho: u64,
) -> i64 {
    0
}

/// `getsockopt(2)` stub -- always succeeds.
pub fn sys_getsockopt_chirho(
    _sockfd_chirho: u64,
    _level_chirho: u64,
    _optname_chirho: u64,
    _optval_chirho: u64,
    _optlen_chirho: u64,
) -> i64 {
    0
}

/// `shutdown(2)` stub -- always succeeds.
pub fn sys_shutdown_chirho(_sockfd_chirho: u64, _how_chirho: u64) -> i64 {
    crate::serial_println_chirho!("[NET] sys_shutdown(fd={}) -> 0 (stub)", _sockfd_chirho);
    0
}

/// `getsockname(2)` stub -- returns -ENOTSOCK.
pub fn sys_getsockname_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    -ENOTSOCK_CHIRHO
}

/// `getpeername(2)` stub -- returns -ENOTSOCK.
pub fn sys_getpeername_chirho(
    _sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    -ENOTSOCK_CHIRHO
}

/// `socketpair(2)` stub -- returns -ENOSYS.
pub fn sys_socketpair_chirho(
    _domain_chirho: u64,
    _type_chirho: u64,
    _protocol_chirho: u64,
    _sv_chirho: u64,
) -> i64 {
    -ENOSYS_CHIRHO
}
