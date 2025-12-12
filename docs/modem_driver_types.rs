//! Modem Driver ICD - Rust Type Definitions
//!
//! Reference implementation of types defined in MODEM_DRIVER_ICD.md v1.1.0
//! This file can be used by both modem drivers and Zenoh.
//!
//! ## Version History
//! - v1.0.0: Initial OAM/control plane types
//! - v1.1.0: Added link management types for dynamic link provider support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Protocol version (1.1.0)
pub const PROTOCOL_VERSION: u8 = 0x01;
pub const PROTOCOL_MINOR_VERSION: u8 = 0x01;

/// Maximum message payload size (64KB)
pub const MAX_PAYLOAD_SIZE: usize = 65536;

// ============================================================================
// Frame Header
// ============================================================================

/// Message types for the protocol
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    // OAM Messages (0x01-0x06)
    ModemInfo = 0x01,
    MetricsUpdate = 0x02,
    StateChange = 0x03,
    Alert = 0x04,
    BandwidthUpdate = 0x05,
    Heartbeat = 0x06,
    // Request/Response (0x10-0x13)
    Request = 0x10,
    Response = 0x11,
    Configure = 0x12,
    ConfigureAck = 0x13,
    // Link Management (0x20-0x24) - v1.1.0
    LinkAvailable = 0x20,
    LinkUnavailable = 0x21,
    LinkUpdate = 0x22,
    LinkQuery = 0x23,
    LinkQueryResponse = 0x24,
    // Error
    Error = 0xFF,
}

/// Frame header (4 bytes after length)
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub version: u8,
    pub message_type: MessageType,
    pub reserved: u16,
}

// ============================================================================
// ModemInfo (0x01)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModemInfo {
    pub modem_id: String,
    pub modem_type: ModemType,
    pub model: String,
    pub firmware_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub capabilities: Capabilities,
    /// Link capabilities (v1.1.0) - describes transport link provisioning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_capabilities: Option<LinkCapabilities>,
    pub initial_state: ModemState,
    pub initial_metrics: Metrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModemType {
    RadioHf,
    RadioVhf,
    RadioUhf,
    RadioTactical,
    RadioMesh,
    SatelliteGeo,
    SatelliteLeo,
    SatelliteMeo,
    AcousticUnderwater,
    OpticalFso,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub supports_reliable_mode: bool,
    pub supports_throughput_mode: bool,
    pub supports_power_control: bool,
    pub supports_frequency_hop: bool,
    pub supports_encryption: bool,
    pub max_bandwidth_bps: u64,
    pub min_bandwidth_bps: u64,
    pub metrics_interval_ms: u32,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_capabilities: HashMap<String, String>,
}

// ============================================================================
// ModemState
// ============================================================================

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModemState {
    Unknown = 0,
    Initializing = 1,
    Searching = 2,
    Synchronizing = 3,
    Connected = 4,
    Degraded = 5,
    HandoverInProgress = 6,
    Standby = 7,
    Disconnected = 8,
    Error = 9,
    Maintenance = 10,
}

impl Default for ModemState {
    fn default() -> Self {
        Self::Unknown
    }
}

// ============================================================================
// MetricsUpdate (0x02)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsUpdate {
    pub timestamp_ns: u64,
    pub seq: u32,
    pub signal: SignalMetrics,
    pub errors: ErrorMetrics,
    pub bandwidth: BandwidthMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satellite: Option<SatelliteMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radio: Option<RadioMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acoustic: Option<AcousticMetrics>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Alias for MetricsUpdate (used in ModemInfo)
pub type Metrics = MetricsUpdate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMetrics {
    /// SNR in centidecibels (dB * 100)
    pub snr_cdb: i32,
    /// RSSI in centi-dBm (dBm * 100)
    pub rssi_cdbm: i32,
    /// Signal quality 0-100%
    pub signal_quality_pct: u8,
    /// Noise floor in centi-dBm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_floor_cdbm: Option<i32>,
}

impl SignalMetrics {
    /// Get SNR in dB (floating point)
    pub fn snr_db(&self) -> f64 {
        self.snr_cdb as f64 / 100.0
    }

