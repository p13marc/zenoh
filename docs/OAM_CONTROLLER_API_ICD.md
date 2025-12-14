# OAM Controller API - Interface Control Document (ICD)

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Implemented |
| Date | 2024-12-14 |
| Feature Flag | `transport_oam` + `unstable` |
| Related | OAM_EXTERNAL_CONTROLLER_ARCHITECTURE.md |

---

## 1. Overview

This document defines the API interface for external controllers to interact with Zenoh's OAM (Operations, Administration, Maintenance) link quality system. The API allows controllers to:

1. **Read link quality metrics** collected via OAM probes
2. **Control link selection** by forcing or disabling specific links
3. **Query metrics via admin space** for remote monitoring

### 1.1 Architecture Summary

```
┌─────────────────────────────────────────────────────────────────┐
│                    External Controller                           │
│                                                                  │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐  │
│  │ 1. Read Metrics  │  │ 2. Make Decision │  │ 3. Apply      │  │
│  │                  │──▶│                  │──▶│    Override   │  │
│  │ link_quality_    │  │ (custom logic)   │  │ set_forced_   │  │
│  │ metrics()        │  │                  │  │ link()        │  │
│  └──────────────────┘  └──────────────────┘  └───────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Zenoh Session                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │  Link WiFi  │  │  Link LTE   │  │  Link Eth   │              │
│  │ RTT: 15ms   │  │ RTT: 45ms   │  │ RTT: 2ms    │              │
│  │ Jitter: 2ms │  │ Jitter: 8ms │  │ Jitter: 0ms │              │
│  │ Loss: 1%    │  │ Loss: 0.5%  │  │ Loss: 0%    │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Prerequisites

### 2.1 Feature Flags

The OAM Controller API requires the following Cargo features:

```toml
[dependencies]
zenoh = { version = "1.x", features = ["unstable", "transport_oam"] }
```

### 2.2 Configuration

Enable OAM in the Zenoh configuration:

```json5
{
  transport: {
    unicast: {
      oam: {
        enabled: true,            // Required: enable OAM probing
        probe_interval_ms: 100,   // Probe frequency (default: 100ms)
        probe_timeout_ms: 500,    // Probe timeout (default: 500ms)
        sample_window: 20,        // Moving average window (default: 20)
        failure_threshold: 3,     // Failures before link marked down (default: 3)
        publish_metrics: true,    // Expose in admin space (default: true)
        publish_interval_ms: 500  // Admin space update interval (default: 500ms)
      }
    }
  }
}
```

---

## 3. Data Types

### 3.1 LinkQualityMetrics

Per-link quality metrics collected from OAM probes.

```rust
pub struct LinkQualityMetrics {
    /// Zenoh locator for this link (e.g., "tcp/192.168.1.100:7447")
    pub locator: Locator,
    
    /// Remote peer ZenohId
    pub peer: ZenohId,
    
    // === RTT Metrics ===
    /// Current (most recent) RTT measurement
    pub rtt_current: Duration,
    
    /// Minimum observed RTT
    pub rtt_min: Duration,
    
    /// Maximum observed RTT  
    pub rtt_max: Duration,
    
    /// Exponential moving average RTT
    pub rtt_avg: Duration,
    
    // === Jitter (RFC 3550) ===
    /// Interarrival jitter as defined in RFC 3550
    pub jitter: Duration,
    
    // === Packet Loss ===
    /// Total probes sent
    pub tx_probe_count: u64,
    
    /// Total probe replies received
    pub rx_probe_count: u64,
    
    /// Loss ratio (0.0 = no loss, 1.0 = 100% loss)
    pub loss_ratio: f64,
    
    // === Link State ===
    /// Current operational state
    pub state: LinkState,
    
    /// Consecutive probe failures
    pub consecutive_failures: u32,
    
    /// Link is responding to probes
    pub is_alive: bool,
    
    // === Timestamps ===
    /// When metrics were last updated
    pub last_updated: Instant,
}
```

### 3.2 LinkState

Operational state of a link.

```rust
pub enum LinkState {
    /// Initial state, no probes exchanged yet
    Unknown,
    
    /// Link is responding normally
    Operational,
    
    /// Link is responding but with degraded metrics
    Degraded,
    
    /// Link is marginally operational
    Marginal,
    
