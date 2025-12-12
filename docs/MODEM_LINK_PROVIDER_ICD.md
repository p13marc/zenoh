# Modem Link Provider Interface Control Document (ICD)

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Draft |
| Date | 2024-12-13 |
| Supersedes | MODEM_DATA_PLANE_ICD.md |
| Related | MODEM_DRIVER_ICD.md (OAM) |

## 1. Overview

This document defines how modem drivers integrate with Zenoh as **link providers**. Instead of a custom data plane protocol, the modem driver creates standard Zenoh Unix socket links for each reachable target, allowing Zenoh's native transport layer to handle data transmission with full QoS support.

### 1.1 Key Concept

```
┌─────────────────────────────────────────────────────────────────┐
│                         Zenoh Router                            │
│                                                                 │
│  Link Manager sees:                                             │
│    unix//run/zenoh/modem/radio0/target_alpha.sock               │
│    unix//run/zenoh/modem/radio0/target_bravo.sock               │
│    unix//run/zenoh/modem/sat0/terminal_42.sock                  │
│                                                                 │
│  These are standard Zenoh links - priority, reliability,        │
│  batching, fragmentation all handled by Zenoh transport layer   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ (standard Zenoh wire protocol)
                              │
┌─────────────────────────────┴───────────────────────────────────┐
│                        Modem Driver                             │
│                                                                 │
│  Creates/destroys Unix sockets dynamically based on             │
│  target reachability. Bridges Zenoh frames to RF/Sat/Acoustic.  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 Benefits

| Benefit | Description |
|---------|-------------|
| Native QoS | Zenoh priority (0-7) works automatically |
| Native Reliability | Reliable/Best-effort per link |
| Native Batching | Zenoh handles message batching to MTU |
| Native Fragmentation | Large messages fragmented by Zenoh |
| Routing Integration | Links visible to Zenoh routing layer |
| OAM Integration | Link quality feeds into link selection |
| Simpler Driver | Focus on bridging, not protocol |

### 1.3 Scope

This ICD covers:
- Dynamic link creation/destruction based on target reachability
- Link capability advertisement (priorities, reliability, MTU)
- Broadcast/multicast link handling
- Integration with OAM for link quality metrics

## 2. Architecture

### 2.1 Component Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Zenoh Router                            │
│                                                                 │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │   Session   │     │   Routing   │     │  Transport  │       │
│  │   Manager   │────►│    Tables   │◄────│   Manager   │       │
│  └─────────────┘     └─────────────┘     └──────┬──────┘       │
│                                                  │              │
│                              ┌───────────────────┴────────┐     │
│                              │      Link Managers         │     │
│                              │  ┌─────┐ ┌─────┐ ┌───────┐ │     │
│                              │  │ TCP │ │ UDP │ │ Unix  │ │     │
│                              │  └─────┘ └─────┘ └───┬───┘ │     │
│                              └──────────────────────┼─────┘     │
└─────────────────────────────────────────────────────┼───────────┘
                                                      │
                    ┌─────────────────────────────────┴──────────┐
                    │                                            │
           ┌────────▼────────┐                    ┌──────────────▼──────────────┐
           │   OAM Socket    │                    │      Target Link Sockets    │
           │ (control plane) │                    │                             │
           └────────┬────────┘                    │  /run/zenoh/modem/radio0/   │
                    │                             │    ├── target_alpha.sock    │
                    │                             │    ├── target_bravo.sock    │
                    │                             │    └── broadcast.sock       │
                    │                             │                             │
                    │                             │  /run/zenoh/modem/sat0/     │
                    │                             │    ├── terminal_42.sock     │
                    │                             │    └── terminal_57.sock     │
                    │                             │                             │
                    └─────────────┬───────────────┴─────────────────────────────┘
                                  │
                    ┌─────────────▼─────────────┐
                    │       Modem Driver        │
                    │                           │
                    │  ┌─────────────────────┐  │
                    │  │   Target Manager    │  │
                    │  │   (creates sockets) │  │
                    │  └──────────┬──────────┘  │
                    │             │             │
                    │  ┌──────────▼──────────┐  │
                    │  │   Physical Bridge   │  │
                    │  │  Unix ◄──► RF/Sat   │  │
                    │  └─────────────────────┘  │
                    │                           │
                    └───────────────────────────┘
```

### 2.2 Socket Layout