    /// Get RSSI in dBm (floating point)
    pub fn rssi_dbm(&self) -> f64 {
        self.rssi_cdbm as f64 / 100.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// BER as 10^(-exponent), e.g., 6 = 10^-6
    pub ber_exponent: i8,
    /// Packet Error Rate in parts per million
    pub per_ppm: u32,
    /// Frame Error Rate in parts per million
    pub fer_ppm: u32,
    /// CRC error count since last report
    pub crc_errors: u32,
    /// Retransmit count since last report
    pub retransmits: u32,
}

impl ErrorMetrics {
    /// Get BER as f64
    pub fn ber(&self) -> f64 {
        10.0_f64.powi(-(self.ber_exponent as i32))
    }

    /// Get PER as percentage
    pub fn per_pct(&self) -> f64 {
        self.per_ppm as f64 / 10000.0
    }

    /// Get FER as percentage
    pub fn fer_pct(&self) -> f64 {
        self.fer_ppm as f64 / 10000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthMetrics {
    /// Current TX rate in bps
    pub tx_rate_bps: u64,
    /// Current RX rate in bps
    pub rx_rate_bps: u64,
    /// Estimated available bandwidth in bps
    pub available_bps: u64,
    /// Link utilization 0-100%
    pub utilization_pct: u8,
    /// TX queue depth in bytes
    pub queue_depth_bytes: u32,
    /// TX queue capacity in bytes
    pub queue_capacity_bytes: u32,
}

impl BandwidthMetrics {
    /// Get queue fill ratio (0.0 to 1.0)
    pub fn queue_fill_ratio(&self) -> f64 {
        if self.queue_capacity_bytes == 0 {
            0.0
        } else {
            self.queue_depth_bytes as f64 / self.queue_capacity_bytes as f64
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteMetrics {
    /// Elevation in centidegrees (deg * 100)
    pub elevation_cdeg: i16,
    /// Azimuth in centidegrees
    pub azimuth_cdeg: u16,
    /// Doppler shift in Hz
    pub doppler_hz: i32,
    /// Range to satellite in km
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_km: Option<u32>,
    /// Satellite identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satellite_id: Option<String>,
    /// Beam identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beam_id: Option<String>,
    /// Handover imminent
    pub handover_pending: bool,
    /// Estimated ms until handover
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_to_handover_ms: Option<u32>,
}

impl SatelliteMetrics {
    /// Get elevation in degrees
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_cdeg as f64 / 100.0
    }

    /// Get azimuth in degrees
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth_cdeg as f64 / 100.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioMetrics {
    /// TX power in centi-dBm
    pub tx_power_cdbm: i16,
    /// Current frequency in Hz
    pub frequency_hz: u64,
    /// Channel bandwidth in Hz
    pub bandwidth_hz: u32,
    /// Current modulation scheme
    pub modulation: String,
    /// FEC rate
    pub fec_rate: String,
    /// Frequency hopping active
    pub hopping: bool,
    /// Encryption active
    pub crypto_active: bool,
}

impl RadioMetrics {
    /// Get TX power in dBm
    pub fn tx_power_dbm(&self) -> f64 {
        self.tx_power_cdbm as f64 / 100.0
    }

    /// Get frequency in MHz
    pub fn frequency_mhz(&self) -> f64 {
        self.frequency_hz as f64 / 1_000_000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcousticMetrics {
    /// Multipath spread in microseconds
    pub multipath_spread_us: u32,
    /// Doppler spread in centi-Hz
    pub doppler_spread_chz: i16,
    /// One-way propagation delay in ms
    pub propagation_delay_ms: u32,
    /// Ambient noise level in centi-dB
    pub ambient_noise_cdb: i16,
    /// Water temperature in centi-Celsius
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_temp_cdc: Option<i16>,
}

impl AcousticMetrics {
    /// Get multipath spread in milliseconds
    pub fn multipath_spread_ms(&self) -> f64 {
        self.multipath_spread_us as f64 / 1000.0
    }

    /// Get Doppler spread in Hz
    pub fn doppler_spread_hz(&self) -> f64 {
        self.doppler_spread_chz as f64 / 100.0
    }

    /// Get ambient noise in dB
    pub fn ambient_noise_db(&self) -> f64 {
        self.ambient_noise_cdb as f64 / 100.0
    }

    /// Get water temperature in Celsius
    pub fn water_temp_celsius(&self) -> Option<f64> {
        self.water_temp_cdc.map(|t| t as f64 / 100.0)
    }
}

// ============================================================================
// StateChange (0x03)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub timestamp_ns: u64,
    pub previous_state: ModemState,
    pub current_state: ModemState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<u32>,
}

// ============================================================================
// Alert (0x04)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub timestamp_ns: u64,
    pub alert_id: u32,
    pub severity: AlertSeverity,
    pub alert_type: AlertType,
    pub message: String,
    pub data: AlertData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u32>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    SignalDegrading = 1,
    HandoverImminent = 2,
    LinkFailureImminent = 3,
    BandwidthReduction = 4,
    InterferenceDetected = 5,
    EncryptionKeyExpiring = 6,
    HardwareFault = 7,
    TemperatureWarning = 8,
    PowerIssue = 9,
    Custom = 255,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertData {
    SignalDegrading {
        current_snr_cdb: i32,
        trend_cdb_per_sec: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        estimated_failure_ms: Option<u32>,
    },
    HandoverImminent {
        #[serde(skip_serializing_if = "Option::is_none")]
        current_satellite_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_satellite_id: Option<String>,
        estimated_duration_ms: u32,
        expected_outage_ms: u32,
    },
    LinkFailureImminent {
        reason: String,
        estimated_time_ms: u32,
    },
    BandwidthReduction {
        current_bps: u64,
        expected_bps: u64,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    InterferenceDetected {
        #[serde(skip_serializing_if = "Option::is_none")]
        frequency_hz: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        estimated_source: Option<String>,
        mitigation_active: bool,
    },
    Custom {
        vendor_type: String,
        data: HashMap<String, serde_json::Value>,
    },
}

// ============================================================================
// BandwidthUpdate (0x05)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthUpdate {
    pub timestamp_ns: u64,
    pub available_tx_bps: u64,
    pub available_rx_bps: u64,
    pub max_tx_bps: u64,
    pub max_rx_bps: u64,
    pub reason: BandwidthChangeReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_estimate_ms: Option<u32>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandwidthChangeReason {
    ModulationChange = 1,
    FecChange = 2,
    PowerChange = 3,
    Congestion = 4,
    Interference = 5,
    ScheduledEvent = 6,
    WeatherImpact = 7,
    HandoverTransition = 8,
    UserRequested = 9,
    Recovery = 10,
}

// ============================================================================
// Heartbeat (0x06)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub timestamp_ns: u64,
    pub seq: u32,
    /// true = request echo, false = echo response
    pub echo: bool,
}

// ============================================================================
// Request (0x10)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub request_id: u32,
    pub request_type: RequestType,
    pub data: RequestData,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    GetMetrics = 1,
    GetState = 2,
    GetInfo = 3,
    SetMode = 4,
    TrafficHint = 5,
    DiagnosticTest = 6,
    Custom = 255,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestData {
    GetMetrics {},
    GetState {},
    GetInfo {},
    SetMode {
        mode: OperatingMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    TrafficHint {
        expected_tx_bps: u64,
        expected_rx_bps: u64,
        /// Zenoh priority (0-7)
        priority: u8,
        duration_ms: u32,
        reliable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        latency_budget_ms: Option<u32>,
    },
    DiagnosticTest {
        test_type: String,
        #[serde(default)]
        parameters: HashMap<String, serde_json::Value>,
    },
    Custom {
        vendor_type: String,
        data: HashMap<String, serde_json::Value>,
    },
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    Normal = 0,
    Reliable = 1,
    Throughput = 2,
    LowPower = 3,
    LowLatency = 4,
    Stealth = 5,
}

impl Default for OperatingMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ============================================================================
// Response (0x11)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub request_id: u32,
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success = 0,
    InvalidRequest = 1,
    NotSupported = 2,
    Busy = 3,
    Failed = 4,
    Timeout = 5,
    PermissionDenied = 6,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseData {
    Metrics(MetricsUpdate),
    State(StateChange),
    Info(ModemInfo),
    ModeSet {
        applied_mode: OperatingMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        effective_until_ns: Option<u64>,
    },
    TrafficHintAck {
        accepted: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        adjusted_tx_bps: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        adjusted_rx_bps: Option<u64>,
    },
    DiagnosticResult {
        test_type: String,
        passed: bool,
        results: HashMap<String, serde_json::Value>,
    },
    Custom {
        vendor_type: String,
        data: HashMap<String, serde_json::Value>,
    },
}

// ============================================================================
// Configure (0x12)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configure {
    pub config_id: u32,
    pub config_type: ConfigType,
    pub data: ConfigData,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigType {
    MetricsInterval = 1,
    AlertThresholds = 2,
    PowerLimits = 3,
    BandwidthLimits = 4,
    Custom = 255,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigData {
    MetricsInterval {
        interval_ms: u32,
    },
    AlertThresholds {
        #[serde(skip_serializing_if = "Option::is_none")]
        snr_warning_cdb: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        snr_critical_cdb: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loss_warning_pct: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loss_critical_pct: Option<u8>,
    },
    PowerLimits {
        #[serde(skip_serializing_if = "Option::is_none")]
        max_tx_power_cdbm: Option<i16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        min_tx_power_cdbm: Option<i16>,
    },
    BandwidthLimits {
        #[serde(skip_serializing_if = "Option::is_none")]
        max_tx_bps: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_rx_bps: Option<u64>,
    },
    Custom {
        vendor_type: String,
        data: HashMap<String, serde_json::Value>,
    },
}

// ============================================================================
// ConfigureAck (0x13)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureAck {
    pub config_id: u32,
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_config: Option<ConfigData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

// ============================================================================
// Link Management Types (v1.1.0)
// ============================================================================

/// Link type enumeration
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    Unicast = 0,
    Broadcast = 1,
    Multicast = 2,
}

impl Default for LinkType {
    fn default() -> Self {
        Self::Unicast
    }
}

/// Target type enumeration
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    ZenohNode = 0,
    Gateway = 1,
    Relay = 2,
    Broadcast = 3,
}

impl Default for TargetType {
    fn default() -> Self {
        Self::ZenohNode
    }
}

/// Reliability mode for links
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    Reliable = 0,
    BestEffort = 1,
}

impl Default for Reliability {
    fn default() -> Self {
        Self::Reliable
    }
}

/// Link capabilities - describes what the modem can provide for transport links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkCapabilities {
    /// Base path for Unix sockets (e.g., "/run/zenoh/modem/radio0")
    pub socket_base_path: String,
    /// Unicast link capabilities
    pub unicast: UnicastCapabilities,
    /// Broadcast link capabilities (if supported)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broadcast: Option<BroadcastCapabilities>,
    /// Multicast link capabilities (if supported)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multicast: Option<MulticastCapabilities>,
}

/// Unicast link capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnicastCapabilities {
    /// Maximum MTU supported
    pub max_mtu: u32,
    /// Minimum MTU supported
    pub min_mtu: u32,
    /// Whether priority levels are supported
    pub supports_priorities: bool,
    /// Supported priority range (if supports_priorities is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_range: Option<PriorityRange>,
    /// Whether express (fast-path) is supported
    pub supports_express: bool,
    /// Priority threshold for express treatment (priorities 0 to this value get express)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub express_priority_threshold: Option<u8>,
    /// Whether reliable delivery is supported
    pub supports_reliable: bool,
    /// Whether best-effort delivery is supported
    pub supports_best_effort: bool,
    /// Default reliability mode
    pub default_reliability: Reliability,
    /// Maximum number of concurrent targets
    pub max_targets: u32,
    /// Maximum pending bytes in TX queue
    pub max_pending_bytes: u32,
}

/// Priority range specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityRange {
    /// Minimum priority value (highest priority)
    pub min: u8,
    /// Maximum priority value (lowest priority)
    pub max: u8,
}

/// Broadcast link capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastCapabilities {
    /// Maximum MTU for broadcast
    pub max_mtu: u32,
    /// Maximum recipients for broadcast
    pub max_recipients: u32,
    /// Whether broadcast supports reliable delivery
    pub supports_reliable: bool,
}

/// Multicast link capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulticastCapabilities {
    /// Maximum MTU for multicast
    pub max_mtu: u32,
    /// Maximum multicast groups
    pub max_groups: u32,
    /// Maximum members per group
    pub max_members_per_group: u32,
}

/// Target information for link management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    /// Unique target identifier
    pub target_id: String,
    /// Type of target
    pub target_type: TargetType,
    /// Human-readable target name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Target address (modem-specific format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

/// Link properties (current state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkProperties {
    /// Maximum transfer unit
    pub mtu: u32,
    /// Current bandwidth in bps
    pub bandwidth_bps: u64,
    /// One-way latency estimate in microseconds
    pub latency_us: u32,
    /// Reliability mode
    pub reliability: Reliability,
    /// Supported priority range
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_range: Option<PriorityRange>,
    /// Whether express is available on this link
    pub express_available: bool,
}

