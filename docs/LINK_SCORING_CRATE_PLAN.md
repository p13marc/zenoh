# zenoh-link-scoring Crate Plan

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Draft |
| Date | 2024-12-13 |
| Related | LINK_QUALITY_SCORING.md, OAM_EXTERNAL_CONTROLLER_ARCHITECTURE.md |

---

## 1. Overview

This document describes the plan for `zenoh-link-scoring`, a standalone Rust crate providing link quality scoring algorithms with a C-compatible API.

### 1.1 Goals

1. **Standalone** - No dependency on Zenoh internals
2. **Reusable** - Usable by Zenoh, external controllers, or any other system
3. **C-compatible** - FFI bindings for C/C++ integration
4. **Well-tested** - Comprehensive unit and property-based tests
5. **Documented** - Full API documentation with examples

### 1.2 Use Cases

| Consumer | Language | Usage |
|----------|----------|-------|
| Zenoh (optional fallback) | Rust | Direct dependency |
| External Controller (Rust) | Rust | Direct dependency |
| External Controller (C/C++) | C | FFI via `libzenoh_link_scoring` |
| External Controller (Python) | Python | Via C bindings or PyO3 (future) |
| Modem Driver | C/C++ | FFI for consistent scoring |

---

## 2. Architecture

### 2.1 Crate Structure

```
zenoh-link-scoring/
├── Cargo.toml
├── cbindgen.toml                 # C header generation config
├── README.md
├── LICENSE
│
├── src/
│   ├── lib.rs                    # Crate root, public API
│   │
│   ├── metrics/
│   │   ├── mod.rs
│   │   ├── oam.rs                # OAM metrics (RTT, jitter, loss)
│   │   ├── modem.rs              # Modem metrics (SNR, BER, state)
│   │   └── combined.rs           # Combined metrics view
│   │
│   ├── scoring/
│   │   ├── mod.rs
│   │   ├── weights.rs            # Scoring weight configurations
│   │   ├── absolute.rs           # Absolute scoring algorithm
│   │   ├── normalized.rs         # Normalized scoring algorithm
│   │   └── priority.rs           # Priority-aware weight selection
│   │
│   ├── baselines/
│   │   ├── mod.rs
│   │   ├── presets.rs            # Built-in link type baselines
│   │   └── custom.rs             # Custom baseline builder
│   │
│   ├── selection/
│   │   ├── mod.rs
│   │   ├── selector.rs           # Link selection with hysteresis
│   │   ├── failover.rs           # Failure detection logic
│   │   └── state.rs              # Link state tracking
│   │
│   └── ffi/
│       ├── mod.rs                # FFI module root
│       ├── types.rs              # C-compatible type definitions
│       ├── metrics.rs            # Metrics FFI functions
│       ├── scoring.rs            # Scoring FFI functions
│       ├── selection.rs          # Selection FFI functions
│       └── error.rs              # Error handling for FFI
│
├── include/
│   └── zenoh_link_scoring.h      # Generated C header (by cbindgen)
│
├── tests/
│   ├── scoring_tests.rs
│   ├── selection_tests.rs
│   └── ffi_tests.rs
│
├── benches/
│   └── scoring_bench.rs
│
└── examples/
    ├── basic_scoring.rs
    ├── link_selection.rs
    └── c_example/
        ├── Makefile
        └── main.c
```

### 2.2 Feature Flags

```toml
[features]
default = ["std"]

# Standard library support (default)
std = []

# FFI/C bindings
ffi = ["std"]

# no_std support for embedded (future)
# no_std = []

# Serde serialization for configs
serde = ["dep:serde"]

# Additional validation
strict = []
```

### 2.3 Dependencies

```toml
[dependencies]
# No required dependencies for core functionality

# Optional
serde = { version = "1.0", features = ["derive"], optional = true }

[build-dependencies]
cbindgen = "0.26"  # For C header generation

[dev-dependencies]
criterion = "0.5"  # Benchmarks
proptest = "1.0"   # Property-based testing
```

---

## 3. Rust API

### 3.1 Metrics Types

```rust
// src/metrics/oam.rs

use core::time::Duration;

/// OAM probe metrics for a single link
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct OamMetrics {
    /// Current RTT
    pub rtt_current_us: u64,
    /// Minimum observed RTT
    pub rtt_min_us: u64,
    /// Maximum observed RTT
    pub rtt_max_us: u64,
    /// Exponential moving average RTT
    pub rtt_avg_us: u64,
    
    /// Jitter (RFC 3550)
    pub jitter_us: u64,
    
    /// Forward delay (requires clock sync, 0 if unavailable)
    pub forward_delay_us: u64,
    /// Reverse delay (requires clock sync, 0 if unavailable)
    pub reverse_delay_us: u64,
    /// Whether one-way delay measurements are valid
    pub one_way_valid: bool,
    
    /// Total probes sent
    pub tx_count: u64,
    /// Total replies received
    pub rx_count: u64,
    
    /// Consecutive probe failures
    pub consecutive_failures: u32,
    
    /// Timestamp of last successful probe (Unix epoch microseconds)
    pub last_success_us: u64,
}

impl OamMetrics {
    /// Calculate loss ratio (0.0 to 1.0)
    pub fn loss_ratio(&self) -> f64 {
        if self.tx_count == 0 {
            return 0.0;
        }
        1.0 - (self.rx_count as f64 / self.tx_count as f64)
    }
    
    /// Calculate asymmetry ratio (0.0 = symmetric, 1.0 = fully asymmetric)
    pub fn asymmetry_ratio(&self) -> Option<f64> {
        if !self.one_way_valid || self.forward_delay_us == 0 || self.reverse_delay_us == 0 {
            return None;
        }
        let fwd = self.forward_delay_us as f64;
        let rev = self.reverse_delay_us as f64;
        let diff = (fwd - rev).abs();
        let sum = fwd + rev;
        Some(diff / sum)
    }
    
    /// Check if link appears alive based on recent activity
    pub fn is_alive(&self, max_failures: u32) -> bool {
        self.consecutive_failures < max_failures
    }
    
    /// Get RTT as Duration
    pub fn rtt_avg(&self) -> Duration {
        Duration::from_micros(self.rtt_avg_us)
    }
    
    /// Get jitter as Duration
    pub fn jitter(&self) -> Duration {
        Duration::from_micros(self.jitter_us)
    }
}
```

