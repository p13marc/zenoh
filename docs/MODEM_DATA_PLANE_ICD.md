# Modem Data Plane Interface Control Document (ICD)

> **SUPERSEDED**: This document has been superseded by the Link Provider approach
> defined in `MODEM_LINK_PROVIDER_ICD.md` and `MODEM_DRIVER_ICD.md` v1.1.0.
>
> The Link Provider approach uses Zenoh's native Unix transport instead of a
> custom data plane protocol, providing better integration with Zenoh's QoS,
> batching, and fragmentation features.
>
> This document is retained for historical reference only.

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | **SUPERSEDED** |
| Date | 2024-12-13 |
| Related | MODEM_DRIVER_ICD.md (OAM/Control Plane) |
| Superseded By | MODEM_LINK_PROVIDER_ICD.md |

## 1. Overview

This document defines the **data plane** interface between Zenoh and modem drivers for actual data transmission. This is separate from the OAM/control plane ICD which handles metrics and management.

### 1.1 Scope

| Capability | Description |
|------------|-------------|
| Data Transmission | Send Zenoh messages through the modem |
| Express Data | Priority/express data handling with minimal latency |
| Reachability | Notify Zenoh when targets become unreachable |
| Flow Control | Backpressure and congestion signaling |
| Fragmentation | Handle MTU constraints and reassembly |

### 1.2 Design Goals

1. **Low Latency**: Express data path with minimal overhead
2. **Zero-Copy**: Avoid unnecessary memory copies when possible
3. **Backpressure**: Proper flow control to prevent buffer bloat
4. **Reliability**: Optional acknowledgment and retransmission
5. **Visibility**: Reachability and delivery status feedback

## 2. Transport Layer

### 2.1 Socket Architecture

The data plane uses a **separate socket** from the OAM socket for performance isolation:

```
                         ┌─────────────────────┐
                         │     Zenoh Router    │
                         │                     │
                         │  ┌───────────────┐  │
                         │  │  OAM Handler  │──┼──── /run/zenoh/modem_radio0_oam.sock
                         │  └───────────────┘  │            (Control Plane)
                         │                     │
                         │  ┌───────────────┐  │
                         │  │ Data Handler  │──┼──── /run/zenoh/modem_radio0_data.sock
                         │  └───────────────┘  │            (Data Plane)
                         │                     │
                         └─────────────────────┘
```

| Parameter | Value |
|-----------|-------|
| Transport | Unix Domain Socket (SOCK_STREAM) |
| Data Socket Path | `{oam_socket_base}_data.sock` |
| Connection Initiator | Zenoh (client) |
| Server | Modem Driver (listener) |

### 2.2 Why Separate Sockets?

1. **Priority Isolation**: OAM traffic doesn't block data, data doesn't delay metrics
2. **Thread Affinity**: Data path can use dedicated high-priority threads
3. **Flow Control**: Independent backpressure per channel
4. **Debugging**: Clear separation for tracing and profiling

### 2.3 Connection Lifecycle

```
Zenoh                                    Modem Driver
  │                                           │
  │────── connect(data_socket) ──────────────>│
  │                                           │
  │<───── DataPlaneReady ────────────────────│  (capabilities, MTU)
  │                                           │
  │────── DataMessage ───────────────────────>│  (Zenoh → Modem)
  │────── ExpressMessage ────────────────────>│  (high priority)
  │                                           │
  │<───── DataMessage ───────────────────────│  (Modem → Zenoh)
  │<───── DeliveryStatus ────────────────────│  (ACK/NACK)
  │<───── ReachabilityUpdate ────────────────│  (target reachable/unreachable)
  │                                           │
  │<───── FlowControl ───────────────────────│  (backpressure)
  │                                           │
```

## 3. Message Framing

### 3.1 Frame Format

Optimized for low overhead:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Length (4 bytes)                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Version    |  Message Type |     Flags     |   Reserved    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Sequence Number                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
|                      Payload (variable)                       |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

