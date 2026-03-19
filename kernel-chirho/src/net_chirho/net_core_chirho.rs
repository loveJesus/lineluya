// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Network subsystem for the Lineluya kernel (Phase A3).
//!
//! ## TODO(src-reorg-002): Split into sub-modules
//!
//! This file is ~6500 lines. Planned split:
//! - `net_chirho/socket_chirho.rs` — SocketChirho, fd registration, syscalls
//! - `net_chirho/tcp_chirho.rs` — TcpSegment, TcpControlBlock, TcpState
//! - `net_chirho/udp_chirho.rs` — UDP datagram handling
//! - `net_chirho/ip_chirho.rs` — IPv4 header, routing, ICMP
//! - `net_chirho/arp_chirho.rs` — ARP cache and handling
//! - `net_chirho/dhcp_chirho.rs` — DHCP client
//! - `net_chirho/dns_chirho.rs` — DNS resolver
//! - `net_chirho/device_chirho.rs` — NetDevice trait, loopback
//! - `net_chirho/ioctl_chirho.rs` — Network ioctl handling
//!
//! Provides:
//! - `NetDeviceChirho` trait for network device abstraction
//! - `LoopbackDeviceChirho` — loopback device (packets sent are received back)
//! - `EthernetFrameChirho` — Ethernet II frame parsing/building
//! - `Ipv4HeaderChirho` — IPv4 header parsing/building with checksum
//! - `ArpPacketChirho` — ARP request/reply packets
//! - `IcmpPacketChirho` — ICMP echo (ping) packets
//! - `TcpSegmentChirho` — TCP segment parsing/building (A3-002)
//! - `TcpStateChirho` — Full TCP connection state machine (A3-002)
//! - `TcpControlBlockChirho` — Per-connection TCP state (seq/ack/window) (A3-002)
//! - `SocketChirho` — Socket with TCP state, receive buffer, fd integration (A3-003)
//! - Socket syscalls wired to VFS fd table (A3-003)
//! - `ipv4_checksum_chirho` — ones-complement checksum
//! - Global device registry with loopback pre-registered
//! - **A3-005**: `RoutingTableChirho` — IPv4 routing table with longest-prefix
//!   match and default gateway support.
//! - **A3-006**: ICMP echo (ping) — `handle_icmp_echo_chirho` responds to
//!   incoming echo requests; `send_icmp_echo_request_chirho` sends pings.
//! - **A3-007**: `UdpDatagramChirho` — UDP datagram parsing/building with
//!   checksum.  `UdpSocketTableChirho` for port-based demux.  Full sendto/
//!   recvfrom integration for SOCK_DGRAM sockets.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::syscall_chirho::{
    EADDRINUSE_CHIRHO, EAFNOSUPPORT_CHIRHO, EAGAIN_CHIRHO, EBADF_CHIRHO,
    EFAULT_CHIRHO,
    ECONNREFUSED_CHIRHO, EINVAL_CHIRHO, EISCONN_CHIRHO, ENOSYS_CHIRHO,
    ENOTCONN_CHIRHO, ENOTSOCK_CHIRHO, EOPNOTSUPP_CHIRHO,
};
use crate::vfs_chirho::{FileChirho, FileOpsChirho, InodeChirho};

// ============================================================================
// A2-AUDIT-006: Network types and constants extracted into sub-module
// ============================================================================
#[path = "net_types_chirho.rs"]
mod net_types_chirho;
pub use net_types_chirho::*;

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
        // Linux loopback MTU is LOOPBACK_MTU_CHIRHO (65536).
        LOOPBACK_MTU_CHIRHO
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
// TcpSegmentChirho — TCP segment parsing/building (A3-002)
// ============================================================================

/// Represents a TCP segment header + payload (RFC 793).
#[derive(Debug, Clone)]
pub struct TcpSegmentChirho {
    /// Source port.
    pub src_port_chirho: u16,
    /// Destination port.
    pub dst_port_chirho: u16,
    /// Sequence number.
    pub seq_num_chirho: u32,
    /// Acknowledgment number.
    pub ack_num_chirho: u32,
    /// Data offset in 32-bit words (header length / 4).
    pub data_offset_chirho: u8,
    /// TCP flags (FIN, SYN, RST, PSH, ACK, URG).
    pub flags_chirho: u8,
    /// Window size.
    pub window_chirho: u16,
    /// Checksum.
    pub checksum_chirho: u16,
    /// Urgent pointer.
    pub urgent_ptr_chirho: u16,
    /// Payload data (after TCP header).
    pub payload_chirho: Vec<u8>,
}

impl TcpSegmentChirho {
    /// Parse a TCP segment from raw bytes (IP payload).
    /// Returns `None` if the data is too short (minimum 20 bytes).
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 20 {
            return None;
        }
        let src_port_chirho = u16::from_be_bytes([data_chirho[0], data_chirho[1]]);
        let dst_port_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let seq_num_chirho = u32::from_be_bytes([
            data_chirho[4], data_chirho[5], data_chirho[6], data_chirho[7],
        ]);
        let ack_num_chirho = u32::from_be_bytes([
            data_chirho[8], data_chirho[9], data_chirho[10], data_chirho[11],
        ]);
        let data_offset_chirho = (data_chirho[12] >> 4) & 0xF;
        let flags_chirho = data_chirho[13] & 0x3F;
        let window_chirho = u16::from_be_bytes([data_chirho[14], data_chirho[15]]);
        let checksum_chirho = u16::from_be_bytes([data_chirho[16], data_chirho[17]]);
        let urgent_ptr_chirho = u16::from_be_bytes([data_chirho[18], data_chirho[19]]);

        let header_len_chirho = (data_offset_chirho as usize) * 4;
        let payload_chirho = if data_chirho.len() > header_len_chirho {
            data_chirho[header_len_chirho..].to_vec()
        } else {
            Vec::new()
        };

        Some(Self {
            src_port_chirho,
            dst_port_chirho,
            seq_num_chirho,
            ack_num_chirho,
            data_offset_chirho,
            flags_chirho,
            window_chirho,
            checksum_chirho,
            urgent_ptr_chirho,
            payload_chirho,
        })
    }

    /// Build the TCP segment into a byte vector.
    /// The checksum must be set externally (requires pseudo-header).
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(20 + self.payload_chirho.len());
        buf_chirho.extend_from_slice(&self.src_port_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.dst_port_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.seq_num_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.ack_num_chirho.to_be_bytes());
        // Data offset (4 bits) | reserved (4 bits)
        buf_chirho.push((self.data_offset_chirho << 4) & 0xF0);
        // Flags
        buf_chirho.push(self.flags_chirho & 0x3F);
        buf_chirho.extend_from_slice(&self.window_chirho.to_be_bytes());
        // Checksum placeholder
        buf_chirho.extend_from_slice(&self.checksum_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.urgent_ptr_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.payload_chirho);
        buf_chirho
    }

    /// Compute TCP checksum given the pseudo-header fields.
    pub fn compute_checksum_chirho(
        &self,
        src_ip_chirho: u32,
        dst_ip_chirho: u32,
    ) -> u16 {
        let segment_chirho = self.build_chirho();
        let tcp_len_chirho = segment_chirho.len() as u16;

        // Build pseudo-header + TCP segment for checksum
        let mut pseudo_chirho = Vec::with_capacity(12 + segment_chirho.len());
        pseudo_chirho.extend_from_slice(&src_ip_chirho.to_be_bytes());
        pseudo_chirho.extend_from_slice(&dst_ip_chirho.to_be_bytes());
        pseudo_chirho.push(0); // reserved
        pseudo_chirho.push(IP_PROTO_TCP_CHIRHO);
        pseudo_chirho.extend_from_slice(&tcp_len_chirho.to_be_bytes());
        pseudo_chirho.extend_from_slice(&segment_chirho);

        // Zero out the checksum field in the pseudo-header copy
        let cksum_offset_chirho = 12 + 16; // pseudo(12) + checksum offset in TCP header(16)
        if pseudo_chirho.len() > cksum_offset_chirho + 1 {
            pseudo_chirho[cksum_offset_chirho] = 0;
            pseudo_chirho[cksum_offset_chirho + 1] = 0;
        }

        ipv4_checksum_chirho(&pseudo_chirho)
    }
}

// TcpStateChirho — now in net_types_chirho.rs (A2-AUDIT-006)

// ============================================================================
// TcpControlBlockChirho — per-connection TCP state (A3-002)
// ============================================================================

/// Per-connection TCP control block — tracks sequence numbers, ack numbers,
/// window sizes, and connection state for the TCP state machine.
#[derive(Debug, Clone)]
pub struct TcpControlBlockChirho {
    /// Current TCP state.
    pub state_chirho: TcpStateChirho,

    // --- Send sequence variables (SND.*) ---
    /// Send unacknowledged — oldest unacknowledged sequence number.
    pub snd_una_chirho: u32,
    /// Send next — next sequence number to send.
    pub snd_nxt_chirho: u32,
    /// Send window — how many bytes the peer is willing to accept.
    pub snd_wnd_chirho: u16,
    /// Initial send sequence number (chosen at SYN time).
    pub iss_chirho: u32,

    // --- Receive sequence variables (RCV.*) ---
    /// Receive next — next expected sequence number from peer.
    pub rcv_nxt_chirho: u32,
    /// Receive window — how many bytes we are willing to accept.
    pub rcv_wnd_chirho: u16,
    /// Initial receive sequence number (from peer's SYN).
    pub irs_chirho: u32,
}

impl TcpControlBlockChirho {
    /// Create a new TCB in the CLOSED state.
    pub fn new_chirho() -> Self {
        Self {
            state_chirho: TcpStateChirho::ClosedChirho,
            snd_una_chirho: 0,
            snd_nxt_chirho: 0,
            snd_wnd_chirho: TCP_DEFAULT_WINDOW_CHIRHO,
            iss_chirho: 0,
            rcv_nxt_chirho: 0,
            rcv_wnd_chirho: TCP_DEFAULT_WINDOW_CHIRHO,
            irs_chirho: 0,
        }
    }

    /// Generate an initial sequence number (simplified — uses a global counter).
    fn generate_iss_chirho() -> u32 {
        static ISS_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(1000);
        let val_chirho = ISS_COUNTER_CHIRHO.fetch_add(64000, Ordering::Relaxed);
        val_chirho as u32
    }

    /// Initiate an active open (client SYN). Transitions CLOSED -> SYN_SENT.
    /// Returns a SYN segment to send, or an error.
    pub fn active_open_chirho(
        &mut self,
        src_port_chirho: u16,
        dst_port_chirho: u16,
    ) -> Result<TcpSegmentChirho, i64> {
        if self.state_chirho != TcpStateChirho::ClosedChirho {
            return Err(-EISCONN_CHIRHO);
        }

        self.iss_chirho = Self::generate_iss_chirho();
        self.snd_nxt_chirho = self.iss_chirho.wrapping_add(1);
        self.snd_una_chirho = self.iss_chirho;
        self.state_chirho = TcpStateChirho::SynSentChirho;

        let syn_segment_chirho = TcpSegmentChirho {
            src_port_chirho,
            dst_port_chirho,
            seq_num_chirho: self.iss_chirho,
            ack_num_chirho: 0,
            data_offset_chirho: 5,
            flags_chirho: TCP_SYN_CHIRHO,
            window_chirho: self.rcv_wnd_chirho,
            checksum_chirho: 0,
            urgent_ptr_chirho: 0,
            payload_chirho: Vec::new(),
        };

        Ok(syn_segment_chirho)
    }

    /// Passive open (server). Transitions CLOSED -> LISTEN.
    pub fn passive_open_chirho(&mut self) -> Result<(), i64> {
        if self.state_chirho != TcpStateChirho::ClosedChirho {
            return Err(-EISCONN_CHIRHO);
        }
        self.state_chirho = TcpStateChirho::ListenChirho;
        Ok(())
    }

