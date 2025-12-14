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

//! OAM Prober - sends probes and tracks pending responses.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tokio::sync::mpsc;
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    common::ZExtBody,
    transport::{
        oam::{id as oam_id, LoopbackPayload, Oam},
        TransportBody, TransportMessage,
    },
};

use super::{LinkQualityMetrics, OamConfig};
use crate::common::pipeline::TransmissionPipelineProducer;

/// Probe types that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeType {
    Loopback,
    DelayMeasurement,
    SyntheticLoss,
}

/// Information about a pending probe awaiting reply.
#[derive(Debug)]
struct PendingProbe {
    sent_at: Instant,
    probe_type: ProbeType,
}

/// OAM Prober - manages sending probes and processing replies.
pub struct OamProber {
    /// Configuration
    config: OamConfig,
    /// Shared metrics updated by this prober
    metrics: Arc<RwLock<LinkQualityMetrics>>,
    /// Pending probes awaiting replies (keyed by sequence number)
    pending_probes: HashMap<u32, PendingProbe>,
    /// Sequence number counter
    seq: AtomicU32,
    /// Pipeline to send OAM messages through
    pipeline: TransmissionPipelineProducer,
}

impl OamProber {
    /// Create a new OAM prober.
    pub(crate) fn new(
        config: OamConfig,
        metrics: Arc<RwLock<LinkQualityMetrics>>,
        pipeline: TransmissionPipelineProducer,
    ) -> Self {
        Self {
            config,
            metrics,
            pending_probes: HashMap::new(),
            seq: AtomicU32::new(0),
            pipeline,
        }
    }

