// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Network subsystem for the Lineluya kernel (Phase A3).
//!
//! Provides:
//! - `NetDeviceChirho` trait for network device abstraction
//! - `LoopbackDeviceChirho` — loopback device (packets sent are received back)
//! - `EthernetFrameChirho` — Ethernet II frame parsing/building
//! - `Ipv4HeaderChirho` — IPv4 header parsing/building with checksum
//! - `ArpPacketChirho` — ARP request/reply packets
//! - `IcmpPacketChirho` — ICMP echo (ping) packets
//! - `ipv4_checksum_chirho` — ones-complement checksum
//! - Global device registry with loopback pre-registered
//! - Socket syscall stubs (carried forward from Phase 6)

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::syscall_chirho::{
    EAGAIN_CHIRHO, ECONNREFUSED_CHIRHO, ENOSYS_CHIRHO, ENOTSOCK_CHIRHO,
};

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
// NetDeviceChirho trait
// ============================================================================

/// Trait for network device abstraction.
///
/// Each network device (loopback, Ethernet NIC, etc.) implements this trait
/// to provide a uniform interface for sending and receiving packets.
pub trait NetDeviceChirho: Send {
    /// Send a packet through this device.
    fn send_packet_chirho(&mut self, data_chirho: &[u8]);

    /// Try to receive a packet from this device.
    /// Returns `None` if no packet is currently available.
    fn recv_packet_chirho(&mut self) -> Option<Vec<u8>>;

    /// Return the MAC address of this device (6 bytes).
    fn mac_address_chirho(&self) -> [u8; 6];

    /// Return the Maximum Transmission Unit for this device.
    fn mtu_chirho(&self) -> usize;
}

// ============================================================================
// LoopbackDeviceChirho
// ============================================================================

/// Loopback network device — packets sent are received back.
///
/// This mimics Linux's `lo` interface. Any packet transmitted is queued
/// internally and returned on the next `recv_packet_chirho()` call.
pub struct LoopbackDeviceChirho {
    /// Internal packet queue: sent packets are enqueued here.
    queue_chirho: VecDeque<Vec<u8>>,
}

impl LoopbackDeviceChirho {
    /// Create a new loopback device.
    pub fn new_chirho() -> Self {
        Self {
            queue_chirho: VecDeque::new(),
        }
    }
}

impl NetDeviceChirho for LoopbackDeviceChirho {
    fn send_packet_chirho(&mut self, data_chirho: &[u8]) {
        // Loopback: the packet is immediately available for receiving.
        self.queue_chirho.push_back(data_chirho.to_vec());
    }

    fn recv_packet_chirho(&mut self) -> Option<Vec<u8>> {
        self.queue_chirho.pop_front()
    }

    fn mac_address_chirho(&self) -> [u8; 6] {
        // Loopback has all-zero MAC (like Linux lo).
        [0x00; 6]
    }

    fn mtu_chirho(&self) -> usize {
        // Linux loopback MTU is 65536.
        65536
    }
}

// ============================================================================
// EthernetFrameChirho
// ============================================================================

/// Represents an Ethernet II frame.
#[derive(Debug, Clone)]
pub struct EthernetFrameChirho {
    /// Destination MAC address (6 bytes).
    pub dst_mac_chirho: [u8; 6],
    /// Source MAC address (6 bytes).
    pub src_mac_chirho: [u8; 6],
    /// Ethertype (e.g., 0x0800 for IPv4, 0x0806 for ARP).
    pub ethertype_chirho: u16,
    /// Frame payload.
    pub payload_chirho: Vec<u8>,
}

impl EthernetFrameChirho {
    /// Parse an Ethernet frame from raw bytes.
    /// Returns `None` if the data is too short (minimum 14 bytes header).
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 14 {
            return None;
        }
        let mut dst_mac_chirho = [0u8; 6];
        let mut src_mac_chirho = [0u8; 6];
        dst_mac_chirho.copy_from_slice(&data_chirho[0..6]);
        src_mac_chirho.copy_from_slice(&data_chirho[6..12]);
        let ethertype_chirho = u16::from_be_bytes([data_chirho[12], data_chirho[13]]);
        let payload_chirho = data_chirho[14..].to_vec();
        Some(Self {
            dst_mac_chirho,
            src_mac_chirho,
            ethertype_chirho,
            payload_chirho,
        })
    }

    /// Build the Ethernet frame into a byte vector.
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(14 + self.payload_chirho.len());
        buf_chirho.extend_from_slice(&self.dst_mac_chirho);
        buf_chirho.extend_from_slice(&self.src_mac_chirho);
        buf_chirho.extend_from_slice(&self.ethertype_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.payload_chirho);
        buf_chirho
    }
}

