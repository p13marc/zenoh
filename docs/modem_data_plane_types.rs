//! Modem Data Plane ICD - Rust Type Definitions
//!
//! **SUPERSEDED**: This module has been superseded by the Link Provider approach.
//! See `modem_driver_types.rs` v1.1.0 for the new link management types.
//!
//! The Link Provider approach uses Zenoh's native Unix transport instead of
//! a custom data plane protocol, providing better integration with Zenoh's
//! QoS, batching, and fragmentation features.
//!
//! This file is retained for historical reference only.
//!
//! Reference implementation of types defined in MODEM_DATA_PLANE_ICD.md
//! This file can be used by both modem drivers and Zenoh.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Protocol version for data plane
pub const DP_PROTOCOL_VERSION: u8 = 0x01;

/// Maximum payload size (1 MB)
pub const DP_MAX_PAYLOAD_SIZE: usize = 1024 * 1024;

/// Default ping interval
pub const DP_DEFAULT_PING_INTERVAL_MS: u32 = 1000;

/// Default ping timeout
pub const DP_DEFAULT_PING_TIMEOUT_MS: u32 = 3000;

/// Default fragment reassembly timeout
pub const DP_DEFAULT_FRAGMENT_TIMEOUT_MS: u32 = 30000;

// ============================================================================
// Message Flags
// ============================================================================

bitflags::bitflags! {
    /// Message flags for data plane messages
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MessageFlags: u8 {
        /// Express - bypass queues, minimal latency
        const EXPRESS = 0b0000_0001;
        /// Reliable - request delivery confirmation
        const RELIABLE = 0b0000_0010;
        /// ACK requested - explicit ACK needed
        const ACK_REQUESTED = 0b0000_0100;
        /// Fragment - this is a fragment
        const FRAGMENT = 0b0000_1000;
        /// Last fragment - last in fragmented sequence
        const LAST_FRAGMENT = 0b0001_0000;
        /// Priority mask (bits 5-7)
        const PRIORITY_MASK = 0b1110_0000;
    }
}

impl MessageFlags {
    /// Get priority from flags (0-7)
    pub fn priority(&self) -> u8 {
        (self.bits() >> 5) & 0x07
    }

    /// Set priority in flags
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.remove(Self::PRIORITY_MASK);
        self.insert(Self::from_bits_truncate((priority & 0x07) << 5));
        self
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Data plane message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DpMessageType {
    DataPlaneReady = 0x01,
    DataMessage = 0x02,
    ExpressMessage = 0x03,
    DeliveryStatus = 0x04,
    ReachabilityUpdate = 0x05,
    FlowControl = 0x06,
    Fragment = 0x07,
    TargetQuery = 0x08,
    TargetResponse = 0x09,
    Ping = 0x0A,
    Pong = 0x0B,
    Error = 0xFF,
}

// ============================================================================
// Target Types
// ============================================================================

/// Target address type
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    ZenohLocator = 0,
    MacAddress = 1,
    IpAddress = 2,
    RadioCallsign = 3,
    SatelliteTerminalId = 4,
    AcousticAddress = 5,
    Broadcast = 6,
    Multicast = 7,
    Custom = 255,
}

/// Target address
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub target_type: TargetType,
    #[serde(with = "serde_bytes")]
    pub address: Vec<u8>,
}

impl Target {
    /// Create a broadcast target
    pub fn broadcast() -> Self {
        Self {
            target_type: TargetType::Broadcast,
            address: vec![],
        }
    }

    /// Create a MAC address target
    pub fn mac(mac: [u8; 6]) -> Self {
        Self {
            target_type: TargetType::MacAddress,
            address: mac.to_vec(),
        }
    }

    /// Create an IPv4 target
    pub fn ipv4(ip: [u8; 4], port: u16) -> Self {
        let mut addr = ip.to_vec();
        addr.push((port >> 8) as u8);
        addr.push((port & 0xFF) as u8);
        Self {
            target_type: TargetType::IpAddress,
            address: addr,
        }
    }

