# Modem Driver Interface Control Document (ICD)

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.1.0 |
| Status | Draft |
| Date | 2024-12-13 |
| Related | MODEM_LINK_PROVIDER_ICD.md |

## 1. Overview

This document defines the interface between Zenoh OAM (Operations, Administration, and Maintenance) and modem drivers for link quality monitoring, control, and **dynamic link management**. The modem driver acts as a **link provider**, creating Unix sockets dynamically for each reachable target. The interface enables bidirectional communication for:

- **Modem → Zenoh**: Metrics, state changes, alerts, bandwidth updates, **link availability**
- **Zenoh → Modem**: Traffic hints, mode requests, configuration

## 2. Transport Layer

### 2.1 Connection

| Parameter | Value |
|-----------|-------|
| Transport | Unix Domain Socket (SOCK_STREAM) |
| Socket Path | Configurable, e.g., `/run/zenoh/modem_{id}.sock` |
| Connection Initiator | Zenoh (client) |
| Server | Modem Driver (listener) |

### 2.2 Connection Lifecycle

```
Zenoh                                    Modem Driver
  │                                           │
  │────── connect() ─────────────────────────>│
  │                                           │
  │<───── ModemInfo ─────────────────────────│  (immediate after connect)
  │                                           │
  │<───── MetricsUpdate ─────────────────────│  (periodic)
  │<───── StateChange ───────────────────────│  (on event)
  │<───── Alert ─────────────────────────────│  (on event)
  │                                           │
  │────── Request ───────────────────────────>│  (on demand)
  │<───── Response ──────────────────────────│
  │                                           │
  │────── close() / Heartbeat timeout ───────│
  │                                           │
```

### 2.3 Reconnection

| Parameter | Value |
|-----------|-------|
| Reconnect on disconnect | Yes (Zenoh responsibility) |
| Reconnect delay | Exponential backoff: 100ms, 200ms, 400ms, ... max 30s |
| Max reconnect attempts | Unlimited |

## 3. Message Framing

### 3.1 Frame Format

All messages are length-prefixed for reliable framing:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Length (4 bytes)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Version     |  Message Type |          Reserved             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
|                    Payload (variable length)                  |
|                         (MessagePack)                         |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

| Field | Size | Description |
|-------|------|-------------|
| Length | 4 bytes | Total frame length including header (big-endian) |
| Version | 1 byte | Protocol version (current: 0x01) |
| Message Type | 1 byte | See Message Types table |
| Reserved | 2 bytes | Reserved for future use (set to 0x0000) |
| Payload | Variable | MessagePack-encoded message body |

### 3.2 Message Types

| Type ID | Name | Direction | Description |
|---------|------|-----------|-------------|
| 0x01 | ModemInfo | M→Z | Modem identification (sent on connect) |
| 0x02 | MetricsUpdate | M→Z | Periodic metrics report |
| 0x03 | StateChange | M→Z | Modem state transition notification |
| 0x04 | Alert | M→Z | Predictive/reactive alert |
| 0x05 | BandwidthUpdate | M→Z | Available bandwidth change |
| 0x06 | Heartbeat | M↔Z | Keepalive (bidirectional) |
| 0x10 | Request | Z→M | Request from Zenoh to modem |
| 0x11 | Response | M→Z | Response to a request |
| 0x12 | Configure | Z→M | Configuration update |
| 0x13 | ConfigureAck | M→Z | Configuration acknowledgment |
| 0x20 | LinkAvailable | M→Z | New link available (target reachable) |
| 0x21 | LinkUnavailable | M→Z | Link no longer available |
| 0x22 | LinkUpdate | M→Z | Link properties changed |
| 0x23 | LinkQuery | Z→M | Query available links |
| 0x24 | LinkQueryResponse | M→Z | Response to link query |
| 0xFF | Error | M↔Z | Error notification |

Direction: M=Modem Driver, Z=Zenoh

## 4. Message Definitions

### 4.1 ModemInfo (0x01)

Sent by modem driver immediately after connection is established.

```
ModemInfo {
    modem_id: string,           // Unique identifier (e.g., "radio0", "sat_geo_1")
    modem_type: string,         // Type identifier (see section 4.1.1)
    model: string,              // Hardware model (e.g., "Harris PRC-152A")
    firmware_version: string,   // Firmware version
    serial_number: string,      // Hardware serial number (optional)
    capabilities: Capabilities, // Supported features
    initial_state: ModemState,  // Current state
    initial_metrics: Metrics,   // Current metrics snapshot
    link_capabilities: LinkCapabilities, // Link provider capabilities (see 4.1.2)
}

Capabilities {
    supports_reliable_mode: bool,     // Can switch to reliable/robust mode
    supports_throughput_mode: bool,   // Can switch to high-throughput mode
    supports_power_control: bool,     // TX power adjustable
    supports_frequency_hop: bool,     // Frequency hopping available
    supports_encryption: bool,        // Link-layer encryption
    max_bandwidth_bps: u64,           // Maximum theoretical bandwidth
    min_bandwidth_bps: u64,           // Minimum operational bandwidth
    metrics_interval_ms: u32,         // Recommended metrics push interval
    custom_capabilities: map<string, string>,  // Vendor-specific
}
```