// ============================================================================
// Ipv4HeaderChirho
// ============================================================================

/// Represents an IPv4 header (RFC 791).
#[derive(Debug, Clone)]
pub struct Ipv4HeaderChirho {
    /// IP version (always 4).
    pub version_chirho: u8,
    /// Internet Header Length in 32-bit words (typically 5).
    pub ihl_chirho: u8,
    /// Type of Service / DSCP + ECN.
    pub tos_chirho: u8,
    /// Total length of the IP datagram (header + payload) in bytes.
    pub total_length_chirho: u16,
    /// Identification field for fragmentation reassembly.
    pub id_chirho: u16,
    /// Flags (3 bits: reserved, DF, MF).
    pub flags_chirho: u8,
    /// Fragment offset (13 bits, in 8-byte units).
    pub fragment_offset_chirho: u16,
    /// Time to live.
    pub ttl_chirho: u8,
    /// Protocol (e.g., 1=ICMP, 6=TCP, 17=UDP).
    pub protocol_chirho: u8,
    /// Header checksum.
    pub checksum_chirho: u16,
    /// Source IP address (network byte order stored as u32).
    pub src_ip_chirho: u32,
    /// Destination IP address (network byte order stored as u32).
    pub dst_ip_chirho: u32,
}

impl Ipv4HeaderChirho {
    /// Parse an IPv4 header from raw bytes.
    /// Returns `None` if the data is too short or the version is not 4.
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 20 {
            return None;
        }
        let version_chirho = (data_chirho[0] >> 4) & 0xF;
        if version_chirho != 4 {
            return None;
        }
        let ihl_chirho = data_chirho[0] & 0xF;
        let tos_chirho = data_chirho[1];
        let total_length_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let id_chirho = u16::from_be_bytes([data_chirho[4], data_chirho[5]]);
        let flags_and_frag_chirho = u16::from_be_bytes([data_chirho[6], data_chirho[7]]);
        let flags_chirho = ((flags_and_frag_chirho >> 13) & 0x7) as u8;
        let fragment_offset_chirho = flags_and_frag_chirho & 0x1FFF;
        let ttl_chirho = data_chirho[8];
        let protocol_chirho = data_chirho[9];
        let checksum_chirho = u16::from_be_bytes([data_chirho[10], data_chirho[11]]);
        let src_ip_chirho = u32::from_be_bytes([
            data_chirho[12],
            data_chirho[13],
            data_chirho[14],
            data_chirho[15],
        ]);
        let dst_ip_chirho = u32::from_be_bytes([
            data_chirho[16],
            data_chirho[17],
            data_chirho[18],
            data_chirho[19],
        ]);

        Some(Self {
            version_chirho,
            ihl_chirho,
            tos_chirho,
            total_length_chirho,
            id_chirho,
            flags_chirho,
            fragment_offset_chirho,
            ttl_chirho,
            protocol_chirho,
            checksum_chirho,
            src_ip_chirho,
            dst_ip_chirho,
        })
    }

    /// Build the IPv4 header into a byte vector (20 bytes, no options).
    /// The checksum field is computed automatically.
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(20);
        // Byte 0: version (4 bits) | IHL (4 bits)
        buf_chirho.push((self.version_chirho << 4) | (self.ihl_chirho & 0xF));
        // Byte 1: TOS
        buf_chirho.push(self.tos_chirho);
        // Bytes 2-3: total length
        buf_chirho.extend_from_slice(&self.total_length_chirho.to_be_bytes());
        // Bytes 4-5: identification
        buf_chirho.extend_from_slice(&self.id_chirho.to_be_bytes());
        // Bytes 6-7: flags (3 bits) | fragment offset (13 bits)
        let flags_and_frag_chirho =
            ((self.flags_chirho as u16 & 0x7) << 13) | (self.fragment_offset_chirho & 0x1FFF);
        buf_chirho.extend_from_slice(&flags_and_frag_chirho.to_be_bytes());
        // Byte 8: TTL
        buf_chirho.push(self.ttl_chirho);
        // Byte 9: protocol
        buf_chirho.push(self.protocol_chirho);
        // Bytes 10-11: checksum — temporarily zero for computation
        buf_chirho.extend_from_slice(&[0u8; 2]);
        // Bytes 12-15: source IP
        buf_chirho.extend_from_slice(&self.src_ip_chirho.to_be_bytes());
        // Bytes 16-19: destination IP
        buf_chirho.extend_from_slice(&self.dst_ip_chirho.to_be_bytes());

        // Compute and fill in the checksum
        let cksum_chirho = ipv4_checksum_chirho(&buf_chirho);
        buf_chirho[10] = (cksum_chirho >> 8) as u8;
        buf_chirho[11] = (cksum_chirho & 0xFF) as u8;

        buf_chirho
    }
}