    /// Create an IPv6 target
    pub fn ipv6(ip: [u8; 16], port: u16) -> Self {
        let mut addr = ip.to_vec();
        addr.push((port >> 8) as u8);
        addr.push((port & 0xFF) as u8);
        Self {
            target_type: TargetType::IpAddress,
            address: addr,
        }
    }

    /// Create a radio callsign target
    pub fn callsign(callsign: &str) -> Self {
        Self {
            target_type: TargetType::RadioCallsign,
            address: callsign.as_bytes().to_vec(),
        }
    }

    /// Create a Zenoh locator target
    pub fn zenoh_locator(locator: &str) -> Self {
        Self {
            target_type: TargetType::ZenohLocator,
            address: locator.as_bytes().to_vec(),
        }
    }
}

// ============================================================================
// Data Plane Ready
// ============================================================================

/// Data plane capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpCapabilities {
    pub max_mtu: u32,
    pub min_mtu: u32,
    pub supports_express: bool,
    pub supports_reliable: bool,
    pub supports_fragmentation: bool,
    pub supports_multicast: bool,
    pub max_pending_messages: u32,
    pub max_targets: u32,
    pub tx_queue_bytes: u32,
    pub rx_queue_bytes: u32,
}

impl Default for DpCapabilities {
    fn default() -> Self {
        Self {
            max_mtu: 1500,
            min_mtu: 256,
            supports_express: true,
            supports_reliable: true,
            supports_fragmentation: true,
            supports_multicast: false,
            max_pending_messages: 1000,
            max_targets: 256,
            tx_queue_bytes: 1024 * 1024,
            rx_queue_bytes: 1024 * 1024,
        }
    }
}

/// Data plane ready message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPlaneReady {
    pub modem_id: String,
    pub protocol_version: u8,
    pub capabilities: DpCapabilities,
}

// ============================================================================
// Data Message
// ============================================================================

/// Congestion control mode
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CongestionControl {
    Drop = 0,
    Block = 1,
}

impl Default for CongestionControl {
    fn default() -> Self {
        Self::Drop
    }
}

/// Message metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub zenoh_priority: u8,
    pub congestion_control: CongestionControl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_expr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "serde_bytes")]
    pub source_id: Option<Vec<u8>>,
}

/// Regular data message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMessage {
    pub message_id: u64,
    pub timestamp_ns: u64,
    pub target: Target,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

impl DataMessage {
    /// Create a new data message with current timestamp
    pub fn new(message_id: u64, target: Target, payload: Vec<u8>) -> Self {
        Self {
            message_id,
            timestamp_ns: current_timestamp_ns(),
            target,
            payload,
            metadata: None,
        }
    }

    /// Add metadata to the message
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Express Message
// ============================================================================

/// Express message metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressMetadata {
    pub deadline_ns: u64,
    pub zenoh_priority: u8,
    pub bypass_queue: bool,
    pub require_ack: bool,
    pub max_retries: u8,
}

impl ExpressMetadata {
    /// Create express metadata with a deadline duration from now
    pub fn with_deadline(deadline_from_now: Duration) -> Self {
        Self {
            deadline_ns: current_timestamp_ns() + deadline_from_now.as_nanos() as u64,
            zenoh_priority: 1, // RealTime by default
            bypass_queue: true,
            require_ack: false,
            max_retries: 0,
        }
    }
}

/// Express (high priority) message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressMessage {
    pub message_id: u64,
    pub timestamp_ns: u64,
    pub target: Target,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    pub express_metadata: ExpressMetadata,
}

impl ExpressMessage {
    /// Create a new express message
    pub fn new(message_id: u64, target: Target, payload: Vec<u8>, deadline: Duration) -> Self {
        Self {
            message_id,
            timestamp_ns: current_timestamp_ns(),
            target,
            payload,
            express_metadata: ExpressMetadata::with_deadline(deadline),
        }
    }