#### 4.1.1 Modem Types

| Type ID | Description |
|---------|-------------|
| `radio_hf` | HF radio (3-30 MHz) |
| `radio_vhf` | VHF radio (30-300 MHz) |
| `radio_uhf` | UHF radio (300 MHz - 3 GHz) |
| `radio_tactical` | Tactical radio (multi-band) |
| `radio_mesh` | Mesh radio network |
| `satellite_geo` | Geostationary satellite terminal |
| `satellite_leo` | Low Earth Orbit satellite terminal |
| `satellite_meo` | Medium Earth Orbit satellite terminal |
| `acoustic_underwater` | Underwater acoustic modem |
| `optical_fso` | Free-space optical link |
| `custom` | Custom/vendor-specific |

#### 4.1.2 Link Capabilities

The modem driver acts as a **link provider** for Zenoh. It creates Unix sockets dynamically for each reachable target, allowing Zenoh to use native transport mechanisms.

```
LinkCapabilities {
    // Socket paths
    socket_base_path: string,       // e.g., "/run/zenoh/modem/radio0"
    
    // Unicast capabilities
    unicast: UnicastCapabilities,
    
    // Broadcast capability (optional)
    broadcast: BroadcastCapabilities?,
    
    // Multicast capability (optional)
    multicast: MulticastCapabilities?,
}

UnicastCapabilities {
    // MTU range
    max_mtu: u32,                   // Maximum MTU (bytes)
    min_mtu: u32,                   // Minimum MTU (degraded mode)
    
    // Priority support
    supports_priorities: bool,      // Can handle Zenoh priorities (0-7)
    priority_range: PriorityRange?, // Supported priority range (if limited)
    
    // Express/high-priority support
    supports_express: bool,         // Has fast path for high priority
    express_priority_threshold: u8?, // Priorities 0..N use express path
    
    // Reliability
    supports_reliable: bool,        // Reliable delivery available
    supports_best_effort: bool,     // Best-effort delivery available
    default_reliability: Reliability,
    
    // Limits
    max_targets: u32,               // Max simultaneous targets
    max_pending_bytes: u32,         // Max TX buffer per link
}

PriorityRange {
    start: u8,                      // Lowest supported priority (0-7)
    end: u8,                        // Highest supported priority (0-7)
}

Reliability: enum {
    Reliable = 0,
    BestEffort = 1,
}

BroadcastCapabilities {
    available: bool,                // Broadcast link can be created
    mtu: u32,                       // Broadcast MTU
    reliability: Reliability,       // Typically best-effort
    priority_range: PriorityRange?, // Supported priorities for broadcast
}

MulticastCapabilities {
    available: bool,                // Multicast supported
    max_groups: u32,                // Max simultaneous groups
    mtu: u32,                       // Multicast MTU
    reliability: Reliability,
    priority_range: PriorityRange?,
}
```

**Express Support**: When `supports_express` is true, the modem driver provides a fast path for high-priority messages (typically priorities 0-2). This bypasses normal queuing and may preempt lower-priority transmissions.

**Broadcast Support**: For modems that can transmit to all reachable targets simultaneously (e.g., radio broadcast). The driver creates a special broadcast socket that internally replicates to all targets.

### 4.2 MetricsUpdate (0x02)

Periodic metrics report. Sent at configurable interval (default: 500ms).