| Field | Size | Description |
|-------|------|-------------|
| Length | 4 bytes | Total frame length (big-endian) |
| Version | 1 byte | Protocol version (0x01) |
| Message Type | 1 byte | See Message Types |
| Flags | 1 byte | Message flags (see 3.3) |
| Reserved | 1 byte | Reserved (0x00) |
| Sequence Number | 4 bytes | Monotonic sequence for ordering/ACK |
| Payload | Variable | Message-specific payload |

### 3.2 Message Types

| Type ID | Name | Direction | Description |
|---------|------|-----------|-------------|
| 0x01 | DataPlaneReady | M→Z | Capabilities after connect |
| 0x02 | DataMessage | Z↔M | Regular data transmission |
| 0x03 | ExpressMessage | Z↔M | Express/priority data |
| 0x04 | DeliveryStatus | M→Z | ACK/NACK for sent messages |
| 0x05 | ReachabilityUpdate | M→Z | Target reachability change |
| 0x06 | FlowControl | M→Z | Backpressure signaling |
| 0x07 | Fragment | Z↔M | Fragmented message part |
| 0x08 | TargetQuery | Z→M | Query target reachability |
| 0x09 | TargetResponse | M→Z | Response to target query |
| 0x0A | Ping | Z↔M | Data plane keepalive |
| 0x0B | Pong | Z↔M | Ping response |
| 0xFF | Error | Z↔M | Error notification |

### 3.3 Flags Byte

```
 0   1   2   3   4   5   6   7
+---+---+---+---+---+---+---+---+
| E | R | A | F | L |  Priority |
+---+---+---+---+---+---+---+---+

E (bit 0): Express - bypass queues, minimal latency
R (bit 1): Reliable - request delivery confirmation
A (bit 2): ACK requested - explicit ACK needed
F (bit 3): Fragment - this is a fragment (see Fragment message)
L (bit 4): Last fragment - last in fragmented sequence
Priority (bits 5-7): Zenoh priority level (0-7)
```

## 4. Message Definitions

### 4.1 DataPlaneReady (0x01)

Sent by modem driver immediately after data socket connection.

```
DataPlaneReady {
    modem_id: string,           // Must match OAM socket modem_id
    protocol_version: u8,       // Data plane protocol version
    capabilities: DataPlaneCapabilities,
}

DataPlaneCapabilities {
    max_mtu: u32,               // Maximum transmission unit (bytes)
    min_mtu: u32,               // Minimum MTU (for degraded modes)
    supports_express: bool,     // Express path available
    supports_reliable: bool,    // Reliable delivery available
    supports_fragmentation: bool, // Can handle fragmentation
    supports_multicast: bool,   // Multicast delivery supported
    max_pending_messages: u32,  // Max messages before backpressure
    max_targets: u32,           // Max simultaneously tracked targets
    tx_queue_bytes: u32,        // TX buffer size in bytes
    rx_queue_bytes: u32,        // RX buffer size in bytes
}
```

### 4.2 DataMessage (0x02)

Regular data transmission in either direction.

```
DataMessage {
    message_id: u64,            // Unique message ID for tracking
    timestamp_ns: u64,          // Send timestamp (for latency measurement)
    target: Target,             // Destination identifier
    payload: bytes,             // Actual data payload
    metadata: MessageMetadata?, // Optional metadata
}

Target {
    target_type: TargetType,
    address: bytes,             // Target-specific address encoding
}

TargetType: enum {
    ZenohLocator = 0,           // Zenoh locator string
    MacAddress = 1,             // 6-byte MAC address
    IpAddress = 2,              // IPv4 (4 bytes) or IPv6 (16 bytes)
    RadioCallsign = 3,          // Radio callsign/ID
    SatelliteTerminalId = 4,    // Satellite terminal identifier
    AcousticAddress = 5,        // Underwater acoustic address
    Broadcast = 6,              // Broadcast to all
    Multicast = 7,              // Multicast group
    Custom = 255,               // Vendor-specific
}

MessageMetadata {
    zenoh_priority: u8,         // Zenoh priority (0-7)
    congestion_control: CongestionControl,
    key_expr: string?,          // Zenoh key expression (for routing)
    encoding: u32?,             // Zenoh encoding ID
    ttl_ms: u32?,               // Time to live
    source_id: bytes?,          // Source identifier
}

CongestionControl: enum {
    Drop = 0,                   // Drop if congested
    Block = 1,                  // Block sender if congested
}
```