```rust
// src/metrics/modem.rs

/// Modem operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub enum ModemState {
    #[default]
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
}

/// Modem/physical layer metrics
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct ModemMetrics {
    /// Signal-to-Noise Ratio in centidecibels (dB * 100)
    /// Example: 2500 = 25.00 dB
    pub snr_cdb: i32,
    /// Whether SNR is valid
    pub snr_valid: bool,
    
    /// Received Signal Strength in centi-dBm (dBm * 100)
    /// Example: -7000 = -70.00 dBm
    pub rssi_cdbm: i32,
    /// Whether RSSI is valid
    pub rssi_valid: bool,
    
    /// Bit Error Rate exponent (BER = 10^(-exponent))
    /// Example: 6 = BER of 10^-6
    pub ber_exponent: i8,
    /// Whether BER is valid
    pub ber_valid: bool,
    
    /// Available bandwidth in bits per second
    pub available_bps: u64,
    /// Maximum bandwidth in bits per second
    pub max_bps: u64,
    
    /// Current modem state
    pub state: ModemState,
    
    /// Time in current state (microseconds)
    pub state_duration_us: u64,
    
    /// Handover expected within this time (microseconds, 0 if not applicable)
    pub handover_eta_us: u64,
}

impl ModemMetrics {
    /// Get SNR in dB
    pub fn snr_db(&self) -> Option<f64> {
        if self.snr_valid {
            Some(self.snr_cdb as f64 / 100.0)
        } else {
            None
        }
    }
    
    /// Get RSSI in dBm
    pub fn rssi_dbm(&self) -> Option<f64> {
        if self.rssi_valid {
            Some(self.rssi_cdbm as f64 / 100.0)
        } else {
            None
        }
    }
    
    /// Get BER as f64
    pub fn ber(&self) -> Option<f64> {
        if self.ber_valid {
            Some(10.0_f64.powi(-(self.ber_exponent as i32)))
        } else {
            None
        }
    }
    
    /// Get bandwidth utilization ratio (0.0 to 1.0)
    pub fn bandwidth_ratio(&self) -> f64 {
        if self.max_bps == 0 {
            return 0.0;
        }
        self.available_bps as f64 / self.max_bps as f64
    }
    
    /// Check if modem is in an operational state
    pub fn is_operational(&self) -> bool {
        matches!(self.state, ModemState::Connected | ModemState::Degraded)
    }
}
```

### 3.2 Scoring Weights

```rust
// src/scoring/weights.rs

/// Scoring weight configuration
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct ScoringWeights {
    /// Penalty per millisecond of RTT
    pub rtt: f64,
    /// Penalty per millisecond of jitter
    pub jitter: f64,
    /// Penalty per percentage point of packet loss
    pub loss: f64,
    /// Penalty per dB below SNR baseline
    pub snr: f64,
    /// Penalty per BER decade above baseline
    pub ber: f64,
    /// Weight for bandwidth contribution (bonus)
    pub bandwidth: f64,
    /// Weight multiplier for modem state penalty
    pub state: f64,
    /// Penalty for delay asymmetry (per 0.1 asymmetry ratio)
    pub asymmetry: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            rtt: 0.5,
            jitter: 2.0,
            loss: 10.0,
            snr: 1.0,
            ber: 5.0,
            bandwidth: 0.1,
            state: 1.0,
            asymmetry: 5.0,
        }
    }
}

impl ScoringWeights {
    /// Weights optimized for real-time traffic (voice, video)
    pub fn realtime() -> Self {
        Self {
            rtt: 1.0,
            jitter: 5.0,
            loss: 20.0,
            snr: 2.0,
            ber: 10.0,
            bandwidth: 0.05,
            state: 2.0,
            asymmetry: 3.0,
        }
    }
    
    /// Weights optimized for interactive traffic
    pub fn interactive() -> Self {
        Self {
            rtt: 2.0,
            jitter: 2.0,
            loss: 10.0,
            snr: 1.0,
            ber: 5.0,
            bandwidth: 0.1,
            state: 1.0,
            asymmetry: 5.0,
        }
    }
    
    /// Weights optimized for bulk data transfer
    pub fn bulk_transfer() -> Self {
        Self {
            rtt: 0.1,
            jitter: 0.2,
            loss: 15.0,
            snr: 0.5,
            ber: 3.0,
            bandwidth: 2.0,
            state: 0.5,
            asymmetry: 2.0,
        }
    }
    
    /// Weights for tactical/harsh environments
    pub fn tactical() -> Self {
        Self {
            rtt: 0.2,
            jitter: 0.5,
            loss: 3.0,
            snr: 3.0,
            ber: 2.0,
            bandwidth: 0.5,
            state: 5.0,
            asymmetry: 1.0,
        }
    }
}

/// Traffic priority levels (matches Zenoh priorities)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Priority {
    Control = 0,
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl ScoringWeights {
    /// Get appropriate weights for a given priority
    pub fn for_priority(priority: Priority) -> Self {
        match priority {
            Priority::Control | Priority::RealTime => Self::realtime(),
            Priority::InteractiveHigh | Priority::InteractiveLow => Self::interactive(),
            Priority::DataHigh | Priority::Data | Priority::DataLow => Self::bulk_transfer(),
            Priority::Background => Self {
                rtt: 0.1,
                jitter: 0.1,
                loss: 5.0,
                bandwidth: 0.5,
                ..Self::default()
            },
        }
    }
}
```

### 3.3 Baselines

