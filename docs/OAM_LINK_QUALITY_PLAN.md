# OAM-Based Link Quality Measurement - Implementation Plan

## Overview

This document describes the implementation of Y.1731-inspired OAM (Operations, Administration, and Maintenance) probes for measuring link quality in Zenoh's multilink transport. The goal is to enable intelligent link selection based on real-time metrics: latency (RTT), jitter, and packet loss.

## Design Goals

1. **Optional Feature**: OAM probing is disabled by default, enabled via configuration
2. **KeepAlive Optimization**: When OAM is enabled, KeepAlive becomes redundant and can be disabled (OAM probes serve the same liveness detection purpose)
3. **Y.1731 Inspired**: Follow ITU-T G.8013/Y.1731 patterns for delay and loss measurement
4. **Backward Compatible**: Nodes without OAM support simply ignore unknown OAM message IDs
5. **Per-Link Metrics**: Each link in a multilink transport maintains its own quality metrics

## OAM Message Types

Based on Y.1731 CFM (Connectivity Fault Management), we define these transport-level OAM messages:

| OAM ID | Name | Description |
|--------|------|-------------|
| `0x0010` | LBM (Loopback Message) | Ping request for RTT measurement |
| `0x0011` | LBR (Loopback Reply) | Ping response |
| `0x0020` | DMM (Delay Measurement Message) | 4-timestamp delay measurement request |
| `0x0021` | DMR (Delay Measurement Reply) | 4-timestamp delay measurement response |
| `0x0030` | SLM (Synthetic Loss Message) | Packet loss measurement request |
| `0x0031` | SLR (Synthetic Loss Reply) | Packet loss measurement response |

### Message Wire Format

Using the existing `Oam` transport message structure:

```
 7 6 5 4 3 2 1 0
+-+-+-+-+-+-+-+-+
|Z|ENC|  OAM    |   (message type = 0x00)
+-+-+-+---------+
~    id:z16     ~   (OAM ID: LBM=0x0010, LBR=0x0011, etc.)
+---------------+
~  [oam_exts]   ~   (QoS extension)
+---------------+
%    length     %   (if ENC == ZBuf)
+---------------+
~   payload     ~   (probe-specific data)
+---------------+
```

### Probe Payloads

#### LBM/LBR (Loopback) Payload
```rust
struct LoopbackPayload {
    seq: u32,           // Sequence number
    timestamp_ns: u64,  // Sender timestamp (nanoseconds since epoch)
}
// Total: 12 bytes, fits in Z64 + extension or small ZBuf
```

#### DMM/DMR (Delay Measurement) Payload
```rust
struct DelayMeasurementPayload {
    seq: u32,   // Sequence number
    t1: u64,    // DMM send timestamp (initiator)
    t2: u64,    // DMM receive timestamp (responder) - 0 in DMM
    t3: u64,    // DMR send timestamp (responder) - 0 in DMM
    // t4 is recorded locally by initiator on DMR receipt
}
// Total: 28 bytes, requires ZBuf encoding
```

#### SLM/SLR (Synthetic Loss Measurement) Payload
```rust
struct SyntheticLossPayload {
    seq: u32,           // Sequence number
    tx_counter: u64,    // Total probes sent by sender
    rx_counter: u64,    // Total probes received (filled in reply)
}
// Total: 20 bytes, requires ZBuf encoding
```

## KeepAlive Optimization

When OAM probing is enabled, KeepAlive messages become redundant because:

1. **Liveness Detection**: OAM probes are sent at regular intervals (configurable, default 100ms). If no probe response is received within the timeout, the link is considered failed - same as KeepAlive.

2. **Lease Maintenance**: The lease timer is reset on ANY incoming message, including OAM replies.

3. **Performance Benefit**: Eliminating KeepAlive when OAM is active reduces duplicate liveness traffic.

### Implementation Strategy

```rust
// In TransportManagerConfigUnicast
pub struct OamConfig {
    pub enabled: bool,
    pub probe_interval_ms: u64,
    pub probe_timeout_ms: u64,
    // ... other fields
}

// In tx_task (universal/link.rs)
// When OAM is enabled:
// - OAM prober task handles liveness (sends probes at probe_interval)
// - TX task timeout uses probe_interval instead of keep_alive
// - On timeout: send OAM probe instead of KeepAlive

// When OAM is disabled:
// - Original KeepAlive behavior is preserved
```

### Lease Calculation with OAM

Following Y.1731 CCM (Continuity Check Messages) principles:
- Link is failed if no message received in `3.5 * probe_interval`
- Recommended: `lease = 4 * probe_interval` (same as current keep_alive divisor logic)

```rust
// Current: keep_alive_interval = lease / keep_alive (e.g., 10000ms / 4 = 2500ms)
// With OAM: probe_interval directly configured (e.g., 100ms)
//           lease should be >= 4 * probe_interval for consistency
```

## Link Quality Metrics

### Collected Metrics

```rust
pub struct LinkQualityMetrics {
    // RTT (Round-Trip Time)
    pub rtt_current: Duration,
    pub rtt_min: Duration,
    pub rtt_max: Duration,
    pub rtt_avg: Duration,          // Exponential moving average
    
    // Jitter (RTT variance)
    pub jitter: Duration,           // RFC 3550 style jitter calculation
    
    // Packet Loss
    pub tx_probe_count: u64,
    pub rx_probe_count: u64,
    pub loss_ratio: f64,            // 0.0 to 1.0
    
    // Timestamps
    pub last_probe_sent: Instant,
    pub last_probe_received: Instant,
    
    // Probe failures (consecutive)
    pub consecutive_failures: u32,
}
```

### Jitter Calculation (RFC 3550)

```rust
// Interarrival jitter as per RFC 3550
fn update_jitter(jitter: &mut Duration, prev_rtt: Duration, curr_rtt: Duration) {
    let diff = if curr_rtt > prev_rtt {
        curr_rtt - prev_rtt
    } else {
        prev_rtt - curr_rtt
    };
    // J(i) = J(i-1) + (|D(i-1,i)| - J(i-1))/16
    *jitter = *jitter + (diff - *jitter) / 16;
}
```

### Link Scoring

```rust
pub struct ScoringWeights {
    pub rtt: f64,           // Default: 1.0
    pub jitter: f64,        // Default: 2.0  
    pub loss: f64,          // Default: 10.0
}

impl LinkQualityMetrics {
    pub fn score(&self, weights: &ScoringWeights) -> f64 {
        // Higher score = better link
        // Base score of 100, reduced by weighted penalties
        let rtt_penalty = self.rtt_avg.as_secs_f64() * 1000.0 * weights.rtt;
        let jitter_penalty = self.jitter.as_secs_f64() * 1000.0 * weights.jitter;
        let loss_penalty = self.loss_ratio * 100.0 * weights.loss;
        
        (100.0 - rtt_penalty - jitter_penalty - loss_penalty).max(0.0)
    }
}
```

## Configuration

### New Configuration Structure

Add under `transport.unicast`:

```json5
{
  transport: {
    unicast: {
      // ... existing config ...
      
      /// OAM (Operations, Administration, Maintenance) configuration
      /// for link quality measurement and intelligent link selection.
      /// When enabled, replaces KeepAlive for liveness detection.
      oam: {
        /// Enable OAM probing (default: false)
        enabled: false,
        
        /// Probe interval in milliseconds (default: 100)
        /// Lower values = more accurate metrics, higher overhead
        probe_interval_ms: 100,
        
        /// Probe timeout in milliseconds (default: 500)
        /// If no response within timeout, probe is considered lost
        probe_timeout_ms: 500,
        
        /// Number of samples for moving average calculations (default: 20)
        sample_window: 20,
        
        /// Number of consecutive probe failures before marking link as degraded (default: 3)
        failure_threshold: 3,
        
        /// Link scoring weights for selection algorithm
        scoring: {
          /// Weight for RTT in scoring (default: 1.0)
          rtt: 1.0,
          /// Weight for jitter in scoring (default: 2.0)  
          jitter: 2.0,
          /// Weight for packet loss in scoring (default: 10.0)
          loss: 10.0,
        },
        
        /// Probe types to use (default: ["loopback"])
        /// Options: "loopback" (RTT only), "delay" (4-timestamp), "loss"
        probe_types: ["loopback"],
      },
    },
  },
}
```

### Feature Flag

The OAM functionality should be gated behind a feature flag:

```toml
# Cargo.toml
[features]
transport_oam = []
```

## Implementation Phases

### Phase 1: Protocol Layer (zenoh-protocol)

**Files to modify:**
- `commons/zenoh-protocol/src/transport/oam.rs`

**Changes:**
1. Add OAM ID constants:
```rust
pub mod id {
    use super::OamId;
    
    // Loopback (ping/pong)
    pub const LBM: OamId = 0x0010;  // Loopback Message
    pub const LBR: OamId = 0x0011;  // Loopback Reply
    
    // Delay Measurement (4-timestamp)
    pub const DMM: OamId = 0x0020;  // Delay Measurement Message
    pub const DMR: OamId = 0x0021;  // Delay Measurement Reply
    
    // Synthetic Loss Measurement
    pub const SLM: OamId = 0x0030;  // Synthetic Loss Message
    pub const SLR: OamId = 0x0031;  // Synthetic Loss Reply
}
```

2. Add payload structures (optional, can use raw ZBuf)

### Phase 2: Codec Layer (zenoh-codec)

**Files to modify:**
- `commons/zenoh-codec/src/transport/oam.rs`

**Changes:**
- No changes required if using `ZExtBody::ZBuf` for payloads
- Optionally add typed payload encode/decode helpers

### Phase 3: Configuration Layer (zenoh-config)

**Files to modify:**
- `commons/zenoh-config/src/lib.rs`
- `commons/zenoh-config/src/defaults.rs`

**Changes:**
1. Add `OamConf` structure
2. Add defaults
3. Add validation

### Phase 4: Transport Layer (zenoh-transport)

**New files:**
- `io/zenoh-transport/src/unicast/oam/mod.rs` - Module entry
- `io/zenoh-transport/src/unicast/oam/prober.rs` - Probe sender task
- `io/zenoh-transport/src/unicast/oam/responder.rs` - Probe reply handler
- `io/zenoh-transport/src/unicast/oam/metrics.rs` - Quality metrics tracking
- `io/zenoh-transport/src/unicast/oam/scorer.rs` - Link scoring algorithm

**Files to modify:**
- `io/zenoh-transport/src/unicast/mod.rs` - Add oam module
- `io/zenoh-transport/src/unicast/manager.rs` - Add OAM config
- `io/zenoh-transport/src/unicast/universal/transport.rs` - Start OAM tasks
- `io/zenoh-transport/src/unicast/universal/link.rs` - Integrate with TX task, replace KeepAlive when OAM enabled
- `io/zenoh-transport/src/unicast/universal/rx.rs` - Handle OAM messages
- `io/zenoh-transport/src/unicast/universal/tx.rs` - Use quality scores in `select()`

### Phase 5: Testing