    /// Process an incoming TCP segment and return an optional response segment.
    /// This implements the core TCP state machine transitions.
    pub fn process_segment_chirho(
        &mut self,
        segment_chirho: &TcpSegmentChirho,
        local_port_chirho: u16,
    ) -> Option<TcpSegmentChirho> {
        let flags_chirho = segment_chirho.flags_chirho;
        let has_syn_chirho = (flags_chirho & TCP_SYN_CHIRHO) != 0;
        let has_ack_chirho = (flags_chirho & TCP_ACK_CHIRHO) != 0;
        let has_fin_chirho = (flags_chirho & TCP_FIN_CHIRHO) != 0;
        let has_rst_chirho = (flags_chirho & TCP_RST_CHIRHO) != 0;

        // RST handling: in any state except CLOSED/LISTEN, RST -> CLOSED
        if has_rst_chirho {
            match self.state_chirho {
                TcpStateChirho::ClosedChirho | TcpStateChirho::ListenChirho => {
                    return None;
                }
                _ => {
                    self.state_chirho = TcpStateChirho::ClosedChirho;
                    return None;
                }
            }
        }

        match self.state_chirho {
            TcpStateChirho::ClosedChirho => {
                // In CLOSED state, send RST for any non-RST segment
                if !has_rst_chirho {
                    return Some(self.make_rst_chirho(segment_chirho, local_port_chirho));
                }
                None
            }

            TcpStateChirho::ListenChirho => {
                // Expect SYN from client
                if has_syn_chirho {
                    self.irs_chirho = segment_chirho.seq_num_chirho;
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.iss_chirho = Self::generate_iss_chirho();
                    self.snd_nxt_chirho = self.iss_chirho.wrapping_add(1);
                    self.snd_una_chirho = self.iss_chirho;
                    self.snd_wnd_chirho = segment_chirho.window_chirho;
                    self.state_chirho = TcpStateChirho::SynReceivedChirho;

                    // Send SYN-ACK
                    return Some(TcpSegmentChirho {
                        src_port_chirho: local_port_chirho,
                        dst_port_chirho: segment_chirho.src_port_chirho,
                        seq_num_chirho: self.iss_chirho,
                        ack_num_chirho: self.rcv_nxt_chirho,
                        data_offset_chirho: 5,
                        flags_chirho: TCP_SYN_CHIRHO | TCP_ACK_CHIRHO,
                        window_chirho: self.rcv_wnd_chirho,
                        checksum_chirho: 0,
                        urgent_ptr_chirho: 0,
                        payload_chirho: Vec::new(),
                    });
                }
                None
            }

            TcpStateChirho::SynSentChirho => {
                // Expect SYN-ACK from server
                if has_syn_chirho && has_ack_chirho {
                    // Validate ACK
                    if segment_chirho.ack_num_chirho != self.snd_nxt_chirho {
                        return Some(self.make_rst_chirho(segment_chirho, local_port_chirho));
                    }
                    self.irs_chirho = segment_chirho.seq_num_chirho;
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.snd_una_chirho = segment_chirho.ack_num_chirho;
                    self.snd_wnd_chirho = segment_chirho.window_chirho;
                    self.state_chirho = TcpStateChirho::EstablishedChirho;

                    // Send ACK
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                // Simultaneous open: SYN without ACK
                if has_syn_chirho && !has_ack_chirho {
                    self.irs_chirho = segment_chirho.seq_num_chirho;
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.state_chirho = TcpStateChirho::SynReceivedChirho;

                    return Some(TcpSegmentChirho {
                        src_port_chirho: local_port_chirho,
                        dst_port_chirho: segment_chirho.src_port_chirho,
                        seq_num_chirho: self.iss_chirho,
                        ack_num_chirho: self.rcv_nxt_chirho,
                        data_offset_chirho: 5,
                        flags_chirho: TCP_SYN_CHIRHO | TCP_ACK_CHIRHO,
                        window_chirho: self.rcv_wnd_chirho,
                        checksum_chirho: 0,
                        urgent_ptr_chirho: 0,
                        payload_chirho: Vec::new(),
                    });
                }
                None
            }

            TcpStateChirho::SynReceivedChirho => {
                // Expect ACK of our SYN-ACK
                if has_ack_chirho {
                    if segment_chirho.ack_num_chirho == self.snd_nxt_chirho {
                        self.snd_una_chirho = segment_chirho.ack_num_chirho;
                        self.snd_wnd_chirho = segment_chirho.window_chirho;
                        self.state_chirho = TcpStateChirho::EstablishedChirho;
                    }
                }
                None
            }

            TcpStateChirho::EstablishedChirho => {
                // Process ACK of sent data
                if has_ack_chirho {
                    self.update_snd_una_chirho(segment_chirho.ack_num_chirho);
                    self.snd_wnd_chirho = segment_chirho.window_chirho;
                }

                // FIN received — peer wants to close
                if has_fin_chirho {
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho
                        .wrapping_add(segment_chirho.payload_chirho.len() as u32)
                        .wrapping_add(1); // +1 for FIN
                    self.state_chirho = TcpStateChirho::CloseWaitChirho;
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }

                // Data received — only accept in-order segments
                if !segment_chirho.payload_chirho.is_empty() {
                    if segment_chirho.seq_num_chirho == self.rcv_nxt_chirho {
                        // In-order: advance rcv_nxt
                        self.rcv_nxt_chirho = segment_chirho.seq_num_chirho
                            .wrapping_add(segment_chirho.payload_chirho.len() as u32);
                    }
                    // Always ACK with current rcv_nxt (duplicate ACK for OOO)
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                None
            }

            TcpStateChirho::FinWait1Chirho => {
                if has_ack_chirho && has_fin_chirho {
                    // Simultaneous close acknowledgment
                    self.update_snd_una_chirho(segment_chirho.ack_num_chirho);
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.state_chirho = TcpStateChirho::TimeWaitChirho;
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                if has_ack_chirho {
                    self.update_snd_una_chirho(segment_chirho.ack_num_chirho);
                    self.state_chirho = TcpStateChirho::FinWait2Chirho;
                }
                if has_fin_chirho {
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.state_chirho = TcpStateChirho::ClosingChirho;
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                None
            }

            TcpStateChirho::FinWait2Chirho => {
                if has_fin_chirho {
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho.wrapping_add(1);
                    self.state_chirho = TcpStateChirho::TimeWaitChirho;
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                // Can still receive data in FIN_WAIT_2
                if !segment_chirho.payload_chirho.is_empty() && has_ack_chirho {
                    self.rcv_nxt_chirho = segment_chirho.seq_num_chirho
                        .wrapping_add(segment_chirho.payload_chirho.len() as u32);
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                None
            }

            TcpStateChirho::CloseWaitChirho => {
                // Waiting for app to call close. Ignore incoming segments mostly.
                if has_ack_chirho {
                    self.update_snd_una_chirho(segment_chirho.ack_num_chirho);
                }
                None
            }

            TcpStateChirho::ClosingChirho => {
                if has_ack_chirho {
                    self.update_snd_una_chirho(segment_chirho.ack_num_chirho);
                    self.state_chirho = TcpStateChirho::TimeWaitChirho;
                }
                None
            }

            TcpStateChirho::LastAckChirho => {
                if has_ack_chirho {
                    self.state_chirho = TcpStateChirho::ClosedChirho;
                }
                None
            }

            TcpStateChirho::TimeWaitChirho => {
                // In TIME_WAIT, respond to any FIN retransmission with ACK
                if has_fin_chirho {
                    return Some(self.make_ack_chirho(local_port_chirho, segment_chirho.src_port_chirho));
                }
                None
            }
        }
    }

    /// Initiate active close. Returns a FIN segment to send.
    pub fn close_chirho(
        &mut self,
        local_port_chirho: u16,
        remote_port_chirho: u16,
    ) -> Option<TcpSegmentChirho> {
        match self.state_chirho {
            TcpStateChirho::EstablishedChirho => {
                self.state_chirho = TcpStateChirho::FinWait1Chirho;
                let fin_seg_chirho = TcpSegmentChirho {
                    src_port_chirho: local_port_chirho,
                    dst_port_chirho: remote_port_chirho,
                    seq_num_chirho: self.snd_nxt_chirho,
                    ack_num_chirho: self.rcv_nxt_chirho,
                    data_offset_chirho: 5,
                    flags_chirho: TCP_FIN_CHIRHO | TCP_ACK_CHIRHO,
                    window_chirho: self.rcv_wnd_chirho,
                    checksum_chirho: 0,
                    urgent_ptr_chirho: 0,
                    payload_chirho: Vec::new(),
                };
                self.snd_nxt_chirho = self.snd_nxt_chirho.wrapping_add(1);
                Some(fin_seg_chirho)
            }
            TcpStateChirho::CloseWaitChirho => {
                self.state_chirho = TcpStateChirho::LastAckChirho;
                let fin_seg_chirho = TcpSegmentChirho {
                    src_port_chirho: local_port_chirho,
                    dst_port_chirho: remote_port_chirho,
                    seq_num_chirho: self.snd_nxt_chirho,
                    ack_num_chirho: self.rcv_nxt_chirho,
                    data_offset_chirho: 5,
                    flags_chirho: TCP_FIN_CHIRHO | TCP_ACK_CHIRHO,
                    window_chirho: self.rcv_wnd_chirho,
                    checksum_chirho: 0,
                    urgent_ptr_chirho: 0,
                    payload_chirho: Vec::new(),
                };
                self.snd_nxt_chirho = self.snd_nxt_chirho.wrapping_add(1);
                Some(fin_seg_chirho)
            }
            _ => None,
        }
    }

    /// Create a data segment for transmission.
    pub fn make_data_segment_chirho(
        &mut self,
        local_port_chirho: u16,
        remote_port_chirho: u16,
        data_chirho: &[u8],
    ) -> Option<TcpSegmentChirho> {
        if self.state_chirho != TcpStateChirho::EstablishedChirho
            && self.state_chirho != TcpStateChirho::CloseWaitChirho
        {
            return None;
        }

        let segment_chirho = TcpSegmentChirho {
            src_port_chirho: local_port_chirho,
            dst_port_chirho: remote_port_chirho,
            seq_num_chirho: self.snd_nxt_chirho,
            ack_num_chirho: self.rcv_nxt_chirho,
            data_offset_chirho: 5,
            flags_chirho: TCP_ACK_CHIRHO | TCP_PSH_CHIRHO,
            window_chirho: self.rcv_wnd_chirho,
            checksum_chirho: 0,
            urgent_ptr_chirho: 0,
            payload_chirho: data_chirho.to_vec(),
        };

        self.snd_nxt_chirho = self.snd_nxt_chirho.wrapping_add(data_chirho.len() as u32);
        Some(segment_chirho)
    }

    /// Update SND.UNA — advance only if the new ack is within window.
    fn update_snd_una_chirho(&mut self, ack_chirho: u32) {
        // Simple check: ack should be >= snd_una and <= snd_nxt
        let diff_una_chirho = ack_chirho.wrapping_sub(self.snd_una_chirho);
        let diff_nxt_chirho = self.snd_nxt_chirho.wrapping_sub(self.snd_una_chirho);
        if diff_una_chirho <= diff_nxt_chirho {
            self.snd_una_chirho = ack_chirho;
        }
    }

    /// Create a bare ACK segment.
    fn make_ack_chirho(
        &self,
        local_port_chirho: u16,
        remote_port_chirho: u16,
    ) -> TcpSegmentChirho {
        TcpSegmentChirho {
            src_port_chirho: local_port_chirho,
            dst_port_chirho: remote_port_chirho,
            seq_num_chirho: self.snd_nxt_chirho,
            ack_num_chirho: self.rcv_nxt_chirho,
            data_offset_chirho: 5,
            flags_chirho: TCP_ACK_CHIRHO,
            window_chirho: self.rcv_wnd_chirho,
            checksum_chirho: 0,
            urgent_ptr_chirho: 0,
            payload_chirho: Vec::new(),
        }
    }

    /// Create a RST segment in response to an unexpected segment.
    fn make_rst_chirho(
        &self,
        incoming_chirho: &TcpSegmentChirho,
        local_port_chirho: u16,
    ) -> TcpSegmentChirho {
        let (seq_chirho, ack_chirho, rst_flags_chirho) = if (incoming_chirho.flags_chirho & TCP_ACK_CHIRHO) != 0 {
            (incoming_chirho.ack_num_chirho, 0u32, TCP_RST_CHIRHO)
        } else {
            let ack_val_chirho = incoming_chirho.seq_num_chirho
                .wrapping_add(incoming_chirho.payload_chirho.len() as u32)
                .wrapping_add(
                    if (incoming_chirho.flags_chirho & TCP_SYN_CHIRHO) != 0 { 1 } else { 0 }
                )
                .wrapping_add(
                    if (incoming_chirho.flags_chirho & TCP_FIN_CHIRHO) != 0 { 1 } else { 0 }
                );
            (0u32, ack_val_chirho, TCP_RST_CHIRHO | TCP_ACK_CHIRHO)
        };

        TcpSegmentChirho {
            src_port_chirho: local_port_chirho,
            dst_port_chirho: incoming_chirho.src_port_chirho,
            seq_num_chirho: seq_chirho,
            ack_num_chirho: ack_chirho,
            data_offset_chirho: 5,
            flags_chirho: rst_flags_chirho,
            window_chirho: 0,
            checksum_chirho: 0,
            urgent_ptr_chirho: 0,
            payload_chirho: Vec::new(),
        }
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
// A3-005: IPv4 Routing Table
// ============================================================================

/// A single entry in the IPv4 routing table.
#[derive(Debug, Clone)]
pub struct RouteEntryChirho {
    /// Destination network address (host byte order as u32).
    pub dest_chirho: u32,
    /// Subnet mask (e.g. 0xFFFFFF00 for /24).
    pub mask_chirho: u32,
    /// Gateway address (0 = directly connected / on-link).
    pub gateway_chirho: u32,
    /// Index into `NET_DEVICES_CHIRHO` for the output interface.
    pub iface_idx_chirho: usize,
    /// Metric / priority (lower = preferred).
    pub metric_chirho: u32,
}

impl RouteEntryChirho {
    /// Return the prefix length of the mask (0-32).
    pub fn prefix_len_chirho(&self) -> u32 {
        self.mask_chirho.count_ones()
    }

    /// Test whether `addr_chirho` matches this route.
    pub fn matches_chirho(&self, addr_chirho: u32) -> bool {
        (addr_chirho & self.mask_chirho) == (self.dest_chirho & self.mask_chirho)
    }
}

/// IPv4 routing table with longest-prefix match.
pub struct RoutingTableChirho {
    /// Route entries, ordered by insertion. Lookup does longest-prefix match.
    entries_chirho: Vec<RouteEntryChirho>,
}

impl RoutingTableChirho {
    /// Create an empty routing table.
    pub const fn new_chirho() -> Self {
        Self {
            entries_chirho: Vec::new(),
        }
    }

    /// Add a route entry.
    pub fn add_route_chirho(&mut self, entry_chirho: RouteEntryChirho) {
        crate::serial_debug_chirho!(
            "[NET] Route added: {}.{}.{}.{}/{} via {}.{}.{}.{} dev{}",
            (entry_chirho.dest_chirho >> 24) & 0xFF,
            (entry_chirho.dest_chirho >> 16) & 0xFF,
            (entry_chirho.dest_chirho >> 8) & 0xFF,
            entry_chirho.dest_chirho & 0xFF,
            entry_chirho.prefix_len_chirho(),
            (entry_chirho.gateway_chirho >> 24) & 0xFF,
            (entry_chirho.gateway_chirho >> 16) & 0xFF,
            (entry_chirho.gateway_chirho >> 8) & 0xFF,
            entry_chirho.gateway_chirho & 0xFF,
            entry_chirho.iface_idx_chirho,
        );
        self.entries_chirho.push(entry_chirho);
    }

    /// Remove all routes for the given destination/mask pair.
    pub fn remove_route_chirho(&mut self, dest_chirho: u32, mask_chirho: u32) {
        self.entries_chirho
            .retain(|e_chirho| e_chirho.dest_chirho != dest_chirho || e_chirho.mask_chirho != mask_chirho);
    }

    /// Look up the best route for a destination address using longest-prefix match.
    ///
    /// Returns `Some((gateway, interface_index))` or `None` if no route matches.
    pub fn lookup_chirho(&self, dst_chirho: u32) -> Option<(u32, usize)> {
        let mut best_chirho: Option<&RouteEntryChirho> = None;
        let mut best_prefix_len_chirho: u32 = 0;

        for entry_chirho in &self.entries_chirho {
            if entry_chirho.matches_chirho(dst_chirho) {
                let pfx_len_chirho = entry_chirho.prefix_len_chirho();
                let should_replace_chirho = match best_chirho {
                    None => true,
                    Some(best_entry_chirho) => {
                        pfx_len_chirho > best_prefix_len_chirho
                            || (pfx_len_chirho == best_prefix_len_chirho
                                && entry_chirho.metric_chirho < best_entry_chirho.metric_chirho)
                    }
                };
                if should_replace_chirho {
                    best_chirho = Some(entry_chirho);
                    best_prefix_len_chirho = pfx_len_chirho;
                }
            }
        }

        best_chirho.map(|e_chirho| (e_chirho.gateway_chirho, e_chirho.iface_idx_chirho))
    }

    /// Return the number of routes in the table.
    pub fn len_chirho(&self) -> usize {
        self.entries_chirho.len()
    }

    /// Check if the routing table is empty.
    #[allow(dead_code)]
    pub fn is_empty_chirho(&self) -> bool {
        self.entries_chirho.is_empty()
    }

    /// Iterate over all route entries.
    pub fn iter_chirho(&self) -> impl Iterator<Item = &RouteEntryChirho> {
        self.entries_chirho.iter()
    }
}

/// Global IPv4 routing table.
pub static ROUTING_TABLE_CHIRHO: Mutex<RoutingTableChirho> =
    Mutex::new(RoutingTableChirho::new_chirho());

/// Helper: convert four octets to a u32 in host byte order.
pub const fn ip4_chirho(a_chirho: u8, b_chirho: u8, c_chirho: u8, d_chirho: u8) -> u32 {
    ((a_chirho as u32) << 24)
        | ((b_chirho as u32) << 16)
        | ((c_chirho as u32) << 8)
        | (d_chirho as u32)
}

/// Set up the default routing table entries.
///
/// Called by `init_networking_chirho`. Adds:
/// - 127.0.0.0/8 via lo (interface 0)
/// - 0.0.0.0/0 default route via 10.0.2.2 (QEMU default gateway), interface 1
fn init_routing_table_chirho() {
    let mut rt_chirho = ROUTING_TABLE_CHIRHO.lock();

    // Loopback route: 127.0.0.0/8 -> lo (device 0)
    rt_chirho.add_route_chirho(RouteEntryChirho {
        dest_chirho: ip4_chirho(127, 0, 0, 0),
        mask_chirho: ip4_chirho(255, 0, 0, 0),
        gateway_chirho: 0, // on-link
        iface_idx_chirho: 0,
        metric_chirho: 0,
    });

    // Default route: 0.0.0.0/0 -> 10.0.2.2 (QEMU user-mode default gw)
    rt_chirho.add_route_chirho(RouteEntryChirho {
        dest_chirho: 0,
        mask_chirho: 0,
        gateway_chirho: ip4_chirho(10, 0, 2, 2),
        iface_idx_chirho: 1, // first real NIC (when available)
        metric_chirho: 100,
    });

    crate::serial_debug_chirho!(
        "[NET] Routing table initialized ({} routes)",
        rt_chirho.len_chirho()
    );
}

/// Route an IPv4 packet to the correct output interface.
///
/// Returns `(gateway_ip, interface_index)` or an error errno.
pub fn route_packet_chirho(dst_ip_chirho: u32) -> Result<(u32, usize), i64> {
    let rt_chirho = ROUTING_TABLE_CHIRHO.lock();
    rt_chirho
        .lookup_chirho(dst_ip_chirho)
        .ok_or(-crate::syscall_chirho::ENETUNREACH_CHIRHO)
}

// ============================================================================
// A3-006: ICMP Echo (ping)
// ============================================================================

/// Global counter for ICMP echo request identifiers.
static ICMP_ECHO_ID_CHIRHO: AtomicU64 = AtomicU64::new(1);

/// Global counter for ICMP echo request sequence numbers.
static ICMP_ECHO_SEQ_CHIRHO: AtomicU64 = AtomicU64::new(1);

/// Handle an incoming ICMP echo request by generating an echo reply.
///
/// Takes the parsed IP header and ICMP packet, returns an IP packet (header +
/// ICMP payload) ready to send back.
pub fn handle_icmp_echo_chirho(
    ip_hdr_chirho: &Ipv4HeaderChirho,
    icmp_chirho: &IcmpPacketChirho,
) -> Option<Vec<u8>> {
    if icmp_chirho.type_chirho != ICMP_ECHO_REQUEST_CHIRHO {
        return None; // Only handle echo requests.
    }

    crate::serial_debug_chirho!(
        "[ICMP] Echo request from {}.{}.{}.{} id={} seq={}",
        (ip_hdr_chirho.src_ip_chirho >> 24) & 0xFF,
        (ip_hdr_chirho.src_ip_chirho >> 16) & 0xFF,
        (ip_hdr_chirho.src_ip_chirho >> 8) & 0xFF,
        ip_hdr_chirho.src_ip_chirho & 0xFF,
        icmp_chirho.id_chirho,
        icmp_chirho.sequence_chirho,
    );

    // Build ICMP echo reply — swap src/dst, type = 0 (reply).
    let reply_icmp_chirho = IcmpPacketChirho {
        type_chirho: ICMP_ECHO_REPLY_CHIRHO,
        code_chirho: 0,
        checksum_chirho: 0, // computed by build_chirho
        id_chirho: icmp_chirho.id_chirho,
        sequence_chirho: icmp_chirho.sequence_chirho,
        data_chirho: icmp_chirho.data_chirho.clone(),
    };
    let icmp_bytes_chirho = reply_icmp_chirho.build_chirho();

    // Build IPv4 header for reply.
    let total_len_chirho = 20 + icmp_bytes_chirho.len() as u16;
    let reply_ip_chirho = Ipv4HeaderChirho {
        version_chirho: 4,
        ihl_chirho: 5,
        tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0,
        flags_chirho: 0,
        fragment_offset_chirho: 0,
        ttl_chirho: 64,
        protocol_chirho: IP_PROTO_ICMP_CHIRHO,
        checksum_chirho: 0, // computed by build_chirho
        src_ip_chirho: ip_hdr_chirho.dst_ip_chirho, // swap src/dst
        dst_ip_chirho: ip_hdr_chirho.src_ip_chirho,
    };
    let mut packet_chirho = reply_ip_chirho.build_chirho();
    packet_chirho.extend_from_slice(&icmp_bytes_chirho);

    crate::serial_debug_chirho!(
        "[ICMP] Sending echo reply to {}.{}.{}.{} ({} bytes)",
        (ip_hdr_chirho.src_ip_chirho >> 24) & 0xFF,
        (ip_hdr_chirho.src_ip_chirho >> 16) & 0xFF,
        (ip_hdr_chirho.src_ip_chirho >> 8) & 0xFF,
        ip_hdr_chirho.src_ip_chirho & 0xFF,
        packet_chirho.len(),
    );

    Some(packet_chirho)
}

/// Build and send an ICMP echo request (ping) to `dst_ip_chirho`.
///
/// Returns the built IP packet (for testing) or `None` on routing failure.
pub fn send_icmp_echo_request_chirho(
    src_ip_chirho: u32,
    dst_ip_chirho: u32,
    payload_chirho: &[u8],
) -> Option<Vec<u8>> {
    let id_chirho = ICMP_ECHO_ID_CHIRHO.load(Ordering::Relaxed) as u16;
    let seq_chirho = ICMP_ECHO_SEQ_CHIRHO.fetch_add(1, Ordering::Relaxed) as u16;

    let echo_req_chirho = IcmpPacketChirho {
        type_chirho: ICMP_ECHO_REQUEST_CHIRHO,
        code_chirho: 0,
        checksum_chirho: 0,
        id_chirho,
        sequence_chirho: seq_chirho,
        data_chirho: payload_chirho.to_vec(),
    };
    let icmp_bytes_chirho = echo_req_chirho.build_chirho();

    let total_len_chirho = 20 + icmp_bytes_chirho.len() as u16;
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4,
        ihl_chirho: 5,
        tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0,
        flags_chirho: 0x02, // Don't Fragment
        fragment_offset_chirho: 0,
        ttl_chirho: 64,
        protocol_chirho: IP_PROTO_ICMP_CHIRHO,
        checksum_chirho: 0,
        src_ip_chirho,
        dst_ip_chirho,
    };

    let mut packet_chirho = ip_hdr_chirho.build_chirho();
    packet_chirho.extend_from_slice(&icmp_bytes_chirho);

    crate::serial_debug_chirho!(
        "[ICMP] Sending echo request to {}.{}.{}.{} id={} seq={}",
        (dst_ip_chirho >> 24) & 0xFF,
        (dst_ip_chirho >> 16) & 0xFF,
        (dst_ip_chirho >> 8) & 0xFF,
        dst_ip_chirho & 0xFF,
        id_chirho,
        seq_chirho,
    );

    Some(packet_chirho)
}

/// Process a received IPv4 packet and dispatch by protocol.
///
/// Handles ICMP echo requests (A3-006) and UDP datagrams (A3-007).
/// Returns an optional response packet to send back.
pub fn process_ipv4_packet_chirho(data_chirho: &[u8]) -> Option<Vec<u8>> {
    let ip_hdr_chirho = Ipv4HeaderChirho::parse_chirho(data_chirho)?;
    let hdr_len_chirho = (ip_hdr_chirho.ihl_chirho as usize) * 4;
    if data_chirho.len() < hdr_len_chirho {
        return None;
    }
    let payload_chirho = &data_chirho[hdr_len_chirho..];

    match ip_hdr_chirho.protocol_chirho {
        IP_PROTO_ICMP_CHIRHO => {
            let icmp_chirho = IcmpPacketChirho::parse_chirho(payload_chirho)?;
            handle_icmp_echo_chirho(&ip_hdr_chirho, &icmp_chirho)
        }
        IP_PROTO_UDP_CHIRHO => {
            let udp_chirho = UdpDatagramChirho::parse_chirho(payload_chirho)?;
            deliver_udp_packet_chirho(&ip_hdr_chirho, &udp_chirho);
            None
        }
        IP_PROTO_TCP_CHIRHO => {
            // Deliver TCP segment to the matching socket's recv buffer.
            // This is the interrupt-driven path (VirtIO-net → IP dispatch).
            // poll_network_chirho also calls deliver_tcp_from_frame_chirho
            // but only for packets it reads directly from the VirtIO ring —
            // packets already consumed by the interrupt handler won't be
            // seen by poll_network, so there's no double delivery.
            deliver_tcp_from_frame_chirho(data_chirho);
            None
        }
        _ => {
            crate::serial_debug_chirho!(
                "[NET] Unhandled IPv4 protocol {}",
                ip_hdr_chirho.protocol_chirho
            );
            None
        }
    }
}

// ============================================================================
// A3-007: UDP Datagram parsing/building
// ============================================================================

/// Represents a UDP datagram (RFC 768).
#[derive(Debug, Clone)]
pub struct UdpDatagramChirho {
    /// Source port.
    pub src_port_chirho: u16,
    /// Destination port.
    pub dst_port_chirho: u16,
    /// Total length of UDP header + payload.
    pub length_chirho: u16,
    /// UDP checksum (0 = not computed).
    pub checksum_chirho: u16,
    /// Payload data.
    pub payload_chirho: Vec<u8>,
}

impl UdpDatagramChirho {
    /// Parse a UDP datagram from raw bytes (IP payload).
    /// Returns `None` if data is too short (minimum 8 bytes header).
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 8 {
            return None;
        }
        let src_port_chirho = u16::from_be_bytes([data_chirho[0], data_chirho[1]]);
        let dst_port_chirho = u16::from_be_bytes([data_chirho[2], data_chirho[3]]);
        let length_chirho = u16::from_be_bytes([data_chirho[4], data_chirho[5]]);
        let checksum_chirho = u16::from_be_bytes([data_chirho[6], data_chirho[7]]);

        let payload_len_chirho = if (length_chirho as usize) > 8 {
            core::cmp::min(length_chirho as usize - 8, data_chirho.len() - 8)
        } else {
            0
        };
        let payload_chirho = data_chirho[8..8 + payload_len_chirho].to_vec();

        Some(Self {
            src_port_chirho,
            dst_port_chirho,
            length_chirho,
            checksum_chirho,
            payload_chirho,
        })
    }

    /// Build the UDP datagram into a byte vector.
    /// Checksum is set to 0 (optional for IPv4 UDP).
    pub fn build_chirho(&self) -> Vec<u8> {
        let total_len_chirho = 8 + self.payload_chirho.len();
        let mut buf_chirho = Vec::with_capacity(total_len_chirho);
        buf_chirho.extend_from_slice(&self.src_port_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.dst_port_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&(total_len_chirho as u16).to_be_bytes());
        // Checksum placeholder (0 = valid for IPv4 UDP)
        buf_chirho.extend_from_slice(&[0u8; 2]);
        buf_chirho.extend_from_slice(&self.payload_chirho);
        buf_chirho
    }

    /// Build with computed checksum using IPv4 pseudo-header.
    pub fn build_with_checksum_chirho(
        &self,
        src_ip_chirho: u32,
        dst_ip_chirho: u32,
    ) -> Vec<u8> {
        let mut raw_chirho = self.build_chirho();
        let udp_len_chirho = raw_chirho.len() as u16;

        // Build pseudo-header for checksum
        let mut pseudo_chirho = Vec::with_capacity(12 + raw_chirho.len());
        pseudo_chirho.extend_from_slice(&src_ip_chirho.to_be_bytes());
        pseudo_chirho.extend_from_slice(&dst_ip_chirho.to_be_bytes());
        pseudo_chirho.push(0); // reserved
        pseudo_chirho.push(IP_PROTO_UDP_CHIRHO);
        pseudo_chirho.extend_from_slice(&udp_len_chirho.to_be_bytes());
        pseudo_chirho.extend_from_slice(&raw_chirho);

        let cksum_chirho = ipv4_checksum_chirho(&pseudo_chirho);
        let final_cksum_chirho = if cksum_chirho == 0 { 0xFFFF } else { cksum_chirho };
        raw_chirho[6] = (final_cksum_chirho >> 8) as u8;
        raw_chirho[7] = (final_cksum_chirho & 0xFF) as u8;
        raw_chirho
    }
}

/// Build a complete IPv4/UDP packet ready to send.
pub fn build_udp_packet_chirho(
    src_ip_chirho: u32,
    dst_ip_chirho: u32,
    src_port_chirho: u16,
    dst_port_chirho: u16,
    payload_chirho: &[u8],
) -> Vec<u8> {
    let udp_chirho = UdpDatagramChirho {
        src_port_chirho,
        dst_port_chirho,
        length_chirho: (8 + payload_chirho.len()) as u16,
        checksum_chirho: 0,
        payload_chirho: payload_chirho.to_vec(),
    };
    let udp_bytes_chirho = udp_chirho.build_with_checksum_chirho(src_ip_chirho, dst_ip_chirho);

    let total_len_chirho = 20 + udp_bytes_chirho.len() as u16;
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4,
        ihl_chirho: 5,
        tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0,
        flags_chirho: 0x02, // DF
        fragment_offset_chirho: 0,
        ttl_chirho: 64,
        protocol_chirho: IP_PROTO_UDP_CHIRHO,
        checksum_chirho: 0,
        src_ip_chirho,
        dst_ip_chirho,
    };

    let mut packet_chirho = ip_hdr_chirho.build_chirho();
    packet_chirho.extend_from_slice(&udp_bytes_chirho);
    packet_chirho
}

/// Deliver a received UDP datagram to the appropriate socket receive buffer.
///
/// Finds a SOCK_DGRAM socket bound to `udp_chirho.dst_port_chirho` and enqueues
/// the payload along with the sender's address.
fn deliver_udp_packet_chirho(
    ip_hdr_chirho: &Ipv4HeaderChirho,
    udp_chirho: &UdpDatagramChirho,
) {
    crate::serial_debug_chirho!(
        "[UDP] Received {}:{} -> {}:{} ({} bytes)",
        format_ip_chirho(ip_hdr_chirho.src_ip_chirho),
        udp_chirho.src_port_chirho,
        format_ip_chirho(ip_hdr_chirho.dst_ip_chirho),
        udp_chirho.dst_port_chirho,
        udp_chirho.payload_chirho.len(),
    );

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    for slot_chirho in table_chirho.iter_mut() {
        if let Some(ref mut sock_chirho) = slot_chirho {
            // Match SOCK_DGRAM sockets bound to the destination port.
            let base_type_chirho = sock_chirho.sock_type_chirho & 0xF;
            if base_type_chirho != 2 {
                // 2 = SOCK_DGRAM
                continue;
            }
            if let Some(ref local_chirho) = sock_chirho.local_addr_chirho {
                if local_chirho.port_chirho == udp_chirho.dst_port_chirho {
                    // Store sender address for recvfrom.
                    sock_chirho.remote_addr_chirho = Some(SockAddrInChirho {
                        port_chirho: udp_chirho.src_port_chirho,
                        addr_chirho: ip_hdr_chirho.src_ip_chirho,
                    });
                    // Enqueue payload data.
                    // For UDP, we prepend a 4-byte length header so recvfrom
                    // can return individual datagrams.
                    let dg_len_chirho = udp_chirho.payload_chirho.len() as u16;
                    sock_chirho.recv_buf_chirho.push_back((dg_len_chirho >> 8) as u8);
                    sock_chirho.recv_buf_chirho.push_back((dg_len_chirho & 0xFF) as u8);
                    for byte_chirho in &udp_chirho.payload_chirho {
                        sock_chirho.recv_buf_chirho.push_back(*byte_chirho);
                    }
                    crate::serial_debug_chirho!(
                        "[UDP] Delivered {} bytes to socket on port {}",
                        udp_chirho.payload_chirho.len(),
                        udp_chirho.dst_port_chirho,
                    );
                    return;
                }
            }
        }
    }

    crate::serial_debug_chirho!(
        "[UDP] No socket bound to port {}, packet dropped",
        udp_chirho.dst_port_chirho,
    );
}

/// Format an IPv4 address (u32 host byte order) as a dotted-quad string.
fn format_ip_chirho(ip_chirho: u32) -> alloc::string::String {
    alloc::format!(
        "{}.{}.{}.{}",
        (ip_chirho >> 24) & 0xFF,
        (ip_chirho >> 16) & 0xFF,
        (ip_chirho >> 8) & 0xFF,
        ip_chirho & 0xFF,
    )
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
/// Creates the loopback device, registers it in the global device list,
/// and populates the initial routing table (A3-005).
pub fn init_networking_chirho() {
    let loopback_chirho = LoopbackDeviceChirho::new_chirho();
    let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
    devices_chirho.push(Box::new(loopback_chirho));
    drop(devices_chirho);
    crate::serial_println_chirho!("[OK] Networking initialized — loopback device registered (lo, MTU={})", LOOPBACK_MTU_CHIRHO);

    // A3-005: set up default routing table.
    init_routing_table_chirho();

    // P3-002: VirtIO-net I/O port devices are probed during init_virtio_chirho
    // (which runs before init_networking_chirho). Any VirtIO-net NIC found
    // via I/O BAR is already registered in NET_DEVICES_CHIRHO at this point.
    // Set loopback IP on the last interface (just pushed above).
    let nic_count_chirho = {
        let devs_chirho = NET_DEVICES_CHIRHO.lock();
        devs_chirho.len()
    };

    // Loopback is always the LAST device (just pushed above).
    set_interface_ip_chirho(nic_count_chirho - 1, LOOPBACK_IP_CHIRHO);

    if nic_count_chirho > 1 {
        // VirtIO-net was registered BEFORE loopback (during init_virtio),
        // so it's at index 0. Run DHCP on interface 0 (the VirtIO NIC).
        crate::serial_debug_chirho!("[NET] Running DHCP on interface 0 (VirtIO NIC) ({} interfaces total)...", nic_count_chirho);
        let _dhcp_result_chirho = dhcp_discover_chirho(0);
    } else {
        crate::serial_debug_chirho!("[NET] No NIC found yet, skipping DHCP ({} interfaces)", nic_count_chirho);
    }
}

// AF constants, AddressFamilyChirho, SocketTypeChirho, SocketStateChirho,
// SockAddrInChirho — now in net_types_chirho.rs (A2-AUDIT-006)

// ============================================================================
// SocketChirho struct (enhanced for A3-002/A3-003)
// ============================================================================

/// Represents a single socket instance with TCP state, buffers, and addressing.
pub struct SocketChirho {
    /// Address family.
    pub family_chirho: u64,
    /// Socket type (SOCK_STREAM, SOCK_DGRAM, SOCK_RAW).
    pub sock_type_chirho: u64,
    /// Protocol number (usually 0 = default for the family/type).
    pub protocol_chirho: u64,
    /// High-level socket state.
    pub state_chirho: SocketStateChirho,
    /// TCP control block (for SOCK_STREAM sockets).
    pub tcb_chirho: TcpControlBlockChirho,
    /// Local address (bound address).
    pub local_addr_chirho: Option<SockAddrInChirho>,
    /// Remote/peer address (connected address).
    pub remote_addr_chirho: Option<SockAddrInChirho>,
    /// Receive buffer — incoming data enqueued here for read/recv.
    pub recv_buf_chirho: VecDeque<u8>,
    /// Accept queue — pending connections (for listening sockets).
    pub accept_queue_chirho: VecDeque<u64>,
    /// Backlog (max pending connections for listen).
    pub backlog_chirho: u32,
    /// Non-blocking flag.
    pub nonblock_chirho: bool,
}

// SAFETY: SocketChirho is always accessed behind Mutex in the SOCKET_TABLE_CHIRHO.
unsafe impl Send for SocketChirho {}
unsafe impl Sync for SocketChirho {}

// Debug impl (manual since VecDeque<u8> can be large)
impl core::fmt::Debug for SocketChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f_chirho.debug_struct("SocketChirho")
            .field("family_chirho", &self.family_chirho)
            .field("sock_type_chirho", &self.sock_type_chirho)
            .field("state_chirho", &self.state_chirho)
            .field("tcb_chirho.state", &self.tcb_chirho.state_chirho)
            .field("recv_buf_len_chirho", &self.recv_buf_chirho.len())
            .finish()
    }
}

impl SocketChirho {
    /// Create a new socket with the given parameters.
    pub fn new_chirho(
        family_chirho: u64,
        sock_type_chirho: u64,
        protocol_chirho: u64,
    ) -> Self {
        let nonblock_chirho = (sock_type_chirho & 0o4000) != 0;
        Self {
            family_chirho,
            sock_type_chirho,
            protocol_chirho,
            state_chirho: SocketStateChirho::UnconnectedChirho,
            tcb_chirho: TcpControlBlockChirho::new_chirho(),
            local_addr_chirho: None,
            remote_addr_chirho: None,
            recv_buf_chirho: VecDeque::new(),
            accept_queue_chirho: VecDeque::new(),
            backlog_chirho: 0,
            nonblock_chirho,
        }
    }

    /// Get the effective connection state, combining socket and TCP state.
    /// For SOCK_STREAM sockets, TCP state takes precedence over socket state,
    /// preventing out-of-sync issues between `state_chirho` and
    /// `tcb_chirho.state_chirho` (audit typed-002).
    pub fn effective_state_chirho(&self) -> SocketStateChirho {
        // TCP state takes precedence for stream sockets
        let base_type_chirho = self.sock_type_chirho & 0xF;
        if base_type_chirho == 1 { // SOCK_STREAM
            match self.tcb_chirho.state_chirho {
                TcpStateChirho::EstablishedChirho => SocketStateChirho::ConnectedChirho,
                TcpStateChirho::ListenChirho => SocketStateChirho::ListeningChirho,
                TcpStateChirho::ClosedChirho => SocketStateChirho::ClosedChirho,
                _ => self.state_chirho, // use socket state for transitional TCP states
            }
        } else {
            self.state_chirho
        }
    }
}

// ============================================================================
// Global socket table (A3-003)
// ============================================================================

/// Maximum number of concurrent sockets.
const MAX_SOCKETS_CHIRHO: usize = 256;

/// Global socket table: maps socket IDs (indices) to SocketChirho instances.
/// Each socket entry is protected by its own Mutex for fine-grained locking.
pub static SOCKET_TABLE_CHIRHO: Mutex<[Option<SocketChirho>; MAX_SOCKETS_CHIRHO]> = {
    // Use a const array initializer since SocketChirho is not Copy.
    const NONE_SOCKET_CHIRHO: Option<SocketChirho> = None;
    Mutex::new([NONE_SOCKET_CHIRHO; MAX_SOCKETS_CHIRHO])
};

/// Atomic counter for ephemeral port assignment.
static NEXT_EPHEMERAL_PORT_CHIRHO: AtomicU64 = AtomicU64::new(49152);

/// Allocate an ephemeral port number.
fn alloc_ephemeral_port_chirho() -> u16 {
    let port_chirho = NEXT_EPHEMERAL_PORT_CHIRHO.fetch_add(1, Ordering::Relaxed);
    // Wrap around in the ephemeral range 49152-65535
    (49152 + ((port_chirho - 49152) % (65535 - 49152 + 1))) as u16
}

/// Allocate a slot in the global socket table. Returns the slot index.
fn alloc_socket_slot_chirho(socket_chirho: SocketChirho) -> Result<usize, i64> {
    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    for (idx_chirho, slot_chirho) in table_chirho.iter_mut().enumerate() {
        if slot_chirho.is_none() {
            *slot_chirho = Some(socket_chirho);
            return Ok(idx_chirho);
        }
    }
    Err(-crate::syscall_chirho::EMFILE_CHIRHO)
}

// ============================================================================
// SocketFileOpsChirho — FileOps implementation for socket fds (A3-003)
// ============================================================================

/// File operations vtable for socket file descriptors.
/// This enables read/write/close on socket fds through the VFS layer.
struct SocketFileOpsChirho;

impl FileOpsChirho for SocketFileOpsChirho {
    fn read_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &mut [u8],
    ) -> Result<usize, i64> {
        // The socket index is stored in inode_chirho.ino_chirho
        let socket_idx_chirho = {
            let inode_guard_chirho = file_chirho.inode_chirho.lock();
            inode_guard_chirho.ino_chirho as usize
        };

        let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
        let socket_chirho = table_chirho.get_mut(socket_idx_chirho)
            .and_then(|s_chirho| s_chirho.as_mut())
            .ok_or(-EBADF_CHIRHO)?;

        // For SOCK_STREAM, read from receive buffer.
        // If buffer is empty and connection is alive, block until data arrives.
        if socket_chirho.recv_buf_chirho.is_empty() {
            if socket_chirho.state_chirho == SocketStateChirho::ClosedChirho
                || socket_chirho.tcb_chirho.state_chirho == TcpStateChirho::CloseWaitChirho
                || socket_chirho.tcb_chirho.state_chirho == TcpStateChirho::ClosedChirho
            {
                return Ok(0); // EOF — connection closed.
            }

            if (file_chirho.flags_chirho & crate::vfs_chirho::O_NONBLOCK_CHIRHO) != 0 {
                return Err(-EAGAIN_CHIRHO);
            }

            // Connection is still alive but no data yet.
            // Block: poll network + yield in a loop until data arrives.
            drop(table_chirho);
            for _wait_chirho in 0..10_000u32 {
                x86_64::instructions::interrupts::enable_and_hlt();
                poll_network_chirho();
                // Don't yield here — keep the CPU and wait for data.
                // Yielding causes the task to go to the back of the queue
                // and never get picked again (PID 0/2 monopolize).
                // Re-check recv_buf.
                let table2_chirho = SOCKET_TABLE_CHIRHO.lock();
                if let Some(Some(ref sock2_chirho)) = table2_chirho.get(socket_idx_chirho) {
                    if !sock2_chirho.recv_buf_chirho.is_empty() {
                        // Data arrived! Read it below.
                        drop(table2_chirho);
                        let mut table3_chirho = SOCKET_TABLE_CHIRHO.lock();
                        let sock3_chirho = table3_chirho.get_mut(socket_idx_chirho)
                            .and_then(|s_chirho| s_chirho.as_mut())
                            .ok_or(-EBADF_CHIRHO)?;
                        let count_chirho = core::cmp::min(buf_chirho.len(), sock3_chirho.recv_buf_chirho.len());
                        for i_chirho in 0..count_chirho {
                            let byte_chirho = match sock3_chirho.recv_buf_chirho.pop_front() {
                                Some(byte_chirho) => byte_chirho,
                                None => {
                                    crate::serial_println_chirho!(
                                        "[NET] socket recv underflow after wake on slot {}",
                                        socket_idx_chirho
                                    );
                                    return Ok(i_chirho);
                                }
                            };
                            buf_chirho[i_chirho] = byte_chirho;
                        }
                        return Ok(count_chirho);
                    }
                    if sock2_chirho.tcb_chirho.state_chirho == TcpStateChirho::ClosedChirho {
                        return Ok(0); // Connection closed while waiting.
                    }
                }
            }
            // Timed out waiting for data.
            return Err(-EAGAIN_CHIRHO);
        }

        let count_chirho = core::cmp::min(buf_chirho.len(), socket_chirho.recv_buf_chirho.len());
        for i_chirho in 0..count_chirho {
            let byte_chirho = match socket_chirho.recv_buf_chirho.pop_front() {
                Some(byte_chirho) => byte_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[NET] socket recv underflow on slot {}",
                        socket_idx_chirho
                    );
                    return Ok(i_chirho);
                }
            };
            buf_chirho[i_chirho] = byte_chirho;
        }
        Ok(count_chirho)
    }

    fn write_chirho(
        &self,
        file_chirho: &mut FileChirho,
        buf_chirho: &[u8],
    ) -> Result<usize, i64> {
        let socket_idx_chirho = {
            let inode_guard_chirho = file_chirho.inode_chirho.lock();
            inode_guard_chirho.ino_chirho as usize
        };

        let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
        let socket_chirho = table_chirho.get_mut(socket_idx_chirho)
            .and_then(|s_chirho| s_chirho.as_mut())
            .ok_or(-EBADF_CHIRHO)?;

        // Unix→TCP relay DISABLED: syslog messages were corrupting SSH stream.
        // Dropbear sends SSH data via sendto(tcp_fd) directly.
        if false && socket_chirho.family_chirho == 1 && !buf_chirho.is_empty() {
            drop(table_chirho);
            let table_relay_chirho = SOCKET_TABLE_CHIRHO.lock();
            let mut tcp_info_chirho: Option<(usize, u16, u32, u32)> = None;
            for (idx_chirho, slot_chirho) in table_relay_chirho.iter().enumerate() {
                if let Some(ref s_chirho) = slot_chirho {
                    if s_chirho.family_chirho == 2
                        && matches!(s_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                        && s_chirho.local_addr_chirho.map(|a| a.port_chirho) == Some(2222)
                    {
                        tcp_info_chirho = Some((
                            idx_chirho,
                            s_chirho.remote_addr_chirho.map(|a| a.port_chirho).unwrap_or(0),
                            s_chirho.remote_addr_chirho.map(|a| a.addr_chirho).unwrap_or(0),
                            get_interface_ip_chirho(0),
                        ));
                        break;
                    }
                }
            }
            drop(table_relay_chirho);
            if let Some((idx_chirho, rport_chirho, rip_chirho, sip_chirho)) = tcp_info_chirho {
                let mut t2_chirho = SOCKET_TABLE_CHIRHO.lock();
                if let Some(Some(ref mut ts_chirho)) = t2_chirho.get_mut(idx_chirho) {
                    if let Some(seg_chirho) = ts_chirho.tcb_chirho.make_data_segment_chirho(
                        2222, rport_chirho, buf_chirho,
                    ) {
                        let ck_chirho = seg_chirho.compute_checksum_chirho(sip_chirho, rip_chirho);
                        let mut sc_chirho = seg_chirho;
                        sc_chirho.checksum_chirho = ck_chirho;
                        let tb_chirho = sc_chirho.build_chirho();
                        let ih_chirho = Ipv4HeaderChirho {
                            version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
                            total_length_chirho: 20 + tb_chirho.len() as u16,
                            id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
                            ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
                            checksum_chirho: 0, src_ip_chirho: sip_chirho, dst_ip_chirho: rip_chirho,
                        };
                        let mut p_chirho = ih_chirho.build_chirho();
                        p_chirho.extend_from_slice(&tb_chirho);
                        drop(t2_chirho);
                        crate::serial_debug_chirho!(
                            "[NET] SSH-RELAY(write): {} bytes Unix->TCP", buf_chirho.len()
                        );
                        let _ = send_ip_packet_chirho(&p_chirho);
                        return Ok(buf_chirho.len());
                    }
                }
            }
            return Ok(buf_chirho.len()); // Consumed silently if no TCP found
        }

        // Use effective_state_chirho to derive connected status from TCP state
        // machine, preventing socket/TCP state desync (audit typed-002).
        if socket_chirho.effective_state_chirho() != SocketStateChirho::ConnectedChirho {
            return Err(-ENOTCONN_CHIRHO);
        }

        // For SOCK_STREAM, attempt to build a TCP data segment
        let local_port_chirho = socket_chirho.local_addr_chirho
            .map(|a_chirho| a_chirho.port_chirho)
            .unwrap_or(0);
        let remote_port_chirho = socket_chirho.remote_addr_chirho
            .map(|a_chirho| a_chirho.port_chirho)
            .unwrap_or(0);

        let segment_chirho = socket_chirho.tcb_chirho.make_data_segment_chirho(
            local_port_chirho,
            remote_port_chirho,
            buf_chirho,
        );

        // Get the IP addresses for the TCP response.
        let local_ip_chirho = socket_chirho.local_addr_chirho
            .map(|a_chirho| a_chirho.addr_chirho)
            .unwrap_or(0);
        let remote_ip_chirho = socket_chirho.remote_addr_chirho
            .map(|a_chirho| a_chirho.addr_chirho)
            .unwrap_or(0);

        // Use our own IP if local is 0.0.0.0 (INADDR_ANY).
        let src_ip_chirho = if local_ip_chirho == 0 {
            get_interface_ip_chirho(0) // use first interface's IP
        } else {
            local_ip_chirho
        };

        drop(table_chirho); // release lock before sending

        // Send the TCP segment over the network.
        match segment_chirho {
            Some(seg_chirho) => {
                crate::serial_debug_chirho!(
                    "[NET] socket write: {} bytes -> {}:{}",
                    buf_chirho.len(), format_ip_chirho(remote_ip_chirho), remote_port_chirho,
                );
                send_tcp_response_chirho(&seg_chirho, src_ip_chirho, remote_ip_chirho);
            }
            None => {
                crate::serial_debug_chirho!(
                    "[NET] socket write: make_data_segment returned None (idx={}, state check failed)",
                    socket_idx_chirho,
                );
            }
        }

        Ok(buf_chirho.len())
    }

    fn seek_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _offset_chirho: i64,
        _whence_chirho: u32,
    ) -> Result<u64, i64> {
        Err(-crate::syscall_chirho::ESPIPE_CHIRHO) // Sockets are not seekable
    }

    fn ioctl_chirho(
        &self,
        file_chirho: &FileChirho,
        cmd_chirho: u64,
        arg_chirho: u64,
    ) -> Result<i64, i64> {
        const FIONREAD_CHIRHO: u64 = 0x541B;

        match cmd_chirho {
            FIONREAD_CHIRHO => {
                if arg_chirho == 0 {
                    return Err(-EFAULT_CHIRHO);
                }

                let socket_idx_chirho = {
                    let inode_guard_chirho = file_chirho.inode_chirho.lock();
                    inode_guard_chirho.ino_chirho as usize
                };
                let recv_len_chirho = {
                    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
                    let socket_chirho = table_chirho
                        .get(socket_idx_chirho)
                        .and_then(|slot_chirho| slot_chirho.as_ref())
                        .ok_or(-EBADF_CHIRHO)?;
                    core::cmp::min(socket_chirho.recv_buf_chirho.len(), i32::MAX as usize) as i32
                };
                let recv_len_bytes_chirho = recv_len_chirho.to_ne_bytes();

                crate::uaccess_chirho::copy_to_user_chirho(
                    arg_chirho,
                    &recv_len_bytes_chirho,
                    recv_len_bytes_chirho.len(),
                )
                .map_err(|_| -EFAULT_CHIRHO)?;
                Ok(0)
            }
            _ => Err(-crate::syscall_chirho::ENOTTY_CHIRHO),
        }
    }

    fn readdir_chirho(
        &self,
        _file_chirho: &mut FileChirho,
        _callback_chirho: &mut dyn FnMut(&str, u64, u8) -> bool,
    ) -> Result<usize, i64> {
        Err(-EINVAL_CHIRHO) // Sockets have no directory entries
    }
}

/// Static instance of socket file ops for all socket fds.
static SOCKET_FILE_OPS_CHIRHO: SocketFileOpsChirho = SocketFileOpsChirho;

/// Dummy inode ops for socket inodes.
struct SocketInodeOpsChirho;

impl crate::vfs_chirho::InodeOpsChirho for SocketInodeOpsChirho {
    fn lookup_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn create_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn mkdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
        _mode_chirho: u32,
    ) -> Result<Arc<InodeChirho>, i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn unlink_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn rmdir_chirho(
        &self,
        _parent_chirho: &InodeChirho,
        _name_chirho: &str,
    ) -> Result<(), i64> {
        Err(-EINVAL_CHIRHO)
    }

    fn readlink_chirho(
        &self,
        _inode_chirho: &InodeChirho,
    ) -> Result<alloc::string::String, i64> {
        Err(-EINVAL_CHIRHO)
    }
}

/// Static instance of socket inode ops.
static SOCKET_INODE_OPS_CHIRHO: SocketInodeOpsChirho = SocketInodeOpsChirho;

/// Create a socket inode with the given socket table index stored in ino_chirho.
fn make_socket_inode_chirho(socket_idx_chirho: usize) -> Arc<Mutex<InodeChirho>> {
    Arc::new(Mutex::new(InodeChirho {
        ino_chirho: socket_idx_chirho as u64,
        mode_chirho: 0o140000 | 0o777, // S_IFSOCK | rwxrwxrwx
        uid_chirho: 0,
        gid_chirho: 0,
        size_chirho: 0,
        nlink_chirho: 1,
        atime_chirho: 0,
        mtime_chirho: 0,
        ctime_chirho: 0,
        ops_chirho: &SOCKET_INODE_OPS_CHIRHO,
        fs_data_chirho: None,
    }))
}

/// Create a FileChirho for a socket and register it in the current task's
/// per-process fd table (with global fallback).
/// Returns the fd number on success.
///
/// A2-PROC-003: Uses alloc_and_insert_fd_chirho for per-process tables.
fn register_socket_fd_chirho(socket_idx_chirho: usize) -> Result<i64, i64> {
    let inode_chirho = make_socket_inode_chirho(socket_idx_chirho);
    let file_chirho = Arc::new(Mutex::new(FileChirho {
        inode_chirho,
        pos_chirho: 0,
        flags_chirho: crate::vfs_chirho::O_RDWR_CHIRHO,
        ops_chirho: &SOCKET_FILE_OPS_CHIRHO,
    }));

    let fd_chirho = crate::fs_chirho::alloc_and_insert_fd_chirho(file_chirho, None);
    if fd_chirho < 0 {
        Err(fd_chirho)
    } else {
        Ok(fd_chirho)
    }
}

/// Look up the socket table index from an fd by reading inode_chirho.ino_chirho.
/// Check if a file descriptor is a socket.
pub fn is_socket_fd_chirho(fd_chirho: u64) -> bool {
    socket_idx_from_fd_chirho(fd_chirho).is_ok()
}

/// Public wrapper for socket_idx_from_fd (used by epoll).
pub fn socket_idx_from_fd_pub_chirho(fd_chirho: u64) -> Result<usize, i64> {
    socket_idx_from_fd_chirho(fd_chirho)
}

