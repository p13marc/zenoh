//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! Link quality metrics collected from OAM probes.

use std::time::{Duration, Instant};

/// Link operational state based on probe responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkState {
    /// Initial state, no probes exchanged yet
    #[default]
    Unknown,
    /// Link is responding normally
    Operational,
    /// Link is responding but with degraded metrics
    Degraded,
    /// Link is marginally operational
    Marginal,
    /// Link is not responding
    Failed,
}

/// Per-link quality metrics collected from OAM probes.
///
/// These metrics are updated by the OAM prober task and can be read
/// by external controllers via the metrics API.
#[derive(Debug, Clone)]
pub struct LinkQualityMetrics {
    // === RTT Metrics ===
    /// Current RTT (last measurement)
    pub rtt_current: Duration,
    /// Minimum observed RTT
    pub rtt_min: Duration,
    /// Maximum observed RTT
    pub rtt_max: Duration,
    /// Exponential moving average RTT
    pub rtt_avg: Duration,

    // === Jitter (RFC 3550) ===
    /// Jitter calculated using RFC 3550 algorithm
    pub jitter: Duration,

    // === One-Way Delay (requires clock sync) ===
    /// Forward delay: local -> remote (if available)
    pub forward_delay: Option<Duration>,
    /// Reverse delay: remote -> local (if available)
    pub reverse_delay: Option<Duration>,

    // === Packet Loss ===
    /// Total probes sent
    pub tx_probe_count: u64,
    /// Total probe replies received
    pub rx_probe_count: u64,
    /// Loss ratio (0.0 to 1.0)
    pub loss_ratio: f64,

    // === Link State ===
    /// Current link state
    pub state: LinkState,
    /// Consecutive probe failures
    pub consecutive_failures: u32,
    /// Whether the link is considered alive
    pub is_alive: bool,

    // === Timestamps ===
    /// When the last probe was sent
    pub last_probe_sent: Option<Instant>,
    /// When the last probe reply was received
    pub last_probe_received: Option<Instant>,
    /// When metrics were last updated
    pub metrics_updated: Instant,

    // === Internal state for calculations ===
    /// EMA alpha factor (typically 1/16 for RFC 3550 jitter)
    ema_alpha: f64,
    /// Previous RTT for jitter calculation
    prev_rtt: Option<Duration>,
}

impl Default for LinkQualityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkQualityMetrics {
    /// Create new metrics with default values.
    pub fn new() -> Self {
        Self {
            rtt_current: Duration::ZERO,
            rtt_min: Duration::MAX,
            rtt_max: Duration::ZERO,
            rtt_avg: Duration::ZERO,
            jitter: Duration::ZERO,
            forward_delay: None,
            reverse_delay: None,
            tx_probe_count: 0,
            rx_probe_count: 0,
            loss_ratio: 0.0,
            state: LinkState::Unknown,
            consecutive_failures: 0,
            is_alive: false,
            last_probe_sent: None,
            last_probe_received: None,
            metrics_updated: Instant::now(),
            ema_alpha: 1.0 / 16.0, // RFC 3550 recommended
            prev_rtt: None,
        }
    }

    /// Record that a probe was sent.
    pub fn record_probe_sent(&mut self) {
        self.tx_probe_count += 1;
        self.last_probe_sent = Some(Instant::now());
    }

    /// Update metrics with a successful probe response.
    ///
    /// # Arguments
    /// * `rtt` - Round-trip time measured for this probe
    pub fn update_from_probe(&mut self, rtt: Duration) {
        let now = Instant::now();

        // Update RTT metrics
        self.rtt_current = rtt;
        self.rtt_min = self.rtt_min.min(rtt);
        self.rtt_max = self.rtt_max.max(rtt);

        // Update EMA for average RTT
        if self.rx_probe_count == 0 {
            // First sample
            self.rtt_avg = rtt;
        } else {
            // Exponential moving average
            let alpha = self.ema_alpha;
            let rtt_nanos = rtt.as_nanos() as f64;
            let avg_nanos = self.rtt_avg.as_nanos() as f64;
            let new_avg = avg_nanos + alpha * (rtt_nanos - avg_nanos);
            self.rtt_avg = Duration::from_nanos(new_avg as u64);
        }

        // Update jitter using RFC 3550 algorithm
        self.update_jitter(rtt);

        // Update counters
        self.rx_probe_count += 1;
        self.consecutive_failures = 0;
        self.is_alive = true;
        self.last_probe_received = Some(now);
        self.metrics_updated = now;

        // Update loss ratio
        if self.tx_probe_count > 0 {
            self.loss_ratio = 1.0 - (self.rx_probe_count as f64 / self.tx_probe_count as f64);
        }

        // Update state
        self.update_state();
    }

    /// Update metrics with delay measurement values.
    ///
    /// # Arguments
    /// * `rtt` - Round-trip time
    /// * `forward` - Forward delay (local -> remote), if clock sync available
    /// * `reverse` - Reverse delay (remote -> local), if clock sync available
    pub fn update_from_delay_measurement(
        &mut self,
        rtt: Duration,
        forward: Option<Duration>,
        reverse: Option<Duration>,
    ) {
        self.update_from_probe(rtt);
        self.forward_delay = forward;
        self.reverse_delay = reverse;
    }

