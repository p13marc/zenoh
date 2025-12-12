# OAM Link Quality - Implementation Plan

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Draft |
| Last Updated | 2024-12-12 |
| Related Documents | OAM_LINK_QUALITY_PLAN.md, MODEM_DRIVER_ICD.md, OAM_CONFIG_EXAMPLE.json5 |

---

## 1. Executive Summary

### 1.1 Overview

This document describes the implementation plan for OAM (Operations, Administration, and Maintenance) based link quality measurement in Zenoh. The feature enables intelligent link selection in multilink transports based on real-time metrics: latency (RTT), jitter, and packet loss.

### 1.2 Key Metrics

| Metric | Value |
|--------|-------|
| **Feasibility** | HIGH - Architecture well-suited |
| **Risk Level** | LOW - No major refactoring needed |
| **Total Duration** | 10-14 weeks |
| **Core OAM (Required)** | 7 weeks, ~3,500 LOC |
| **Modem Integration (Optional)** | 3 weeks, ~1,900 LOC |
| **Testing & Documentation** | 4 weeks, ~1,800 LOC |

### 1.3 Feature Tiers

The implementation is structured in **two tiers**:

```
┌─────────────────────────────────────────────────────────────────┐
│                      TIER 1: Core OAM (Required)                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ OAM Probes  │  │  Metrics    │  │  Quality-Based Link     │  │
│  │ (LBM/DMM/   │──│  Collection │──│  Selection              │  │
│  │  SLM)       │  │  & Scoring  │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
│                                                                 │
│  Feature Flag: transport_oam                                    │
│  Standalone: YES - Works without any external dependencies      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ (optional enhancement)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                TIER 2: Modem Integration (Optional)             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Modem       │  │  Combined   │  │  Predictive Quality     │  │
│  │ Metrics     │──│  Scoring    │──│  & Alerts               │  │
│  │ Provider    │  │  (OAM+PHY)  │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
│                                                                 │
│  Feature Flag: transport_oam_modem                              │
│  Requires: transport_oam + external modem driver                │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Architecture Assessment

### 2.1 Zenoh Compatibility Matrix

| Requirement | Support | Assessment | Notes |
|-------------|---------|------------|-------|
| Per-link metrics storage | ✅ | Excellent | `TransportLinkUnicastUniversal` already wraps per-link state |
| Link selection extension | ✅ | Excellent | Isolated in `tx.rs::select()`, ~50 LOC change |
| OAM protocol support | ✅ | Excellent | `TransportBody::Oam` already in wire format |
| Configuration system | ✅ | Good | Validated struct pattern supports new types |
| Priority/Express integration | ✅ | Excellent | Three-tier matching; OAM adds scoring layer |
| Modem metrics (optional) | ⚠️ | Moderate | Requires new infrastructure, not plugin-based |
| Backward compatibility | ✅ | Excellent | Feature-gated, disabled by default |

### 2.2 Key Integration Points

```
┌──────────────────────────────────────────────────────────────────────┐
│                        ZENOH TRANSPORT LAYER                          │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────┐         ┌─────────────────────────────────┐ │
│  │  TransportManager   │         │  TransportUnicastUniversal      │ │
│  │  (manager.rs)       │────────>│  (transport.rs)                 │ │
│  │                     │         │                                 │ │
│  │  + OamConfig        │         │  + links: Vec<LinkUniversal>    │ │
│  └─────────────────────┘         │  + oam_config                   │ │
│                                  └──────────┬──────────────────────┘ │
│                                             │                        │
│                    ┌────────────────────────┼────────────────────┐   │
│                    │                        │                    │   │
│                    ▼                        ▼                    ▼   │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌──────────────┐ │
│  │ TransportLinkUni-   │  │ TransportLinkUni-   │  │    ...       │ │
│  │ castUniversal       │  │ castUniversal       │  │              │ │
│  │ (link.rs)           │  │ (link.rs)           │  │              │ │
│  │                     │  │                     │  │              │ │
│  │ + pipeline          │  │ + pipeline          │  │              │ │
│  │ + NEW: quality_     │  │ + NEW: quality_     │  │              │ │
│  │        metrics      │  │        metrics      │  │              │ │
│  │ + NEW: oam_prober   │  │ + NEW: oam_prober   │  │              │ │
│  └─────────┬───────────┘  └─────────┬───────────┘  └──────────────┘ │
│            │                        │                                │
│            ▼                        ▼                                │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                        tx.rs::select()                          │ │
│  │                                                                 │ │
│  │  CURRENT:  Match by (Reliability, PriorityRange)                │ │
│  │  NEW:      + Score by LinkQualityMetrics                        │ │
│  │            + Apply priority-specific weights                     │ │
│  │            + Hysteresis to prevent flapping                     │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                       │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.3 No Breaking Changes

All modifications are:
- **Additive**: New modules, new config fields
- **Feature-gated**: `#[cfg(feature = "transport_oam")]`
- **Backward compatible**: Existing behavior preserved when feature disabled

---

## 3. Feature Flags

### 3.1 Flag Definitions

```toml
# In io/zenoh-transport/Cargo.toml and zenoh/Cargo.toml

[features]
# Core OAM: Probing, metrics, quality-based link selection
# No external dependencies required
transport_oam = []

# Optional: External modem metrics integration
# Requires: transport_oam
# Adds: Unix socket modem provider, combined scoring
transport_oam_modem = ["transport_oam"]
```

### 3.2 Compilation Matrix

| Feature Flags | OAM Probes | Link Scoring | Modem Metrics | Use Case |
|---------------|------------|--------------|---------------|----------|
| (none) | ❌ | ❌ | ❌ | Standard Zenoh |
| `transport_oam` | ✅ | ✅ | ❌ | Quality routing without modems |
| `transport_oam` + `transport_oam_modem` | ✅ | ✅ | ✅ | Full feature set |