### 4.3 ExpressMessage (0x03)

High-priority message with minimal latency path.

```
ExpressMessage {
    message_id: u64,
    timestamp_ns: u64,
    target: Target,
    payload: bytes,
    express_metadata: ExpressMetadata,
}

ExpressMetadata {
    deadline_ns: u64,           // Absolute deadline (drop if missed)
    zenoh_priority: u8,         // Must be 0-2 (high priority)
    bypass_queue: bool,         // Skip TX queue if possible
    require_ack: bool,          // Need delivery confirmation
    max_retries: u8,            // Retry count before giving up
}
```

**Express Path Guarantees:**

| Guarantee | Description |
|-----------|-------------|
| Queue Bypass | Skip regular TX queue when possible |
| Priority Preemption | Preempt lower priority transmissions |
| Deadline Enforcement | Drop if deadline cannot be met |
| Fast ACK | Immediate acknowledgment path |

### 4.4 DeliveryStatus (0x04)

Delivery confirmation from modem to Zenoh.

```
DeliveryStatus {
    message_id: u64,            // Original message ID
    status: DeliveryStatusCode,
    timestamp_ns: u64,          // Status timestamp
    details: DeliveryDetails?,
}

DeliveryStatusCode: enum {
    Delivered = 0,              // Successfully delivered to target
    Acknowledged = 1,           // Target acknowledged receipt
    Queued = 2,                 // Queued for transmission
    Transmitted = 3,            // Transmitted (no ACK expected)
    Failed = 4,                 // Delivery failed
    Expired = 5,                // TTL expired before delivery
    Rejected = 6,               // Target rejected message
    Unreachable = 7,            // Target unreachable
    Congested = 8,              // Dropped due to congestion
}

DeliveryDetails {
    latency_us: u32?,           // End-to-end latency (if measured)
    retries: u8?,               // Number of retries used
    failure_reason: string?,    // Human-readable failure reason
    hop_count: u8?,             // Number of hops (mesh networks)
}
```

### 4.5 ReachabilityUpdate (0x05)

Notify Zenoh about target reachability changes.

```
ReachabilityUpdate {
    timestamp_ns: u64,
    target: Target,
    status: ReachabilityStatus,
    details: ReachabilityDetails?,
}

ReachabilityStatus: enum {
    Reachable = 0,              // Target is reachable
    Unreachable = 1,            // Target is not reachable
    Degraded = 2,               // Reachable but with degraded quality
    Unknown = 3,                // Status unknown
}

ReachabilityDetails {
    reason: UnreachableReason?, // Why unreachable (if applicable)
    estimated_recovery_ms: u32?, // When might become reachable
    last_seen_ns: u64?,         // Last successful communication
    quality_score: u8?,         // 0-100 quality if degraded
    alternate_path: bool?,      // Alternate path available
}

UnreachableReason: enum {
    LinkDown = 0,               // Physical link is down
    NoRoute = 1,                // No route to target
    TargetOffline = 2,          // Target device offline
    Timeout = 3,                // Communication timeout
    Congestion = 4,             // Network congestion
    Handover = 5,               // During satellite handover
    Interference = 6,           // RF interference
    OutOfRange = 7,             // Target out of range
    AuthFailure = 8,            // Authentication failed
    Maintenance = 9,            // Scheduled maintenance
    Unknown = 255,              // Unknown reason
}
```

### 4.6 FlowControl (0x06)

Backpressure signaling from modem to Zenoh.