```rust
// src/baselines/presets.rs

/// Expected metrics for a specific link type
/// Used for normalized scoring
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct LinkTypeBaseline {
    /// Link type identifier
    pub link_type: LinkType,
    
    /// Typical RTT for healthy link (microseconds)
    pub typical_rtt_us: u64,
    /// Maximum acceptable RTT (microseconds)
    pub max_rtt_us: u64,
    
    /// Typical jitter (microseconds)
    pub typical_jitter_us: u64,
    /// Maximum acceptable jitter (microseconds)
    pub max_jitter_us: u64,
    
    /// Typical loss ratio (0.0 to 1.0)
    pub typical_loss: f64,
    /// Maximum acceptable loss ratio
    pub max_loss: f64,
    
    /// Expected SNR baseline (centidecibels)
    pub snr_baseline_cdb: i32,
    
    /// Typical bandwidth (bps)
    pub typical_bandwidth_bps: u64,
}

/// Link type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum LinkType {
    /// Unknown or custom link type
    Unknown = 0,
    /// Fiber optic
    Fiber = 1,
    /// Ethernet LAN
    EthernetLan = 2,
    /// Ethernet WAN / Internet
    EthernetWan = 3,
    /// Geostationary satellite
    SatelliteGeo = 4,
    /// Low Earth Orbit satellite
    SatelliteLeo = 5,
    /// Tactical radio
    RadioTactical = 6,
    /// Mobile/cellular radio
    RadioMobile = 7,
    /// Underwater acoustic
    Acoustic = 8,
}

impl LinkTypeBaseline {
    pub fn fiber() -> Self {
        Self {
            link_type: LinkType::Fiber,
            typical_rtt_us: 500,
            max_rtt_us: 5_000,
            typical_jitter_us: 50,
            max_jitter_us: 500,
            typical_loss: 0.0,
            max_loss: 0.0001,
            snr_baseline_cdb: 3000, // 30 dB
            typical_bandwidth_bps: 10_000_000_000,
        }
    }
    
    pub fn ethernet_lan() -> Self {
        Self {
            link_type: LinkType::EthernetLan,
            typical_rtt_us: 1_000,
            max_rtt_us: 10_000,
            typical_jitter_us: 200,
            max_jitter_us: 2_000,
            typical_loss: 0.0,
            max_loss: 0.001,
            snr_baseline_cdb: 2500,
            typical_bandwidth_bps: 1_000_000_000,
        }
    }
    
    pub fn ethernet_wan() -> Self {
        Self {
            link_type: LinkType::EthernetWan,
            typical_rtt_us: 50_000,
            max_rtt_us: 200_000,
            typical_jitter_us: 5_000,
            max_jitter_us: 30_000,
            typical_loss: 0.001,
            max_loss: 0.02,
            snr_baseline_cdb: 2000,
            typical_bandwidth_bps: 100_000_000,
        }
    }
    
    pub fn satellite_geo() -> Self {
        Self {
            link_type: LinkType::SatelliteGeo,
            typical_rtt_us: 600_000,
            max_rtt_us: 800_000,
            typical_jitter_us: 10_000,
            max_jitter_us: 50_000,
            typical_loss: 0.01,
            max_loss: 0.05,
            snr_baseline_cdb: 1000,
            typical_bandwidth_bps: 20_000_000,
        }
    }
    
    pub fn satellite_leo() -> Self {
        Self {
            link_type: LinkType::SatelliteLeo,
            typical_rtt_us: 40_000,
            max_rtt_us: 100_000,
            typical_jitter_us: 10_000,
            max_jitter_us: 30_000,
            typical_loss: 0.005,
            max_loss: 0.03,
            snr_baseline_cdb: 1200,
            typical_bandwidth_bps: 100_000_000,
        }
    }
    
    pub fn radio_tactical() -> Self {
        Self {
            link_type: LinkType::RadioTactical,
            typical_rtt_us: 100_000,
            max_rtt_us: 500_000,
            typical_jitter_us: 30_000,
            max_jitter_us: 100_000,
            typical_loss: 0.02,
            max_loss: 0.10,
            snr_baseline_cdb: 1500,
            typical_bandwidth_bps: 256_000,
        }
    }
    
    pub fn radio_mobile() -> Self {
        Self {
            link_type: LinkType::RadioMobile,
            typical_rtt_us: 50_000,
            max_rtt_us: 150_000,
            typical_jitter_us: 20_000,
            max_jitter_us: 80_000,
            typical_loss: 0.01,
            max_loss: 0.05,
            snr_baseline_cdb: 1500,
            typical_bandwidth_bps: 10_000_000,
        }
    }
    
    pub fn acoustic() -> Self {
        Self {
            link_type: LinkType::Acoustic,
            typical_rtt_us: 2_000_000,
            max_rtt_us: 10_000_000,
            typical_jitter_us: 500_000,
            max_jitter_us: 2_000_000,
            typical_loss: 0.10,
            max_loss: 0.30,
            snr_baseline_cdb: 1000,
            typical_bandwidth_bps: 1_000,
        }
    }
    
    /// Get baseline for a link type
    pub fn for_type(link_type: LinkType) -> Self {
        match link_type {
            LinkType::Unknown => Self::ethernet_lan(), // Default fallback
            LinkType::Fiber => Self::fiber(),
            LinkType::EthernetLan => Self::ethernet_lan(),
            LinkType::EthernetWan => Self::ethernet_wan(),
            LinkType::SatelliteGeo => Self::satellite_geo(),
            LinkType::SatelliteLeo => Self::satellite_leo(),
            LinkType::RadioTactical => Self::radio_tactical(),
            LinkType::RadioMobile => Self::radio_mobile(),
            LinkType::Acoustic => Self::acoustic(),
        }
    }
}
```

### 3.4 Scoring Functions

```rust
// src/scoring/absolute.rs

use crate::metrics::{OamMetrics, ModemMetrics, ModemState};
use crate::scoring::ScoringWeights;

/// Calculate absolute score (0-100, higher is better)
/// 
/// # Arguments
/// * `oam` - OAM probe metrics
/// * `modem` - Optional modem metrics
/// * `weights` - Scoring weight configuration
/// 
/// # Returns
/// Score from 0.0 to 100.0
pub fn absolute_score(
    oam: &OamMetrics,
    modem: Option<&ModemMetrics>,
    weights: &ScoringWeights,
) -> f64 {
    let mut score = 100.0;
    
    // === OAM Penalties ===
    
    // RTT penalty (convert us to ms)
    let rtt_ms = oam.rtt_avg_us as f64 / 1000.0;
    score -= rtt_ms * weights.rtt;
    
    // Jitter penalty (convert us to ms)
    let jitter_ms = oam.jitter_us as f64 / 1000.0;
    score -= jitter_ms * weights.jitter;
    
    // Loss penalty (percentage)
    let loss_pct = oam.loss_ratio() * 100.0;
    score -= loss_pct * weights.loss;
    
    // Asymmetry penalty
    if let Some(asymmetry) = oam.asymmetry_ratio() {
        score -= asymmetry * 10.0 * weights.asymmetry;
    }
    
    // === Modem Penalties ===
    
    if let Some(m) = modem {
        // SNR penalty
        if let Some(snr_db) = m.snr_db() {
            let snr_baseline = 20.0;
            if snr_db < snr_baseline {
                score -= (snr_baseline - snr_db) * weights.snr;
            }
        }
        
        // BER penalty
        if let Some(ber) = m.ber() {
            if ber > 0.0 {
                let exp = ber.log10();
                let baseline_exp = -6.0;
                if exp > baseline_exp {
                    score -= (exp - baseline_exp) * weights.ber;
                }
            }
        }
        
        // Bandwidth bonus
        let bw_ratio = m.bandwidth_ratio();
        score += bw_ratio * 10.0 * weights.bandwidth;
        
        // State penalty
        let state_penalty = modem_state_penalty(m.state);
        score -= state_penalty * weights.state;
        
        // Handover penalty
        if m.handover_eta_us > 0 && m.handover_eta_us < 30_000_000 {
            // Handover within 30 seconds
            score -= 10.0;
        }
    }
    
    score.clamp(0.0, 100.0)
}

/// Get penalty for modem state
fn modem_state_penalty(state: ModemState) -> f64 {
    match state {
        ModemState::Connected => 0.0,
        ModemState::Degraded => 15.0,
        ModemState::HandoverInProgress => 25.0,
        ModemState::Synchronizing => 30.0,
        ModemState::Searching => 50.0,
        ModemState::Initializing => 40.0,
        ModemState::Standby => 35.0,
        ModemState::Disconnected => 100.0,
        ModemState::Error => 100.0,
        ModemState::Unknown => 10.0,
    }
}

/// Calculate score with only OAM metrics (convenience function)
pub fn score_oam_only(oam: &OamMetrics, weights: &ScoringWeights) -> f64 {
    absolute_score(oam, None, weights)
}
```