    /// Check if the deadline has passed
    pub fn is_expired(&self) -> bool {
        current_timestamp_ns() > self.express_metadata.deadline_ns
    }
}

// ============================================================================
// Delivery Status
// ============================================================================

/// Delivery status codes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatusCode {
    Delivered = 0,
    Acknowledged = 1,
    Queued = 2,
    Transmitted = 3,
    Failed = 4,
    Expired = 5,
    Rejected = 6,
    Unreachable = 7,
    Congested = 8,
}

/// Delivery details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeliveryDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_us: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_count: Option<u8>,
}

/// Delivery status notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStatus {
    pub message_id: u64,
    pub status: DeliveryStatusCode,
    pub timestamp_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<DeliveryDetails>,
}

impl DeliveryStatus {
    /// Create a transmitted status
    pub fn transmitted(message_id: u64) -> Self {
        Self {
            message_id,
            status: DeliveryStatusCode::Transmitted,
            timestamp_ns: current_timestamp_ns(),
            details: None,
        }
    }

    /// Create a delivered status
    pub fn delivered(message_id: u64) -> Self {
        Self {
            message_id,
            status: DeliveryStatusCode::Delivered,
            timestamp_ns: current_timestamp_ns(),
            details: None,
        }
    }

    /// Create an unreachable status
    pub fn unreachable(message_id: u64, reason: impl Into<String>) -> Self {
        Self {
            message_id,
            status: DeliveryStatusCode::Unreachable,
            timestamp_ns: current_timestamp_ns(),
            details: Some(DeliveryDetails {
                failure_reason: Some(reason.into()),
                ..Default::default()
            }),
        }
    }

    /// Create a queued status
    pub fn queued(message_id: u64) -> Self {
        Self {
            message_id,
            status: DeliveryStatusCode::Queued,
            timestamp_ns: current_timestamp_ns(),
            details: None,
        }
    }

    /// Create an expired status
    pub fn expired(message_id: u64) -> Self {
        Self {
            message_id,
            status: DeliveryStatusCode::Expired,
            timestamp_ns: current_timestamp_ns(),
            details: None,
        }
    }
}

// ============================================================================
// Reachability
// ============================================================================

/// Reachability status
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityStatus {
    Reachable = 0,
    Unreachable = 1,
    Degraded = 2,
    Unknown = 3,
}

/// Reasons for unreachability
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreachableReason {
    LinkDown = 0,
    NoRoute = 1,
    TargetOffline = 2,
    Timeout = 3,
    Congestion = 4,
    Handover = 5,
    Interference = 6,
    OutOfRange = 7,
    AuthFailure = 8,
    Maintenance = 9,
    Unknown = 255,
}

/// Reachability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReachabilityDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<UnreachableReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_recovery_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternate_path: Option<bool>,
}

/// Reachability update notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachabilityUpdate {
    pub timestamp_ns: u64,
    pub target: Target,
    pub status: ReachabilityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ReachabilityDetails>,
}

impl ReachabilityUpdate {
    /// Create a reachable update
    pub fn reachable(target: Target) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            target,
            status: ReachabilityStatus::Reachable,
            details: None,
        }
    }

    /// Create an unreachable update
    pub fn unreachable(target: Target, reason: UnreachableReason) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            target,
            status: ReachabilityStatus::Unreachable,
            details: Some(ReachabilityDetails {
                reason: Some(reason),
                ..Default::default()
            }),
        }
    }

    /// Create a degraded update
    pub fn degraded(target: Target, quality_score: u8) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            target,
            status: ReachabilityStatus::Degraded,
            details: Some(ReachabilityDetails {
                quality_score: Some(quality_score),
                ..Default::default()
            }),
        }
    }
}

// ============================================================================
// Flow Control
// ============================================================================

