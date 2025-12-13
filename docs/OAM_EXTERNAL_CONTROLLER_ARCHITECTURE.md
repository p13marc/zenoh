# OAM External Controller Architecture

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Draft |
| Date | 2024-12-13 |
| Related | OAM_LINK_QUALITY_PLAN.md, MODEM_DRIVER_ICD.md, MODEM_LINK_PROVIDER_ICD.md |

---

## 1. Executive Summary

This document describes an alternative architecture for OAM-based link quality management in Zenoh. Instead of Zenoh performing link selection based on collected metrics, an **external controller service** reads all metrics (both Zenoh OAM and modem metrics) and commands Zenoh to use specific links.

### 1.1 Key Principles

1. **Zenoh provides metrics, not decisions** - OAM probes measure link quality, metrics are exposed via API
2. **External controller owns decision logic** - reads metrics from multiple sources, applies custom logic
3. **Modem metrics bypass Zenoh** - controller reads directly from modem drivers
4. **No change to default behavior** - without a controller, Zenoh behaves as today
5. **Controller can force link selection** - when present, overrides default behavior

### 1.2 Comparison with Original Design

| Aspect | Original Design | This Architecture |
|--------|-----------------|-------------------|
| Metrics collection | Zenoh collects OAM + modem | Zenoh collects OAM only |
| Decision logic | Inside Zenoh (scoring weights) | External controller |
| Modem integration | `transport_oam_modem` feature | Not in Zenoh |
| Complexity in Zenoh | Higher | Lower |
| Flexibility | Limited to configured weights | Unlimited (external logic) |
| Default behavior | Scoring-based selection | Same as current Zenoh |

---

## 2. Architecture Overview

### 2.1 Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              ZENOH ROUTER                                   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         OAM Module                                   │   │
│  │                                                                      │   │
│  │   ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐    │   │
│  │   │   Prober    │───►│   Metrics   │───►│   Metrics API       │    │   │
│  │   │ (LBM/DMM/   │    │   Storage   │    │   (exposed to       │    │   │
│  │   │  SLM)       │    │ (per-link)  │    │    external world)  │────┼───┼──► To Controller
│  │   └─────────────┘    └─────────────┘    └─────────────────────┘    │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      Link Management                                 │   │
│  │                                                                      │   │
│  │   ┌─────────────────────────────────────────────────────────────┐   │   │
│  │   │                    Link Selection                            │   │   │
│  │   │                                                              │   │   │
│  │   │   Default: Current Zenoh behavior (round-robin, first-fit)  │   │   │
│  │   │   Override: Use forced link if set by controller            │   │   │
│  │   │                                                              │   │   │
│  │   └─────────────────────────────────────────────────────────────┘   │   │
│  │                              ▲                                       │   │
│  │                              │                                       │   │
│  │   ┌──────────────────────────┴──────────────────────────────────┐   │   │
│  │   │                  Link Control API                            │   │   │
│  │   │                                                              │   │   │
│  │   │   set_forced_link(peer, link)                               │◄──┼───┼── From Controller
│  │   │   clear_forced_link(peer)                                   │   │   │
│  │   │   disable_link(link) / enable_link(link)                    │   │   │
│  │   │                                                              │   │   │
│  │   └──────────────────────────────────────────────────────────────┘   │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                         │                              ▲
                         │ (Zenoh transport)            │ (Control commands)
                         ▼                              │