### 3.3 Runtime Configuration

Even with feature flags enabled, OAM is **disabled by default** at runtime:

```json5
// Default: OAM disabled, standard KeepAlive behavior
transport: {
  unicast: {
    oam: {
      enabled: false,  // <-- Must explicitly enable
    }
  }
}
```

---

## 4. Implementation Phases

### 4.1 Phase Overview

```
Duration:  Week 1    Week 2    Week 3    Week 4    Week 5    Week 6    Week 7
           ├─────────┼─────────┼─────────┼─────────┼─────────┼─────────┤
Phase 1    ██████████████████░░                                         Protocol & Config
Phase 2              ░░░░░░░░░░██████████████████████████████░░         Core OAM
Phase 3                                  ░░░░░░░░░░██████████████████   Link Selection
           ════════════════════════════════════════════════════════════
           │          M1: Protocol     M2: Probes    M3: Quality       │
           │              Ready          Working       Routing         │
           └────────────────────────────────────────────────────────────┘
                              TIER 1 COMPLETE (Week 7)

Duration:  Week 8    Week 9    Week 10   Week 11   Week 12   Week 13   Week 14
           ├─────────┼─────────┼─────────┼─────────┼─────────┼─────────┤
Phase 4    ██████████████████████████████░░                             Modem (OPTIONAL)
Phase 5                        ░░░░░░░░░░██████████████████████████████ Testing & Docs
           ════════════════════════════════════════════════════════════
           │              M4: Modem                 M5: Production     │
           │             Integration                   Ready           │
           └────────────────────────────────────────────────────────────┘
                              TIER 2 COMPLETE (Week 14)
```

### 4.2 Minimum Viable Product (MVP)

**MVP = Phase 1 + Phase 2 + Phase 3 (7 weeks)**

At MVP completion:
- ✅ OAM probes measure RTT, jitter, loss per link
- ✅ Link selection uses quality scores
- ✅ Priority-aware scoring (different weights per priority)
- ✅ KeepAlive replaced by OAM probes when enabled
- ✅ Configuration presets for different link types
- ❌ No external modem integration (Phase 4)
- ❌ Comprehensive testing/docs (Phase 5)

---

## 5. Phase 1: Protocol & Configuration (Weeks 1-2)

### 5.1 Objectives

- Define OAM message types (LBM, LBR, DMM, DMR, SLM, SLR)
- Define configuration structures
- Establish preset system for link types

### 5.2 Week 1: Protocol Layer

#### 5.2.1 Files to Modify

| File | Changes |
|------|---------|
| `commons/zenoh-protocol/src/transport/oam.rs` | Add OAM ID constants, payload types |
| `commons/zenoh-protocol/src/transport/mod.rs` | Re-export new types |
| `commons/zenoh-codec/src/transport/oam.rs` | Payload serialization (if needed) |

#### 5.2.2 OAM Message IDs

```rust
// commons/zenoh-protocol/src/transport/oam.rs

pub mod id {
    use super::OamId;
    
    // Loopback (simple ping/pong for RTT)
    pub const LBM: OamId = 0x0010;  // Loopback Message
    pub const LBR: OamId = 0x0011;  // Loopback Reply
    
    // Delay Measurement (4-timestamp for one-way delay)
    pub const DMM: OamId = 0x0020;  // Delay Measurement Message
    pub const DMR: OamId = 0x0021;  // Delay Measurement Reply
    
    // Synthetic Loss Measurement
    pub const SLM: OamId = 0x0030;  // Synthetic Loss Message
    pub const SLR: OamId = 0x0031;  // Synthetic Loss Reply
}
```

#### 5.2.3 Payload Structures

```rust
/// Loopback probe payload (12 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackPayload {
    pub seq: u32,           // Sequence number
    pub timestamp_ns: u64,  // Sender timestamp (nanoseconds)
}

/// Delay measurement payload (28 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelayMeasurementPayload {
    pub seq: u32,
    pub t1: u64,  // DMM send time (initiator)
    pub t2: u64,  // DMM receive time (responder) - 0 in DMM
    pub t3: u64,  // DMR send time (responder) - 0 in DMM
    // t4 recorded locally by initiator
}

/// Synthetic loss payload (20 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticLossPayload {
    pub seq: u32,
    pub tx_counter: u64,  // Total probes sent
    pub rx_counter: u64,  // Total probes received (filled in reply)
}
```

#### 5.2.4 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Define OAM ID constants | 30 | 2 | Simple module with constants |
| Define payload structures | 60 | 3 | Three structs with derives |
| Implement payload encode/decode | 80 | 4 | Using ZBuf encoding |
| Add OAM QoS defaults | 20 | 1 | Priority::Control, Block |
| Unit tests for serialization | 100 | 4 | Round-trip tests |
| **Week 1 Total** | **290** | **14** | |

#### 5.2.5 Deliverables

- [ ] `oam::id` module with all constants
- [ ] Payload structs with encode/decode
- [ ] Unit tests passing
- [ ] Documentation comments

### 5.3 Week 2: Configuration Layer

#### 5.3.1 Files to Modify

| File | Changes |
|------|---------|
| `commons/zenoh-config/src/lib.rs` | Add OamConf, ScoringConf, etc. |
| `commons/zenoh-config/src/defaults.rs` | Add OAM defaults |
| `commons/zenoh-config/src/mode_dependent.rs` | Mode-specific defaults |

#### 5.3.2 Configuration Structure