    /// Link is not responding (exceeded failure threshold)
    Failed,
}
```

### 3.3 LinkOverrides

Control structure for forcing and disabling links.

```rust
pub struct LinkOverrides {
    // Internal fields - access via methods
}

impl LinkOverrides {
    /// Force all traffic to a peer to use a specific link
    pub fn set_forced_link(&self, peer: ZenohId, link: Locator);
    
    /// Clear forced link for a peer
    pub fn clear_forced_link(&self, peer: &ZenohId);
    
    /// Get the forced link for a peer (if any)
    pub fn get_forced_link(&self, peer: &ZenohId) -> Option<Locator>;
    
    /// Disable a link (excluded from selection)
    pub fn disable_link(&self, link: Locator);
    
    /// Re-enable a previously disabled link
    pub fn enable_link(&self, link: &Locator);
    
    /// Check if a link is disabled
    pub fn is_disabled(&self, link: &Locator) -> bool;
}
```

---

## 4. Session API

All methods are available on `zenoh::Session` when the `transport_oam` feature is enabled.

### 4.1 Reading Metrics

#### `link_quality_metrics()`

Get OAM metrics for all active links across all transports.

```rust
#[zenoh_macros::unstable]
pub fn link_quality_metrics(&self) -> Vec<LinkQualityMetrics>
```

**Returns:** Vector of metrics for each active link.

**Example:**
```rust
let metrics = session.link_quality_metrics();
for m in &metrics {
    println!("Link to {}: RTT={:?}, jitter={:?}, loss={:.2}%",
        m.peer, m.rtt_avg, m.jitter, m.loss_ratio * 100.0);
}
```

---

#### `link_quality_metrics_for_peer()`

Get OAM metrics for links to a specific peer.

```rust
#[zenoh_macros::unstable]
pub fn link_quality_metrics_for_peer(
    &self,
    peer: ZenohId
) -> Vec<LinkQualityMetrics>
```

**Arguments:**
- `peer` - The ZenohId of the remote peer

**Returns:** Vector of metrics for links to the specified peer.

**Example:**
```rust
let peer_zid: ZenohId = "0123456789abcdef".parse().unwrap();
let metrics = session.link_quality_metrics_for_peer(peer_zid);
```

---

### 4.2 Controlling Links

#### `set_forced_link()`

Force all traffic to a specific peer to use a specific link.

```rust
#[zenoh_macros::unstable]
pub fn set_forced_link(
    &self,
    peer: ZenohId,
    link: Locator
)
```

**Arguments:**
- `peer` - The remote peer ZenohId
- `link` - The locator of the link to use

**Behavior:**
- When set, Zenoh will use ONLY this link for the specified peer
- Overrides all other link selection logic (priority, reliability, etc.)
- If the forced link becomes unavailable, traffic to that peer will fail

**Example:**
```rust
let peer: ZenohId = "0123456789abcdef".parse().unwrap();
let link: Locator = "tcp/192.168.1.100:7447".parse().unwrap();
session.set_forced_link(peer, link);
```

---

#### `clear_forced_link()`

Clear the forced link for a peer, returning to default link selection.

```rust
#[zenoh_macros::unstable]
pub fn clear_forced_link(&self, peer: ZenohId)
```

**Arguments:**
- `peer` - The remote peer ZenohId

**Example:**
```rust
session.clear_forced_link(peer);
```

---

#### `disable_link()`

Disable a link entirely. Traffic will not use this link.

```rust
#[zenoh_macros::unstable]
pub fn disable_link(&self, link: Locator)
```

**Arguments:**
- `link` - The locator of the link to disable

**Behavior:**
- The link remains connected but is excluded from selection
- OAM probes continue to be sent for monitoring
- The link can be re-enabled later

**Example:**
```rust
let bad_link: Locator = "tcp/192.168.1.200:7447".parse().unwrap();
session.disable_link(bad_link);
```

---

#### `enable_link()`

Re-enable a previously disabled link.

```rust
#[zenoh_macros::unstable]
pub fn enable_link(&self, link: &Locator)
```

**Arguments:**
- `link` - The locator of the link to enable

**Example:**
```rust
session.enable_link(&bad_link);
```

---

#### `get_link_overrides()`

Get direct access to the LinkOverrides for advanced control.

```rust
#[zenoh_macros::unstable]
pub fn get_link_overrides(&self) -> Arc<RwLock<LinkOverrides>>
```

**Returns:** Thread-safe reference to the LinkOverrides structure.

**Example:**
```rust
let overrides = session.get_link_overrides();
let guard = overrides.read().unwrap();
if let Some(forced) = guard.get_forced_link(&peer) {
    println!("Peer {} forced to use {}", peer, forced);
}
```

---

## 5. Admin Space API

Metrics are published to the Zenoh admin space for remote monitoring.

### 5.1 Key Format

```
@/<zid>/session/transport/<peer_zid>
```

Where:
- `<zid>` - Local Zenoh ID
- `<peer_zid>` - Remote peer Zenoh ID

### 5.2 Payload Format (JSON)

```json
{
  "peer": "0123456789abcdef...",
  "links": [...],
  "oam_metrics": [
    {
      "locator": "tcp/192.168.1.100:7447",
      "rtt_current_us": 1250,
      "rtt_avg_us": 1150,
      "rtt_min_us": 980,
      "rtt_max_us": 2100,
      "jitter_us": 85,
      "loss_ratio": 0.001,
      "state": "Operational",
      "is_alive": true
    },
    {
      "locator": "udp/192.168.1.100:7447",
      "rtt_current_us": 2500,
      "rtt_avg_us": 2800,
      "rtt_min_us": 2000,
      "rtt_max_us": 5000,
      "jitter_us": 450,
      "loss_ratio": 0.02,
      "state": "Degraded",
      "is_alive": true
    }
  ]
}
```

### 5.3 Querying Metrics Remotely

```rust
// Query all routers for their transport metrics
let replies = session.get("@/*/session/transport/**").await?;
while let Ok(reply) = replies.recv_async().await {
    if let Ok(sample) = reply.result() {
        let payload: String = sample.payload().deserialize()?;
        let json: serde_json::Value = serde_json::from_str(&payload)?;
        
        if let Some(oam_metrics) = json.get("oam_metrics") {
            println!("{}: {:?}", sample.key_expr(), oam_metrics);
        }
    }
}
```

---

## 6. Complete Controller Example

```rust
use std::collections::HashMap;
use std::time::Duration;
use zenoh::prelude::*;
use zenoh::config::ZenohId;
use zenoh::link_quality::{LinkQualityMetrics, LinkState};