```
MetricsUpdate {
    timestamp_ns: u64,          // Unix epoch nanoseconds (modem clock)
    seq: u32,                   // Sequence number (monotonic)
    
    // Signal quality
    signal: SignalMetrics,
    
    // Error rates
    errors: ErrorMetrics,
    
    // Bandwidth/throughput
    bandwidth: BandwidthMetrics,
    
    // Link-specific (optional)
    satellite: SatelliteMetrics?,    // Only for satellite modems
    radio: RadioMetrics?,            // Only for radio modems
    acoustic: AcousticMetrics?,      // Only for acoustic modems
    
    // Vendor extensions
    custom: map<string, Value>?,     // Vendor-specific metrics
}

SignalMetrics {
    snr_cdb: i32,               // SNR in centidecibels (dB * 100)
    rssi_cdbm: i32,             // RSSI in centi-dBm (dBm * 100)
    signal_quality_pct: u8,     // 0-100 quality percentage
    noise_floor_cdbm: i32?,     // Noise floor in centi-dBm
}

ErrorMetrics {
    ber_exponent: i8,           // BER as 10^(-exponent), e.g., 6 = 10^-6
    per_ppm: u32,               // Packet Error Rate in parts per million
    fer_ppm: u32,               // Frame Error Rate in parts per million
    crc_errors: u32,            // CRC error count since last report
    retransmits: u32,           // Retransmit count since last report
}

BandwidthMetrics {
    tx_rate_bps: u64,           // Current TX rate
    rx_rate_bps: u64,           // Current RX rate
    available_bps: u64,         // Estimated available bandwidth
    utilization_pct: u8,        // Link utilization 0-100
    queue_depth_bytes: u32,     // TX queue depth in bytes
    queue_capacity_bytes: u32,  // TX queue capacity
}

SatelliteMetrics {
    elevation_cdeg: i16,        // Elevation in centidegrees (deg * 100)
    azimuth_cdeg: u16,          // Azimuth in centidegrees
    doppler_hz: i32,            // Doppler shift in Hz
    range_km: u32?,             // Range to satellite in km (if known)
    satellite_id: string?,      // Satellite identifier
    beam_id: string?,           // Beam identifier
    handover_pending: bool,     // Handover imminent
    time_to_handover_ms: u32?,  // Estimated ms until handover
}

RadioMetrics {
    tx_power_cdbm: i16,         // TX power in centi-dBm
    frequency_hz: u64,          // Current frequency
    bandwidth_hz: u32,          // Channel bandwidth
    modulation: string,         // Current modulation (e.g., "QPSK", "16QAM")
    fec_rate: string,           // FEC rate (e.g., "1/2", "3/4")
    hopping: bool,              // Frequency hopping active
    crypto_active: bool,        // Encryption active
}

AcousticMetrics {
    multipath_spread_us: u32,   // Multipath spread in microseconds
    doppler_spread_chz: i16,    // Doppler spread in centi-Hz
    propagation_delay_ms: u32,  // One-way propagation delay
    ambient_noise_cdb: i16,     // Ambient noise level in centi-dB
    water_temp_cdc: i16?,       // Water temperature in centi-Celsius
}
```

### 4.3 StateChange (0x03)

Sent when modem state changes.

```
StateChange {
    timestamp_ns: u64,          // When state changed
    previous_state: ModemState,
    current_state: ModemState,
    reason: string?,            // Human-readable reason
    error_code: u32?,           // Error code if entering error state
}

ModemState: enum {
    Unknown = 0,
    Initializing = 1,
    Searching = 2,              // Looking for signal/satellite
    Synchronizing = 3,          // Acquiring sync
    Connected = 4,              // Normal operation
    Degraded = 5,               // Connected but poor quality
    HandoverInProgress = 6,     // Actively handing over (LEO sat)
    Standby = 7,                // Low power standby
    Disconnected = 8,           // No connectivity
    Error = 9,                  // Error state
    Maintenance = 10,           // Maintenance mode
}
```

### 4.4 Alert (0x04)

Predictive or reactive alerts for proactive handling.

```
Alert {
    timestamp_ns: u64,
    alert_id: u32,              // Unique alert ID (for correlation)
    severity: AlertSeverity,
    alert_type: AlertType,
    message: string,            // Human-readable description
    data: AlertData,            // Type-specific data
    ttl_ms: u32?,               // Alert validity period (optional)
}

AlertSeverity: enum {
    Info = 0,                   // Informational
    Warning = 1,                // Degradation expected
    Critical = 2,               // Imminent failure
}

AlertType: enum {
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

AlertData: union {
    SignalDegrading {
        current_snr_cdb: i32,
        trend_cdb_per_sec: i32,     // Negative = degrading
        estimated_failure_ms: u32?, // Time to failure if trend continues
    },
    
    HandoverImminent {
        current_satellite_id: string?,
        next_satellite_id: string?,
        estimated_duration_ms: u32,
        expected_outage_ms: u32,    // Expected connectivity gap
    },
    
    LinkFailureImminent {
        reason: string,
        estimated_time_ms: u32,
    },
    
    BandwidthReduction {
        current_bps: u64,
        expected_bps: u64,
        reason: string,
        duration_ms: u32?,          // Expected duration (if known)
    },
    
    InterferenceDetected {
        frequency_hz: u64?,
        estimated_source: string?,
        mitigation_active: bool,
    },
    
    Custom {
        vendor_type: string,
        data: map<string, Value>,
    },
}
```

### 4.5 BandwidthUpdate (0x05)

Sent when available bandwidth changes significantly (>10% change or threshold crossing).

```
BandwidthUpdate {
    timestamp_ns: u64,
    available_tx_bps: u64,      // Available TX bandwidth
    available_rx_bps: u64,      // Available RX bandwidth
    max_tx_bps: u64,            // Maximum possible TX
    max_rx_bps: u64,            // Maximum possible RX
    reason: BandwidthChangeReason,
    duration_estimate_ms: u32?, // How long this state expected to last
}

BandwidthChangeReason: enum {
    ModulationChange = 1,       // Adaptive modulation adjusted
    FecChange = 2,              // FEC rate changed
    PowerChange = 3,            // TX power adjusted
    Congestion = 4,             // Network congestion
    Interference = 5,           // Interference mitigation
    ScheduledEvent = 6,         // Planned maintenance/event
    WeatherImpact = 7,          // Weather affecting link (rain fade)
    HandoverTransition = 8,     // During satellite handover
    UserRequested = 9,          // User/Zenoh requested mode change
    Recovery = 10,              // Recovering from degradation
}
```