impl LinkProperties {
    /// Create default properties
    pub fn new(mtu: u32, bandwidth_bps: u64, latency_us: u32) -> Self {
        Self {
            mtu,
            bandwidth_bps,
            latency_us,
            reliability: Reliability::default(),
            priority_range: None,
            express_available: false,
        }
    }

    /// Get latency in milliseconds
    pub fn latency_ms(&self) -> f64 {
        self.latency_us as f64 / 1000.0
    }
}

/// Reason for link unavailability
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkUnavailableReason {
    TargetUnreachable = 0,
    LinkFailure = 1,
    Timeout = 2,
    Shutdown = 3,
    ResourceExhausted = 4,
    ConfigurationChange = 5,
    Other = 255,
}

impl Default for LinkUnavailableReason {
    fn default() -> Self {
        Self::Other
    }
}

// ============================================================================
// Link Management Messages (0x20-0x24)
// ============================================================================

/// LinkAvailable (0x20) - Modem notifies Zenoh that a link to a target is available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkAvailable {
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Unique identifier for this link
    pub link_id: String,
    /// Type of link
    pub link_type: LinkType,
    /// Zenoh locator string (e.g., "unixpipe//run/zenoh/modem/radio0/targets/node1.sock")
    pub locator: String,
    /// Target information
    pub target: TargetInfo,
    /// Initial link properties
    pub properties: LinkProperties,
}