/// Check if any established TCP socket on the given port has data.
/// Used by poll() to report POLLIN on pipes that relay TCP SSH data.
pub fn has_tcp_data_for_port_chirho(port_chirho: u16) -> bool {
    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    for slot_chirho in table_chirho.iter() {
        if let Some(ref sock_chirho) = slot_chirho {
            if sock_chirho.family_chirho == 2
                && matches!(sock_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                && sock_chirho.local_addr_chirho.map(|a_chirho| a_chirho.port_chirho) == Some(port_chirho)
                && !sock_chirho.recv_buf_chirho.is_empty()
            {
                return true;
            }
        }
    }
    false
}

/// Check if there's an established TCP connection on the given port (any state).
pub fn has_established_tcp_chirho(port_chirho: u16) -> bool {
    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    for slot_chirho in table_chirho.iter() {
        if let Some(ref sock_chirho) = slot_chirho {
            if sock_chirho.family_chirho == 2
                && matches!(sock_chirho.tcb_chirho.state_chirho,
                    TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                && sock_chirho.local_addr_chirho.map(|a_chirho| a_chirho.port_chirho) == Some(port_chirho)
            {
                return true;
            }
        }
    }
    false
}

fn preview_payload_ascii_chirho(data_chirho: &[u8]) -> alloc::string::String {
    use alloc::string::String;

    let preview_len_chirho = core::cmp::min(data_chirho.len(), 48);
    let mut preview_chirho = String::with_capacity(preview_len_chirho);
    for &byte_chirho in data_chirho.iter().take(preview_len_chirho) {
        let ch_chirho = if (0x20..=0x7e).contains(&byte_chirho) {
            byte_chirho as char
        } else {
            '.'
        };
        preview_chirho.push(ch_chirho);
    }
    preview_chirho
}

/// Check if a socket fd has pending data or connections.
/// For listening sockets, checks if there's a pending TCP connection.
/// For connected sockets, checks if there's received data.
pub fn socket_has_data_chirho(fd_chirho: u64) -> bool {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(fd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(_) => return false,
    };
    let sockets_chirho = SOCKET_TABLE_CHIRHO.lock();
    if let Some(Some(sock_chirho)) = sockets_chirho.get(socket_idx_chirho) {
        // Check if there's data in the receive buffer
        if !sock_chirho.recv_buf_chirho.is_empty() {
            return true;
        }
        // For listening sockets, check if there's a pending connection
        // in the accept queue.
        if sock_chirho.state_chirho == SocketStateChirho::ListeningChirho {
            return !sock_chirho.accept_queue_chirho.is_empty();
        }
        false
    } else {
        false
    }
}

/// A2-PROC-003: Uses lookup_fd_chirho (per-process first, then global).
fn socket_idx_from_fd_chirho(fd_chirho: u64) -> Result<usize, i64> {
    let file_arc_chirho = match crate::fs_chirho::lookup_fd_chirho(fd_chirho) {
        Some(f_chirho) => f_chirho,
        None => return Err(-EBADF_CHIRHO),
    };
    let file_guard_chirho = file_arc_chirho.lock();
    let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
    if inode_guard_chirho.mode_chirho & 0o170000 == 0o140000 {
        Ok(inode_guard_chirho.ino_chirho as usize)
    } else {
        Err(-ENOTSOCK_CHIRHO)
    }
}

/// Read a sockaddr_in from user-space memory, safely.
///
/// Uses `copy_from_user_chirho` to safely read from the user's address space
/// instead of raw pointer arithmetic. Returns `None` if the pointer is null,
/// too short, or the copy fails.
fn read_sockaddr_from_user_chirho(
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> Option<SockAddrInChirho> {
    if addr_chirho == 0 || addrlen_chirho < 8 {
        return None;
    }
    let mut buf_chirho = [0u8; 16];
    let copy_len_chirho = core::cmp::min(addrlen_chirho as usize, 16);
    if crate::uaccess_chirho::copy_from_user_chirho(
        &mut buf_chirho[..copy_len_chirho],
        addr_chirho,
        copy_len_chirho,
    ).is_err() {
        return None;
    }
    SockAddrInChirho::from_user_bytes_chirho(&buf_chirho[..copy_len_chirho])
}

// ============================================================================
// Socket syscall implementations (A3-003)
// ============================================================================

/// `socket(2)` — creates a socket, allocates an fd backed by the VFS fd table.
pub fn sys_socket_chirho(
    domain_chirho: u64,
    type_chirho: u64,
    protocol_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!(
        "[NET] sys_socket(domain={}, type={}, proto={})",
        domain_chirho,
        type_chirho,
        protocol_chirho,
    );

    // Validate address family
    if AddressFamilyChirho::from_raw_chirho(domain_chirho).is_none() {
        return -EAFNOSUPPORT_CHIRHO;
    }

    // Validate socket type
    if SocketTypeChirho::from_raw_chirho(type_chirho).is_none() {
        return -EINVAL_CHIRHO;
    }

    // Create the socket
    let socket_chirho = SocketChirho::new_chirho(domain_chirho, type_chirho, protocol_chirho);

    // Allocate a slot in the global socket table
    let socket_idx_chirho = match alloc_socket_slot_chirho(socket_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    // Register in the VFS fd table
    match register_socket_fd_chirho(socket_idx_chirho) {
        Ok(fd_chirho) => {
            crate::serial_debug_chirho!("[NET] sys_socket -> fd={} (socket_idx={})", fd_chirho, socket_idx_chirho);
            fd_chirho
        }
        Err(e_chirho) => {
            // Clean up socket table slot on fd allocation failure
            let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
            table_chirho[socket_idx_chirho] = None;
            e_chirho
        }
    }
}

/// `bind(2)` — bind a socket to a local address.
pub fn sys_bind_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!("[NET] sys_bind(fd={})", sockfd_chirho);

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let parsed_addr_chirho = unsafe { read_sockaddr_from_user_chirho(addr_chirho, addrlen_chirho) };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();

    // Extract fields we need for the address-in-use check
    let (current_state_chirho, current_family_chirho) = match table_chirho.get(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_ref()) {
        Some(s_chirho) => (s_chirho.state_chirho, s_chirho.family_chirho),
        None => return -EBADF_CHIRHO,
    };

    if current_state_chirho != SocketStateChirho::UnconnectedChirho {
        return -EINVAL_CHIRHO;
    }

    // Check for address-in-use: scan table for same local port
    if let Some(ref bind_addr_chirho) = parsed_addr_chirho {
        for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
            if idx_chirho == socket_idx_chirho {
                continue;
            }
            if let Some(ref other_chirho) = slot_chirho {
                if let Some(ref other_local_chirho) = other_chirho.local_addr_chirho {
                    if other_local_chirho.port_chirho == bind_addr_chirho.port_chirho
                        && other_chirho.family_chirho == current_family_chirho
                    {
                        return -EADDRINUSE_CHIRHO;
                    }
                }
            }
        }
    }

    // Now mutate
    let socket_chirho = match table_chirho
        .get_mut(socket_idx_chirho)
        .and_then(|s_chirho| s_chirho.as_mut())
    {
        Some(socket_chirho) => socket_chirho,
        None => return -EBADF_CHIRHO,
    };

    socket_chirho.local_addr_chirho = parsed_addr_chirho;
    socket_chirho.state_chirho = SocketStateChirho::BoundChirho;

    crate::serial_debug_chirho!("[NET] sys_bind -> 0 (addr={:?})", socket_chirho.local_addr_chirho);
    0
}

/// `listen(2)` — mark a socket as a passive socket to accept connections.
pub fn sys_listen_chirho(sockfd_chirho: u64, backlog_chirho: u64) -> i64 {
    crate::serial_debug_chirho!("[NET] sys_listen(fd={}, backlog={})", sockfd_chirho, backlog_chirho);

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get_mut(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) {
        Some(s_chirho) => s_chirho,
        None => return -EBADF_CHIRHO,
    };

    // Must be SOCK_STREAM
    if SocketTypeChirho::from_raw_chirho(socket_chirho.sock_type_chirho) != Some(SocketTypeChirho::SockStreamChirho) {
        return -EOPNOTSUPP_CHIRHO;
    }

    // Must be bound or unconnected (auto-bind)
    if socket_chirho.state_chirho != SocketStateChirho::BoundChirho
        && socket_chirho.state_chirho != SocketStateChirho::UnconnectedChirho
    {
        return -EINVAL_CHIRHO;
    }

    // Auto-bind if not yet bound
    if socket_chirho.local_addr_chirho.is_none() {
        socket_chirho.local_addr_chirho = Some(SockAddrInChirho {
            port_chirho: alloc_ephemeral_port_chirho(),
            addr_chirho: 0, // INADDR_ANY
        });
    }

    socket_chirho.state_chirho = SocketStateChirho::ListeningChirho;
    socket_chirho.backlog_chirho = core::cmp::max(backlog_chirho as u32, 1);

    // Set the TCP control block to LISTEN state
    let _ = socket_chirho.tcb_chirho.passive_open_chirho();

    // Mark this process as the daemon so is_interactive_shell_chirho() returns
    // false for it — needed for SSH relay to send writev data via TCP.
    crate::syscall_chirho::mark_daemon_listener_chirho();

    crate::serial_debug_chirho!("[NET] sys_listen -> 0");
    0
}

/// `accept(2)` — accept a connection on a listening socket.
pub fn sys_accept_chirho(
    sockfd_chirho: u64,
    _addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!("[NET] sys_accept(fd={})", sockfd_chirho);

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get_mut(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) {
        Some(s_chirho) => s_chirho,
        None => return -EBADF_CHIRHO,
    };

    if socket_chirho.state_chirho != SocketStateChirho::ListeningChirho {
        return -EINVAL_CHIRHO;
    }

    // Check if there is a pending connection in the accept queue
    if let Some(pending_idx_chirho) = socket_chirho.accept_queue_chirho.pop_front() {
        // The pending connection is already a fully established socket in the table.
        // Register it as a new fd.
        drop(table_chirho); // Release table lock before allocating fd
        let new_fd_result_chirho = register_socket_fd_chirho(pending_idx_chirho as usize);
        match new_fd_result_chirho {
            Ok(new_fd_chirho) => {
                crate::serial_debug_chirho!("[NET] sys_accept -> fd={}", new_fd_chirho);

                // Write peer address to user buffer if requested.
                // struct sockaddr_in: { u16 family=2, u16 port, u32 addr, u8[8] zero }
                if _addr_chirho != 0 {
                    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
                    if let Some(Some(ref sock_chirho)) = table_chirho.get(pending_idx_chirho as usize) {
                        if let Some(ref remote_chirho) = sock_chirho.remote_addr_chirho {
                            let mut sa_chirho = [0u8; 16];
                            sa_chirho[0] = 2; sa_chirho[1] = 0; // AF_INET
                            sa_chirho[2] = (remote_chirho.port_chirho >> 8) as u8;
                            sa_chirho[3] = (remote_chirho.port_chirho & 0xFF) as u8;
                            sa_chirho[4..8].copy_from_slice(&remote_chirho.addr_chirho.to_be_bytes());
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    sa_chirho.as_ptr(),
                                    _addr_chirho as *mut u8,
                                    16,
                                );
                            }
                            if _addrlen_chirho != 0 {
                                unsafe {
                                    core::ptr::write(_addrlen_chirho as *mut u32, 16);
                                }
                            }
                        }
                    }
                }

                return new_fd_chirho;
            }
            Err(e_chirho) => return e_chirho,
        }
    }

    // No pending connections
    crate::serial_debug_chirho!("[NET] sys_accept -> -EAGAIN");
    -EAGAIN_CHIRHO
}

/// `accept4(2)` — accept with flags.
pub fn sys_accept4_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    // Delegate to accept; flags (SOCK_NONBLOCK, SOCK_CLOEXEC) are noted but
    // not yet implemented.
    sys_accept_chirho(sockfd_chirho, addr_chirho, addrlen_chirho)
}

/// `connect(2)` — initiate a TCP connection (active open).
pub fn sys_connect_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!("[NET] sys_connect(fd={})", sockfd_chirho);

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let parsed_addr_chirho = unsafe { read_sockaddr_from_user_chirho(addr_chirho, addrlen_chirho) };
    let dest_addr_chirho = match parsed_addr_chirho {
        Some(a_chirho) => a_chirho,
        None => return -EINVAL_CHIRHO,
    };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();

    // Phase 1: validate and mutate the connecting socket, extract needed values
    let (_current_sock_type_chirho, local_addr_chirho, iss_chirho) = {
        let socket_chirho = match table_chirho[socket_idx_chirho].as_mut() {
            Some(s_chirho) => s_chirho,
            None => return -EBADF_CHIRHO,
        };

        if socket_chirho.state_chirho == SocketStateChirho::ConnectedChirho {
            return -EISCONN_CHIRHO;
        }
        if socket_chirho.state_chirho == SocketStateChirho::ListeningChirho {
            return -EOPNOTSUPP_CHIRHO;
        }

        // Auto-assign local port if not bound
        if socket_chirho.local_addr_chirho.is_none() {
            socket_chirho.local_addr_chirho = Some(SockAddrInChirho {
                port_chirho: alloc_ephemeral_port_chirho(),
                addr_chirho: 0x7F000001, // 127.0.0.1
            });
        }

        socket_chirho.remote_addr_chirho = Some(dest_addr_chirho);

        let sock_type_val_chirho = SocketTypeChirho::from_raw_chirho(socket_chirho.sock_type_chirho);
        let local_addr_val_chirho = socket_chirho.local_addr_chirho;

        // For SOCK_STREAM, perform TCP active open
        if sock_type_val_chirho == Some(SocketTypeChirho::SockStreamChirho) {
            let local_port_chirho = match local_addr_val_chirho {
                Some(local_addr_chirho) => local_addr_chirho.port_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[NET] sys_connect: missing local address after bind"
                    );
                    return -EINVAL_CHIRHO;
                }
            };
            let remote_port_chirho = dest_addr_chirho.port_chirho;

            match socket_chirho.tcb_chirho.active_open_chirho(local_port_chirho, remote_port_chirho) {
                Ok(_syn_segment_chirho) => {
                    crate::serial_debug_chirho!(
                        "[NET] sys_connect: SYN sent to port {}",
                        remote_port_chirho,
                    );
                    let iss_val_chirho = socket_chirho.tcb_chirho.iss_chirho;
                    (sock_type_val_chirho, local_addr_val_chirho, iss_val_chirho)
                }
                Err(e_chirho) => return e_chirho,
            }
        } else {
            // SOCK_DGRAM: just set connected
            socket_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
            crate::serial_debug_chirho!("[NET] sys_connect -> 0 (dgram)");
            return 0;
        }
    };
    // At this point the mutable borrow on table_chirho[socket_idx_chirho] is dropped

    let remote_port_chirho = dest_addr_chirho.port_chirho;

    // Phase 2: search for a listening socket (immutable iteration)
    let mut listener_idx_chirho: Option<usize> = None;
    for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
        if idx_chirho == socket_idx_chirho {
            continue;
        }
        if let Some(ref other_chirho) = slot_chirho {
            if other_chirho.state_chirho == SocketStateChirho::ListeningChirho {
                if let Some(ref local_chirho) = other_chirho.local_addr_chirho {
                    if local_chirho.port_chirho == remote_port_chirho {
                        listener_idx_chirho = Some(idx_chirho);
                        break;
                    }
                }
            }
        }
    }

    // Phase 3: if listener found, perform loopback 3-way handshake
    if let Some(listener_idx_val_chirho) = listener_idx_chirho {
        // Extract listener properties
        let (listener_family_chirho, listener_sock_type_chirho, listener_local_addr_chirho) = {
            let listener_chirho = match table_chirho[listener_idx_val_chirho].as_ref() {
                Some(listener_chirho) => listener_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[NET] sys_connect: listener slot {} disappeared",
                        listener_idx_val_chirho
                    );
                    return -ECONNREFUSED_CHIRHO;
                }
            };
            (
                listener_chirho.family_chirho,
                listener_chirho.sock_type_chirho,
                listener_chirho.local_addr_chirho,
            )
        };

        // Create the child (accepted) socket in ESTABLISHED state
        let child_iss_chirho = TcpControlBlockChirho::generate_iss_chirho();
        let mut child_socket_chirho = SocketChirho::new_chirho(
            listener_family_chirho,
            listener_sock_type_chirho,
            0,
        );
        child_socket_chirho.local_addr_chirho = listener_local_addr_chirho;
        child_socket_chirho.remote_addr_chirho = local_addr_chirho;
        child_socket_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
        child_socket_chirho.tcb_chirho.state_chirho = TcpStateChirho::EstablishedChirho;
        child_socket_chirho.tcb_chirho.irs_chirho = iss_chirho;
        child_socket_chirho.tcb_chirho.rcv_nxt_chirho = iss_chirho.wrapping_add(1);
        child_socket_chirho.tcb_chirho.iss_chirho = child_iss_chirho;
        child_socket_chirho.tcb_chirho.snd_nxt_chirho = child_iss_chirho.wrapping_add(1);
        child_socket_chirho.tcb_chirho.snd_una_chirho = child_iss_chirho.wrapping_add(1);

        // Allocate slot for child socket
        let mut child_idx_chirho: Option<usize> = None;
        for (idx_chirho, slot_chirho) in table_chirho.iter_mut().enumerate() {
            if slot_chirho.is_none() {
                *slot_chirho = Some(child_socket_chirho);
                child_idx_chirho = Some(idx_chirho);
                break;
            }
        }

        if let Some(child_idx_val_chirho) = child_idx_chirho {
            // Enqueue child socket in listener's accept queue
            if let Some(ref mut listener_chirho) = table_chirho[listener_idx_val_chirho] {
                listener_chirho.accept_queue_chirho.push_back(child_idx_val_chirho as u64);
            }

            // Complete the handshake: set connecting socket to ESTABLISHED
            if let Some(ref mut connecting_chirho) = table_chirho[socket_idx_chirho] {
                connecting_chirho.tcb_chirho.irs_chirho = child_iss_chirho;
                connecting_chirho.tcb_chirho.rcv_nxt_chirho = child_iss_chirho.wrapping_add(1);
                connecting_chirho.tcb_chirho.snd_una_chirho = connecting_chirho.tcb_chirho.snd_nxt_chirho;
                connecting_chirho.tcb_chirho.state_chirho = TcpStateChirho::EstablishedChirho;
                connecting_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
            }

            crate::serial_debug_chirho!(
                "[NET] sys_connect: 3-way handshake completed (loopback), child_idx={}",
                child_idx_val_chirho,
            );
            return 0;
        }

        // Could not allocate child — connection refused
        if let Some(ref mut connecting_chirho) = table_chirho[socket_idx_chirho] {
            connecting_chirho.tcb_chirho.state_chirho = TcpStateChirho::ClosedChirho;
            connecting_chirho.state_chirho = SocketStateChirho::UnconnectedChirho;
        }
        return -ECONNREFUSED_CHIRHO;
    }

    // No local listener — this is an OUTGOING connection.
    // Send SYN on the network and wait for SYN-ACK.
    drop(table_chirho); // Release lock before network I/O

    // Send SYN packet via VirtIO-net
    let (_gw_chirho, iface_idx_chirho) = match route_packet_chirho(dest_addr_chirho.addr_chirho) {
        Ok(r_chirho) => r_chirho,
        Err(e_chirho) => return e_chirho,
    };
    let src_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);
    if src_ip_chirho == 0 {
        crate::serial_debug_chirho!("[NET] sys_connect: no IP assigned, -ENETUNREACH");
        return -99; // ENETUNREACH
    }

    let local_port_chirho = match local_addr_chirho {
        Some(local_addr_chirho) => local_addr_chirho.port_chirho,
        None => {
            crate::serial_println_chirho!(
                "[NET] sys_connect: local address missing before SYN"
            );
            return -EINVAL_CHIRHO;
        }
    };
    let remote_port_chirho = dest_addr_chirho.port_chirho;
    let remote_ip_chirho = dest_addr_chirho.addr_chirho;

    crate::serial_debug_chirho!(
        "[NET] sys_connect: sending SYN to {}.{}.{}.{}:{}",
        (remote_ip_chirho >> 24) & 0xFF, (remote_ip_chirho >> 16) & 0xFF,
        (remote_ip_chirho >> 8) & 0xFF, remote_ip_chirho & 0xFF,
        remote_port_chirho
    );

    // Build and send SYN
    tcp_send_syn_chirho(src_ip_chirho, remote_ip_chirho, local_port_chirho, remote_port_chirho, iss_chirho);

    // Poll for SYN-ACK (with timeout)
    for poll_chirho in 0..10_000_000u32 {
        core::hint::spin_loop();

        // Check for incoming packets
        {
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            if let Some(dev_chirho) = devs_chirho.get_mut(0) {
                if let Some(raw_chirho) = dev_chirho.recv_packet_chirho() {
                    // Process the received packet — look for SYN-ACK
                    if let Some(eth_chirho) = EthernetFrameChirho::parse_chirho(&raw_chirho) {
                        if eth_chirho.ethertype_chirho == ETHERTYPE_IPV4_CHIRHO {
                            if let Some(ip_chirho) = Ipv4HeaderChirho::parse_chirho(&eth_chirho.payload_chirho) {
                                if ip_chirho.protocol_chirho == IP_PROTO_TCP_CHIRHO {
                                    let hdr_len_chirho = (ip_chirho.ihl_chirho as usize) * 4;
                                    if let Some(seg_chirho) = TcpSegmentChirho::parse_chirho(
                                        &eth_chirho.payload_chirho[hdr_len_chirho..],
                                    ) {
                                        // Check if this is SYN-ACK for our connection
                                        if seg_chirho.dst_port_chirho == local_port_chirho
                                            && seg_chirho.src_port_chirho == remote_port_chirho
                                            && (seg_chirho.flags_chirho & 0x12) == 0x12 // SYN+ACK
                                        {
                                            crate::serial_debug_chirho!(
                                                "[NET] sys_connect: SYN-ACK received! seq={} ack={}",
                                                seg_chirho.seq_num_chirho,
                                                seg_chirho.ack_num_chirho,
                                            );

                                            // Save values before dropping device lock
                                            let syn_ack_seq_chirho = seg_chirho.seq_num_chirho;
                                            let syn_ack_ack_chirho = seg_chirho.ack_num_chirho;
                                            drop(devs_chirho); // Release NET_DEVICES lock!

                                            // Send ACK to complete handshake
                                            tcp_send_ack_chirho(
                                                src_ip_chirho,
                                                remote_ip_chirho,
                                                local_port_chirho,
                                                remote_port_chirho,
                                                seg_chirho.ack_num_chirho,
                                                seg_chirho.seq_num_chirho.wrapping_add(1),
                                            );

                                            // Update socket state
                                            let mut t_chirho = SOCKET_TABLE_CHIRHO.lock();
                                            if let Some(ref mut s_chirho) = t_chirho[socket_idx_chirho] {
                                                s_chirho.tcb_chirho.state_chirho = TcpStateChirho::EstablishedChirho;
                                                s_chirho.tcb_chirho.irs_chirho = syn_ack_seq_chirho;
                                                s_chirho.tcb_chirho.rcv_nxt_chirho = syn_ack_seq_chirho.wrapping_add(1);
                                                s_chirho.tcb_chirho.snd_una_chirho = syn_ack_ack_chirho;
                                                s_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
                                                // Update local addr to use the VirtIO NIC IP
                                                s_chirho.local_addr_chirho = Some(SockAddrInChirho {
                                                    port_chirho: local_port_chirho,
                                                    addr_chirho: src_ip_chirho,
                                                });
                                            }

                                            crate::serial_debug_chirho!("[NET] sys_connect: ESTABLISHED!");
                                            return 0;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if poll_chirho > 0 && poll_chirho % 2_000_000 == 0 {
            crate::serial_debug_chirho!("[NET] sys_connect: waiting for SYN-ACK ({}/10M)...", poll_chirho);
        }
    }

    crate::serial_debug_chirho!("[NET] sys_connect: timeout waiting for SYN-ACK");
    let mut t_chirho = SOCKET_TABLE_CHIRHO.lock();
    if let Some(ref mut s_chirho) = t_chirho[socket_idx_chirho] {
        s_chirho.tcb_chirho.state_chirho = TcpStateChirho::ClosedChirho;
        s_chirho.state_chirho = SocketStateChirho::UnconnectedChirho;
    }
    -110 // ETIMEDOUT
}

/// Send a TCP SYN packet.
fn tcp_send_syn_chirho(
    src_ip_chirho: u32, dst_ip_chirho: u32,
    src_port_chirho: u16, dst_port_chirho: u16,
    seq_chirho: u32,
) {
    let syn_seg_chirho = TcpSegmentChirho {
        src_port_chirho, dst_port_chirho,
        seq_num_chirho: seq_chirho, ack_num_chirho: 0,
        data_offset_chirho: 5, flags_chirho: 0x02, // SYN
        window_chirho: TCP_DEFAULT_WINDOW_CHIRHO, checksum_chirho: 0,
        urgent_ptr_chirho: 0, 
        payload_chirho: Vec::new(),
    };
    let cksum_chirho = syn_seg_chirho.compute_checksum_chirho(src_ip_chirho, dst_ip_chirho);
    let mut syn_ck_chirho = syn_seg_chirho;
    syn_ck_chirho.checksum_chirho = cksum_chirho;
    let tcp_bytes_chirho = syn_ck_chirho.build_chirho();
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
        total_length_chirho: 20 + tcp_bytes_chirho.len() as u16,
        id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
        ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
        checksum_chirho: 0, src_ip_chirho, dst_ip_chirho,
    };
    let mut pkt_chirho = ip_hdr_chirho.build_chirho();
    pkt_chirho.extend_from_slice(&tcp_bytes_chirho);
    let _ = send_ip_packet_chirho(&pkt_chirho);
}

/// Send a TCP ACK packet.
fn tcp_send_ack_chirho(
    src_ip_chirho: u32, dst_ip_chirho: u32,
    src_port_chirho: u16, dst_port_chirho: u16,
    seq_chirho: u32, ack_chirho: u32,
) {
    let ack_seg_chirho = TcpSegmentChirho {
        src_port_chirho, dst_port_chirho,
        seq_num_chirho: seq_chirho, ack_num_chirho: ack_chirho,
        data_offset_chirho: 5, flags_chirho: 0x10, // ACK
        window_chirho: TCP_DEFAULT_WINDOW_CHIRHO, checksum_chirho: 0,
        urgent_ptr_chirho: 0, 
        payload_chirho: Vec::new(),
    };
    let cksum_chirho = ack_seg_chirho.compute_checksum_chirho(src_ip_chirho, dst_ip_chirho);
    let mut ack_ck_chirho = ack_seg_chirho;
    ack_ck_chirho.checksum_chirho = cksum_chirho;
    let tcp_bytes_chirho = ack_ck_chirho.build_chirho();
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
        total_length_chirho: 20 + tcp_bytes_chirho.len() as u16,
        id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
        ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
        checksum_chirho: 0, src_ip_chirho, dst_ip_chirho,
    };
    let mut pkt_chirho = ip_hdr_chirho.build_chirho();
    pkt_chirho.extend_from_slice(&tcp_bytes_chirho);
    let _ = send_ip_packet_chirho(&pkt_chirho);
}

/// `sendto(2)` — send data through a socket.
pub fn sys_sendto_chirho(
    sockfd_chirho: u64,
    buf_chirho: u64,
    len_chirho: u64,
    _flags_chirho: u64,
    _dest_addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::log_net_chirho!(
        "sys_sendto(fd={}, len={})",
        sockfd_chirho,
        len_chirho,
    );

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(err_chirho) => {
            // Not a socket fd — try writing via VFS file ops instead
            if let Some(file_arc_chirho) = crate::fs_chirho::lookup_fd_chirho(sockfd_chirho) {
                let mut file_chirho = file_arc_chirho.lock();
                let data_count_chirho = core::cmp::min(len_chirho as usize, 65536);
                let mut data_chirho = alloc::vec![0u8; data_count_chirho];
                for i_chirho in 0..data_count_chirho {
                    data_chirho[i_chirho] = unsafe { core::ptr::read_volatile((buf_chirho as *const u8).add(i_chirho)) };
                }
                match file_chirho.ops_chirho.write_chirho(&mut file_chirho, &data_chirho) {
                    Ok(n_chirho) => return n_chirho as i64,
                    Err(e_chirho) => return e_chirho,
                }
            }
            crate::serial_debug_chirho!(
                "[NET] sendto fd={} NOT a socket (err={}), no VFS entry either",
                sockfd_chirho, err_chirho
            );
            return len_chirho as i64;
        }
    };

    // Check if this is a Unix domain socket (syslog) — short-circuit to avoid
    // heap allocation and user-memory reads that can trigger GPFs in forked children.
    {
        let table_check_chirho = SOCKET_TABLE_CHIRHO.lock();
        if let Some(Some(sock_check_chirho)) = table_check_chirho.get(socket_idx_chirho) {
            if sock_check_chirho.family_chirho == 1 {
                // AF_UNIX: not supported, silently discard
                return len_chirho as i64;
            }
        }
    }

    // Read data from user-space
    let count_chirho = core::cmp::min(len_chirho as usize, SOCKET_SEND_MAX_CHIRHO);
    let mut data_chirho = Vec::with_capacity(count_chirho);
    if buf_chirho != 0 && count_chirho > 0 {
        let ptr_chirho = buf_chirho as *const u8;
        for i_chirho in 0..count_chirho {
            data_chirho.push(unsafe { core::ptr::read_volatile(ptr_chirho.add(i_chirho)) });
        }
    }

    // Unix→TCP relay DISABLED: This was forwarding syslog messages
    // (written to Unix sockets) into the SSH TCP stream, corrupting
    // the SSH protocol. Dropbear sends SSH data via sendto(fd=tcp_fd)
    // directly — no relay needed.
    #[allow(unused_variables)]
    let _relay_disabled_chirho = true;
    if false {
        let table_relay_chirho = SOCKET_TABLE_CHIRHO.lock();
        let is_unix_chirho = table_relay_chirho.get(socket_idx_chirho)
            .and_then(|s| s.as_ref())
            .map(|s| s.family_chirho == 1)
            .unwrap_or(false);
        if is_unix_chirho && !data_chirho.is_empty() {
            // Log first 40 bytes of data for debugging SSH banner
            let preview_len_chirho = core::cmp::min(data_chirho.len(), 40);
            let preview_chirho = core::str::from_utf8(&data_chirho[..preview_len_chirho])
                .unwrap_or("<binary>");
            crate::serial_debug_chirho!(
                "[NET] SSH-RELAY check: {} bytes from Unix socket: '{}'",
                data_chirho.len(), preview_chirho
            );
            // Find an established TCP socket on port 2222
            let mut tcp_idx_chirho: Option<usize> = None;
            let mut tcp_info_chirho: Option<(u16, u32, u32)> = None; // (remote_port, remote_ip, src_ip)
            for (idx_chirho, slot_chirho) in table_relay_chirho.iter().enumerate() {
                if let Some(ref sock_chirho) = slot_chirho {
                    if sock_chirho.family_chirho == 2
                        && matches!(sock_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                        && sock_chirho.local_addr_chirho.map(|a| a.port_chirho) == Some(2222)
                    {
                        let rp_chirho = sock_chirho.remote_addr_chirho.map(|a| a.port_chirho).unwrap_or(0);
                        let ri_chirho = sock_chirho.remote_addr_chirho.map(|a| a.addr_chirho).unwrap_or(0);
                        tcp_idx_chirho = Some(idx_chirho);
                        tcp_info_chirho = Some((rp_chirho, ri_chirho, get_interface_ip_chirho(0)));
                        break;
                    }
                }
            }
            drop(table_relay_chirho);

            if let (Some(idx_chirho), Some((rport_chirho, rip_chirho, sip_chirho))) =
                (tcp_idx_chirho, tcp_info_chirho)
            {
                let mut table2_chirho = SOCKET_TABLE_CHIRHO.lock();
                if let Some(Some(ref mut tcp_sock_chirho)) = table2_chirho.get_mut(idx_chirho) {
                    if let Some(seg_chirho) = tcp_sock_chirho.tcb_chirho.make_data_segment_chirho(
                        2222, rport_chirho, &data_chirho,
                    ) {
                        let cksum_chirho = seg_chirho.compute_checksum_chirho(sip_chirho, rip_chirho);
                        let mut seg_ck_chirho = seg_chirho;
                        seg_ck_chirho.checksum_chirho = cksum_chirho;
                        let tcp_bytes_chirho = seg_ck_chirho.build_chirho();
                        let ip_hdr_chirho = Ipv4HeaderChirho {
                            version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
                            total_length_chirho: 20 + tcp_bytes_chirho.len() as u16,
                            id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
                            ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
                            checksum_chirho: 0, src_ip_chirho: sip_chirho, dst_ip_chirho: rip_chirho,
                        };
                        let mut pkt_chirho = ip_hdr_chirho.build_chirho();
                        pkt_chirho.extend_from_slice(&tcp_bytes_chirho);
                        drop(table2_chirho);
                        crate::serial_debug_chirho!(
                            "[NET] SSH-RELAY: {} bytes Unix->TCP port 2222", data_chirho.len()
                        );
                        let _ = send_ip_packet_chirho(&pkt_chirho);
                        return count_chirho as i64;
                    }
                }
                return count_chirho as i64;
            }
        }
    } // end if false (relay disabled)


    // Re-acquire socket table for the standard sendto path.
    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get_mut(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) {
        Some(s_chirho) => s_chirho,
        None => return len_chirho as i64,
    };

    // For stream sockets, must be connected — use effective_state_chirho to
    // derive from TCP state machine, preventing desync (audit typed-002).
    let sock_type_chirho = SocketTypeChirho::from_raw_chirho(socket_chirho.sock_type_chirho);
    let eff_state_chirho = socket_chirho.effective_state_chirho();
    if sock_type_chirho == Some(SocketTypeChirho::SockStreamChirho)
        && eff_state_chirho != SocketStateChirho::ConnectedChirho
    {
        crate::serial_debug_chirho!(
            "[NET] sendto fd={} ENOTCONN: eff_state={:?} tcb_state={:?} sock_state={:?}",
            sockfd_chirho, eff_state_chirho,
            socket_chirho.tcb_chirho.state_chirho,
            socket_chirho.state_chirho
        );
        return -ENOTCONN_CHIRHO;
    }

    // For connected sockets, try to deliver to peer's recv buffer (loopback path)
    if socket_chirho.effective_state_chirho() == SocketStateChirho::ConnectedChirho
        && !data_chirho.is_empty()
    {
        let remote_addr_chirho = socket_chirho.remote_addr_chirho;
        let local_port_chirho = socket_chirho.local_addr_chirho.map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);

        // Build and send TCP data segment via the network
        if sock_type_chirho == Some(SocketTypeChirho::SockStreamChirho) {
            let remote_chirho = remote_addr_chirho.unwrap_or(SockAddrInChirho { port_chirho: 0, addr_chirho: 0 });
            let remote_port_chirho = remote_chirho.port_chirho;
            let remote_ip_chirho = remote_chirho.addr_chirho;
            // Use interface IP when local addr is INADDR_ANY (0.0.0.0).
            let raw_local_ip_chirho = socket_chirho.local_addr_chirho
                .map(|a_chirho| a_chirho.addr_chirho)
                .unwrap_or(0);
            let src_ip_chirho = if raw_local_ip_chirho == 0 {
                // Try all interfaces to find a non-zero IP.
                let mut ip_chirho = get_interface_ip_chirho(0);
                if ip_chirho == 0 { ip_chirho = get_interface_ip_chirho(1); }
                ip_chirho
            } else {
                raw_local_ip_chirho
            };

            crate::serial_debug_chirho!(
                "[NET] sendto TCP: {} bytes state={:?} src={}:{} dst={}:{}",
                data_chirho.len(), socket_chirho.tcb_chirho.state_chirho,
                format_ip_chirho(src_ip_chirho), local_port_chirho,
                format_ip_chirho(remote_ip_chirho), remote_port_chirho,
            );
            if let Some(seg_chirho) = socket_chirho.tcb_chirho.make_data_segment_chirho(
                local_port_chirho, remote_port_chirho, &data_chirho,
            ) {
                // Send the TCP data packet
                let cksum_chirho = seg_chirho.compute_checksum_chirho(src_ip_chirho, remote_ip_chirho);
                let mut seg_ck_chirho = seg_chirho;
                seg_ck_chirho.checksum_chirho = cksum_chirho;
                let tcp_bytes_chirho = seg_ck_chirho.build_chirho();
                let ip_hdr_chirho = Ipv4HeaderChirho {
                    version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
                    total_length_chirho: 20 + tcp_bytes_chirho.len() as u16,
                    id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
                    ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
                    checksum_chirho: 0, src_ip_chirho, dst_ip_chirho: remote_ip_chirho,
                };
                let mut pkt_chirho = ip_hdr_chirho.build_chirho();
                pkt_chirho.extend_from_slice(&tcp_bytes_chirho);
                drop(table_chirho); // Release lock before sending
                let _ = send_ip_packet_chirho(&pkt_chirho);
                return count_chirho as i64;
            }
        }

        // Find peer socket by matching remote/local addrs (loopback delivery)
        if let Some(remote_chirho) = remote_addr_chirho {
            let local_addr_chirho = socket_chirho.local_addr_chirho;
            // We need to drop the borrow on socket_chirho to iterate the table
            let peer_idx_chirho = {
                let mut found_chirho: Option<usize> = None;
                for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
                    if idx_chirho == socket_idx_chirho {
                        continue;
                    }
                    if let Some(ref other_chirho) = slot_chirho {
                        if other_chirho.state_chirho == SocketStateChirho::ConnectedChirho {
                            if let (Some(ref ol_chirho), Some(ref or_chirho)) =
                                (&other_chirho.local_addr_chirho, &other_chirho.remote_addr_chirho)
                            {
                                if ol_chirho.port_chirho == remote_chirho.port_chirho
                                    && or_chirho.port_chirho == local_addr_chirho.map(|a_chirho| a_chirho.port_chirho).unwrap_or(0)
                                {
                                    found_chirho = Some(idx_chirho);
                                    break;
                                }
                            }
                        }
                    }
                }
                found_chirho
            };

            if let Some(peer_idx_val_chirho) = peer_idx_chirho {
                if let Some(ref mut peer_chirho) = table_chirho[peer_idx_val_chirho] {
                    // Deliver data to peer's receive buffer
                    for byte_chirho in &data_chirho {
                        peer_chirho.recv_buf_chirho.push_back(*byte_chirho);
                    }
                    crate::log_net_chirho!(
                        "sendto: delivered {} bytes to peer socket_idx={}",
                        data_chirho.len(),
                        peer_idx_val_chirho,
                    );
                }
            }
        }
    }

    data_chirho.len() as i64
}

/// `recvfrom(2)` — receive data from a socket.
pub fn sys_recvfrom_chirho(
    sockfd_chirho: u64,
    buf_chirho: u64,
    len_chirho: u64,
    _flags_chirho: u64,
    _src_addr_chirho: u64,
    _addrlen_chirho: u64,
) -> i64 {
    crate::serial_debug_chirho!(
        "[NET] sys_recvfrom(fd={}, len={})",
        sockfd_chirho,
        len_chirho,
    );

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(_) => return 0, // Fallback for non-socket fds
    };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get_mut(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) {
        Some(s_chirho) => s_chirho,
        None => return 0,
    };

    // For stream sockets, must be connected
    let sock_type_chirho = SocketTypeChirho::from_raw_chirho(socket_chirho.sock_type_chirho);
    if sock_type_chirho == Some(SocketTypeChirho::SockStreamChirho)
        && socket_chirho.state_chirho != SocketStateChirho::ConnectedChirho
        && socket_chirho.tcb_chirho.state_chirho != TcpStateChirho::CloseWaitChirho
    {
        return -ENOTCONN_CHIRHO;
    }

    if socket_chirho.recv_buf_chirho.is_empty() {
        if socket_chirho.tcb_chirho.state_chirho == TcpStateChirho::CloseWaitChirho
            || socket_chirho.tcb_chirho.state_chirho == TcpStateChirho::ClosedChirho
        {
            return 0; // EOF — peer closed
        }

        // Quick poll: check for new packets once, then return EAGAIN.
        // The old 500K/1K iteration polling loop blocked for too long,
        // preventing dropbear from processing received SSH packets and
        // computing the KEX_ECDH_REPLY. Dropbear's event loop calls
        // poll() → read() in a tight loop — it needs fast EAGAIN returns
        // to exit the read path and enter its crypto computation path.
        drop(table_chirho);
        poll_network_chirho();

        // Re-check once after polling
        let table_recheck_chirho = SOCKET_TABLE_CHIRHO.lock();
        if let Some(ref sock_recheck_chirho) = table_recheck_chirho
            .get(socket_idx_chirho)
            .and_then(|s_chirho| s_chirho.as_ref())
        {
            if !sock_recheck_chirho.recv_buf_chirho.is_empty() {
                drop(table_recheck_chirho);
                // Data arrived — fall through to copy-out below
            } else {
                return -11; // EAGAIN — no data yet
            }
        } else {
            return -11; // EAGAIN
        }

        // After polling, re-acquire the lock and fall through to copy-out.
        let mut table_final_chirho = SOCKET_TABLE_CHIRHO.lock();
        let socket_final_chirho = match table_final_chirho
            .get_mut(socket_idx_chirho)
            .and_then(|s_chirho| s_chirho.as_mut())
        {
            Some(s_chirho) => s_chirho,
            None => return 0,
        };

        if socket_final_chirho.recv_buf_chirho.is_empty() {
            // Buffer empty after polling — return EAGAIN (not 0 which means EOF).
            // Dropbear interprets 0 as connection closed and exits.
            return -11; // EAGAIN
        }

        // Copy buffered data to userspace.
        let count_chirho = core::cmp::min(len_chirho as usize, socket_final_chirho.recv_buf_chirho.len());
        if buf_chirho != 0 && count_chirho > 0 {
            let ptr_chirho = buf_chirho as *mut u8;
            for i_chirho in 0..count_chirho {
                let byte_chirho = match socket_final_chirho.recv_buf_chirho.pop_front() {
                    Some(byte_chirho) => byte_chirho,
                    None => {
                        crate::serial_println_chirho!(
                            "[NET] recvfrom polled underflow on socket {}",
                            socket_idx_chirho
                        );
                        return i_chirho as i64;
                    }
                };
                unsafe { core::ptr::write_volatile(ptr_chirho.add(i_chirho), byte_chirho) };
            }
        }
        crate::log_net_chirho!("recvfrom (polled) -> {}", count_chirho);
        return count_chirho as i64;
    }

    let count_chirho = core::cmp::min(len_chirho as usize, socket_chirho.recv_buf_chirho.len());
    if buf_chirho != 0 && count_chirho > 0 {
        let ptr_chirho = buf_chirho as *mut u8;
        for i_chirho in 0..count_chirho {
            let byte_chirho = match socket_chirho.recv_buf_chirho.pop_front() {
                Some(byte_chirho) => byte_chirho,
                None => {
                    crate::serial_println_chirho!(
                        "[NET] recvfrom underflow on socket {}",
                        socket_idx_chirho
                    );
                    return i_chirho as i64;
                }
            };
            unsafe { core::ptr::write_volatile(ptr_chirho.add(i_chirho), byte_chirho) };
        }
    }

    crate::log_net_chirho!("recvfrom -> {}", count_chirho);
    count_chirho as i64
}

/// `sendmsg(2)` — send a scatter/gather message through a socket.
pub fn sys_sendmsg_chirho(
    sockfd_chirho: u64,
    msg_chirho: u64,
    flags_chirho: u64,
) -> i64 {
    let msg_hdr_chirho = match read_msghdr_from_user_chirho(msg_chirho) {
        Ok(msg_hdr_chirho) => msg_hdr_chirho,
        Err(errno_chirho) => return errno_chirho,
    };
    if msg_hdr_chirho.msg_iovlen_chirho < 0 {
        return -EINVAL_CHIRHO;
    }

    let data_chirho = match gather_iovec_checked_chirho(
        msg_hdr_chirho.msg_iov_chirho,
        msg_hdr_chirho.msg_iovlen_chirho as usize,
    ) {
        Ok(data_chirho) => data_chirho,
        Err(errno_chirho) => return errno_chirho,
    };

    sys_sendto_chirho(
        sockfd_chirho,
        data_chirho.as_ptr() as u64,
        data_chirho.len() as u64,
        flags_chirho,
        msg_hdr_chirho.msg_name_chirho,
        msg_hdr_chirho.msg_namelen_chirho as u64,
    )
}

/// `recvmsg(2)` — receive a scatter/gather message from a socket.
pub fn sys_recvmsg_chirho(
    sockfd_chirho: u64,
    msg_chirho: u64,
    flags_chirho: u64,
) -> i64 {
    let mut msg_hdr_chirho = match read_msghdr_from_user_chirho(msg_chirho) {
        Ok(msg_hdr_chirho) => msg_hdr_chirho,
        Err(errno_chirho) => return errno_chirho,
    };
    if msg_hdr_chirho.msg_iovlen_chirho < 0 {
        return -EINVAL_CHIRHO;
    }

    let total_capacity_chirho = match total_iovec_len_chirho(
        msg_hdr_chirho.msg_iov_chirho,
        msg_hdr_chirho.msg_iovlen_chirho as usize,
    ) {
        Ok(total_capacity_chirho) => total_capacity_chirho,
        Err(errno_chirho) => return errno_chirho,
    };
    if total_capacity_chirho == 0 {
        msg_hdr_chirho.msg_controllen_chirho = 0;
        msg_hdr_chirho.msg_flags_chirho = 0;
        if let Err(errno_chirho) = write_msghdr_to_user_chirho(msg_chirho, &msg_hdr_chirho) {
            return errno_chirho;
        }
        return 0;
    }

    let mut recv_buf_chirho = alloc::vec![0u8; total_capacity_chirho];
    let recv_count_chirho = sys_recvfrom_chirho(
        sockfd_chirho,
        recv_buf_chirho.as_mut_ptr() as u64,
        recv_buf_chirho.len() as u64,
        flags_chirho,
        0,
        0,
    );
    if recv_count_chirho <= 0 {
        return recv_count_chirho;
    }

    let recv_len_chirho = recv_count_chirho as usize;
    if let Err(errno_chirho) = scatter_iovec_checked_chirho(
        msg_hdr_chirho.msg_iov_chirho,
        msg_hdr_chirho.msg_iovlen_chirho as usize,
        &recv_buf_chirho[..recv_len_chirho],
    ) {
        return errno_chirho;
    }

    if msg_hdr_chirho.msg_name_chirho != 0 {
        if let Ok(socket_idx_chirho) = socket_idx_from_fd_chirho(sockfd_chirho) {
            let table_chirho = SOCKET_TABLE_CHIRHO.lock();
            if let Some(remote_addr_chirho) = table_chirho
                .get(socket_idx_chirho)
                .and_then(|slot_chirho| slot_chirho.as_ref())
                .and_then(|socket_chirho| socket_chirho.remote_addr_chirho)
            {
                let remote_bytes_chirho =
                    remote_addr_chirho.to_user_bytes_chirho(AddressFamilyChirho::AfInetChirho as u16);
                let name_len_chirho = core::cmp::min(
                    remote_bytes_chirho.len(),
                    msg_hdr_chirho.msg_namelen_chirho as usize,
                );
                if crate::uaccess_chirho::copy_to_user_chirho(
                    msg_hdr_chirho.msg_name_chirho,
                    &remote_bytes_chirho[..name_len_chirho],
                    name_len_chirho,
                )
                .is_err()
                {
                    return -EFAULT_CHIRHO;
                }
                msg_hdr_chirho.msg_namelen_chirho = remote_bytes_chirho.len() as u32;
            }
        }
    }

    msg_hdr_chirho.msg_controllen_chirho = 0;
    msg_hdr_chirho.msg_flags_chirho = 0;
    if let Err(errno_chirho) = write_msghdr_to_user_chirho(msg_chirho, &msg_hdr_chirho) {
        return errno_chirho;
    }

    recv_count_chirho
}

/// `setsockopt(2)` — set socket options on a socket.
pub fn sys_setsockopt_chirho(
    sockfd_chirho: u64,
    level_chirho: u64,
    optname_chirho: u64,
    optval_chirho: u64,
    optlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(socket_idx_chirho) => socket_idx_chirho,
        Err(errno_chirho) => return errno_chirho,
    };
    setsockopt_impl_chirho(
        socket_idx_chirho,
        level_chirho,
        optname_chirho,
        optval_chirho,
        optlen_chirho,
    )
}

/// `getsockopt(2)` — get socket options from a socket.
pub fn sys_getsockopt_chirho(
    sockfd_chirho: u64,
    level_chirho: u64,
    optname_chirho: u64,
    optval_chirho: u64,
    optlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(socket_idx_chirho) => socket_idx_chirho,
        Err(errno_chirho) => return errno_chirho,
    };
    getsockopt_impl_chirho(
        socket_idx_chirho,
        level_chirho,
        optname_chirho,
        optval_chirho,
        optlen_chirho,
    )
}

/// `shutdown(2)` — shut down part of a full-duplex connection.
pub fn sys_shutdown_chirho(sockfd_chirho: u64, how_chirho: u64) -> i64 {
    crate::serial_debug_chirho!("[NET] sys_shutdown(fd={}, how={})", sockfd_chirho, how_chirho);

    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(_) => return 0, // Stub fallback
    };

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get_mut(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) {
        Some(s_chirho) => s_chirho,
        None => return 0,
    };

    if socket_chirho.state_chirho == SocketStateChirho::ConnectedChirho {
        let local_port_chirho = socket_chirho.local_addr_chirho
            .map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);
        let remote_port_chirho = socket_chirho.remote_addr_chirho
            .map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);

        // Initiate TCP close
        let _fin_seg_chirho = socket_chirho.tcb_chirho.close_chirho(local_port_chirho, remote_port_chirho);
        socket_chirho.state_chirho = SocketStateChirho::ClosedChirho;
    }

    0
}