```
FlowControl {
    timestamp_ns: u64,
    control_type: FlowControlType,
    target: Target?,            // Specific target or null for global
    details: FlowControlDetails,
}

FlowControlType: enum {
    Pause = 0,                  // Stop sending
    Resume = 1,                 // Resume sending
    SlowDown = 2,               // Reduce rate
    SpeedUp = 3,                // Can increase rate
    QueueStatus = 4,            // Queue status update
}

FlowControlDetails {
    // For Pause/Resume
    duration_ms: u32?,          // Expected duration (if known)
    reason: string?,            // Human-readable reason
    
    // For SlowDown/SpeedUp
    suggested_rate_bps: u64?,   // Suggested send rate
    current_rate_bps: u64?,     // Current measured rate
    
    // For QueueStatus
    queue_depth_bytes: u32?,    // Current queue depth
    queue_capacity_bytes: u32?, // Total queue capacity
    queue_depth_messages: u32?, // Messages in queue
    high_water_mark_pct: u8?,   // High water mark percentage
    low_water_mark_pct: u8?,    // Low water mark percentage
}
```

**Flow Control Behavior:**

| State | Zenoh Behavior |
|-------|----------------|
| Pause | Buffer locally or apply backpressure to publishers |
| Resume | Flush buffered messages, resume normal flow |
| SlowDown | Reduce send rate to suggested_rate_bps |
| QueueStatus | Adjust rate based on queue fill level |

### 4.7 Fragment (0x07)

For messages larger than MTU.

```
Fragment {
    original_message_id: u64,   // ID of complete message
    fragment_index: u16,        // 0-based fragment index
    total_fragments: u16,       // Total fragment count
    fragment_offset: u32,       // Byte offset in original message
    fragment_data: bytes,       // Fragment payload
    original_length: u32,       // Total original message length
}
```

**Fragmentation Rules:**

1. Sender fragments if payload > MTU
2. Fragment size ≤ (MTU - header overhead)
3. Receiver reassembles using original_message_id
4. Out-of-order fragments supported
5. Reassembly timeout: 30 seconds default
6. Missing fragment → entire message fails

### 4.8 TargetQuery (0x08)

Query target reachability (Zenoh → Modem).

```
TargetQuery {
    query_id: u32,              // For correlating response
    targets: [Target],          // Targets to query
    timeout_ms: u32,            // Query timeout
    detailed: bool,             // Request detailed info
}
```

### 4.9 TargetResponse (0x09)

Response to TargetQuery (Modem → Zenoh).

```
TargetResponse {
    query_id: u32,              // Matches TargetQuery.query_id
    results: [TargetStatus],
}

TargetStatus {
    target: Target,
    status: ReachabilityStatus,
    details: ReachabilityDetails?,
    rtt_us: u32?,               // Round-trip time if probed
}
```

### 4.10 Ping/Pong (0x0A/0x0B)

Data plane keepalive (separate from OAM heartbeat).

```
Ping {
    timestamp_ns: u64,
    seq: u32,
}

Pong {
    timestamp_ns: u64,
    seq: u32,                   // Echo back Ping.seq
    processing_ns: u64,         // Time spent processing (for latency calc)
}
```

| Parameter | Value |
|-----------|-------|
| Ping interval | 1 second |
| Ping timeout | 3 seconds |
| Action on timeout | Report data plane failure |

### 4.11 Error (0xFF)

Data plane error notification.

```
Error {
    error_code: u32,
    severity: ErrorSeverity,
    message: string,
    related_message_id: u64?,   // Related message if applicable
    recoverable: bool,
}

ErrorSeverity: enum {
    Warning = 0,
    Error = 1,
    Fatal = 2,
}
```

## 5. Express Data Path

### 5.1 Express Mode Requirements

| Requirement | Description |
|-------------|-------------|
| Latency | < 1ms added latency vs direct modem access |
| Jitter | < 100µs jitter for express messages |
| Priority | Always processed before regular messages |
| Preemption | Can preempt in-progress regular transmissions |

### 5.2 Express Message Flow

```
Zenoh                                    Modem Driver
  │                                           │
  │                                    ┌──────┴──────┐
  │                                    │ Express     │
  │────── ExpressMessage ─────────────>│ Handler     │
  │                                    │ (dedicated  │
  │                                    │  thread)    │
  │                                    └──────┬──────┘
  │                                           │
  │                                    ┌──────┴──────┐
  │                                    │ Immediate   │
  │                                    │ TX (bypass  │
  │                                    │ regular     │
  │<───── DeliveryStatus(Transmitted) │ queue)      │
  │                                    └──────┬──────┘
  │                                           │
  │<───── DeliveryStatus(Acknowledged)────────│ (if require_ack)
  │                                           │
```