impl LinkAvailable {
    /// Create a new LinkAvailable message with current timestamp
    pub fn new(link_id: impl Into<String>, locator: impl Into<String>, target: TargetInfo) -> Self {
        Self {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            link_id: link_id.into(),
            link_type: LinkType::Unicast,
            locator: locator.into(),
            target,
            properties: LinkProperties::new(1500, 0, 0),
        }
    }

    /// Set link type
    pub fn with_link_type(mut self, link_type: LinkType) -> Self {
        self.link_type = link_type;
        self
    }

    /// Set link properties
    pub fn with_properties(mut self, properties: LinkProperties) -> Self {
        self.properties = properties;
        self
    }
}

/// LinkUnavailable (0x21) - Modem notifies Zenoh that a link is no longer available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkUnavailable {
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Link identifier (must match a previously announced link)
    pub link_id: String,
    /// Reason for unavailability
    pub reason: LinkUnavailableReason,
    /// Human-readable message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Whether the link may become available again
    pub may_reconnect: bool,
    /// Estimated time until reconnection in milliseconds (if may_reconnect is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_reconnect_ms: Option<u32>,
}

impl LinkUnavailable {
    /// Create a new LinkUnavailable message with current timestamp
    pub fn new(link_id: impl Into<String>, reason: LinkUnavailableReason) -> Self {
        Self {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            link_id: link_id.into(),
            reason,
            message: None,
            may_reconnect: false,
            estimated_reconnect_ms: None,
        }
    }