**New test files:**
- `io/zenoh-transport/tests/unicast_oam.rs` - OAM probe tests
- `io/zenoh-transport/tests/unicast_link_quality.rs` - Scoring/selection tests

## Detailed Implementation: Key Components

### OAM Prober Task

```rust
// io/zenoh-transport/src/unicast/oam/prober.rs

async fn oam_prober_task(
    link: TransportLinkUnicast,
    config: OamConfig,
    metrics: Arc<RwLock<LinkQualityMetrics>>,
    token: CancellationToken,
) -> ZResult<()> {
    let mut interval = tokio::time::interval(config.probe_interval);
    let mut seq: u32 = 0;
    let mut pending_probes: HashMap<u32, Instant> = HashMap::new();
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                seq = seq.wrapping_add(1);
                let probe = create_loopback_message(seq);
                pending_probes.insert(seq, Instant::now());
                
                if let Err(e) = link.send(&probe).await {
                    tracing::warn!("OAM probe send failed: {}", e);
                    metrics.write().await.consecutive_failures += 1;
                }
                
                // Cleanup old pending probes (timed out)
                let timeout = config.probe_timeout;
                pending_probes.retain(|_, sent_at| {
                    sent_at.elapsed() < timeout
                });
            }
            _ = token.cancelled() => break,
        }
    }
    Ok(())
}
```

### Modified Link Selection

```rust
// io/zenoh-transport/src/unicast/universal/tx.rs

fn select(
    links: &[TransportLinkUnicastUniversal],
    reliability: Reliability,
    priority: Priority,
    #[cfg(feature = "transport_oam")]
    scoring_weights: &ScoringWeights,
) -> Option<usize> {
    // Step 1: Filter by reliability and priority (existing logic)
    let candidates: Vec<_> = links
        .iter()
        .enumerate()
        .filter(|(_, link)| matches_criteria(link, reliability, priority))
        .collect();
    
    if candidates.is_empty() {
        return None;
    }
    
    #[cfg(feature = "transport_oam")]
    {
        // Step 2: Score by link quality and select best
        candidates
            .into_iter()
            .max_by(|(_, a), (_, b)| {
                let score_a = a.quality_metrics.read().score(scoring_weights);
                let score_b = b.quality_metrics.read().score(scoring_weights);
                score_a.partial_cmp(&score_b).unwrap_or(Ordering::Equal)
            })
            .map(|(idx, _)| idx)
    }
    
    #[cfg(not(feature = "transport_oam"))]
    {
        // Original behavior: return first match
        candidates.first().map(|(idx, _)| *idx)
    }
}
```

### KeepAlive Replacement in TX Task

```rust
// io/zenoh-transport/src/unicast/universal/link.rs

async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    keep_alive: Duration,
    #[cfg(feature = "transport_oam")]
    oam_config: Option<OamConfig>,
    token: CancellationToken,
) -> ZResult<()> {
    // Determine timeout: use OAM probe interval if enabled, else keep_alive
    #[cfg(feature = "transport_oam")]
    let timeout_duration = oam_config
        .as_ref()
        .filter(|c| c.enabled)
        .map(|c| Duration::from_millis(c.probe_interval_ms))
        .unwrap_or(keep_alive);
    
    #[cfg(not(feature = "transport_oam"))]
    let timeout_duration = keep_alive;
    
    loop {
        tokio::select! {
            res = tokio::time::timeout(timeout_duration, pipeline.pull()) => {
                match res {
                    Ok(Some((batch, priority))) => {
                        // Send batch (existing logic)
                    }
                    Ok(None) => break,
                    Err(_) => {
                        // Timeout: send liveness message
                        #[cfg(feature = "transport_oam")]
                        if oam_config.as_ref().map(|c| c.enabled).unwrap_or(false) {
                            // OAM prober task handles probing, 
                            // we just need to check link is alive
                            // (or integrate probe sending here)
                        } else {
                            let message: TransportMessage = KeepAlive.into();
                            link.send(&message).await?;
                        }
                        
                        #[cfg(not(feature = "transport_oam"))]
                        {
                            let message: TransportMessage = KeepAlive.into();
                            link.send(&message).await?;
                        }
                    }
                }
            }
            _ = token.cancelled() => break,
        }
    }
    Ok(())
}
```

## API Exposure (Optional)

For users who want to access link quality metrics programmatically:

```rust
// In zenoh/src/api/session.rs or similar

impl Session {
    /// Get link quality metrics for all active transport links.
    /// Requires `unstable` and `transport_oam` features.
    #[cfg(all(feature = "unstable", feature = "transport_oam"))]
    pub fn link_quality_metrics(&self) -> Vec<LinkQualityReport> {
        // ...
    }
}

pub struct LinkQualityReport {
    pub link_locator: Locator,
    pub peer_zid: ZenohId,
    pub rtt: Duration,
    pub jitter: Duration,
    pub packet_loss: f64,
    pub score: f64,
}
```

## Backward Compatibility

1. **Unknown OAM IDs**: Nodes without OAM support will log "unknown OAM type" and ignore the message (existing behavior in `rx.rs`)

2. **Feature-gated**: All OAM code is behind `#[cfg(feature = "transport_oam")]`

3. **Configuration**: OAM is disabled by default (`enabled: false`)

4. **Protocol Version**: No protocol version bump needed - OAM message type already exists

## Testing Strategy

1. **Unit Tests**
   - Payload serialization/deserialization
   - Jitter calculation
   - Score calculation
   - Metric aggregation

2. **Integration Tests**
   - Single link OAM probe exchange
   - Multi-link with quality-based selection
   - Link failure detection via OAM timeout
   - KeepAlive disabled when OAM enabled

3. **Performance Tests**
   - Overhead of OAM probing at various intervals
   - Memory usage for metrics storage
   - Lock contention on metrics access

## Link-Type Specific Considerations

This section provides guidance for deploying OAM across different physical media, each with unique characteristics affecting probe configuration and metrics interpretation.

### Link Type Characteristics Summary

| Link Type | Typical RTT | Jitter | Loss | Asymmetry | Clock Sync | Key Challenges |
|-----------|-------------|--------|------|-----------|------------|----------------|
| Fiber Optic | <1ms | Very low | Very low | Symmetric | Optional | Fiber cuts, amplifier issues |
| Ethernet (LAN) | <1ms | Low | Very low | Symmetric | Optional | Switch congestion, cable issues |
| Ethernet (WAN) | 10-100ms | Low-Medium | Low | Mostly symmetric | Recommended | Route changes, congestion |
| Radio (Tactical) | 10-500ms | High | Medium-High | Often asymmetric | Required | Interference, mobility, terrain |
| Satellite (GEO) | 500-700ms | Medium | Low-Medium | Asymmetric | Required | Fixed high latency, rain fade |
| Satellite (LEO) | 20-50ms | Medium-High | Low-Medium | Asymmetric | Required | Handovers, variable path |
| Acoustic (Underwater) | 1-10s | Very high | High | Asymmetric | Required | Extreme latency, multipath |

### Fiber Optic Links

**Characteristics:**
- Ultra-low latency (<1ms for short runs, proportional to distance at ~5μs/km)
- Extremely low jitter (sub-microsecond)
- Near-zero loss under normal conditions
- Symmetric delay (same fiber path both directions typically)
- Failures are typically binary (working or cut)

**Recommended Configuration:**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 50,        // Fast detection of fiber cuts
  probe_timeout_ms: 200,        // Tight timeout for low-latency link
  failure_threshold: 2,         // Quick failover on cut
  probe_types: ["loopback"],    // RTT sufficient, no asymmetry concerns
  scoring: {
    rtt: 1.0,
    jitter: 1.0,                // Low weight - jitter is naturally minimal
    loss: 20.0,                 // Any loss on fiber is concerning
  },
}
```

**Notes:**
- Clock sync optional - RTT-based metrics sufficient due to symmetric paths
- Focus on fast failure detection rather than quality gradation
- Consider using OAM to detect degraded optical power (future: integrate with transceiver diagnostics)

**Relevant Standards:**
- ITU-T G.709: OTN (Optical Transport Network) OAM
- IEEE 802.3 Clause 57: Ethernet OAM for point-to-point links

### Ethernet Links (LAN/WAN)

**Characteristics:**
- LAN: Sub-millisecond RTT, minimal jitter
- WAN: Variable RTT depending on hop count and geography
- Generally symmetric, but WAN can have asymmetric routing
- Loss typically due to congestion, not medium errors

**Recommended Configuration (LAN):**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 100,       // Standard interval
  probe_timeout_ms: 500,
  failure_threshold: 3,
  probe_types: ["loopback"],
  scoring: {
    rtt: 1.0,
    jitter: 2.0,
    loss: 10.0,
  },
}
```

**Recommended Configuration (WAN):**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 200,       // Less aggressive for WAN
  probe_timeout_ms: 1000,
  failure_threshold: 3,
  probe_types: ["loopback", "delay"],  // DMM for asymmetry detection
  scoring: {
    rtt: 1.0,
    jitter: 2.0,
    loss: 10.0,
    asymmetry: 1.0,             // Monitor for routing asymmetry
  },
}
```

**Notes:**
- LAN: Clock sync optional, RTT sufficient
- WAN: Clock sync recommended (NTP) for one-way delay to detect asymmetric routing issues
- Native Y.1731 support possible if switches support CFM

**Relevant Standards:**
- IEEE 802.1ag: Connectivity Fault Management
- ITU-T G.8013/Y.1731: Ethernet OAM
- IEEE 802.3ah: Ethernet in the First Mile (link OAM)

### Radio Links (Tactical/Mobile)

**Characteristics:**
- Highly variable latency (10-500ms depending on waveform, hops, crypto)
- High jitter due to channel access contention, ARQ retransmissions
- Packet loss from interference, jamming, mobility
- Often asymmetric (different TX power, antenna gains, or relay paths)
- Bandwidth often very limited (9.6kbps to few Mbps)

**Recommended Configuration:**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 1000,      // Conserve bandwidth
  probe_timeout_ms: 5000,       // Accommodate high/variable latency
  failure_threshold: 5,         // Tolerate intermittent loss
  sample_window: 30,            // Longer window for noisy link
  probe_types: ["loopback", "delay", "loss"],  // All metrics needed
  scoring: {
    rtt: 0.5,                   // RTT naturally high, don't over-penalize
    jitter: 3.0,                // Jitter indicates channel quality
    loss: 5.0,                  // Some loss is normal
    asymmetry: 2.0,             // Asymmetry common and significant
  },
}
```

**Notes:**
- **Clock sync required** - GPS time recommended (most tactical radios have GPS)
- One-way delay critical for detecting asymmetric degradation (e.g., one direction jammed)
- Consider probe size - some waveforms have small MTU
- May need to reduce probe frequency during high traffic to avoid self-congestion
- Link state detection important for mobility (see extended metrics below)

**Relevant Standards:**
- MIL-STD-188-220: Tactical radio interoperability
- STANAG 4677: Tactical data link quality metrics
- Link 16/JREAP: Joint tactical data links (for reference)

### Satellite Links

#### GEO (Geostationary) Satellites