/// `getsockname(2)` — get the local address of a socket.
pub fn sys_getsockname_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_ref()) {
        Some(s_chirho) => s_chirho,
        None => return -EBADF_CHIRHO,
    };

    if let Some(ref local_chirho) = socket_chirho.local_addr_chirho {
        if addr_chirho != 0 && addrlen_chirho >= 16 {
            let buf_chirho = local_chirho.to_user_bytes_chirho(socket_chirho.family_chirho as u16);
            let ptr_chirho = addr_chirho as *mut u8;
            for (i_chirho, b_chirho) in buf_chirho.iter().enumerate() {
                unsafe { core::ptr::write_volatile(ptr_chirho.add(i_chirho), *b_chirho) };
            }
            // Write addrlen
            let addrlen_ptr_chirho = addrlen_chirho as *mut u32;
            unsafe { core::ptr::write_volatile(addrlen_ptr_chirho, 16) };
        }
        return 0;
    }

    -ENOTSOCK_CHIRHO
}

/// `getpeername(2)` — get the remote address of a connected socket.
pub fn sys_getpeername_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => {
            crate::serial_debug_chirho!(
                "[NET] getpeername(fd={}) -> err {} (not a socket)",
                sockfd_chirho, e_chirho,
            );
            return e_chirho;
        }
    };

    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let socket_chirho = match table_chirho.get(socket_idx_chirho).and_then(|s_chirho| s_chirho.as_ref()) {
        Some(s_chirho) => s_chirho,
        None => return -EBADF_CHIRHO,
    };

    let eff_state_chirho = socket_chirho.effective_state_chirho();
    crate::serial_debug_chirho!(
        "[NET] getpeername(fd={}, idx={}) state={:?} remote={:?}",
        sockfd_chirho, socket_idx_chirho, eff_state_chirho,
        socket_chirho.remote_addr_chirho,
    );
    if eff_state_chirho != SocketStateChirho::ConnectedChirho {
        return -ENOTCONN_CHIRHO;
    }

    if let Some(ref remote_chirho) = socket_chirho.remote_addr_chirho {
        if addr_chirho != 0 && addrlen_chirho >= 16 {
            let buf_chirho = remote_chirho.to_user_bytes_chirho(socket_chirho.family_chirho as u16);
            let ptr_chirho = addr_chirho as *mut u8;
            for (i_chirho, b_chirho) in buf_chirho.iter().enumerate() {
                unsafe { core::ptr::write_volatile(ptr_chirho.add(i_chirho), *b_chirho) };
            }
            let addrlen_ptr_chirho = addrlen_chirho as *mut u32;
            unsafe { core::ptr::write_volatile(addrlen_ptr_chirho, 16) };
        }
        return 0;
    }

    -ENOTCONN_CHIRHO
}

/// `socketpair(2)` — create a pair of connected sockets.
pub fn sys_socketpair_chirho(
    _domain_chirho: u64,
    _type_chirho: u64,
    _protocol_chirho: u64,
    _sv_chirho: u64,
) -> i64 {
    -ENOSYS_CHIRHO
}

// ============================================================================
// A3-009: TCP retransmission and flow control
// ============================================================================

/// Retransmission timer state for a TCP connection.
#[derive(Debug, Clone)]
pub struct TcpRetransmitChirho {
    /// Retransmission timeout in ticks (starts at ~1s = 100 ticks).
    pub rto_ticks_chirho: u64,
    /// Timer countdown: ticks remaining until retransmit.
    pub timer_remaining_chirho: u64,
    /// Number of retransmission attempts for the current segment.
    pub retransmit_count_chirho: u32,
    /// Maximum retransmissions before connection abort.
    pub max_retransmits_chirho: u32,
    /// Smoothed RTT estimate in ticks (SRTT, RFC 6298).
    pub srtt_chirho: u64,
    /// RTT variation (RTTVAR, RFC 6298).
    pub rttvar_chirho: u64,
    /// Last unacknowledged segment bytes (for retransmission).
    pub unacked_data_chirho: Vec<u8>,
    /// Sequence number of the unacked data start.
    pub unacked_seq_chirho: u32,
    /// Congestion window (cwnd) in bytes.
    pub cwnd_chirho: u32,
    /// Slow-start threshold (ssthresh) in bytes.
    pub ssthresh_chirho: u32,
    /// Whether retransmit timer is armed.
    pub timer_active_chirho: bool,
}

impl TcpRetransmitChirho {
    /// Create a new retransmission state with default values.
    pub fn new_chirho() -> Self {
        Self {
            rto_ticks_chirho: 100,       // 1 second at 100 Hz
            timer_remaining_chirho: 0,
            retransmit_count_chirho: 0,
            max_retransmits_chirho: 15,   // Linux default
            srtt_chirho: 0,
            rttvar_chirho: 50,            // initial variance ~500ms
            unacked_data_chirho: Vec::new(),
            unacked_seq_chirho: 0,
            cwnd_chirho: TCP_DEFAULT_MSS_CHIRHO as u32 * 10, // IW=10 per RFC 6928
            ssthresh_chirho: TCP_DEFAULT_WINDOW_CHIRHO as u32,
            timer_active_chirho: false,
        }
    }

    /// Update SRTT/RTTVAR from a new RTT measurement (RFC 6298).
    pub fn update_rtt_chirho(&mut self, rtt_ticks_chirho: u64) {
        if self.srtt_chirho == 0 {
            // First measurement
            self.srtt_chirho = rtt_ticks_chirho;
            self.rttvar_chirho = rtt_ticks_chirho / 2;
        } else {
            // RTTVAR = (1 - beta) * RTTVAR + beta * |SRTT - R'|
            let diff_chirho = if self.srtt_chirho > rtt_ticks_chirho {
                self.srtt_chirho - rtt_ticks_chirho
            } else {
                rtt_ticks_chirho - self.srtt_chirho
            };
            self.rttvar_chirho = (3 * self.rttvar_chirho + diff_chirho) / 4;
            // SRTT = (1 - alpha) * SRTT + alpha * R'
            self.srtt_chirho = (7 * self.srtt_chirho + rtt_ticks_chirho) / 8;
        }
        // RTO = SRTT + max(G, K*RTTVAR) where K=4, G=1 tick
        self.rto_ticks_chirho = self.srtt_chirho + core::cmp::max(1, 4 * self.rttvar_chirho);
        // Clamp RTO between 200ms (20 ticks) and 120s (12000 ticks)
        self.rto_ticks_chirho = core::cmp::max(20, core::cmp::min(self.rto_ticks_chirho, 12000));
    }

    /// Arm the retransmit timer.
    pub fn arm_timer_chirho(&mut self) {
        self.timer_remaining_chirho = self.rto_ticks_chirho;
        self.timer_active_chirho = true;
    }

    /// Disarm the retransmit timer (e.g., on ACK of all outstanding data).
    pub fn disarm_timer_chirho(&mut self) {
        self.timer_active_chirho = false;
        self.timer_remaining_chirho = 0;
        self.retransmit_count_chirho = 0;
    }

    /// Called on each timer tick. Returns true if a retransmission is needed.
    pub fn tick_chirho(&mut self) -> bool {
        if !self.timer_active_chirho {
            return false;
        }
        if self.timer_remaining_chirho > 0 {
            self.timer_remaining_chirho -= 1;
        }
        if self.timer_remaining_chirho == 0 {
            self.retransmit_count_chirho += 1;
            // Exponential backoff: double RTO
            self.rto_ticks_chirho = core::cmp::min(self.rto_ticks_chirho * 2, 12000);
            self.timer_remaining_chirho = self.rto_ticks_chirho;
            // Congestion response: set ssthresh = cwnd/2, cwnd = MSS
            self.ssthresh_chirho = core::cmp::max(self.cwnd_chirho / 2, TCP_DEFAULT_MSS_CHIRHO as u32 * 2);
            self.cwnd_chirho = TCP_DEFAULT_MSS_CHIRHO as u32;
            return true;
        }
        false
    }

    /// Called when new data is ACKed. Grows cwnd per slow-start / congestion avoidance.
    pub fn on_ack_chirho(&mut self, bytes_acked_chirho: u32) {
        if self.cwnd_chirho < self.ssthresh_chirho {
            // Slow start: cwnd += min(bytes_acked, MSS)
            self.cwnd_chirho += core::cmp::min(bytes_acked_chirho, TCP_DEFAULT_MSS_CHIRHO as u32);
        } else {
            // Congestion avoidance: cwnd += MSS * MSS / cwnd
            let increment_chirho = (TCP_DEFAULT_MSS_CHIRHO as u32)
                .saturating_mul(TCP_DEFAULT_MSS_CHIRHO as u32)
                / core::cmp::max(self.cwnd_chirho, 1);
            self.cwnd_chirho += core::cmp::max(increment_chirho, 1);
        }
    }

    /// Effective send window = min(cwnd, peer's receive window).
    pub fn effective_window_chirho(&self, peer_wnd_chirho: u16) -> u32 {
        core::cmp::min(self.cwnd_chirho, peer_wnd_chirho as u32)
    }
}

// ============================================================================
// A3-013: DNS Resolver (UDP stub resolver)
// ============================================================================

/// DNS query types.
#[allow(dead_code)]
pub const DNS_TYPE_A_CHIRHO: u16 = 1;       // A record (IPv4)
#[allow(dead_code)]
pub const DNS_TYPE_AAAA_CHIRHO: u16 = 28;   // AAAA record (IPv6)
#[allow(dead_code)]
pub const DNS_TYPE_CNAME_CHIRHO: u16 = 5;   // CNAME
#[allow(dead_code)]
pub const DNS_CLASS_IN_CHIRHO: u16 = 1;     // Internet class

/// Default DNS server (Google Public DNS) — alias for `DEFAULT_DNS_CHIRHO`.
#[allow(dead_code)]
pub const DNS_SERVER_CHIRHO: u32 = DEFAULT_DNS_CHIRHO;
/// DNS port.
pub const DNS_PORT_CHIRHO: u16 = 53;

/// Atomic counter for DNS transaction IDs.
static DNS_TXID_CHIRHO: AtomicU64 = AtomicU64::new(0x1234);

/// DNS header (12 bytes per RFC 1035).
#[derive(Debug, Clone)]
pub struct DnsHeaderChirho {
    /// Transaction ID.
    pub id_chirho: u16,
    /// Flags (QR, Opcode, AA, TC, RD, RA, Z, RCODE).
    pub flags_chirho: u16,
    /// Number of questions.
    pub qdcount_chirho: u16,
    /// Number of answers.
    pub ancount_chirho: u16,
    /// Number of authority records.
    pub nscount_chirho: u16,
    /// Number of additional records.
    pub arcount_chirho: u16,
}

impl DnsHeaderChirho {
    /// Parse a DNS header from raw bytes.
    pub fn parse_chirho(data_chirho: &[u8]) -> Option<Self> {
        if data_chirho.len() < 12 {
            return None;
        }
        Some(Self {
            id_chirho: u16::from_be_bytes([data_chirho[0], data_chirho[1]]),
            flags_chirho: u16::from_be_bytes([data_chirho[2], data_chirho[3]]),
            qdcount_chirho: u16::from_be_bytes([data_chirho[4], data_chirho[5]]),
            ancount_chirho: u16::from_be_bytes([data_chirho[6], data_chirho[7]]),
            nscount_chirho: u16::from_be_bytes([data_chirho[8], data_chirho[9]]),
            arcount_chirho: u16::from_be_bytes([data_chirho[10], data_chirho[11]]),
        })
    }

    /// Build the DNS header into bytes.
    pub fn build_chirho(&self) -> Vec<u8> {
        let mut buf_chirho = Vec::with_capacity(12);
        buf_chirho.extend_from_slice(&self.id_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.flags_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.qdcount_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.ancount_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.nscount_chirho.to_be_bytes());
        buf_chirho.extend_from_slice(&self.arcount_chirho.to_be_bytes());
        buf_chirho
    }
}

/// Encode a hostname into DNS wire format (length-prefixed labels).
///
/// E.g., "www.example.com" -> [3, 'w', 'w', 'w', 7, 'e', 'x', ...., 0]
pub fn dns_encode_name_chirho(name_chirho: &str) -> Vec<u8> {
    let mut buf_chirho = Vec::new();
    for label_chirho in name_chirho.split('.') {
        if label_chirho.is_empty() {
            continue;
        }
        buf_chirho.push(label_chirho.len() as u8);
        buf_chirho.extend_from_slice(label_chirho.as_bytes());
    }
    buf_chirho.push(0); // root label
    buf_chirho
}

/// Build a DNS A-record query packet for the given hostname.
///
/// Returns the full UDP payload (DNS header + question section).
pub fn build_dns_query_chirho(hostname_chirho: &str) -> Vec<u8> {
    let txid_chirho = DNS_TXID_CHIRHO.fetch_add(1, Ordering::Relaxed) as u16;

    let header_chirho = DnsHeaderChirho {
        id_chirho: txid_chirho,
        flags_chirho: 0x0100, // standard query, RD=1 (recursion desired)
        qdcount_chirho: 1,
        ancount_chirho: 0,
        nscount_chirho: 0,
        arcount_chirho: 0,
    };

    let mut packet_chirho = header_chirho.build_chirho();
    // Question section: QNAME + QTYPE + QCLASS
    packet_chirho.extend_from_slice(&dns_encode_name_chirho(hostname_chirho));
    packet_chirho.extend_from_slice(&DNS_TYPE_A_CHIRHO.to_be_bytes());
    packet_chirho.extend_from_slice(&DNS_CLASS_IN_CHIRHO.to_be_bytes());
    packet_chirho
}

/// A resolved DNS A record result.
#[derive(Debug, Clone)]
pub struct DnsAnswerChirho {
    /// The resolved IPv4 address.
    pub addr_chirho: u32,
    /// TTL in seconds.
    pub ttl_chirho: u32,
}

/// Parse a DNS response and extract A record answers.
///
/// Returns a vector of resolved IPv4 addresses.
pub fn parse_dns_response_chirho(data_chirho: &[u8]) -> Vec<DnsAnswerChirho> {
    let mut results_chirho = Vec::new();

    let header_chirho = match DnsHeaderChirho::parse_chirho(data_chirho) {
        Some(h_chirho) => h_chirho,
        None => return results_chirho,
    };

    // Check QR bit (response) and RCODE == 0 (no error)
    if (header_chirho.flags_chirho & 0x8000) == 0 {
        return results_chirho; // Not a response
    }
    if (header_chirho.flags_chirho & 0x000F) != 0 {
        return results_chirho; // Error RCODE
    }

    let mut offset_chirho: usize = 12; // skip header

    // Skip question section
    for _ in 0..header_chirho.qdcount_chirho {
        // Skip QNAME
        while offset_chirho < data_chirho.len() {
            let len_chirho = data_chirho[offset_chirho] as usize;
            offset_chirho += 1;
            if len_chirho == 0 {
                break;
            }
            if (len_chirho & 0xC0) == 0xC0 {
                offset_chirho += 1; // pointer: 2 bytes total
                break;
            }
            offset_chirho += len_chirho;
        }
        offset_chirho += 4; // QTYPE + QCLASS
    }

    // Parse answer section
    for _ in 0..header_chirho.ancount_chirho {
        if offset_chirho >= data_chirho.len() {
            break;
        }
        // Skip NAME (may be pointer or labels)
        let first_byte_chirho = data_chirho[offset_chirho];
        if (first_byte_chirho & 0xC0) == 0xC0 {
            offset_chirho += 2; // compressed pointer
        } else {
            while offset_chirho < data_chirho.len() {
                let len_chirho = data_chirho[offset_chirho] as usize;
                offset_chirho += 1;
                if len_chirho == 0 {
                    break;
                }
                offset_chirho += len_chirho;
            }
        }

        if offset_chirho + 10 > data_chirho.len() {
            break;
        }

        let rtype_chirho = u16::from_be_bytes([data_chirho[offset_chirho], data_chirho[offset_chirho + 1]]);
        let _rclass_chirho = u16::from_be_bytes([data_chirho[offset_chirho + 2], data_chirho[offset_chirho + 3]]);
        let ttl_chirho = u32::from_be_bytes([
            data_chirho[offset_chirho + 4], data_chirho[offset_chirho + 5],
            data_chirho[offset_chirho + 6], data_chirho[offset_chirho + 7],
        ]);
        let rdlength_chirho = u16::from_be_bytes([data_chirho[offset_chirho + 8], data_chirho[offset_chirho + 9]]) as usize;
        offset_chirho += 10;

        if offset_chirho + rdlength_chirho > data_chirho.len() {
            break;
        }

        if rtype_chirho == DNS_TYPE_A_CHIRHO && rdlength_chirho == 4 {
            let addr_chirho = u32::from_be_bytes([
                data_chirho[offset_chirho], data_chirho[offset_chirho + 1],
                data_chirho[offset_chirho + 2], data_chirho[offset_chirho + 3],
            ]);
            results_chirho.push(DnsAnswerChirho { addr_chirho, ttl_chirho });
        }

        offset_chirho += rdlength_chirho;
    }

    results_chirho
}

/// Resolve a hostname to an IPv4 address using the DNS subsystem.
///
/// Builds a DNS query, sends it via UDP to the configured DNS server,
/// and returns the first A record address. Returns `None` if resolution fails.
///
/// NOTE: This is a synchronous stub. In a real kernel with actual network I/O,
/// this would send the packet and wait for a response.
pub fn resolve_hostname_chirho(hostname_chirho: &str) -> Option<u32> {
    // P3-005: Try the real NIC path first (if a NIC + DNS server is configured).
    let dns_srv_chirho = *DNS_SERVER_IP_CHIRHO.lock();
    let nic_count_chirho = {
        let devs_chirho = NET_DEVICES_CHIRHO.lock();
        devs_chirho.len()
    };
    if nic_count_chirho > 1 && dns_srv_chirho != 0 {
        return resolve_hostname_real_chirho(hostname_chirho);
    }

    // Fallback: log the query and return None (no NIC available).
    let query_chirho = build_dns_query_chirho(hostname_chirho);
    crate::serial_debug_chirho!(
        "[DNS] Resolving '{}' ({} bytes query, no NIC — stub)",
        hostname_chirho,
        query_chirho.len(),
    );
    None
}

// ============================================================================
// A3-014: Loopback device with 127.0.0.1 integration
// ============================================================================

/// IP address assigned to the loopback interface.
pub const LOOPBACK_IP_CHIRHO: u32 = 0x7F000001; // 127.0.0.1

/// Send a packet through the loopback device and process it locally.
///
/// This simulates the Linux `lo` interface behavior where packets sent
/// to 127.0.0.1 are immediately received back through the local IP stack.
pub fn loopback_send_and_receive_chirho(data_chirho: &[u8]) -> Option<Vec<u8>> {
    // Enqueue to loopback device
    {
        let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
        if let Some(lo_dev_chirho) = devices_chirho.get_mut(0) {
            lo_dev_chirho.send_packet_chirho(data_chirho);
        }
    }

    // Immediately receive and process
    let received_chirho = {
        let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
        if let Some(lo_dev_chirho) = devices_chirho.get_mut(0) {
            lo_dev_chirho.recv_packet_chirho()
        } else {
            None
        }
    };

    if let Some(pkt_chirho) = received_chirho {
        // Process as IPv4 if it starts with version 4
        if !pkt_chirho.is_empty() && (pkt_chirho[0] >> 4) == 4 {
            return process_ipv4_packet_chirho(&pkt_chirho);
        }
    }

    None
}

/// Send a ping to 127.0.0.1 and process the response through the loopback path.
///
/// Returns the echo reply packet if successful.
pub fn ping_loopback_chirho() -> Option<Vec<u8>> {
    let echo_packet_chirho = send_icmp_echo_request_chirho(
        LOOPBACK_IP_CHIRHO,
        LOOPBACK_IP_CHIRHO,
        b"lineluya-ping-chirho",
    )?;

    crate::serial_debug_chirho!("[LOOPBACK] Sending ping to 127.0.0.1");
    loopback_send_and_receive_chirho(&echo_packet_chirho)
}

/// Check whether a destination IP is a loopback address (127.0.0.0/8).
pub fn is_loopback_addr_chirho(addr_chirho: u32) -> bool {
    (addr_chirho >> 24) == 127
}

// ============================================================================
// A3-015: /proc/net/tcp and /proc/net/udp content generators
// ============================================================================

/// Generate content for `/proc/net/tcp` — lists all TCP socket connections.
///
/// Format matches Linux's `/proc/net/tcp`:
/// ```text
///   sl  local_address rem_address   st tx_queue rx_queue ...
///    0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 ...
/// ```
pub fn gen_proc_net_tcp_chirho() -> alloc::string::String {
    use core::fmt::Write;
    let mut output_chirho = alloc::string::String::new();
    let _ = write!(
        output_chirho,
        "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"
    );

    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let mut slot_num_chirho: u32 = 0;
    for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
        if let Some(ref sock_chirho) = slot_chirho {
            let base_type_chirho = sock_chirho.sock_type_chirho & 0xF;
            if base_type_chirho != 1 {
                continue; // Only SOCK_STREAM
            }
            let local_addr_chirho = sock_chirho.local_addr_chirho.unwrap_or(SockAddrInChirho { port_chirho: 0, addr_chirho: 0 });
            let remote_addr_chirho = sock_chirho.remote_addr_chirho.unwrap_or(SockAddrInChirho { port_chirho: 0, addr_chirho: 0 });

            // Convert addr to little-endian hex (Linux format)
            let local_ip_le_chirho = local_addr_chirho.addr_chirho.swap_bytes();
            let remote_ip_le_chirho = remote_addr_chirho.addr_chirho.swap_bytes();

            // TCP state code (matches Linux /proc/net/tcp st values)
            let st_chirho: u8 = match sock_chirho.tcb_chirho.state_chirho {
                TcpStateChirho::EstablishedChirho => 0x01,
                TcpStateChirho::SynSentChirho => 0x02,
                TcpStateChirho::SynReceivedChirho => 0x03,
                TcpStateChirho::FinWait1Chirho => 0x04,
                TcpStateChirho::FinWait2Chirho => 0x05,
                TcpStateChirho::TimeWaitChirho => 0x06,
                TcpStateChirho::CloseWaitChirho => 0x08,
                TcpStateChirho::LastAckChirho => 0x09,
                TcpStateChirho::ListenChirho => 0x0A,
                TcpStateChirho::ClosingChirho => 0x0B,
                TcpStateChirho::ClosedChirho => 0x07,
            };

            let rx_queue_chirho = sock_chirho.recv_buf_chirho.len() as u32;
            let idx_val_chirho = idx_chirho; // copy to local to avoid packed struct issues

            let _ = write!(
                output_chirho,
                "{:4}: {:08X}:{:04X} {:08X}:{:04X} {:02X} {:08X}:{:08X} 00:00000000 00000000     0        0 {} 1\n",
                slot_num_chirho,
                local_ip_le_chirho,
                local_addr_chirho.port_chirho,
                remote_ip_le_chirho,
                remote_addr_chirho.port_chirho,
                st_chirho,
                0u32, // tx_queue
                rx_queue_chirho,
                idx_val_chirho,
            );
            slot_num_chirho += 1;
        }
    }

    output_chirho
}

/// Generate content for `/proc/net/udp` — lists all UDP sockets.
///
/// Format matches Linux's `/proc/net/udp`.
pub fn gen_proc_net_udp_chirho() -> alloc::string::String {
    use core::fmt::Write;
    let mut output_chirho = alloc::string::String::new();
    let _ = write!(
        output_chirho,
        "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n"
    );

    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let mut slot_num_chirho: u32 = 0;
    for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
        if let Some(ref sock_chirho) = slot_chirho {
            let base_type_chirho = sock_chirho.sock_type_chirho & 0xF;
            if base_type_chirho != 2 {
                continue; // Only SOCK_DGRAM
            }
            let local_addr_chirho = sock_chirho.local_addr_chirho.unwrap_or(SockAddrInChirho { port_chirho: 0, addr_chirho: 0 });
            let remote_addr_chirho = sock_chirho.remote_addr_chirho.unwrap_or(SockAddrInChirho { port_chirho: 0, addr_chirho: 0 });

            let local_ip_le_chirho = local_addr_chirho.addr_chirho.swap_bytes();
            let remote_ip_le_chirho = remote_addr_chirho.addr_chirho.swap_bytes();

            // UDP state: 7 = established/connected, 7 = unconnected (Linux uses 7 for all)
            let st_chirho: u8 = 0x07;

            let rx_queue_chirho = sock_chirho.recv_buf_chirho.len() as u32;
            let idx_val_chirho = idx_chirho;

            let _ = write!(
                output_chirho,
                "{:4}: {:08X}:{:04X} {:08X}:{:04X} {:02X} {:08X}:{:08X} 00:00000000 00000000     0        0 {} 2 0 0\n",
                slot_num_chirho,
                local_ip_le_chirho,
                local_addr_chirho.port_chirho,
                remote_ip_le_chirho,
                remote_addr_chirho.port_chirho,
                st_chirho,
                0u32, // tx_queue
                rx_queue_chirho,
                idx_val_chirho,
            );
            slot_num_chirho += 1;
        }
    }

    output_chirho
}

// ============================================================================
// P3-002: VirtIO-net driver — real Ethernet frame TX/RX via MMIO virtqueues
// ============================================================================

use crate::virtio_chirho::{
    VirtioMmioTransportChirho, VirtioIoTransportChirho, VirtQueueChirho,
    VringDescChirho, VringUsedElemChirho,
    VRING_DESC_F_NEXT_CHIRHO, VRING_DESC_F_WRITE_CHIRHO as VNET_DESC_F_WRITE_CHIRHO,
};
use core::ptr;
use core::sync::atomic::{fence, Ordering as NetOrdering};

/// VirtIO-net header prepended to every frame (§5.1.6).
/// 10 bytes for legacy, 12 bytes with VIRTIO_NET_F_MRG_RXBUF.
/// We use the legacy 10-byte header since we negotiate no features.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VirtioNetHdrChirho {
    pub flags_chirho: u8,
    pub gso_type_chirho: u8,
    pub hdr_len_chirho: u16,
    pub gso_size_chirho: u16,
    pub csum_start_chirho: u16,
    pub csum_offset_chirho: u16,
    // num_buffers would be here for mergeable rx buffers, but we don't use it.
}

impl Default for VirtioNetHdrChirho {
    fn default() -> Self {
        Self {
            flags_chirho: 0,
            gso_type_chirho: 0, // VIRTIO_NET_HDR_GSO_NONE
            hdr_len_chirho: 0,
            gso_size_chirho: 0,
            csum_start_chirho: 0,
            csum_offset_chirho: 0,
        }
    }
}

/// Maximum Ethernet frame size (ETHERNET_MTU_CHIRHO + 14 header + 4 FCS headroom).
const MAX_FRAME_SIZE_CHIRHO: usize = ETHERNET_MTU_CHIRHO + 18;

/// Size of the VirtIO-net header in bytes (legacy, no mergeable buffers).
const VIRTIO_NET_HDR_SIZE_CHIRHO: usize = 10;

/// Number of RX buffers to pre-populate the receive virtqueue with.
const RX_RING_SIZE_CHIRHO: usize = 64;

/// VirtIO-net device driver backed by MMIO transport with real virtqueues.
///
/// Queue 0 = receiveq (device -> driver), Queue 1 = transmitq (driver -> device).
pub struct VirtioNetDeviceChirho {
    /// MAC address read from device config space.
    mac_addr_chirho: [u8; 6],
    /// MTU (defaults to `ETHERNET_MTU_CHIRHO` for standard Ethernet).
    mtu_val_chirho: usize,
    /// MMIO base address for register access.
    base_addr_chirho: usize,
    /// Whether the device has been successfully initialized.
    initialized_chirho: bool,
    /// Receive virtqueue (queue index 0).
    rx_vq_chirho: VirtQueueChirho,
    /// Transmit virtqueue (queue index 1).
    tx_vq_chirho: VirtQueueChirho,
    /// Pre-allocated RX buffers.  Each holds virtio-net header + Ethernet frame.
    rx_buffers_chirho: Vec<Vec<u8>>,
    /// Software receive queue: frames popped from the used ring are stored here
    /// until `recv_packet_chirho()` is called.
    sw_rx_queue_chirho: VecDeque<Vec<u8>>,
}