// ============================================================================
// ArpPacketChirho
// ============================================================================

/// Represents an ARP packet (for IPv4 over Ethernet).
#[derive(Debug, Clone)]
pub struct ArpPacketChirho {
    /// Hardware type (1 = Ethernet).
    pub htype_chirho: u16,
    /// Protocol type (0x0800 = IPv4).
    pub ptype_chirho: u16,
    /// Hardware address length (6 for Ethernet).
    pub hlen_chirho: u8,
    /// Protocol address length (4 for IPv4).
    pub plen_chirho: u8,
    /// Operation: 1 = request, 2 = reply.
    pub operation_chirho: u16,
    /// Sender hardware address (MAC).
    pub sender_ha_chirho: [u8; 6],
    /// Sender protocol address (IPv4, big-endian u32).
    pub sender_pa_chirho: u32,
    /// Target hardware address (MAC).
    pub target_ha_chirho: [u8; 6],
    /// Target protocol address (IPv4, big-endian u32).
    pub target_pa_chirho: u32,
}

impl ArpPacketChirho {
    /// Parse an ARP packet from raw bytes.
    /// Returns `None` if the data is too short (minimum 28 bytes for IPv4/Ethernet ARP).
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 28 {
            return None;
        }
        let htype_chirho = u16::from_be_bytes([data_chirho[0], data_chirho[1]]);
        let ptype_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let hlen_chirho = data_chirho[4];
        let plen_chirho = data_chirho[5];
        let operation_chirho = u16::from_be_bytes([data_chirho[6], data_chirho[7]]);

        let mut sender_ha_chirho = [0u8; 6];
        sender_ha_chirho.copy_from_slice(&data_chirho[8..14]);
        let sender_pa_chirho = u32::from_be_bytes([
            data_chirho[14],
            data_chirho[15],
            data_chirho[16],
            data_chirho[17],
        ]);
        let mut target_ha_chirho = [0u8; 6];
        target_ha_chirho.copy_from_slice(&data_chirho[18..24]);
        let target_pa_chirho = u32::from_be_bytes([
            data_chirho[24],
            data_chirho[25],
            data_chirho[26],
            data_chirho[27],
        ]);

        Some(Self {
            htype_chirho,
            ptype_chirho,
            hlen_chirho,
            plen_chirho,
            operation_chirho,
            sender_ha_chirho,
            sender_pa_chirho,
            target_ha_chirho,
            target_pa_chirho,
        })
    }

    /// Build the ARP packet into a byte vector (28 bytes for IPv4/Ethernet).
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(28);
        buf_chirho.extend_from_slice(&self.htype_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.ptype_chirho.to_be_bytes());
        buf_chirho.push(self.hlen_chirho);
        buf_chirho.push(self.plen_chirho);
        buf_chirho.extend_from_slice(&self.operation_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.sender_ha_chirho);
        buf_chirho.extend_from_slice(&self.sender_pa_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.target_ha_chirho);
        buf_chirho.extend_from_slice(&self.target_pa_chirho.to_be_bytes());
        buf_chirho
    }
}

// ============================================================================
// IcmpPacketChirho
// ============================================================================

