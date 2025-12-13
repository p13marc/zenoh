# Link Quality Scoring

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Status | Draft |
| Date | 2024-12-13 |

---

## 1. Overview

This document describes how to score and rank communication links based on quality metrics. The scoring algorithms are **architecture-independent** and can be implemented in Zenoh, an external controller, or any other component that needs to select the best link among multiple options.

### 1.1 Purpose

Link scoring enables:
- **Optimal link selection** - Choose the best link for current conditions
- **Load balancing** - Distribute traffic across links based on capacity
- **Failover** - Detect degraded links and switch to alternatives
- **QoS routing** - Match link characteristics to traffic requirements

### 1.2 Input Metrics

Scoring combines metrics from two sources:

| Source | Metrics | How Obtained |
|--------|---------|--------------|
| **OAM Probes** | RTT, jitter, packet loss, one-way delay | Zenoh OAM module |
| **Modem/Physical Layer** | SNR, BER, bandwidth, modem state | Modem driver |

---

## 2. OAM Metrics

### 2.1 Round-Trip Time (RTT)

**Definition:** Time for a probe to travel from sender to receiver and back.

**Measurement:** Send LBM (Loopback Message), receive LBR (Loopback Reply).

```
RTT = T_receive(LBR) - T_send(LBM)
```

**Aggregation:**

```rust
struct RttMetrics {
    /// Most recent RTT measurement
    current: Duration,
    
    /// Minimum observed RTT (best case)
    min: Duration,
    
    /// Maximum observed RTT (worst case)
    max: Duration,
    
    /// Exponential moving average
    /// EMA = alpha * current + (1 - alpha) * previous_EMA
    /// Typical alpha = 0.1 to 0.3
    avg: Duration,
}

impl RttMetrics {
    fn update(&mut self, rtt: Duration, alpha: f64) {
        self.current = rtt;
        self.min = self.min.min(rtt);
        self.max = self.max.max(rtt);
        
        // Exponential moving average
        let current_us = rtt.as_micros() as f64;
        let avg_us = self.avg.as_micros() as f64;
        let new_avg = alpha * current_us + (1.0 - alpha) * avg_us;
        self.avg = Duration::from_micros(new_avg as u64);
    }
}
```

**Interpretation:**
- Lower RTT = better link
- High variance (max - min) indicates instability
- Use `avg` for scoring to smooth out spikes

### 2.2 Jitter

**Definition:** Variation in packet delay over time.

**Measurement:** RFC 3550 interarrival jitter calculation.

```rust
/// RFC 3550 jitter calculation
/// J(i) = J(i-1) + (|D(i)| - J(i-1)) / 16
/// where D(i) = (R(i) - R(i-1)) - (S(i) - S(i-1))
/// 
/// Simplified using RTT difference:
/// D(i) = RTT(i) - RTT(i-1)

fn update_jitter(jitter: &mut Duration, prev_rtt: Duration, curr_rtt: Duration) {
    let diff = if curr_rtt > prev_rtt {
        curr_rtt - prev_rtt
    } else {
        prev_rtt - curr_rtt
    };
    
    // J = J + (|D| - J) / 16
    let j_us = jitter.as_micros() as i64;
    let d_us = diff.as_micros() as i64;
    let new_j = j_us + (d_us - j_us) / 16;
    
    *jitter = Duration::from_micros(new_j.max(0) as u64);
}
```

**Interpretation:**
- Lower jitter = more stable link
- High jitter problematic for real-time traffic (voice, video)
- Satellite and radio links typically have higher jitter

### 2.3 Packet Loss

**Definition:** Ratio of probes that did not receive a reply.

**Measurement:** Track sent probes and received replies.

```rust
struct LossMetrics {
    /// Total probes sent
    tx_count: u64,
    
    /// Total replies received
    rx_count: u64,
    
    /// Loss ratio (0.0 to 1.0)
    loss_ratio: f64,
    
    /// Recent loss (sliding window)
    recent_loss_ratio: f64,
}

impl LossMetrics {
    fn update(&mut self, probe_succeeded: bool, window_size: usize) {
        self.tx_count += 1;
        if probe_succeeded {
            self.rx_count += 1;
        }
        
        // Overall loss ratio
        self.loss_ratio = 1.0 - (self.rx_count as f64 / self.tx_count as f64);
        
        // Recent loss (use circular buffer for sliding window)
        // ... implementation depends on window tracking
    }
}
```