### 5.3 Express Priority Mapping

| Zenoh Priority | Express Eligible | Modem Treatment |
|----------------|------------------|-----------------|
| 0 (Control) | Yes | Immediate, preemptive |
| 1 (RealTime) | Yes | Immediate, preemptive |
| 2 (InteractiveHigh) | Yes | Fast queue, preemptive |
| 3 (InteractiveLow) | No | Regular queue |
| 4 (DataHigh) | No | Regular queue |
| 5 (Data) | No | Regular queue |
| 6 (DataLow) | No | Regular queue |
| 7 (Background) | No | Low priority queue |

### 5.4 Deadline Enforcement

Express messages include a deadline. The modem driver MUST:

1. Check deadline before queueing
2. Drop if deadline already passed
3. Prioritize based on deadline urgency
4. Report Expired status if deadline missed

## 6. Reachability Tracking

### 6.1 Reachability Events

The modem driver MUST emit ReachabilityUpdate when:

| Event | Status | Required |
|-------|--------|----------|
| Target responds after being unreachable | Reachable | Yes |
| Target stops responding | Unreachable | Yes |
| Link quality degrades significantly | Degraded | Yes |
| Handover starts (satellite) | Unreachable/Degraded | Yes |
| Handover completes | Reachable | Yes |
| Physical link goes down | Unreachable | Yes |
| Physical link comes up | Reachable | Yes |

### 6.2 Proactive Reachability Detection

```
┌─────────────────────────────────────────────────────────────┐
│                    Modem Driver                             │
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │ ARP/NDP     │    │ Link Layer  │    │ Application │     │
│  │ Monitoring  │───>│ Keepalive   │───>│ Heartbeat   │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                  │                  │             │
│         └──────────────────┴──────────────────┘             │
│                           │                                 │
│                    ┌──────▼──────┐                          │
│                    │ Reachability│                          │
│                    │   State     │                          │
│                    └──────┬──────┘                          │
│                           │                                 │
└───────────────────────────┼─────────────────────────────────┘
                            │
                     ReachabilityUpdate
                            │
                            ▼
                    ┌───────────────┐
                    │    Zenoh      │
                    └───────────────┘
```

### 6.3 Reachability Cache

The modem driver SHOULD maintain a reachability cache:

```
ReachabilityCache {
    entries: map<Target, CacheEntry>,
    max_entries: u32,
    default_ttl_ms: u32,
}

CacheEntry {
    status: ReachabilityStatus,
    last_update_ns: u64,
    last_probe_ns: u64,
    probe_count: u32,
    failure_count: u32,
    success_count: u32,
    average_rtt_us: u32,
    details: ReachabilityDetails,
}
```

## 7. Flow Control

### 7.1 Backpressure Strategy

```
                    Queue Fill Level
                           │
    100% ─────────────────►├─── PAUSE (stop sending)
                           │
     80% ─────────────────►├─── HIGH WATER (slow down)
                           │
                           │    Normal operation
                           │
     20% ─────────────────►├─── LOW WATER (speed up)
                           │
      0% ─────────────────►└─── RESUME (full speed)
```

### 7.2 Per-Target Flow Control

Flow control can be global or per-target:

```
// Global flow control (all targets)
FlowControl {
    target: null,
    control_type: Pause,
    ...
}

// Per-target flow control
FlowControl {
    target: Target { address: "192.168.1.100" },
    control_type: SlowDown,
    details: { suggested_rate_bps: 100000 },
    ...
}
```

### 7.3 Congestion Control Integration

| Zenoh CongestionControl | Modem Behavior on Congestion |
|-------------------------|------------------------------|
| Drop | Drop message, report Congested status |
| Block | Buffer message, apply backpressure |

## 8. Error Handling