/// Flow control type
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowControlType {
    Pause = 0,
    Resume = 1,
    SlowDown = 2,
    SpeedUp = 3,
    QueueStatus = 4,
}

/// Flow control details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowControlDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_rate_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_rate_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_depth_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_capacity_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_depth_messages: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_water_mark_pct: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_water_mark_pct: Option<u8>,
}

/// Flow control message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowControl {
    pub timestamp_ns: u64,
    pub control_type: FlowControlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    pub details: FlowControlDetails,
}

impl FlowControl {
    /// Create a global pause
    pub fn pause(reason: impl Into<String>) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            control_type: FlowControlType::Pause,
            target: None,
            details: FlowControlDetails {
                reason: Some(reason.into()),
                ..Default::default()
            },
        }
    }

    /// Create a global resume
    pub fn resume() -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            control_type: FlowControlType::Resume,
            target: None,
            details: FlowControlDetails::default(),
        }
    }

    /// Create a slow down request
    pub fn slow_down(suggested_rate_bps: u64) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            control_type: FlowControlType::SlowDown,
            target: None,
            details: FlowControlDetails {
                suggested_rate_bps: Some(suggested_rate_bps),
                ..Default::default()
            },
        }
    }

    /// Create a queue status update
    pub fn queue_status(depth_bytes: u32, capacity_bytes: u32) -> Self {
        let fill_pct = (depth_bytes as f64 / capacity_bytes as f64 * 100.0) as u8;
        Self {
            timestamp_ns: current_timestamp_ns(),
            control_type: FlowControlType::QueueStatus,
            target: None,
            details: FlowControlDetails {
                queue_depth_bytes: Some(depth_bytes),
                queue_capacity_bytes: Some(capacity_bytes),
                high_water_mark_pct: if fill_pct >= 80 { Some(fill_pct) } else { None },
                low_water_mark_pct: if fill_pct <= 20 { Some(fill_pct) } else { None },
                ..Default::default()
            },
        }
    }
}

// ============================================================================
// Fragment
// ============================================================================

/// Fragment message for large payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    pub original_message_id: u64,
    pub fragment_index: u16,
    pub total_fragments: u16,
    pub fragment_offset: u32,
    #[serde(with = "serde_bytes")]
    pub fragment_data: Vec<u8>,
    pub original_length: u32,
}

impl Fragment {
    /// Check if this is the last fragment
    pub fn is_last(&self) -> bool {
        self.fragment_index + 1 == self.total_fragments
    }
}

// ============================================================================
// Target Query/Response
// ============================================================================

/// Target query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetQuery {
    pub query_id: u32,
    pub targets: Vec<Target>,
    pub timeout_ms: u32,
    pub detailed: bool,
}

/// Target status in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetStatus {
    pub target: Target,
    pub status: ReachabilityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ReachabilityDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_us: Option<u32>,
}

/// Target response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetResponse {
    pub query_id: u32,
    pub results: Vec<TargetStatus>,
}

// ============================================================================
// Ping/Pong
// ============================================================================

/// Ping message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping {
    pub timestamp_ns: u64,
    pub seq: u32,
}

impl Ping {
    pub fn new(seq: u32) -> Self {
        Self {
            timestamp_ns: current_timestamp_ns(),
            seq,
        }
    }
}

/// Pong message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {
    pub timestamp_ns: u64,
    pub seq: u32,
    pub processing_ns: u64,
}

impl Pong {
    pub fn from_ping(ping: &Ping, processing_start_ns: u64) -> Self {
        let now = current_timestamp_ns();
        Self {
            timestamp_ns: now,
            seq: ping.seq,
            processing_ns: now - processing_start_ns,
        }
    }
}

// ============================================================================
// Error
// ============================================================================

/// Error severity
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DpErrorSeverity {
    Warning = 0,
    Error = 1,
    Fatal = 2,
}