**Interpretation:**
- Lower loss = better link
- Even small loss (1-2%) significantly impacts TCP throughput
- Burst loss more problematic than distributed loss

### 2.4 One-Way Delay

**Definition:** Time for a packet to travel in one direction only.

**Measurement:** Requires synchronized clocks. Use DMM/DMR (Delay Measurement Message/Reply) with 4 timestamps.

```
Forward delay  = T2 - T1  (sender's send time to receiver's receive time)
Reverse delay  = T4 - T3  (receiver's send time to sender's receive time)

Where:
  T1 = DMM send timestamp (sender clock)
  T2 = DMM receive timestamp (receiver clock)
  T3 = DMR send timestamp (receiver clock)
  T4 = DMR receive timestamp (sender clock)
```

**Clock synchronization requirements:**

| Link Type | Clock Sync Method | Accuracy Needed |
|-----------|-------------------|-----------------|
| Fiber/Ethernet LAN | Optional (RTT sufficient) | N/A |
| Ethernet WAN | NTP recommended | ~10ms |
| Radio/Satellite | GPS required | ~1ms |
| Acoustic | Pre-synchronized atomic | ~10ms |

**Asymmetry detection:**

```rust
struct DelayMetrics {
    forward_delay: Option<Duration>,
    reverse_delay: Option<Duration>,
}

impl DelayMetrics {
    /// Asymmetry ratio: 0.0 = symmetric, 1.0 = fully asymmetric
    fn asymmetry_ratio(&self) -> Option<f64> {
        match (self.forward_delay, self.reverse_delay) {
            (Some(fwd), Some(rev)) => {
                let fwd_us = fwd.as_micros() as f64;
                let rev_us = rev.as_micros() as f64;
                let diff = (fwd_us - rev_us).abs();
                let sum = fwd_us + rev_us;
                if sum > 0.0 {
                    Some(diff / sum)
                } else {
                    Some(0.0)
                }
            }
            _ => None,
        }
    }
}
```

**Interpretation:**
- High asymmetry indicates different paths or capacity in each direction
- Common in satellite (more downlink than uplink)
- May indicate routing issues in WAN

---

## 3. Modem/Physical Layer Metrics

### 3.1 Signal-to-Noise Ratio (SNR)

**Definition:** Ratio of signal power to noise power, in decibels (dB).

**Typical ranges:**

| Link Type | Poor | Marginal | Good | Excellent |
|-----------|------|----------|------|-----------|
| Radio | < 10 dB | 10-15 dB | 15-25 dB | > 25 dB |
| Satellite | < 5 dB | 5-10 dB | 10-15 dB | > 15 dB |
| Acoustic | < 5 dB | 5-10 dB | 10-20 dB | > 20 dB |

**Scoring contribution:**

```rust
/// SNR penalty: lower SNR = higher penalty
fn snr_penalty(snr_db: f64, baseline_db: f64) -> f64 {
    // No penalty above baseline
    if snr_db >= baseline_db {
        return 0.0;
    }
    
    // Linear penalty below baseline
    // Example: baseline=20dB, snr=15dB -> penalty = 5 * scale
    let deficit = baseline_db - snr_db;
    deficit * SNR_PENALTY_SCALE  // e.g., 1.0 per dB
}
```

### 3.2 Received Signal Strength (RSSI)

**Definition:** Power level of received signal, in dBm.

**Typical ranges:**

| Strength | RSSI (dBm) |
|----------|------------|
| Excellent | > -50 |
| Good | -50 to -70 |
| Fair | -70 to -85 |
| Poor | -85 to -100 |
| No signal | < -100 |

**Note:** RSSI alone doesn't indicate link quality; SNR is more meaningful.

### 3.3 Bit Error Rate (BER)

**Definition:** Ratio of errored bits to total bits transmitted.

**Typical ranges:**

| Quality | BER | Exponent |
|---------|-----|----------|
| Excellent | < 10^-9 | < -9 |
| Good | 10^-9 to 10^-6 | -9 to -6 |
| Marginal | 10^-6 to 10^-4 | -6 to -4 |
| Poor | > 10^-4 | > -4 |

**Scoring contribution:**