### 4.6 Heartbeat (0x06)

Bidirectional keepalive. Either side can send; both should respond.

```
Heartbeat {
    timestamp_ns: u64,
    seq: u32,
    echo: bool,                 // true = request echo, false = echo response
}
```

| Parameter | Value |
|-----------|-------|
| Heartbeat interval | 5 seconds (configurable) |
| Heartbeat timeout | 15 seconds (3 missed heartbeats) |
| Action on timeout | Close connection, reconnect |

### 4.7 Request (0x10)

Request from Zenoh to modem driver.

```
Request {
    request_id: u32,            // For correlating response
    request_type: RequestType,
    data: RequestData,
}

RequestType: enum {
    GetMetrics = 1,             // Request immediate metrics
    GetState = 2,               // Request current state
    GetInfo = 3,                // Request modem info
    SetMode = 4,                // Set operating mode
    TrafficHint = 5,            // Hint about expected traffic
    DiagnosticTest = 6,         // Run diagnostic test
    Custom = 255,               // Vendor-specific
}

RequestData: union {
    GetMetrics { },             // No parameters
    
    GetState { },               // No parameters
    
    GetInfo { },                // No parameters
    
    SetMode {
        mode: OperatingMode,
        duration_ms: u32?,      // How long to maintain mode (0 = permanent)
    },
    
    TrafficHint {
        expected_tx_bps: u64,   // Expected TX rate
        expected_rx_bps: u64,   // Expected RX rate
        priority: u8,           // Zenoh priority (0-7)
        duration_ms: u32,       // How long this traffic pattern expected
        reliable: bool,         // Reliable delivery needed
        latency_budget_ms: u32?, // Maximum acceptable latency
    },
    
    DiagnosticTest {
        test_type: string,      // Test identifier
        parameters: map<string, Value>,
    },
    
    Custom {
        vendor_type: string,
        data: map<string, Value>,
    },
}

OperatingMode: enum {
    Normal = 0,                 // Balanced operation
    Reliable = 1,               // Maximize reliability (lower rate, more FEC)
    Throughput = 2,             // Maximize throughput (less FEC)
    LowPower = 3,               // Minimize power consumption
    LowLatency = 4,             // Minimize latency
    Stealth = 5,                // Minimize emissions (if supported)
}
```

### 4.8 Response (0x11)

Response to a Request.

```
Response {
    request_id: u32,            // Matches Request.request_id
    status: ResponseStatus,
    data: ResponseData?,
    error_message: string?,     // If status != Success
}

ResponseStatus: enum {
    Success = 0,
    InvalidRequest = 1,
    NotSupported = 2,
    Busy = 3,                   // Modem busy, try later
    Failed = 4,                 // Operation failed
    Timeout = 5,                // Operation timed out
    PermissionDenied = 6,       // Not authorized
}

ResponseData: union {
    Metrics(MetricsUpdate),     // Response to GetMetrics
    State(StateChange),         // Response to GetState
    Info(ModemInfo),            // Response to GetInfo
    ModeSet { 
        applied_mode: OperatingMode,
        effective_until_ns: u64?,
    },
    TrafficHintAck {
        accepted: bool,
        adjusted_tx_bps: u64?,  // Modem may adjust hint
        adjusted_rx_bps: u64?,
    },
    DiagnosticResult {
        test_type: string,
        passed: bool,
        results: map<string, Value>,
    },
    Custom {
        vendor_type: string,
        data: map<string, Value>,
    },
}
```

### 4.9 Configure (0x12)

Configuration update from Zenoh to modem.

```
Configure {
    config_id: u32,             // For correlating ack
    config_type: ConfigType,
    data: ConfigData,
}

ConfigType: enum {
    MetricsInterval = 1,        // Change metrics push interval
    AlertThresholds = 2,        // Set alert thresholds
    PowerLimits = 3,            // Set TX power limits
    BandwidthLimits = 4,        // Set bandwidth limits
    Custom = 255,
}

ConfigData: union {
    MetricsInterval {
        interval_ms: u32,       // New metrics interval
    },
    
    AlertThresholds {
        snr_warning_cdb: i32?,  // SNR warning threshold
        snr_critical_cdb: i32?, // SNR critical threshold
        loss_warning_pct: u8?,  // Loss % warning threshold
        loss_critical_pct: u8?, // Loss % critical threshold
    },
    
    PowerLimits {
        max_tx_power_cdbm: i16?,
        min_tx_power_cdbm: i16?,
    },
    
    BandwidthLimits {
        max_tx_bps: u64?,
        max_rx_bps: u64?,
    },
    
    Custom {
        vendor_type: string,
        data: map<string, Value>,
    },
}
```