```rust
/// Main OAM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OamConf {
    /// Enable OAM probing (default: false)
    pub enabled: bool,
    
    /// Probe interval in milliseconds (default: 100)
    pub probe_interval_ms: u64,
    
    /// Probe timeout in milliseconds (default: 500)
    pub probe_timeout_ms: u64,
    
    /// Number of samples for moving average (default: 20)
    pub sample_window: usize,
    
    /// Consecutive failures before link marked degraded (default: 3)
    pub failure_threshold: u32,
    
    /// Probe types to use (default: ["loopback"])
    pub probe_types: Vec<ProbeType>,
    
    /// Scoring configuration
    pub scoring: ScoringConf,
    
    /// Link-type presets (fiber, radio, satellite, acoustic)
    pub presets: HashMap<String, PresetConf>,
    
    /// Per-link overrides by locator pattern
    pub links: Vec<LinkPatternConf>,
    
    /// Clock synchronization settings (for DMM)
    pub clock_sync: Option<ClockSyncConf>,
    
    /// Modem metrics configuration (optional, requires transport_oam_modem)
    #[cfg(feature = "transport_oam_modem")]
    pub modem_metrics: Option<ModemMetricsConf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConf {
    /// Scoring mode: "absolute" or "normalized"
    pub mode: ScoringMode,
    
    /// Default weights
    pub weights: ScoringWeights,
    
    /// Per-priority weight overrides
    pub priority_weights: HashMap<String, ScoringWeights>,
    
    /// Selection strategy
    pub selection_strategy: SelectionStrategy,
    
    /// Hysteresis to prevent link flapping
    pub hysteresis: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub rtt: f64,
    pub jitter: f64,
    pub loss: f64,
    #[serde(default)]
    pub asymmetry: f64,
    #[serde(default)]
    pub bandwidth: f64,
    #[serde(default)]
    pub capacity: f64,
}
```

#### 5.3.3 Preset Definitions

```rust
impl OamConf {
    pub fn preset_fiber() -> PresetConf { /* ... */ }
    pub fn preset_ethernet_lan() -> PresetConf { /* ... */ }
    pub fn preset_ethernet_wan() -> PresetConf { /* ... */ }
    pub fn preset_radio() -> PresetConf { /* ... */ }
    pub fn preset_satellite_geo() -> PresetConf { /* ... */ }
    pub fn preset_satellite_leo() -> PresetConf { /* ... */ }
    pub fn preset_acoustic() -> PresetConf { /* ... */ }
}
```

#### 5.3.4 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Define OamConf struct | 80 | 3 | Main config with all fields |
| Define ScoringConf, ScoringWeights | 60 | 2 | Scoring parameters |
| Define PresetConf, LinkPatternConf | 80 | 3 | Per-link config |
| Define enums (ScoringMode, SelectionStrategy, ProbeType) | 40 | 2 | Type-safe options |
| Implement preset factory methods | 120 | 5 | All 7 presets |
| Add defaults | 60 | 2 | Default values |
| Configuration validation | 80 | 4 | Validate consistency |
| Integration tests for parsing | 100 | 4 | JSON5 round-trip |
| **Week 2 Total** | **620** | **25** | |

#### 5.3.5 Deliverables

- [ ] Complete `OamConf` with all nested types
- [ ] All 7 presets defined
- [ ] Configuration validation working
- [ ] OAM_CONFIG_EXAMPLE.json5 parses correctly
- [ ] Unit and integration tests passing

### 5.4 Phase 1 Summary

| Metric | Value |
|--------|-------|
| Duration | 2 weeks |
| Total LOC | ~910 |
| Files Modified | 5-6 |
| Files Created | 0 |

**Exit Criteria:**
- [ ] All OAM ID constants defined
- [ ] Payload serialization tests passing
- [ ] Configuration parses from JSON5
- [ ] All presets defined with sensible defaults

---

## 6. Phase 2: Core OAM Implementation (Weeks 3-5)

### 6.1 Objectives

- Create OAM module structure
- Implement metrics collection and scoring
- Implement prober task (sends probes)
- Implement responder (replies to probes)
- Integrate with RX path

### 6.2 Module Structure

```
io/zenoh-transport/src/unicast/oam/
├── mod.rs              # Module entry, public API
├── config.rs           # Runtime config handling
├── metrics.rs          # LinkQualityMetrics, scoring
├── prober.rs           # Probe sending task
├── responder.rs        # Probe reply handling
└── scorer.rs           # Scoring algorithms
```

### 6.3 Week 3: Metrics & Module Structure

#### 6.3.1 Files to Create

| File | Purpose |
|------|---------|
| `io/zenoh-transport/src/unicast/oam/mod.rs` | Module entry |
| `io/zenoh-transport/src/unicast/oam/metrics.rs` | Metrics struct and calculations |
| `io/zenoh-transport/src/unicast/oam/scorer.rs` | Scoring algorithms |

#### 6.3.2 LinkQualityMetrics