```rust
// src/scoring/normalized.rs

use crate::metrics::OamMetrics;
use crate::baselines::LinkTypeBaseline;
use crate::scoring::ScoringWeights;

/// Calculate normalized score relative to link type baseline
/// 
/// This scoring method compares metrics against what's expected for the link type,
/// allowing fair comparison between different link types (e.g., fiber vs satellite).
/// 
/// # Arguments
/// * `oam` - OAM probe metrics
/// * `baseline` - Expected metrics for this link type
/// * `weights` - Scoring weight configuration
/// 
/// # Returns
/// Score from 0.0 to 100.0
pub fn normalized_score(
    oam: &OamMetrics,
    baseline: &LinkTypeBaseline,
    weights: &ScoringWeights,
) -> f64 {
    let mut score = 100.0;
    
    // RTT: penalize deviation from typical
    if baseline.typical_rtt_us > 0 {
        let rtt_ratio = oam.rtt_avg_us as f64 / baseline.typical_rtt_us as f64;
        if rtt_ratio > 1.0 {
            // Penalty for being worse than typical
            score -= (rtt_ratio - 1.0) * 20.0 * weights.rtt;
        } else {
            // Small bonus for being better than typical
            score += (1.0 - rtt_ratio) * 5.0 * weights.rtt;
        }
    }
    
    // Jitter: penalize deviation from typical
    if baseline.typical_jitter_us > 0 {
        let jitter_ratio = oam.jitter_us as f64 / baseline.typical_jitter_us as f64;
        if jitter_ratio > 1.0 {
            score -= (jitter_ratio - 1.0) * 15.0 * weights.jitter;
        }
    }
    
    // Loss: penalize excess over typical
    let loss = oam.loss_ratio();
    let loss_excess = loss - baseline.typical_loss;
    if loss_excess > 0.0 {
        score -= loss_excess * 100.0 * weights.loss;
    }
    
    // Severely penalize if exceeding max thresholds
    if oam.rtt_avg_us > baseline.max_rtt_us {
        score -= 20.0;
    }
    if oam.jitter_us > baseline.max_jitter_us {
        score -= 15.0;
    }
    if loss > baseline.max_loss {
        score -= 25.0;
    }
    
    score.clamp(0.0, 100.0)
}
```

### 3.5 Link Selection

```rust
// src/selection/selector.rs

use crate::metrics::{OamMetrics, ModemMetrics};
use crate::scoring::{ScoringWeights, absolute_score};

/// Link identifier (opaque to the scoring library)
pub type LinkId = u64;

/// Link with its metrics
#[derive(Debug, Clone)]
pub struct ScoredLink {
    pub id: LinkId,
    pub oam: OamMetrics,
    pub modem: Option<ModemMetrics>,
    pub score: f64,
}

/// Link selector with hysteresis to prevent flapping
pub struct LinkSelector {
    /// Currently selected link
    current_link: Option<LinkId>,
    /// Minimum score improvement required to switch
    hysteresis: f64,
    /// Maximum consecutive failures before excluding link
    failure_threshold: u32,
}

impl LinkSelector {
    /// Create a new link selector
    pub fn new(hysteresis: f64, failure_threshold: u32) -> Self {
        Self {
            current_link: None,
            hysteresis,
            failure_threshold,
        }
    }
    
    /// Create with default settings
    pub fn with_defaults() -> Self {
        Self::new(5.0, 3)
    }
    
    /// Select the best link from available options
    /// 
    /// # Arguments
    /// * `links` - Available links with their metrics
    /// * `weights` - Scoring weights to use
    /// 
    /// # Returns
    /// The selected link ID, or None if no links are available
    pub fn select(
        &mut self,
        links: &[(LinkId, &OamMetrics, Option<&ModemMetrics>)],
        weights: &ScoringWeights,
    ) -> Option<LinkId> {
        // Filter and score links
        let scored: Vec<_> = links
            .iter()
            .filter(|(_, oam, _)| oam.is_alive(self.failure_threshold))
            .map(|(id, oam, modem)| {
                let score = absolute_score(oam, *modem, weights);
                (*id, score)
            })
            .collect();
        
        if scored.is_empty() {
            self.current_link = None;
            return None;
        }
        
        // Find best link
        let (best_id, best_score) = scored
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        
        // Apply hysteresis
        if let Some(current_id) = self.current_link {
            if let Some((_, current_score)) = scored.iter().find(|(id, _)| *id == current_id) {
                // Only switch if improvement exceeds hysteresis
                if best_score - current_score < self.hysteresis {
                    return Some(current_id);
                }
            }
        }
        
        // Switch to best link
        self.current_link = Some(*best_id);
        Some(*best_id)
    }
    
    /// Get currently selected link
    pub fn current(&self) -> Option<LinkId> {
        self.current_link
    }
    
    /// Reset selection (clears current link)
    pub fn reset(&mut self) {
        self.current_link = None;
    }
    
    /// Force selection of a specific link
    pub fn force_select(&mut self, link_id: LinkId) {
        self.current_link = Some(link_id);
    }
}

/// Score multiple links and return them sorted by score (best first)
pub fn rank_links(
    links: &[(LinkId, &OamMetrics, Option<&ModemMetrics>)],
    weights: &ScoringWeights,
) -> Vec<ScoredLink> {
    let mut scored: Vec<_> = links
        .iter()
        .map(|(id, oam, modem)| {
            let score = absolute_score(oam, *modem, weights);
            ScoredLink {
                id: *id,
                oam: (*oam).clone(),
                modem: modem.cloned(),
                score,
            }
        })
        .collect();
    
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored
}
```

---

## 4. C API (FFI)

### 4.1 Design Principles

1. **Opaque handles** for complex objects
2. **Error codes** instead of Result types
3. **Output parameters** for returning data
4. **Explicit memory management** (caller allocates/frees)
5. **No panics** - all errors returned as codes

### 4.2 Error Codes

```rust
// src/ffi/error.rs

/// FFI error codes
#[repr(C)]
pub enum ZlsError {
    /// Success
    Ok = 0,
    /// Null pointer provided
    NullPointer = 1,
    /// Invalid argument value
    InvalidArgument = 2,
    /// Buffer too small
    BufferTooSmall = 3,
    /// Invalid handle
    InvalidHandle = 4,
    /// Out of memory
    OutOfMemory = 5,
    /// Unknown error
    Unknown = 255,
}
```

### 4.3 FFI Types