### 4.10 ConfigureAck (0x13)

Acknowledgment of Configure message.

```
ConfigureAck {
    config_id: u32,             // Matches Configure.config_id
    status: ResponseStatus,
    applied_config: ConfigData?, // Actual applied config (may differ)
    error_message: string?,
}
```

### 4.11 Error (0xFF)

Error notification (bidirectional).

```
Error {
    error_code: u32,
    severity: ErrorSeverity,
    message: string,
    recoverable: bool,
    context: map<string, Value>?,
}

ErrorSeverity: enum {
    Warning = 0,                // Non-fatal
    Error = 1,                  // Operation failed
    Fatal = 2,                  // Connection should be closed
}
```

### 4.12 LinkAvailable (0x20)

Sent when a new target becomes reachable and a link socket is ready for Zenoh to connect.

```
LinkAvailable {
    timestamp_ns: u64,
    link_id: string,                // Unique link identifier
    link_type: LinkType,
    locator: string,                // Zenoh locator (e.g., "unixpipe//run/zenoh/modem/radio0/targets/alpha.sock")
    target: TargetInfo,             // Target identification
    properties: LinkProperties,     // Current link properties
}

LinkType: enum {
    Unicast = 0,
    Broadcast = 1,
    Multicast = 2,
}

TargetInfo {
    target_id: string,              // Unique target identifier
    target_type: TargetType,        // Type of target addressing
    address: bytes,                 // Target-specific address
    display_name: string?,          // Human-readable name
    zenoh_id: bytes?,               // Remote Zenoh ID if known (16 bytes)
}

TargetType: enum {
    ZenohId = 0,                    // Zenoh node ID
    MacAddress = 1,                 // Layer 2 MAC
    IpAddress = 2,                  // IPv4/IPv6
    RadioCallsign = 3,              // Radio callsign
    SatelliteTerminal = 4,          // Satellite terminal ID
    AcousticAddress = 5,            // Underwater acoustic
    Broadcast = 6,                  // Broadcast address
    Multicast = 7,                  // Multicast group
    Custom = 255,                   // Vendor-specific
}

LinkProperties {
    // Current performance
    mtu: u32,                       // Current MTU
    bandwidth_bps: u64,             // Available bandwidth
    latency_us: u32,                // One-way latency estimate
    jitter_us: u32?,                // Latency jitter
    loss_ppm: u32?,                 // Packet loss (parts per million)
    
    // QoS capabilities for this specific link
    reliability: Reliability,
    priority_range: PriorityRange?,
    supports_express: bool,
    
    // Locator metadata (included in locator string)
    metadata: map<string, string>?, // Additional locator metadata
}
```

**Example:**
```json
{
    "timestamp_ns": 1702483200000000000,
    "link_id": "radio0_alpha",
    "link_type": "unicast",
    "locator": "unixpipe//run/zenoh/modem/radio0/targets/alpha.sock?rel=reliable&prio=0-7&express=0-2",
    "target": {
        "target_id": "alpha",
        "target_type": "radio_callsign",
        "address": [0x41, 0x4C, 0x50, 0x48, 0x41],
        "display_name": "Station Alpha"
    },
    "properties": {
        "mtu": 1500,
        "bandwidth_bps": 1000000,
        "latency_us": 50000,
        "reliability": "reliable",
        "priority_range": {"start": 0, "end": 7},
        "supports_express": true
    }
}
```

### 4.13 LinkUnavailable (0x21)

Sent when a link is no longer available (target unreachable).

```
LinkUnavailable {
    timestamp_ns: u64,
    link_id: string,                // Link identifier
    locator: string,                // Zenoh locator being removed
    reason: LinkUnavailableReason,
    details: LinkUnavailableDetails?,
}

LinkUnavailableReason: enum {
    TargetOffline = 0,              // Target went offline
    LinkDown = 1,                   // Physical link failure
    Timeout = 2,                    // Communication timeout
    OutOfRange = 3,                 // Target out of range
    Handover = 4,                   // Satellite handover in progress
    Interference = 5,               // RF interference
    Congestion = 6,                 // Network congestion
    AuthFailure = 7,                // Authentication failed
    Maintenance = 8,                // Scheduled maintenance
    DriverShutdown = 9,             // Modem driver shutting down
    Unknown = 255,
}

LinkUnavailableDetails {
    error_message: string?,
    estimated_recovery_ms: u32?,    // When link might be back
    last_seen_ns: u64?,             // Last successful communication
    alternate_available: bool?,     // Another path exists
}
```

### 4.14 LinkUpdate (0x22)

Sent when link properties change significantly (e.g., bandwidth change, degradation).