┌─────────────────────────────────────────────────────────────────────────────┐
│                          EXTERNAL CONTROLLER                                │
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐ │
│  │  Zenoh Metrics  │  │  Modem Metrics  │  │  Other Data Sources         │ │
│  │  Reader         │  │  Reader         │  │  (GPS, mission, policy)     │ │
│  └────────┬────────┘  └────────┬────────┘  └─────────────┬───────────────┘ │
│           │                    │                         │                  │
│           └────────────────────┼─────────────────────────┘                  │
│                                ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                       Decision Engine                                │   │
│  │                                                                      │   │
│  │   - Custom scoring algorithms                                       │   │
│  │   - Machine learning models                                         │   │
│  │   - Policy-based rules                                              │   │
│  │   - Cross-node optimization                                         │   │
│  │   - Predictive handover                                             │   │
│  │   - Operator manual override                                        │   │
│  │                                                                      │   │
│  └──────────────────────────────┬──────────────────────────────────────┘   │
│                                 │                                           │
│                                 ▼                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                       Command Generator                              │   │
│  │                                                                      │   │
│  │   zenoh.set_forced_link(peer, link)                                 │   │
│  │   zenoh.disable_link(link)                                          │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  │ (OAM socket - direct connection)
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            MODEM DRIVER                                     │
│                                                                             │
│   Provides:                                                                 │
│   - Link sockets (as per MODEM_LINK_PROVIDER_ICD.md)                       │
│   - Modem metrics via OAM socket (SNR, BER, state, etc.)                   │
│   - Alerts (handover, degradation, etc.)                                   │
│                                                                             │
│   Reports to: External Controller (NOT to Zenoh)                           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow

```
                    Metrics Flow                      Command Flow
                    ────────────                      ────────────

    Zenoh                                         Controller
      │                                               │
      │  OAM Probes (LBM/LBR, DMM/DMR, SLM/SLR)      │
      │◄─────────────────────────────────────────────►│
      │         (between Zenoh peers)                 │
      │                                               │
      │                                               │
      │  Link Metrics (RTT, jitter, loss)            │
      │──────────────────────────────────────────────►│
      │         (API or pub/sub)                      │
      │                                               │
      │                                               │
      │                                    Modem      │
      │                                      │        │
      │                                      │        │
      │                    Modem Metrics     │        │
      │                    (SNR, BER, etc.)  ├───────►│
      │                                      │        │
      │                    Alerts            │        │
      │                    (handover, etc.)  ├───────►│
      │                                      │        │
      │                                               │
      │                                               │
      │               Decision                        │
      │               ────────                        │
      │                                               │
      │                                               │
      │  Force Link Command                          │
      │◄──────────────────────────────────────────────│
      │         (set_forced_link)                     │
      │                                               │
      ▼                                               ▼
```

---

## 3. Zenoh OAM Module

### 3.1 Scope

The OAM module in Zenoh is responsible for:

1. **Sending OAM probes** to measure link quality
2. **Receiving OAM probe replies** and calculating metrics
3. **Storing per-link metrics** (RTT, jitter, loss)
4. **Exposing metrics via API** for external consumption
5. **Accepting link control commands** from external controller

The OAM module is **NOT** responsible for:

- Reading modem metrics (external controller does this)
- Scoring links (external controller does this)
- Deciding which link to use (external controller commands this)

### 3.2 OAM Probe Types

As defined in OAM_LINK_QUALITY_PLAN.md:

| OAM ID | Name | Purpose |
|--------|------|---------|
| 0x0010 | LBM (Loopback Message) | RTT measurement request |
| 0x0011 | LBR (Loopback Reply) | RTT measurement response |
| 0x0020 | DMM (Delay Measurement Message) | One-way delay request (requires clock sync) |
| 0x0021 | DMR (Delay Measurement Reply) | One-way delay response |
| 0x0030 | SLM (Synthetic Loss Message) | Packet loss measurement request |
| 0x0031 | SLR (Synthetic Loss Reply) | Packet loss measurement response |

### 3.3 Collected Metrics

```rust
/// Per-link quality metrics collected from OAM probes
pub struct LinkQualityMetrics {
    /// Link identifier
    pub link_id: String,
    
    /// Zenoh locator for this link
    pub locator: Locator,
    
    /// Remote peer ZenohId
    pub peer_zid: ZenohId,
    
    // === RTT Metrics ===
    /// Current RTT
    pub rtt_current: Duration,
    /// Minimum observed RTT
    pub rtt_min: Duration,
    /// Maximum observed RTT
    pub rtt_max: Duration,
    /// Exponential moving average RTT
    pub rtt_avg: Duration,
    
    // === Jitter (RFC 3550) ===
    pub jitter: Duration,
    
    // === One-Way Delay (requires clock sync) ===
    /// Forward delay: local -> remote
    pub forward_delay: Option<Duration>,
    /// Reverse delay: remote -> local  
    pub reverse_delay: Option<Duration>,
    
    // === Packet Loss ===
    /// Total probes sent
    pub tx_probe_count: u64,
    /// Total probe replies received
    pub rx_probe_count: u64,
    /// Loss ratio (0.0 to 1.0)
    pub loss_ratio: f64,
    
    // === Link State ===
    /// Consecutive probe failures
    pub consecutive_failures: u32,
    /// Link is responding to probes
    pub is_alive: bool,
    
    // === Timestamps ===
    pub last_probe_sent: Instant,
    pub last_probe_received: Instant,
    pub metrics_updated: Instant,
}
```