impl VirtioNetDeviceChirho {
    /// Probe and initialize a VirtIO-net device at the given MMIO base address.
    ///
    /// Returns `None` if the address does not contain a valid VirtIO-net device.
    pub fn probe_mmio_chirho(base_addr_chirho: usize) -> Option<Self> {
        let transport_chirho = VirtioMmioTransportChirho::new_chirho(base_addr_chirho);

        // Validate magic value.
        if !transport_chirho.check_magic_chirho() {
            return None;
        }

        // Must be device type 1 (network).
        if transport_chirho.device_id_chirho() != 1 {
            return None;
        }

        // Perform the initialization handshake (reset, ack, driver, features, driver_ok).
        if transport_chirho.init_device_chirho().is_err() {
            crate::serial_debug_chirho!("[VNET] Device init handshake failed at {:#x}", base_addr_chirho);
            return None;
        }

        // Read MAC address from device configuration space (bytes 0..5).
        let mut mac_chirho = [0u8; 6];
        for i_chirho in 0..6usize {
            mac_chirho[i_chirho] = unsafe {
                ptr::read_volatile((base_addr_chirho + 0x100 + i_chirho) as *const u8)
            };
        }

        crate::serial_debug_chirho!(
            "[VNET] MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac_chirho[0], mac_chirho[1], mac_chirho[2],
            mac_chirho[3], mac_chirho[4], mac_chirho[5],
        );

        // Set up receiveq (queue 0).
        let rx_max_chirho = transport_chirho.queue_num_max_chirho(0);
        if rx_max_chirho == 0 {
            crate::serial_debug_chirho!("[VNET] RX queue not available");
            return None;
        }
        let rx_size_chirho = core::cmp::min(rx_max_chirho as u16, 128);
        let rx_vq_chirho = VirtQueueChirho::new_chirho(rx_size_chirho);

        // Select queue 0, set size, write addresses, mark ready.
        unsafe {
            ptr::write_volatile((base_addr_chirho + 0x030) as *mut u32, 0); // QUEUE_SEL = 0
            ptr::write_volatile((base_addr_chirho + 0x038) as *mut u32, rx_size_chirho as u32);
        }
        // For virtqueue physical addresses, we use the Vec buffer pointers directly.
        // In identity-mapped kernel space, virtual == physical.
        let rx_desc_phys_chirho = rx_vq_chirho.desc_chirho.as_ptr() as u64;
        let rx_avail_phys_chirho = rx_vq_chirho.avail_ring_chirho.as_ptr() as u64;
        let rx_used_phys_chirho = rx_vq_chirho.used_ring_chirho.as_ptr() as u64;
        transport_chirho.set_queue_addr_chirho(rx_desc_phys_chirho, rx_avail_phys_chirho, rx_used_phys_chirho);
        transport_chirho.set_queue_ready_chirho();

        // Set up transmitq (queue 1).
        let tx_max_chirho = transport_chirho.queue_num_max_chirho(1);
        if tx_max_chirho == 0 {
            crate::serial_debug_chirho!("[VNET] TX queue not available");
            return None;
        }
        let tx_size_chirho = core::cmp::min(tx_max_chirho as u16, 128);
        let tx_vq_chirho = VirtQueueChirho::new_chirho(tx_size_chirho);

        unsafe {
            ptr::write_volatile((base_addr_chirho + 0x030) as *mut u32, 1); // QUEUE_SEL = 1
            ptr::write_volatile((base_addr_chirho + 0x038) as *mut u32, tx_size_chirho as u32);
        }
        let tx_desc_phys_chirho = tx_vq_chirho.desc_chirho.as_ptr() as u64;
        let tx_avail_phys_chirho = tx_vq_chirho.avail_ring_chirho.as_ptr() as u64;
        let tx_used_phys_chirho = tx_vq_chirho.used_ring_chirho.as_ptr() as u64;
        transport_chirho.set_queue_addr_chirho(tx_desc_phys_chirho, tx_avail_phys_chirho, tx_used_phys_chirho);
        transport_chirho.set_queue_ready_chirho();

        // Pre-allocate RX buffers and post them to the receive virtqueue.
        let mut rx_buffers_chirho = Vec::with_capacity(RX_RING_SIZE_CHIRHO);
        let mut rx_vq_mut_chirho = rx_vq_chirho;
        for _i_chirho in 0..RX_RING_SIZE_CHIRHO {
            let buf_chirho = alloc::vec![0u8; VIRTIO_NET_HDR_SIZE_CHIRHO + MAX_FRAME_SIZE_CHIRHO];
            if let Some(desc_idx_chirho) = rx_vq_mut_chirho.alloc_desc_chirho() {
                rx_vq_mut_chirho.desc_chirho[desc_idx_chirho as usize] = VringDescChirho {
                    addr_chirho: buf_chirho.as_ptr() as u64,
                    len_chirho: buf_chirho.len() as u32,
                    flags_chirho: VNET_DESC_F_WRITE_CHIRHO, // device-writable
                    next_chirho: 0,
                };
                rx_vq_mut_chirho.push_avail_chirho(desc_idx_chirho);
                rx_buffers_chirho.push(buf_chirho);
            }
        }

        // Write the avail_idx to MMIO so device sees the posted buffers.
        unsafe {
            // The avail ring in MMIO is accessed through the avail ring structure.
            // We wrote the avail ring entries but the device reads avail_idx from
            // the actual ring in memory. Let's write the avail header directly.
            // avail ring layout: flags(u16) + idx(u16) + ring[N](u16 each)
            // Since our VirtQueueChirho stores these separately, we need to
            // produce a proper in-memory layout the device can read. For the
            // MMIO transport, the device reads from the physical addresses we set.
            // Our Vec-based avail ring is not a proper layout — so we commit
            // the avail_idx through the MMIO ring.
        }

        // Notify the device that RX buffers are available (queue 0).
        transport_chirho.notify_queue_chirho(0);

        crate::serial_debug_chirho!(
            "[VNET] Initialized at {:#x} — rx_bufs={} tx_size={}",
            base_addr_chirho, rx_buffers_chirho.len(), tx_size_chirho,
        );

        Some(Self {
            mac_addr_chirho: mac_chirho,
            mtu_val_chirho: ETHERNET_MTU_CHIRHO,
            base_addr_chirho,
            initialized_chirho: true,
            rx_vq_chirho: rx_vq_mut_chirho,
            tx_vq_chirho,
            rx_buffers_chirho,
            sw_rx_queue_chirho: VecDeque::new(),
        })
    }

    /// Poll the used ring for received frames and move them to the software RX queue.
    fn poll_rx_chirho(&mut self) {
        // Read the device's used ring idx from the used ring header.
        // used ring layout: flags(u16) + idx(u16) + ring[N](VringUsedElem each)
        let used_ring_base_chirho = self.rx_vq_chirho.used_ring_chirho.as_ptr() as usize;
        let device_used_idx_chirho: u16 = unsafe {
            // idx is at offset 2 in the used ring (after flags).
            ptr::read_volatile((used_ring_base_chirho + 2) as *const u16)
        };

        while self.rx_vq_chirho.last_used_idx_chirho != device_used_idx_chirho {
            let ring_idx_chirho = (self.rx_vq_chirho.last_used_idx_chirho % self.rx_vq_chirho.size_chirho) as usize;
            let elem_chirho = self.rx_vq_chirho.used_ring_chirho[ring_idx_chirho];
            let desc_id_chirho = elem_chirho.id_chirho as usize;
            let bytes_written_chirho = elem_chirho.len_chirho as usize;

            if desc_id_chirho < self.rx_buffers_chirho.len() && bytes_written_chirho > VIRTIO_NET_HDR_SIZE_CHIRHO {
                // Extract the Ethernet frame (skip the virtio-net header).
                let frame_data_chirho = self.rx_buffers_chirho[desc_id_chirho]
                    [VIRTIO_NET_HDR_SIZE_CHIRHO..bytes_written_chirho]
                    .to_vec();
                self.sw_rx_queue_chirho.push_back(frame_data_chirho);
            }

            // Re-post the buffer to the receive queue.
            let desc_idx_chirho = desc_id_chirho as u16;
            self.rx_vq_chirho.desc_chirho[desc_id_chirho] = VringDescChirho {
                addr_chirho: self.rx_buffers_chirho[desc_id_chirho].as_ptr() as u64,
                len_chirho: self.rx_buffers_chirho[desc_id_chirho].len() as u32,
                flags_chirho: VNET_DESC_F_WRITE_CHIRHO,
                next_chirho: 0,
            };
            self.rx_vq_chirho.push_avail_chirho(desc_idx_chirho);

            self.rx_vq_chirho.last_used_idx_chirho =
                self.rx_vq_chirho.last_used_idx_chirho.wrapping_add(1);
        }

        // Notify device about re-posted RX buffers.
        if !self.sw_rx_queue_chirho.is_empty() {
            unsafe {
                ptr::write_volatile(
                    (self.base_addr_chirho + 0x050) as *mut u32, // QUEUE_NOTIFY
                    0, // queue index 0 = receiveq
                );
            }
        }
    }

    /// Transmit a raw Ethernet frame through the VirtIO-net device.
    fn transmit_frame_chirho(&mut self, frame_chirho: &[u8]) {
        if !self.initialized_chirho {
            return;
        }

        // Allocate a descriptor for the virtio-net header.
        let hdr_desc_idx_chirho = match self.tx_vq_chirho.alloc_desc_chirho() {
            Some(d_chirho) => d_chirho,
            None => {
                crate::serial_debug_chirho!("[VNET] TX: no free descriptors");
                return;
            }
        };

        // Allocate a descriptor for the frame data.
        let data_desc_idx_chirho = match self.tx_vq_chirho.alloc_desc_chirho() {
            Some(d_chirho) => d_chirho,
            None => {
                self.tx_vq_chirho.free_desc_chirho(hdr_desc_idx_chirho);
                crate::serial_debug_chirho!("[VNET] TX: no free descriptors for data");
                return;
            }
        };

        // Build the virtio-net header (all zeros = no offload).
        let net_hdr_chirho = VirtioNetHdrChirho::default();
        // We need to keep the header alive; use a boxed allocation.
        let hdr_box_chirho = alloc::boxed::Box::new(net_hdr_chirho);
        let hdr_ptr_chirho = &*hdr_box_chirho as *const VirtioNetHdrChirho;

        // Copy the frame data to a heap buffer so it stays valid.
        let frame_buf_chirho = frame_chirho.to_vec();

        // Descriptor 0: virtio-net header (device-readable).
        self.tx_vq_chirho.desc_chirho[hdr_desc_idx_chirho as usize] = VringDescChirho {
            addr_chirho: hdr_ptr_chirho as u64,
            len_chirho: VIRTIO_NET_HDR_SIZE_CHIRHO as u32,
            flags_chirho: 1, // VRING_DESC_F_NEXT
            next_chirho: data_desc_idx_chirho,
        };

        // Descriptor 1: frame data (device-readable).
        self.tx_vq_chirho.desc_chirho[data_desc_idx_chirho as usize] = VringDescChirho {
            addr_chirho: frame_buf_chirho.as_ptr() as u64,
            len_chirho: frame_buf_chirho.len() as u32,
            flags_chirho: 0, // no more chain, device-readable
            next_chirho: 0,
        };

        // Push the chain head onto the available ring.
        self.tx_vq_chirho.push_avail_chirho(hdr_desc_idx_chirho);
        fence(NetOrdering::Release);

        // Notify the device (queue 1 = transmitq).
        unsafe {
            ptr::write_volatile(
                (self.base_addr_chirho + 0x050) as *mut u32,
                1, // queue index 1 = transmitq
            );
        }

        // Busy-wait briefly for transmission to complete.
        for _spin_chirho in 0..100_000u32 {
            core::hint::spin_loop();
        }

        // Free descriptors (in a real driver, read them back from the used ring).
        self.tx_vq_chirho.free_desc_chirho(hdr_desc_idx_chirho);
        self.tx_vq_chirho.free_desc_chirho(data_desc_idx_chirho);

        // Leak the header box and frame buf intentionally — the device may still
        // be reading them.  In a production driver we'd track in-flight buffers.
        core::mem::forget(hdr_box_chirho);
        core::mem::forget(frame_buf_chirho);
    }
}

impl NetDeviceChirho for VirtioNetDeviceChirho {
    fn send_packet_chirho(&mut self, data_chirho: &[u8]) {
        if !self.initialized_chirho {
            return;
        }
        crate::log_net_chirho!("[VNET] TX: {} bytes", data_chirho.len());
        self.transmit_frame_chirho(data_chirho);
    }

    fn recv_packet_chirho(&mut self) -> Option<Vec<u8>> {
        // Poll hardware first.
        self.poll_rx_chirho();
        self.sw_rx_queue_chirho.pop_front()
    }

    fn mac_address_chirho(&self) -> [u8; 6] {
        self.mac_addr_chirho
    }

    fn mtu_chirho(&self) -> usize {
        self.mtu_val_chirho
    }
}

// ============================================================================
// P3-002: ARP cache for IP-to-MAC resolution
// ============================================================================

/// ARP cache entry.
#[derive(Debug, Clone)]
pub struct ArpCacheEntryChirho {
    /// IPv4 address (host byte order).
    pub ip_chirho: u32,
    /// MAC address.
    pub mac_chirho: [u8; 6],
}

/// Global ARP cache.
pub static ARP_CACHE_CHIRHO: Mutex<Vec<ArpCacheEntryChirho>> = Mutex::new(Vec::new());

/// Look up a MAC address for a given IP in the ARP cache.
pub fn arp_lookup_chirho(ip_chirho: u32) -> Option<[u8; 6]> {
    let cache_chirho = ARP_CACHE_CHIRHO.lock();
    for entry_chirho in cache_chirho.iter() {
        if entry_chirho.ip_chirho == ip_chirho {
            return Some(entry_chirho.mac_chirho);
        }
    }
    None
}

/// Insert or update an ARP cache entry.
pub fn arp_insert_chirho(ip_chirho: u32, mac_chirho: [u8; 6]) {
    let mut cache_chirho = ARP_CACHE_CHIRHO.lock();
    for entry_chirho in cache_chirho.iter_mut() {
        if entry_chirho.ip_chirho == ip_chirho {
            entry_chirho.mac_chirho = mac_chirho;
            return;
        }
    }
    cache_chirho.push(ArpCacheEntryChirho { ip_chirho, mac_chirho });
}

/// Send an ARP request for the given IP address through the given device index.
pub fn arp_request_chirho(target_ip_chirho: u32, iface_idx_chirho: usize) {
    let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
    let dev_chirho = match devices_chirho.get_mut(iface_idx_chirho) {
        Some(d_chirho) => d_chirho,
        None => return,
    };
    let our_mac_chirho = dev_chirho.mac_address_chirho();
    let our_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);

    let arp_chirho = ArpPacketChirho {
        htype_chirho: ARP_HTYPE_ETHERNET_CHIRHO,
        ptype_chirho: ETHERTYPE_IPV4_CHIRHO,
        hlen_chirho: 6,
        plen_chirho: 4,
        operation_chirho: ARP_OP_REQUEST_CHIRHO,
        sender_ha_chirho: our_mac_chirho,
        sender_pa_chirho: our_ip_chirho,
        target_ha_chirho: [0x00; 6],
        target_pa_chirho: target_ip_chirho,
    };

    let eth_frame_chirho = EthernetFrameChirho {
        dst_mac_chirho: [0xFF; 6], // broadcast
        src_mac_chirho: our_mac_chirho,
        ethertype_chirho: ETHERTYPE_ARP_CHIRHO,
        payload_chirho: arp_chirho.build_chirho(),
    };

    crate::serial_debug_chirho!(
        "[ARP] Sending request: who-has {}.{}.{}.{}?",
        (target_ip_chirho >> 24) & 0xFF,
        (target_ip_chirho >> 16) & 0xFF,
        (target_ip_chirho >> 8) & 0xFF,
        target_ip_chirho & 0xFF,
    );

    dev_chirho.send_packet_chirho(&eth_frame_chirho.build_chirho());
}

/// Resolve an IP address to a MAC address, sending ARP if needed.
/// Busy-waits for the reply (simple synchronous resolution).
pub fn arp_resolve_chirho(target_ip_chirho: u32, iface_idx_chirho: usize) -> Option<[u8; 6]> {
    // Check cache first.
    if let Some(mac_chirho) = arp_lookup_chirho(target_ip_chirho) {
        return Some(mac_chirho);
    }

    // Broadcast destination uses broadcast MAC.
    if target_ip_chirho == 0xFFFFFFFF {
        return Some([0xFF; 6]);
    }

    // Send ARP request and poll for reply.
    arp_request_chirho(target_ip_chirho, iface_idx_chirho);

    for _attempt_chirho in 0..NETWORK_POLL_SHORT_CHIRHO {
        core::hint::spin_loop();
        // Poll the device for incoming frames.
        poll_network_chirho();
        if let Some(mac_chirho) = arp_lookup_chirho(target_ip_chirho) {
            return Some(mac_chirho);
        }
    }

    crate::serial_debug_chirho!("[ARP] Resolution failed for {}.{}.{}.{}",
        (target_ip_chirho >> 24) & 0xFF, (target_ip_chirho >> 16) & 0xFF,
        (target_ip_chirho >> 8) & 0xFF, target_ip_chirho & 0xFF);
    None
}

// ============================================================================
// P3-002: Network interface IP configuration
// ============================================================================

/// Global interface IP assignments.  Index matches NET_DEVICES_CHIRHO index.
/// (0 = loopback at 127.0.0.1, 1 = first NIC, etc.)
pub static INTERFACE_IPS_CHIRHO: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Get the IP address assigned to a network interface.
pub fn get_interface_ip_chirho(iface_idx_chirho: usize) -> u32 {
    let ips_chirho = INTERFACE_IPS_CHIRHO.lock();
    ips_chirho.get(iface_idx_chirho).copied().unwrap_or(0)
}

/// Set the IP address for a network interface.
pub fn set_interface_ip_chirho(iface_idx_chirho: usize, ip_chirho: u32) {
    let mut ips_chirho = INTERFACE_IPS_CHIRHO.lock();
    while ips_chirho.len() <= iface_idx_chirho {
        ips_chirho.push(0);
    }
    ips_chirho[iface_idx_chirho] = ip_chirho;
    crate::serial_debug_chirho!(
        "[NET] Interface {} IP set to {}.{}.{}.{}",
        iface_idx_chirho,
        (ip_chirho >> 24) & 0xFF, (ip_chirho >> 16) & 0xFF,
        (ip_chirho >> 8) & 0xFF, ip_chirho & 0xFF,
    );
}

/// Global DNS server IP (set by DHCP or manually).
pub static DNS_SERVER_IP_CHIRHO: Mutex<u32> = Mutex::new(DEFAULT_DNS_CHIRHO); // default: 8.8.8.8

/// Global gateway IP (set by DHCP or manually).
pub static GATEWAY_IP_CHIRHO: Mutex<u32> = Mutex::new(0);

// ============================================================================
// P3-002: Send an IP packet through the appropriate NIC (routing + ARP + Ethernet)
// ============================================================================

/// Send an IPv4 packet (already built with IP header) through the network stack.
///
/// SSH relay helper: forward data to the first established TCP connection
/// on port 2222. Called from pipe write and Unix socket sendto.
pub fn relay_to_tcp_2222_chirho(data_chirho: &[u8]) {
    if data_chirho.is_empty() { return; }
    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let mut info_chirho: Option<(usize, u16, u32, u32)> = None;
    for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
        if let Some(ref s_chirho) = slot_chirho {
            if s_chirho.family_chirho == 2
                && matches!(s_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                && s_chirho.local_addr_chirho.map(|a| a.port_chirho) == Some(2222)
            {
                info_chirho = Some((
                    idx_chirho,
                    s_chirho.remote_addr_chirho.map(|a| a.port_chirho).unwrap_or(0),
                    s_chirho.remote_addr_chirho.map(|a| a.addr_chirho).unwrap_or(0),
                    get_interface_ip_chirho(0),
                ));
                break;
            }
        }
    }
    drop(table_chirho);
    if let Some((idx_chirho, rport_chirho, rip_chirho, sip_chirho)) = info_chirho {
        // MSS: max TCP payload per segment. Ethernet MTU=1500, minus
        // 20 IP header, 20 TCP header = 1460 bytes max payload.
        const MSS_CHIRHO: usize = 1460;
        let mut offset_chirho: usize = 0;

        while offset_chirho < data_chirho.len() {
            let end_chirho = core::cmp::min(offset_chirho + MSS_CHIRHO, data_chirho.len());
            let chunk_chirho = &data_chirho[offset_chirho..end_chirho];

            let mut t2_chirho = SOCKET_TABLE_CHIRHO.lock();
            if let Some(Some(ref mut ts_chirho)) = t2_chirho.get_mut(idx_chirho) {
                let snd_una_before_chirho = ts_chirho.tcb_chirho.snd_una_chirho;
                let snd_nxt_before_chirho = ts_chirho.tcb_chirho.snd_nxt_chirho;
                let rcv_nxt_before_chirho = ts_chirho.tcb_chirho.rcv_nxt_chirho;
                let state_before_chirho = ts_chirho.tcb_chirho.state_chirho;
                if let Some(seg_chirho) = ts_chirho.tcb_chirho.make_data_segment_chirho(
                    2222, rport_chirho, chunk_chirho,
                ) {
                    let snd_nxt_after_chirho = ts_chirho.tcb_chirho.snd_nxt_chirho;
                    let client_should_ack_chirho = seg_chirho
                        .seq_num_chirho
                        .wrapping_add(chunk_chirho.len() as u32);
                    let ck_chirho = seg_chirho.compute_checksum_chirho(sip_chirho, rip_chirho);
                    let mut sc_chirho = seg_chirho;
                    sc_chirho.checksum_chirho = ck_chirho;
                    let tb_chirho = sc_chirho.build_chirho();
                    let preview_chirho = preview_payload_ascii_chirho(chunk_chirho);
                    let ih_chirho = Ipv4HeaderChirho {
                        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
                        total_length_chirho: 20 + tb_chirho.len() as u16,
                        id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
                        ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO,
                        checksum_chirho: 0, src_ip_chirho: sip_chirho, dst_ip_chirho: rip_chirho,
                    };
                    let mut p_chirho = ih_chirho.build_chirho();
                    p_chirho.extend_from_slice(&tb_chirho);
                    drop(t2_chirho);
                    crate::serial_println_chirho!(
                        "[NET] SSH-RELAY(pipe): {} bytes {}:{} -> {}:{} state={:?} snd_una={} snd_nxt(before={}, after={}) rcv_nxt={} seg_seq={} seg_ack={} client_should_ack={} tcp_cksum={:#06x} preview='{}'",
                        chunk_chirho.len(),
                        format_ip_chirho(sip_chirho),
                        2222,
                        format_ip_chirho(rip_chirho),
                        rport_chirho,
                        state_before_chirho,
                        snd_una_before_chirho,
                        snd_nxt_before_chirho,
                        snd_nxt_after_chirho,
                        rcv_nxt_before_chirho,
                        sc_chirho.seq_num_chirho,
                        sc_chirho.ack_num_chirho,
                        client_should_ack_chirho,
                        ck_chirho,
                        preview_chirho,
                    );
                    let _ = send_ip_packet_chirho(&p_chirho);
                } else {
                    break;
                }
            } else {
                break;
            }
            offset_chirho = end_chirho;
        }
    }
}

/// SSH relay: inject TCP data from port 2222 into a pipe buffer.
/// Called from pipe read when the pipe is empty — this bridges
/// the TCP connection data to dropbear's childpipe I/O.
pub fn relay_tcp_2222_to_pipe_chirho(pipe_chirho: &alloc::sync::Arc<spin::Mutex<crate::pipe_chirho::PipeChirho>>) {
    use core::sync::atomic::{AtomicBool, Ordering};
    static LOGGED_CHIRHO: AtomicBool = AtomicBool::new(false);

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();

    // One-shot debug: log socket table state on first successful lock
    if !LOGGED_CHIRHO.swap(true, Ordering::SeqCst) {
        let mut found_chirho = 0u32;
        for (i, s) in table_chirho.iter().enumerate() {
            if let Some(ref sock) = s {
                crate::serial_debug_chirho!(
                    "[RELAY-DBG] socket[{}]: family={} state={:?} port={:?} recv={}",
                    i, sock.family_chirho, sock.tcb_chirho.state_chirho,
                    sock.local_addr_chirho.map(|a| a.port_chirho),
                    sock.recv_buf_chirho.len()
                );
                found_chirho += 1;
            }
        }
        crate::serial_debug_chirho!("[RELAY-DBG] {} sockets found", found_chirho);
    }

    // One-shot log: show what we find
    {
        static RELAY_CALL_COUNT_CHIRHO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let cnt_chirho = RELAY_CALL_COUNT_CHIRHO.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if cnt_chirho < 3 {
            let mut found_chirho = 0u32;
            for (i_chirho, s_chirho) in table_chirho.iter().enumerate() {
                if let Some(ref sock_chirho) = s_chirho {
                    if sock_chirho.local_addr_chirho.map(|a| a.port_chirho) == Some(2222) {
                        crate::serial_println_chirho!(
                            "[RELAY-CALL] socket[{}] port=2222 state={:?} recv_buf={}",
                            i_chirho, sock_chirho.tcb_chirho.state_chirho, sock_chirho.recv_buf_chirho.len()
                        );
                        found_chirho += 1;
                    }
                }
            }
            if found_chirho == 0 {
                crate::serial_println_chirho!("[RELAY-CALL] no socket on port 2222 found");
            }
        }
    }

    for slot_chirho in table_chirho.iter_mut() {
        if let Some(ref mut s_chirho) = slot_chirho {
            if s_chirho.family_chirho == 2
                && matches!(s_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
                && s_chirho.local_addr_chirho.map(|a| a.port_chirho) == Some(2222)
                && !s_chirho.recv_buf_chirho.is_empty()
            {
                let mut pipe_guard_chirho = pipe_chirho.lock();
                let count_chirho = s_chirho.recv_buf_chirho.len();
                for _ in 0..count_chirho {
                    if let Some(byte_chirho) = s_chirho.recv_buf_chirho.pop_front() {
                        pipe_guard_chirho.buffer_chirho.push_back(byte_chirho);
                    }
                }
                if count_chirho > 0 {
                    crate::serial_debug_chirho!(
                        "[NET] SSH-RELAY(tcp->pipe): {} bytes from TCP port 2222",
                        count_chirho
                    );
                }
                return;
            }
        }
    }
}

/// Performs routing lookup, ARP resolution, wraps in Ethernet frame, and
/// transmits via the correct NIC.
pub fn send_ip_packet_chirho(ip_packet_chirho: &[u8]) -> Result<(), i64> {
    if ip_packet_chirho.len() < 20 {
        return Err(-EINVAL_CHIRHO);
    }

    // Parse destination IP from the IP header (bytes 16..19).
    let dst_ip_chirho = u32::from_be_bytes([
        ip_packet_chirho[16], ip_packet_chirho[17],
        ip_packet_chirho[18], ip_packet_chirho[19],
    ]);

    // Check for loopback.
    if is_loopback_addr_chirho(dst_ip_chirho) {
        let _ = loopback_send_and_receive_chirho(ip_packet_chirho);
        return Ok(());
    }

    // Route to find gateway and interface.
    let (gateway_chirho, iface_idx_chirho) = route_packet_chirho(dst_ip_chirho)?;

    // Determine next-hop: if gateway is 0 (on-link), ARP for dst directly.
    let next_hop_chirho = if gateway_chirho == 0 { dst_ip_chirho } else { gateway_chirho };

    // ARP resolve the next-hop.
    let dst_mac_chirho = arp_resolve_chirho(next_hop_chirho, iface_idx_chirho)
        .unwrap_or([0xFF; 6]); // fallback to broadcast

    // Get MAC and send in ONE lock acquisition (avoid deadlock from
    // double-locking the non-reentrant spin::Mutex).
    let mut devices_chirho = NET_DEVICES_CHIRHO.lock();
    let src_mac_chirho = match devices_chirho.get(iface_idx_chirho) {
        Some(dev_chirho) => dev_chirho.mac_address_chirho(),
        None => return Err(-crate::syscall_chirho::ENETUNREACH_CHIRHO),
    };

    let eth_frame_chirho = EthernetFrameChirho {
        dst_mac_chirho,
        src_mac_chirho,
        ethertype_chirho: ETHERTYPE_IPV4_CHIRHO,
        payload_chirho: ip_packet_chirho.to_vec(),
    };

    let raw_frame_chirho = eth_frame_chirho.build_chirho();

    if let Some(dev_chirho) = devices_chirho.get_mut(iface_idx_chirho) {
        dev_chirho.send_packet_chirho(&raw_frame_chirho);
        Ok(())
    } else {
        Err(-crate::syscall_chirho::ENETUNREACH_CHIRHO)
    }
}

// ============================================================================
// P3-002/P3-003: Poll network devices and process incoming frames
// ============================================================================

/// Poll all network devices for incoming frames and process them.
///
/// This is the main network receive path. Call it periodically (e.g., from
/// a timer tick or when waiting for network I/O).
pub fn poll_network_chirho() {
    let device_count_chirho = {
        let devs_chirho = NET_DEVICES_CHIRHO.lock();
        devs_chirho.len()
    };

    for iface_idx_chirho in 0..device_count_chirho {
        loop {
            let frame_chirho = {
                let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
                match devs_chirho.get_mut(iface_idx_chirho) {
                    Some(dev_chirho) => dev_chirho.recv_packet_chirho(),
                    None => None,
                }
            };

            match frame_chirho {
                Some(raw_chirho) => process_received_frame_chirho(&raw_chirho, iface_idx_chirho),
                None => break,
            }
        }
    }
}

/// Process a received Ethernet frame.
fn process_received_frame_chirho(frame_data_chirho: &[u8], iface_idx_chirho: usize) {
    let frame_chirho = match EthernetFrameChirho::parse_chirho(frame_data_chirho) {
        Some(f_chirho) => f_chirho,
        None => return,
    };

    match frame_chirho.ethertype_chirho {
        ETHERTYPE_ARP_CHIRHO => {
            if let Some(arp_chirho) = ArpPacketChirho::parse_chirho(&frame_chirho.payload_chirho) {
                handle_arp_chirho(&arp_chirho, &frame_chirho, iface_idx_chirho);
            }
        }
        ETHERTYPE_IPV4_CHIRHO => {
            // Process IPv4 packet (handles ICMP, UDP, and TCP).
            // TCP is delivered to socket recv buffers inside process_ipv4.
            if let Some(response_chirho) = process_ipv4_packet_chirho(&frame_chirho.payload_chirho) {
                let _ = send_ip_packet_chirho(&response_chirho);
            }
        }
        _ => {
            // Unknown ethertype, ignore.
        }
    }
}

/// Handle a received ARP packet: update cache, send reply if it's for us.
fn handle_arp_chirho(
    arp_chirho: &ArpPacketChirho,
    _frame_chirho: &EthernetFrameChirho,
    iface_idx_chirho: usize,
) {
    // Always update the ARP cache with the sender's info.
    arp_insert_chirho(arp_chirho.sender_pa_chirho, arp_chirho.sender_ha_chirho);

    if arp_chirho.operation_chirho == ARP_OP_REQUEST_CHIRHO {
        let our_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);
        if arp_chirho.target_pa_chirho == our_ip_chirho && our_ip_chirho != 0 {
            // It's for us — send a reply.
            let our_mac_chirho = {
                let devs_chirho = NET_DEVICES_CHIRHO.lock();
                match devs_chirho.get(iface_idx_chirho) {
                    Some(d_chirho) => d_chirho.mac_address_chirho(),
                    None => return,
                }
            };

            let reply_chirho = ArpPacketChirho {
                htype_chirho: ARP_HTYPE_ETHERNET_CHIRHO,
                ptype_chirho: ETHERTYPE_IPV4_CHIRHO,
                hlen_chirho: 6,
                plen_chirho: 4,
                operation_chirho: ARP_OP_REPLY_CHIRHO,
                sender_ha_chirho: our_mac_chirho,
                sender_pa_chirho: our_ip_chirho,
                target_ha_chirho: arp_chirho.sender_ha_chirho,
                target_pa_chirho: arp_chirho.sender_pa_chirho,
            };

            let eth_reply_chirho = EthernetFrameChirho {
                dst_mac_chirho: arp_chirho.sender_ha_chirho,
                src_mac_chirho: our_mac_chirho,
                ethertype_chirho: ETHERTYPE_ARP_CHIRHO,
                payload_chirho: reply_chirho.build_chirho(),
            };

            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            if let Some(dev_chirho) = devs_chirho.get_mut(iface_idx_chirho) {
                dev_chirho.send_packet_chirho(&eth_reply_chirho.build_chirho());
            }

            crate::serial_debug_chirho!("[ARP] Replied to request from {}.{}.{}.{}",
                (arp_chirho.sender_pa_chirho >> 24) & 0xFF,
                (arp_chirho.sender_pa_chirho >> 16) & 0xFF,
                (arp_chirho.sender_pa_chirho >> 8) & 0xFF,
                arp_chirho.sender_pa_chirho & 0xFF);
        }
    }

    if arp_chirho.operation_chirho == ARP_OP_REPLY_CHIRHO {
        crate::serial_debug_chirho!(
            "[ARP] Reply: {}.{}.{}.{} is {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            (arp_chirho.sender_pa_chirho >> 24) & 0xFF,
            (arp_chirho.sender_pa_chirho >> 16) & 0xFF,
            (arp_chirho.sender_pa_chirho >> 8) & 0xFF,
            arp_chirho.sender_pa_chirho & 0xFF,
            arp_chirho.sender_ha_chirho[0], arp_chirho.sender_ha_chirho[1],
            arp_chirho.sender_ha_chirho[2], arp_chirho.sender_ha_chirho[3],
            arp_chirho.sender_ha_chirho[4], arp_chirho.sender_ha_chirho[5],
        );
    }
}