    /// Set human-readable message
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Indicate that reconnection may occur
    pub fn may_reconnect_in(mut self, estimated_ms: u32) -> Self {
        self.may_reconnect = true;
        self.estimated_reconnect_ms = Some(estimated_ms);
        self
    }
}

/// LinkUpdate (0x22) - Modem updates link properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkUpdate {
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Link identifier (must match a previously announced link)
    pub link_id: String,
    /// Updated properties
    pub properties: LinkProperties,
    /// Reason for update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl LinkUpdate {
    /// Create a new LinkUpdate message with current timestamp
    pub fn new(link_id: impl Into<String>, properties: LinkProperties) -> Self {
        Self {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            link_id: link_id.into(),
            properties,
            reason: None,
        }
    }

    /// Set update reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// LinkQuery (0x23) - Zenoh queries modem for current links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkQuery {
    /// Query identifier for matching response
    pub query_id: u32,
    /// Optional filter by link type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type_filter: Option<LinkType>,
    /// Optional filter by target ID pattern (glob)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_filter: Option<String>,
}

impl LinkQuery {
    /// Create a new LinkQuery
    pub fn new(query_id: u32) -> Self {
        Self {
            query_id,
            link_type_filter: None,
            target_filter: None,
        }
    }

    /// Filter by link type
    pub fn with_link_type(mut self, link_type: LinkType) -> Self {
        self.link_type_filter = Some(link_type);
        self
    }