**Characteristics:**
- Fixed high latency: ~540-640ms RTT (propagation delay dominant)
- Moderate jitter (mostly from ground segment processing)
- Loss from rain fade, sun outages, antenna pointing
- Asymmetric bandwidth typical (more downlink than uplink)
- Path delay is constant and predictable

**Recommended Configuration:**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 2000,      // Long interval due to high RTT
  probe_timeout_ms: 5000,       // Well above RTT
  failure_threshold: 3,
  probe_types: ["loopback", "delay", "loss"],
  scoring: {
    rtt: 0.1,                   // RTT is fixed, don't use for scoring
    jitter: 3.0,                // Jitter indicates ground segment issues
    loss: 8.0,                  // Rain fade causes burst loss
    asymmetry: 2.0,             // Uplink/downlink often different
  },
}
```

**Notes:**
- **Clock sync required** - GPS time (ground stations have GPS)
- RTT is constant (~600ms), so jitter and loss are the meaningful metrics
- Consider burst loss patterns from rain fade - may want specialized burst loss metric
- One-way delay helps identify uplink vs downlink issues

#### LEO (Low Earth Orbit) Satellites

**Characteristics:**
- Lower latency: 20-50ms RTT (Starlink, OneWeb, etc.)
- Higher jitter from satellite handovers (every few minutes)
- Path changes as satellites move overhead
- Doppler effects can affect link quality
- Constellation-dependent characteristics

**Recommended Configuration:**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 500,       // More responsive for handovers
  probe_timeout_ms: 2000,
  failure_threshold: 4,         // Tolerate handover disruption
  sample_window: 40,            // Longer window to smooth handovers
  probe_types: ["loopback", "delay", "loss"],
  scoring: {
    rtt: 1.0,                   // RTT varies with satellite position
    jitter: 2.5,                // Handovers cause jitter spikes
    loss: 6.0,
    asymmetry: 1.5,
  },
}
```

**Notes:**
- **Clock sync required** - GPS time
- Consider handover detection - rapid RTT change indicates satellite switch
- May want to track RTT trend (increasing RTT = satellite moving away)
- Ground station location affects metrics

**Relevant Standards (Satellite):**
- CCSDS 131.0-B: TM Synchronization and Channel Coding
- CCSDS 732.0-B: AOS Space Data Link Protocol
- DVB-S2 (ETSI EN 302 307): Satellite transmission
- 3GPP TR 38.821: NTN (Non-Terrestrial Networks) for 5G

### Acoustic Links (Underwater)

**Characteristics:**
- Extreme latency: 1-10+ seconds RTT (sound speed ~1500 m/s in water)
- Very high jitter from multipath propagation, thermoclines
- High packet loss from ambient noise, marine life, multipath fading
- Severely asymmetric possible (different transducer depths/orientations)
- Extremely limited bandwidth (typically 100 bps to 10 kbps)
- Half-duplex operation common

**Recommended Configuration:**
```json5
oam: {
  enabled: true,
  probe_interval_ms: 30000,     // 30 seconds - bandwidth is precious
  probe_timeout_ms: 60000,      // 60 seconds - accommodate extreme latency
  failure_threshold: 3,
  sample_window: 10,            // Fewer samples due to long intervals
  probe_types: ["loopback", "loss"],  // Skip DMM - clock sync unreliable
  scoring: {
    rtt: 0.2,                   // RTT highly variable, less useful
    jitter: 1.0,                // Jitter is informative but expected
    loss: 3.0,                  // High loss is normal, don't over-penalize
    asymmetry: 0.5,             // Hard to measure accurately
  },
}
```

**Notes:**
- **Clock sync challenging** - GPS doesn't work underwater; consider:
  - Pre-synchronized atomic clocks
  - Acoustic time sync protocols
  - Accept RTT-only metrics if clock sync unavailable
- Probe size critical - keep payloads minimal
- Consider adaptive probing - reduce rate when channel is poor
- Half-duplex: may need to coordinate probe timing with data traffic
- Multipath can cause duplicate packet reception - handle in SLM

**Relevant Standards:**
- JANUS (STANAG 4748): Underwater acoustic digital signaling standard
- NATO ANEP-87: Underwater communications
- IEEE 802.11-based underwater acoustic modems (emerging)

## Extended Metrics for Asymmetric Links

For radio, satellite, and acoustic links, extend the base metrics structure:

### Enhanced LinkQualityMetrics

```rust
pub struct LinkQualityMetrics {
    // === Existing RTT metrics ===
    pub rtt_current: Duration,
    pub rtt_min: Duration,
    pub rtt_max: Duration,
    pub rtt_avg: Duration,
    pub jitter: Duration,
    
    // === One-Way Delay (requires clock sync) ===
    /// Forward delay: local -> remote
    pub forward_delay: Option<Duration>,
    /// Reverse delay: remote -> local
    pub reverse_delay: Option<Duration>,
    /// Asymmetry ratio: |fwd - rev| / (fwd + rev), 0.0 = symmetric
    pub delay_asymmetry: Option<f64>,
    
    // === Packet Loss ===
    pub tx_probe_count: u64,
    pub rx_probe_count: u64,
    pub loss_ratio: f64,
    /// Forward loss ratio (if SLM supports directional)
    pub forward_loss_ratio: Option<f64>,
    /// Reverse loss ratio
    pub reverse_loss_ratio: Option<f64>,
    
    // === Link State ===
    pub state: LinkState,
    pub last_state_change: Instant,
    pub consecutive_failures: u32,
    
    // === Timestamps ===
    pub last_probe_sent: Instant,
    pub last_probe_received: Instant,
    
    // === Trend indicators (for LEO/mobile) ===
    /// RTT trend: positive = increasing (link degrading)
    pub rtt_trend: f64,
    /// True if rapid RTT change detected (possible handover)
    pub handover_detected: bool,
}

/// Link operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Normal operation - all metrics within thresholds
    Operational,
    /// Degraded - elevated loss or latency but still usable
    Degraded,
    /// Marginal - barely usable, prefer alternatives
    Marginal,
    /// Failed - no connectivity
    Failed,
    /// Handover - transitioning (LEO satellite, mobile radio)
    Handover,
    /// Unknown - insufficient data to determine state
    Unknown,
}
```

### Enhanced Scoring with Asymmetry

```rust
pub struct ScoringWeights {
    pub rtt: f64,
    pub jitter: f64,
    pub loss: f64,
    /// Weight for delay asymmetry penalty (default: 1.0)
    pub asymmetry: f64,
    /// Weight for link state penalty (default: 5.0)
    pub state: f64,
}

impl LinkQualityMetrics {
    pub fn score(&self, weights: &ScoringWeights) -> f64 {
        let mut score = 100.0;
        
        // RTT penalty (ms)
        score -= self.rtt_avg.as_secs_f64() * 1000.0 * weights.rtt;
        
        // Jitter penalty (ms)
        score -= self.jitter.as_secs_f64() * 1000.0 * weights.jitter;
        
        // Loss penalty (percentage)
        score -= self.loss_ratio * 100.0 * weights.loss;
        
        // Asymmetry penalty (0-1 scale, 1 = fully asymmetric)
        if let Some(asymmetry) = self.delay_asymmetry {
            score -= asymmetry * 50.0 * weights.asymmetry;
        }
        
        // State penalty
        let state_penalty = match self.state {
            LinkState::Operational => 0.0,
            LinkState::Degraded => 10.0,
            LinkState::Marginal => 30.0,
            LinkState::Handover => 20.0,
            LinkState::Failed => 100.0,
            LinkState::Unknown => 5.0,
        };
        score -= state_penalty * weights.state;
        
        score.max(0.0)
    }
}
```

### Link State Transitions

```rust
impl LinkQualityMetrics {
    /// Update link state based on current metrics
    pub fn update_state(&mut self, thresholds: &StateThresholds) {
        let new_state = if self.consecutive_failures >= thresholds.failure_count {
            LinkState::Failed
        } else if self.handover_detected {
            LinkState::Handover
        } else if self.loss_ratio > thresholds.marginal_loss 
               || self.rtt_avg > thresholds.marginal_rtt {
            LinkState::Marginal
        } else if self.loss_ratio > thresholds.degraded_loss
               || self.rtt_avg > thresholds.degraded_rtt
               || self.jitter > thresholds.degraded_jitter {
            LinkState::Degraded
        } else {
            LinkState::Operational
        };
        
        if new_state != self.state {
            self.state = new_state;
            self.last_state_change = Instant::now();
            tracing::info!("Link state changed to {:?}", new_state);
        }
    }
}

pub struct StateThresholds {
    pub failure_count: u32,         // Default: 3
    pub degraded_loss: f64,         // Default: 0.01 (1%)
    pub degraded_rtt: Duration,     // Default: link-type specific
    pub degraded_jitter: Duration,  // Default: link-type specific
    pub marginal_loss: f64,         // Default: 0.10 (10%)
    pub marginal_rtt: Duration,     // Default: link-type specific
}
```

## Clock Synchronization Requirements

### Summary by Link Type

| Link Type | Clock Sync | Method | Accuracy Needed |
|-----------|------------|--------|-----------------|
| Fiber Optic | Optional | NTP or none | N/A (RTT sufficient) |
| Ethernet LAN | Optional | NTP | N/A (RTT sufficient) |
| Ethernet WAN | Recommended | NTP | ~10ms |
| Radio Tactical | **Required** | GPS | ~1ms |
| Satellite GEO | **Required** | GPS | ~1ms |
| Satellite LEO | **Required** | GPS | ~1ms |
| Acoustic | Required* | Atomic/GPS pre-sync | ~10ms |

*Acoustic links cannot receive GPS underwater; pre-synchronization before deployment required.

### Clock Sync Configuration

```json5
oam: {
  enabled: true,
  // ... other config ...
  
  /// Clock synchronization settings for one-way delay measurement
  clock_sync: {
    /// Require synchronized clocks for DMM/DMR (default: false)
    /// If true and clocks not synced, DMM probes will not be sent
    required: false,
    
    /// Expected clock accuracy in milliseconds
    /// Used to determine if one-way delay measurements are meaningful
    accuracy_ms: 10,
    
    /// Clock source (informational, for logging/diagnostics)
    /// Options: "none", "ntp", "ptp", "gps", "atomic"
    source: "ntp",
  },
}
```

### Handling Unsynchronized Clocks

When clock sync is unavailable or unreliable:

```rust
impl DelayMeasurement {
    pub fn calculate_delays(
        &self, 
        t1: u64, t2: u64, t3: u64, t4: u64,
        clock_accuracy: Duration,
    ) -> DelayResult {
        let rtt = Duration::from_nanos(t4 - t1);
        
        // One-way delays only valid if clock accuracy < expected asymmetry
        let forward = Duration::from_nanos(t2.saturating_sub(t1));
        let reverse = Duration::from_nanos(t4.saturating_sub(t3));
        
        // If calculated one-way delay is less than clock accuracy,
        // the measurement is unreliable
        let one_way_valid = forward > clock_accuracy && reverse > clock_accuracy;
        
        if one_way_valid {
            DelayResult::Full { rtt, forward, reverse }
        } else {
            // Fall back to RTT-only, estimate symmetric delay
            DelayResult::RttOnly { 
                rtt, 
                estimated_one_way: rtt / 2,
            }
        }
    }
}
```