    /// Get the next sequence number.
    fn next_seq(&self) -> u32 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Get current timestamp in nanoseconds since UNIX epoch.
    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64
    }

    /// Send a loopback probe (LBM).
    pub fn send_loopback(&mut self) -> Result<(), String> {
        let seq = self.next_seq();
        let timestamp_ns = Self::now_ns();
        let payload = LoopbackPayload::new(seq, timestamp_ns);

        let oam = Oam {
            id: oam_id::LBM,
            body: ZExtBody::ZBuf(ZBuf::from(payload.encode().to_vec())),
            ext_qos: Default::default(),
        };

        let msg = TransportMessage {
            body: TransportBody::OAM(oam),
        };

        // Record pending probe
        self.pending_probes.insert(
            seq,
            PendingProbe {
                sent_at: Instant::now(),
                probe_type: ProbeType::Loopback,
            },
        );

        // Update metrics
        if let Ok(mut m) = self.metrics.write() {
            m.record_probe_sent();
        }

        // Send the probe through the pipeline at background priority
        match self
            .pipeline
            .push_transport_message(msg, zenoh_protocol::core::Priority::Background)
        {
            Ok(true) => Ok(()),
            Ok(false) => Err("Pipeline congested, probe not sent".to_string()),
            Err(_) => Err("Pipeline closed".to_string()),
        }
    }

    /// Handle an incoming probe reply.
    ///
    /// Returns true if the reply was handled successfully.
    pub fn handle_reply(&mut self, oam_id: u16, payload: &[u8]) -> bool {
        match oam_id {
            oam_id::LBR => self.handle_loopback_reply(payload),
            oam_id::DMR => self.handle_delay_reply(payload),
            oam_id::SLR => self.handle_loss_reply(payload),
            _ => {
                tracing::debug!("Unknown OAM reply ID: {:#06x}", oam_id);
                false
            }
        }
    }

    /// Handle a loopback reply (LBR).
    fn handle_loopback_reply(&mut self, payload: &[u8]) -> bool {
        let Some(reply) = LoopbackPayload::decode(payload) else {
            tracing::warn!("Failed to decode LBR payload");
            return false;
        };

        let Some(pending) = self.pending_probes.remove(&reply.seq) else {
            tracing::debug!("Received LBR for unknown seq: {}", reply.seq);
            return false;
        };

        let rtt = pending.sent_at.elapsed();

        // Update metrics
        if let Ok(mut m) = self.metrics.write() {
            m.update_from_probe(rtt);
        }

        tracing::trace!("LBR seq={} rtt={:?}", reply.seq, rtt);

        true
    }

    /// Handle a delay measurement reply (DMR).
    fn handle_delay_reply(&mut self, payload: &[u8]) -> bool {
        use zenoh_protocol::transport::oam::DelayMeasurementPayload;

        let Some(reply) = DelayMeasurementPayload::decode(payload) else {
            tracing::warn!("Failed to decode DMR payload");
            return false;
        };

        let Some(pending) = self.pending_probes.remove(&reply.seq) else {
            tracing::debug!("Received DMR for unknown seq: {}", reply.seq);
            return false;
        };

        let t4 = Self::now_ns();
        let rtt = pending.sent_at.elapsed();

        // Calculate one-way delays if timestamps are valid
        let (forward, reverse) = if reply.t2 > reply.t1 && t4 > reply.t3 {
            let forward = Duration::from_nanos(reply.t2.saturating_sub(reply.t1));
            let reverse = Duration::from_nanos(t4.saturating_sub(reply.t3));
            (Some(forward), Some(reverse))
        } else {
            (None, None)
        };

        // Update metrics
        if let Ok(mut m) = self.metrics.write() {
            m.update_from_delay_measurement(rtt, forward, reverse);
        }

        true
    }

    /// Handle a synthetic loss reply (SLR).
    fn handle_loss_reply(&mut self, payload: &[u8]) -> bool {
        use zenoh_protocol::transport::oam::SyntheticLossPayload;

        let Some(reply) = SyntheticLossPayload::decode(payload) else {
            tracing::warn!("Failed to decode SLR payload");
            return false;
        };

        let Some(pending) = self.pending_probes.remove(&reply.seq) else {
            tracing::debug!("Received SLR for unknown seq: {}", reply.seq);
            return false;
        };

        let rtt = pending.sent_at.elapsed();

        // Update metrics with RTT
        if let Ok(mut m) = self.metrics.write() {
            m.update_from_probe(rtt);
        }

        // Log loss information from remote
        tracing::trace!(
            "SLR seq={} tx={} rx={}",
            reply.seq,
            reply.tx_counter,
            reply.rx_counter
        );

        true
    }

    /// Check for timed-out probes and update metrics accordingly.
    pub fn check_timeouts(&mut self) {
        let now = Instant::now();
        let timeout = self.config.probe_timeout;
        let threshold = self.config.failure_threshold;

        let timed_out: Vec<u32> = self
            .pending_probes
            .iter()
            .filter(|(_, probe)| now.duration_since(probe.sent_at) > timeout)
            .map(|(seq, _)| *seq)
            .collect();

        for seq in timed_out {
            if let Some(probe) = self.pending_probes.remove(&seq) {
                tracing::debug!("Probe timeout: seq={} type={:?}", seq, probe.probe_type);

                if let Ok(mut m) = self.metrics.write() {
                    m.record_timeout(threshold);
                }
            }
        }
    }

    /// Get a reference to the shared metrics.
    pub fn metrics(&self) -> &Arc<RwLock<LinkQualityMetrics>> {
        &self.metrics
    }

    /// Get the current probe timeout setting.
    pub fn probe_timeout(&self) -> Duration {
        self.config.probe_timeout
    }

    /// Get the current probe interval setting.
    pub fn probe_interval(&self) -> Duration {
        self.config.probe_interval
    }
}

/// Run the OAM probing loop.
///
/// This function sends periodic probes and checks for timeouts.
/// It runs until the cancellation token is triggered.
pub(crate) async fn run_oam_prober(
    config: OamConfig,
    metrics: Arc<RwLock<LinkQualityMetrics>>,
    pipeline: TransmissionPipelineProducer,
    mut rx_receiver: mpsc::Receiver<(u16, Vec<u8>)>,
    token: tokio_util::sync::CancellationToken,
) {
    let mut prober = OamProber::new(config.clone(), metrics, pipeline);
    let mut probe_interval = tokio::time::interval(config.probe_interval);
    let mut timeout_check_interval = tokio::time::interval(config.probe_timeout / 2);

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                tracing::trace!("OAM prober cancelled");
                break;
            }
            _ = probe_interval.tick() => {
                if let Err(e) = prober.send_loopback() {
                    tracing::debug!("Failed to send OAM probe: {}", e);
                }
            }
            _ = timeout_check_interval.tick() => {
                prober.check_timeouts();
            }
            Some((oam_id, payload)) = rx_receiver.recv() => {
                prober.handle_reply(oam_id, &payload);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loopback_payload_roundtrip() {
        let payload = LoopbackPayload::new(42, 1234567890);
        let encoded = payload.encode();
        let decoded = LoopbackPayload::decode(&encoded).unwrap();

        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.timestamp_ns, 1234567890);
    }
}