```
LinkUpdate {
    timestamp_ns: u64,
    link_id: string,
    updated_properties: LinkPropertiesUpdate,
}

LinkPropertiesUpdate {
    // Only fields that changed (all optional)
    mtu: u32?,
    bandwidth_bps: u64?,
    latency_us: u32?,
    jitter_us: u32?,
    loss_ppm: u32?,
    reliability: Reliability?,
    priority_range: PriorityRange?,
    supports_express: bool?,
}
```

### 4.15 LinkQuery (0x23)

Zenoh can query available links from the modem driver.

```
LinkQuery {
    query_id: u32,
    filter: LinkFilter?,
}

LinkFilter {
    link_type: LinkType?,           // Filter by type
    target_type: TargetType?,       // Filter by target type
    min_bandwidth_bps: u64?,        // Minimum bandwidth
    max_latency_us: u32?,           // Maximum latency
    require_reliable: bool?,        // Must support reliable
    require_express: bool?,         // Must support express
}
```

### 4.16 LinkQueryResponse (0x24)

Response to LinkQuery.

```
LinkQueryResponse {
    query_id: u32,
    links: [LinkInfo],
}

LinkInfo {
    link_id: string,
    link_type: LinkType,
    locator: string,
    target: TargetInfo,
    properties: LinkProperties,
    state: LinkState,
}

LinkState: enum {
    Active = 0,                     // Link is active and healthy
    Degraded = 1,                   // Link is active but degraded
    Connecting = 2,                 // Link is being established
    Suspended = 3,                  // Link temporarily suspended
}
```

## 5. Standard Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0x0000 | OK | No error |
| 0x0001 | UNKNOWN | Unknown error |
| 0x0002 | INVALID_MESSAGE | Malformed message |
| 0x0003 | UNSUPPORTED_VERSION | Protocol version not supported |
| 0x0004 | UNSUPPORTED_TYPE | Message type not supported |
| 0x0005 | INVALID_PARAMETER | Invalid parameter value |
| 0x0006 | TIMEOUT | Operation timed out |
| 0x0007 | BUSY | Resource busy |
| 0x0008 | NOT_READY | Modem not ready |
| 0x0009 | HARDWARE_FAULT | Hardware error |
| 0x000A | PERMISSION_DENIED | Operation not permitted |
| 0x1000-0xFFFF | Vendor-specific | Reserved for vendor codes |

## 6. Timing Requirements

| Operation | Requirement |
|-----------|-------------|
| ModemInfo after connect | ≤ 100ms |
| MetricsUpdate interval | Configurable, default 500ms, min 50ms, max 60000ms |
| Request → Response | ≤ 1000ms (may be longer for diagnostic tests) |
| StateChange notification | ≤ 50ms after state change |
| Alert notification | ≤ 10ms after detection |
| BandwidthUpdate notification | ≤ 100ms after significant change |
| Heartbeat interval | 5000ms default |
| Heartbeat timeout | 15000ms (3 missed) |
| LinkAvailable after target discovered | ≤ 100ms |
| LinkUnavailable after target lost | ≤ 50ms |
| LinkUpdate on significant change | ≤ 100ms |
| LinkQuery → LinkQueryResponse | ≤ 500ms |

## 7. Implementation Requirements

### 7.1 Modem Driver Requirements

1. **MUST** listen on configured Unix socket path (OAM)
2. **MUST** send ModemInfo immediately after connection
3. **MUST** send MetricsUpdate at configured interval
4. **MUST** send StateChange on any state transition
5. **MUST** respond to Heartbeat within 1 second
6. **MUST** respond to Request within specified timeout
7. **MUST** create Unix socket for each reachable target
8. **MUST** send LinkAvailable when target socket is ready
9. **MUST** send LinkUnavailable when target becomes unreachable
10. **MUST** close target socket and cleanup when target lost
11. **SHOULD** send Alert for predictable degradation
12. **SHOULD** send BandwidthUpdate for >10% changes
13. **SHOULD** send LinkUpdate on significant property changes
14. **SHOULD** implement express path for high-priority messages
15. **MAY** create broadcast socket if supported
16. **MAY** implement vendor-specific extensions

### 7.2 Zenoh Requirements

1. **MUST** initiate connection to modem OAM socket
2. **MUST** handle reconnection with exponential backoff
3. **MUST** respond to Heartbeat requests
4. **MUST** timeout requests that don't receive Response
5. **MUST** handle LinkAvailable and register locator with transport manager
6. **MUST** handle LinkUnavailable and unregister locator
7. **MUST** connect to target sockets using standard Zenoh transport protocol
8. **SHOULD** send TrafficHint before significant traffic changes
9. **SHOULD** respect modem capabilities in requests
10. **SHOULD** use LinkUpdate to update link properties for routing decisions
11. **SHOULD** integrate OAM metrics with link selection scoring
12. **MAY** send Configure to optimize modem behavior
13. **MAY** query links via LinkQuery

### 7.3 Thread Safety