```rust
/// BER penalty using log scale
/// BER of 10^-6 = exponent -6
fn ber_penalty(ber: f64) -> f64 {
    if ber <= 0.0 {
        return 0.0;  // Perfect (or no data)
    }
    
    let exponent = ber.log10();  // e.g., 10^-6 -> -6
    
    // Baseline: 10^-6 = no penalty
    // Penalty increases as exponent approaches 0
    let baseline_exp = -6.0;
    if exponent < baseline_exp {
        return 0.0;
    }
    
    (exponent - baseline_exp) * BER_PENALTY_SCALE  // e.g., 5.0 per decade
}
```

### 3.4 Available Bandwidth

**Definition:** Current usable data rate in bits per second.

**Considerations:**
- May vary dynamically (adaptive modulation)
- Affected by interference, congestion, channel conditions
- Different in each direction (asymmetric links)

**Scoring contribution:**

```rust
/// Bandwidth contribution to score
/// Higher bandwidth = better score
fn bandwidth_score(available_bps: u64, max_bps: u64) -> f64 {
    if max_bps == 0 {
        return 0.0;
    }
    
    // Normalize to 0-100 scale
    let utilization = available_bps as f64 / max_bps as f64;
    utilization * 100.0 * BANDWIDTH_WEIGHT
}
```

### 3.5 Modem State

**Definition:** Operational state of the modem.

**States and penalties:**

| State | Description | Suggested Penalty |
|-------|-------------|-------------------|
| Connected | Normal operation | 0 |
| Degraded | Connected but poor quality | 15-20 |
| Synchronizing | Acquiring sync | 30 |
| Searching | Looking for signal | 50 |
| HandoverInProgress | Satellite handover | 20-30 |
| Disconnected | No connectivity | 100 (exclude) |
| Error | Error state | 100 (exclude) |

---

## 4. Scoring Algorithms

### 4.1 Absolute Scoring

**Concept:** Calculate a score from 0-100 where higher is better. Apply penalties for each metric that deviates from ideal.

```rust
pub struct ScoringWeights {
    pub rtt: f64,       // Penalty per millisecond of RTT
    pub jitter: f64,    // Penalty per millisecond of jitter
    pub loss: f64,      // Penalty per percentage point of loss
    pub snr: f64,       // Penalty per dB below baseline
    pub ber: f64,       // Penalty per decade above baseline
    pub bandwidth: f64, // Weight for bandwidth contribution
    pub state: f64,     // Weight for modem state penalty
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            rtt: 0.5,       // 0.5 points per ms
            jitter: 2.0,    // 2 points per ms
            loss: 10.0,     // 10 points per 1% loss
            snr: 1.0,       // 1 point per dB below baseline
            ber: 5.0,       // 5 points per decade
            bandwidth: 0.1, // 0.1 weight for bandwidth
            state: 1.0,     // 1x state penalty
        }
    }
}

pub fn absolute_score(
    oam: &OamMetrics,
    modem: Option<&ModemMetrics>,
    weights: &ScoringWeights,
) -> f64 {
    let mut score = 100.0;
    
    // === OAM Penalties ===
    
    // RTT penalty (ms)
    score -= oam.rtt_avg.as_millis() as f64 * weights.rtt;
    
    // Jitter penalty (ms)
    score -= oam.jitter.as_millis() as f64 * weights.jitter;
    
    // Loss penalty (percentage)
    score -= oam.loss_ratio * 100.0 * weights.loss;
    
    // === Modem Penalties (if available) ===
    
    if let Some(m) = modem {
        // SNR penalty
        if let Some(snr) = m.snr_db {
            let snr_baseline = 20.0;  // dB
            if snr < snr_baseline {
                score -= (snr_baseline - snr) * weights.snr;
            }
        }
        
        // BER penalty
        if let Some(ber) = m.ber {
            if ber > 0.0 {
                let exp = ber.log10();
                let baseline_exp = -6.0;
                if exp > baseline_exp {
                    score -= (exp - baseline_exp) * weights.ber;
                }
            }
        }
        
        // Bandwidth bonus
        if m.max_bandwidth_bps > 0 {
            let bw_ratio = m.available_bandwidth_bps as f64 / m.max_bandwidth_bps as f64;
            score += bw_ratio * 10.0 * weights.bandwidth;
        }
        
        // State penalty
        let state_penalty = match m.state {
            ModemState::Connected => 0.0,
            ModemState::Degraded => 15.0,
            ModemState::Synchronizing => 30.0,
            ModemState::Searching => 50.0,
            ModemState::HandoverInProgress => 25.0,
            ModemState::Disconnected => 100.0,
            ModemState::Error => 100.0,
            _ => 5.0,
        };
        score -= state_penalty * weights.state;
    }
    
    score.max(0.0)
}
```