```
/run/zenoh/modem/
├── {modem_id}/
│   ├── oam.sock                    # OAM control socket (existing ICD)
│   ├── targets/
│   │   ├── {target_id}.sock        # Unicast link to specific target
│   │   ├── {target_id}.sock
│   │   └── ...
│   ├── broadcast.sock              # Broadcast link (optional)
│   └── multicast/
│       ├── {group_id}.sock         # Multicast group (optional)
│       └── ...
```

### 2.3 Connection Flow

```
                    Modem Driver                           Zenoh
                         │                                   │
    1. Start             │                                   │
    ─────────────────────┤                                   │
                         │                                   │
    2. Connect OAM       │◄──────── OAM Connect ────────────│
                         │                                   │
    3. Send ModemInfo    │─────── ModemInfo ────────────────►│
       (with link caps)  │                                   │
                         │                                   │
    4. Discover target   │                                   │
    ─────────────────────┤                                   │
                         │                                   │
    5. Create socket     │  mkdir targets/                   │
       for target        │  listen(target_alpha.sock)        │
                         │                                   │
    6. Notify Zenoh      │─────── LinkAvailable ────────────►│
                         │        (via OAM socket)           │
                         │                                   │
    7. Zenoh connects    │◄──────── Connect ─────────────────│
       to target socket  │         (standard Zenoh)          │
                         │                                   │
    8. Data flows        │◄─────── Zenoh Frames ────────────►│
       (Zenoh protocol)  │                                   │
                         │                                   │
    ... later ...        │                                   │
                         │                                   │
    9. Target lost       │                                   │
    ─────────────────────┤                                   │
                         │                                   │
   10. Notify Zenoh      │─────── LinkUnavailable ──────────►│
                         │        (via OAM socket)           │
                         │                                   │
   11. Close socket      │  close(target_alpha.sock)         │
                         │  rm target_alpha.sock             │
                         │                                   │
```

## 3. OAM Protocol Extensions

The OAM socket (defined in MODEM_DRIVER_ICD.md) is extended with link management messages.

### 3.1 New Message Types

| Type ID | Name | Direction | Description |
|---------|------|-----------|-------------|
| 0x20 | LinkCapabilities | M→Z | Link capabilities (in ModemInfo) |
| 0x21 | LinkAvailable | M→Z | New link available |
| 0x22 | LinkUnavailable | M→Z | Link no longer available |
| 0x23 | LinkUpdate | M→Z | Link properties changed |
| 0x24 | LinkQuery | Z→M | Query available links |
| 0x25 | LinkQueryResponse | M→Z | Response to query |

### 3.2 LinkCapabilities (in ModemInfo)

Extended ModemInfo to include link capabilities:

```
ModemInfo {
    modem_id: string,
    modem_type: ModemType,
    model: string,
    firmware_version: string,
    serial_number: string?,
    capabilities: Capabilities,
    initial_state: ModemState,
    initial_metrics: Metrics,
    
    // NEW: Link provider capabilities
    link_capabilities: LinkCapabilities,
}

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
    supports_priorities: bool,      // Can handle Zenoh priorities
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
    reliability: Reliability,       // Always best-effort typically
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

### 3.3 LinkAvailable (0x21)

Sent when a new target becomes reachable and a link socket is ready.

```
LinkAvailable {
    timestamp_ns: u64,
    link_id: string,                // Unique link identifier
    link_type: LinkType,
    locator: string,                // Zenoh locator for the link
    target: TargetInfo,             // Target identification
    properties: LinkProperties,      // Current link properties
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
    zenoh_id: ZenohId?,             // Remote Zenoh ID if known
}