## Multi-Link Heterogeneous Configuration

When operating with multiple link types simultaneously (fiber + radio + satellite + acoustic), each link requires appropriate OAM parameters. The configuration supports per-link-type and per-link overrides.

### Configuration Hierarchy

```
Global OAM defaults
    └── Per-link-type config (preset)
            └── Per-link config (locator-specific)
```

### Multi-Link Configuration Structure

```json5
{
  transport: {
    unicast: {
      oam: {
        /// Global enable (can be overridden per-link)
        enabled: true,
        
        /// Default configuration (used when no specific config matches)
        defaults: {
          probe_interval_ms: 100,
          probe_timeout_ms: 500,
          failure_threshold: 3,
          probe_types: ["loopback"],
        },
        
        /// Per-link-type configurations (by transport protocol or preset name)
        link_types: {
          /// Match by transport protocol
          "tcp": { preset: "ethernet_lan" },
          "tls": { preset: "ethernet_wan" },
          "udp": { preset: "ethernet_lan" },
          "quic": { preset: "ethernet_wan" },
          
          /// Custom link types (matched by link metadata or locator pattern)
          "serial": { preset: "radio" },
          "unixpipe": { preset: "fiber" },  // Local IPC, treat as low-latency
        },
        
        /// Per-link configurations (by locator pattern, highest priority)
        links: [
          {
            /// Glob pattern matching locator
            pattern: "tcp/192.168.1.*:*",
            config: { preset: "ethernet_lan" },
          },
          {
            /// Satellite ground station
            pattern: "udp/10.0.100.*:*",
            config: {
              preset: "satellite_geo",
              /// Override specific values
              probe_interval_ms: 3000,
            },
          },
          {
            /// Tactical radio network
            pattern: "serial/ttyUSB*",
            config: {
              preset: "radio",
              probe_interval_ms: 2000,  // Very constrained bandwidth
            },
          },
          {
            /// Acoustic modem
            pattern: "serial/ttyACM*",
            config: { preset: "acoustic" },
          },
          {
            /// LEO satellite terminal
            pattern: "udp/10.0.200.*:*",
            config: { preset: "satellite_leo" },
          },
        ],
        
        /// Link selection strategy when multiple links available
        selection: {
          /// Strategy: "quality" (OAM-based), "priority" (manual), "round_robin"
          strategy: "quality",
          
          /// For "priority" strategy: ordered preference by link pattern
          priority_order: [
            "tcp/192.168.1.*:*",     // Prefer fiber/LAN
            "udp/10.0.200.*:*",      // Then LEO sat
            "udp/10.0.100.*:*",      // Then GEO sat
            "serial/ttyUSB*",        // Then radio
            "serial/ttyACM*",        // Acoustic as last resort
          ],
          
          /// Minimum score difference to trigger link switch (prevents flapping)
          hysteresis: 5.0,
          
          /// How often to re-evaluate link selection (ms)
          evaluation_interval_ms: 1000,
        },
      },
    },
  },
}
```

### Link Matching Logic

```rust
impl OamConfigResolver {
    /// Resolve OAM configuration for a specific link
    pub fn resolve(&self, locator: &Locator) -> OamConfig {
        // 1. Check per-link patterns (highest priority)
        for link_config in &self.links {
            if link_config.pattern.matches(locator) {
                return self.apply_preset_and_overrides(&link_config.config);
            }
        }
        
        // 2. Check per-link-type by transport protocol
        let protocol = locator.protocol();
        if let Some(type_config) = self.link_types.get(protocol) {
            return self.apply_preset_and_overrides(type_config);
        }
        
        // 3. Fall back to defaults
        self.defaults.clone()
    }
}
```

### Runtime Link Type Detection

For links where the physical medium isn't obvious from the locator (e.g., UDP could be LAN, WAN, or satellite), support runtime detection:

```rust
/// Hint about the physical link type, provided by configuration or detected
#[derive(Debug, Clone)]
pub enum LinkMedium {
    /// Auto-detect based on RTT measurements
    Auto,
    /// Explicit medium type
    Fiber,
    EthernetLan,
    EthernetWan,
    RadioTactical,
    RadioMobile,
    SatelliteGeo,
    SatelliteLeo,
    Acoustic,
    /// Custom with explicit parameters
    Custom(String),
}

impl LinkMedium {
    /// Detect medium type from initial probe results
    pub fn detect_from_rtt(rtt: Duration) -> Self {
        match rtt.as_millis() {
            0..=5 => LinkMedium::Fiber,           // Sub-5ms = local/fiber
            6..=50 => LinkMedium::EthernetLan,    // Typical LAN
            51..=150 => LinkMedium::EthernetWan,  // WAN or LEO sat
            151..=400 => LinkMedium::SatelliteLeo, // Could be LEO or long WAN
            401..=800 => LinkMedium::SatelliteGeo, // GEO range
            _ => LinkMedium::Acoustic,            // Extreme latency
        }
    }
}
```

## Modem Metrics Integration

Physical layer metrics from modems provide early warning of link degradation before OAM probes detect issues. This is particularly valuable for radio, satellite, and acoustic links.

### Modem Metrics Structure

```rust
/// Physical layer metrics reported by modem/transceiver
#[derive(Debug, Clone, Default)]
pub struct ModemMetrics {
    // === Signal Quality ===
    /// Signal-to-Noise Ratio in dB (higher = better)
    pub snr_db: Option<f64>,
    /// Received Signal Strength Indicator in dBm (less negative = stronger)
    pub rssi_dbm: Option<f64>,
    /// Signal quality percentage (0-100, modem-specific interpretation)
    pub signal_quality: Option<f64>,
    
    // === Error Rates ===
    /// Bit Error Rate (0.0 to 1.0)
    pub ber: Option<f64>,
    /// Packet Error Rate (0.0 to 1.0)
    pub per: Option<f64>,
    /// Frame Error Rate (0.0 to 1.0)
    pub fer: Option<f64>,
    
    // === Link Parameters ===
    /// Current data rate in bits per second
    pub data_rate_bps: Option<u64>,
    /// Maximum supported data rate
    pub max_data_rate_bps: Option<u64>,
    /// Modulation scheme (e.g., "QPSK", "16QAM", "BPSK")
    pub modulation: Option<String>,
    /// Forward Error Correction rate (e.g., "1/2", "3/4")
    pub fec_rate: Option<String>,
    
    // === Modem State ===
    /// Modem operational state
    pub state: ModemState,
    /// Time since last state change
    pub state_duration: Duration,
    
    // === Satellite-Specific ===
    /// Elevation angle in degrees (for satellite)
    pub elevation_deg: Option<f64>,
    /// Azimuth angle in degrees
    pub azimuth_deg: Option<f64>,
    /// Doppler shift in Hz
    pub doppler_hz: Option<f64>,
    
    // === Radio-Specific ===
    /// Transmit power in dBm
    pub tx_power_dbm: Option<f64>,
    /// Frequency in Hz
    pub frequency_hz: Option<u64>,
    /// Channel bandwidth in Hz
    pub bandwidth_hz: Option<u64>,
    
    // === Acoustic-Specific ===
    /// Multipath spread in milliseconds
    pub multipath_spread_ms: Option<f64>,
    /// Doppler spread in Hz
    pub doppler_spread_hz: Option<f64>,
    
    // === Timestamps ===
    /// When metrics were last updated
    pub last_updated: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModemState {
    #[default]
    Unknown,
    Initializing,
    Searching,      // Looking for signal/satellite
    Synchronizing,  // Acquiring sync
    Connected,      // Normal operation
    Degraded,       // Connected but poor quality
    HandoverPending,// About to switch (LEO sat)
    Disconnected,
    Error,
}
```

### Modem Metrics Provider Interface

```rust
/// Trait for modem metrics providers
/// Implementations can be:
/// - Direct serial/USB communication with modem
/// - SNMP polling
/// - Modem vendor API
/// - Sysfs/procfs for Linux drivers
/// - External monitoring agent via IPC
#[async_trait]
pub trait ModemMetricsProvider: Send + Sync {
    /// Get current metrics from the modem
    async fn get_metrics(&self) -> ZResult<ModemMetrics>;
    
    /// Get modem type/model identifier
    fn modem_type(&self) -> &str;
    
    /// Subscribe to metric change events (optional)
    async fn subscribe(&self) -> Option<broadcast::Receiver<ModemMetrics>> {
        None
    }
}

/// Factory for creating modem metrics providers
pub struct ModemMetricsProviderFactory;

impl ModemMetricsProviderFactory {
    pub fn create(config: &ModemConfig) -> ZResult<Box<dyn ModemMetricsProvider>> {
        match config.provider_type.as_str() {
            "snmp" => Ok(Box::new(SnmpModemProvider::new(&config.connection)?)),
            "serial" => Ok(Box::new(SerialModemProvider::new(&config.connection)?)),
            "iridium" => Ok(Box::new(IridiumModemProvider::new(&config.connection)?)),
            "starlink" => Ok(Box::new(StarlinkProvider::new(&config.connection)?)),
            "evologics" => Ok(Box::new(EvologicsAcousticProvider::new(&config.connection)?)),
            "harris" => Ok(Box::new(HarrisRadioProvider::new(&config.connection)?)),
            _ => Err(zerror!("Unknown modem provider type: {}", config.provider_type)),
        }
    }
}
```

### Combined Scoring with Modem Metrics