### 4.2 Normalized Scoring

**Concept:** Score relative to what's expected for the link type. A "perfect" acoustic link shouldn't compete unfairly with a "degraded" fiber link.

```rust
pub struct LinkTypeBaseline {
    /// Expected RTT for a healthy link
    pub typical_rtt: Duration,
    /// Maximum acceptable RTT
    pub max_rtt: Duration,
    /// Expected jitter
    pub typical_jitter: Duration,
    /// Expected loss ratio
    pub typical_loss: f64,
    /// Expected bandwidth
    pub typical_bandwidth_bps: u64,
}

impl LinkTypeBaseline {
    pub fn fiber() -> Self {
        Self {
            typical_rtt: Duration::from_micros(500),
            max_rtt: Duration::from_millis(5),
            typical_jitter: Duration::from_micros(50),
            typical_loss: 0.0,
            typical_bandwidth_bps: 10_000_000_000,
        }
    }
    
    pub fn satellite_geo() -> Self {
        Self {
            typical_rtt: Duration::from_millis(600),
            max_rtt: Duration::from_millis(800),
            typical_jitter: Duration::from_millis(10),
            typical_loss: 0.01,
            typical_bandwidth_bps: 20_000_000,
        }
    }
    
    pub fn radio_tactical() -> Self {
        Self {
            typical_rtt: Duration::from_millis(100),
            max_rtt: Duration::from_millis(500),
            typical_jitter: Duration::from_millis(30),
            typical_loss: 0.02,
            typical_bandwidth_bps: 256_000,
        }
    }
    
    pub fn acoustic() -> Self {
        Self {
            typical_rtt: Duration::from_secs(2),
            max_rtt: Duration::from_secs(10),
            typical_jitter: Duration::from_millis(500),
            typical_loss: 0.10,
            typical_bandwidth_bps: 1_000,
        }
    }
}

pub fn normalized_score(
    oam: &OamMetrics,
    baseline: &LinkTypeBaseline,
    weights: &ScoringWeights,
) -> f64 {
    let mut score = 100.0;
    
    // RTT: penalize deviation from typical
    let rtt_ratio = oam.rtt_avg.as_micros() as f64 / baseline.typical_rtt.as_micros() as f64;
    if rtt_ratio > 1.0 {
        // Penalty for being worse than typical
        score -= (rtt_ratio - 1.0) * 20.0 * weights.rtt;
    }
    
    // Jitter: penalize deviation from typical
    let jitter_ratio = oam.jitter.as_micros() as f64 / baseline.typical_jitter.as_micros().max(1) as f64;
    if jitter_ratio > 1.0 {
        score -= (jitter_ratio - 1.0) * 15.0 * weights.jitter;
    }
    
    // Loss: penalize deviation from typical
    let loss_excess = oam.loss_ratio - baseline.typical_loss;
    if loss_excess > 0.0 {
        score -= loss_excess * 100.0 * weights.loss;
    }
    
    score.max(0.0)
}
```

### 4.3 Priority-Aware Scoring

**Concept:** Different traffic priorities have different requirements. Apply different weights based on priority.

```rust
/// Zenoh priorities (0 = highest, 7 = lowest)
pub enum Priority {
    Control = 0,        // Session control
    RealTime = 1,       // Real-time data
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

pub fn weights_for_priority(priority: Priority) -> ScoringWeights {
    match priority {
        // Real-time: jitter and loss are critical
        Priority::Control | Priority::RealTime => ScoringWeights {
            rtt: 1.0,
            jitter: 5.0,    // High jitter penalty
            loss: 20.0,     // High loss penalty
            ..Default::default()
        },
        
        // Interactive: latency matters
        Priority::InteractiveHigh | Priority::InteractiveLow => ScoringWeights {
            rtt: 2.0,       // High RTT penalty
            jitter: 2.0,
            loss: 10.0,
            ..Default::default()
        },
        
        // Bulk data: bandwidth and loss matter, latency less important
        Priority::DataHigh | Priority::Data | Priority::DataLow => ScoringWeights {
            rtt: 0.2,       // Low RTT penalty
            jitter: 0.5,
            loss: 15.0,     // Loss still matters for TCP
            bandwidth: 1.0, // Bandwidth important
            ..Default::default()
        },
        
        // Background: just don't fail
        Priority::Background => ScoringWeights {
            rtt: 0.1,
            jitter: 0.1,
            loss: 5.0,
            bandwidth: 0.5,
            ..Default::default()
        },
    }
}
```