/// Data plane error codes
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DpErrorCode {
    InvalidTarget = 0x0100,
    TargetUnreachable = 0x0101,
    MtuExceeded = 0x0102,
    QueueFull = 0x0103,
    DeadlineMissed = 0x0104,
    FragmentationFailed = 0x0105,
    DeliveryTimeout = 0x0106,
    AckTimeout = 0x0107,
    LinkDown = 0x0108,
    AuthFailed = 0x0109,
    EncryptionError = 0x010A,
    ProtocolError = 0x010B,
    ResourceExhausted = 0x010C,
    Vendor(u16),
}

/// Data plane error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpError {
    pub error_code: u32,
    pub severity: DpErrorSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_message_id: Option<u64>,
    pub recoverable: bool,
}

// ============================================================================
// Message Envelope
// ============================================================================

/// Unified message enum for the data plane
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum DpMessage {
    DataPlaneReady(DataPlaneReady),
    DataMessage(DataMessage),
    ExpressMessage(ExpressMessage),
    DeliveryStatus(DeliveryStatus),
    ReachabilityUpdate(ReachabilityUpdate),
    FlowControl(FlowControl),
    Fragment(Fragment),
    TargetQuery(TargetQuery),
    TargetResponse(TargetResponse),
    Ping(Ping),
    Pong(Pong),
    Error(DpError),
}

impl DpMessage {
    /// Get the message type ID
    pub fn message_type(&self) -> DpMessageType {
        match self {
            DpMessage::DataPlaneReady(_) => DpMessageType::DataPlaneReady,
            DpMessage::DataMessage(_) => DpMessageType::DataMessage,
            DpMessage::ExpressMessage(_) => DpMessageType::ExpressMessage,
            DpMessage::DeliveryStatus(_) => DpMessageType::DeliveryStatus,
            DpMessage::ReachabilityUpdate(_) => DpMessageType::ReachabilityUpdate,
            DpMessage::FlowControl(_) => DpMessageType::FlowControl,
            DpMessage::Fragment(_) => DpMessageType::Fragment,
            DpMessage::TargetQuery(_) => DpMessageType::TargetQuery,
            DpMessage::TargetResponse(_) => DpMessageType::TargetResponse,
            DpMessage::Ping(_) => DpMessageType::Ping,
            DpMessage::Pong(_) => DpMessageType::Pong,
            DpMessage::Error(_) => DpMessageType::Error,
        }
    }
}

// ============================================================================
// Codec
// ============================================================================

/// Frame header for data plane messages
#[derive(Debug, Clone)]
pub struct DpFrameHeader {
    pub version: u8,
    pub message_type: DpMessageType,
    pub flags: MessageFlags,
    pub reserved: u8,
    pub sequence: u32,
}

/// Encode a data plane message
pub fn encode_dp_message(
    msg: &DpMessage,
    seq: u32,
    flags: MessageFlags,
) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    let payload = rmp_serde::to_vec_named(msg)?;

    let total_len = 8 + payload.len(); // 8 bytes header
    let mut buf = Vec::with_capacity(4 + total_len);

    // Length (big-endian u32)
    buf.extend_from_slice(&(total_len as u32).to_be_bytes());

    // Header
    buf.push(DP_PROTOCOL_VERSION);
    buf.push(msg.message_type() as u8);
    buf.push(flags.bits());
    buf.push(0); // Reserved

    // Sequence number
    buf.extend_from_slice(&seq.to_be_bytes());

    // Payload
    buf.extend(payload);

    Ok(buf)
}

/// Decode error
#[derive(Debug)]
pub enum DpDecodeError {
    TooShort,
    UnsupportedVersion(u8),
    Deserialize(rmp_serde::decode::Error),
}

impl std::fmt::Display for DpDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DpDecodeError::TooShort => write!(f, "message too short"),
            DpDecodeError::UnsupportedVersion(v) => write!(f, "unsupported version: {}", v),
            DpDecodeError::Deserialize(e) => write!(f, "deserialize error: {}", e),
        }
    }
}