/// Simple external controller that selects the best link per peer
async fn run_controller(session: &Session) {
    let mut last_decisions: HashMap<ZenohId, String> = HashMap::new();
    
    loop {
        // 1. Collect metrics for all links
        let metrics = session.link_quality_metrics();
        
        // 2. Group by peer
        let mut by_peer: HashMap<ZenohId, Vec<&LinkQualityMetrics>> = HashMap::new();
        for m in &metrics {
            by_peer.entry(m.peer).or_default().push(m);
        }
        
        // 3. For each peer, select the best link
        for (peer, peer_metrics) in by_peer {
            let best = select_best_link(&peer_metrics);
            
            match best {
                Some(link) => {
                    let locator_str = link.locator.to_string();
                    
                    // Only update if decision changed (hysteresis)
                    if last_decisions.get(&peer) != Some(&locator_str) {
                        println!("Switching peer {} to link {} (RTT: {:?}, loss: {:.2}%)",
                            peer, link.locator, link.rtt_avg, link.loss_ratio * 100.0);
                        
                        session.set_forced_link(peer, link.locator.clone());
                        last_decisions.insert(peer, locator_str);
                    }
                }
                None => {
                    // No healthy link available
                    if last_decisions.remove(&peer).is_some() {
                        println!("No healthy link for peer {}, clearing override", peer);
                        session.clear_forced_link(peer);
                    }
                }
            }
        }
        
        // 4. Wait before next evaluation
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Select the best link based on RTT and loss
fn select_best_link<'a>(metrics: &[&'a LinkQualityMetrics]) -> Option<&'a LinkQualityMetrics> {
    metrics
        .iter()
        .filter(|m| {
            // Filter out failed links
            !matches!(m.state, LinkState::Failed) && m.is_alive
        })
        .min_by(|a, b| {
            // Score: lower is better
            // RTT weight: 1.0, Loss weight: 100.0
            let score_a = a.rtt_avg.as_micros() as f64 + a.loss_ratio * 100_000.0;
            let score_b = b.rtt_avg.as_micros() as f64 + b.loss_ratio * 100_000.0;
            score_a.partial_cmp(&score_b).unwrap()
        })
        .copied()
}