```rust
// src/ffi/types.rs

use crate::metrics::{OamMetrics, ModemMetrics, ModemState};
use crate::scoring::ScoringWeights;
use crate::baselines::{LinkType, LinkTypeBaseline};

/// C-compatible OAM metrics (same layout as Rust struct)
#[repr(C)]
pub struct ZlsOamMetrics {
    pub rtt_current_us: u64,
    pub rtt_min_us: u64,
    pub rtt_max_us: u64,
    pub rtt_avg_us: u64,
    pub jitter_us: u64,
    pub forward_delay_us: u64,
    pub reverse_delay_us: u64,
    pub one_way_valid: bool,
    pub tx_count: u64,
    pub rx_count: u64,
    pub consecutive_failures: u32,
    pub last_success_us: u64,
}

/// C-compatible modem metrics
#[repr(C)]
pub struct ZlsModemMetrics {
    pub snr_cdb: i32,
    pub snr_valid: bool,
    pub rssi_cdbm: i32,
    pub rssi_valid: bool,
    pub ber_exponent: i8,
    pub ber_valid: bool,
    pub available_bps: u64,
    pub max_bps: u64,
    pub state: ModemState,
    pub state_duration_us: u64,
    pub handover_eta_us: u64,
}

/// C-compatible scoring weights
#[repr(C)]
pub struct ZlsScoringWeights {
    pub rtt: f64,
    pub jitter: f64,
    pub loss: f64,
    pub snr: f64,
    pub ber: f64,
    pub bandwidth: f64,
    pub state: f64,
    pub asymmetry: f64,
}

/// C-compatible baseline
#[repr(C)]
pub struct ZlsLinkTypeBaseline {
    pub link_type: LinkType,
    pub typical_rtt_us: u64,
    pub max_rtt_us: u64,
    pub typical_jitter_us: u64,
    pub max_jitter_us: u64,
    pub typical_loss: f64,
    pub max_loss: f64,
    pub snr_baseline_cdb: i32,
    pub typical_bandwidth_bps: u64,
}

/// Opaque handle for link selector
pub struct ZlsLinkSelector {
    inner: crate::selection::LinkSelector,
}

// Conversion implementations
impl From<&ZlsOamMetrics> for OamMetrics {
    fn from(c: &ZlsOamMetrics) -> Self {
        Self {
            rtt_current_us: c.rtt_current_us,
            rtt_min_us: c.rtt_min_us,
            rtt_max_us: c.rtt_max_us,
            rtt_avg_us: c.rtt_avg_us,
            jitter_us: c.jitter_us,
            forward_delay_us: c.forward_delay_us,
            reverse_delay_us: c.reverse_delay_us,
            one_way_valid: c.one_way_valid,
            tx_count: c.tx_count,
            rx_count: c.rx_count,
            consecutive_failures: c.consecutive_failures,
            last_success_us: c.last_success_us,
        }
    }
}

impl From<&ZlsScoringWeights> for ScoringWeights {
    fn from(c: &ZlsScoringWeights) -> Self {
        Self {
            rtt: c.rtt,
            jitter: c.jitter,
            loss: c.loss,
            snr: c.snr,
            ber: c.ber,
            bandwidth: c.bandwidth,
            state: c.state,
            asymmetry: c.asymmetry,
        }
    }
}
```

### 4.4 FFI Functions

```rust
// src/ffi/scoring.rs

use std::ptr;
use crate::ffi::error::ZlsError;
use crate::ffi::types::*;
use crate::scoring::{absolute_score, ScoringWeights};
use crate::baselines::LinkTypeBaseline;

/// Calculate absolute score from OAM metrics only
/// 
/// # Safety
/// - `oam` must be a valid pointer to ZlsOamMetrics
/// - `weights` must be a valid pointer to ZlsScoringWeights
/// - `score_out` must be a valid pointer to f64
#[no_mangle]
pub unsafe extern "C" fn zls_score_oam(
    oam: *const ZlsOamMetrics,
    weights: *const ZlsScoringWeights,
    score_out: *mut f64,
) -> ZlsError {
    if oam.is_null() || weights.is_null() || score_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let oam_rust = crate::metrics::OamMetrics::from(&*oam);
    let weights_rust = ScoringWeights::from(&*weights);
    
    let score = absolute_score(&oam_rust, None, &weights_rust);
    *score_out = score;
    
    ZlsError::Ok
}

/// Calculate absolute score from OAM and modem metrics
/// 
/// # Safety
/// - `oam` must be a valid pointer to ZlsOamMetrics
/// - `modem` may be null (modem metrics optional)
/// - `weights` must be a valid pointer to ZlsScoringWeights
/// - `score_out` must be a valid pointer to f64
#[no_mangle]
pub unsafe extern "C" fn zls_score_full(
    oam: *const ZlsOamMetrics,
    modem: *const ZlsModemMetrics,
    weights: *const ZlsScoringWeights,
    score_out: *mut f64,
) -> ZlsError {
    if oam.is_null() || weights.is_null() || score_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let oam_rust = crate::metrics::OamMetrics::from(&*oam);
    let weights_rust = ScoringWeights::from(&*weights);
    
    let modem_rust = if modem.is_null() {
        None
    } else {
        Some(crate::metrics::ModemMetrics::from(&*modem))
    };
    
    let score = absolute_score(&oam_rust, modem_rust.as_ref(), &weights_rust);
    *score_out = score;
    
    ZlsError::Ok
}

/// Calculate normalized score relative to link type baseline
#[no_mangle]
pub unsafe extern "C" fn zls_score_normalized(
    oam: *const ZlsOamMetrics,
    baseline: *const ZlsLinkTypeBaseline,
    weights: *const ZlsScoringWeights,
    score_out: *mut f64,
) -> ZlsError {
    if oam.is_null() || baseline.is_null() || weights.is_null() || score_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let oam_rust = crate::metrics::OamMetrics::from(&*oam);
    let baseline_rust = LinkTypeBaseline::from(&*baseline);
    let weights_rust = ScoringWeights::from(&*weights);
    
    let score = crate::scoring::normalized_score(&oam_rust, &baseline_rust, &weights_rust);
    *score_out = score;
    
    ZlsError::Ok
}

/// Get default scoring weights
#[no_mangle]
pub unsafe extern "C" fn zls_weights_default(
    weights_out: *mut ZlsScoringWeights,
) -> ZlsError {
    if weights_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let defaults = ScoringWeights::default();
    *weights_out = ZlsScoringWeights {
        rtt: defaults.rtt,
        jitter: defaults.jitter,
        loss: defaults.loss,
        snr: defaults.snr,
        ber: defaults.ber,
        bandwidth: defaults.bandwidth,
        state: defaults.state,
        asymmetry: defaults.asymmetry,
    };
    
    ZlsError::Ok
}

/// Get weights for real-time traffic
#[no_mangle]
pub unsafe extern "C" fn zls_weights_realtime(
    weights_out: *mut ZlsScoringWeights,
) -> ZlsError {
    if weights_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let weights = ScoringWeights::realtime();
    *weights_out = ZlsScoringWeights {
        rtt: weights.rtt,
        jitter: weights.jitter,
        loss: weights.loss,
        snr: weights.snr,
        ber: weights.ber,
        bandwidth: weights.bandwidth,
        state: weights.state,
        asymmetry: weights.asymmetry,
    };
    
    ZlsError::Ok
}

/// Get baseline for a link type
#[no_mangle]
pub unsafe extern "C" fn zls_baseline_for_type(
    link_type: LinkType,
    baseline_out: *mut ZlsLinkTypeBaseline,
) -> ZlsError {
    if baseline_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let baseline = LinkTypeBaseline::for_type(link_type);
    *baseline_out = ZlsLinkTypeBaseline {
        link_type: baseline.link_type,
        typical_rtt_us: baseline.typical_rtt_us,
        max_rtt_us: baseline.max_rtt_us,
        typical_jitter_us: baseline.typical_jitter_us,
        max_jitter_us: baseline.max_jitter_us,
        typical_loss: baseline.typical_loss,
        max_loss: baseline.max_loss,
        snr_baseline_cdb: baseline.snr_baseline_cdb,
        typical_bandwidth_bps: baseline.typical_bandwidth_bps,
    };
    
    ZlsError::Ok
}
```