/// Represents an ICMP echo request/reply packet.
#[derive(Debug, Clone)]
pub struct IcmpPacketChirho {
    /// ICMP type (8 = echo request, 0 = echo reply).
    pub type_chirho: u8,
    /// ICMP code (usually 0 for echo).
    pub code_chirho: u8,
    /// ICMP checksum over the entire ICMP message.
    pub checksum_chirho: u16,
    /// Identifier (used to match requests and replies).
    pub id_chirho: u16,
    /// Sequence number.
    pub sequence_chirho: u16,
    /// Optional payload data.
    pub data_chirho: Vec<u8>,
}

impl IcmpPacketChirho {
    /// Parse an ICMP packet from raw bytes.
    /// Returns `None` if the data is too short (minimum 8 bytes).
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 8 {
            return None;
        }
        let type_chirho = data_chirho[0];
        let code_chirho = data_chirho[1];
        let checksum_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let id_chirho = u16::from_be_bytes([data_chirho[4], data_chirho[5]]);
        let sequence_chirho = u16::from_be_bytes([data_chirho[6], data_chirho[7]]);
        let payload_data_chirho = data_chirho[8..].to_vec();

        Some(Self {
            type_chirho,
            code_chirho,
            checksum_chirho,
            id_chirho,
            sequence_chirho,
            data_chirho: payload_data_chirho,
        })
    }

    /// Build the ICMP packet into a byte vector.
    /// The checksum is computed automatically.
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(8 + self.data_chirho.len());
        buf_chirho.push(self.type_chirho);
        buf_chirho.push(self.code_chirho);
        // Checksum placeholder
        buf_chirho.extend_from_slice(&[0u8; 2]);
        buf_chirho.extend_from_slice(&self.id_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.sequence_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.data_chirho);

        // Compute checksum over the entire ICMP message
        let cksum_chirho = ipv4_checksum_chirho(&buf_chirho);
        buf_chirho[2] = (cksum_chirho >> 8) as u8;
        buf_chirho[3] = (cksum_chirho & 0xFF) as u8;

        buf_chirho
    }
}

// ============================================================================
// ipv4_checksum_chirho — ones-complement checksum (RFC 1071)
// ============================================================================

/// Compute the Internet checksum (ones-complement sum) over the given data.
///
/// This is used for IPv4 headers and ICMP packets. The algorithm treats the
/// data as a sequence of 16-bit big-endian words, sums them with ones-complement
/// arithmetic, and returns the bitwise NOT of the result.
pub fn ipv4_checksum_chirho(data_chirho: &[u8]) -> u16 {
    let mut sum_chirho: u32 = 0;
    let len_chirho = data_chirho.len();
    let mut i_chirho: usize = 0;

    // Sum 16-bit words
    while i_chirho + 1 < len_chirho {
        let word_chirho =
            ((data_chirho[i_chirho] as u32) << 8) | (data_chirho[i_chirho + 1] as u32);
        sum_chirho += word_chirho;
        i_chirho += 2;
    }

    // If odd number of bytes, pad the last byte with zero
    if i_chirho < len_chirho {
        sum_chirho += (data_chirho[i_chirho] as u32) << 8;
    }

    // Fold 32-bit sum into 16 bits
    while (sum_chirho >> 16) != 0 {
        sum_chirho = (sum_chirho & 0xFFFF) + (sum_chirho >> 16);
    }

    // Return ones-complement
    !(sum_chirho as u16)
}

// ============================================================================
// Global network device registry
// ============================================================================

/// Global registry of network devices, protected by a spin mutex.
/// The loopback device is pre-registered at index 0 by `init_networking_chirho`.
pub static NET_DEVICES_CHIRHO: Mutex<Vec<Box<dyn NetDeviceChirho>>> = Mutex::new(Vec::new());

// ============================================================================
// init_networking_chirho — initialize the networking subsystem
// ============================================================================

/// Initialize the networking subsystem.
///
/// Creates the loopback device and registers it in the global device list.
pub fn init_networking_chirho() {
    let loopback_chirho = LoopbackDeviceChirho::new_chirho();
    let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
    devices_chirho.push(Box::new(loopback_chirho));
    crate::serial_println_chirho!("[OK] Networking initialized — loopback device registered (lo, MTU=65536)");
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