```rust
/// Per-link quality metrics collected from OAM probes
#[derive(Debug, Clone)]
pub struct LinkQualityMetrics {
    // === RTT Metrics ===
    pub rtt_current: Duration,
    pub rtt_min: Duration,
    pub rtt_max: Duration,
    pub rtt_avg: Duration,      // Exponential moving average
    
    // === Jitter (RFC 3550) ===
    pub jitter: Duration,
    
    // === One-Way Delay (requires clock sync) ===
    pub forward_delay: Option<Duration>,
    pub reverse_delay: Option<Duration>,
    pub delay_asymmetry: Option<f64>,
    
    // === Packet Loss ===
    pub tx_probe_count: u64,
    pub rx_probe_count: u64,
    pub loss_ratio: f64,        // 0.0 to 1.0
    
    // === Link State ===
    pub state: LinkState,
    pub consecutive_failures: u32,
    
    // === Timestamps ===
    pub last_probe_sent: Instant,
    pub last_probe_received: Instant,
    pub last_state_change: Instant,
    
    // === Configuration ===
    baseline: LinkTypeBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Unknown,
    Operational,
    Degraded,
    Marginal,
    Failed,
}

impl LinkQualityMetrics {
    /// Update metrics with new probe response
    pub fn update_from_probe(&mut self, rtt: Duration, seq: u32);
    
    /// Update jitter using RFC 3550 algorithm
    fn update_jitter(&mut self, rtt: Duration);
    
    /// Transition state based on current metrics
    pub fn update_state(&mut self, thresholds: &StateThresholds);
    
    /// Calculate absolute score (0-100)
    pub fn score(&self, weights: &ScoringWeights) -> f64;
    
    /// Calculate normalized score relative to baseline
    pub fn normalized_score(&self, weights: &ScoringWeights) -> f64;
}
```

#### 6.3.3 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Create module structure | 40 | 2 | mod.rs, feature gates |
| Implement LinkQualityMetrics struct | 120 | 5 | All fields, derives |
| Implement jitter calculation | 40 | 2 | RFC 3550 algorithm |
| Implement LinkState transitions | 80 | 3 | State machine |
| Implement absolute scoring | 60 | 3 | score() method |
| Implement normalized scoring | 80 | 4 | With baseline |
| Implement LinkTypeBaseline | 100 | 4 | All link types |
| Unit tests | 150 | 6 | Metrics and scoring |
| **Week 3 Total** | **670** | **29** | |

### 6.4 Week 4: OAM Prober Task

#### 6.4.1 Files to Create

| File | Purpose |
|------|---------|
| `io/zenoh-transport/src/unicast/oam/prober.rs` | Async probe sending task |

#### 6.4.2 Prober Design

```rust
/// OAM Prober - sends probes and tracks pending responses
pub struct OamProber {
    config: OamConfig,
    metrics: Arc<RwLock<LinkQualityMetrics>>,
    pending_probes: HashMap<u32, PendingProbe>,
    seq: AtomicU32,
    tx_sender: /* channel to TX task */,
}

struct PendingProbe {
    sent_at: Instant,
    probe_type: ProbeType,
}

impl OamProber {
    /// Spawn prober task for a link
    pub fn spawn(
        config: OamConfig,
        metrics: Arc<RwLock<LinkQualityMetrics>>,
        link: TransportLinkUnicast,
        token: CancellationToken,
    ) -> JoinHandle<ZResult<()>>;
    
    /// Main prober loop
    async fn run(&mut self) -> ZResult<()>;
    
    /// Send a loopback probe
    async fn send_loopback(&mut self) -> ZResult<()>;
    
    /// Send a delay measurement probe
    async fn send_delay_measurement(&mut self) -> ZResult<()>;
    
    /// Send a synthetic loss probe
    async fn send_synthetic_loss(&mut self) -> ZResult<()>;
    
    /// Handle probe reply, update metrics
    pub fn handle_reply(&mut self, oam_id: OamId, payload: &[u8]);
    
    /// Check for timed-out probes
    fn check_timeouts(&mut self);
}
```

#### 6.4.3 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Implement OamProber struct | 80 | 3 | Fields, constructors |
| Implement spawn() and run() | 100 | 4 | Async task loop |
| Implement send_loopback() | 80 | 3 | LBM creation and sending |
| Implement send_delay_measurement() | 80 | 3 | DMM with timestamps |
| Implement send_synthetic_loss() | 60 | 3 | SLM with counters |
| Implement pending probe tracking | 80 | 3 | HashMap management |
| Implement timeout handling | 60 | 3 | Mark failed, update metrics |
| Unit tests with mock link | 120 | 6 | All probe types |
| **Week 4 Total** | **660** | **28** | |

### 6.5 Week 5: OAM Responder & RX Integration

#### 6.5.1 Files to Create/Modify

| File | Purpose |
|------|---------|
| `io/zenoh-transport/src/unicast/oam/responder.rs` | Reply generation |
| `io/zenoh-transport/src/unicast/universal/rx.rs` | OAM message routing |

#### 6.5.2 Responder Design

```rust
/// OAM Responder - generates replies to incoming probes
pub struct OamResponder;

impl OamResponder {
    /// Handle incoming OAM message, return reply if applicable
    pub fn handle_oam(
        oam_id: OamId,
        payload: ZBuf,
        link: &TransportLinkUnicast,
    ) -> Option<TransportMessage>;
    
    /// Generate LBR from LBM
    fn reply_loopback(payload: &LoopbackPayload) -> TransportMessage;
    
    /// Generate DMR from DMM
    fn reply_delay_measurement(
        payload: &DelayMeasurementPayload,
        receive_time: u64,
    ) -> TransportMessage;
    
    /// Generate SLR from SLM
    fn reply_synthetic_loss(
        payload: &SyntheticLossPayload,
        rx_counter: u64,
    ) -> TransportMessage;
}
```

#### 6.5.3 RX Integration

```rust
// In rx.rs::read_messages()

match msg.body {
    TransportBody::KeepAlive(_) => {
        // Existing: ignore (lease reset happens elsewhere)
    }
    
    #[cfg(feature = "transport_oam")]
    TransportBody::Oam(oam) => {
        use crate::unicast::oam::{id, responder::OamResponder, prober};
        
        match oam.id {
            // Probe requests - generate reply
            id::LBM | id::DMM | id::SLM => {
                if let Some(reply) = OamResponder::handle_oam(
                    oam.id, 
                    oam.payload,
                    &self.link,
                ) {
                    self.send_oam_reply(reply).await?;
                }
            }
            
            // Probe replies - route to prober
            id::LBR | id::DMR | id::SLR => {
                if let Some(prober) = &self.oam_prober {
                    prober.handle_reply(oam.id, &oam.payload);
                }
            }
            
            // Unknown OAM ID - log and ignore (backward compat)
            _ => {
                tracing::debug!("Unknown OAM ID: {:#06x}", oam.id);
            }
        }
    }
    
    _ => { /* existing handling */ }
}
```