```rust
pub struct CombinedScoringWeights {
    // OAM-based weights
    pub rtt: f64,
    pub jitter: f64,
    pub loss: f64,
    pub asymmetry: f64,
    pub state: f64,
    
    // Modem-based weights
    pub snr: f64,           // Default: 2.0
    pub signal_strength: f64, // Default: 1.0
    pub error_rate: f64,    // Default: 5.0
    pub modem_state: f64,   // Default: 10.0
}

impl LinkQualityMetrics {
    pub fn combined_score(
        &self,
        modem: &ModemMetrics,
        weights: &CombinedScoringWeights,
    ) -> f64 {
        let mut score = 100.0;
        
        // === OAM-based penalties ===
        score -= self.rtt_avg.as_secs_f64() * 1000.0 * weights.rtt;
        score -= self.jitter.as_secs_f64() * 1000.0 * weights.jitter;
        score -= self.loss_ratio * 100.0 * weights.loss;
        
        if let Some(asymmetry) = self.delay_asymmetry {
            score -= asymmetry * 50.0 * weights.asymmetry;
        }
        
        // === Modem-based penalties ===
        
        // SNR penalty (lower SNR = worse)
        // Normalize: 30dB+ = excellent, <10dB = poor
        if let Some(snr) = modem.snr_db {
            let snr_penalty = ((30.0 - snr).max(0.0) / 30.0) * 20.0;
            score -= snr_penalty * weights.snr;
        }
        
        // Signal strength penalty
        // Normalize: -50dBm = excellent, -100dBm = poor
        if let Some(rssi) = modem.rssi_dbm {
            let rssi_penalty = ((-50.0 - rssi).max(0.0) / 50.0) * 20.0;
            score -= rssi_penalty * weights.signal_strength;
        }
        
        // Error rate penalty (BER)
        if let Some(ber) = modem.ber {
            // BER of 1e-6 is good, 1e-3 is poor
            let ber_penalty = (ber.log10() + 6.0).max(0.0) * 10.0;
            score -= ber_penalty * weights.error_rate;
        }
        
        // Modem state penalty
        let modem_state_penalty = match modem.state {
            ModemState::Connected => 0.0,
            ModemState::Degraded => 15.0,
            ModemState::HandoverPending => 10.0,
            ModemState::Synchronizing => 20.0,
            ModemState::Searching => 40.0,
            ModemState::Initializing => 30.0,
            ModemState::Disconnected => 100.0,
            ModemState::Error => 100.0,
            ModemState::Unknown => 5.0,
        };
        score -= modem_state_penalty * weights.modem_state;
        
        // === Satellite-specific adjustments ===
        
        // Low elevation = longer path, more atmospheric effects
        if let Some(elevation) = modem.elevation_deg {
            if elevation < 20.0 {
                score -= (20.0 - elevation) * 0.5;  // Penalty for low elevation
            }
        }
        
        // High Doppler = satellite moving fast (LEO), possible handover soon
        if let Some(doppler) = modem.doppler_hz {
            if doppler.abs() > 10000.0 {
                score -= 5.0;  // Minor penalty for high Doppler
            }
        }
        
        score.max(0.0)
    }
}
```

### Modem Configuration

```json5
{
  transport: {
    unicast: {
      oam: {
        enabled: true,
        
        /// Modem metrics integration
        modem_metrics: {
          /// Enable modem metrics in scoring (default: false)
          enabled: true,
          
          /// Poll interval in milliseconds (default: 1000)
          poll_interval_ms: 1000,
          
          /// Scoring weights for modem metrics
          scoring: {
            snr: 2.0,
            signal_strength: 1.0,
            error_rate: 5.0,
            modem_state: 10.0,
          },
          
          /// Per-link modem configurations
          modems: [
            {
              /// Link pattern this modem serves
              link_pattern: "udp/10.0.100.*:*",
              /// Modem provider type
              provider: "snmp",
              /// Connection string (provider-specific)
              connection: "udp/192.168.1.50:161",
              /// SNMP community (for SNMP provider)
              options: {
                community: "public",
                oid_prefix: "1.3.6.1.4.1.example.modem",
              },
            },
            {
              link_pattern: "serial/ttyUSB0",
              provider: "harris",
              connection: "serial:///dev/ttyUSB1:9600",
              options: {
                model: "PRC-152A",
              },
            },
            {
              link_pattern: "udp/10.0.200.*:*",
              provider: "starlink",
              connection: "http://192.168.100.1/api",
            },
            {
              link_pattern: "serial/ttyACM0",
              provider: "evologics",
              connection: "serial:///dev/ttyACM1:19200",
            },
          ],
        },
      },
    },
  },
}
```

## Cross-Link Score Normalization

When heterogeneous links (fiber, satellite, acoustic) compete for selection, raw scoring is unfair: a "perfect" acoustic link will always lose to a "degraded" fiber link. Normalization addresses this.

### Normalization Strategy

Use **per-link-type baseline normalization** combined with **effective bandwidth weighting**:

```rust
/// Baseline expectations per link type
/// These represent "typical good" values for each medium
#[derive(Debug, Clone)]
pub struct LinkTypeBaseline {
    /// Expected RTT for a healthy link of this type
    pub typical_rtt: Duration,
    /// Maximum acceptable RTT before considered degraded
    pub max_acceptable_rtt: Duration,
    /// Expected jitter
    pub typical_jitter: Duration,
    /// Expected loss rate (0.0 to 1.0)
    pub typical_loss: f64,
    /// Maximum theoretical bandwidth (bits per second)
    pub max_bandwidth_bps: u64,
    /// Typical available bandwidth
    pub typical_bandwidth_bps: u64,
}

impl LinkTypeBaseline {
    pub fn fiber() -> Self {
        Self {
            typical_rtt: Duration::from_micros(500),
            max_acceptable_rtt: Duration::from_millis(5),
            typical_jitter: Duration::from_micros(50),
            typical_loss: 0.0,
            max_bandwidth_bps: 100_000_000_000,  // 100 Gbps
            typical_bandwidth_bps: 10_000_000_000, // 10 Gbps
        }
    }
    
    pub fn ethernet_lan() -> Self {
        Self {
            typical_rtt: Duration::from_millis(1),
            max_acceptable_rtt: Duration::from_millis(10),
            typical_jitter: Duration::from_micros(200),
            typical_loss: 0.0,
            max_bandwidth_bps: 10_000_000_000,   // 10 Gbps
            typical_bandwidth_bps: 1_000_000_000, // 1 Gbps
        }
    }
    
    pub fn radio_tactical() -> Self {
        Self {
            typical_rtt: Duration::from_millis(100),
            max_acceptable_rtt: Duration::from_millis(500),
            typical_jitter: Duration::from_millis(30),
            typical_loss: 0.02,  // 2% typical
            max_bandwidth_bps: 2_000_000,        // 2 Mbps
            typical_bandwidth_bps: 256_000,       // 256 kbps
        }
    }
    
    pub fn satellite_geo() -> Self {
        Self {
            typical_rtt: Duration::from_millis(600),
            max_acceptable_rtt: Duration::from_millis(800),
            typical_jitter: Duration::from_millis(10),
            typical_loss: 0.01,
            max_bandwidth_bps: 100_000_000,      // 100 Mbps
            typical_bandwidth_bps: 20_000_000,    // 20 Mbps
        }
    }
    
    pub fn satellite_leo() -> Self {
        Self {
            typical_rtt: Duration::from_millis(40),
            max_acceptable_rtt: Duration::from_millis(100),
            typical_jitter: Duration::from_millis(15),
            typical_loss: 0.005,
            max_bandwidth_bps: 500_000_000,      // 500 Mbps (Starlink-class)
            typical_bandwidth_bps: 100_000_000,   // 100 Mbps
        }
    }
    
    pub fn acoustic() -> Self {
        Self {
            typical_rtt: Duration::from_secs(2),
            max_acceptable_rtt: Duration::from_secs(10),
            typical_jitter: Duration::from_millis(500),
            typical_loss: 0.10,  // 10% typical
            max_bandwidth_bps: 30_000,           // 30 kbps
            typical_bandwidth_bps: 5_000,         // 5 kbps
        }
    }
}
```

### Normalized Scoring Algorithm

```rust
impl LinkQualityMetrics {
    /// Calculate normalized score (0-100) relative to link type baseline
    /// A score of 100 = performing at or better than typical
    /// A score of 0 = performing at maximum acceptable threshold or worse
    pub fn normalized_score(
        &self,
        baseline: &LinkTypeBaseline,
        current_bandwidth_bps: Option<u64>,
        weights: &NormalizedScoringWeights,
    ) -> f64 {
        let mut score = 100.0;
        
        // === RTT normalization ===
        // Score based on how close to typical vs max_acceptable
        let rtt_ratio = if self.rtt_avg <= baseline.typical_rtt {
            0.0  // At or better than typical = no penalty
        } else {
            let range = baseline.max_acceptable_rtt.as_secs_f64() 
                      - baseline.typical_rtt.as_secs_f64();
            let excess = self.rtt_avg.as_secs_f64() 
                       - baseline.typical_rtt.as_secs_f64();
            (excess / range).min(1.0)  // 0.0 to 1.0
        };
        score -= rtt_ratio * 30.0 * weights.rtt;
        
        // === Jitter normalization ===
        let jitter_ratio = if self.jitter <= baseline.typical_jitter {
            0.0
        } else {
            // Jitter > 3x typical is considered bad
            let max_jitter = baseline.typical_jitter * 3;
            ((self.jitter.as_secs_f64() - baseline.typical_jitter.as_secs_f64())
                / (max_jitter.as_secs_f64() - baseline.typical_jitter.as_secs_f64()))
                .min(1.0)
        };
        score -= jitter_ratio * 20.0 * weights.jitter;
        
        // === Loss normalization ===
        let loss_ratio = if self.loss_ratio <= baseline.typical_loss {
            0.0
        } else {
            // Loss > 3x typical is considered bad
            let max_loss = (baseline.typical_loss * 3.0).min(0.5);
            ((self.loss_ratio - baseline.typical_loss) / (max_loss - baseline.typical_loss))
                .min(1.0)
        };
        score -= loss_ratio * 30.0 * weights.loss;
        
        // === Bandwidth factor (bonus/penalty) ===
        // Higher bandwidth links get a bonus when performing well
        if let Some(current_bw) = current_bandwidth_bps {
            let bw_utilization = current_bw as f64 / baseline.typical_bandwidth_bps as f64;
            if bw_utilization >= 0.8 {
                // Performing at 80%+ of typical bandwidth = bonus
                score += 10.0 * weights.bandwidth;
            } else if bw_utilization < 0.3 {
                // Performing at <30% of typical = penalty
                score -= 10.0 * weights.bandwidth;
            }
            
            // Absolute bandwidth bonus for high-capacity links
            // Encourages using fiber over acoustic when both are healthy
            let capacity_bonus = (baseline.typical_bandwidth_bps as f64).log10() - 3.0; // log10(1kbps) = 3
            score += capacity_bonus * weights.capacity;
        }
        
        score.max(0.0).min(100.0)
    }
}

pub struct NormalizedScoringWeights {
    pub rtt: f64,       // Default: 1.0
    pub jitter: f64,    // Default: 1.0
    pub loss: f64,      // Default: 1.0
    pub bandwidth: f64, // Default: 1.0 - current bandwidth utilization
    pub capacity: f64,  // Default: 0.5 - absolute capacity preference
}
```

### Selection Strategies with Normalization