```rust
// src/ffi/selection.rs

use std::ptr;
use crate::ffi::error::ZlsError;
use crate::ffi::types::*;
use crate::selection::LinkSelector;

/// Create a new link selector
/// 
/// # Safety
/// - `selector_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn zls_selector_new(
    hysteresis: f64,
    failure_threshold: u32,
    selector_out: *mut *mut ZlsLinkSelector,
) -> ZlsError {
    if selector_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let selector = Box::new(ZlsLinkSelector {
        inner: LinkSelector::new(hysteresis, failure_threshold),
    });
    
    *selector_out = Box::into_raw(selector);
    ZlsError::Ok
}

/// Free a link selector
/// 
/// # Safety
/// - `selector` must be a valid pointer created by zls_selector_new
#[no_mangle]
pub unsafe extern "C" fn zls_selector_free(
    selector: *mut ZlsLinkSelector,
) -> ZlsError {
    if selector.is_null() {
        return ZlsError::NullPointer;
    }
    
    drop(Box::from_raw(selector));
    ZlsError::Ok
}

/// Link entry for selection
#[repr(C)]
pub struct ZlsLinkEntry {
    pub id: u64,
    pub oam: ZlsOamMetrics,
    pub modem: ZlsModemMetrics,
    pub has_modem: bool,
}

/// Select best link from available options
/// 
/// # Safety
/// - `selector` must be a valid pointer
/// - `links` must be a valid pointer to array of ZlsLinkEntry
/// - `link_count` must be the length of the links array
/// - `selected_id_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn zls_selector_select(
    selector: *mut ZlsLinkSelector,
    links: *const ZlsLinkEntry,
    link_count: usize,
    weights: *const ZlsScoringWeights,
    selected_id_out: *mut u64,
    has_selection_out: *mut bool,
) -> ZlsError {
    if selector.is_null() || links.is_null() || weights.is_null() 
       || selected_id_out.is_null() || has_selection_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    let selector = &mut (*selector).inner;
    let weights_rust = crate::scoring::ScoringWeights::from(&*weights);
    
    // Convert links
    let links_slice = std::slice::from_raw_parts(links, link_count);
    let links_rust: Vec<_> = links_slice
        .iter()
        .map(|entry| {
            let oam = crate::metrics::OamMetrics::from(&entry.oam);
            let modem = if entry.has_modem {
                Some(crate::metrics::ModemMetrics::from(&entry.modem))
            } else {
                None
            };
            (entry.id, oam, modem)
        })
        .collect();
    
    // Create reference tuples
    let links_refs: Vec<_> = links_rust
        .iter()
        .map(|(id, oam, modem)| (*id, oam, modem.as_ref()))
        .collect();
    
    match selector.select(&links_refs, &weights_rust) {
        Some(id) => {
            *selected_id_out = id;
            *has_selection_out = true;
        }
        None => {
            *has_selection_out = false;
        }
    }
    
    ZlsError::Ok
}

/// Get currently selected link
#[no_mangle]
pub unsafe extern "C" fn zls_selector_current(
    selector: *const ZlsLinkSelector,
    selected_id_out: *mut u64,
    has_selection_out: *mut bool,
) -> ZlsError {
    if selector.is_null() || selected_id_out.is_null() || has_selection_out.is_null() {
        return ZlsError::NullPointer;
    }
    
    match (*selector).inner.current() {
        Some(id) => {
            *selected_id_out = id;
            *has_selection_out = true;
        }
        None => {
            *has_selection_out = false;
        }
    }
    
    ZlsError::Ok
}

/// Reset selector state
#[no_mangle]
pub unsafe extern "C" fn zls_selector_reset(
    selector: *mut ZlsLinkSelector,
) -> ZlsError {
    if selector.is_null() {
        return ZlsError::NullPointer;
    }
    
    (*selector).inner.reset();
    ZlsError::Ok
}
```

### 4.5 Generated C Header

Using `cbindgen`, the following header is generated:

```c
/* zenoh_link_scoring.h - Auto-generated by cbindgen */

#ifndef ZENOH_LINK_SCORING_H
#define ZENOH_LINK_SCORING_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Error codes */
typedef enum {
    ZLS_OK = 0,
    ZLS_NULL_POINTER = 1,
    ZLS_INVALID_ARGUMENT = 2,
    ZLS_BUFFER_TOO_SMALL = 3,
    ZLS_INVALID_HANDLE = 4,
    ZLS_OUT_OF_MEMORY = 5,
    ZLS_UNKNOWN = 255,
} zls_error_t;

/* Modem state */
typedef enum {
    ZLS_MODEM_STATE_UNKNOWN = 0,
    ZLS_MODEM_STATE_INITIALIZING = 1,
    ZLS_MODEM_STATE_SEARCHING = 2,
    ZLS_MODEM_STATE_SYNCHRONIZING = 3,
    ZLS_MODEM_STATE_CONNECTED = 4,
    ZLS_MODEM_STATE_DEGRADED = 5,
    ZLS_MODEM_STATE_HANDOVER_IN_PROGRESS = 6,
    ZLS_MODEM_STATE_STANDBY = 7,
    ZLS_MODEM_STATE_DISCONNECTED = 8,
    ZLS_MODEM_STATE_ERROR = 9,
} zls_modem_state_t;

/* Link type */
typedef enum {
    ZLS_LINK_TYPE_UNKNOWN = 0,
    ZLS_LINK_TYPE_FIBER = 1,
    ZLS_LINK_TYPE_ETHERNET_LAN = 2,
    ZLS_LINK_TYPE_ETHERNET_WAN = 3,
    ZLS_LINK_TYPE_SATELLITE_GEO = 4,
    ZLS_LINK_TYPE_SATELLITE_LEO = 5,
    ZLS_LINK_TYPE_RADIO_TACTICAL = 6,
    ZLS_LINK_TYPE_RADIO_MOBILE = 7,
    ZLS_LINK_TYPE_ACOUSTIC = 8,
} zls_link_type_t;

/* OAM metrics */
typedef struct {
    uint64_t rtt_current_us;
    uint64_t rtt_min_us;
    uint64_t rtt_max_us;
    uint64_t rtt_avg_us;
    uint64_t jitter_us;
    uint64_t forward_delay_us;
    uint64_t reverse_delay_us;
    bool one_way_valid;
    uint64_t tx_count;
    uint64_t rx_count;
    uint32_t consecutive_failures;
    uint64_t last_success_us;
} zls_oam_metrics_t;

/* Modem metrics */
typedef struct {
    int32_t snr_cdb;
    bool snr_valid;
    int32_t rssi_cdbm;
    bool rssi_valid;
    int8_t ber_exponent;
    bool ber_valid;
    uint64_t available_bps;
    uint64_t max_bps;
    zls_modem_state_t state;
    uint64_t state_duration_us;
    uint64_t handover_eta_us;
} zls_modem_metrics_t;

/* Scoring weights */
typedef struct {
    double rtt;
    double jitter;
    double loss;
    double snr;
    double ber;
    double bandwidth;
    double state;
    double asymmetry;
} zls_scoring_weights_t;

/* Link type baseline */
typedef struct {
    zls_link_type_t link_type;
    uint64_t typical_rtt_us;
    uint64_t max_rtt_us;
    uint64_t typical_jitter_us;
    uint64_t max_jitter_us;
    double typical_loss;
    double max_loss;
    int32_t snr_baseline_cdb;
    uint64_t typical_bandwidth_bps;
} zls_link_type_baseline_t;