#### 6.5.4 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Implement OamResponder | 80 | 3 | Static methods |
| Implement reply_loopback() | 60 | 3 | LBR generation |
| Implement reply_delay_measurement() | 80 | 4 | DMR with t2, t3 |
| Implement reply_synthetic_loss() | 60 | 3 | SLR with counters |
| Modify rx.rs for OAM routing | 80 | 4 | Match on OAM IDs |
| Route replies to prober | 60 | 3 | Channel or direct call |
| Handle unknown OAM IDs | 30 | 1 | Log and ignore |
| Integration tests (prober + responder) | 200 | 8 | End-to-end |
| **Week 5 Total** | **650** | **29** | |

### 6.6 Phase 2 Summary

| Metric | Value |
|--------|-------|
| Duration | 3 weeks |
| Total LOC | ~1,980 |
| Files Created | 5 |
| Files Modified | 2-3 |

**Exit Criteria:**
- [ ] LBM/LBR round-trip works between nodes
- [ ] RTT, jitter, loss calculated correctly
- [ ] Prober handles timeouts, updates metrics
- [ ] Unknown OAM IDs logged and ignored
- [ ] Integration test passing

---

## 7. Phase 3: Link Selection Integration (Weeks 6-7)

### 7.1 Objectives

- Integrate OAM metrics with link selection
- Implement priority-aware scoring
- Replace KeepAlive with OAM probes
- Implement hysteresis

### 7.2 Week 6: Link Selection Enhancement

#### 7.2.1 Files to Modify

| File | Changes |
|------|---------|
| `io/zenoh-transport/src/unicast/universal/tx.rs` | Extend select() |
| `io/zenoh-transport/src/unicast/universal/link.rs` | Add metrics field |
| `io/zenoh-transport/src/unicast/universal/transport.rs` | Pass config |

#### 7.2.2 Extended Link Selection

```rust
// In tx.rs

#[cfg(feature = "transport_oam")]
fn select_with_quality(
    links: &[TransportLinkUnicastUniversal],
    reliability: Reliability,
    priority: Priority,
    scoring_config: &ScoringConfig,
) -> Option<usize> {
    // Step 1: Apply existing tier matching (full/partial/any)
    let candidates = Self::filter_by_tier(links, reliability, priority);
    
    if candidates.is_empty() {
        return None;
    }
    
    // Step 2: Get priority-specific scoring weights
    let weights = scoring_config.weights_for_priority(priority);
    
    // Step 3: Score each candidate
    let scored: Vec<(usize, f64)> = candidates
        .iter()
        .map(|&idx| {
            let metrics = links[idx].quality_metrics.read();
            let score = match scoring_config.mode {
                ScoringMode::Absolute => metrics.score(weights),
                ScoringMode::Normalized => metrics.normalized_score(weights),
            };
            (idx, score)
        })
        .collect();
    
    // Step 4: Apply hysteresis (prefer current link if close)
    let current_link = /* get current preferred link */;
    let best = scored.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    
    if let (Some(current), Some((best_idx, best_score))) = (current_link, best) {
        let current_score = scored.iter().find(|(i, _)| *i == current).map(|(_, s)| *s);
        if let Some(cs) = current_score {
            // Only switch if improvement exceeds hysteresis
            if *best_score - cs < scoring_config.hysteresis {
                return Some(current);
            }
        }
    }
    
    best.map(|(idx, _)| *idx)
}
```

#### 7.2.3 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Add quality_metrics to link struct | 30 | 2 | Arc<RwLock<LinkQualityMetrics>> |
| Implement filter_by_tier() | 60 | 3 | Extract existing logic |
| Implement select_with_quality() | 120 | 5 | Scoring integration |
| Implement priority-aware weights | 60 | 3 | Lookup by priority |
| Implement hysteresis | 50 | 3 | Prevent flapping |
| Implement selection strategies | 100 | 4 | Multiple strategies |
| Pass OAM config through manager | 40 | 2 | Config propagation |
| Unit tests | 150 | 6 | All selection paths |
| **Week 6 Total** | **610** | **28** | |

### 7.3 Week 7: KeepAlive Replacement & Lifecycle

#### 7.3.1 KeepAlive → OAM Migration

```rust
// In link.rs tx_task()

#[cfg(feature = "transport_oam")]
let (liveness_interval, use_oam) = if let Some(oam_config) = &oam_config {
    if oam_config.enabled {
        // OAM enabled: use probe interval, OAM prober handles liveness
        (Duration::from_millis(oam_config.probe_interval_ms), true)
    } else {
        (keep_alive, false)
    }
} else {
    (keep_alive, false)
};

#[cfg(not(feature = "transport_oam"))]
let (liveness_interval, use_oam) = (keep_alive, false);

loop {
    tokio::select! {
        res = tokio::time::timeout(liveness_interval, pipeline.pull()) => {
            match res {
                Ok(Some((batch, _))) => { /* send batch */ }
                Ok(None) => break,
                Err(_) => {
                    // Timeout: send liveness message
                    if !use_oam {
                        // OAM disabled: send KeepAlive
                        let msg: TransportMessage = KeepAlive.into();
                        link.send(&msg).await?;
                    }
                    // OAM enabled: prober task handles probes
                }
            }
        }
        _ = token.cancelled() => break,
    }
}
```