    /// Record a probe timeout (failure).
    pub fn record_timeout(&mut self, failure_threshold: u32) {
        self.consecutive_failures += 1;
        self.metrics_updated = Instant::now();

        if self.consecutive_failures >= failure_threshold {
            self.is_alive = false;
            self.state = LinkState::Failed;
        } else {
            self.state = LinkState::Degraded;
        }

        // Update loss ratio
        if self.tx_probe_count > 0 {
            self.loss_ratio = 1.0 - (self.rx_probe_count as f64 / self.tx_probe_count as f64);
        }
    }

    /// Update jitter using RFC 3550 algorithm.
    ///
    /// J(i) = J(i-1) + (|D(i-1,i)| - J(i-1)) / 16
    /// where D(i-1,i) is the difference in RTT between consecutive probes.
    fn update_jitter(&mut self, rtt: Duration) {
        if let Some(prev) = self.prev_rtt {
            // Calculate difference between consecutive RTTs
            let diff = if rtt > prev { rtt - prev } else { prev - rtt };

            // Apply RFC 3550 jitter calculation
            let diff_nanos = diff.as_nanos() as f64;
            let jitter_nanos = self.jitter.as_nanos() as f64;
            let new_jitter = jitter_nanos + (diff_nanos - jitter_nanos) / 16.0;
            self.jitter = Duration::from_nanos(new_jitter as u64);
        }
        self.prev_rtt = Some(rtt);
    }

    /// Update link state based on current metrics.
    fn update_state(&mut self) {
        // Simple state machine based on consecutive failures and loss ratio
        if self.consecutive_failures > 0 {
            if self.is_alive {
                self.state = LinkState::Degraded;
            } else {
                self.state = LinkState::Failed;
            }
        } else if self.loss_ratio > 0.1 {
            // More than 10% loss
            self.state = LinkState::Marginal;
        } else if self.loss_ratio > 0.01 {
            // More than 1% loss
            self.state = LinkState::Degraded;
        } else if self.rx_probe_count > 0 {
            self.state = LinkState::Operational;
        }
    }

    /// Reset all metrics to initial state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let metrics = LinkQualityMetrics::new();
        assert_eq!(metrics.state, LinkState::Unknown);
        assert!(!metrics.is_alive);
        assert_eq!(metrics.tx_probe_count, 0);
        assert_eq!(metrics.rx_probe_count, 0);
    }

    #[test]
    fn test_probe_update() {
        let mut metrics = LinkQualityMetrics::new();

        metrics.record_probe_sent();
        assert_eq!(metrics.tx_probe_count, 1);

        let rtt = Duration::from_millis(10);
        metrics.update_from_probe(rtt);

        assert_eq!(metrics.rtt_current, rtt);
        assert_eq!(metrics.rtt_min, rtt);
        assert_eq!(metrics.rtt_max, rtt);
        assert_eq!(metrics.rx_probe_count, 1);
        assert!(metrics.is_alive);
        assert_eq!(metrics.state, LinkState::Operational);
    }

    #[test]
    fn test_jitter_calculation() {
        let mut metrics = LinkQualityMetrics::new();

        // First probe - no jitter yet
        metrics.update_from_probe(Duration::from_millis(10));
        assert_eq!(metrics.jitter, Duration::ZERO);

        // Second probe with different RTT - jitter should be calculated
        metrics.update_from_probe(Duration::from_millis(20));
        assert!(metrics.jitter > Duration::ZERO);
    }

    #[test]
    fn test_timeout_handling() {
        let mut metrics = LinkQualityMetrics::new();

        // First successful probe
        metrics.record_probe_sent();
        metrics.update_from_probe(Duration::from_millis(10));
        assert!(metrics.is_alive);

        // Record timeouts
        metrics.record_probe_sent();
        metrics.record_timeout(3);
        assert_eq!(metrics.consecutive_failures, 1);
        assert!(metrics.is_alive); // Still alive, not at threshold

        metrics.record_probe_sent();
        metrics.record_timeout(3);
        assert_eq!(metrics.consecutive_failures, 2);
        assert!(metrics.is_alive);

        metrics.record_probe_sent();
        metrics.record_timeout(3);
        assert_eq!(metrics.consecutive_failures, 3);
        assert!(!metrics.is_alive); // Now failed
        assert_eq!(metrics.state, LinkState::Failed);
    }

    #[test]
    fn test_loss_ratio() {
        let mut metrics = LinkQualityMetrics::new();

        // Send 10 probes, receive 8
        for _ in 0..10 {
            metrics.record_probe_sent();
        }
        for _ in 0..8 {
            metrics.update_from_probe(Duration::from_millis(10));
        }

        // Loss ratio should be 0.2 (2 lost out of 10)
        assert!((metrics.loss_ratio - 0.2).abs() < 0.001);
    }
}