---

## 5. Link Selection

### 5.1 Best Link Selection

Select the link with the highest score:

```rust
pub fn select_best_link(
    links: &[Link],
    priority: Priority,
    weights: &ScoringWeights,
) -> Option<&Link> {
    links
        .iter()
        .filter(|l| l.is_alive())
        .max_by(|a, b| {
            let score_a = absolute_score(&a.oam_metrics, a.modem_metrics.as_ref(), weights);
            let score_b = absolute_score(&b.oam_metrics, b.modem_metrics.as_ref(), weights);
            score_a.partial_cmp(&score_b).unwrap_or(Ordering::Equal)
        })
}
```

### 5.2 Hysteresis

**Concept:** Prevent rapid switching between links when scores are close. Only switch if the improvement exceeds a threshold.

```rust
pub struct LinkSelector {
    current_link: Option<LinkId>,
    hysteresis: f64,  // Minimum score improvement to switch
}

impl LinkSelector {
    pub fn select(
        &mut self,
        links: &[Link],
        weights: &ScoringWeights,
    ) -> Option<LinkId> {
        let scored: Vec<_> = links
            .iter()
            .filter(|l| l.is_alive())
            .map(|l| {
                let score = absolute_score(&l.oam_metrics, l.modem_metrics.as_ref(), weights);
                (l.id, score)
            })
            .collect();
        
        if scored.is_empty() {
            self.current_link = None;
            return None;
        }
        
        // Find best link
        let (best_id, best_score) = scored
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        
        // Check if we should switch
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
}
```

### 5.3 Failover Detection

**Concept:** Detect when a link has failed and exclude it from selection.

```rust
pub struct FailoverConfig {
    /// Consecutive probe failures before marking link as down
    pub failure_threshold: u32,
    
    /// Time without response before considering link failed
    pub timeout: Duration,
    
    /// Minimum probes before trusting recovery
    pub recovery_threshold: u32,
}

impl Link {
    pub fn update_alive_status(&mut self, config: &FailoverConfig) {
        let now = Instant::now();
        
        // Check timeout
        if now.duration_since(self.last_probe_received) > config.timeout {
            self.is_alive = false;
            return;
        }
        
        // Check consecutive failures
        if self.consecutive_failures >= config.failure_threshold {
            self.is_alive = false;
            return;
        }
        
        // Check recovery
        if !self.is_alive && self.consecutive_successes >= config.recovery_threshold {
            self.is_alive = true;
        }
    }
}
```

---

## 6. Link Type Considerations

### 6.1 Fiber/Ethernet LAN

| Characteristic | Value |
|----------------|-------|
| RTT | < 1ms |
| Jitter | Very low (< 100us) |
| Loss | Near zero |
| Scoring focus | Mostly binary (working or not) |

**Recommendation:** Use simple alive/dead detection. Scoring differences will be minimal.

### 6.2 WAN / Internet

| Characteristic | Value |
|----------------|-------|
| RTT | 10-200ms |
| Jitter | Low to medium |
| Loss | 0-1% typical |
| Scoring focus | RTT and loss |

**Recommendation:** Standard scoring with moderate weights.

### 6.3 Satellite (GEO)

| Characteristic | Value |
|----------------|-------|
| RTT | 500-700ms (fixed) |
| Jitter | Low to medium |
| Loss | Variable (rain fade) |
| Scoring focus | Loss, SNR, not RTT |

**Recommendation:** Use normalized scoring. Don't penalize inherent high RTT.

### 6.4 Satellite (LEO)

| Characteristic | Value |
|----------------|-------|
| RTT | 20-50ms |
| Jitter | Medium (handovers) |
| Loss | Low to medium |
| Scoring focus | Handover prediction |

**Recommendation:** Include modem state heavily. Anticipate handovers.

### 6.5 Tactical Radio