TargetType: enum {
    ZenohId = 0,                    // Zenoh node ID
    MacAddress = 1,                 // Layer 2 MAC
    IpAddress = 2,                  // IPv4/IPv6
    RadioCallsign = 3,              // Radio callsign
    SatelliteTerminal = 4,          // Satellite terminal ID
    AcousticAddress = 5,            // Underwater acoustic
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
    
    // Metadata for Zenoh locator
    metadata: map<string, string>?, // Additional locator metadata
}
```

**Example LinkAvailable:**
```json
{
    "timestamp_ns": 1702483200000000000,
    "link_id": "radio0_alpha",
    "link_type": "unicast",
    "locator": "unixpipe//run/zenoh/modem/radio0/targets/alpha.sock?rel=reliable&prio=0-7",
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

### 3.4 LinkUnavailable (0x22)

Sent when a link is no longer available.

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

### 3.5 LinkUpdate (0x23)

Sent when link properties change significantly.

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

### 3.6 LinkQuery (0x24)

Zenoh can query available links.

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

### 3.7 LinkQueryResponse (0x25)

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

## 4. Broadcast Link

### 4.1 Concept

For modems that support broadcast (e.g., radio broadcast, satellite broadcast beam), a special broadcast link can be created.

```
/run/zenoh/modem/radio0/broadcast.sock
```

### 4.2 Behavior

| Aspect | Behavior |
|--------|----------|
| Direction | TX only (send to all) or RX only (receive from all) |
| Reliability | Best-effort only |
| Priority | Typically limited range |
| Locator | `unixpipe//run/zenoh/modem/radio0/broadcast.sock?rel=best_effort` |

### 4.3 Zenoh Integration

Zenoh can use broadcast links for:
- Scouting (discovery)
- Pub/sub to wildcard subscribers
- Announcements

**Note**: Zenoh's current multicast is UDP-only. For non-IP modems, broadcast is implemented as a special unicast link that the modem driver replicates to all targets internally.

### 4.4 Broadcast LinkAvailable

```json
{
    "link_id": "radio0_broadcast",
    "link_type": "broadcast",
    "locator": "unixpipe//run/zenoh/modem/radio0/broadcast.sock?rel=best_effort&prio=4-7",
    "target": {
        "target_id": "broadcast",
        "target_type": "broadcast",
        "display_name": "All Stations"
    },
    "properties": {
        "mtu": 512,
        "bandwidth_bps": 9600,
        "reliability": "best_effort",
        "priority_range": {"start": 4, "end": 7}
    }
}
```

## 5. Express / High-Priority Support

### 5.1 Concept

"Express" in Zenoh context means messages that should bypass normal queuing for minimal latency. The modem driver can advertise express support for high-priority messages.

### 5.2 Implementation

The modem driver advertises express capability via:

1. **LinkCapabilities.supports_express**: Global express support
2. **LinkCapabilities.express_priority_threshold**: Priorities 0..N use express path
3. **LinkProperties.supports_express**: Per-link express support

### 5.3 Locator Metadata

Express-capable links include metadata:

```
unixpipe//run/zenoh/modem/radio0/targets/alpha.sock?rel=reliable&prio=0-7&express=0-2
```

The `express=0-2` metadata indicates priorities 0, 1, 2 get express treatment.

### 5.4 Driver Behavior

When `supports_express` is true, the modem driver SHOULD:

1. **Prioritize high-priority frames**: Process priority 0-2 before others
2. **Bypass TX queues**: Send immediately if possible
3. **Preempt transmissions**: Interrupt lower-priority TX if supported by hardware
4. **Minimize buffering**: Keep express path latency minimal

### 5.5 Express Negotiation

During Zenoh transport establishment, QoS is negotiated. The modem driver's link properties inform this negotiation via the locator metadata.

## 6. Link Socket Protocol

### 6.1 Socket Type

Links use **Unix stream sockets** (`SOCK_STREAM`) for reliable, ordered delivery.

### 6.2 Wire Protocol

The socket carries **standard Zenoh transport frames**. The modem driver:

1. Receives Zenoh frames from Unix socket
2. Encapsulates for physical layer (RF, satellite, acoustic)
3. Transmits over physical medium
4. Receives from physical medium
5. Decapsulates and writes to Unix socket

### 6.3 Frame Format

Standard Zenoh transport frame (handled by Zenoh, not driver):

```
┌─────────────────────────────────────────┐
│            Zenoh Batch Header           │
├─────────────────────────────────────────┤
│                                         │
│            Zenoh Messages               │
│         (with priority, QoS)            │
│                                         │
└─────────────────────────────────────────┘
```

The modem driver treats this as opaque bytes.

### 6.4 Connection Handling

| Event | Driver Action |
|-------|---------------|
| Zenoh connects | Accept connection, start bridging |
| Zenoh disconnects | Stop bridging, keep socket listening |
| Target lost | Close socket, send LinkUnavailable |
| Driver shutdown | Close all sockets, send LinkUnavailable for each |

## 7. Modem Driver Implementation

### 7.1 Required Components

```
┌─────────────────────────────────────────────────────────────────┐
│                        Modem Driver                             │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    OAM Handler                           │   │
│  │  - Metrics reporting                                     │   │
│  │  - State management                                      │   │
│  │  - Link availability notifications                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ LinkAvailable/Unavailable        │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Target Manager                         │   │
│  │  - Discover/track reachable targets                      │   │
│  │  - Create/destroy Unix sockets                           │   │
│  │  - Map target_id ↔ socket                                │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ socket ↔ target mapping          │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Bridge Manager                         │   │
│  │  - Accept Zenoh connections on sockets                   │   │
│  │  - Bridge Unix socket ↔ physical layer                   │   │
│  │  - Handle priority/express if supported                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                 Physical Layer Interface                 │   │
│  │  - RF modem, satellite terminal, acoustic modem          │   │
│  │  - Framing, modulation, encryption                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 7.2 Target Discovery

The modem driver discovers targets through physical layer mechanisms:

| Modem Type | Discovery Method |
|------------|------------------|
| Radio | Beacon reception, ALE scan, routing table |
| Satellite | Terminal registration, beam scan |
| Acoustic | Acoustic ping, neighbor table |
| Mesh | Routing protocol (OLSR, AODV, etc.) |

### 7.3 Socket Lifecycle

```
Target Discovered
       │
       ▼
┌──────────────────┐
│  Create socket   │  mkdir -p /run/zenoh/modem/{id}/targets/
│  and listen      │  socket(AF_UNIX, SOCK_STREAM)
│                  │  bind({target}.sock)
│                  │  listen()
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Send LinkAvailable│  via OAM socket
│    to Zenoh      │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Accept Zenoh    │  accept() → connection
│   connection     │  start bridge threads
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Bridge active   │  Unix socket ↔ Physical layer
│                  │  (ongoing)
└────────┬─────────┘
         │
    Target Lost
         │
         ▼
┌──────────────────┐
│  Close socket    │  close(connection)
│                  │  close(listener)
│                  │  unlink({target}.sock)
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│Send LinkUnavailable│  via OAM socket
│    to Zenoh      │
└──────────────────┘
```

### 7.4 Bridge Implementation

```rust
// Pseudocode for bridge loop
async fn bridge_loop(
    unix_socket: UnixStream,
    physical_tx: PhysicalTx,
    physical_rx: PhysicalRx,
    target_id: TargetId,
    supports_express: bool,
) {
    loop {
        tokio::select! {
            // Unix → Physical (TX path)
            frame = unix_socket.read_frame() => {
                let frame = frame?;
                
                // Optional: inspect priority for express handling
                if supports_express {
                    let priority = extract_priority(&frame);
                    if priority <= EXPRESS_THRESHOLD {
                        physical_tx.send_express(target_id, frame).await?;
                        continue;
                    }
                }
                
                physical_tx.send(target_id, frame).await?;
            }
            
            // Physical → Unix (RX path)
            frame = physical_rx.recv_from(target_id) => {
                let frame = frame?;
                unix_socket.write_frame(frame).await?;
            }
        }
    }
}
```

## 8. Zenoh Integration

### 8.1 Locator Registration

When Zenoh receives `LinkAvailable`, it can:

1. Parse the locator: `unixpipe//run/zenoh/modem/radio0/targets/alpha.sock?rel=reliable&prio=0-7`
2. Extract metadata (reliability, priorities)
3. Register with Transport Manager
4. Connect when routing requires this link

### 8.2 Link Selection

The OAM metrics (from MODEM_DRIVER_ICD.md) feed into Zenoh's link selection:

```
LinkProperties (from LinkAvailable)
        +
MetricsUpdate (from OAM)
        +
Zenoh Routing Policy
        ↓
    Link Score → Select best link
```

### 8.3 Priority Mapping

| Zenoh Priority | Name | Typical Use | Express Eligible |
|----------------|------|-------------|------------------|
| 0 | Control | Session control | Yes |
| 1 | RealTime | Real-time data | Yes |
| 2 | InteractiveHigh | Interactive apps | Yes |
| 3 | InteractiveLow | Interactive apps | No |
| 4 | DataHigh | Bulk data (high) | No |
| 5 | Data | Bulk data | No |
| 6 | DataLow | Bulk data (low) | No |
| 7 | Background | Background sync | No |

### 8.4 Reliability Mapping

| Zenoh Reliability | Link Requirement |
|-------------------|------------------|
| Reliable | Link must have `reliability: reliable` |
| BestEffort | Any link acceptable |

## 9. Example Scenarios

### 9.1 Tactical Radio Network

```
Modem Driver: radio0 (tactical radio)

Initial state:
  - OAM connected
  - No targets known

Time T+0: Beacon received from station ALPHA
  1. Driver creates /run/zenoh/modem/radio0/targets/alpha.sock
  2. Driver sends LinkAvailable:
     {
       "link_id": "radio0_alpha",
       "locator": "unixpipe//run/zenoh/modem/radio0/targets/alpha.sock?rel=reliable&prio=0-7&express=0-2",
       "target": {"target_id": "alpha", "display_name": "Station Alpha"},
       "properties": {"mtu": 1500, "bandwidth_bps": 1000000, "latency_us": 50000}
     }
  3. Zenoh registers link, can now route to ALPHA

Time T+5: Station ALPHA goes out of range
  1. Driver detects timeout
  2. Driver closes alpha.sock
  3. Driver sends LinkUnavailable:
     {
       "link_id": "radio0_alpha",
       "reason": "out_of_range",
       "details": {"last_seen_ns": 1702483205000000000}
     }
  4. Zenoh removes link from routing
```

### 9.2 LEO Satellite with Handover

```
Modem Driver: sat0 (LEO satellite terminal)

Steady state:
  - Connected to satellite STARLINK-1234
  - Link active: sat0_terminal42 → remote terminal 42

Time T+0: Handover alert (satellite about to go below horizon)
  1. OAM sends Alert(HandoverImminent)
  2. Driver keeps link active but sends LinkUpdate:
     {
       "link_id": "sat0_terminal42",
       "updated_properties": {"latency_us": 80000, "jitter_us": 20000}
     }

Time T+5: Handover starts
  1. Driver sends LinkUnavailable:
     {
       "link_id": "sat0_terminal42",
       "reason": "handover",
       "details": {"estimated_recovery_ms": 500}
     }
  2. Zenoh buffers or reroutes traffic

Time T+5.5: Handover complete (now on STARLINK-5678)
  1. Driver sends LinkAvailable (same target, new path)
  2. Zenoh reconnects to link
```

### 9.3 Broadcast on Radio Network

```
Modem Driver: radio0

Startup:
  1. Driver creates /run/zenoh/modem/radio0/broadcast.sock
  2. Driver sends LinkAvailable:
     {
       "link_id": "radio0_broadcast",
       "link_type": "broadcast",
       "locator": "unixpipe//run/zenoh/modem/radio0/broadcast.sock?rel=best_effort&prio=5-7",
       "properties": {"mtu": 512, "reliability": "best_effort"}
     }

Usage:
  - Zenoh uses broadcast link for scouting
  - Zenoh may use for pub/sub fanout
  - Modem driver internally replicates to all known targets
```

## 10. Implementation Requirements

### 10.1 Modem Driver Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| MD-01 | MUST implement OAM socket per MODEM_DRIVER_ICD.md | Required |
| MD-02 | MUST send LinkCapabilities in ModemInfo | Required |
| MD-03 | MUST create Unix socket per reachable target | Required |
| MD-04 | MUST send LinkAvailable when socket ready | Required |
| MD-05 | MUST send LinkUnavailable when target lost | Required |
| MD-06 | MUST bridge Zenoh frames opaquely (no parsing) | Required |
| MD-07 | SHOULD send LinkUpdate on significant property changes | Recommended |
| MD-08 | SHOULD implement express path for high priorities | Recommended |
| MD-09 | MAY implement broadcast link | Optional |
| MD-10 | MAY implement multicast links | Optional |

### 10.2 Zenoh Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| ZN-01 | MUST connect to OAM socket to receive link notifications | Required |
| ZN-02 | MUST parse LinkAvailable and register locator | Required |
| ZN-03 | MUST handle LinkUnavailable and unregister locator | Required |
| ZN-04 | SHOULD use LinkUpdate to update link properties | Recommended |
| ZN-05 | SHOULD integrate OAM metrics with link selection | Recommended |
| ZN-06 | MAY query links via LinkQuery | Optional |

## 11. Security Considerations

### 11.1 Socket Permissions

```bash
# Socket directory
mkdir -p /run/zenoh/modem
chown zenoh:modem /run/zenoh/modem
chmod 750 /run/zenoh/modem

# Per-modem directory (created by driver)
# Owner: modem driver user
# Group: zenoh group
chmod 750 /run/zenoh/modem/{modem_id}
chmod 660 /run/zenoh/modem/{modem_id}/*.sock
```

### 11.2 Authentication

The OAM socket authenticates the modem driver. Link sockets inherit this trust:
- Only Zenoh (with OAM connection) knows about link sockets
- Socket paths are communicated via authenticated OAM channel

## 12. Appendices

### Appendix A: Locator Metadata Keys

| Key | Values | Description |
|-----|--------|-------------|
| `rel` | `reliable`, `best_effort` | Link reliability |
| `prio` | `N-M` (e.g., `0-7`) | Supported priority range |
| `express` | `N-M` (e.g., `0-2`) | Express-eligible priorities |
| `mtu` | integer | Maximum transmission unit |
| `bw` | integer | Available bandwidth (bps) |

### Appendix B: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-12-13 | Initial draft |