/* Link entry for selection */
typedef struct {
    uint64_t id;
    zls_oam_metrics_t oam;
    zls_modem_metrics_t modem;
    bool has_modem;
} zls_link_entry_t;

/* Opaque selector handle */
typedef struct zls_link_selector zls_link_selector_t;

/* === Scoring Functions === */

/* Calculate score from OAM metrics only */
zls_error_t zls_score_oam(
    const zls_oam_metrics_t *oam,
    const zls_scoring_weights_t *weights,
    double *score_out
);

/* Calculate score from OAM and modem metrics */
zls_error_t zls_score_full(
    const zls_oam_metrics_t *oam,
    const zls_modem_metrics_t *modem,  /* may be NULL */
    const zls_scoring_weights_t *weights,
    double *score_out
);

/* Calculate normalized score relative to baseline */
zls_error_t zls_score_normalized(
    const zls_oam_metrics_t *oam,
    const zls_link_type_baseline_t *baseline,
    const zls_scoring_weights_t *weights,
    double *score_out
);

/* === Weight Presets === */

zls_error_t zls_weights_default(zls_scoring_weights_t *weights_out);
zls_error_t zls_weights_realtime(zls_scoring_weights_t *weights_out);
zls_error_t zls_weights_interactive(zls_scoring_weights_t *weights_out);
zls_error_t zls_weights_bulk_transfer(zls_scoring_weights_t *weights_out);
zls_error_t zls_weights_tactical(zls_scoring_weights_t *weights_out);

/* === Baselines === */

zls_error_t zls_baseline_for_type(
    zls_link_type_t link_type,
    zls_link_type_baseline_t *baseline_out
);

/* === Link Selector === */

/* Create a new link selector */
zls_error_t zls_selector_new(
    double hysteresis,
    uint32_t failure_threshold,
    zls_link_selector_t **selector_out
);

/* Free a link selector */
zls_error_t zls_selector_free(zls_link_selector_t *selector);

/* Select best link */
zls_error_t zls_selector_select(
    zls_link_selector_t *selector,
    const zls_link_entry_t *links,
    size_t link_count,
    const zls_scoring_weights_t *weights,
    uint64_t *selected_id_out,
    bool *has_selection_out
);

/* Get current selection */
zls_error_t zls_selector_current(
    const zls_link_selector_t *selector,
    uint64_t *selected_id_out,
    bool *has_selection_out
);

/* Reset selector state */
zls_error_t zls_selector_reset(zls_link_selector_t *selector);

/* Force selection of specific link */
zls_error_t zls_selector_force(
    zls_link_selector_t *selector,
    uint64_t link_id
);

#ifdef __cplusplus
}
#endif

#endif /* ZENOH_LINK_SCORING_H */
```

---

## 5. C Usage Example

```c
/* examples/c_example/main.c */

#include <stdio.h>
#include <string.h>
#include "zenoh_link_scoring.h"

int main(void) {
    zls_error_t err;
    
    /* === Basic Scoring === */
    
    /* Create OAM metrics */
    zls_oam_metrics_t oam = {
        .rtt_current_us = 5000,
        .rtt_min_us = 4000,
        .rtt_max_us = 8000,
        .rtt_avg_us = 5500,
        .jitter_us = 500,
        .forward_delay_us = 0,
        .reverse_delay_us = 0,
        .one_way_valid = false,
        .tx_count = 100,
        .rx_count = 99,
        .consecutive_failures = 0,
        .last_success_us = 0,
    };
    
    /* Get default weights */
    zls_scoring_weights_t weights;
    err = zls_weights_default(&weights);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to get default weights: %d\n", err);
        return 1;
    }
    
    /* Calculate score */
    double score;
    err = zls_score_oam(&oam, &weights, &score);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to calculate score: %d\n", err);
        return 1;
    }
    
    printf("OAM-only score: %.2f\n", score);
    
    /* === Scoring with Modem Metrics === */
    
    zls_modem_metrics_t modem = {
        .snr_cdb = 2500,  /* 25 dB */
        .snr_valid = true,
        .rssi_cdbm = -6500,  /* -65 dBm */
        .rssi_valid = true,
        .ber_exponent = 7,  /* 10^-7 */
        .ber_valid = true,
        .available_bps = 50000000,
        .max_bps = 100000000,
        .state = ZLS_MODEM_STATE_CONNECTED,
        .state_duration_us = 60000000,
        .handover_eta_us = 0,
    };
    
    err = zls_score_full(&oam, &modem, &weights, &score);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to calculate full score: %d\n", err);
        return 1;
    }
    
    printf("Full score (OAM + modem): %.2f\n", score);
    
    /* === Normalized Scoring === */
    
    zls_link_type_baseline_t baseline;
    err = zls_baseline_for_type(ZLS_LINK_TYPE_ETHERNET_LAN, &baseline);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to get baseline: %d\n", err);
        return 1;
    }
    
    err = zls_score_normalized(&oam, &baseline, &weights, &score);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to calculate normalized score: %d\n", err);
        return 1;
    }
    
    printf("Normalized score (vs Ethernet LAN baseline): %.2f\n", score);
    
    /* === Link Selection === */
    
    /* Create selector */
    zls_link_selector_t *selector = NULL;
    err = zls_selector_new(5.0, 3, &selector);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to create selector: %d\n", err);
        return 1;
    }
    
    /* Create some links */
    zls_link_entry_t links[3];
    
    /* Link 0: Good link */
    links[0].id = 100;
    links[0].oam = oam;
    links[0].has_modem = false;
    
    /* Link 1: Degraded link */
    links[1].id = 101;
    links[1].oam = oam;
    links[1].oam.rtt_avg_us = 50000;  /* Higher RTT */
    links[1].oam.consecutive_failures = 1;
    links[1].has_modem = false;
    
    /* Link 2: Failed link */
    links[2].id = 102;
    links[2].oam = oam;
    links[2].oam.consecutive_failures = 5;  /* Exceeds threshold */
    links[2].has_modem = false;
    
    /* Select best link */
    uint64_t selected_id;
    bool has_selection;
    
    err = zls_selector_select(selector, links, 3, &weights, &selected_id, &has_selection);
    if (err != ZLS_OK) {
        fprintf(stderr, "Failed to select link: %d\n", err);
        zls_selector_free(selector);
        return 1;
    }
    
    if (has_selection) {
        printf("Selected link: %llu\n", (unsigned long long)selected_id);
    } else {
        printf("No link available\n");
    }
    
    /* Cleanup */
    zls_selector_free(selector);
    
    printf("Done!\n");
    return 0;
}
```

Makefile for C example:

```makefile
# examples/c_example/Makefile

CC = gcc
CFLAGS = -Wall -Wextra -I../../include
LDFLAGS = -L../../target/release -lzenoh_link_scoring -lpthread -ldl -lm

main: main.c
	$(CC) $(CFLAGS) -o main main.c $(LDFLAGS)

.PHONY: clean
clean:
	rm -f main