    /// Filter by target pattern
    pub fn with_target_filter(mut self, pattern: impl Into<String>) -> Self {
        self.target_filter = Some(pattern.into());
        self
    }
}

/// LinkQueryResponse (0x24) - Modem responds with current links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkQueryResponse {
    /// Query identifier (matches LinkQuery.query_id)
    pub query_id: u32,
    /// List of currently available links
    pub links: Vec<LinkAvailable>,
}

impl LinkQueryResponse {
    /// Create a new LinkQueryResponse
    pub fn new(query_id: u32, links: Vec<LinkAvailable>) -> Self {
        Self { query_id, links }
    }
}

// ============================================================================
// Error (0xFF)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub error_code: u32,
    pub severity: ErrorSeverity,
    pub message: String,
    pub recoverable: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub context: HashMap<String, serde_json::Value>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Warning = 0,
    Error = 1,
    Fatal = 2,
}

// ============================================================================
// Standard Error Codes
// ============================================================================

pub mod error_codes {
    pub const OK: u32 = 0x0000;
    pub const UNKNOWN: u32 = 0x0001;
    pub const INVALID_MESSAGE: u32 = 0x0002;
    pub const UNSUPPORTED_VERSION: u32 = 0x0003;
    pub const UNSUPPORTED_TYPE: u32 = 0x0004;
    pub const INVALID_PARAMETER: u32 = 0x0005;
    pub const TIMEOUT: u32 = 0x0006;
    pub const BUSY: u32 = 0x0007;
    pub const NOT_READY: u32 = 0x0008;
    pub const HARDWARE_FAULT: u32 = 0x0009;
    pub const PERMISSION_DENIED: u32 = 0x000A;

    /// Vendor-specific error codes start at 0x1000
    pub const VENDOR_BASE: u32 = 0x1000;
}

// ============================================================================
// Message Envelope (for serialization)
// ============================================================================

/// Unified message enum for serialization/deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum Message {
    // OAM Messages
    ModemInfo(ModemInfo),
    MetricsUpdate(MetricsUpdate),
    StateChange(StateChange),
    Alert(Alert),
    BandwidthUpdate(BandwidthUpdate),
    Heartbeat(Heartbeat),
    // Request/Response
    Request(Request),
    Response(Response),
    Configure(Configure),
    ConfigureAck(ConfigureAck),
    // Link Management (v1.1.0)
    LinkAvailable(LinkAvailable),
    LinkUnavailable(LinkUnavailable),
    LinkUpdate(LinkUpdate),
    LinkQuery(LinkQuery),
    LinkQueryResponse(LinkQueryResponse),
    // Error
    Error(Error),
}

impl Message {
    /// Get the message type ID
    pub fn message_type(&self) -> MessageType {
        match self {
            Message::ModemInfo(_) => MessageType::ModemInfo,
            Message::MetricsUpdate(_) => MessageType::MetricsUpdate,
            Message::StateChange(_) => MessageType::StateChange,
            Message::Alert(_) => MessageType::Alert,
            Message::BandwidthUpdate(_) => MessageType::BandwidthUpdate,
            Message::Heartbeat(_) => MessageType::Heartbeat,
            Message::Request(_) => MessageType::Request,
            Message::Response(_) => MessageType::Response,
            Message::Configure(_) => MessageType::Configure,
            Message::ConfigureAck(_) => MessageType::ConfigureAck,
            Message::LinkAvailable(_) => MessageType::LinkAvailable,
            Message::LinkUnavailable(_) => MessageType::LinkUnavailable,
            Message::LinkUpdate(_) => MessageType::LinkUpdate,
            Message::LinkQuery(_) => MessageType::LinkQuery,
            Message::LinkQueryResponse(_) => MessageType::LinkQueryResponse,
            Message::Error(_) => MessageType::Error,
        }
    }