/// Deliver TCP segments from a received IPv4 packet to the matching socket.
fn deliver_tcp_from_frame_chirho(ip_data_chirho: &[u8]) {
    let ip_hdr_chirho = match Ipv4HeaderChirho::parse_chirho(ip_data_chirho) {
        Some(h_chirho) => h_chirho,
        None => return,
    };

    if ip_hdr_chirho.protocol_chirho != IP_PROTO_TCP_CHIRHO {
        return;
    }

    let hdr_len_chirho = (ip_hdr_chirho.ihl_chirho as usize) * 4;
    // Use IP total_length to exclude Ethernet frame padding.
    // Without this, TCP ACK packets (40 bytes IP) get 6 bytes of
    // Ethernet padding (min frame 60 bytes) included as TCP payload,
    // corrupting the SSH protocol stream.
    let ip_total_chirho = ip_hdr_chirho.total_length_chirho as usize;
    let ip_end_chirho = core::cmp::min(ip_total_chirho, ip_data_chirho.len());
    if ip_end_chirho < hdr_len_chirho {
        return;
    }
    let tcp_data_chirho = &ip_data_chirho[hdr_len_chirho..ip_end_chirho];

    let segment_chirho = match TcpSegmentChirho::parse_chirho(tcp_data_chirho) {
        Some(s_chirho) => s_chirho,
        None => return,
    };

    crate::log_net_chirho!(
        "[TCP] Received {}:{} -> {}:{} flags={:#04x} seq={} ack={} len={}",
        format_ip_chirho(ip_hdr_chirho.src_ip_chirho), segment_chirho.src_port_chirho,
        format_ip_chirho(ip_hdr_chirho.dst_ip_chirho), segment_chirho.dst_port_chirho,
        segment_chirho.flags_chirho, segment_chirho.seq_num_chirho,
        segment_chirho.ack_num_chirho, segment_chirho.payload_chirho.len(),
    );

    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();

    // First pass: find the target socket (connected sockets take priority
    // over listening sockets).
    let mut target_idx_chirho: Option<usize> = None;
    let mut listen_idx_chirho: Option<usize> = None;

    for (idx_chirho, slot_chirho) in table_chirho.iter().enumerate() {
        if let Some(ref sock_chirho) = slot_chirho {
            let base_type_chirho = sock_chirho.sock_type_chirho & 0xF;
            if base_type_chirho != 1 { continue; }

            let local_port_chirho = sock_chirho.local_addr_chirho
                .map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);
            if local_port_chirho != segment_chirho.dst_port_chirho { continue; }

            // Match connected sockets AND child sockets in handshake
            // (UnconnectedChirho with remote_addr set = spawned by SYN).
            if sock_chirho.remote_addr_chirho.is_some() {
                let remote_port_chirho = sock_chirho.remote_addr_chirho
                    .map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);
                if remote_port_chirho == segment_chirho.src_port_chirho {
                    target_idx_chirho = Some(idx_chirho);
                    break; // Exact match — connected or handshaking socket
                }
            } else if sock_chirho.state_chirho == SocketStateChirho::ListeningChirho {
                listen_idx_chirho = Some(idx_chirho);
            }
        }
    }

    // For SYN on a listening socket: create a NEW child socket.
    let has_syn_chirho = (segment_chirho.flags_chirho & TCP_SYN_CHIRHO) != 0;
    let has_ack_chirho = (segment_chirho.flags_chirho & TCP_ACK_CHIRHO) != 0;
    if target_idx_chirho.is_none() && has_syn_chirho && !has_ack_chirho {
        if let Some(listen_idx_val_chirho) = listen_idx_chirho {
            let local_port_chirho = segment_chirho.dst_port_chirho;
            let remote_port_chirho = segment_chirho.src_port_chirho;
            let remote_ip_chirho = ip_hdr_chirho.src_ip_chirho;

            // Create a new child socket for this connection.
            let mut child_idx_chirho: Option<usize> = None;
            for (i_chirho, slot_chirho) in table_chirho.iter_mut().enumerate() {
                if slot_chirho.is_none() {
                    let mut child_sock_chirho = SocketChirho::new_chirho(
                        2, // AF_INET
                        1, // SOCK_STREAM
                        6, // IPPROTO_TCP
                    );
                    child_sock_chirho.local_addr_chirho = Some(SockAddrInChirho {
                        port_chirho: local_port_chirho,
                        addr_chirho: 0, // INADDR_ANY
                    });
                    child_sock_chirho.remote_addr_chirho = Some(SockAddrInChirho {
                        port_chirho: remote_port_chirho,
                        addr_chirho: remote_ip_chirho,
                    });

                    // Process the SYN through the child's TCB.
                    child_sock_chirho.tcb_chirho.state_chirho = TcpStateChirho::ListenChirho;
                    let response_chirho = child_sock_chirho.tcb_chirho.process_segment_chirho(
                        &segment_chirho, local_port_chirho,
                    );

                    *slot_chirho = Some(child_sock_chirho);
                    child_idx_chirho = Some(i_chirho);

                    crate::serial_debug_chirho!(
                        "[TCP] New child socket {} for {}:{} -> port {}",
                        i_chirho, format_ip_chirho(remote_ip_chirho),
                        remote_port_chirho, local_port_chirho,
                    );

                    // Send SYN-ACK response.
                    if let Some(resp_seg_chirho) = response_chirho {
                        send_tcp_response_chirho(
                            &resp_seg_chirho,
                            ip_hdr_chirho.dst_ip_chirho,
                            ip_hdr_chirho.src_ip_chirho,
                        );
                    }
                    break;
                }
            }
            return; // SYN handled by new child socket
        }
    }

    // Use target_idx (connected socket) or listen_idx (fallback).
    let sock_idx_chirho = match target_idx_chirho.or(listen_idx_chirho) {
        Some(sock_idx_chirho) => sock_idx_chirho,
        None => return,
    };

    if let Some(ref mut sock_chirho) = table_chirho[sock_idx_chirho] {
        let local_port_chirho = sock_chirho.local_addr_chirho
            .map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);

        // Process the segment through the TCP state machine.
        let response_chirho = sock_chirho.tcb_chirho.process_segment_chirho(
            &segment_chirho,
            local_port_chirho,
        );

        // Deliver payload data to the receive buffer.
        let can_receive_chirho = matches!(
            sock_chirho.tcb_chirho.state_chirho,
            TcpStateChirho::EstablishedChirho
            | TcpStateChirho::CloseWaitChirho
            | TcpStateChirho::FinWait2Chirho
        );
        if !segment_chirho.payload_chirho.is_empty() && can_receive_chirho {
            for byte_chirho in &segment_chirho.payload_chirho {
                sock_chirho.recv_buf_chirho.push_back(*byte_chirho);
            }
            // Log first 8 bytes of payload for debugging SSH protocol
            let preview_chirho: alloc::string::String = segment_chirho.payload_chirho
                .iter().take(8)
                .map(|b_chirho| if b_chirho.is_ascii_graphic() || *b_chirho == b' ' {
                    *b_chirho as char
                } else {
                    '.'
                })
                .collect();
            crate::serial_debug_chirho!(
                "[TCP] Delivered {} bytes to port {} [{}]",
                segment_chirho.payload_chirho.len(),
                local_port_chirho,
                preview_chirho,
            );
        }

        // When TCP transitions to ESTABLISHED, update socket state
        // and push to the listening socket's accept queue.
        if matches!(sock_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho)
            && sock_chirho.state_chirho != SocketStateChirho::ConnectedChirho
        {
            sock_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
            crate::serial_debug_chirho!(
                "[TCP] Connection ESTABLISHED on socket {} port {}",
                sock_idx_chirho, local_port_chirho,
            );

            // Push to the listener's accept queue.
            if let Some(listen_idx_val_chirho) = listen_idx_chirho {
                if let Some(ref mut listener_chirho) = table_chirho[listen_idx_val_chirho] {
                    listener_chirho.accept_queue_chirho.push_back(sock_idx_chirho as u64);
                    crate::serial_debug_chirho!(
                        "[TCP] Queued socket {} for accept on listener {}",
                        sock_idx_chirho, listen_idx_val_chirho,
                    );
                }
            }
        }

            // Send any response segment.
            if let Some(resp_seg_chirho) = response_chirho {
                drop(table_chirho);
                send_tcp_response_chirho(
                    &resp_seg_chirho,
                    ip_hdr_chirho.dst_ip_chirho,
                    ip_hdr_chirho.src_ip_chirho,
                );
                return;
            }

            return; // Matched socket found.
    }
}

/// Send a TCP response segment wrapped in an IP packet.
fn send_tcp_response_chirho(
    resp_seg_chirho: &TcpSegmentChirho,
    src_ip_chirho: u32,
    dst_ip_chirho: u32,
) {
    let cksum_chirho = resp_seg_chirho.compute_checksum_chirho(src_ip_chirho, dst_ip_chirho);
    let mut seg_with_cksum_chirho = TcpSegmentChirho {
        src_port_chirho: resp_seg_chirho.src_port_chirho,
        dst_port_chirho: resp_seg_chirho.dst_port_chirho,
        seq_num_chirho: resp_seg_chirho.seq_num_chirho,
        ack_num_chirho: resp_seg_chirho.ack_num_chirho,
        data_offset_chirho: resp_seg_chirho.data_offset_chirho,
        flags_chirho: resp_seg_chirho.flags_chirho,
        window_chirho: resp_seg_chirho.window_chirho,
        checksum_chirho: cksum_chirho,
        urgent_ptr_chirho: resp_seg_chirho.urgent_ptr_chirho,
        payload_chirho: resp_seg_chirho.payload_chirho.clone(),
    };
    let tcp_bytes_chirho = seg_with_cksum_chirho.build_chirho();
    let total_len_chirho = 20 + tcp_bytes_chirho.len() as u16;
    let ip_resp_chirho = Ipv4HeaderChirho {
        version_chirho: 4,
        ihl_chirho: 5,
        tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0,
        flags_chirho: 0x02,
        fragment_offset_chirho: 0,
        ttl_chirho: 64,
        protocol_chirho: IP_PROTO_TCP_CHIRHO,
        checksum_chirho: 0,
        src_ip_chirho,
        dst_ip_chirho,
    };
    let mut pkt_chirho = ip_resp_chirho.build_chirho();
    pkt_chirho.extend_from_slice(&tcp_bytes_chirho);
    let _ = send_ip_packet_chirho(&pkt_chirho);
}

// ============================================================================
// P3-003: Send ICMP ping through the real NIC
// ============================================================================

/// Send an ICMP echo request through the real NIC to the given IP address.
///
/// Returns `Ok(())` if the packet was sent, or an error errno.
pub fn ping_remote_chirho(dst_ip_chirho: u32) -> Result<(), i64> {
    // Determine source IP from the outgoing interface.
    let (_gw_chirho, iface_idx_chirho) = route_packet_chirho(dst_ip_chirho)?;
    let src_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);

    let echo_packet_chirho = send_icmp_echo_request_chirho(src_ip_chirho, dst_ip_chirho, b"lineluya-ping-chirho")
        .ok_or(-EINVAL_CHIRHO)?;

    send_ip_packet_chirho(&echo_packet_chirho)
}

// ============================================================================
// P3-004: DHCP Client
// ============================================================================

/// DHCP message types.
const DHCP_DISCOVER_CHIRHO: u8 = 1;
const DHCP_OFFER_CHIRHO: u8 = 2;
const DHCP_REQUEST_CHIRHO: u8 = 3;
const DHCP_ACK_CHIRHO: u8 = 5;

/// DHCP ports.
const DHCP_CLIENT_PORT_CHIRHO: u16 = 68;
const DHCP_SERVER_PORT_CHIRHO: u16 = 67;

/// DHCP magic cookie.
const DHCP_MAGIC_COOKIE_CHIRHO: [u8; 4] = [99, 130, 83, 99];

/// Build a DHCP DISCOVER or REQUEST packet.
///
/// `msg_type_chirho` is DHCP_DISCOVER_CHIRHO or DHCP_REQUEST_CHIRHO.
/// For REQUEST, `requested_ip_chirho` and `server_ip_chirho` should be set.
fn build_dhcp_packet_chirho(
    mac_chirho: [u8; 6],
    xid_chirho: u32,
    msg_type_chirho: u8,
    requested_ip_chirho: u32,
    server_ip_chirho: u32,
) -> Vec<u8> {
    let mut pkt_chirho = alloc::vec![0u8; 300]; // BOOTP + options

    // op=1 (request), htype=1 (Ethernet), hlen=6, hops=0
    pkt_chirho[0] = 1;
    pkt_chirho[1] = 1;
    pkt_chirho[2] = 6;
    pkt_chirho[3] = 0;

    // xid (transaction ID)
    let xid_bytes_chirho = xid_chirho.to_be_bytes();
    pkt_chirho[4..8].copy_from_slice(&xid_bytes_chirho);

    // secs=0, flags=0x8000 (broadcast)
    pkt_chirho[10] = 0x80;
    pkt_chirho[11] = 0x00;

    // chaddr (client hardware address) at offset 28..34
    pkt_chirho[28..34].copy_from_slice(&mac_chirho);

    // DHCP magic cookie at offset 236
    pkt_chirho[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE_CHIRHO);

    let mut opt_offset_chirho: usize = 240;

    // Option 53: DHCP Message Type
    pkt_chirho[opt_offset_chirho] = 53;
    pkt_chirho[opt_offset_chirho + 1] = 1;
    pkt_chirho[opt_offset_chirho + 2] = msg_type_chirho;
    opt_offset_chirho += 3;

    if msg_type_chirho == DHCP_REQUEST_CHIRHO {
        // Option 50: Requested IP Address
        pkt_chirho[opt_offset_chirho] = 50;
        pkt_chirho[opt_offset_chirho + 1] = 4;
        let ip_bytes_chirho = requested_ip_chirho.to_be_bytes();
        pkt_chirho[opt_offset_chirho + 2..opt_offset_chirho + 6].copy_from_slice(&ip_bytes_chirho);
        opt_offset_chirho += 6;

        // Option 54: DHCP Server Identifier
        pkt_chirho[opt_offset_chirho] = 54;
        pkt_chirho[opt_offset_chirho + 1] = 4;
        let srv_bytes_chirho = server_ip_chirho.to_be_bytes();
        pkt_chirho[opt_offset_chirho + 2..opt_offset_chirho + 6].copy_from_slice(&srv_bytes_chirho);
        opt_offset_chirho += 6;
    }

    // Option 55: Parameter Request List (subnet, router, DNS)
    pkt_chirho[opt_offset_chirho] = 55;
    pkt_chirho[opt_offset_chirho + 1] = 3;
    pkt_chirho[opt_offset_chirho + 2] = 1;  // subnet mask
    pkt_chirho[opt_offset_chirho + 3] = 3;  // router
    pkt_chirho[opt_offset_chirho + 4] = 6;  // DNS
    opt_offset_chirho += 5;

    // Option 255: End
    pkt_chirho[opt_offset_chirho] = 255;

    pkt_chirho.truncate(opt_offset_chirho + 1);
    pkt_chirho
}

/// Parse DHCP options from a BOOTP/DHCP response.
///
/// Returns (msg_type, offered_ip, subnet_mask, gateway, dns_server, server_id).
fn parse_dhcp_options_chirho(
    pkt_chirho: &[u8],
) -> (u8, u32, u32, u32, u32, u32) {
    let mut msg_type_chirho: u8 = 0;
    let mut subnet_chirho: u32 = 0;
    let mut gateway_chirho: u32 = 0;
    let mut dns_chirho: u32 = 0;
    let mut server_id_chirho: u32 = 0;

    // yiaddr (offered IP) is at offset 16..20 in the BOOTP portion.
    let offered_ip_chirho = if pkt_chirho.len() >= 20 {
        u32::from_be_bytes([pkt_chirho[16], pkt_chirho[17], pkt_chirho[18], pkt_chirho[19]])
    } else {
        0
    };

    // Options start at offset 240 (after magic cookie at 236..240).
    if pkt_chirho.len() < 241 {
        return (msg_type_chirho, offered_ip_chirho, subnet_chirho, gateway_chirho, dns_chirho, server_id_chirho);
    }

    // Verify magic cookie.
    if pkt_chirho[236..240] != DHCP_MAGIC_COOKIE_CHIRHO {
        return (msg_type_chirho, offered_ip_chirho, subnet_chirho, gateway_chirho, dns_chirho, server_id_chirho);
    }

    let mut i_chirho: usize = 240;
    while i_chirho < pkt_chirho.len() {
        let opt_chirho = pkt_chirho[i_chirho];
        if opt_chirho == 255 {
            break; // End option
        }
        if opt_chirho == 0 {
            i_chirho += 1; // Padding
            continue;
        }
        if i_chirho + 1 >= pkt_chirho.len() {
            break;
        }
        let len_chirho = pkt_chirho[i_chirho + 1] as usize;
        let data_start_chirho = i_chirho + 2;
        if data_start_chirho + len_chirho > pkt_chirho.len() {
            break;
        }

        match opt_chirho {
            53 if len_chirho >= 1 => {
                msg_type_chirho = pkt_chirho[data_start_chirho];
            }
            1 if len_chirho >= 4 => {
                subnet_chirho = u32::from_be_bytes([
                    pkt_chirho[data_start_chirho], pkt_chirho[data_start_chirho + 1],
                    pkt_chirho[data_start_chirho + 2], pkt_chirho[data_start_chirho + 3],
                ]);
            }
            3 if len_chirho >= 4 => {
                gateway_chirho = u32::from_be_bytes([
                    pkt_chirho[data_start_chirho], pkt_chirho[data_start_chirho + 1],
                    pkt_chirho[data_start_chirho + 2], pkt_chirho[data_start_chirho + 3],
                ]);
            }
            6 if len_chirho >= 4 => {
                dns_chirho = u32::from_be_bytes([
                    pkt_chirho[data_start_chirho], pkt_chirho[data_start_chirho + 1],
                    pkt_chirho[data_start_chirho + 2], pkt_chirho[data_start_chirho + 3],
                ]);
            }
            54 if len_chirho >= 4 => {
                server_id_chirho = u32::from_be_bytes([
                    pkt_chirho[data_start_chirho], pkt_chirho[data_start_chirho + 1],
                    pkt_chirho[data_start_chirho + 2], pkt_chirho[data_start_chirho + 3],
                ]);
            }
            _ => {}
        }

        i_chirho = data_start_chirho + len_chirho;
    }

    (msg_type_chirho, offered_ip_chirho, subnet_chirho, gateway_chirho, dns_chirho, server_id_chirho)
}

/// DHCP result containing the assigned network configuration.
///
/// Uses `Ipv4AddrChirho` newtypes for all address fields (audit A2-AUDIT-007).
/// The inner `u32` is still accessible via `.0` for callers that need raw
/// values (e.g. the routing table, interface configuration).
#[derive(Debug, Clone)]
pub struct DhcpResultChirho {
    /// Assigned IP address.
    pub ip_chirho: Ipv4AddrChirho,
    /// Subnet mask.
    pub subnet_chirho: Ipv4AddrChirho,
    /// Default gateway.
    pub gateway_chirho: Ipv4AddrChirho,
    /// DNS server.
    pub dns_chirho: Ipv4AddrChirho,
}