#### 7.3.2 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Modify TX task for OAM interval | 60 | 3 | Conditional timeout |
| Skip KeepAlive when OAM enabled | 40 | 2 | Conditional send |
| Compute lease from OAM config | 40 | 2 | lease = 4 * probe_interval |
| Start prober on link open | 60 | 3 | Spawn in transport.rs |
| Stop prober on link close | 40 | 2 | Cancel token |
| Link failure detection via OAM | 80 | 4 | Consecutive failures |
| Graceful degradation (OAM disabled) | 40 | 2 | Fallback to KeepAlive |
| Integration tests | 150 | 6 | Both paths |
| **Week 7 Total** | **510** | **24** | |

### 7.4 Phase 3 Summary

| Metric | Value |
|--------|-------|
| Duration | 2 weeks |
| Total LOC | ~1,120 |
| Files Modified | 4-5 |

**Exit Criteria:**
- [ ] Link selection uses OAM quality scores
- [ ] Priority-aware scoring works
- [ ] KeepAlive disabled when OAM enabled
- [ ] Hysteresis prevents link flapping
- [ ] No regression in existing tests
- [ ] Link failure detected via OAM timeout

---

## 8. Phase 4: Modem Metrics Integration (Weeks 8-10) — OPTIONAL

> **Note**: This phase is **optional** and requires the `transport_oam_modem` feature flag.
> Core OAM functionality (Phases 1-3) works without modem integration.

### 8.1 Objectives

- Define modem metrics types (matching ICD)
- Implement ModemMetricsProvider trait
- Implement Unix socket provider
- Combine OAM and modem metrics in scoring

### 8.2 Module Structure

```
io/zenoh-transport/src/unicast/oam/modem/    # Only with transport_oam_modem
├── mod.rs              # Module entry, feature-gated
├── types.rs            # ModemMetrics, ModemState, etc.
├── provider.rs         # ModemMetricsProvider trait
└── unix_socket.rs      # Unix socket implementation
```

### 8.3 Week 8: Modem Provider Infrastructure

#### 8.3.1 Type Definitions (from ICD)

```rust
#[cfg(feature = "transport_oam_modem")]
pub mod modem {
    /// Modem metrics from physical layer
    #[derive(Debug, Clone, Default)]
    pub struct ModemMetrics {
        pub signal: SignalMetrics,
        pub errors: ErrorMetrics,
        pub bandwidth: BandwidthMetrics,
        pub satellite: Option<SatelliteMetrics>,
        pub radio: Option<RadioMetrics>,
        pub acoustic: Option<AcousticMetrics>,
        pub state: ModemState,
        pub last_updated: Instant,
    }
    
    /// Provider trait for modem metrics
    #[async_trait]
    pub trait ModemMetricsProvider: Send + Sync {
        async fn get_metrics(&self) -> ZResult<ModemMetrics>;
        fn modem_type(&self) -> &str;
        async fn subscribe(&self) -> Option<broadcast::Receiver<ModemMetrics>>;
    }
    
    /// Factory for creating providers
    pub fn create_provider(config: &ModemProviderConf) -> ZResult<Box<dyn ModemMetricsProvider>>;
}
```

#### 8.3.2 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Create modem module structure | 40 | 2 | mod.rs with feature gate |
| Define ModemMetrics struct | 200 | 6 | All fields from ICD |
| Define ModemState enum | 40 | 2 | State machine |
| Define SignalMetrics, ErrorMetrics, etc. | 150 | 5 | Nested structs |
| Define ModemMetricsProvider trait | 60 | 3 | Async trait |
| Implement provider factory | 60 | 3 | Match on provider type |
| Unit tests for types | 100 | 4 | Serialization, defaults |
| **Week 8 Total** | **650** | **25** | |

### 8.4 Week 9: Unix Socket Provider

#### 8.4.1 Implementation

```rust
#[cfg(feature = "transport_oam_modem")]
pub struct UnixSocketModemProvider {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
    metrics: Arc<RwLock<ModemMetrics>>,
    reconnect_delay: Duration,
}

impl UnixSocketModemProvider {
    pub async fn connect(socket_path: &Path) -> ZResult<Self>;
    
    async fn read_messages(&mut self) -> ZResult<()>;
    
    async fn send_request(&mut self, request: Request) -> ZResult<Response>;
    
    async fn reconnect(&mut self) -> ZResult<()>;
}

#[async_trait]
impl ModemMetricsProvider for UnixSocketModemProvider {
    async fn get_metrics(&self) -> ZResult<ModemMetrics> {
        Ok(self.metrics.read().clone())
    }
    
    fn modem_type(&self) -> &str {
        "unix_socket"
    }
    
    async fn subscribe(&self) -> Option<broadcast::Receiver<ModemMetrics>> {
        // Return channel that receives updates
    }
}
```

#### 8.4.2 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Implement socket connection | 80 | 4 | Connect, handle errors |
| Implement message framing | 60 | 3 | Length-prefixed |
| Implement MessagePack serialization | 40 | 2 | Using rmp_serde |
| Implement metrics polling task | 100 | 4 | Async loop |
| Implement alert handling | 80 | 4 | Route to handler |
| Implement request/response | 100 | 4 | TrafficHint, SetMode |
| Implement reconnection logic | 80 | 4 | Exponential backoff |
| Unit tests with mock socket | 150 | 6 | All message types |
| **Week 9 Total** | **690** | **31** | |

### 8.5 Week 10: Combined Scoring & Integration

#### 8.5.1 Combined Scorer