    /// Check if this is a link management message (v1.1.0)
    pub fn is_link_management(&self) -> bool {
        matches!(
            self,
            Message::LinkAvailable(_)
                | Message::LinkUnavailable(_)
                | Message::LinkUpdate(_)
                | Message::LinkQuery(_)
                | Message::LinkQueryResponse(_)
        )
    }
}

// ============================================================================
// Codec helpers
// ============================================================================

/// Encode a message to bytes (MessagePack format with length prefix)
pub fn encode_message(msg: &Message) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    let payload = rmp_serde::to_vec_named(msg)?;

    let total_len = 4 + payload.len(); // 4 bytes header
    let mut buf = Vec::with_capacity(4 + total_len);

    // Length (big-endian u32)
    buf.extend_from_slice(&(total_len as u32).to_be_bytes());

    // Header
    buf.push(PROTOCOL_VERSION);
    buf.push(msg.message_type() as u8);
    buf.extend_from_slice(&[0u8; 2]); // Reserved

    // Payload
    buf.extend(payload);

    Ok(buf)
}

/// Decode a message from bytes (after reading length prefix)
pub fn decode_message(data: &[u8]) -> Result<Message, DecodeError> {
    if data.len() < 4 {
        return Err(DecodeError::TooShort);
    }

    let version = data[0];
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }

    let _msg_type = data[1]; // Could validate, but serde handles it
                             // data[2..4] reserved

    let payload = &data[4..];
    rmp_serde::from_slice(payload).map_err(DecodeError::Deserialize)
}

#[derive(Debug)]
pub enum DecodeError {
    TooShort,
    UnsupportedVersion(u8),
    Deserialize(rmp_serde::decode::Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::TooShort => write!(f, "message too short"),
            DecodeError::UnsupportedVersion(v) => write!(f, "unsupported version: {}", v),
            DecodeError::Deserialize(e) => write!(f, "deserialize error: {}", e),
        }
    }
}

impl std::error::Error for DecodeError {}

// ============================================================================
// Builder helpers
// ============================================================================

impl MetricsUpdate {
    /// Create a new MetricsUpdate with current timestamp
    pub fn new(seq: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            seq,
            signal: SignalMetrics {
                snr_cdb: 0,
                rssi_cdbm: 0,
                signal_quality_pct: 0,
                noise_floor_cdbm: None,
            },
            errors: ErrorMetrics {
                ber_exponent: 0,
                per_ppm: 0,
                fer_ppm: 0,
                crc_errors: 0,
                retransmits: 0,
            },
            bandwidth: BandwidthMetrics {
                tx_rate_bps: 0,
                rx_rate_bps: 0,
                available_bps: 0,
                utilization_pct: 0,
                queue_depth_bytes: 0,
                queue_capacity_bytes: 0,
            },
            satellite: None,
            radio: None,
            acoustic: None,
            custom: HashMap::new(),
        }
    }
}