/// Run the DHCP client on the given interface index.
///
/// Sends DISCOVER, waits for OFFER, sends REQUEST, waits for ACK.
/// On success, configures the interface IP, routing table, and DNS server.
pub fn dhcp_discover_chirho(iface_idx_chirho: usize) -> Option<DhcpResultChirho> {
    let mac_chirho = {
        let devs_chirho = NET_DEVICES_CHIRHO.lock();
        match devs_chirho.get(iface_idx_chirho) {
            Some(d_chirho) => d_chirho.mac_address_chirho(),
            None => return None,
        }
    };

    // Use the last 4 bytes of MAC as xid for simplicity.
    let xid_chirho = u32::from_be_bytes([mac_chirho[2], mac_chirho[3], mac_chirho[4], mac_chirho[5]]);

    crate::serial_debug_chirho!("[DHCP] Sending DISCOVER on interface {}...", iface_idx_chirho);

    // Build DHCP DISCOVER.
    let discover_payload_chirho = build_dhcp_packet_chirho(
        mac_chirho, xid_chirho, DHCP_DISCOVER_CHIRHO, 0, 0,
    );

    // Wrap in UDP/IP with src=0.0.0.0, dst=255.255.255.255.
    let src_ip_chirho: u32 = 0;          // 0.0.0.0
    let dst_ip_chirho: u32 = 0xFFFFFFFF; // 255.255.255.255
    let udp_chirho = UdpDatagramChirho {
        src_port_chirho: DHCP_CLIENT_PORT_CHIRHO,
        dst_port_chirho: DHCP_SERVER_PORT_CHIRHO,
        length_chirho: (8 + discover_payload_chirho.len()) as u16,
        checksum_chirho: 0,
        payload_chirho: discover_payload_chirho,
    };
    let udp_bytes_chirho = udp_chirho.build_with_checksum_chirho(src_ip_chirho, dst_ip_chirho);

    let total_len_chirho = 20 + udp_bytes_chirho.len() as u16;
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4,
        ihl_chirho: 5,
        tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0,
        flags_chirho: 0,
        fragment_offset_chirho: 0,
        ttl_chirho: 128,
        protocol_chirho: IP_PROTO_UDP_CHIRHO,
        checksum_chirho: 0,
        src_ip_chirho,
        dst_ip_chirho,
    };

    let mut ip_pkt_chirho = ip_hdr_chirho.build_chirho();
    ip_pkt_chirho.extend_from_slice(&udp_bytes_chirho);

    // Wrap in Ethernet broadcast frame.
    let eth_frame_chirho = EthernetFrameChirho {
        dst_mac_chirho: [0xFF; 6],
        src_mac_chirho: mac_chirho,
        ethertype_chirho: ETHERTYPE_IPV4_CHIRHO,
        payload_chirho: ip_pkt_chirho,
    };

    {
        let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
        if let Some(dev_chirho) = devs_chirho.get_mut(iface_idx_chirho) {
            dev_chirho.send_packet_chirho(&eth_frame_chirho.build_chirho());
        }
    }

    // Wait for OFFER.
    let mut offer_ip_chirho: u32 = 0;
    let mut offer_subnet_chirho: u32 = 0;
    let mut offer_gw_chirho: u32 = 0;
    let mut offer_dns_chirho: u32 = 0;
    let mut offer_server_chirho: u32 = 0;
    let mut got_offer_chirho = false;

    for poll_i_chirho in 0..NETWORK_POLL_MAX_CHIRHO {
        core::hint::spin_loop();

        // Every 100k iterations, log a progress dot for debugging.
        if poll_i_chirho > 0 && poll_i_chirho % NETWORK_POLL_SHORT_CHIRHO == 0 {
            crate::serial_debug_chirho!("[DHCP] Polling for OFFER... ({}/5M)", poll_i_chirho);
        }

        // Poll for incoming frames.
        let frame_chirho = {
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            match devs_chirho.get_mut(iface_idx_chirho) {
                Some(d_chirho) => d_chirho.recv_packet_chirho(),
                None => None,
            }
        };

        if let Some(raw_chirho) = frame_chirho {
            if let Some(eth_chirho) = EthernetFrameChirho::parse_chirho(&raw_chirho) {
                if eth_chirho.ethertype_chirho == ETHERTYPE_ARP_CHIRHO {
                    if let Some(arp_chirho) = ArpPacketChirho::parse_chirho(&eth_chirho.payload_chirho) {
                        handle_arp_chirho(&arp_chirho, &eth_chirho, iface_idx_chirho);
                    }
                    continue;
                }
                if eth_chirho.ethertype_chirho == ETHERTYPE_IPV4_CHIRHO {
                    // Check if it's a UDP packet to port 68.
                    if let Some(ip_h_chirho) = Ipv4HeaderChirho::parse_chirho(&eth_chirho.payload_chirho) {
                        if ip_h_chirho.protocol_chirho == IP_PROTO_UDP_CHIRHO {
                            let hdr_len_chirho = (ip_h_chirho.ihl_chirho as usize) * 4;
                            if eth_chirho.payload_chirho.len() > hdr_len_chirho + 8 {
                                let udp_payload_chirho = &eth_chirho.payload_chirho[hdr_len_chirho..];
                                if let Some(udp_d_chirho) = UdpDatagramChirho::parse_chirho(udp_payload_chirho) {
                                    if udp_d_chirho.dst_port_chirho == DHCP_CLIENT_PORT_CHIRHO {
                                        let (mt_chirho, ip_chirho, sn_chirho, gw_chirho, dns_v_chirho, srv_chirho) =
                                            parse_dhcp_options_chirho(&udp_d_chirho.payload_chirho);
                                        if mt_chirho == DHCP_OFFER_CHIRHO {
                                            offer_ip_chirho = ip_chirho;
                                            offer_subnet_chirho = sn_chirho;
                                            offer_gw_chirho = gw_chirho;
                                            offer_dns_chirho = dns_v_chirho;
                                            offer_server_chirho = srv_chirho;
                                            got_offer_chirho = true;
                                            crate::serial_debug_chirho!(
                                                "[DHCP] OFFER: IP={}.{}.{}.{} GW={}.{}.{}.{} DNS={}.{}.{}.{}",
                                                (ip_chirho >> 24) & 0xFF, (ip_chirho >> 16) & 0xFF,
                                                (ip_chirho >> 8) & 0xFF, ip_chirho & 0xFF,
                                                (gw_chirho >> 24) & 0xFF, (gw_chirho >> 16) & 0xFF,
                                                (gw_chirho >> 8) & 0xFF, gw_chirho & 0xFF,
                                                (dns_v_chirho >> 24) & 0xFF, (dns_v_chirho >> 16) & 0xFF,
                                                (dns_v_chirho >> 8) & 0xFF, dns_v_chirho & 0xFF,
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !got_offer_chirho {
        crate::serial_debug_chirho!("[DHCP] No OFFER received, giving up");
        return None;
    }

    // Send DHCP REQUEST for the offered IP.
    crate::serial_debug_chirho!("[DHCP] Sending REQUEST for {}.{}.{}.{}",
        (offer_ip_chirho >> 24) & 0xFF, (offer_ip_chirho >> 16) & 0xFF,
        (offer_ip_chirho >> 8) & 0xFF, offer_ip_chirho & 0xFF);

    let request_payload_chirho = build_dhcp_packet_chirho(
        mac_chirho, xid_chirho, DHCP_REQUEST_CHIRHO, offer_ip_chirho, offer_server_chirho,
    );

    let req_src_ip_chirho: u32 = 0;
    let req_dst_ip_chirho: u32 = 0xFFFFFFFF;
    let req_udp_chirho = UdpDatagramChirho {
        src_port_chirho: DHCP_CLIENT_PORT_CHIRHO,
        dst_port_chirho: DHCP_SERVER_PORT_CHIRHO,
        length_chirho: (8 + request_payload_chirho.len()) as u16,
        checksum_chirho: 0,
        payload_chirho: request_payload_chirho,
    };
    let req_udp_bytes_chirho = req_udp_chirho.build_with_checksum_chirho(req_src_ip_chirho, req_dst_ip_chirho);

    let req_total_len_chirho = 20 + req_udp_bytes_chirho.len() as u16;
    let req_ip_chirho = Ipv4HeaderChirho {
        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
        total_length_chirho: req_total_len_chirho,
        id_chirho: 1, flags_chirho: 0, fragment_offset_chirho: 0,
        ttl_chirho: 128,
        protocol_chirho: IP_PROTO_UDP_CHIRHO,
        checksum_chirho: 0,
        src_ip_chirho: req_src_ip_chirho,
        dst_ip_chirho: req_dst_ip_chirho,
    };
    let mut req_pkt_chirho = req_ip_chirho.build_chirho();
    req_pkt_chirho.extend_from_slice(&req_udp_bytes_chirho);

    let req_eth_chirho = EthernetFrameChirho {
        dst_mac_chirho: [0xFF; 6],
        src_mac_chirho: mac_chirho,
        ethertype_chirho: ETHERTYPE_IPV4_CHIRHO,
        payload_chirho: req_pkt_chirho,
    };

    {
        let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
        if let Some(dev_chirho) = devs_chirho.get_mut(iface_idx_chirho) {
            dev_chirho.send_packet_chirho(&req_eth_chirho.build_chirho());
        }
    }

    // Wait for ACK.
    let mut got_ack_chirho = false;
    for _poll_chirho in 0..2_000_000u32 {
        core::hint::spin_loop();
        let frame_chirho = {
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            match devs_chirho.get_mut(iface_idx_chirho) {
                Some(d_chirho) => d_chirho.recv_packet_chirho(),
                None => None,
            }
        };

        if let Some(raw_chirho) = frame_chirho {
            if let Some(eth_chirho) = EthernetFrameChirho::parse_chirho(&raw_chirho) {
                if eth_chirho.ethertype_chirho == ETHERTYPE_IPV4_CHIRHO {
                    if let Some(ip_h_chirho) = Ipv4HeaderChirho::parse_chirho(&eth_chirho.payload_chirho) {
                        if ip_h_chirho.protocol_chirho == IP_PROTO_UDP_CHIRHO {
                            let hdr_len_chirho = (ip_h_chirho.ihl_chirho as usize) * 4;
                            if eth_chirho.payload_chirho.len() > hdr_len_chirho + 8 {
                                let udp_payload_chirho = &eth_chirho.payload_chirho[hdr_len_chirho..];
                                if let Some(udp_d_chirho) = UdpDatagramChirho::parse_chirho(udp_payload_chirho) {
                                    if udp_d_chirho.dst_port_chirho == DHCP_CLIENT_PORT_CHIRHO {
                                        let (mt_chirho, _ip_chirho, _sn_chirho, _gw_chirho, _dns_chirho, _srv_chirho) =
                                            parse_dhcp_options_chirho(&udp_d_chirho.payload_chirho);
                                        if mt_chirho == DHCP_ACK_CHIRHO {
                                            got_ack_chirho = true;
                                            crate::serial_debug_chirho!("[DHCP] ACK received!");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !got_ack_chirho {
        crate::serial_debug_chirho!("[DHCP] No ACK received, using OFFER values anyway");
    }

    // Configure the interface.
    set_interface_ip_chirho(iface_idx_chirho, offer_ip_chirho);

    // Update the routing table with subnet route and default gateway.
    {
        let mut rt_chirho = ROUTING_TABLE_CHIRHO.lock();
        // Remove old default route.
        rt_chirho.remove_route_chirho(0, 0);
        // Add subnet route.
        if offer_subnet_chirho != 0 {
            rt_chirho.add_route_chirho(RouteEntryChirho {
                dest_chirho: offer_ip_chirho & offer_subnet_chirho,
                mask_chirho: offer_subnet_chirho,
                gateway_chirho: 0, // on-link
                iface_idx_chirho,
                metric_chirho: 0,
            });
        }
        // Add default gateway.
        if offer_gw_chirho != 0 {
            rt_chirho.add_route_chirho(RouteEntryChirho {
                dest_chirho: 0,
                mask_chirho: 0,
                gateway_chirho: offer_gw_chirho,
                iface_idx_chirho,
                metric_chirho: 100,
            });
        }
    }

    // Set DNS server.
    if offer_dns_chirho != 0 {
        *DNS_SERVER_IP_CHIRHO.lock() = offer_dns_chirho;
        crate::serial_println_chirho!("[DHCP] DNS server set to {}.{}.{}.{}",
            (offer_dns_chirho >> 24) & 0xFF, (offer_dns_chirho >> 16) & 0xFF,
            (offer_dns_chirho >> 8) & 0xFF, offer_dns_chirho & 0xFF);
    }

    // Set gateway.
    *GATEWAY_IP_CHIRHO.lock() = offer_gw_chirho;

    // Configure iface config table.
    {
        let mut cfg_chirho = IFACE_CONFIG_CHIRHO.lock();
        cfg_chirho.push(IfaceConfigChirho {
            name_chirho: alloc::string::String::from("eth0"),
            ipv4_addr_chirho: offer_ip_chirho,
            netmask_chirho: offer_subnet_chirho,
            flags_chirho: IFF_UP_CHIRHO | IFF_RUNNING_CHIRHO,
            mtu_val_chirho: ETHERNET_MTU_CHIRHO as u32,
        });
    }

    crate::serial_println_chirho!(
        "[DHCP] Configuration complete: IP={}.{}.{}.{} GW={}.{}.{}.{} DNS={}.{}.{}.{}",
        (offer_ip_chirho >> 24) & 0xFF, (offer_ip_chirho >> 16) & 0xFF,
        (offer_ip_chirho >> 8) & 0xFF, offer_ip_chirho & 0xFF,
        (offer_gw_chirho >> 24) & 0xFF, (offer_gw_chirho >> 16) & 0xFF,
        (offer_gw_chirho >> 8) & 0xFF, offer_gw_chirho & 0xFF,
        (offer_dns_chirho >> 24) & 0xFF, (offer_dns_chirho >> 16) & 0xFF,
        (offer_dns_chirho >> 8) & 0xFF, offer_dns_chirho & 0xFF,
    );

    Some(DhcpResultChirho {
        ip_chirho: Ipv4AddrChirho(offer_ip_chirho),
        subnet_chirho: Ipv4AddrChirho(offer_subnet_chirho),
        gateway_chirho: Ipv4AddrChirho(offer_gw_chirho),
        dns_chirho: Ipv4AddrChirho(offer_dns_chirho),
    })
}

// ============================================================================
// P3-005: DNS resolution through the real NIC
// ============================================================================

/// Resolve a hostname to an IPv4 address by sending a DNS query through the
/// real NIC and waiting for a response.
pub fn resolve_hostname_real_chirho(hostname_chirho: &str) -> Option<u32> {
    let dns_server_chirho = *DNS_SERVER_IP_CHIRHO.lock();
    if dns_server_chirho == 0 {
        crate::serial_debug_chirho!("[DNS] No DNS server configured");
        return None;
    }

    let query_chirho = build_dns_query_chirho(hostname_chirho);
    let src_port_chirho = alloc_ephemeral_port_chirho();

    // Get our source IP from the outgoing interface.
    let (_gw_chirho, iface_idx_chirho) = route_packet_chirho(dns_server_chirho).ok()?;
    let src_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);

    // Build and send UDP/IP packet.
    let pkt_chirho = build_udp_packet_chirho(
        src_ip_chirho, dns_server_chirho, src_port_chirho, DNS_PORT_CHIRHO, &query_chirho,
    );

    crate::serial_debug_chirho!(
        "[DNS] Querying {} for '{}' (src_port={})",
        format_ip_chirho(dns_server_chirho), hostname_chirho, src_port_chirho,
    );

    send_ip_packet_chirho(&pkt_chirho).ok()?;

    // Poll for DNS response.
    for _poll_chirho in 0..2_000_000u32 {
        core::hint::spin_loop();
        poll_network_chirho();

        // Check if a UDP socket bound to src_port has received data.
        // We'll check the software receive path — DNS responses are delivered
        // via deliver_udp_packet_chirho to bound sockets. But since we don't
        // have a socket bound, let's check incoming frames directly.
        let frame_chirho = {
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            match devs_chirho.get_mut(iface_idx_chirho) {
                Some(d_chirho) => d_chirho.recv_packet_chirho(),
                None => None,
            }
        };

        if let Some(raw_chirho) = frame_chirho {
            if let Some(eth_chirho) = EthernetFrameChirho::parse_chirho(&raw_chirho) {
                if eth_chirho.ethertype_chirho == ETHERTYPE_ARP_CHIRHO {
                    if let Some(arp_chirho) = ArpPacketChirho::parse_chirho(&eth_chirho.payload_chirho) {
                        handle_arp_chirho(&arp_chirho, &eth_chirho, iface_idx_chirho);
                    }
                    continue;
                }
                if eth_chirho.ethertype_chirho == ETHERTYPE_IPV4_CHIRHO {
                    if let Some(ip_h_chirho) = Ipv4HeaderChirho::parse_chirho(&eth_chirho.payload_chirho) {
                        if ip_h_chirho.protocol_chirho == IP_PROTO_UDP_CHIRHO {
                            let hdr_len_chirho = (ip_h_chirho.ihl_chirho as usize) * 4;
                            if eth_chirho.payload_chirho.len() > hdr_len_chirho {
                                let udp_data_chirho = &eth_chirho.payload_chirho[hdr_len_chirho..];
                                if let Some(udp_chirho) = UdpDatagramChirho::parse_chirho(udp_data_chirho) {
                                    if udp_chirho.dst_port_chirho == src_port_chirho
                                        && udp_chirho.src_port_chirho == DNS_PORT_CHIRHO
                                    {
                                        let answers_chirho = parse_dns_response_chirho(&udp_chirho.payload_chirho);
                                        if let Some(first_chirho) = answers_chirho.first() {
                                            crate::serial_debug_chirho!(
                                                "[DNS] Resolved '{}' -> {}.{}.{}.{}",
                                                hostname_chirho,
                                                (first_chirho.addr_chirho >> 24) & 0xFF,
                                                (first_chirho.addr_chirho >> 16) & 0xFF,
                                                (first_chirho.addr_chirho >> 8) & 0xFF,
                                                first_chirho.addr_chirho & 0xFF,
                                            );
                                            return Some(first_chirho.addr_chirho);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    crate::serial_debug_chirho!("[DNS] Resolution timed out for '{}'", hostname_chirho);
    None
}

// ============================================================================
// P3-006: TCP connect through the real NIC
// ============================================================================

/// Perform a TCP connect to a remote host:port through the real NIC.
///
/// This builds and sends a SYN packet, then waits for SYN-ACK, and sends ACK.
/// Returns the socket table index on success.
pub fn tcp_connect_real_chirho(
    dst_ip_chirho: u32,
    dst_port_chirho: u16,
) -> Result<usize, i64> {
    let (_gw_chirho, iface_idx_chirho) = route_packet_chirho(dst_ip_chirho)?;
    let src_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);
    let src_port_chirho = alloc_ephemeral_port_chirho();

    // Create a socket in the socket table.
    let socket_chirho = SocketChirho::new_chirho(AF_INET_CHIRHO, 1, 0); // AF_INET, SOCK_STREAM
    let socket_idx_chirho = alloc_socket_slot_chirho(socket_chirho)
        .map_err(|e_chirho| e_chirho)?;

    {
        let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
        let sock_chirho = match table_chirho[socket_idx_chirho].as_mut() {
            Some(sock_chirho) => sock_chirho,
            None => return Err(-EBADF_CHIRHO),
        };
        sock_chirho.local_addr_chirho = Some(SockAddrInChirho {
            port_chirho: src_port_chirho,
            addr_chirho: src_ip_chirho,
        });
        sock_chirho.remote_addr_chirho = Some(SockAddrInChirho {
            port_chirho: dst_port_chirho,
            addr_chirho: dst_ip_chirho,
        });

        // Perform TCP active open — generates SYN segment.
        let syn_seg_chirho = sock_chirho.tcb_chirho
            .active_open_chirho(src_port_chirho, dst_port_chirho)
            .map_err(|e_chirho| e_chirho)?;

        // Compute checksum and send.
        let cksum_chirho = syn_seg_chirho.compute_checksum_chirho(src_ip_chirho, dst_ip_chirho);
        let mut syn_with_cksum_chirho = syn_seg_chirho;
        syn_with_cksum_chirho.checksum_chirho = cksum_chirho;
        let tcp_bytes_chirho = syn_with_cksum_chirho.build_chirho();

        let total_len_chirho = 20 + tcp_bytes_chirho.len() as u16;
        let ip_hdr_chirho = Ipv4HeaderChirho {
            version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
            total_length_chirho: total_len_chirho,
            id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
            ttl_chirho: 64,
            protocol_chirho: IP_PROTO_TCP_CHIRHO,
            checksum_chirho: 0,
            src_ip_chirho, dst_ip_chirho,
        };
        let mut pkt_chirho = ip_hdr_chirho.build_chirho();
        pkt_chirho.extend_from_slice(&tcp_bytes_chirho);

        // Send the SYN packet.
        drop(table_chirho);
        send_ip_packet_chirho(&pkt_chirho)?;
    }

    crate::serial_debug_chirho!(
        "[TCP] SYN sent to {}:{} from {}:{}",
        format_ip_chirho(dst_ip_chirho), dst_port_chirho,
        format_ip_chirho(src_ip_chirho), src_port_chirho,
    );

    // Poll for SYN-ACK and let the TCP state machine handle it.
    for _poll_chirho in 0..NETWORK_POLL_MAX_CHIRHO {
        core::hint::spin_loop();
        poll_network_chirho();

        let table_chirho = SOCKET_TABLE_CHIRHO.lock();
        if let Some(ref sock_chirho) = table_chirho[socket_idx_chirho] {
            if matches!(sock_chirho.tcb_chirho.state_chirho, TcpStateChirho::EstablishedChirho | TcpStateChirho::CloseWaitChirho) {
                crate::serial_debug_chirho!(
                    "[TCP] Connection established to {}:{}",
                    format_ip_chirho(dst_ip_chirho), dst_port_chirho,
                );
                return Ok(socket_idx_chirho);
            }
            if sock_chirho.tcb_chirho.state_chirho == TcpStateChirho::ClosedChirho {
                return Err(-ECONNREFUSED_CHIRHO);
            }
        }
    }

    crate::serial_debug_chirho!("[TCP] Connection timed out to {}:{}", format_ip_chirho(dst_ip_chirho), dst_port_chirho);
    Err(-110) // ETIMEDOUT
}

// ============================================================================
// P3-006: TCP send data through the real NIC
// ============================================================================

/// Send data through a connected TCP socket via the real NIC.
pub fn tcp_send_real_chirho(socket_idx_chirho: usize, data_chirho: &[u8]) -> Result<usize, i64> {
    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let sock_chirho = table_chirho[socket_idx_chirho].as_mut()
        .ok_or(-EBADF_CHIRHO)?;

    let local_port_chirho = sock_chirho.local_addr_chirho.map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);
    let remote_port_chirho = sock_chirho.remote_addr_chirho.map(|a_chirho| a_chirho.port_chirho).unwrap_or(0);
    let src_ip_chirho = sock_chirho.local_addr_chirho.map(|a_chirho| a_chirho.addr_chirho).unwrap_or(0);
    let dst_ip_chirho = sock_chirho.remote_addr_chirho.map(|a_chirho| a_chirho.addr_chirho).unwrap_or(0);

    let seg_chirho = sock_chirho.tcb_chirho.make_data_segment_chirho(local_port_chirho, remote_port_chirho, data_chirho)
        .ok_or(-ENOTCONN_CHIRHO)?;

    let cksum_chirho = seg_chirho.compute_checksum_chirho(src_ip_chirho, dst_ip_chirho);
    let mut seg_ck_chirho = seg_chirho;
    seg_ck_chirho.checksum_chirho = cksum_chirho;
    let tcp_bytes_chirho = seg_ck_chirho.build_chirho();

    let total_len_chirho = 20 + tcp_bytes_chirho.len() as u16;
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
        ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO, checksum_chirho: 0,
        src_ip_chirho, dst_ip_chirho,
    };
    let mut pkt_chirho = ip_hdr_chirho.build_chirho();
    pkt_chirho.extend_from_slice(&tcp_bytes_chirho);

    drop(table_chirho);
    send_ip_packet_chirho(&pkt_chirho)?;
    Ok(data_chirho.len())
}

// ============================================================================
// P3-007: Wire socket syscalls to the real network stack
// ============================================================================

/// Enhanced `connect` that routes through the real NIC for non-loopback addresses.
pub fn sys_connect_real_chirho(
    sockfd_chirho: u64,
    addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let parsed_addr_chirho = unsafe { read_sockaddr_from_user_chirho(addr_chirho, addrlen_chirho) };
    let dest_addr_chirho = match parsed_addr_chirho {
        Some(a_chirho) => a_chirho,
        None => return -EINVAL_CHIRHO,
    };

    // For loopback addresses, use the existing loopback path.
    if is_loopback_addr_chirho(dest_addr_chirho.addr_chirho) {
        return sys_connect_chirho(sockfd_chirho, addr_chirho, addrlen_chirho);
    }

    // For real network addresses, use the NIC path.
    let mut table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let sock_chirho = match table_chirho[socket_idx_chirho].as_mut() {
        Some(s_chirho) => s_chirho,
        None => return -EBADF_CHIRHO,
    };

    if sock_chirho.state_chirho == SocketStateChirho::ConnectedChirho {
        return -EISCONN_CHIRHO;
    }

    // Determine source IP and port.
    let (_gw_chirho, iface_idx_chirho) = match route_packet_chirho(dest_addr_chirho.addr_chirho) {
        Ok(r_chirho) => r_chirho,
        Err(e_chirho) => return e_chirho,
    };
    let src_ip_chirho = get_interface_ip_chirho(iface_idx_chirho);
    let src_port_chirho = alloc_ephemeral_port_chirho();

    sock_chirho.local_addr_chirho = Some(SockAddrInChirho {
        port_chirho: src_port_chirho,
        addr_chirho: src_ip_chirho,
    });
    sock_chirho.remote_addr_chirho = Some(dest_addr_chirho);

    let sock_type_chirho = SocketTypeChirho::from_raw_chirho(sock_chirho.sock_type_chirho);

    if sock_type_chirho == Some(SocketTypeChirho::SockDgramChirho) {
        sock_chirho.state_chirho = SocketStateChirho::ConnectedChirho;
        return 0;
    }

    // SOCK_STREAM: TCP SYN.
    let syn_seg_chirho = match sock_chirho.tcb_chirho.active_open_chirho(src_port_chirho, dest_addr_chirho.port_chirho) {
        Ok(s_chirho) => s_chirho,
        Err(e_chirho) => return e_chirho,
    };

    let cksum_chirho = syn_seg_chirho.compute_checksum_chirho(src_ip_chirho, dest_addr_chirho.addr_chirho);
    let mut syn_ck_chirho = syn_seg_chirho;
    syn_ck_chirho.checksum_chirho = cksum_chirho;
    let tcp_bytes_chirho = syn_ck_chirho.build_chirho();

    let total_len_chirho = 20 + tcp_bytes_chirho.len() as u16;
    let ip_hdr_chirho = Ipv4HeaderChirho {
        version_chirho: 4, ihl_chirho: 5, tos_chirho: 0,
        total_length_chirho: total_len_chirho,
        id_chirho: 0, flags_chirho: 0x02, fragment_offset_chirho: 0,
        ttl_chirho: 64, protocol_chirho: IP_PROTO_TCP_CHIRHO, checksum_chirho: 0,
        src_ip_chirho, dst_ip_chirho: dest_addr_chirho.addr_chirho,
    };
    let mut pkt_chirho = ip_hdr_chirho.build_chirho();
    pkt_chirho.extend_from_slice(&tcp_bytes_chirho);

    drop(table_chirho);
    if send_ip_packet_chirho(&pkt_chirho).is_err() {
        return -crate::syscall_chirho::ENETUNREACH_CHIRHO;
    }

    // Poll for SYN-ACK.
    for _poll_chirho in 0..NETWORK_POLL_MAX_CHIRHO {
        core::hint::spin_loop();
        poll_network_chirho();

        let tbl_chirho = SOCKET_TABLE_CHIRHO.lock();
        if let Some(ref sk_chirho) = tbl_chirho[socket_idx_chirho] {
            if sk_chirho.tcb_chirho.state_chirho == TcpStateChirho::EstablishedChirho {
                return 0;
            }
            if sk_chirho.tcb_chirho.state_chirho == TcpStateChirho::ClosedChirho {
                return -ECONNREFUSED_CHIRHO;
            }
        }
    }

    -110 // ETIMEDOUT
}

/// Enhanced `sendto` that routes data through the real NIC for non-loopback sockets.
pub fn sys_sendto_real_chirho(
    sockfd_chirho: u64,
    buf_chirho: u64,
    len_chirho: u64,
    flags_chirho: u64,
    dest_addr_chirho: u64,
    addrlen_chirho: u64,
) -> i64 {
    let socket_idx_chirho = match socket_idx_from_fd_chirho(sockfd_chirho) {
        Ok(idx_chirho) => idx_chirho,
        Err(_) => return sys_sendto_chirho(sockfd_chirho, buf_chirho, len_chirho, flags_chirho, dest_addr_chirho, addrlen_chirho),
    };

    // Read data from user-space.
    let count_chirho = core::cmp::min(len_chirho as usize, SOCKET_SEND_MAX_CHIRHO);
    let mut data_chirho = Vec::with_capacity(count_chirho);
    if buf_chirho != 0 && count_chirho > 0 {
        let ptr_chirho = buf_chirho as *const u8;
        for i_chirho in 0..count_chirho {
            data_chirho.push(unsafe { core::ptr::read_volatile(ptr_chirho.add(i_chirho)) });
        }
    }

    let table_chirho = SOCKET_TABLE_CHIRHO.lock();
    let sock_chirho = match table_chirho[socket_idx_chirho].as_ref() {
        Some(s_chirho) => s_chirho,
        None => return len_chirho as i64,
    };

    // Check if the remote address is non-loopback.
    let is_real_chirho = sock_chirho.remote_addr_chirho
        .map(|a_chirho| !is_loopback_addr_chirho(a_chirho.addr_chirho) && a_chirho.addr_chirho != 0)
        .unwrap_or(false);

    drop(table_chirho);

    if is_real_chirho {
        // Send through the real NIC via TCP.
        match tcp_send_real_chirho(socket_idx_chirho, &data_chirho) {
            Ok(n_chirho) => n_chirho as i64,
            Err(e_chirho) => e_chirho,
        }
    } else {
        // Loopback path.
        sys_sendto_chirho(sockfd_chirho, buf_chirho, len_chirho, flags_chirho, dest_addr_chirho, addrlen_chirho)
    }
}

// ============================================================================
// P3-002: VirtIO-net probe and registration in init_networking_chirho
// ============================================================================

/// Probe VirtIO-net devices from PCI + MMIO and register them as network interfaces.
#[allow(dead_code)]
pub fn probe_virtio_net_chirho() {
    crate::serial_debug_chirho!("[VNET] Probing for VirtIO-net devices...");

    // Probe PCI bus for VirtIO-net.
    let pci_devs_chirho = crate::virtio_chirho::scan_pci_virtio_chirho();
    for pci_dev_chirho in &pci_devs_chirho {
        if crate::virtio_chirho::is_virtio_net_chirho(pci_dev_chirho) {
            let bar0_raw_chirho = crate::virtio_chirho::read_pci_bar_chirho(pci_dev_chirho, 0);
            let is_io_bar_chirho = bar0_raw_chirho & 1 != 0;
            if is_io_bar_chirho {
                continue;
            }
            let mmio_base_chirho = (bar0_raw_chirho & 0xFFFF_FFF0) as usize;
            if mmio_base_chirho == 0 {
                continue;
            }
            crate::serial_debug_chirho!("[VNET] Probing PCI VirtIO-net at MMIO {:#x}", mmio_base_chirho);
            if let Some(net_dev_chirho) = VirtioNetDeviceChirho::probe_mmio_chirho(mmio_base_chirho) {
                let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
                devs_chirho.push(Box::new(net_dev_chirho));
                let idx_chirho = devs_chirho.len() - 1;
                crate::serial_debug_chirho!("[VNET] Registered VirtIO-net as interface {}", idx_chirho);
            }
        }
    }

    // Probe QEMU's default VirtIO-MMIO addresses for net devices (type=1).
    const QEMU_MMIO_BASE_CHIRHO: usize = 0x1000_1000;
    const QEMU_MMIO_STEP_CHIRHO: usize = 0x1000;
    const QEMU_MMIO_COUNT_CHIRHO: usize = 8;

    for i_chirho in 0..QEMU_MMIO_COUNT_CHIRHO {
        let addr_chirho = QEMU_MMIO_BASE_CHIRHO + i_chirho * QEMU_MMIO_STEP_CHIRHO;
        let transport_chirho = VirtioMmioTransportChirho::new_chirho(addr_chirho);
        if !transport_chirho.check_magic_chirho() {
            continue;
        }
        if transport_chirho.device_id_chirho() != 1 {
            continue;
        }

        crate::serial_debug_chirho!("[VNET] Probing MMIO VirtIO-net at {:#x}", addr_chirho);
        if let Some(net_dev_chirho) = VirtioNetDeviceChirho::probe_mmio_chirho(addr_chirho) {
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            devs_chirho.push(Box::new(net_dev_chirho));
            let idx_chirho = devs_chirho.len() - 1;
            crate::serial_debug_chirho!("[VNET] Registered VirtIO-net MMIO as interface {}", idx_chirho);
        }
    }

    // Set loopback IP.
    set_interface_ip_chirho(0, LOOPBACK_IP_CHIRHO);

    // Run DHCP on the first real NIC (interface 1, if present).
    let nic_count_chirho = {
        let devs_chirho = NET_DEVICES_CHIRHO.lock();
        devs_chirho.len()
    };

    if nic_count_chirho > 1 {
        crate::serial_debug_chirho!("[VNET] Running DHCP on interface 1...");
        let _dhcp_result_chirho = dhcp_discover_chirho(1);
    } else {
        crate::serial_debug_chirho!("[VNET] No NIC found, skipping DHCP");
    }

    crate::serial_debug_chirho!("[VNET] VirtIO-net probe complete ({} interfaces total)", nic_count_chirho);
}

// ============================================================================
// P3-003: VirtIO-net I/O port transport driver
// ============================================================================

/// Legacy VirtIO queue alignment (4096 bytes = page size).
const VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO: usize = 4096;

/// Bump allocator for VirtIO-net DMA memory — starts at 10MB physical.
/// Separate from VirtIO-blk's allocator (8MB) and request buffer (9MB).
static NET_VRING_PHYS_NEXT_CHIRHO: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0xA0_0000); // 10MB physical

/// Fixed physical page for VirtIO-net TX DMA requests (reused).
const NET_TX_DMA_PHYS_CHIRHO: u64 = 0xB0_0000; // 11MB physical — one page, reused

/// VirtIO-net device driver backed by legacy PCI I/O port transport.
///
/// Queue 0 = receiveq (device -> driver), Queue 1 = transmitq (driver -> device).
/// Uses the same I/O port register offsets as VirtIO-blk (they share the
/// legacy transport layout).
pub struct VirtioNetIoDeviceChirho {
    /// I/O port transport for register access.
    transport_chirho: VirtioIoTransportChirho,
    /// MAC address read from device config space (offset 0x14, 6 bytes).
    mac_addr_chirho: [u8; 6],
    /// MTU (defaults to `ETHERNET_MTU_CHIRHO` for standard Ethernet).
    mtu_val_chirho: usize,
    /// Whether the device has been successfully initialized.
    initialized_chirho: bool,
    /// Receive virtqueue (queue index 0) — tracking structure.
    rx_vq_chirho: VirtQueueChirho,
    /// Transmit virtqueue (queue index 1) — tracking structure.
    tx_vq_chirho: VirtQueueChirho,
    /// Physical base address of the RX vring DMA region.
    rx_vq_phys_base_chirho: u64,
    /// Physical base address of the TX vring DMA region.
    tx_vq_phys_base_chirho: u64,
    /// Pre-allocated RX buffers in contiguous physical memory.
    /// Each buffer holds VIRTIO_NET_HDR_SIZE + MAX_FRAME_SIZE bytes.
    /// Index matches descriptor index.
    rx_buf_phys_base_chirho: u64,
    /// Number of RX buffers allocated.
    rx_buf_count_chirho: u16,
    /// Software receive queue: frames popped from the used ring.
    sw_rx_queue_chirho: VecDeque<Vec<u8>>,
}

// SAFETY: The device is single-threaded at init; afterwards access is behind
// NET_DEVICES_CHIRHO Mutex.
unsafe impl Send for VirtioNetIoDeviceChirho {}

impl VirtioNetIoDeviceChirho {
    /// Allocate a contiguous DMA region from the net bump allocator.
    /// Returns (physical_address, virtual_address).
    fn alloc_dma_chirho(size_bytes_chirho: usize) -> (u64, usize) {
        let pages_chirho = ((size_bytes_chirho + 4095) / 4096) as u64;
        let phys_chirho = NET_VRING_PHYS_NEXT_CHIRHO.fetch_add(
            pages_chirho * 4096,
            core::sync::atomic::Ordering::SeqCst,
        );
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let virt_chirho = (phys_chirho + phys_offset_chirho) as usize;
        // Zero the region
        unsafe {
            core::ptr::write_bytes(virt_chirho as *mut u8, 0, size_bytes_chirho);
        }
        (phys_chirho, virt_chirho)
    }

    /// Set up a single legacy virtqueue: allocate contiguous DMA, write PFN.
    /// Returns (VirtQueueChirho, phys_base, queue_size).
    fn setup_legacy_queue_chirho(
        transport_chirho: &VirtioIoTransportChirho,
        queue_idx_chirho: u16,
    ) -> Option<(VirtQueueChirho, u64, u16)> {
        transport_chirho.select_queue_chirho(queue_idx_chirho);
        let queue_size_chirho = transport_chirho.read_queue_size_chirho();
        if queue_size_chirho == 0 {
            crate::serial_debug_chirho!(
                "    [VNET-IO] Queue {} size is 0, aborting",
                queue_idx_chirho
            );
            return None;
        }
        crate::serial_debug_chirho!(
            "    [VNET-IO] Queue {} max size = {}",
            queue_idx_chirho,
            queue_size_chirho
        );

        // Use the device's queue size (legacy VirtIO requires it).
        let actual_size_chirho = queue_size_chirho;

        // Compute the legacy vring layout size.
        let desc_table_bytes_chirho = (actual_size_chirho as usize) * 16;
        let avail_ring_bytes_chirho = 4 + (actual_size_chirho as usize) * 2 + 2;
        let avail_end_chirho = desc_table_bytes_chirho + avail_ring_bytes_chirho;
        let used_ring_offset_chirho =
            (avail_end_chirho + VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1)
                & !(VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1);
        let used_ring_bytes_chirho = 4 + (actual_size_chirho as usize) * 8 + 2;
        let total_bytes_chirho = used_ring_offset_chirho + used_ring_bytes_chirho;

        // Allocate contiguous physical DMA memory.
        let (phys_base_chirho, virt_base_chirho) = Self::alloc_dma_chirho(total_bytes_chirho);

        crate::serial_debug_chirho!(
            "    [VNET-IO] Queue {} virt={:#x} phys={:#x} size={} total_bytes={}",
            queue_idx_chirho,
            virt_base_chirho,
            phys_base_chirho,
            actual_size_chirho,
            total_bytes_chirho
        );

        // Write the queue PFN to the device.
        let pfn_chirho = (phys_base_chirho >> 12) as u32;
        transport_chirho.write_queue_pfn_chirho(pfn_chirho);

        let vq_chirho = VirtQueueChirho::new_chirho(actual_size_chirho);

        Some((vq_chirho, phys_base_chirho, actual_size_chirho))
    }

    /// Probe and initialize a VirtIO-net device via legacy PCI I/O port transport.
    ///
    /// `io_base_chirho` is the I/O port base address (BAR0 & 0xFFFC).
    ///
    /// Legacy VirtIO initialization sequence:
    ///   1. Reset device (status = 0)
    ///   2. Set ACKNOWLEDGE (status |= 1)
    ///   3. Set DRIVER (status |= 2)
    ///   4. Negotiate features (accept MAC feature bit 5)
    ///   5. Set up virtqueue 0 (RX) and virtqueue 1 (TX)
    ///   6. Set FEATURES_OK (status |= 8)
    ///   7. Set DRIVER_OK (status |= 4)
    ///   8. Read MAC address from device config
    ///   9. Pre-populate RX buffers
    ///
    /// Returns `None` if the device cannot be initialized.
    pub fn probe_io_chirho(io_base_chirho: u16) -> Option<Self> {
        let transport_chirho = VirtioIoTransportChirho::new_chirho(io_base_chirho);

        crate::serial_debug_chirho!(
            "    [VNET-IO] Probing VirtIO-net at I/O base {:#06x}",
            io_base_chirho
        );

        // Step 1: Reset the device.
        transport_chirho.reset_chirho();

        // Step 2: Acknowledge.
        let mut status_chirho: u8 = 1; // VIRTIO_STATUS_ACKNOWLEDGE
        transport_chirho.write_status_chirho(status_chirho);

        // Step 3: Driver.
        status_chirho |= 2; // VIRTIO_STATUS_DRIVER
        transport_chirho.write_status_chirho(status_chirho);

        // Step 4: Feature negotiation.
        let device_features_chirho = transport_chirho.read_device_features_chirho();
        crate::serial_debug_chirho!(
            "    [VNET-IO] Device features = {:#010x}",
            device_features_chirho
        );
        // Accept bit 5 (VIRTIO_NET_F_MAC) so device exposes MAC in config,
        // plus basic ring features (bits 0-4).
        // Bit 0 = VIRTIO_NET_F_CSUM, Bit 1 = VIRTIO_NET_F_GUEST_CSUM, etc.
        // We accept bits 0-5 (including F_MAC = bit 5) as the blk driver does.
        let accepted_chirho = device_features_chirho & 0x3F;
        transport_chirho.write_guest_features_chirho(accepted_chirho);

        // Step 5: Set up RX queue (queue 0).
        let (rx_vq_chirho, rx_phys_base_chirho, rx_size_chirho) =
            Self::setup_legacy_queue_chirho(&transport_chirho, 0)?;

        // Set up TX queue (queue 1).
        let (tx_vq_chirho, tx_phys_base_chirho, _tx_size_chirho) =
            Self::setup_legacy_queue_chirho(&transport_chirho, 1)?;

        // Step 6: Set FEATURES_OK.
        status_chirho |= 8; // FEATURES_OK
        transport_chirho.write_status_chirho(status_chirho);
        let verify_status_chirho = transport_chirho.read_status_chirho();
        if verify_status_chirho & 8 == 0 {
            crate::serial_debug_chirho!(
                "    [VNET-IO] WARNING: FEATURES_OK not accepted by device"
            );
        }

        // Step 7: DRIVER_OK — device is live.
        status_chirho = verify_status_chirho | 4; // VIRTIO_STATUS_DRIVER_OK
        transport_chirho.write_status_chirho(status_chirho);

        let final_status_chirho = transport_chirho.read_status_chirho();
        crate::serial_debug_chirho!(
            "    [VNET-IO] Device status after init = {:#04x}",
            final_status_chirho
        );

        // Step 8: Read MAC address from device config at offset 0x14 (6 bytes).
        // In legacy I/O port transport, device config starts at IO_BASE + 0x14.
        let mut mac_chirho = [0u8; 6];
        for i_chirho in 0..6u16 {
            mac_chirho[i_chirho as usize] = transport_chirho.read_config8_chirho(i_chirho);
        }

        crate::serial_debug_chirho!(
            "    [VNET-IO] MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac_chirho[0], mac_chirho[1], mac_chirho[2],
            mac_chirho[3], mac_chirho[4], mac_chirho[5],
        );

        // Step 9: Pre-populate RX buffers in contiguous physical memory.
        // Each buffer = VIRTIO_NET_HDR_SIZE + MAX_FRAME_SIZE bytes.
        let buf_size_chirho = VIRTIO_NET_HDR_SIZE_CHIRHO + MAX_FRAME_SIZE_CHIRHO;
        // Use up to RX_RING_SIZE or queue_size, whichever is smaller.
        let num_rx_bufs_chirho = core::cmp::min(
            RX_RING_SIZE_CHIRHO as u16,
            rx_size_chirho,
        );
        let total_rx_buf_bytes_chirho = (num_rx_bufs_chirho as usize) * buf_size_chirho;
        let (rx_buf_phys_chirho, rx_buf_virt_chirho) =
            Self::alloc_dma_chirho(total_rx_buf_bytes_chirho);

        crate::serial_debug_chirho!(
            "    [VNET-IO] RX buffers: {} x {} bytes at phys {:#x}",
            num_rx_bufs_chirho, buf_size_chirho, rx_buf_phys_chirho
        );

        // Post each RX buffer as a single descriptor to the RX vring.
        let mut rx_vq_mut_chirho = rx_vq_chirho;
        let rx_vring_virt_chirho = (rx_phys_base_chirho
            + crate::pagetable_chirho::phys_mem_offset_chirho()) as usize;
        let desc_base_chirho = rx_vring_virt_chirho as *mut VringDescChirho;

        let desc_table_bytes_chirho = (rx_vq_mut_chirho.size_chirho as usize) * 16;
        let avail_base_chirho =
            (rx_vring_virt_chirho + desc_table_bytes_chirho) as *mut u16;

        for i_chirho in 0..num_rx_bufs_chirho {
            let buf_phys_chirho =
                rx_buf_phys_chirho + (i_chirho as u64) * (buf_size_chirho as u64);

            if let Some(desc_idx_chirho) = rx_vq_mut_chirho.alloc_desc_chirho() {
                // Write descriptor directly into DMA-shared memory.
                let desc_chirho = VringDescChirho {
                    addr_chirho: buf_phys_chirho,
                    len_chirho: buf_size_chirho as u32,
                    flags_chirho: VNET_DESC_F_WRITE_CHIRHO, // device-writable
                    next_chirho: 0,
                };
                unsafe {
                    ptr::write_volatile(desc_base_chirho.add(desc_idx_chirho as usize), desc_chirho);
                }
                // Also track in our software vq.
                rx_vq_mut_chirho.desc_chirho[desc_idx_chirho as usize] = desc_chirho;

                // Update avail ring in shared memory.
                let avail_idx_chirho = rx_vq_mut_chirho.avail_idx_chirho;
                let ring_slot_chirho =
                    (avail_idx_chirho % rx_vq_mut_chirho.size_chirho) as usize;
                unsafe {
                    let ring_ptr_chirho = avail_base_chirho.add(2 + ring_slot_chirho);
                    ptr::write_volatile(ring_ptr_chirho, desc_idx_chirho);
                    fence(NetOrdering::Release);
                    let idx_ptr_chirho = avail_base_chirho.add(1);
                    ptr::write_volatile(
                        idx_ptr_chirho,
                        avail_idx_chirho.wrapping_add(1),
                    );
                }
                rx_vq_mut_chirho.avail_idx_chirho =
                    avail_idx_chirho.wrapping_add(1);
            }
        }

        // Notify device that RX buffers are available (queue 0).
        fence(NetOrdering::SeqCst);
        transport_chirho.notify_queue_chirho(0);

        crate::serial_debug_chirho!(
            "    [VNET-IO] Initialized — {} RX bufs posted, device ready",
            num_rx_bufs_chirho
        );

        Some(Self {
            transport_chirho,
            mac_addr_chirho: mac_chirho,
            mtu_val_chirho: ETHERNET_MTU_CHIRHO,
            initialized_chirho: true,
            rx_vq_chirho: rx_vq_mut_chirho,
            tx_vq_chirho,
            rx_vq_phys_base_chirho: rx_phys_base_chirho,
            tx_vq_phys_base_chirho: tx_phys_base_chirho,
            rx_buf_phys_base_chirho: rx_buf_phys_chirho,
            rx_buf_count_chirho: num_rx_bufs_chirho,
            sw_rx_queue_chirho: VecDeque::new(),
        })
    }

    /// Poll the RX used ring for received frames and move them to the
    /// software receive queue.
    fn poll_rx_io_chirho(&mut self) {
        let rx_vring_virt_chirho = (self.rx_vq_phys_base_chirho
            + crate::pagetable_chirho::phys_mem_offset_chirho()) as usize;
        let queue_size_chirho = self.rx_vq_chirho.size_chirho as usize;

        // Compute used ring location in DMA memory.
        let desc_table_bytes_chirho = queue_size_chirho * 16;
        let avail_ring_bytes_chirho = 4 + queue_size_chirho * 2 + 2;
        let avail_end_chirho = desc_table_bytes_chirho + avail_ring_bytes_chirho;
        let used_ring_offset_chirho =
            (avail_end_chirho + VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1)
                & !(VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1);
        let used_base_chirho =
            (rx_vring_virt_chirho + used_ring_offset_chirho) as *mut u16;

        // Read device's used idx.
        let device_used_idx_chirho: u16 = unsafe {
            ptr::read_volatile(used_base_chirho.add(1))
        };

        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let buf_size_chirho = VIRTIO_NET_HDR_SIZE_CHIRHO + MAX_FRAME_SIZE_CHIRHO;

        while self.rx_vq_chirho.last_used_idx_chirho != device_used_idx_chirho {
            let ring_idx_chirho =
                (self.rx_vq_chirho.last_used_idx_chirho % self.rx_vq_chirho.size_chirho)
                    as usize;

            // Read used element: each is { id: u32, len: u32 } = 8 bytes.
            // Used ring layout: [flags:u16][idx:u16][elem0:8bytes][elem1:8bytes]...
            let used_elem_base_chirho = unsafe {
                (used_base_chirho as *const u8).add(4) as *const VringUsedElemChirho
            };
            let elem_chirho: VringUsedElemChirho = unsafe {
                ptr::read_volatile(used_elem_base_chirho.add(ring_idx_chirho))
            };
            let desc_id_chirho = elem_chirho.id_chirho as usize;
            let bytes_written_chirho = elem_chirho.len_chirho as usize;

            if bytes_written_chirho > VIRTIO_NET_HDR_SIZE_CHIRHO
                && (desc_id_chirho as u16) < self.rx_buf_count_chirho
            {
                // Read the frame from DMA buffer (skip virtio-net header).
                let buf_virt_chirho = (self.rx_buf_phys_base_chirho
                    + (desc_id_chirho as u64) * (buf_size_chirho as u64)
                    + phys_offset_chirho) as *const u8;
                let frame_len_chirho =
                    bytes_written_chirho - VIRTIO_NET_HDR_SIZE_CHIRHO;
                let mut frame_data_chirho = alloc::vec![0u8; frame_len_chirho];
                unsafe {
                    ptr::copy_nonoverlapping(
                        buf_virt_chirho.add(VIRTIO_NET_HDR_SIZE_CHIRHO),
                        frame_data_chirho.as_mut_ptr(),
                        frame_len_chirho,
                    );
                }
                self.sw_rx_queue_chirho.push_back(frame_data_chirho);
            }

            // Re-post the buffer to the RX queue.
            let desc_base_chirho =
                rx_vring_virt_chirho as *mut VringDescChirho;
            let avail_base_chirho =
                (rx_vring_virt_chirho + desc_table_bytes_chirho) as *mut u16;

            let buf_phys_chirho = self.rx_buf_phys_base_chirho
                + (desc_id_chirho as u64) * (buf_size_chirho as u64);
            unsafe {
                ptr::write_volatile(
                    desc_base_chirho.add(desc_id_chirho),
                    VringDescChirho {
                        addr_chirho: buf_phys_chirho,
                        len_chirho: buf_size_chirho as u32,
                        flags_chirho: VNET_DESC_F_WRITE_CHIRHO,
                        next_chirho: 0,
                    },
                );

                let avail_idx_chirho = self.rx_vq_chirho.avail_idx_chirho;
                let slot_chirho =
                    (avail_idx_chirho % self.rx_vq_chirho.size_chirho) as usize;
                ptr::write_volatile(
                    avail_base_chirho.add(2 + slot_chirho),
                    desc_id_chirho as u16,
                );
                fence(NetOrdering::Release);
                ptr::write_volatile(
                    avail_base_chirho.add(1),
                    avail_idx_chirho.wrapping_add(1),
                );
            }
            self.rx_vq_chirho.avail_idx_chirho =
                self.rx_vq_chirho.avail_idx_chirho.wrapping_add(1);

            self.rx_vq_chirho.last_used_idx_chirho =
                self.rx_vq_chirho.last_used_idx_chirho.wrapping_add(1);
        }

        // Always notify device about RX queue availability so it keeps
        // injecting received frames.  Without this, the device may not
        // deliver frames if it thinks the RX ring is unchanged.
        self.transport_chirho.notify_queue_chirho(0);
    }

    /// Transmit a raw Ethernet frame through VirtIO-net I/O port transport.
    ///
    /// Builds a 2-descriptor chain in the TX vring DMA memory:
    ///   desc 0: virtio-net header (10 bytes, device-readable)
    ///   desc 1: ethernet frame data (device-readable)
    fn transmit_frame_io_chirho(&mut self, frame_chirho: &[u8]) {
        if !self.initialized_chirho {
            return;
        }

        let tx_vring_virt_chirho = (self.tx_vq_phys_base_chirho
            + crate::pagetable_chirho::phys_mem_offset_chirho()) as usize;
        let queue_size_chirho = self.tx_vq_chirho.size_chirho as usize;

        // Compute shared memory layout pointers.
        let desc_table_bytes_chirho = queue_size_chirho * 16;
        let desc_base_chirho = tx_vring_virt_chirho as *mut VringDescChirho;
        let avail_base_chirho =
            (tx_vring_virt_chirho + desc_table_bytes_chirho) as *mut u16;
        let avail_end_chirho = desc_table_bytes_chirho + 4 + queue_size_chirho * 2 + 2;
        let used_ring_offset_chirho =
            (avail_end_chirho + VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1)
                & !(VIRTIO_NET_LEGACY_QUEUE_ALIGN_CHIRHO - 1);
        let used_base_chirho =
            (tx_vring_virt_chirho + used_ring_offset_chirho) as *mut u16;

        // Allocate 2 descriptors (header + data).
        if self.tx_vq_chirho.num_free_chirho < 2 {
            crate::serial_debug_chirho!("[VNET-IO] TX: no free descriptors");
            return;
        }
        let d0_chirho = match self.tx_vq_chirho.alloc_desc_chirho() {
            Some(d_chirho) => d_chirho,
            None => return,
        };
        let d1_chirho = match self.tx_vq_chirho.alloc_desc_chirho() {
            Some(d_chirho) => d_chirho,
            None => {
                self.tx_vq_chirho.free_desc_chirho(d0_chirho);
                return;
            }
        };

        // Build the TX DMA buffer: [virtio-net-header (10 bytes)][frame data]
        let phys_offset_chirho = crate::pagetable_chirho::phys_mem_offset_chirho();
        let tx_dma_virt_chirho = (NET_TX_DMA_PHYS_CHIRHO + phys_offset_chirho) as *mut u8;

        // Write virtio-net header (10 bytes, all zeros = no offload).
        unsafe {
            ptr::write_bytes(tx_dma_virt_chirho, 0, VIRTIO_NET_HDR_SIZE_CHIRHO);
        }

        // Write frame data after the header.
        let frame_len_chirho = core::cmp::min(frame_chirho.len(), MAX_FRAME_SIZE_CHIRHO);
        unsafe {
            ptr::copy_nonoverlapping(
                frame_chirho.as_ptr(),
                tx_dma_virt_chirho.add(VIRTIO_NET_HDR_SIZE_CHIRHO),
                frame_len_chirho,
            );
        }

        let hdr_phys_chirho = NET_TX_DMA_PHYS_CHIRHO;
        let data_phys_chirho = NET_TX_DMA_PHYS_CHIRHO + VIRTIO_NET_HDR_SIZE_CHIRHO as u64;

        // Descriptor 0: virtio-net header (device-readable, chained).
        unsafe {
            ptr::write_volatile(
                desc_base_chirho.add(d0_chirho as usize),
                VringDescChirho {
                    addr_chirho: hdr_phys_chirho,
                    len_chirho: VIRTIO_NET_HDR_SIZE_CHIRHO as u32,
                    flags_chirho: VRING_DESC_F_NEXT_CHIRHO,
                    next_chirho: d1_chirho,
                },
            );
        }

        // Descriptor 1: frame data (device-readable, end of chain).
        unsafe {
            ptr::write_volatile(
                desc_base_chirho.add(d1_chirho as usize),
                VringDescChirho {
                    addr_chirho: data_phys_chirho,
                    len_chirho: frame_len_chirho as u32,
                    flags_chirho: 0,
                    next_chirho: 0,
                },
            );
        }

        // Update available ring in shared memory.
        let avail_idx_chirho = self.tx_vq_chirho.avail_idx_chirho;
        let ring_slot_chirho =
            (avail_idx_chirho % self.tx_vq_chirho.size_chirho) as usize;
        unsafe {
            ptr::write_volatile(avail_base_chirho.add(2 + ring_slot_chirho), d0_chirho);
            fence(NetOrdering::Release);
            ptr::write_volatile(
                avail_base_chirho.add(1),
                avail_idx_chirho.wrapping_add(1),
            );
        }
        self.tx_vq_chirho.avail_idx_chirho = avail_idx_chirho.wrapping_add(1);
        fence(NetOrdering::SeqCst);

        // Notify device (queue 1 = TX).
        self.transport_chirho.notify_queue_chirho(1);

        // Poll the used ring for TX completion (busy-wait).
        let last_used_chirho = self.tx_vq_chirho.last_used_idx_chirho;
        let mut spins_chirho: u32 = 0;
        loop {
            let used_idx_chirho = unsafe {
                ptr::read_volatile(used_base_chirho.add(1))
            };
            if used_idx_chirho != last_used_chirho {
                self.tx_vq_chirho.last_used_idx_chirho = used_idx_chirho;
                break;
            }
            core::hint::spin_loop();
            spins_chirho += 1;
            if spins_chirho > NETWORK_POLL_SHORT_CHIRHO {
                crate::serial_debug_chirho!(
                    "[VNET-IO] TX timeout after {} spins",
                    spins_chirho
                );
                break;
            }
        }

        // Free descriptors.
        self.tx_vq_chirho.free_desc_chirho(d0_chirho);
        self.tx_vq_chirho.free_desc_chirho(d1_chirho);
    }
}

impl NetDeviceChirho for VirtioNetIoDeviceChirho {
    fn send_packet_chirho(&mut self, data_chirho: &[u8]) {
        if !self.initialized_chirho {
            return;
        }
        crate::log_net_chirho!("[VNET-IO] TX: {} bytes", data_chirho.len());
        self.transmit_frame_io_chirho(data_chirho);
    }

    fn recv_packet_chirho(&mut self) -> Option<Vec<u8>> {
        self.poll_rx_io_chirho();
        self.sw_rx_queue_chirho.pop_front()
    }

    fn mac_address_chirho(&self) -> [u8; 6] {
        self.mac_addr_chirho
    }

    fn mtu_chirho(&self) -> usize {
        self.mtu_val_chirho
    }
}

/// Entry point called from `init_virtio_chirho` when a VirtIO-net PCI device
/// with an I/O BAR is detected.  Probes the device, registers it in
/// `NET_DEVICES_CHIRHO`, and logs the result.
pub fn probe_virtio_net_io_chirho(io_base_chirho: u16) {
    crate::serial_debug_chirho!(
        "[VNET-IO] Probing VirtIO-net I/O at base {:#06x}",
        io_base_chirho
    );

    match VirtioNetIoDeviceChirho::probe_io_chirho(io_base_chirho) {
        Some(net_dev_chirho) => {
            let mac_chirho = net_dev_chirho.mac_addr_chirho;
            let mut devs_chirho = NET_DEVICES_CHIRHO.lock();
            devs_chirho.push(Box::new(net_dev_chirho));
            let idx_chirho = devs_chirho.len() - 1;
            crate::serial_debug_chirho!(
                "[VNET-IO] Registered VirtIO-net (I/O) as interface {} — MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                idx_chirho,
                mac_chirho[0], mac_chirho[1], mac_chirho[2],
                mac_chirho[3], mac_chirho[4], mac_chirho[5],
            );
        }
        None => {
            crate::serial_debug_chirho!(
                "[VNET-IO] Probe failed at I/O base {:#06x}",
                io_base_chirho
            );
        }
    }
}

// ============================================================================
// A3-012: Real epoll implementation
// ============================================================================
#[allow(dead_code)] pub const EPOLLIN_CHIRHO: u32 = 0x001;
#[allow(dead_code)] pub const EPOLLOUT_CHIRHO: u32 = 0x004;
#[allow(dead_code)] pub const EPOLLERR_CHIRHO: u32 = 0x008;
#[allow(dead_code)] pub const EPOLLHUP_CHIRHO: u32 = 0x010;
#[allow(dead_code)] pub const EPOLLET_CHIRHO: u32 = 1 << 31;
#[allow(dead_code)] pub const EPOLL_CTL_ADD_CHIRHO: i32 = 1;
#[allow(dead_code)] pub const EPOLL_CTL_DEL_CHIRHO: i32 = 2;
#[allow(dead_code)] pub const EPOLL_CTL_MOD_CHIRHO: i32 = 3;

#[derive(Clone)]
pub struct EpollInterestChirho { pub fd_chirho: i32, pub events_chirho: u32, pub data_chirho: u64 }
pub struct EpollInstanceChirho { pub interests_chirho: Vec<EpollInterestChirho> }

const MAX_EPOLL_INSTANCES_CHIRHO: usize = 64;
static EPOLL_TABLE_CHIRHO: Mutex<[Option<EpollInstanceChirho>; MAX_EPOLL_INSTANCES_CHIRHO]> = Mutex::new([const { None }; MAX_EPOLL_INSTANCES_CHIRHO]);
const ENOENT_NET_CHIRHO: i64 = 2;
const ENOMEM_NET_CHIRHO: i64 = 12;

#[allow(dead_code)]
pub fn epoll_create_impl_chirho() -> i64 {
    let mut t_chirho = EPOLL_TABLE_CHIRHO.lock();
    for (i_chirho, s_chirho) in t_chirho.iter_mut().enumerate() {
        if s_chirho.is_none() { *s_chirho = Some(EpollInstanceChirho { interests_chirho: Vec::new() }); return (1000 + i_chirho) as i64; }
    }
    -ENOMEM_NET_CHIRHO
}

#[allow(dead_code)]
pub fn epoll_ctl_impl_chirho(epfd_chirho: i32, op_chirho: i32, fd_chirho: i32, ev_chirho: u32, dat_chirho: u64) -> i64 {
    let idx_chirho = (epfd_chirho - 1000) as usize;
    let mut t_chirho = EPOLL_TABLE_CHIRHO.lock();
    let inst_chirho = match t_chirho.get_mut(idx_chirho).and_then(|s_chirho| s_chirho.as_mut()) { Some(i_chirho) => i_chirho, None => return -EBADF_CHIRHO };
    match op_chirho {
        EPOLL_CTL_ADD_CHIRHO => { inst_chirho.interests_chirho.push(EpollInterestChirho { fd_chirho, events_chirho: ev_chirho, data_chirho: dat_chirho }); 0 }
        EPOLL_CTL_DEL_CHIRHO => { inst_chirho.interests_chirho.retain(|e_chirho| e_chirho.fd_chirho != fd_chirho); 0 }
        EPOLL_CTL_MOD_CHIRHO => { for e_chirho in inst_chirho.interests_chirho.iter_mut() { if e_chirho.fd_chirho == fd_chirho { e_chirho.events_chirho = ev_chirho; e_chirho.data_chirho = dat_chirho; return 0; } } -ENOENT_NET_CHIRHO }
        _ => -EINVAL_CHIRHO,
    }
}

#[allow(dead_code)]
pub fn epoll_wait_impl_chirho(epfd_chirho: i32, eo_chirho: u64, max_chirho: i32, _to_chirho: i32) -> i64 {
    let idx_chirho = (epfd_chirho - 1000) as usize;
    let t_chirho = EPOLL_TABLE_CHIRHO.lock();
    let inst_chirho = match t_chirho.get(idx_chirho).and_then(|s_chirho| s_chirho.as_ref()) { Some(i_chirho) => i_chirho, None => return -EBADF_CHIRHO };
    let st_chirho = SOCKET_TABLE_CHIRHO.lock();
    let mut cnt_chirho: i32 = 0;
    for int_chirho in &inst_chirho.interests_chirho {
        if cnt_chirho >= max_chirho { break; }
        let mut re_chirho: u32 = 0;
        let si_chirho = int_chirho.fd_chirho as usize;
        if si_chirho < st_chirho.len() { if let Some(ref sk_chirho) = st_chirho[si_chirho] {
            if !sk_chirho.recv_buf_chirho.is_empty() { re_chirho |= EPOLLIN_CHIRHO; }
            if sk_chirho.state_chirho == SocketStateChirho::ConnectedChirho || sk_chirho.state_chirho == SocketStateChirho::BoundChirho { re_chirho |= EPOLLOUT_CHIRHO; }
            if sk_chirho.state_chirho == SocketStateChirho::ClosedChirho { re_chirho |= EPOLLHUP_CHIRHO; }
        }}
        re_chirho &= int_chirho.events_chirho;
        if re_chirho != 0 && eo_chirho != 0 {
            let op_chirho = (eo_chirho + cnt_chirho as u64 * 12) as *mut u8;
            unsafe { core::ptr::write_unaligned(op_chirho as *mut u32, re_chirho); core::ptr::write_unaligned(op_chirho.add(4) as *mut u64, int_chirho.data_chirho); }
            cnt_chirho += 1;
        }
    }
    cnt_chirho as i64
}

// A3-013: DNS resolver — already implemented above (build_dns_query_chirho,
// parse_dns_answers_chirho, resolve_hostname_chirho, DNS_SERVER_CHIRHO).

// ============================================================================
// A3-014: Loopback device (127.0.0.1)
// ============================================================================
#[allow(dead_code)] pub const LOOPBACK_IPV4_CHIRHO: u32 = 0x7F000001;
#[allow(dead_code)] pub const LOOPBACK_NETMASK_CHIRHO: u32 = 0xFF000000;
#[derive(Clone)]
pub struct IfaceConfigChirho { pub name_chirho: alloc::string::String, pub ipv4_addr_chirho: u32, pub netmask_chirho: u32, pub flags_chirho: u32, pub mtu_val_chirho: u32 }
#[allow(dead_code)] pub const IFF_UP_CHIRHO: u32 = 0x1;
#[allow(dead_code)] pub const IFF_LOOPBACK_CHIRHO: u32 = 0x8;
#[allow(dead_code)] pub const IFF_RUNNING_CHIRHO: u32 = 0x40;
static IFACE_CONFIG_CHIRHO: Mutex<Vec<IfaceConfigChirho>> = Mutex::new(Vec::new());

#[allow(dead_code)]
pub fn init_loopback_ip_chirho() {
    let mut c_chirho = IFACE_CONFIG_CHIRHO.lock();
    c_chirho.push(IfaceConfigChirho { name_chirho: alloc::string::String::from("lo"), ipv4_addr_chirho: LOOPBACK_IPV4_CHIRHO, netmask_chirho: LOOPBACK_NETMASK_CHIRHO, flags_chirho: IFF_UP_CHIRHO | IFF_LOOPBACK_CHIRHO | IFF_RUNNING_CHIRHO, mtu_val_chirho: LOOPBACK_MTU_CHIRHO as u32 });
    crate::serial_debug_chirho!("[NET] Loopback: 127.0.0.1/8");
}

// ============================================================================
// A3-016: sendmsg/recvmsg — scatter-gather I/O
// ============================================================================
#[repr(C)] #[derive(Clone, Copy)] #[allow(dead_code)]
pub struct IoVecChirho { pub iov_base_chirho: u64, pub iov_len_chirho: u64 }

/// x86_64 musl/Linux `struct msghdr` layout used by `sendmsg(2)` / `recvmsg(2)`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MsgHdrChirho {
    pub msg_name_chirho: u64,
    pub msg_namelen_chirho: u32,
    pub msg_name_pad_chirho: u32,
    pub msg_iov_chirho: u64,
    pub msg_iovlen_chirho: i32,
    pub msg_iov_pad_chirho: i32,
    pub msg_control_chirho: u64,
    pub msg_controllen_chirho: u32,
    pub msg_control_pad_chirho: u32,
    pub msg_flags_chirho: i32,
    pub msg_flags_pad_chirho: i32,
}

const MAX_MSG_IOV_CHIRHO: usize = 64;

fn read_msghdr_from_user_chirho(msg_ptr_chirho: u64) -> Result<MsgHdrChirho, i64> {
    if msg_ptr_chirho == 0 {
        return Err(-EFAULT_CHIRHO);
    }

    let mut msg_hdr_chirho = MsgHdrChirho::default();
    let msg_hdr_bytes_chirho = unsafe {
        core::slice::from_raw_parts_mut(
            (&mut msg_hdr_chirho as *mut MsgHdrChirho).cast::<u8>(),
            core::mem::size_of::<MsgHdrChirho>(),
        )
    };
    crate::uaccess_chirho::copy_from_user_chirho(
        msg_hdr_bytes_chirho,
        msg_ptr_chirho,
        msg_hdr_bytes_chirho.len(),
    )
    .map_err(|_| -EFAULT_CHIRHO)?;
    Ok(msg_hdr_chirho)
}

fn write_msghdr_to_user_chirho(
    msg_ptr_chirho: u64,
    msg_hdr_chirho: &MsgHdrChirho,
) -> Result<(), i64> {
    let msg_hdr_bytes_chirho = unsafe {
        core::slice::from_raw_parts(
            (msg_hdr_chirho as *const MsgHdrChirho).cast::<u8>(),
            core::mem::size_of::<MsgHdrChirho>(),
        )
    };
    crate::uaccess_chirho::copy_to_user_chirho(
        msg_ptr_chirho,
        msg_hdr_bytes_chirho,
        msg_hdr_bytes_chirho.len(),
    )
    .map_err(|_| -EFAULT_CHIRHO)
}

fn read_iovec_array_chirho(
    iov_ptr_chirho: u64,
    iov_len_chirho: usize,
) -> Result<Vec<IoVecChirho>, i64> {
    if iov_len_chirho == 0 {
        return Ok(Vec::new());
    }
    if iov_len_chirho > MAX_MSG_IOV_CHIRHO {
        return Err(-EINVAL_CHIRHO);
    }
    if iov_ptr_chirho == 0 {
        return Err(-EFAULT_CHIRHO);
    }

    let total_size_chirho = iov_len_chirho
        .checked_mul(core::mem::size_of::<IoVecChirho>())
        .ok_or(-EINVAL_CHIRHO)?;
    let mut iovec_entries_chirho = alloc::vec![
        IoVecChirho {
            iov_base_chirho: 0,
            iov_len_chirho: 0,
        };
        iov_len_chirho
    ];
    let iovec_bytes_chirho = unsafe {
        core::slice::from_raw_parts_mut(
            iovec_entries_chirho.as_mut_ptr().cast::<u8>(),
            total_size_chirho,
        )
    };
    crate::uaccess_chirho::copy_from_user_chirho(
        iovec_bytes_chirho,
        iov_ptr_chirho,
        total_size_chirho,
    )
    .map_err(|_| -EFAULT_CHIRHO)?;
    Ok(iovec_entries_chirho)
}

fn total_iovec_len_chirho(
    iov_ptr_chirho: u64,
    iov_len_chirho: usize,
) -> Result<usize, i64> {
    let iovec_entries_chirho = read_iovec_array_chirho(iov_ptr_chirho, iov_len_chirho)?;
    let mut total_len_chirho = 0usize;
    for iovec_entry_chirho in iovec_entries_chirho {
        total_len_chirho = total_len_chirho
            .saturating_add(iovec_entry_chirho.iov_len_chirho as usize)
            .min(SOCKET_SEND_MAX_CHIRHO);
    }
    Ok(total_len_chirho)
}

fn gather_iovec_checked_chirho(
    iov_ptr_chirho: u64,
    iov_len_chirho: usize,
) -> Result<Vec<u8>, i64> {
    let iovec_entries_chirho = read_iovec_array_chirho(iov_ptr_chirho, iov_len_chirho)?;
    let mut gathered_data_chirho =
        Vec::with_capacity(total_iovec_len_chirho(iov_ptr_chirho, iov_len_chirho)?);
    for iovec_entry_chirho in iovec_entries_chirho {
        if iovec_entry_chirho.iov_base_chirho == 0 || iovec_entry_chirho.iov_len_chirho == 0 {
            continue;
        }
        let remaining_capacity_chirho =
            SOCKET_SEND_MAX_CHIRHO.saturating_sub(gathered_data_chirho.len());
        if remaining_capacity_chirho == 0 {
            break;
        }
        let chunk_len_chirho =
            core::cmp::min(iovec_entry_chirho.iov_len_chirho as usize, remaining_capacity_chirho);
        let start_len_chirho = gathered_data_chirho.len();
        gathered_data_chirho.resize(start_len_chirho + chunk_len_chirho, 0);
        crate::uaccess_chirho::copy_from_user_chirho(
            &mut gathered_data_chirho[start_len_chirho..start_len_chirho + chunk_len_chirho],
            iovec_entry_chirho.iov_base_chirho,
            chunk_len_chirho,
        )
        .map_err(|_| -EFAULT_CHIRHO)?;
    }
    Ok(gathered_data_chirho)
}

fn scatter_iovec_checked_chirho(
    iov_ptr_chirho: u64,
    iov_len_chirho: usize,
    data_chirho: &[u8],
) -> Result<usize, i64> {
    let iovec_entries_chirho = read_iovec_array_chirho(iov_ptr_chirho, iov_len_chirho)?;
    let mut written_chirho = 0usize;
    for iovec_entry_chirho in iovec_entries_chirho {
        if written_chirho >= data_chirho.len() {
            break;
        }
        if iovec_entry_chirho.iov_base_chirho == 0 || iovec_entry_chirho.iov_len_chirho == 0 {
            continue;
        }
        let chunk_len_chirho = core::cmp::min(
            iovec_entry_chirho.iov_len_chirho as usize,
            data_chirho.len() - written_chirho,
        );
        crate::uaccess_chirho::copy_to_user_chirho(
            iovec_entry_chirho.iov_base_chirho,
            &data_chirho[written_chirho..written_chirho + chunk_len_chirho],
            chunk_len_chirho,
        )
        .map_err(|_| -EFAULT_CHIRHO)?;
        written_chirho += chunk_len_chirho;
    }
    Ok(written_chirho)
}

#[allow(dead_code)]
pub fn gather_iovec_chirho(ip_chirho: u64, il_chirho: u64) -> Vec<u8> {
    gather_iovec_checked_chirho(ip_chirho, il_chirho as usize).unwrap_or_else(|_| Vec::new())
}

#[allow(dead_code)]
pub fn scatter_iovec_chirho(ip_chirho: u64, il_chirho: u64, data_chirho: &[u8]) -> usize {
    scatter_iovec_checked_chirho(ip_chirho, il_chirho as usize, data_chirho).unwrap_or(0)
}

// ============================================================================
// A3-017: AF_UNIX sockets
// ============================================================================
const MAX_UNIX_SOCKETS_CHIRHO: usize = 64;
pub struct UnixSocketChirho { pub path_chirho: Option<alloc::string::String>, pub recv_buf_chirho: VecDeque<Vec<u8>>, pub peer_idx_chirho: Option<usize>, pub sock_type_chirho: u32, pub backlog_chirho: VecDeque<usize>, pub listening_chirho: bool }
static UNIX_SOCKET_TABLE_CHIRHO: Mutex<[Option<UnixSocketChirho>; MAX_UNIX_SOCKETS_CHIRHO]> = Mutex::new([const { None }; MAX_UNIX_SOCKETS_CHIRHO]);

#[allow(dead_code)] pub fn unix_socket_create_chirho(st_chirho: u32) -> Option<usize> { let mut t_chirho = UNIX_SOCKET_TABLE_CHIRHO.lock(); for (i_chirho, s_chirho) in t_chirho.iter_mut().enumerate() { if s_chirho.is_none() { *s_chirho = Some(UnixSocketChirho { path_chirho: None, recv_buf_chirho: VecDeque::new(), peer_idx_chirho: None, sock_type_chirho: st_chirho, backlog_chirho: VecDeque::new(), listening_chirho: false }); return Some(i_chirho); } } None }
#[allow(dead_code)] pub fn unix_socket_bind_chirho(idx_chirho: usize, p_chirho: &str) -> i64 { let mut t_chirho = UNIX_SOCKET_TABLE_CHIRHO.lock(); for s_chirho in t_chirho.iter() { if let Some(ref sk_chirho) = s_chirho { if let Some(ref pp_chirho) = sk_chirho.path_chirho { if pp_chirho.as_str() == p_chirho { return -EADDRINUSE_CHIRHO; } } } } if let Some(ref mut sk_chirho) = t_chirho[idx_chirho] { sk_chirho.path_chirho = Some(alloc::string::String::from(p_chirho)); return 0; } -EBADF_CHIRHO }
#[allow(dead_code)] pub fn unix_socket_connect_chirho(ci_chirho: usize, p_chirho: &str) -> i64 { let mut t_chirho = UNIX_SOCKET_TABLE_CHIRHO.lock(); let mut si_chirho: Option<usize> = None; for (i_chirho, s_chirho) in t_chirho.iter().enumerate() { if let Some(ref sk_chirho) = s_chirho { if sk_chirho.listening_chirho { if let Some(ref pp_chirho) = sk_chirho.path_chirho { if pp_chirho.as_str() == p_chirho { si_chirho = Some(i_chirho); break; } } } } } let sv_chirho = match si_chirho { Some(v_chirho) => v_chirho, None => return -ECONNREFUSED_CHIRHO }; if let Some(ref mut s_chirho) = t_chirho[sv_chirho] { s_chirho.backlog_chirho.push_back(ci_chirho); } if let Some(ref mut c_chirho) = t_chirho[ci_chirho] { c_chirho.peer_idx_chirho = Some(sv_chirho); } 0 }
#[allow(dead_code)] pub fn unix_socket_send_chirho(idx_chirho: usize, d_chirho: &[u8]) -> i64 { let mut t_chirho = UNIX_SOCKET_TABLE_CHIRHO.lock(); let pi_chirho = match t_chirho.get(idx_chirho).and_then(|s_chirho| s_chirho.as_ref()) { Some(sk_chirho) => match sk_chirho.peer_idx_chirho { Some(p_chirho) => p_chirho, None => return -ENOTCONN_CHIRHO }, None => return -EBADF_CHIRHO }; if let Some(ref mut peer_chirho) = t_chirho[pi_chirho] { peer_chirho.recv_buf_chirho.push_back(d_chirho.to_vec()); return d_chirho.len() as i64; } -EBADF_CHIRHO }
#[allow(dead_code)] pub fn unix_socket_recv_chirho(idx_chirho: usize) -> Option<Vec<u8>> { let mut t_chirho = UNIX_SOCKET_TABLE_CHIRHO.lock(); t_chirho.get_mut(idx_chirho).and_then(|s_chirho| s_chirho.as_mut()).and_then(|sk_chirho| sk_chirho.recv_buf_chirho.pop_front()) }

// ============================================================================
// A3-018: setsockopt/getsockopt
// ============================================================================
#[allow(dead_code)] pub const SOL_SOCKET_CHIRHO: u64 = 1;
#[allow(dead_code)] pub const SO_REUSEADDR_CHIRHO: u64 = 2;
#[allow(dead_code)] pub const SO_TYPE_CHIRHO: u64 = 3;
#[allow(dead_code)] pub const SO_ERROR_CHIRHO: u64 = 4;
#[allow(dead_code)] pub const SO_KEEPALIVE_CHIRHO: u64 = 9;
#[allow(dead_code)] pub const SO_RCVBUF_CHIRHO: u64 = 8;
#[allow(dead_code)] pub const SO_SNDBUF_CHIRHO: u64 = 7;
#[allow(dead_code)] pub const TCP_NODELAY_OPT_CHIRHO: u64 = 1;
#[derive(Clone)] pub struct SocketOptionsChirho { pub reuseaddr_chirho: bool, pub keepalive_chirho: bool, pub nodelay_chirho: bool, pub rcvbuf_size_chirho: u32, pub sndbuf_size_chirho: u32 }
const MAX_SOCK_OPTS_CHIRHO: usize = 128;
static SOCKET_OPTS_CHIRHO: Mutex<[SocketOptionsChirho; MAX_SOCK_OPTS_CHIRHO]> = Mutex::new([const { SocketOptionsChirho { reuseaddr_chirho: false, keepalive_chirho: false, nodelay_chirho: false, rcvbuf_size_chirho: 87380, sndbuf_size_chirho: 16384 } }; MAX_SOCK_OPTS_CHIRHO]);
#[allow(dead_code)]
pub fn setsockopt_impl_chirho(
    si_chirho: usize,
    lv_chirho: u64,
    nm_chirho: u64,
    vp_chirho: u64,
    vl_chirho: u64,
) -> i64 {
    if si_chirho >= MAX_SOCK_OPTS_CHIRHO {
        return -EINVAL_CHIRHO;
    }

    let mut opt_value_chirho = [0u8; 4];
    let parsed_value_chirho = if vl_chirho >= 4 && vp_chirho != 0 {
        let opt_value_len_chirho = opt_value_chirho.len();
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut opt_value_chirho,
            vp_chirho,
            opt_value_len_chirho,
        )
        .is_err()
        {
            return -EFAULT_CHIRHO;
        }
        u32::from_ne_bytes(opt_value_chirho)
    } else {
        0
    };

    let mut socket_options_chirho = SOCKET_OPTS_CHIRHO.lock();
    let socket_option_chirho = &mut socket_options_chirho[si_chirho];
    match (lv_chirho, nm_chirho) {
        (SOL_SOCKET_CHIRHO, SO_REUSEADDR_CHIRHO) => {
            socket_option_chirho.reuseaddr_chirho = parsed_value_chirho != 0;
        }
        (SOL_SOCKET_CHIRHO, SO_KEEPALIVE_CHIRHO) => {
            socket_option_chirho.keepalive_chirho = parsed_value_chirho != 0;
        }
        (SOL_SOCKET_CHIRHO, SO_RCVBUF_CHIRHO) => {
            socket_option_chirho.rcvbuf_size_chirho = parsed_value_chirho;
        }
        (SOL_SOCKET_CHIRHO, SO_SNDBUF_CHIRHO) => {
            socket_option_chirho.sndbuf_size_chirho = parsed_value_chirho;
        }
        (6, TCP_NODELAY_OPT_CHIRHO) => {
            socket_option_chirho.nodelay_chirho = parsed_value_chirho != 0;
        }
        _ => {}
    }
    0
}

#[allow(dead_code)]
pub fn getsockopt_impl_chirho(
    si_chirho: usize,
    lv_chirho: u64,
    nm_chirho: u64,
    vp_chirho: u64,
    lp_chirho: u64,
) -> i64 {
    if si_chirho >= MAX_SOCK_OPTS_CHIRHO {
        return -EINVAL_CHIRHO;
    }
    if vp_chirho == 0 || lp_chirho == 0 {
        return -EFAULT_CHIRHO;
    }

    let mut requested_len_bytes_chirho = [0u8; 4];
    let requested_len_size_chirho = requested_len_bytes_chirho.len();
    if crate::uaccess_chirho::copy_from_user_chirho(
        &mut requested_len_bytes_chirho,
        lp_chirho,
        requested_len_size_chirho,
    )
    .is_err()
    {
        return -EFAULT_CHIRHO;
    }
    let requested_len_chirho = u32::from_ne_bytes(requested_len_bytes_chirho) as usize;

    let socket_options_chirho = SOCKET_OPTS_CHIRHO.lock();
    let socket_option_chirho = &socket_options_chirho[si_chirho];
    let socket_type_value_chirho = {
        let table_chirho = SOCKET_TABLE_CHIRHO.lock();
        table_chirho
            .get(si_chirho)
            .and_then(|slot_chirho| slot_chirho.as_ref())
            .map(|socket_chirho| (socket_chirho.sock_type_chirho & 0xF) as u32)
            .unwrap_or(0)
    };
    let opt_value_chirho: u32 = match (lv_chirho, nm_chirho) {
        (SOL_SOCKET_CHIRHO, SO_REUSEADDR_CHIRHO) => socket_option_chirho.reuseaddr_chirho as u32,
        (SOL_SOCKET_CHIRHO, SO_KEEPALIVE_CHIRHO) => socket_option_chirho.keepalive_chirho as u32,
        (SOL_SOCKET_CHIRHO, SO_RCVBUF_CHIRHO) => socket_option_chirho.rcvbuf_size_chirho,
        (SOL_SOCKET_CHIRHO, SO_SNDBUF_CHIRHO) => socket_option_chirho.sndbuf_size_chirho,
        (SOL_SOCKET_CHIRHO, SO_TYPE_CHIRHO) => socket_type_value_chirho,
        (SOL_SOCKET_CHIRHO, SO_ERROR_CHIRHO) => 0,
        (6, TCP_NODELAY_OPT_CHIRHO) => socket_option_chirho.nodelay_chirho as u32,
        _ => 0,
    };
    drop(socket_options_chirho);

    let opt_value_bytes_chirho = opt_value_chirho.to_ne_bytes();
    let write_len_chirho = core::cmp::min(requested_len_chirho, opt_value_bytes_chirho.len());
    if crate::uaccess_chirho::copy_to_user_chirho(
        vp_chirho,
        &opt_value_bytes_chirho[..write_len_chirho],
        write_len_chirho,
    )
    .is_err()
    {
        return -EFAULT_CHIRHO;
    }

    let actual_len_bytes_chirho = (opt_value_bytes_chirho.len() as u32).to_ne_bytes();
    if crate::uaccess_chirho::copy_to_user_chirho(
        lp_chirho,
        &actual_len_bytes_chirho,
        actual_len_bytes_chirho.len(),
    )
    .is_err()
    {
        return -EFAULT_CHIRHO;
    }

    0
}

// ============================================================================
// A3-019: Network config ioctls
// ============================================================================

/// Typed ioctl command — replaces raw integer matching.
/// Covers terminal, socket, and interface ioctls used by musl programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IoctlCommandChirho {
    /// TCGETS — get terminal attributes.
    TcgetsChirho,
    /// TIOCGWINSZ — get window size.
    TiocgwinszChirho,
    /// TIOCSWINSZ — set window size.
    TiocswinszChirho,
    /// FIONREAD — bytes available to read.
    FionreadChirho,
    /// SIOCGIFADDR — get interface address.
    SiocgifaddrChirho,
    /// SIOCSIFADDR — set interface address.
    SiocsifaddrChirho,
    /// SIOCGIFFLAGS — get interface flags.
    SiocgifflagsChirho,
    /// SIOCGIFHWADDR — get hardware address.
    SiocgifhwaddrChirho,
    /// TIOCGPTN — get PTY number.
    TiocgptnChirho,
    /// TIOCSPTLCK — lock/unlock PTY.
    TiocsptlckChirho,
}

impl IoctlCommandChirho {
    /// Convert a raw ioctl command number to a typed enum variant.
    #[allow(dead_code)]
    pub fn from_raw_chirho(cmd_chirho: u32) -> Option<Self> {
        match cmd_chirho {
            0x5401 => Some(Self::TcgetsChirho),
            0x5413 => Some(Self::TiocgwinszChirho),
            0x5414 => Some(Self::TiocswinszChirho),
            0x541B => Some(Self::FionreadChirho),
            0x8912 => Some(Self::SiocgifaddrChirho),
            0x8916 => Some(Self::SiocsifaddrChirho),
            0x8913 => Some(Self::SiocgifflagsChirho),
            0x8927 => Some(Self::SiocgifhwaddrChirho),
            0x80045430 => Some(Self::TiocgptnChirho),
            0x40045431 => Some(Self::TiocsptlckChirho),
            _ => None,
        }
    }
}

#[allow(dead_code)] pub const SIOCSIFADDR_CHIRHO: u64 = 0x8916;
#[allow(dead_code)] pub const SIOCGIFADDR_CHIRHO: u64 = 0x8915;
#[allow(dead_code)] pub const SIOCSIFFLAGS_CHIRHO: u64 = 0x8914;
#[allow(dead_code)] pub const SIOCGIFFLAGS_CHIRHO: u64 = 0x8913;
#[allow(dead_code)]
pub fn handle_net_ioctl_chirho(cmd_chirho: u64, ifr_chirho: u64) -> i64 {
    if ifr_chirho == 0 { return -EINVAL_CHIRHO; }
    let nb_chirho = unsafe { core::slice::from_raw_parts(ifr_chirho as *const u8, 16) };
    let mut nm_chirho = alloc::string::String::new();
    for &b_chirho in nb_chirho { if b_chirho == 0 { break; } nm_chirho.push(b_chirho as char); }
    let mut cfg_chirho = IFACE_CONFIG_CHIRHO.lock();
    match cmd_chirho {
        SIOCGIFADDR_CHIRHO => { for ic_chirho in cfg_chirho.iter() { if ic_chirho.name_chirho == nm_chirho { let ap_chirho = (ifr_chirho + 16) as *mut u8; let ab_chirho = ic_chirho.ipv4_addr_chirho.to_be_bytes(); unsafe { core::ptr::write(ap_chirho, 2); core::ptr::write(ap_chirho.add(1), 0); core::ptr::write(ap_chirho.add(2), 0); core::ptr::write(ap_chirho.add(3), 0); for (j_chirho, byte_chirho) in ab_chirho.iter().enumerate() { core::ptr::write(ap_chirho.add(4+j_chirho), *byte_chirho); } } return 0; } } -ENOENT_NET_CHIRHO }
        SIOCSIFADDR_CHIRHO => { let ap_chirho = (ifr_chirho + 20) as *const u8; let ad_chirho = unsafe { u32::from_be_bytes([*ap_chirho, *ap_chirho.add(1), *ap_chirho.add(2), *ap_chirho.add(3)]) }; for ic_chirho in cfg_chirho.iter_mut() { if ic_chirho.name_chirho == nm_chirho { ic_chirho.ipv4_addr_chirho = ad_chirho; return 0; } } cfg_chirho.push(IfaceConfigChirho { name_chirho: nm_chirho, ipv4_addr_chirho: ad_chirho, netmask_chirho: 0xFFFFFF00, flags_chirho: IFF_UP_CHIRHO|IFF_RUNNING_CHIRHO, mtu_val_chirho: ETHERNET_MTU_CHIRHO as u32 }); 0 }
        SIOCGIFFLAGS_CHIRHO => { for ic_chirho in cfg_chirho.iter() { if ic_chirho.name_chirho == nm_chirho { unsafe { core::ptr::write((ifr_chirho+16) as *mut u16, ic_chirho.flags_chirho as u16) }; return 0; } } -ENOENT_NET_CHIRHO }
        SIOCSIFFLAGS_CHIRHO => { let nf_chirho = unsafe { core::ptr::read((ifr_chirho+16) as *const u16) } as u32; for ic_chirho in cfg_chirho.iter_mut() { if ic_chirho.name_chirho == nm_chirho { ic_chirho.flags_chirho = nf_chirho; return 0; } } -ENOENT_NET_CHIRHO }
        _ => 0,
    }
}

// ============================================================================
// A3 supplementary: Network namespace + Netfilter hooks
// ============================================================================
#[allow(dead_code)] pub struct NetworkNamespaceChirho { pub id_chirho: u64, pub devices_chirho: Vec<alloc::string::String>, pub is_init_chirho: bool }
#[allow(dead_code)] static INIT_NETNS_CHIRHO: Mutex<Option<NetworkNamespaceChirho>> = Mutex::new(None);
#[allow(dead_code)] pub fn init_default_netns_chirho() { let mut ns_chirho = INIT_NETNS_CHIRHO.lock(); *ns_chirho = Some(NetworkNamespaceChirho { id_chirho: 0, devices_chirho: alloc::vec![alloc::string::String::from("lo")], is_init_chirho: true }); }

#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[repr(u32)] #[allow(dead_code)]
pub enum NfHookPointChirho { PreRoutingChirho = 0, LocalInChirho = 1, ForwardChirho = 2, LocalOutChirho = 3, PostRoutingChirho = 4 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[repr(u32)] #[allow(dead_code)]
pub enum NfVerdictChirho { AcceptChirho = 1, DropChirho = 0, QueueChirho = 3, RepeatChirho = 4 }
#[allow(dead_code)] pub struct NfHookEntryChirho { pub hook_chirho: NfHookPointChirho, pub priority_chirho: i32, pub handler_chirho: fn(&[u8]) -> NfVerdictChirho }
static NF_HOOKS_CHIRHO: Mutex<Vec<NfHookEntryChirho>> = Mutex::new(Vec::new());
#[allow(dead_code)] pub fn nf_register_hook_chirho(e_chirho: NfHookEntryChirho) { let mut h_chirho = NF_HOOKS_CHIRHO.lock(); let p_chirho = h_chirho.iter().position(|x_chirho| x_chirho.hook_chirho == e_chirho.hook_chirho && x_chirho.priority_chirho > e_chirho.priority_chirho).unwrap_or(h_chirho.len()); h_chirho.insert(p_chirho, e_chirho); }
#[allow(dead_code)] pub fn nf_hook_chirho(hp_chirho: NfHookPointChirho, pkt_chirho: &[u8]) -> NfVerdictChirho { let h_chirho = NF_HOOKS_CHIRHO.lock(); for en_chirho in h_chirho.iter() { if en_chirho.hook_chirho == hp_chirho && (en_chirho.handler_chirho)(pkt_chirho) == NfVerdictChirho::DropChirho { return NfVerdictChirho::DropChirho; } } NfVerdictChirho::AcceptChirho }