```rust
#[cfg(feature = "transport_oam_modem")]
impl LinkQualityMetrics {
    /// Calculate score combining OAM probes and modem metrics
    pub fn combined_score(
        &self,
        modem: &ModemMetrics,
        weights: &CombinedScoringWeights,
    ) -> f64 {
        let oam_score = self.normalized_score(&weights.oam);
        
        let modem_score = calculate_modem_score(modem, &weights.modem);
        
        // Weighted combination
        let combined = oam_score * weights.oam_weight 
                     + modem_score * weights.modem_weight;
        
        // Modem state can veto (failed modem = 0 score)
        if modem.state == ModemState::Error || modem.state == ModemState::Disconnected {
            return 0.0;
        }
        
        combined
    }
}
```

#### 8.5.2 Tasks

| Task | Est. LOC | Est. Hours | Notes |
|------|----------|------------|-------|
| Implement combined_score() | 120 | 5 | OAM + modem |
| Implement modem score calculation | 100 | 4 | SNR, BER, etc. |
| Add modem config parsing | 80 | 3 | ModemMetricsConf |
| Wire provider to link lifecycle | 80 | 4 | Start/stop with link |
| Implement predictive quality | 100 | 4 | Trend detection |
| Implement alert-triggered actions | 80 | 4 | Mode changes |
| Integration tests | 200 | 8 | Full stack |
| **Week 10 Total** | **760** | **32** | |

### 8.6 Phase 4 Summary

| Metric | Value |
|--------|-------|
| Duration | 3 weeks |
| Total LOC | ~2,100 |
| Files Created | 4 |
| Files Modified | 2-3 |
| Feature Flag | `transport_oam_modem` |

**Exit Criteria:**
- [ ] Unix socket provider connects to mock modem
- [ ] Modem metrics affect combined score
- [ ] Alerts trigger appropriate responses
- [ ] Bidirectional communication works
- [ ] Reconnection works after disconnect

---

## 9. Phase 5: Testing, Documentation & Polish (Weeks 11-14)

### 9.1 Objectives

- Comprehensive test coverage
- Performance benchmarking
- User and developer documentation
- Example modem driver

### 9.2 Week 11: Unit & Integration Testing

#### 9.2.1 Test Files

```
io/zenoh-transport/tests/
├── unicast_oam.rs              # OAM probe tests
├── unicast_link_quality.rs     # Scoring and selection tests
└── unicast_modem.rs            # Modem provider tests (optional)
```

#### 9.2.2 Test Cases

| Category | Test Cases | Est. LOC |
|----------|------------|----------|
| OAM Probes | LBM/LBR round-trip, DMM/DMR timestamps, SLM/SLR counters | 200 |
| Metrics | RTT calculation, jitter (RFC 3550), loss ratio, state transitions | 150 |
| Scoring | Absolute, normalized, priority-aware, hysteresis | 150 |
| Selection | Multi-link, degraded links, failed links, priority routing | 200 |
| Modem (opt) | Unix socket, reconnection, combined scoring | 150 |
| Failure | Timeout handling, link failure, recovery | 150 |
| **Total** | | **1,000** |

### 9.3 Week 12: Performance & Stress Testing

#### 9.3.1 Benchmarks

| Benchmark | Target | Measurement |
|-----------|--------|-------------|
| Probe overhead | <0.1% CPU at 100ms interval | CPU profiling |
| Scoring latency | <10μs per score() call | Criterion benchmark |
| Lock contention | No blocking on metrics read | Lock analysis |
| Memory usage | <1KB per link for metrics | Memory profiling |
| Many-links scaling | Linear up to 100 links | Scaling test |

#### 9.3.2 Tasks

| Task | Est. Hours |
|------|------------|
| Create benchmark suite | 6 |
| Run and analyze benchmarks | 4 |
| Profile memory usage | 4 |
| Analyze lock contention | 4 |
| Optimize hot paths | 8 |
| Document performance | 4 |
| **Week 12 Total** | **30** |

### 9.4 Week 13: Documentation & Examples

#### 9.4.1 Documentation

| Document | Purpose | Est. LOC |
|----------|---------|----------|
| User Guide | Configuration, presets, tuning | 300 |
| Developer Guide | Extending, custom providers | 200 |
| API Documentation | Inline rustdoc | 150 |
| Example Modem Driver | Mock implementation | 400 |

#### 9.4.2 Tasks

| Task | Est. Hours |
|------|------------|
| Write user guide | 8 |
| Write developer guide | 6 |
| Complete inline documentation | 4 |
| Create example modem driver | 10 |
| Update CLAUDE.md | 1 |
| Update OAM_LINK_QUALITY_PLAN.md | 2 |
| **Week 13 Total** | **31** |

### 9.5 Week 14: Final Polish

#### 9.5.1 Tasks

| Task | Est. Hours |
|------|------------|
| Code review | 8 |
| Address review feedback | 8 |
| Final integration testing | 6 |
| Update CHANGELOG | 2 |
| Feature flag documentation | 2 |
| Release preparation | 4 |
| **Week 14 Total** | **30** |

### 9.6 Phase 5 Summary

| Metric | Value |
|--------|-------|
| Duration | 4 weeks |
| Test LOC | ~1,000 |
| Documentation | ~1,050 lines |
| Coverage Target | >80% for OAM module |

**Exit Criteria:**
- [ ] >80% code coverage
- [ ] No performance regression
- [ ] Documentation complete
- [ ] Example modem driver works
- [ ] All CI checks pass
- [ ] Ready for merge

---

## 10. Summary

### 10.1 Total Effort