```rust
pub enum LinkSelectionStrategy {
    /// Always pick highest absolute score (fiber wins if healthy)
    /// Use when: low latency is critical, bandwidth is abundant
    AbsoluteQuality,
    
    /// Pick highest normalized score (best-performing relative to type)
    /// Use when: all links matter, want to use each at its best
    NormalizedQuality,
    
    /// Pick link with best bandwidth * quality product
    /// Use when: throughput matters, want to maximize effective capacity
    BandwidthWeighted,
    
    /// Normalized quality, but with minimum bandwidth threshold
    /// Use when: need minimum throughput, otherwise prefer best quality
    BandwidthConstrained { min_bps: u64 },
    
    /// Manual priority order, skip failed/degraded links
    /// Use when: explicit control needed, OAM only for failure detection
    PriorityWithFailover,
    
    /// Weighted random based on scores (load distribution)
    /// Use when: want to utilize multiple links proportionally
    WeightedDistribution,
}

impl LinkSelector {
    pub fn select(
        &self,
        links: &[LinkWithMetrics],
        strategy: &LinkSelectionStrategy,
        message_size: usize,
    ) -> Option<usize> {
        match strategy {
            LinkSelectionStrategy::BandwidthWeighted => {
                // Score = normalized_quality * effective_bandwidth
                links.iter()
                    .enumerate()
                    .filter(|(_, l)| l.state != LinkState::Failed)
                    .max_by(|(_, a), (_, b)| {
                        let score_a = a.normalized_score() * a.current_bandwidth_bps as f64;
                        let score_b = b.normalized_score() * b.current_bandwidth_bps as f64;
                        score_a.partial_cmp(&score_b).unwrap_or(Ordering::Equal)
                    })
                    .map(|(idx, _)| idx)
            }
            
            LinkSelectionStrategy::BandwidthConstrained { min_bps } => {
                // Filter by minimum bandwidth, then pick best normalized
                links.iter()
                    .enumerate()
                    .filter(|(_, l)| {
                        l.state != LinkState::Failed 
                        && l.current_bandwidth_bps >= *min_bps
                    })
                    .max_by(|(_, a), (_, b)| {
                        a.normalized_score().partial_cmp(&b.normalized_score())
                            .unwrap_or(Ordering::Equal)
                    })
                    .map(|(idx, _)| idx)
                    // Fallback: if no link meets bandwidth, pick best available
                    .or_else(|| {
                        links.iter()
                            .enumerate()
                            .filter(|(_, l)| l.state != LinkState::Failed)
                            .max_by(|(_, a), (_, b)| {
                                a.normalized_score().partial_cmp(&b.normalized_score())
                                    .unwrap_or(Ordering::Equal)
                            })
                            .map(|(idx, _)| idx)
                    })
            }
            
            // ... other strategies
            _ => todo!()
        }
    }
}
```

### Configuration for Normalization

```json5
oam: {
  scoring: {
    /// Scoring mode: "absolute" or "normalized" (default: "absolute")
    mode: "normalized",
    
    /// Weights for normalized scoring
    normalized_weights: {
      rtt: 1.0,
      jitter: 1.0,
      loss: 1.0,
      bandwidth: 1.0,      // Current bandwidth utilization factor
      capacity: 0.5,       // Absolute capacity preference (0 = ignore capacity)
    },
    
    /// Link selection strategy
    selection_strategy: "bandwidth_weighted",
    
    /// For "bandwidth_constrained" strategy
    min_bandwidth_bps: 100000,  // 100 kbps minimum
  },
  
  /// Per-link-type baseline overrides (optional)
  /// Use if your links differ from defaults
  baselines: {
    "radio": {
      typical_rtt_ms: 150,
      max_acceptable_rtt_ms: 800,
      typical_jitter_ms: 50,
      typical_loss_percent: 3.0,
      typical_bandwidth_bps: 128000,
    },
    "acoustic": {
      typical_rtt_ms: 3000,
      max_acceptable_rtt_ms: 15000,
      typical_loss_percent: 15.0,
      typical_bandwidth_bps: 2400,  // 2.4 kbps modem
    },
  },
}
```

## Modem-Zenoh Communication Architecture

Since you control both the modem drivers and Zenoh, here are the recommended integration patterns:

### Option 1: Shared Memory (Recommended for Low Latency)

Best for: Real-time metrics, same-host deployment

```rust
/// Shared memory structure for modem metrics
/// Modem driver writes, Zenoh OAM reads
#[repr(C)]
pub struct SharedModemMetrics {
    /// Magic number for validation
    pub magic: u32,                    // 0x4D4F444D = "MODM"
    /// Version for compatibility
    pub version: u16,
    /// Sequence number (incremented on each update)
    pub seq: AtomicU64,
    /// Timestamp of last update (Unix epoch nanoseconds)
    pub timestamp_ns: AtomicU64,
    
    // === Signal metrics (scaled integers for atomic access) ===
    /// SNR in dB * 100 (e.g., 2550 = 25.50 dB)
    pub snr_cdb: AtomicI32,
    /// RSSI in dBm * 100
    pub rssi_cdbm: AtomicI32,
    /// BER as -log10(ber) * 100 (e.g., 600 = 10^-6)
    pub ber_log_scaled: AtomicI32,
    
    // === Bandwidth metrics ===
    /// Current TX rate in bps
    pub tx_rate_bps: AtomicU64,
    /// Current RX rate in bps
    pub rx_rate_bps: AtomicU64,
    /// Maximum available bandwidth in bps
    pub max_bandwidth_bps: AtomicU64,
    
    // === State ===
    /// ModemState as u8
    pub state: AtomicU8,
    /// Elevation in degrees * 100 (for satellite)
    pub elevation_cdeg: AtomicI16,
    /// Doppler in Hz
    pub doppler_hz: AtomicI32,
    
    // Padding for cache line alignment
    _pad: [u8; 32],
}

impl SharedModemMetrics {
    pub const SHM_NAME: &'static str = "/zenoh_modem_metrics";
    pub const MAGIC: u32 = 0x4D4F444D;
    
    /// Read metrics with sequence number check (detect torn reads)
    pub fn read(&self) -> Option<ModemMetrics> {
        let seq1 = self.seq.load(Ordering::Acquire);
        
        // Read all fields
        let metrics = ModemMetrics {
            snr_db: Some(self.snr_cdb.load(Ordering::Relaxed) as f64 / 100.0),
            rssi_dbm: Some(self.rssi_cdbm.load(Ordering::Relaxed) as f64 / 100.0),
            // ... other fields
            last_updated: Instant::now(),
        };
        
        let seq2 = self.seq.load(Ordering::Acquire);
        
        // If sequence changed during read, data may be inconsistent
        if seq1 == seq2 {
            Some(metrics)
        } else {
            None  // Retry
        }
    }
}

// Modem driver side (C-compatible for driver integration)
#[no_mangle]
pub extern "C" fn zenoh_modem_update_snr(shm: *mut SharedModemMetrics, snr_db: f64) {
    unsafe {
        let metrics = &*shm;
        metrics.seq.fetch_add(1, Ordering::Release);
        metrics.snr_cdb.store((snr_db * 100.0) as i32, Ordering::Relaxed);
        metrics.timestamp_ns.store(
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64,
            Ordering::Relaxed
        );
        metrics.seq.fetch_add(1, Ordering::Release);
    }
}
```

Configuration:
```json5
modem_metrics: {
  provider: "shm",
  connection: "/zenoh_modem_metrics_radio0",  // shm name
}
```

### Option 2: Unix Domain Socket (Recommended for Isolation)

Best for: Separate processes, structured updates, bidirectional communication

```rust
/// Message protocol over Unix socket
#[derive(Serialize, Deserialize)]
pub enum ModemMessage {
    /// Periodic metrics update from modem
    MetricsUpdate(ModemMetrics),
    
    /// Modem state change notification
    StateChange { old: ModemState, new: ModemState },
    
    /// Bandwidth availability update
    BandwidthUpdate { 
        available_bps: u64, 
        max_bps: u64,
        /// Estimated queue depth in bytes
        queue_depth: u32,
    },
    
    /// Predictive alert from modem
    Alert(ModemAlert),
    
    /// Request from Zenoh to modem
    Request(ZenohRequest),
    
    /// Response from modem to Zenoh
    Response(ModemResponse),
}

#[derive(Serialize, Deserialize)]
pub enum ModemAlert {
    /// Handover starting (LEO satellite)
    HandoverStarting { estimated_duration_ms: u32 },
    /// Signal degrading
    SignalDegrading { current_snr: f64, trend: f64 },
    /// Link about to fail
    LinkFailureImminent { reason: String },
    /// Bandwidth reduction imminent
    BandwidthReduction { current_bps: u64, expected_bps: u64 },
}

#[derive(Serialize, Deserialize)]
pub enum ZenohRequest {
    /// Request current metrics
    GetMetrics,
    /// Request modem to adjust power/rate for reliability
    RequestReliableMode,
    /// Request modem to maximize throughput
    RequestThroughputMode,
    /// Hint about expected traffic pattern
    TrafficHint { expected_bps: u64, priority: Priority },
}

/// Modem driver daemon
async fn modem_daemon(socket_path: &str, modem: impl Modem) -> ZResult<()> {
    let listener = UnixListener::bind(socket_path)?;
    
    loop {
        let (stream, _) = listener.accept().await?;
        let modem = modem.clone();
        
        tokio::spawn(async move {
            let mut stream = BufReader::new(stream);
            
            // Send initial metrics
            let metrics = modem.get_metrics().await;
            send_message(&mut stream, ModemMessage::MetricsUpdate(metrics)).await;
            
            // Periodic updates + event handling
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let metrics = modem.get_metrics().await;
                        send_message(&mut stream, ModemMessage::MetricsUpdate(metrics)).await;
                    }
                    
                    // Handle alerts from modem hardware
                    alert = modem.next_alert() => {
                        send_message(&mut stream, ModemMessage::Alert(alert)).await;
                    }
                    
                    // Handle requests from Zenoh
                    msg = read_message(&mut stream) => {
                        if let ModemMessage::Request(req) = msg {
                            let resp = modem.handle_request(req).await;
                            send_message(&mut stream, ModemMessage::Response(resp)).await;
                        }
                    }
                }
            }
        });
    }
}
```

Configuration:
```json5
modem_metrics: {
  provider: "unix_socket",
  connection: "/run/zenoh/modem_radio0.sock",
  /// Enable bidirectional communication (Zenoh can send hints to modem)
  bidirectional: true,
}
```

### Option 3: Zenoh Pub/Sub (Recommended for Distributed)

Best for: Modems on remote systems, fleet monitoring, debugging

```rust
/// Modem publishes metrics to Zenoh key expressions
/// Multiple Zenoh nodes can subscribe to same modem metrics

// Modem driver publishes to:
//   modem/{modem_id}/metrics      - periodic metrics
//   modem/{modem_id}/state        - state changes
//   modem/{modem_id}/alerts       - predictive alerts
//   modem/{modem_id}/bandwidth    - bandwidth updates

// Zenoh OAM subscribes to relevant modem topics

async fn modem_zenoh_publisher(session: Session, modem_id: &str, modem: impl Modem) {
    let metrics_key = format!("modem/{}/metrics", modem_id);
    let state_key = format!("modem/{}/state", modem_id);
    let alerts_key = format!("modem/{}/alerts", modem_id);
    
    let publisher = session.declare_publisher(&metrics_key).await.unwrap();
    
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let mut last_state = ModemState::Unknown;
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let metrics = modem.get_metrics().await;
                let payload = serde_json::to_vec(&metrics).unwrap();
                publisher.put(payload).await.unwrap();
                
                // Publish state changes
                if metrics.state != last_state {
                    session.put(&state_key, format!("{:?}", metrics.state)).await.unwrap();
                    last_state = metrics.state;
                }
            }
            
            alert = modem.next_alert() => {
                let payload = serde_json::to_vec(&alert).unwrap();
                session.put(&alerts_key, payload).await.unwrap();
            }
        }
    }
}
```