### 8.1 Standard Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0x0100 | INVALID_TARGET | Target address invalid |
| 0x0101 | TARGET_UNREACHABLE | Cannot reach target |
| 0x0102 | MTU_EXCEEDED | Payload exceeds MTU (fragmentation disabled) |
| 0x0103 | QUEUE_FULL | TX queue full, message dropped |
| 0x0104 | DEADLINE_MISSED | Express deadline expired |
| 0x0105 | FRAGMENTATION_FAILED | Fragment reassembly failed |
| 0x0106 | DELIVERY_TIMEOUT | Delivery timeout |
| 0x0107 | ACK_TIMEOUT | Acknowledgment timeout |
| 0x0108 | LINK_DOWN | Physical link is down |
| 0x0109 | AUTH_FAILED | Target authentication failed |
| 0x010A | ENCRYPTION_ERROR | Encryption/decryption error |
| 0x010B | PROTOCOL_ERROR | Protocol violation |
| 0x010C | RESOURCE_EXHAUSTED | Out of resources |
| 0x0200+ | Vendor-specific | |

### 8.2 Error Recovery

| Error | Zenoh Recovery Action |
|-------|----------------------|
| TARGET_UNREACHABLE | Try alternate link, queue for retry |
| QUEUE_FULL | Apply backpressure to publishers |
| DEADLINE_MISSED | Report to application, no retry |
| LINK_DOWN | Failover to alternate modem if available |
| ACK_TIMEOUT | Retry with exponential backoff |

## 9. Serialization

### 9.1 Payload Encoding

Data message payloads are opaque bytes (Zenoh-encoded). The modem driver does not parse payloads.

### 9.2 Control Message Encoding

Control messages (headers, metadata) use **MessagePack** for consistency with OAM ICD.

### 9.3 Byte Order

All multi-byte integers are **big-endian** (network byte order).

## 10. Timing Requirements

| Operation | Requirement |
|-----------|-------------|
| DataPlaneReady after connect | ≤ 50ms |
| Express message processing | ≤ 1ms added latency |
| Regular message queueing | ≤ 5ms |
| DeliveryStatus for transmitted | ≤ 10ms after TX |
| ReachabilityUpdate on change | ≤ 50ms |
| FlowControl on threshold | ≤ 10ms |
| Ping/Pong RTT | ≤ 10ms (local processing) |

## 11. Implementation Requirements

### 11.1 Modem Driver Requirements

1. **MUST** listen on data socket path
2. **MUST** send DataPlaneReady immediately after connect
3. **MUST** support DataMessage transmission
4. **MUST** emit ReachabilityUpdate on target status change
5. **MUST** emit FlowControl on queue thresholds
6. **MUST** handle Ping and respond with Pong
7. **SHOULD** support ExpressMessage if capable
8. **SHOULD** support reliable delivery if capable
9. **SHOULD** implement fragmentation for large messages
10. **MAY** implement per-target flow control

### 11.2 Zenoh Requirements

1. **MUST** connect to data socket after OAM socket
2. **MUST** handle DataPlaneReady to learn capabilities
3. **MUST** respect FlowControl messages
4. **MUST** handle ReachabilityUpdate for routing decisions
5. **SHOULD** use ExpressMessage for priority 0-2 traffic
6. **SHOULD** track DeliveryStatus for reliable sessions
7. **SHOULD** implement message fragmentation
8. **MAY** query target reachability proactively

### 11.3 Thread Model

```
┌─────────────────────────────────────────────────────────────┐
│                    Modem Driver                             │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Express   │  │   Regular   │  │  Receive    │         │
│  │   TX Thread │  │   TX Thread │  │  Thread     │         │
│  │  (RT prio)  │  │  (normal)   │  │             │         │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
│         │                │                │                 │
│         └────────────────┴────────────────┘                 │
│                          │                                  │
│                   ┌──────▼──────┐                           │
│                   │   Hardware  │                           │
│                   │   Interface │                           │
│                   └─────────────┘                           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 12. Example Message Sequences

### 12.1 Normal Data Transmission

```
Zenoh                                    Modem Driver
  │                                           │
  │────── DataMessage(id=1, target=A) ───────>│
  │                                           │
  │<───── DeliveryStatus(id=1, Queued) ──────│
  │                                           │
  │       ... modem transmits ...             │
  │                                           │
  │<───── DeliveryStatus(id=1, Transmitted) ─│
  │                                           │
