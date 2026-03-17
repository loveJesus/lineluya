// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Network type definitions and constants for the Lineluya kernel (A2-AUDIT-006).
//!
//! Extracted from `net_core_chirho.rs` to keep the networking module manageable.
//! Contains:
//! - Ethertype, IP protocol, ARP, ICMP, and TCP flag constants
//! - Network tuning constants (MTU, poll limits, etc.)
//! - `Ipv4AddrChirho` / `PortChirho` newtypes (A2-AUDIT-007)
//! - `NetErrorChirho` — typed errors for the network subsystem
//! - `TcpStateChirho` — TCP connection state machine (RFC 793)
//! - `AddressFamilyChirho`, `SocketTypeChirho`, `SocketStateChirho` enums
//! - `SockAddrInChirho` — parsed IPv4 socket address

use crate::syscall_chirho::{
    EADDRINUSE_CHIRHO, EBADF_CHIRHO,
    ECONNREFUSED_CHIRHO, EINVAL_CHIRHO, EISCONN_CHIRHO,
    ENOTCONN_CHIRHO, EOPNOTSUPP_CHIRHO,
};

// ============================================================================
// Ipv4AddrChirho / PortChirho — typed newtypes (audit A2-AUDIT-007)
// ============================================================================

/// Typed IPv4 address newtype wrapping a `u32` in **network byte order**
/// (big-endian, same as `struct in_addr`).
///
/// Provides safe construction, display formatting, and named constants
/// for common addresses.  Raw `u32` interop is available via `.0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ipv4AddrChirho(pub u32);

impl Ipv4AddrChirho {
    /// 0.0.0.0 — INADDR_ANY / unspecified.
    pub const UNSPECIFIED_CHIRHO: Self = Self(0);
    /// 127.0.0.1 — loopback.
    pub const LOOPBACK_CHIRHO: Self = Self(0x7F000001);
    /// 255.255.255.255 — broadcast.
    pub const BROADCAST_CHIRHO: Self = Self(0xFFFFFFFF);

    /// Return the four octets in network byte order.
    pub fn octets_chirho(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }

    /// Construct from four octets.
    pub const fn from_octets_chirho(a_chirho: u8, b_chirho: u8, c_chirho: u8, d_chirho: u8) -> Self {
        Self(((a_chirho as u32) << 24)
            | ((b_chirho as u32) << 16)
            | ((c_chirho as u32) << 8)
            | (d_chirho as u32))
    }
}

impl core::fmt::Display for Ipv4AddrChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let o_chirho = self.octets_chirho();
        write!(f_chirho, "{}.{}.{}.{}", o_chirho[0], o_chirho[1], o_chirho[2], o_chirho[3])
    }
}

/// Typed TCP/UDP port newtype wrapping a `u16` in **host byte order**.
///
/// Prevents accidental mixing of port numbers with other `u16` values
/// (sequence numbers, window sizes, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortChirho(pub u16);

impl PortChirho {
    /// Ephemeral port range start (Linux default).
    pub const EPHEMERAL_START_CHIRHO: Self = Self(32768);
}

impl core::fmt::Display for PortChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f_chirho, "{}", self.0)
    }
}

// ============================================================================
// Ethertype constants
// ============================================================================

/// Ethertype for IPv4.
#[allow(dead_code)]
pub const ETHERTYPE_IPV4_CHIRHO: u16 = 0x0800;
/// Ethertype for ARP.
#[allow(dead_code)]
pub const ETHERTYPE_ARP_CHIRHO: u16 = 0x0806;
/// Ethertype for IPv6.
#[allow(dead_code)]
pub const ETHERTYPE_IPV6_CHIRHO: u16 = 0x86DD;

// ============================================================================
// IP protocol numbers
// ============================================================================