Configuration:
```json5
modem_metrics: {
  provider: "zenoh",
  /// Key expression to subscribe to
  key_expr: "modem/radio0/**",
  /// Optional: specific session config for modem network
  session_config: {
    connect: { endpoints: ["tcp/modem-gateway:7447"] },
  },
}
```

### Option 4: Direct Driver Integration (Lowest Latency)

Best for: When modem driver is a Rust library you can link directly

```rust
/// Modem driver exposes a trait, Zenoh links directly
pub trait ModemDriver: Send + Sync + 'static {
    /// Get current metrics (non-blocking, returns cached)
    fn metrics(&self) -> ModemMetrics;
    
    /// Subscribe to state changes
    fn subscribe_state(&self) -> broadcast::Receiver<ModemState>;
    
    /// Subscribe to alerts
    fn subscribe_alerts(&self) -> broadcast::Receiver<ModemAlert>;
    
    /// Get current bandwidth availability
    fn available_bandwidth(&self) -> u64;
    
    /// Hint to modem about expected traffic
    fn traffic_hint(&self, expected_bps: u64, priority: Priority);
}

// In Zenoh transport initialization:
impl TransportLinkUnicast {
    pub fn with_modem_driver(
        self, 
        driver: Arc<dyn ModemDriver>
    ) -> Self {
        Self {
            modem_driver: Some(driver),
            ..self
        }
    }
}
```

Configuration:
```json5
modem_metrics: {
  provider: "driver",
  /// Path to shared library containing driver
  driver_path: "/usr/lib/zenoh/modems/libharris_radio.so",
  /// Driver-specific configuration
  driver_config: {
    device: "/dev/ttyUSB0",
    baud_rate: 115200,
  },
}
```

### Recommendation Summary

| Approach | Latency | Isolation | Complexity | Best For |
|----------|---------|-----------|------------|----------|
| Shared Memory | ~1μs | Low | Medium | Same-host, real-time |
| Unix Socket | ~50μs | High | Medium | Same-host, robust |
| Zenoh Pub/Sub | ~1ms | High | Low | Distributed, monitoring |
| Direct Driver | ~0.1μs | None | High | Embedded, single binary |

**My recommendation for your use case**: 

1. **Primary**: Unix Domain Socket for production
   - Good isolation between modem driver and Zenoh
   - Bidirectional (Zenoh can request reliable mode during critical data)
   - Easy to debug/monitor traffic
   - Survives Zenoh restarts

2. **Secondary**: Zenoh Pub/Sub for monitoring/debugging
   - Publish metrics to Zenoh for logging/visualization
   - Allows central monitoring of all modems

3. **Future**: Direct driver integration for embedded deployments
   - Single binary, lowest latency
   - When modem code is stable

### Predictive Link Quality

```rust
impl LinkQualityPredictor {
    /// Predict link quality trend based on modem metrics
    pub fn predict_trend(&self, modem: &ModemMetrics) -> QualityTrend {
        let mut trend = QualityTrend::Stable;
        
        // Satellite elevation trending down = link will degrade
        if let Some(elevation) = modem.elevation_deg {
            if elevation < 15.0 {
                trend = QualityTrend::Degrading;
            }
        }
        
        // LEO handover pending
        if modem.state == ModemState::HandoverPending {
            trend = QualityTrend::HandoverImminent;
        }
        
        // High Doppler on LEO = satellite passing, handover soon
        if let Some(doppler) = modem.doppler_hz {
            if doppler.abs() > 20000.0 {
                trend = QualityTrend::Degrading;
            }
        }
        
        // SNR trending down (compare with historical)
        if let Some(snr) = modem.snr_db {
            if snr < self.snr_history.average() - 3.0 {
                trend = QualityTrend::Degrading;
            }
        }
        
        trend
    }
}

#[derive(Debug, Clone, Copy)]
pub enum QualityTrend {
    Improving,
    Stable,
    Degrading,
    HandoverImminent,
    FailureImminent,
}
```

## Configuration Presets

```json5
oam: {
  enabled: true,
  
  /// Use a preset configuration (overrides individual settings)
  /// Options: "fiber", "ethernet_lan", "ethernet_wan", "radio", 
  ///          "satellite_geo", "satellite_leo", "acoustic"
  preset: "radio",
  
  // Individual settings can still override preset values
  probe_interval_ms: 2000,  // Override preset's default
}
```

### Preset Definitions

```rust
pub fn get_preset(name: &str) -> Option<OamConfig> {
    match name {
        "fiber" => Some(OamConfig {
            probe_interval_ms: 50,
            probe_timeout_ms: 200,
            failure_threshold: 2,
            probe_types: vec![ProbeType::Loopback],
            scoring: ScoringWeights {
                rtt: 1.0, jitter: 1.0, loss: 20.0, asymmetry: 0.0, state: 5.0,
            },
            ..Default::default()
        }),
        "ethernet_lan" => Some(OamConfig {
            probe_interval_ms: 100,
            probe_timeout_ms: 500,
            failure_threshold: 3,
            probe_types: vec![ProbeType::Loopback],
            scoring: ScoringWeights {
                rtt: 1.0, jitter: 2.0, loss: 10.0, asymmetry: 0.0, state: 5.0,
            },
            ..Default::default()
        }),
        "ethernet_wan" => Some(OamConfig {
            probe_interval_ms: 200,
            probe_timeout_ms: 1000,
            failure_threshold: 3,
            probe_types: vec![ProbeType::Loopback, ProbeType::Delay],
            scoring: ScoringWeights {
                rtt: 1.0, jitter: 2.0, loss: 10.0, asymmetry: 1.0, state: 5.0,
            },
            clock_sync: ClockSyncConfig { required: false, accuracy_ms: 10, source: "ntp".into() },
            ..Default::default()
        }),
        "radio" => Some(OamConfig {
            probe_interval_ms: 1000,
            probe_timeout_ms: 5000,
            failure_threshold: 5,
            sample_window: 30,
            probe_types: vec![ProbeType::Loopback, ProbeType::Delay, ProbeType::Loss],
            scoring: ScoringWeights {
                rtt: 0.5, jitter: 3.0, loss: 5.0, asymmetry: 2.0, state: 5.0,
            },
            clock_sync: ClockSyncConfig { required: true, accuracy_ms: 1, source: "gps".into() },
            ..Default::default()
        }),
        "satellite_geo" => Some(OamConfig {
            probe_interval_ms: 2000,
            probe_timeout_ms: 5000,
            failure_threshold: 3,
            probe_types: vec![ProbeType::Loopback, ProbeType::Delay, ProbeType::Loss],
            scoring: ScoringWeights {
                rtt: 0.1, jitter: 3.0, loss: 8.0, asymmetry: 2.0, state: 5.0,
            },
            clock_sync: ClockSyncConfig { required: true, accuracy_ms: 1, source: "gps".into() },
            ..Default::default()
        }),
        "satellite_leo" => Some(OamConfig {
            probe_interval_ms: 500,
            probe_timeout_ms: 2000,
            failure_threshold: 4,
            sample_window: 40,
            probe_types: vec![ProbeType::Loopback, ProbeType::Delay, ProbeType::Loss],
            scoring: ScoringWeights {
                rtt: 1.0, jitter: 2.5, loss: 6.0, asymmetry: 1.5, state: 5.0,
            },
            clock_sync: ClockSyncConfig { required: true, accuracy_ms: 1, source: "gps".into() },
            ..Default::default()
        }),
        "acoustic" => Some(OamConfig {
            probe_interval_ms: 30000,
            probe_timeout_ms: 60000,
            failure_threshold: 3,
            sample_window: 10,
            probe_types: vec![ProbeType::Loopback, ProbeType::Loss],  // No DMM - clock sync hard
            scoring: ScoringWeights {
                rtt: 0.2, jitter: 1.0, loss: 3.0, asymmetry: 0.5, state: 5.0,
            },
            clock_sync: ClockSyncConfig { required: false, accuracy_ms: 100, source: "atomic".into() },
            ..Default::default()
        }),
        _ => None,
    }
}
```

## Integration with Zenoh Priority and Express Mechanisms

OAM link quality scoring must integrate with Zenoh's existing QoS system. This section describes how they work together.

### Zenoh QoS Overview

Zenoh has a sophisticated QoS system with:

1. **8 Priority Levels** (0-7, lower = higher priority):
   ```
   Control (0)      - Highest, for protocol messages
   RealTime (1)     - Real-time data
   InteractiveHigh (2)
   InteractiveLow (3)
   DataHigh (4)
   Data (5)         - Default
   DataLow (6)
   Background (7)   - Lowest
   ```

2. **Congestion Control**:
   - `Drop` - Drop messages when queue is full (default for unreliable)
   - `Block` - Wait for queue space (default for reliable)
   - `BlockFirst` - Block for first message only (unstable feature)

3. **Express Flag**: Bypass batching, send immediately

4. **Per-Link Priority Ranges**: Links can be configured to only carry specific priorities

### Current Link Selection Logic

The existing `select()` function in `tx.rs` uses a three-tier matching:

```rust
// Current behavior:
// 1. FULL MATCH: Link's reliability AND priority_range both match message
//    - If multiple candidates, select link with SMALLEST priority range
// 2. PARTIAL MATCH: Link's reliability matches but priority_range doesn't
// 3. ANY MATCH: Any available link as fallback
```

### OAM Integration Strategy

OAM quality scoring integrates as a **secondary criterion** after priority/reliability matching:

```rust
/// Extended link selection with OAM quality scoring
fn select_with_quality(
    links: &[TransportLinkUnicastUniversal],
    reliability: Reliability,
    priority: Priority,
    #[cfg(feature = "transport_oam")]
    scoring_config: &ScoringConfig,
) -> Option<usize> {
    // Step 1: Categorize links by match tier (existing logic)
    let mut full_matches: Vec<(usize, &TransportLinkUnicastUniversal)> = vec![];
    let mut partial_matches: Vec<(usize, &TransportLinkUnicastUniversal)> = vec![];
    let mut any_matches: Vec<(usize, &TransportLinkUnicastUniversal)> = vec![];
    
    for (idx, link) in links.iter().enumerate() {
        let link_reliability = link.config.reliability
            .unwrap_or(Reliability::from(link.link.is_reliable()));
        let link_priorities = &link.config.priorities;
        
        if link_reliability == reliability {
            if let Some(range) = link_priorities {
                if range.contains(priority) {
                    full_matches.push((idx, link));
                } else {
                    partial_matches.push((idx, link));
                }
            } else {
                full_matches.push((idx, link));  // No range = accepts all
            }
        } else {
            any_matches.push((idx, link));
        }
    }
    
    // Step 2: Select from best tier available, using OAM scores as tiebreaker
    let candidates = if !full_matches.is_empty() {
        full_matches
    } else if !partial_matches.is_empty() {
        partial_matches
    } else {
        any_matches
    };
    
    if candidates.is_empty() {
        return None;
    }
    
    #[cfg(feature = "transport_oam")]
    {
        // Step 3: Among candidates, select by quality score
        candidates
            .into_iter()
            .max_by(|(_, a), (_, b)| {
                let score_a = a.quality_score(scoring_config);
                let score_b = b.quality_score(scoring_config);
                score_a.partial_cmp(&score_b).unwrap_or(Ordering::Equal)
            })
            .map(|(idx, _)| idx)
    }
    
    #[cfg(not(feature = "transport_oam"))]
    {
        // Original: pick first match (or smallest priority range)
        candidates.first().map(|(idx, _)| *idx)
    }
}
```