| Phase | Weeks | LOC | Optional |
|-------|-------|-----|----------|
| 1. Protocol & Config | 2 | 910 | No |
| 2. Core OAM | 3 | 1,980 | No |
| 3. Link Selection | 2 | 1,120 | No |
| 4. Modem Integration | 3 | 2,100 | **Yes** |
| 5. Testing & Docs | 4 | 2,050 | Partially |
| **Core Only (no modem)** | **9** | **~5,000** | |
| **Full Implementation** | **14** | **~8,160** | |

### 10.2 Milestones

| Milestone | Week | Description |
|-----------|------|-------------|
| **M1** | 2 | Protocol and configuration ready |
| **M2** | 5 | OAM probes working between nodes |
| **M3** | 7 | Quality-based link selection working |
| **M4** | 10 | Modem integration complete (optional) |
| **M5** | 14 | Production ready |

### 10.3 Deployment Configurations

| Configuration | Features | Use Case |
|---------------|----------|----------|
| Standard Zenoh | (none) | No OAM, KeepAlive only |
| Core OAM | `transport_oam` | Quality routing, no external modems |
| Full OAM | `transport_oam` + `transport_oam_modem` | Quality routing + modem metrics |

### 10.4 Dependencies

```
                    ┌─────────────────────┐
                    │  Phase 1            │
                    │  Protocol & Config  │
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │  Phase 2        │ │  Phase 4        │ │                 │
    │  Core OAM       │ │  Modem (OPT)    │ │                 │
    └────────┬────────┘ └────────┬────────┘ │                 │
             │                   │          │                 │
             ▼                   │          │                 │
    ┌─────────────────┐          │          │                 │
    │  Phase 3        │          │          │                 │
    │  Link Selection │          │          │                 │
    └────────┬────────┘          │          │                 │
             │                   │          │                 │
             └───────────────────┼──────────┘                 │
                                 │                            │
                                 ▼                            │
                    ┌─────────────────────┐                   │
                    │  Phase 5            │◄──────────────────┘
                    │  Testing & Docs     │
                    └─────────────────────┘
```

### 10.5 Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Scope creep | Medium | High | Strict phase gates, MVP focus |
| Performance regression | Low | High | Early benchmarking, feature-gated |
| Breaking changes | Low | High | All new code behind feature flag |
| Modem driver complexity | Medium | Medium | Start with mock, real drivers later |
| Clock sync issues | Medium | Low | Default to RTT-only (no DMM) |
| Integration conflicts | Low | Medium | Incremental integration, extensive tests |

### 10.6 Success Criteria

**Core OAM (Tier 1):**
- [ ] RTT/jitter/loss measured accurately
- [ ] Quality scores affect link selection
- [ ] Priority-aware scoring works
- [ ] KeepAlive replaced when OAM enabled
- [ ] No regression in existing functionality
- [ ] Documentation complete

**Modem Integration (Tier 2, Optional):**
- [ ] Unix socket provider works
- [ ] Combined scoring functional
- [ ] Alerts handled appropriately
- [ ] Example modem driver provided

---

## Appendix A: File Change Summary

### New Files

```
commons/zenoh-protocol/src/transport/
└── oam/
    └── payload.rs                          # OAM payload types

commons/zenoh-config/src/
└── oam.rs                                  # OAM configuration types

io/zenoh-transport/src/unicast/oam/
├── mod.rs                                  # Module entry
├── config.rs                               # Runtime config
├── metrics.rs                              # LinkQualityMetrics
├── prober.rs                               # OAM prober task
├── responder.rs                            # OAM responder
├── scorer.rs                               # Scoring algorithms
└── modem/                                  # (optional)
    ├── mod.rs
    ├── types.rs
    ├── provider.rs
    └── unix_socket.rs

io/zenoh-transport/tests/
├── unicast_oam.rs
├── unicast_link_quality.rs
└── unicast_modem.rs                        # (optional)

examples/
└── mock_modem_driver/                      # (optional)
    ├── Cargo.toml
    └── src/main.rs
```

### Modified Files

```
commons/zenoh-protocol/src/transport/
├── oam.rs                                  # Add ID constants
└── mod.rs                                  # Re-exports

commons/zenoh-config/src/
├── lib.rs                                  # Add OamConf
└── defaults.rs                             # Add OAM defaults

io/zenoh-transport/
├── Cargo.toml                              # Feature flags
└── src/unicast/
    ├── mod.rs                              # Add oam module
    ├── manager.rs                          # OAM config handling
    └── universal/
        ├── tx.rs                           # Link selection
        ├── rx.rs                           # OAM message routing
        ├── link.rs                         # Metrics field, TX task
        └── transport.rs                    # Prober lifecycle

zenoh/Cargo.toml                            # Feature flag exposure
```

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| **OAM** | Operations, Administration, and Maintenance |
| **LBM/LBR** | Loopback Message/Reply (ping/pong for RTT) |
| **DMM/DMR** | Delay Measurement Message/Reply (4-timestamp) |
| **SLM/SLR** | Synthetic Loss Measurement Message/Reply |
| **RTT** | Round-Trip Time |
| **Jitter** | Variation in packet delay |
| **Hysteresis** | Minimum score difference to trigger link switch |
| **Baseline** | Expected "good" metrics for a link type |
| **Normalized Score** | Score relative to link type baseline |

---

## Appendix C: References

- [OAM_LINK_QUALITY_PLAN.md](OAM_LINK_QUALITY_PLAN.md) - Detailed design document
- [MODEM_DRIVER_ICD.md](MODEM_DRIVER_ICD.md) - Modem interface specification
- [OAM_CONFIG_EXAMPLE.json5](OAM_CONFIG_EXAMPLE.json5) - Example configuration
- [modem_driver_types.rs](modem_driver_types.rs) - Rust type definitions
- ITU-T G.8013/Y.1731 - OAM functions for Ethernet networks
- RFC 3550 - RTP jitter calculation