```

---

## 6. Implementation Plan

### 6.1 Phase Overview

| Phase | Description | Duration | Deliverables |
|-------|-------------|----------|--------------|
| 1 | Core Types & Metrics | 1 week | OamMetrics, ModemMetrics, ScoringWeights |
| 2 | Scoring Algorithms | 1 week | absolute_score, normalized_score |
| 3 | Link Selection | 1 week | LinkSelector, hysteresis, failover |
| 4 | FFI Layer | 1 week | C API, header generation |
| 5 | Testing & Examples | 1 week | Tests, benchmarks, C example |
| **Total** | | **5 weeks** | |

### 6.2 Phase 1: Core Types & Metrics (Week 1)

**Files:**
- `src/lib.rs`
- `src/metrics/mod.rs`
- `src/metrics/oam.rs`
- `src/metrics/modem.rs`

**Tasks:**
- [ ] Create crate structure
- [ ] Implement `OamMetrics` struct
- [ ] Implement `ModemMetrics` struct
- [ ] Implement `ModemState` enum
- [ ] Add helper methods (loss_ratio, snr_db, etc.)
- [ ] Add `#[repr(C)]` for FFI compatibility
- [ ] Unit tests for metrics

**Estimated LOC:** ~400

### 6.3 Phase 2: Scoring Algorithms (Week 2)

**Files:**
- `src/scoring/mod.rs`
- `src/scoring/weights.rs`
- `src/scoring/absolute.rs`
- `src/scoring/normalized.rs`
- `src/scoring/priority.rs`
- `src/baselines/mod.rs`
- `src/baselines/presets.rs`

**Tasks:**
- [ ] Implement `ScoringWeights` with presets
- [ ] Implement `absolute_score()` function
- [ ] Implement `normalized_score()` function
- [ ] Implement `LinkTypeBaseline` with presets
- [ ] Implement priority-based weight selection
- [ ] Unit tests for scoring
- [ ] Property-based tests (scores always in 0-100)

**Estimated LOC:** ~600

### 6.4 Phase 3: Link Selection (Week 3)

**Files:**
- `src/selection/mod.rs`
- `src/selection/selector.rs`
- `src/selection/failover.rs`
- `src/selection/state.rs`

**Tasks:**
- [ ] Implement `LinkSelector` struct
- [ ] Implement hysteresis logic
- [ ] Implement failure detection
- [ ] Implement `rank_links()` function
- [ ] Unit tests for selection
- [ ] Integration tests (selector + scoring)

**Estimated LOC:** ~400

### 6.5 Phase 4: FFI Layer (Week 4)

**Files:**
- `src/ffi/mod.rs`
- `src/ffi/types.rs`
- `src/ffi/error.rs`
- `src/ffi/metrics.rs`
- `src/ffi/scoring.rs`
- `src/ffi/selection.rs`
- `cbindgen.toml`
- `build.rs`

**Tasks:**
- [ ] Define C-compatible types
- [ ] Implement FFI functions for scoring
- [ ] Implement FFI functions for selection
- [ ] Implement FFI functions for weights/baselines
- [ ] Configure cbindgen
- [ ] Generate C header
- [ ] Test FFI from C

**Estimated LOC:** ~800

### 6.6 Phase 5: Testing & Examples (Week 5)

**Files:**
- `tests/scoring_tests.rs`
- `tests/selection_tests.rs`
- `tests/ffi_tests.rs`
- `benches/scoring_bench.rs`
- `examples/basic_scoring.rs`
- `examples/link_selection.rs`
- `examples/c_example/main.c`
- `examples/c_example/Makefile`
- `README.md`

**Tasks:**
- [ ] Comprehensive unit tests
- [ ] Integration tests
- [ ] FFI tests (Rust calling C calling Rust)
- [ ] Benchmarks for scoring performance
- [ ] Rust usage examples
- [ ] C usage example
- [ ] Documentation
- [ ] README with usage guide

**Estimated LOC:** ~600

---

## 7. Summary

### 7.1 Crate Statistics

| Metric | Value |
|--------|-------|
| Total estimated LOC | ~2,800 |
| Implementation time | 5 weeks |
| Rust API | Full-featured |
| C API | Complete FFI bindings |
| Dependencies | Minimal (serde optional) |

### 7.2 API Surface

| Category | Rust Functions | C Functions |
|----------|---------------|-------------|
| Scoring | 3 | 3 |
| Weights | 5 presets | 5 |
| Baselines | 8 presets | 1 (by type) |
| Selection | 5 | 5 |
| **Total** | **21** | **14** |

### 7.3 Consumers

| Consumer | Integration |
|----------|-------------|
| Zenoh (Rust) | `zenoh-link-scoring = "0.1"` |
| External Controller (Rust) | `zenoh-link-scoring = "0.1"` |
| External Controller (C/C++) | Link `libzenoh_link_scoring.so` |
| Modem Driver (C) | Link `libzenoh_link_scoring.so` |

---

## Appendix A: Cargo.toml

```toml
[package]
name = "zenoh-link-scoring"
version = "0.1.0"
edition = "2021"
authors = ["ZettaScale Technology"]
description = "Link quality scoring algorithms for Zenoh"
license = "Apache-2.0"
repository = "https://github.com/eclipse-zenoh/zenoh"
categories = ["network-programming", "algorithms"]
keywords = ["zenoh", "networking", "scoring", "link-quality"]

[lib]
crate-type = ["lib", "cdylib", "staticlib"]

[features]
default = ["std"]
std = []
ffi = ["std"]
serde = ["dep:serde"]

[dependencies]
serde = { version = "1.0", features = ["derive"], optional = true }

[build-dependencies]
cbindgen = "0.26"

[dev-dependencies]
criterion = "0.5"
proptest = "1.0"

[[bench]]
name = "scoring_bench"
harness = false
```

---

## Appendix B: Build Script

```rust
// build.rs

fn main() {
    #[cfg(feature = "ffi")]
    {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        
        cbindgen::Builder::new()
            .with_crate(crate_dir)
            .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
            .generate()
            .expect("Unable to generate C bindings")
            .write_to_file("include/zenoh_link_scoring.h");
    }
}
```

```toml
# cbindgen.toml

language = "C"
header = "/* Auto-generated by cbindgen - Do not edit */"
include_guard = "ZENOH_LINK_SCORING_H"
autogen_warning = "/* Warning: this file is autogenerated by cbindgen. Don't modify this manually. */"
include_version = true
namespace = ""
cpp_compat = true

[defines]
"feature = ffi" = ""

[enum]
rename_variants = "ScreamingSnakeCase"
prefix_with_name = true

[export]
prefix = "zls_"

[export.rename]
"ZlsError" = "zls_error_t"
"ZlsOamMetrics" = "zls_oam_metrics_t"
"ZlsModemMetrics" = "zls_modem_metrics_t"
"ZlsScoringWeights" = "zls_scoring_weights_t"
"ZlsLinkTypeBaseline" = "zls_link_type_baseline_t"
"ZlsLinkSelector" = "zls_link_selector_t"
"ZlsLinkEntry" = "zls_link_entry_t"
"ModemState" = "zls_modem_state_t"
"LinkType" = "zls_link_type_t"
```