### Priority-Aware Link Configuration

Links can be configured to handle specific priorities, and OAM respects this:

```json5
{
  transport: {
    unicast: {
      oam: {
        links: [
          {
            // Fiber: handle high-priority traffic
            pattern: "tcp/192.168.1.*:*",
            config: {
              preset: "fiber",
              /// Only route Control, RealTime, InteractiveHigh to this link
              priorities: "0-2",
            },
          },
          {
            // Satellite: handle bulk data
            pattern: "udp/10.0.100.*:*",
            config: {
              preset: "satellite_geo",
              /// Route Data, DataLow, Background to this link
              priorities: "5-7",
            },
          },
          {
            // Radio: backup for all priorities
            pattern: "serial/ttyUSB*",
            config: {
              preset: "radio",
              /// Accept any priority (fallback)
              priorities: null,  // or omit
            },
          },
        ],
      },
    },
  },
}
```

### Express Messages and OAM

Express messages bypass batching but still go through link selection:

```rust
impl TransmissionPipelineProducer {
    pub fn push_network_message(&self, msg: NetworkMessageRef) -> Result<bool, TransportClosed> {
        // Link was already selected by select_with_quality()
        // Express flag affects batching, not link selection
        
        if msg.is_express() {
            // Send immediately, don't accumulate in batch
            self.send_batch_now(batch);
        } else {
            // Accumulate in batch until full or timeout
            self.accumulate_in_batch(batch);
        }
    }
}
```

**OAM consideration**: For express messages, link quality matters more because there's no retry/reordering at the batch level. The scoring should consider:

```rust
impl LinkQualityMetrics {
    /// Score adjusted for express messages (lower jitter tolerance)
    pub fn score_for_express(&self, weights: &ScoringWeights) -> f64 {
        let mut score = self.score(weights);
        
        // Express messages are latency-sensitive, penalize jitter more
        let express_jitter_penalty = self.jitter.as_secs_f64() * 1000.0 * 2.0;
        score -= express_jitter_penalty;
        
        score.max(0.0)
    }
}
```

### Congestion Control Interaction

OAM interacts with congestion control in two ways:

#### 1. Link Congestion Detection

OAM can detect congestion before the pipeline queue fills:

```rust
pub struct LinkQualityMetrics {
    // ... existing fields ...
    
    /// Estimated congestion level (0.0 = clear, 1.0 = congested)
    pub congestion_estimate: f64,
    
    /// Recent queue depth trend from modem
    pub queue_depth_trend: Trend,
}

impl LinkQualityMetrics {
    /// Incorporate congestion into scoring
    pub fn score_with_congestion(&self, weights: &ScoringWeights) -> f64 {
        let mut score = self.score(weights);
        
        // Heavy penalty for congested links
        score -= self.congestion_estimate * 30.0;
        
        score.max(0.0)
    }
}
```

#### 2. Congestion-Aware Link Selection

For `CongestionControl::Drop` messages, prefer uncongested links:

```rust
fn select_with_quality(
    links: &[TransportLinkUnicastUniversal],
    reliability: Reliability,
    priority: Priority,
    congestion_control: CongestionControl,
    scoring_config: &ScoringConfig,
) -> Option<usize> {
    // ... tier matching ...
    
    // For droppable messages, filter out congested links if alternatives exist
    if congestion_control == CongestionControl::Drop {
        let uncongested: Vec<_> = candidates.iter()
            .filter(|(_, link)| !link.is_congested())
            .cloned()
            .collect();
        
        if !uncongested.is_empty() {
            candidates = uncongested;
        }
        // If all congested, proceed with original candidates (may drop later)
    }
    
    // Select by quality score
    // ...
}
```

### Priority-Based Scoring Weights

Different priorities may benefit from different scoring weights:

```json5
oam: {
  scoring: {
    /// Default weights
    default_weights: {
      rtt: 1.0,
      jitter: 2.0,
      loss: 10.0,
    },
    
    /// Per-priority weight overrides
    priority_weights: {
      /// Control & RealTime: latency critical
      "0-1": {
        rtt: 3.0,      // RTT matters most
        jitter: 4.0,   // Low jitter critical
        loss: 5.0,     // Some loss tolerable
      },
      
      /// Interactive: balanced
      "2-3": {
        rtt: 2.0,
        jitter: 2.0,
        loss: 10.0,
      },
      
      /// Data & Background: throughput oriented
      "4-7": {
        rtt: 0.5,      // Latency less important
        jitter: 1.0,
        loss: 15.0,    // Loss is costly (retransmits)
        bandwidth: 2.0, // Prefer high bandwidth
      },
    },
  },
}
```

```rust
impl ScoringConfig {
    pub fn weights_for_priority(&self, priority: Priority) -> &ScoringWeights {
        for (range, weights) in &self.priority_weights {
            if range.contains(priority) {
                return weights;
            }
        }
        &self.default_weights
    }
}

fn select_with_quality(/* ... */) -> Option<usize> {
    let weights = scoring_config.weights_for_priority(priority);
    
    candidates
        .into_iter()
        .max_by(|(_, a), (_, b)| {
            let score_a = a.quality_metrics.score(weights);
            let score_b = b.quality_metrics.score(weights);
            // ...
        })
        .map(|(idx, _)| idx)
}
```

### OAM Probes Priority

OAM probes themselves use `Priority::Control` (highest) to ensure they're sent even under congestion:

```rust
// In oam.rs
pub const OAM_QOS: QoSType = QoSType::new(
    Priority::Control,           // Highest priority
    CongestionControl::Block,    // Don't drop probes
    false,                       // Not express (can batch with other control)
);
```

This ensures accurate measurements even when data links are congested.

### Compatibility Matrix

| Feature | OAM Interaction | Notes |
|---------|-----------------|-------|
| **Priority 0-7** | Respects existing priority matching | OAM is tiebreaker within same tier |
| **Express flag** | Transparent | Link already selected, express affects batching |
| **CongestionControl::Drop** | Enhanced | OAM detects congestion, avoids dropping |
| **CongestionControl::Block** | Compatible | Blocking happens after link selection |
| **Per-link priorities** | Enhanced | OAM scores within priority-compatible links |
| **Reliability matching** | Respects | OAM is secondary to reliability matching |

### Example: Multi-Link Priority Routing

With fiber (low latency), satellite (high bandwidth), and radio (backup):

```
Message Priority    Best Link Selection
----------------    -------------------
Control (0)         Fiber (lowest RTT, even if score slightly lower)
RealTime (1)        Fiber (jitter-weighted scoring)
InteractiveHigh (2) Fiber or LEO Sat (depends on current quality)
Data (5)            Satellite (bandwidth-weighted scoring)
Background (7)      Any available (score-based, bandwidth bonus)
```

Configuration:
```json5
oam: {
  links: [
    { pattern: "tcp/*", config: { priorities: "0-3" } },      // Fiber: control+interactive
    { pattern: "udp/10.0.200.*", config: { priorities: "2-7" } }, // LEO: interactive+data
    { pattern: "udp/10.0.100.*", config: { priorities: "4-7" } }, // GEO: data only
    { pattern: "serial/*", config: { priorities: null } },    // Radio: fallback for all
  ],
  
  scoring: {
    priority_weights: {
      "0-1": { rtt: 3.0, jitter: 4.0, loss: 5.0 },
      "2-3": { rtt: 2.0, jitter: 2.0, loss: 10.0 },
      "4-7": { rtt: 0.5, jitter: 1.0, loss: 15.0, bandwidth: 2.0 },
    },
  },
}
```

## Open Questions

1. ~~**Clock Synchronization**: For accurate one-way delay (DMM/DMR), should we require/recommend NTP sync? Or focus only on RTT-based metrics?~~
   **Resolved**: Clock sync requirements are now link-type specific. See "Clock Synchronization Requirements" section. GPS time required for radio/satellite; optional for fiber/Ethernet.

2. **Probe Priority**: Should OAM probes use `Priority::Control` or a dedicated priority? Currently using Control.

3. **Multicast Support**: Should OAM be extended to multicast transports? (Initially: no, unicast only)

4. **Metrics Export**: Should metrics be exposed via the admin space? Via stats feature?

5. **Adaptive Probing**: Should probe interval automatically adjust based on link quality or bandwidth constraints? Particularly relevant for acoustic and low-bandwidth radio links.

6. **Half-Duplex Coordination**: For half-duplex links (acoustic, some radio), should OAM coordinate probe timing with application traffic to avoid collisions?

7. **Handover Detection**: For LEO satellites, should we implement explicit handover detection and notification to the application layer?

8. **Modem Provider Plugins**: Should modem metrics providers be implemented as plugins (like storage backends) to allow vendor-specific implementations without core changes?

9. ~~**Cross-Link Normalization**: When scoring heterogeneous links (fiber vs acoustic), should scores be normalized to account for inherently different baselines? A "good" acoustic link will always score lower than a "good" fiber link with raw scoring.~~
   **Resolved**: Implemented per-link-type baseline normalization with bandwidth weighting. See "Cross-Link Score Normalization" section. Supports multiple selection strategies: absolute, normalized, bandwidth-weighted, and bandwidth-constrained.

10. **Bandwidth-Aware Probing**: Should OAM probe frequency automatically reduce when modem reports low available bandwidth or high channel utilization?

11. **Link Bonding/Aggregation**: For scenarios where multiple links should be used simultaneously (load balancing), how should OAM metrics influence traffic distribution ratios?

## References

### General OAM Standards
- ITU-T G.8013/Y.1731: OAM functions and mechanisms for Ethernet-based networks
- RFC 3550: RTP - A Transport Protocol for Real-Time Applications (jitter calculation)
- IEEE 802.1ag: Connectivity Fault Management

### Fiber Optic
- ITU-T G.709: Interfaces for the optical transport network
- ITU-T G.7710/Y.1701: Common equipment management function requirements

### Ethernet
- IEEE 802.3ah: Ethernet in the First Mile (link OAM)
- ITU-T Y.1731: OAM functions and mechanisms for Ethernet-based networks

### Radio/Tactical
- MIL-STD-188-220: Digital message transfer device subsystems
- STANAG 4677: Tactical data link quality of service metrics
- NATO STANAG 5066: HF radio data communications

### Satellite
- CCSDS 131.0-B-4: TM Synchronization and Channel Coding
- CCSDS 732.0-B-4: AOS Space Data Link Protocol
- ETSI EN 302 307: DVB-S2 coding and modulation
- 3GPP TR 38.821: Solutions for NR to support non-terrestrial networks

### Acoustic/Underwater
- STANAG 4748 (JANUS): Digital underwater signaling standard
- NATO ANEP-87: Interoperable underwater acoustic communications
- Akyildiz, I.F. et al.: Underwater acoustic sensor networks: research challenges (2005)