- Modem driver **MUST** handle concurrent reads/writes on socket
- Messages **MAY** be sent in any order (use request_id for correlation)
- Metrics/Alerts **MAY** arrive while waiting for Response

## 8. Example Message Sequences

### 8.1 Normal Operation

```
Zenoh                                    Modem Driver
  │                                           │
  │────── connect() ─────────────────────────>│
  │<───── ModemInfo ─────────────────────────│
  │                                           │
  │<───── MetricsUpdate ─────────────────────│  (every 500ms)
  │<───── MetricsUpdate ─────────────────────│
  │                                           │
  │────── TrafficHint(10Mbps, Data) ─────────>│
  │<───── Response(TrafficHintAck) ───────────│
  │                                           │
  │<───── BandwidthUpdate(available: 8Mbps) ──│
  │                                           │
  │<───── MetricsUpdate ─────────────────────│
  │                                           │
```

### 8.2 Satellite Handover

```
Zenoh                                    Modem Driver
  │                                           │
  │<───── Alert(HandoverImminent, 30s) ──────│
  │                                           │
  │────── SetMode(Reliable) ─────────────────>│  Prepare for disruption
  │<───── Response(ModeSet) ─────────────────│
  │                                           │
  │<───── StateChange(Connected→Handover) ───│
  │<───── BandwidthUpdate(0) ────────────────│
  │                                           │
  │       ... handover in progress ...        │
  │                                           │
  │<───── StateChange(Handover→Synchronizing)│
  │<───── StateChange(Synchronizing→Connected)│
  │<───── BandwidthUpdate(restored) ─────────│
  │                                           │
  │────── SetMode(Normal) ───────────────────>│  Resume normal
  │<───── Response(ModeSet) ─────────────────│
  │                                           │
```

### 8.3 Link Degradation

```
Zenoh                                    Modem Driver
  │                                           │
  │<───── Alert(SignalDegrading, Warning) ───│
  │                                           │
  │<───── MetricsUpdate(SNR: 15dB) ──────────│
  │<───── StateChange(Connected→Degraded) ───│
  │<───── BandwidthUpdate(reduced) ──────────│
  │                                           │
  │────── SetMode(Reliable) ─────────────────>│
  │<───── Response(ModeSet) ─────────────────│
  │                                           │
  │       ... signal improves ...             │
  │                                           │
  │<───── StateChange(Degraded→Connected) ───│
  │<───── BandwidthUpdate(restored) ─────────│
  │                                           │
```

### 8.4 Dynamic Link Management

```
Zenoh                                    Modem Driver
  │                                           │
  │────── connect(OAM) ──────────────────────>│
  │<───── ModemInfo (with LinkCapabilities) ─│
  │                                           │
  │       ... target ALPHA discovered ...     │
  │                                           │
  │                               (driver creates alpha.sock)
  │<───── LinkAvailable ─────────────────────│
  │        link_id: "radio0_alpha"            │
  │        locator: "unixpipe//...alpha.sock" │
  │                                           │
  │  (Zenoh registers locator)                │
  │                                           │
  │────── connect(alpha.sock) ───────────────>│  (standard Zenoh transport)
  │<═════ Zenoh Transport Established ═══════│
  │                                           │
  │<═════ Data flows (Zenoh frames) ═════════>│
  │                                           │
  │       ... target BRAVO discovered ...     │
  │                                           │
  │<───── LinkAvailable ─────────────────────│
  │        link_id: "radio0_bravo"            │
  │                                           │
  │────── connect(bravo.sock) ───────────────>│
  │                                           │
  │       ... ALPHA goes out of range ...     │
  │                                           │
  │<───── LinkUnavailable ───────────────────│
  │        link_id: "radio0_alpha"            │
  │        reason: OutOfRange                 │
  │                                           │
  │  (Zenoh unregisters locator)              │
  │  (alpha.sock closed by driver)            │
  │                                           │
```

### 8.5 Broadcast Link

```
Zenoh                                    Modem Driver
  │                                           │
  │<───── ModemInfo ─────────────────────────│
  │        broadcast.available: true          │
  │                                           │
  │                               (driver creates broadcast.sock)
  │<───── LinkAvailable ─────────────────────│
  │        link_id: "radio0_broadcast"        │
  │        link_type: Broadcast               │
  │        locator: "unixpipe//...broadcast.sock?rel=best_effort"
  │                                           │
  │────── connect(broadcast.sock) ───────────>│
  │                                           │
  │══════ Zenoh Scouting/Pub ════════════════>│
  │                                           │
  │       (driver replicates to all targets)  │
  │                                           │
```

### 8.6 Express High-Priority Message

```
Zenoh                                    Modem Driver
  │                                           │
  │  (link supports express: priorities 0-2)  │
  │                                           │
  │══════ Priority 0 Frame ══════════════════>│
  │       (via alpha.sock)                    │
  │                                           │
  │       (driver: express path, bypass queue)│
  │       (driver: immediate TX, may preempt) │
  │                                           │
  │══════ Priority 5 Frame ══════════════════>│
  │       (via alpha.sock)                    │
  │                                           │
  │       (driver: normal queue)              │
  │                                           │
```