/// IP protocol number for ICMP.
#[allow(dead_code)]
pub const IP_PROTO_ICMP_CHIRHO: u8 = 1;
/// IP protocol number for TCP.
#[allow(dead_code)]
pub const IP_PROTO_TCP_CHIRHO: u8 = 6;
/// IP protocol number for UDP.
#[allow(dead_code)]
pub const IP_PROTO_UDP_CHIRHO: u8 = 17;

// ============================================================================
// ARP constants
// ============================================================================

/// ARP hardware type: Ethernet.
#[allow(dead_code)]
pub const ARP_HTYPE_ETHERNET_CHIRHO: u16 = 1;
/// ARP operation: request.
#[allow(dead_code)]
pub const ARP_OP_REQUEST_CHIRHO: u16 = 1;
/// ARP operation: reply.
#[allow(dead_code)]
pub const ARP_OP_REPLY_CHIRHO: u16 = 2;

// ============================================================================
// ICMP type constants
// ============================================================================

/// ICMP type: echo reply.
#[allow(dead_code)]
pub const ICMP_ECHO_REPLY_CHIRHO: u8 = 0;
/// ICMP type: echo request (ping).
#[allow(dead_code)]
pub const ICMP_ECHO_REQUEST_CHIRHO: u8 = 8;

// ============================================================================
// Network constants — centralized to avoid magic numbers (audit const-001)
// ============================================================================

/// Maximum Transmission Unit for standard Ethernet.
pub const ETHERNET_MTU_CHIRHO: usize = 1500;

/// Maximum Transmission Unit for loopback interface (matches Linux `lo`).
pub const LOOPBACK_MTU_CHIRHO: usize = 65536;

/// Google Public DNS server (8.8.8.8) as u32 in network byte order.
pub const DEFAULT_DNS_CHIRHO: u32 = 0x08080808;

/// Spin-loop iterations for DHCP / TCP-connect polling before timeout.
pub const NETWORK_POLL_MAX_CHIRHO: u32 = 5_000_000;

/// Spin-loop iterations for TCP recv / ARP / short polls.
pub const NETWORK_POLL_SHORT_CHIRHO: u32 = 500_000;

/// Maximum bytes copied from user-space in a single send/sendto call.
pub const SOCKET_SEND_MAX_CHIRHO: usize = 65536;

// ============================================================================
// TCP flag constants (A3-002)
// ============================================================================

/// TCP FIN flag.
pub const TCP_FIN_CHIRHO: u8 = 0x01;
/// TCP SYN flag.
pub const TCP_SYN_CHIRHO: u8 = 0x02;
/// TCP RST flag.
pub const TCP_RST_CHIRHO: u8 = 0x04;
/// TCP PSH flag.
pub const TCP_PSH_CHIRHO: u8 = 0x08;
/// TCP ACK flag.
pub const TCP_ACK_CHIRHO: u8 = 0x10;
/// TCP URG flag.
#[allow(dead_code)]
pub const TCP_URG_CHIRHO: u8 = 0x20;

/// Default TCP window size.
pub const TCP_DEFAULT_WINDOW_CHIRHO: u16 = 65535;
/// Default TCP MSS (Maximum Segment Size).
#[allow(dead_code)]
pub const TCP_DEFAULT_MSS_CHIRHO: u16 = 1460;

// ============================================================================
// NetErrorChirho — typed errors for the network subsystem (audit err-002)
// ============================================================================

/// Typed errors for the network subsystem.
/// Convert to Linux errno at syscall boundary only.
#[derive(Debug, Clone, Copy)]
pub enum NetErrorChirho {
    /// Socket not found or invalid fd.
    BadFdChirho,
    /// Address already in use.
    AddrInUseChirho,
    /// Connection refused by peer.
    ConnRefusedChirho,
    /// Network unreachable.
    NetUnreachChirho,
    /// Connection timed out.
    TimedOutChirho,
    /// Operation not supported on this socket type.
    OpNotSuppChirho,
    /// Socket not connected.
    NotConnectedChirho,
    /// Invalid argument.
    InvalidArgChirho,
    /// No buffer space available.
    NoBufsChirho,
}