### 3.4 Metrics Exposure API

Two methods for exposing metrics to the external controller:

#### Option A: Direct API Call

```rust
impl Session {
    /// Get OAM metrics for all active links
    /// Requires: `unstable` + `transport_oam` features
    #[zenoh_macros::unstable]
    pub fn link_quality_metrics(&self) -> Vec<LinkQualityMetrics> {
        // Returns current metrics for all links
    }
    
    /// Get OAM metrics for links to a specific peer
    #[zenoh_macros::unstable]
    pub fn link_quality_metrics_for_peer(&self, peer: ZenohId) -> Vec<LinkQualityMetrics> {
        // Returns metrics for links to specific peer
    }
}
```

#### Option B: Pub/Sub on Admin Space

Zenoh publishes metrics periodically on admin keys:

```
@/{zid}/link/{link_id}/metrics
```

Payload (JSON or MessagePack):
```json
{
    "link_id": "tcp_192.168.1.100_7447",
    "locator": "tcp/192.168.1.100:7447",
    "peer_zid": "abc123...",
    "rtt_us": 1250,
    "rtt_min_us": 980,
    "rtt_max_us": 2100,
    "rtt_avg_us": 1150,
    "jitter_us": 85,
    "loss_ratio": 0.001,
    "is_alive": true,
    "timestamp_ns": 1702483200000000000
}
```

#### Recommendation

Support **both options**:
- Direct API for low-latency access
- Pub/sub for distributed controllers and monitoring

---

## 4. Link Control API

### 4.1 Overview

The external controller uses this API to command link selection. When no commands are given, Zenoh behaves exactly as it does today.

### 4.2 API Definition

```rust
impl Session {
    /// Force all traffic to a specific peer to use a specific link.
    /// 
    /// When set, Zenoh will use ONLY this link for the specified peer,
    /// regardless of priority, reliability, or other factors.
    /// 
    /// # Arguments
    /// * `peer` - The remote peer ZenohId
    /// * `link` - The locator of the link to use
    /// 
    /// # Returns
    /// * `Ok(())` - Link override set successfully
    /// * `Err` - Link not found or not connected to peer
    /// 
    /// # Example
    /// ```
    /// session.set_forced_link(
    ///     peer_zid,
    ///     "tcp/192.168.1.100:7447".parse()?
    /// )?;
    /// ```
    #[zenoh_macros::unstable]
    pub fn set_forced_link(&self, peer: ZenohId, link: Locator) -> ZResult<()>;
    
    /// Clear the forced link for a peer, returning to default behavior.
    /// 
    /// After calling this, Zenoh will use its default link selection
    /// logic for the specified peer.
    #[zenoh_macros::unstable]
    pub fn clear_forced_link(&self, peer: ZenohId) -> ZResult<()>;
    
    /// Disable a link entirely. Traffic will not use this link.
    /// 
    /// The link remains connected but is excluded from selection.
    /// OAM probes continue to be sent for monitoring.
    #[zenoh_macros::unstable]
    pub fn disable_link(&self, link: Locator) -> ZResult<()>;
    
    /// Re-enable a previously disabled link.
    #[zenoh_macros::unstable]
    pub fn enable_link(&self, link: Locator) -> ZResult<()>;
    
    /// Check if a link is currently disabled.
    #[zenoh_macros::unstable]
    pub fn is_link_disabled(&self, link: Locator) -> bool;
    
    /// Get all current link overrides.
    #[zenoh_macros::unstable]
    pub fn get_link_overrides(&self) -> HashMap<ZenohId, Locator>;
    
    /// Get all currently disabled links.
    #[zenoh_macros::unstable]
    pub fn get_disabled_links(&self) -> Vec<Locator>;
}
```

### 4.3 Behavior

| Scenario | Behavior |
|----------|----------|
| No override set | Default Zenoh behavior (current implementation) |
| `set_forced_link(peer, link)` called | All traffic to `peer` uses `link` exclusively |
| Forced link becomes unavailable | Traffic to `peer` fails until override cleared or link recovers |
| `clear_forced_link(peer)` called | Return to default behavior for `peer` |
| `disable_link(link)` called | Link excluded from selection, OAM probes continue |
| `enable_link(link)` called | Link available for selection again |

### 4.4 Implementation in Transport Layer

```rust
// In io/zenoh-transport/src/unicast/universal/tx.rs