impl std::error::Error for DpDecodeError {}

/// Decode a data plane message (after reading length prefix)
pub fn decode_dp_message(data: &[u8]) -> Result<(DpMessage, DpFrameHeader), DpDecodeError> {
    if data.len() < 8 {
        return Err(DpDecodeError::TooShort);
    }

    let version = data[0];
    if version != DP_PROTOCOL_VERSION {
        return Err(DpDecodeError::UnsupportedVersion(version));
    }

    let message_type_byte = data[1];
    let flags = MessageFlags::from_bits_truncate(data[2]);
    let reserved = data[3];
    let sequence = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

    let header = DpFrameHeader {
        version,
        message_type: unsafe { std::mem::transmute(message_type_byte) },
        flags,
        reserved,
        sequence,
    };

    let payload = &data[8..];
    let msg = rmp_serde::from_slice(payload).map_err(DpDecodeError::Deserialize)?;

    Ok((msg, header))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get current timestamp in nanoseconds since UNIX epoch
pub fn current_timestamp_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_creation() {
        let mac = Target::mac([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        assert_eq!(mac.target_type, TargetType::MacAddress);
        assert_eq!(mac.address.len(), 6);

        let broadcast = Target::broadcast();
        assert_eq!(broadcast.target_type, TargetType::Broadcast);

        let callsign = Target::callsign("ALPHA1");
        assert_eq!(callsign.target_type, TargetType::RadioCallsign);
    }

    #[test]
    fn test_delivery_status() {
        let transmitted = DeliveryStatus::transmitted(42);
        assert_eq!(transmitted.message_id, 42);
        assert_eq!(transmitted.status, DeliveryStatusCode::Transmitted);

        let unreachable = DeliveryStatus::unreachable(123, "No route to host");
        assert_eq!(unreachable.status, DeliveryStatusCode::Unreachable);
        assert!(unreachable.details.is_some());
    }

    #[test]
    fn test_reachability_update() {
        let target = Target::callsign("BRAVO2");
        let update = ReachabilityUpdate::unreachable(target.clone(), UnreachableReason::Timeout);

        assert_eq!(update.status, ReachabilityStatus::Unreachable);
        assert!(update.details.is_some());
        assert_eq!(
            update.details.unwrap().reason,
            Some(UnreachableReason::Timeout)
        );
    }

    #[test]
    fn test_flow_control() {
        let pause = FlowControl::pause("TX buffer full");
        assert_eq!(pause.control_type, FlowControlType::Pause);
        assert!(pause.target.is_none()); // Global

        let slow_down = FlowControl::slow_down(500_000);
        assert_eq!(slow_down.control_type, FlowControlType::SlowDown);
        assert_eq!(slow_down.details.suggested_rate_bps, Some(500_000));
    }

    #[test]
    fn test_express_message_expiry() {
        let target = Target::broadcast();
        let msg = ExpressMessage::new(1, target, vec![1, 2, 3], Duration::from_millis(100));

        // Message should not be expired immediately
        assert!(!msg.is_expired());
    }

    #[test]
    fn test_message_flags() {
        let flags = MessageFlags::EXPRESS | MessageFlags::RELIABLE;
        assert!(flags.contains(MessageFlags::EXPRESS));
        assert!(flags.contains(MessageFlags::RELIABLE));

        let flags_with_priority = flags.with_priority(2);
        assert_eq!(flags_with_priority.priority(), 2);
    }

    #[test]
    fn test_message_roundtrip() {
        let msg = DpMessage::Ping(Ping::new(42));
        let encoded = encode_dp_message(&msg, 1, MessageFlags::empty()).unwrap();
        let (decoded, header) = decode_dp_message(&encoded[4..]).unwrap(); // Skip length prefix

        match decoded {
            DpMessage::Ping(ping) => assert_eq!(ping.seq, 42),
            _ => panic!("wrong message type"),
        }
        assert_eq!(header.sequence, 1);
    }
}