impl NetErrorChirho {
    /// Convert to Linux errno value for syscall return.
    pub fn to_errno_chirho(self) -> i64 {
        match self {
            Self::BadFdChirho => -9,         // EBADF
            Self::AddrInUseChirho => -98,    // EADDRINUSE
            Self::ConnRefusedChirho => -111, // ECONNREFUSED
            Self::NetUnreachChirho => -101,  // ENETUNREACH
            Self::TimedOutChirho => -110,    // ETIMEDOUT
            Self::OpNotSuppChirho => -95,    // EOPNOTSUPP
            Self::NotConnectedChirho => -107, // ENOTCONN
            Self::InvalidArgChirho => -22,   // EINVAL
            Self::NoBufsChirho => -105,      // ENOBUFS
        }
    }
}

// ============================================================================
// TcpStateChirho — TCP connection state machine (A3-002, RFC 793)
// ============================================================================

/// TCP connection states per RFC 793.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpStateChirho {
    /// No connection — initial/final state.
    ClosedChirho,
    /// Waiting for a connection request (server).
    ListenChirho,
    /// SYN sent, waiting for SYN-ACK (client active open).
    SynSentChirho,
    /// SYN received, SYN-ACK sent, waiting for ACK (server).
    SynReceivedChirho,
    /// Connection established — data transfer.
    EstablishedChirho,
    /// FIN sent (active close initiated), waiting for ACK.
    FinWait1Chirho,
    /// FIN acknowledged, waiting for peer's FIN.
    FinWait2Chirho,
    /// Received FIN while established (passive close), waiting for app close.
    CloseWaitChirho,
    /// App closed after CloseWait, FIN sent, waiting for ACK.
    LastAckChirho,
    /// Both FINs exchanged, waiting for TIME_WAIT timeout.
    TimeWaitChirho,
    /// Simultaneous close: both sides sent FIN simultaneously.
    ClosingChirho,
}

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

/// High-level socket state machine (complements TCP state for the socket API).
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
// SockAddrInChirho — struct sockaddr_in for IPv4 (A3-003)
// ============================================================================

/// Parsed IPv4 socket address (equivalent to struct sockaddr_in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SockAddrInChirho {
    /// Port number (host byte order).
    pub port_chirho: u16,
    /// IPv4 address (network byte order, stored as u32).
    pub addr_chirho: u32,
}

impl SockAddrInChirho {
    /// Parse from user-space `struct sockaddr_in` layout (16 bytes).
    /// Layout: family(2) + port(2 big-endian) + addr(4 big-endian) + pad(8).
    pub fn from_user_bytes_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 8 {
            return None;
        }
        let port_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let addr_chirho = u32::from_be_bytes([
            data_chirho[4], data_chirho[5], data_chirho[6], data_chirho[7],
        ]);
        Some(Self { port_chirho, addr_chirho })
    }

    /// Write into user-space sockaddr_in layout (16 bytes).
    pub fn to_user_bytes_chirho(&self, family_chirho: u16) -> [u8; 16] {
        let mut buf_chirho = [0u8; 16];
        let fam_bytes_chirho = family_chirho.to_le_bytes();
        buf_chirho[0] = fam_bytes_chirho[0];
        buf_chirho[1] = fam_bytes_chirho[1];
        let port_bytes_chirho = self.port_chirho.to_be_bytes();
        buf_chirho[2] = port_bytes_chirho[0];
        buf_chirho[3] = port_bytes_chirho[1];
        let addr_bytes_chirho = self.addr_chirho.to_be_bytes();
        buf_chirho[4] = addr_bytes_chirho[0];
        buf_chirho[5] = addr_bytes_chirho[1];
        buf_chirho[6] = addr_bytes_chirho[2];
        buf_chirho[7] = addr_bytes_chirho[3];
        buf_chirho
    }
}