## 9. Serialization

### 9.1 MessagePack Encoding

All payloads use [MessagePack](https://msgpack.org/) binary serialization:

- Compact binary format
- Self-describing (no schema required at runtime)
- Wide language support
- Efficient for numeric data

### 9.2 Field Naming Convention

| Convention | Example |
|------------|---------|
| Snake_case for fields | `signal_quality_pct` |
| Abbreviations for units | `_ns` (nanoseconds), `_ms` (milliseconds) |
| Centi-prefix for fixed-point | `_cdb` (centidecibels), `_cdeg` (centidegrees) |
| Optional fields | Nullable in MessagePack (nil) |

### 9.3 Numeric Encoding

| Type | Encoding |
|------|----------|
| Timestamps | u64, Unix epoch nanoseconds |
| Fixed-point dB | i32, value * 100 (centidecibels) |
| Fixed-point degrees | i16/u16, value * 100 (centidegrees) |
| Percentages | u8, 0-100 |
| Rates | u64, bits per second |
| Frequencies | u64, Hz |

## 10. Security Considerations

### 10.1 Socket Permissions

```bash
# Recommended socket directory permissions
mkdir -p /run/zenoh
chown zenoh:modem /run/zenoh
chmod 750 /run/zenoh

# Socket file permissions (created by modem driver)
# Owner: modem driver process
# Group: zenoh group (for Zenoh access)
chmod 660 /run/zenoh/modem_*.sock
```

### 10.2 Authentication

The current version relies on Unix socket permissions for access control. Future versions may add:

- Challenge-response authentication
- Capability tokens
- Audit logging

### 10.3 Data Validation

- **Modem Driver**: Validate all incoming requests
- **Zenoh**: Validate all incoming metrics/alerts
- **Both**: Reject oversized messages (max 64KB payload)

## 11. Versioning

### 11.1 Version Negotiation

1. Zenoh connects with highest supported version in first message
2. Modem responds with ModemInfo using mutually supported version
3. If incompatible, Error(UNSUPPORTED_VERSION) and disconnect

### 11.2 Compatibility Rules

| Change Type | Version Bump | Compatibility |
|-------------|--------------|---------------|
| Add optional field | Patch (0.0.x) | Backward compatible |
| Add new message type | Minor (0.x.0) | Backward compatible |
| Change field semantics | Major (x.0.0) | Breaking |
| Remove field/message | Major (x.0.0) | Breaking |

## 12. Reference Implementation

### 12.1 Rust Types (Modem Driver)

```rust
// See separate file: modem_driver_types.rs
```

### 12.2 Rust Types (Zenoh)

```rust
// See separate file: zenoh_modem_client.rs
```

## Appendix A: Vendor Extensions

Vendors may extend the protocol using:

1. `Custom` variants in unions (AlertType, RequestType, etc.)
2. `custom_capabilities` map in ModemInfo
3. `custom` map in MetricsUpdate
4. Vendor-specific error codes (0x1000-0xFFFF)

Extension naming convention: `vendor_name.extension_name`

Example:
```
MetricsUpdate {
    ...
    custom: {
        "harris.crypto_key_id": 42,
        "harris.comsec_status": "active",
    }
}
```

## Appendix B: Unit Conventions

| Quantity | Field Suffix | Scale | Example |
|----------|--------------|-------|---------|
| Time (ns) | `_ns` | 1 | 1000000000 = 1 second |
| Time (ms) | `_ms` | 1 | 1000 = 1 second |
| Time (us) | `_us` | 1 | 1000000 = 1 second |
| Power (dBm) | `_cdbm` | 0.01 | -7000 = -70 dBm |
| Ratio (dB) | `_cdb` | 0.01 | 2500 = 25 dB |
| Angle (deg) | `_cdeg` | 0.01 | 4500 = 45.00° |
| Temperature (°C) | `_cdc` | 0.01 | 2350 = 23.50°C |
| Frequency (Hz) | `_hz` | 1 | 2400000000 = 2.4 GHz |
| Frequency (cHz) | `_chz` | 0.01 | 100 = 1 Hz |
| Rate (bps) | `_bps` | 1 | 1000000 = 1 Mbps |
| Percentage | `_pct` | 1 | 50 = 50% |
| PPM | `_ppm` | 0.0001% | 10000 = 1% |

## Appendix C: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.1.0 | 2024-12-13 | Added Link Provider support: LinkCapabilities in ModemInfo, LinkAvailable (0x20), LinkUnavailable (0x21), LinkUpdate (0x22), LinkQuery (0x23), LinkQueryResponse (0x24). Modem driver now acts as dynamic link provider creating Unix sockets per target. Added broadcast and express support. |
| 1.0.0 | 2024-12-12 | Initial draft |