#[tokio::main]
async fn main() {
    // Open session with OAM enabled
    let config = zenoh::Config::from_file("config.json5").unwrap();
    let session = zenoh::open(config).await.unwrap();
    
    // Run the controller
    run_controller(&session).await;
}
```

---

## 7. Behavioral Specifications

### 7.1 Default Behavior (No Controller)

When no controller is active (no forced links set):
- Zenoh uses its default link selection algorithm
- Links are selected based on priority, reliability requirements, and availability
- OAM metrics are collected but not used for selection

### 7.2 Forced Link Behavior

| Scenario | Behavior |
|----------|----------|
| `set_forced_link(peer, link)` | All traffic to `peer` uses `link` exclusively |
| Forced link becomes unavailable | Traffic to `peer` fails until override cleared or link recovers |
| `clear_forced_link(peer)` | Return to default behavior for `peer` |
| Multiple `set_forced_link` for same peer | Latest call wins |

### 7.3 Disabled Link Behavior

| Scenario | Behavior |
|----------|----------|
| `disable_link(link)` | Link excluded from selection for all peers |
| OAM probes | Continue to be sent/received on disabled links |
| Forced link that is also disabled | Forced link takes precedence (will be used) |
| `enable_link(link)` | Link available for selection again |

### 7.4 Metric Collection

| Parameter | Value |
|-----------|-------|
| Probe type | Loopback (LBM/LBR) |
| Default interval | 100ms |
| RTT calculation | Exponential moving average (α = 2/(N+1), N = sample_window) |
| Jitter calculation | RFC 3550 interarrival jitter |
| Loss calculation | 1 - (rx_probe_count / tx_probe_count) |
| Failure threshold | 3 consecutive missed probes (configurable) |

---

## 8. Error Handling

### 8.1 API Errors

The Session API methods do not return errors. Invalid operations are silently ignored:

| Operation | Invalid Case | Behavior |
|-----------|--------------|----------|
| `set_forced_link` | Link not connected | Override stored, applied when link connects |
| `clear_forced_link` | No override set | No-op |
| `disable_link` | Link not found | Disabled entry stored |
| `enable_link` | Link not disabled | No-op |

### 8.2 Link Failures

When a forced link fails:
1. `LinkQualityMetrics.state` changes to `Failed`
2. `LinkQualityMetrics.is_alive` becomes `false`
3. Traffic to that peer will fail (not rerouted)
4. Controller should detect via metrics and call `clear_forced_link` or `set_forced_link` to another link

---

## 9. Thread Safety

All API methods are thread-safe:
- `link_quality_metrics()` - Returns a snapshot (cloned data)
- `set_forced_link()` / `clear_forced_link()` - Uses internal RwLock
- `disable_link()` / `enable_link()` - Uses internal RwLock
- `get_link_overrides()` - Returns `Arc<RwLock<...>>`

Controllers can safely call these methods from multiple threads.

---

## 10. Performance Considerations

### 10.1 Metric Collection Overhead

| Configuration | CPU Overhead | Network Overhead |
|---------------|--------------|------------------|
| 100ms interval, 10 links | ~0.1% | ~1 KB/s per link |
| 50ms interval, 10 links | ~0.2% | ~2 KB/s per link |
| 100ms interval, 100 links | ~1% | ~10 KB/s total |

### 10.2 Controller Recommendations

- Evaluation interval: 500ms - 2s (avoid thrashing)
- Hysteresis: Require 5-10% improvement before switching
- Metrics caching: `link_quality_metrics()` clones data; cache if calling frequently

---

## Appendix A: Import Paths

```rust
// Main session type
use zenoh::Session;

// OAM types (requires features = ["unstable", "transport_oam"])
use zenoh::link_quality::{
    LinkQualityMetrics,
    LinkState,
    LinkOverrides,
    OamConfig,
};

// Locator and ZenohId
use zenoh::core::Locator;
use zenoh::config::ZenohId;
```

---

## Appendix B: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-12-14 | Initial implementation |