```

### 12.2 Express Message with ACK

```
Zenoh                                    Modem Driver
  │                                           │
  │────── ExpressMessage(id=2, require_ack) ─>│
  │                                           │
  │<───── DeliveryStatus(id=2, Transmitted) ─│  (immediate)
  │                                           │
  │       ... target acknowledges ...         │
  │                                           │
  │<───── DeliveryStatus(id=2, Acknowledged) │
  │                                           │
```

### 12.3 Target Becomes Unreachable

```
Zenoh                                    Modem Driver
  │                                           │
  │────── DataMessage(id=3, target=B) ───────>│
  │                                           │
  │<───── DeliveryStatus(id=3, Queued) ──────│
  │                                           │
  │       ... no response from B ...          │
  │       ... retry timeout ...               │
  │                                           │
  │<───── ReachabilityUpdate(B, Unreachable) │
  │<───── DeliveryStatus(id=3, Unreachable) ─│
  │                                           │
  │       ... later, B comes back ...         │
  │                                           │
  │<───── ReachabilityUpdate(B, Reachable) ──│
  │                                           │
```

### 12.4 Congestion and Backpressure

```
Zenoh                                    Modem Driver
  │                                           │
  │────── DataMessage(id=10) ────────────────>│
  │────── DataMessage(id=11) ────────────────>│
  │────── DataMessage(id=12) ────────────────>│
  │                                           │
  │       ... queue reaches 80% ...           │
  │                                           │
  │<───── FlowControl(SlowDown, 500kbps) ────│
  │                                           │
  │       (Zenoh reduces send rate)           │
  │                                           │
  │       ... queue drains to 20% ...         │
  │                                           │
  │<───── FlowControl(SpeedUp, 2Mbps) ───────│
  │                                           │
  │       ... queue fills to 100% ...         │
  │                                           │
  │<───── FlowControl(Pause) ────────────────│
  │                                           │
  │       (Zenoh buffers locally)             │
  │                                           │
  │       ... queue drains ...                │
  │                                           │
  │<───── FlowControl(Resume) ───────────────│
  │                                           │
```

### 12.5 Fragmented Message

```
Zenoh                                    Modem Driver
  │                                           │
  │   (message size 15000 bytes, MTU 1500)    │
  │                                           │
  │────── Fragment(id=20, 0/10, 0-1499) ─────>│
  │────── Fragment(id=20, 1/10, 1500-2999) ──>│
  │────── Fragment(id=20, 2/10, 3000-4499) ──>│
  │       ...                                 │
  │────── Fragment(id=20, 9/10, 13500-14999) >│
  │                                           │
  │<───── DeliveryStatus(id=20, Transmitted) │
  │                                           │
```

## 13. Relationship with OAM ICD

| Aspect | OAM ICD | Data Plane ICD |
|--------|---------|----------------|
| Socket | `*_oam.sock` | `*_data.sock` |
| Purpose | Metrics, state, alerts | Data transmission |
| Priority | Normal | Can be real-time |
| Messages | ModemInfo, Metrics, etc. | DataMessage, Express, etc. |
| Initiated by | Zenoh | Zenoh |
| Required | Yes | Yes (for data-capable modems) |

**Coordination:**

1. OAM socket MUST be connected before data socket
2. ModemInfo.modem_id MUST match DataPlaneReady.modem_id
3. OAM StateChange to Disconnected implies data plane failure
4. OAM bandwidth metrics inform data plane rate limiting

## Appendix A: C Structure Definitions

See `zenoh_modem_data_plane.h` for C API definitions.

## Appendix B: Rust Type Definitions

See `modem_data_plane_types.rs` for Rust type definitions.

## Appendix C: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-12-13 | Initial draft |