fn select(
    links: &[TransportLinkUnicastUniversal],
    reliability: Reliability,
    priority: Priority,
    peer_zid: ZenohId,
    #[cfg(feature = "transport_oam")]
    overrides: &LinkOverrides,
) -> Option<usize> {
    #[cfg(feature = "transport_oam")]
    {
        // Check for forced link override
        if let Some(forced_locator) = overrides.get_forced_link(&peer_zid) {
            // Find the forced link in the list
            for (idx, link) in links.iter().enumerate() {
                if link.locator() == forced_locator && !overrides.is_disabled(forced_locator) {
                    return Some(idx);
                }
            }
            // Forced link not available - return None (traffic fails)
            return None;
        }
        
        // Filter out disabled links
        let available_links: Vec<_> = links
            .iter()
            .enumerate()
            .filter(|(_, link)| !overrides.is_disabled(link.locator()))
            .collect();
        
        // Use default selection on available links
        Self::default_select(&available_links, reliability, priority)
    }
    
    #[cfg(not(feature = "transport_oam"))]
    {
        // Original behavior unchanged
        Self::default_select(links, reliability, priority)
    }
}
```

---

## 5. External Controller

### 5.1 Responsibilities

The external controller is a separate service (not part of Zenoh) that:

1. **Reads Zenoh OAM metrics** via API or pub/sub
2. **Reads modem metrics** directly from modem drivers (via OAM socket as per MODEM_DRIVER_ICD.md)
3. **Reads other relevant data** (GPS, mission parameters, policies, etc.)
4. **Applies decision logic** to determine optimal link selection
5. **Commands Zenoh** to use specific links via Link Control API

### 5.2 Interface with Zenoh

```
Controller                                    Zenoh
    │                                           │
    │  Subscribe to metrics                     │
    │───────────────────────────────────────────►
    │  @/{zid}/link/*/metrics                   │
    │                                           │
    │  Or call API                              │
    │───────────────────────────────────────────►
    │  session.link_quality_metrics()           │
    │◄──────────────────────────────────────────│
    │  Vec<LinkQualityMetrics>                  │
    │                                           │
    │                                           │
    │  ... decision logic ...                   │
    │                                           │
    │                                           │
    │  Command link selection                   │
    │───────────────────────────────────────────►
    │  session.set_forced_link(peer, link)      │
    │                                           │
```

### 5.3 Interface with Modem Driver

The controller connects directly to modem drivers using the OAM socket protocol defined in MODEM_DRIVER_ICD.md:

```
Controller                                 Modem Driver
    │                                           │
    │  Connect to OAM socket                    │
    │───────────────────────────────────────────►
    │  /run/zenoh/modem_{id}.sock               │
    │                                           │
    │◄──────────────────────────────────────────│
    │  ModemInfo                                │
    │                                           │
    │◄──────────────────────────────────────────│
    │  MetricsUpdate (periodic)                 │
    │  - SNR, RSSI, BER                         │
    │  - Bandwidth, utilization                 │
    │  - Satellite/radio specific              │
    │                                           │
    │◄──────────────────────────────────────────│
    │  StateChange                              │
    │                                           │
    │◄──────────────────────────────────────────│
    │  Alert (handover, degradation)            │
    │                                           │
    │◄──────────────────────────────────────────│
    │  LinkAvailable / LinkUnavailable          │
    │                                           │
    │                                           │
    │  TrafficHint (optional)                   │
    │───────────────────────────────────────────►
    │                                           │
    │  SetMode (optional)                       │
    │───────────────────────────────────────────►
    │                                           │
```

### 5.4 Example Controller Logic

```python
# Pseudo-code for external controller

class LinkController:
    def __init__(self):
        self.zenoh_session = zenoh.open()
        self.modem_clients = {}  # modem_id -> OAM client
        
    async def run(self):
        # Subscribe to Zenoh OAM metrics
        zenoh_metrics = {}
        self.zenoh_session.subscribe("@/*/link/*/metrics", 
            lambda m: self.on_zenoh_metrics(m))
        
        # Connect to all modem drivers
        for modem_id in discover_modems():
            client = ModemOAMClient(f"/run/zenoh/modem_{modem_id}.sock")
            self.modem_clients[modem_id] = client
            client.on_metrics = lambda m: self.on_modem_metrics(modem_id, m)
            client.on_alert = lambda a: self.on_modem_alert(modem_id, a)
        
        # Decision loop
        while True:
            await asyncio.sleep(1.0)  # Evaluate every second
            self.evaluate_and_command()
    
    def evaluate_and_command(self):
        for peer in self.get_peers():
            best_link = self.score_links(peer)
            current_link = self.get_current_forced_link(peer)
            
            if best_link != current_link:
                if best_link is not None:
                    self.zenoh_session.set_forced_link(peer, best_link)
                else:
                    self.zenoh_session.clear_forced_link(peer)
    
    def score_links(self, peer):
        """Custom scoring logic combining all metrics."""
        best_score = -1
        best_link = None
        
        for link in self.get_links_for_peer(peer):
            # Get Zenoh OAM metrics
            oam = self.zenoh_metrics.get(link.id)
            
            # Get modem metrics for this link's modem
            modem = self.modem_metrics.get(link.modem_id)
            
            # Custom scoring
            score = 100.0
            
            # Penalize high RTT
            if oam:
                score -= oam.rtt_avg_ms * 0.5
                score -= oam.jitter_ms * 2.0
                score -= oam.loss_ratio * 100
            
            # Penalize poor signal
            if modem:
                if modem.snr_db < 15:
                    score -= (15 - modem.snr_db) * 3
                if modem.state == "degraded":
                    score -= 20
                if modem.handover_pending:
                    score -= 30  # Avoid handover
            
            # Policy overrides
            if self.policy.prefer_satellite and link.is_satellite:
                score += 10
            
            if score > best_score:
                best_score = score
                best_link = link.locator
        
        return best_link
```

---

## 6. Configuration

### 6.1 Zenoh Configuration

```json5
{
  transport: {
    unicast: {
      // OAM configuration (metrics collection only)
      oam: {
        /// Enable OAM probing (default: false)
        enabled: true,
        
        /// Probe interval in milliseconds (default: 100)
        probe_interval_ms: 100,
        
        /// Probe timeout in milliseconds (default: 500)
        probe_timeout_ms: 500,
        
        /// Number of consecutive failures before marking link as down
        failure_threshold: 3,
        
        /// Probe types to use
        probe_types: ["loopback"],  // or ["loopback", "delay", "loss"]
        
        /// Publish metrics on admin space (for external controller)
        publish_metrics: true,
        
        /// Metrics publication interval in milliseconds
        publish_interval_ms: 500,
      },
    },
  },
}
```

### 6.2 Controller Configuration (Example)

```json5
{
  // Zenoh connection
  zenoh: {
    connect: ["tcp/127.0.0.1:7447"],
    // Subscribe to metrics from all routers
    metrics_key: "@/*/link/*/metrics",
  },
  
  // Modem drivers to monitor
  modems: [
    {
      id: "radio0",
      socket: "/run/zenoh/modem_radio0.sock",
      type: "radio_tactical",
    },
    {
      id: "sat0", 
      socket: "/run/zenoh/modem_sat0.sock",
      type: "satellite_leo",
    },
  ],
  
  // Decision parameters
  decision: {
    // How often to re-evaluate link selection
    interval_ms: 1000,
    
    // Hysteresis: minimum score difference to trigger switch
    hysteresis: 5.0,
    
    // Scoring weights (example)
    weights: {
      rtt: 1.0,
      jitter: 2.0,
      loss: 10.0,
      snr: 2.0,
      modem_state: 5.0,
    },
  },
}
```

---

## 7. Implementation Plan

### 7.1 Phase Overview

| Phase | Description | Duration | Zenoh Changes |
|-------|-------------|----------|---------------|
| 1 | Protocol & Configuration | 2 weeks | OAM IDs, config structs |
| 2 | OAM Probes & Metrics | 3 weeks | Prober, responder, metrics storage |
| 3 | Metrics Exposure | 1 week | API, pub/sub on admin space |
| 4 | Link Control API | 2 weeks | Force/disable link commands |
| 5 | Testing | 2 weeks | Integration tests |
| **Total** | | **10 weeks** | |

### 7.2 Phase 1: Protocol & Configuration (Weeks 1-2)

**Objective:** Define OAM message types and configuration structures.

**Files to modify:**
- `commons/zenoh-protocol/src/transport/oam.rs` - Add OAM ID constants
- `commons/zenoh-config/src/lib.rs` - Add OamConf structure

**Deliverables:**
- [ ] OAM ID constants (LBM, LBR, DMM, DMR, SLM, SLR)
- [ ] Payload structures for each probe type
- [ ] Configuration structure for OAM settings
- [ ] Unit tests for serialization

**Estimated LOC:** ~500

### 7.3 Phase 2: OAM Probes & Metrics (Weeks 3-5)

**Objective:** Implement probe sending, receiving, and metrics calculation.

**New files:**
- `io/zenoh-transport/src/unicast/oam/mod.rs`
- `io/zenoh-transport/src/unicast/oam/metrics.rs`
- `io/zenoh-transport/src/unicast/oam/prober.rs`
- `io/zenoh-transport/src/unicast/oam/responder.rs`

**Files to modify:**
- `io/zenoh-transport/src/unicast/universal/rx.rs` - Route OAM messages
- `io/zenoh-transport/src/unicast/universal/link.rs` - Add metrics field

**Deliverables:**
- [ ] OAM prober task (sends probes at configured interval)
- [ ] OAM responder (replies to incoming probes)
- [ ] Metrics calculation (RTT, jitter, loss)
- [ ] Per-link metrics storage
- [ ] Integration with RX path

**Estimated LOC:** ~1,500

### 7.4 Phase 3: Metrics Exposure (Week 6)

**Objective:** Expose metrics to external controller.

**Files to modify:**
- `zenoh/src/api/session.rs` - Add `link_quality_metrics()` API
- `zenoh/src/net/runtime/mod.rs` - Metrics publication on admin space

**Deliverables:**
- [ ] `Session::link_quality_metrics()` API
- [ ] `Session::link_quality_metrics_for_peer()` API
- [ ] Optional pub/sub on `@/{zid}/link/{link_id}/metrics`
- [ ] Configuration for metrics publication

**Estimated LOC:** ~400

### 7.5 Phase 4: Link Control API (Weeks 7-8)

**Objective:** Implement commands for external controller.

**Files to modify:**
- `zenoh/src/api/session.rs` - Add control APIs
- `io/zenoh-transport/src/unicast/universal/tx.rs` - Modify `select()`
- `io/zenoh-transport/src/unicast/manager.rs` - Store overrides

**Deliverables:**
- [ ] `Session::set_forced_link()` API
- [ ] `Session::clear_forced_link()` API
- [ ] `Session::disable_link()` / `enable_link()` APIs
- [ ] Override storage in transport layer
- [ ] Modified link selection respecting overrides

**Estimated LOC:** ~600

### 7.6 Phase 5: Testing (Weeks 9-10)

**Objective:** Comprehensive testing of OAM and control APIs.

**New files:**
- `io/zenoh-transport/tests/unicast_oam.rs`
- `io/zenoh-transport/tests/unicast_link_control.rs`

**Test cases:**
- [ ] OAM probe round-trip (LBM/LBR)
- [ ] Metrics calculation accuracy
- [ ] Link override behavior
- [ ] Disabled link exclusion
- [ ] Default behavior when no override
- [ ] Override with unavailable link
- [ ] Metrics publication on admin space

**Estimated LOC:** ~800

---

## 8. What Is NOT in Zenoh

The following are explicitly **outside** Zenoh's scope in this architecture:

| Component | Responsibility | Owner |
|-----------|----------------|-------|
| Modem metrics collection | Read from modem driver | External Controller |
| Link scoring/ranking | Apply weights to metrics | External Controller |
| Decision logic | Choose which link to use | External Controller |
| Cross-node optimization | Consider global state | External Controller |
| Policy enforcement | Apply routing policies | External Controller |
| Machine learning | Predictive models | External Controller |
| Modem control | SetMode, TrafficHint | External Controller |

---

## 9. Comparison: Removed Complexity

By adopting this architecture, the following is **removed** from Zenoh:

| Original Design Component | Status |
|---------------------------|--------|
| `transport_oam_modem` feature | Not needed |
| ModemMetricsProvider trait | Not in Zenoh |
| Unix socket modem client | Not in Zenoh |
| Combined scoring (OAM + modem) | Not in Zenoh |
| Scoring weights configuration | Not in Zenoh |
| Link scoring algorithm | Not in Zenoh |
| Normalized scoring / baselines | Not in Zenoh |
| Per-link-type presets | Not in Zenoh |

**Estimated savings:** ~2,000 LOC, 3 weeks of implementation

---

## 10. Summary

### 10.1 Zenoh Provides

1. **OAM probes** - Measure RTT, jitter, loss between Zenoh peers
2. **Metrics storage** - Per-link metrics with history
3. **Metrics API** - External access to collected metrics
4. **Link control API** - Accept commands to force/disable links
5. **Default behavior** - When no commands, behave as current Zenoh

### 10.2 External Controller Provides

1. **Modem integration** - Direct connection to modem drivers
2. **Data aggregation** - Combine Zenoh metrics + modem metrics + other sources
3. **Decision logic** - Custom scoring, ML, policies
4. **Commands** - Tell Zenoh which links to use

### 10.3 Benefits

1. **Simpler Zenoh** - Less code, fewer features, easier maintenance
2. **Flexible controller** - Any logic, any data sources, easy to update
3. **Separation of concerns** - Zenoh does transport, controller does decisions
4. **No lock-in** - Controller can be replaced without changing Zenoh
5. **Backward compatible** - No controller = current Zenoh behavior

---

## Appendix A: Feature Flag

All OAM functionality is gated behind a feature flag:

```toml
# io/zenoh-transport/Cargo.toml
[features]
transport_oam = []

# zenoh/Cargo.toml
[features]
transport_oam = ["zenoh-transport/transport_oam"]
```

When `transport_oam` is disabled:
- No OAM probes are sent
- No metrics are collected
- Link control APIs return errors
- Zenoh behaves exactly as today

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| **OAM** | Operations, Administration, and Maintenance |
| **LBM/LBR** | Loopback Message/Reply (ping for RTT) |
| **DMM/DMR** | Delay Measurement Message/Reply (one-way delay) |
| **SLM/SLR** | Synthetic Loss Message/Reply (packet loss) |
| **Forced link** | Link commanded by external controller |
| **Disabled link** | Link excluded from selection |
| **External controller** | Service that reads metrics and commands Zenoh |

---

## Appendix C: Related Documents

- [OAM_LINK_QUALITY_PLAN.md](OAM_LINK_QUALITY_PLAN.md) - Original OAM design (scoring in Zenoh)
- [MODEM_DRIVER_ICD.md](MODEM_DRIVER_ICD.md) - Modem driver OAM interface
- [MODEM_LINK_PROVIDER_ICD.md](MODEM_LINK_PROVIDER_ICD.md) - Modem as link provider