impl Heartbeat {
    /// Create a heartbeat request
    pub fn request(seq: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            seq,
            echo: true,
        }
    }

    /// Create a heartbeat response
    pub fn response(seq: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            seq,
            echo: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_metrics_conversion() {
        let signal = SignalMetrics {
            snr_cdb: 2550,
            rssi_cdbm: -7000,
            signal_quality_pct: 85,
            noise_floor_cdbm: Some(-10000),
        };

        assert!((signal.snr_db() - 25.5).abs() < 0.01);
        assert!((signal.rssi_dbm() - (-70.0)).abs() < 0.01);
    }

    #[test]
    fn test_error_metrics_conversion() {
        let errors = ErrorMetrics {
            ber_exponent: 6,
            per_ppm: 10000, // 1%
            fer_ppm: 5000,  // 0.5%
            crc_errors: 0,
            retransmits: 0,
        };

        assert!((errors.ber() - 1e-6).abs() < 1e-10);
        assert!((errors.per_pct() - 1.0).abs() < 0.01);
        assert!((errors.fer_pct() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_message_roundtrip() {
        let msg = Message::Heartbeat(Heartbeat::request(42));
        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded[4..]).unwrap(); // Skip length prefix

        match decoded {
            Message::Heartbeat(hb) => {
                assert_eq!(hb.seq, 42);
                assert!(hb.echo);
            }
            _ => panic!("wrong message type"),
        }
    }

    // ========================================================================
    // Link Management Tests (v1.1.0)
    // ========================================================================

    #[test]
    fn test_link_available() {
        let target = TargetInfo {
            target_id: "node1".into(),
            target_type: TargetType::ZenohNode,
            name: Some("Remote Node 1".into()),
            address: Some("192.168.1.100".into()),
        };

        let link = LinkAvailable::new(
            "link-001",
            "unixpipe//run/zenoh/modem/radio0/targets/node1.sock",
            target,
        )
        .with_link_type(LinkType::Unicast)
        .with_properties(LinkProperties {
            mtu: 1400,
            bandwidth_bps: 9600,
            latency_us: 250_000,
            reliability: Reliability::Reliable,
            priority_range: Some(PriorityRange { min: 0, max: 7 }),
            express_available: true,
        });

        assert_eq!(link.link_id, "link-001");
        assert_eq!(link.link_type, LinkType::Unicast);
        assert_eq!(link.properties.mtu, 1400);
        assert!(link.properties.express_available);
        assert!((link.properties.latency_ms() - 250.0).abs() < 0.01);
    }

    #[test]
    fn test_link_unavailable() {
        let msg = LinkUnavailable::new("link-001", LinkUnavailableReason::TargetUnreachable)
            .with_message("Target went out of range")
            .may_reconnect_in(30_000);

        assert_eq!(msg.link_id, "link-001");
        assert_eq!(msg.reason, LinkUnavailableReason::TargetUnreachable);
        assert!(msg.may_reconnect);
        assert_eq!(msg.estimated_reconnect_ms, Some(30_000));
    }

    #[test]
    fn test_link_update() {
        let props = LinkProperties::new(1400, 4800, 500_000);
        let update =
            LinkUpdate::new("link-001", props).with_reason("Bandwidth reduced due to interference");

        assert_eq!(update.link_id, "link-001");
        assert_eq!(update.properties.bandwidth_bps, 4800);
        assert!(update.reason.is_some());
    }

    #[test]
    fn test_link_query() {
        let query = LinkQuery::new(42)
            .with_link_type(LinkType::Unicast)
            .with_target_filter("node*");

        assert_eq!(query.query_id, 42);
        assert_eq!(query.link_type_filter, Some(LinkType::Unicast));
        assert_eq!(query.target_filter, Some("node*".into()));
    }

    #[test]
    fn test_link_message_roundtrip() {
        let target = TargetInfo {
            target_id: "node1".into(),
            target_type: TargetType::ZenohNode,
            name: None,
            address: None,
        };

        let link = LinkAvailable::new("link-001", "unixpipe//test.sock", target);
        let msg = Message::LinkAvailable(link);

        assert!(msg.is_link_management());
        assert_eq!(msg.message_type(), MessageType::LinkAvailable);

        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded[4..]).unwrap();

        match decoded {
            Message::LinkAvailable(la) => {
                assert_eq!(la.link_id, "link-001");
                assert_eq!(la.target.target_id, "node1");
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_link_capabilities() {
        let caps = LinkCapabilities {
            socket_base_path: "/run/zenoh/modem/radio0".into(),
            unicast: UnicastCapabilities {
                max_mtu: 1500,
                min_mtu: 64,
                supports_priorities: true,
                priority_range: Some(PriorityRange { min: 0, max: 7 }),
                supports_express: true,
                express_priority_threshold: Some(2),
                supports_reliable: true,
                supports_best_effort: true,
                default_reliability: Reliability::Reliable,
                max_targets: 16,
                max_pending_bytes: 65536,
            },
            broadcast: Some(BroadcastCapabilities {
                max_mtu: 1400,
                max_recipients: 16,
                supports_reliable: false,
            }),
            multicast: None,
        };

        assert_eq!(caps.unicast.max_mtu, 1500);
        assert!(caps.unicast.supports_express);
        assert_eq!(caps.unicast.express_priority_threshold, Some(2));
        assert!(caps.broadcast.is_some());
        assert!(caps.multicast.is_none());
    }
}