| Characteristic | Value |
|----------------|-------|
| RTT | 10-500ms |
| Jitter | High |
| Loss | 2-10% typical |
| Scoring focus | SNR, modem state |

**Recommendation:** Tolerate higher loss/jitter. Weight modem metrics heavily.

### 6.6 Acoustic (Underwater)

| Characteristic | Value |
|----------------|-------|
| RTT | 1-10 seconds |
| Jitter | Very high |
| Loss | 10-30% typical |
| Scoring focus | Alive/dead, bandwidth |

**Recommendation:** Use very relaxed thresholds. Focus on availability.

---

## 7. Example Configurations

### 7.1 Default (Balanced)

```json
{
  "weights": {
    "rtt": 0.5,
    "jitter": 2.0,
    "loss": 10.0,
    "snr": 1.0,
    "ber": 5.0,
    "bandwidth": 0.1,
    "state": 1.0
  },
  "hysteresis": 5.0,
  "failure_threshold": 3,
  "timeout_ms": 5000
}
```

### 7.2 Real-Time Optimized

```json
{
  "weights": {
    "rtt": 1.0,
    "jitter": 5.0,
    "loss": 20.0,
    "snr": 2.0,
    "ber": 10.0,
    "bandwidth": 0.05,
    "state": 2.0
  },
  "hysteresis": 3.0,
  "failure_threshold": 2,
  "timeout_ms": 2000
}
```

### 7.3 Bulk Transfer Optimized

```json
{
  "weights": {
    "rtt": 0.1,
    "jitter": 0.2,
    "loss": 15.0,
    "snr": 0.5,
    "ber": 3.0,
    "bandwidth": 2.0,
    "state": 0.5
  },
  "hysteresis": 10.0,
  "failure_threshold": 5,
  "timeout_ms": 10000
}
```

### 7.4 Tactical (Harsh Environment)

```json
{
  "weights": {
    "rtt": 0.2,
    "jitter": 0.5,
    "loss": 3.0,
    "snr": 3.0,
    "ber": 2.0,
    "bandwidth": 0.5,
    "state": 5.0
  },
  "hysteresis": 15.0,
  "failure_threshold": 10,
  "timeout_ms": 30000
}
```

---

## 8. Implementation Notes

### 8.1 Thread Safety

Metrics are updated from probe responses (RX thread) and read for scoring (TX thread or controller). Use appropriate synchronization:

```rust
pub struct LinkMetrics {
    inner: RwLock<LinkMetricsInner>,
}

impl LinkMetrics {
    pub fn update(&self, rtt: Duration) {
        let mut inner = self.inner.write();
        inner.update(rtt);
    }
    
    pub fn score(&self, weights: &ScoringWeights) -> f64 {
        let inner = self.inner.read();
        absolute_score(&inner, weights)
    }
}
```

### 8.2 Clock Considerations

- RTT measurements use local clock only (no sync needed)
- One-way delay requires synchronized clocks
- Jitter calculation uses RTT differences (no sync needed)

### 8.3 Metric Freshness

Consider metric age in scoring:

```rust
fn age_penalty(last_update: Instant, max_age: Duration) -> f64 {
    let age = Instant::now().duration_since(last_update);
    if age > max_age {
        // Stale metrics - reduce confidence
        return 20.0;
    }
    0.0
}
```

---

## Appendix A: Formulas Summary

| Metric | Formula |
|--------|---------|
| RTT | `T_receive(reply) - T_send(request)` |
| RTT EMA | `alpha * current + (1 - alpha) * previous` |
| Jitter (RFC 3550) | `J = J + (\|D\| - J) / 16` |
| Loss ratio | `1 - (rx_count / tx_count)` |
| Forward delay | `T2 - T1` (requires clock sync) |
| Asymmetry | `\|fwd - rev\| / (fwd + rev)` |
| Absolute score | `100 - sum(penalties)` |
| Normalized score | `100 - sum(deviation_penalties)` |

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| **RTT** | Round-Trip Time |
| **EMA** | Exponential Moving Average |
| **SNR** | Signal-to-Noise Ratio |
| **RSSI** | Received Signal Strength Indicator |
| **BER** | Bit Error Rate |
| **Jitter** | Variation in packet delay |
| **Hysteresis** | Minimum improvement required to switch |
| **Baseline** | Expected metrics for a link type |
