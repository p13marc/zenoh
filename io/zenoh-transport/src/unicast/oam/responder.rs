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

//! OAM Responder - generates replies to incoming probes.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    common::ZExtBody,
    transport::{
        oam::{
            id as oam_id, DelayMeasurementPayload, LoopbackPayload, Oam, OamId,
            SyntheticLossPayload,
        },
        TransportBody, TransportMessage,
    },
};

/// OAM Responder - stateless handler for generating probe replies.
pub struct OamResponder;

impl OamResponder {
    /// Get current timestamp in nanoseconds since UNIX epoch.
    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64
    }

    /// Handle an incoming OAM probe and generate a reply if applicable.
    ///
    /// # Arguments
    /// * `oam_id` - The OAM message ID
    /// * `payload` - The probe payload bytes
    /// * `rx_probe_count` - Total probes received (for SLM)
    ///
    /// # Returns
    /// * `Some(TransportMessage)` - Reply message to send
    /// * `None` - No reply needed or unknown probe type
    pub fn handle_probe(
        oam_id: OamId,
        payload: &[u8],
        rx_probe_count: u64,
    ) -> Option<TransportMessage> {
        match oam_id {
            oam_id::LBM => Self::reply_loopback(payload),
            oam_id::DMM => Self::reply_delay_measurement(payload),
            oam_id::SLM => Self::reply_synthetic_loss(payload, rx_probe_count),
            _ => {
                tracing::debug!("Unknown OAM probe ID: {:#06x}", oam_id);
                None
            }
        }
    }

    /// Generate a Loopback Reply (LBR) from a Loopback Message (LBM).
    ///
    /// The reply echoes the same payload back to the sender.
    fn reply_loopback(payload: &[u8]) -> Option<TransportMessage> {
        let request = LoopbackPayload::decode(payload)?;

        // Echo back the same payload
        let reply = LoopbackPayload::new(request.seq, request.timestamp_ns);

        let oam = Oam {
            id: oam_id::LBR,
            body: ZExtBody::ZBuf(ZBuf::from(reply.encode().to_vec())),
            ext_qos: Default::default(),
        };

        Some(TransportMessage {
            body: TransportBody::OAM(oam),
        })
    }

    /// Generate a Delay Measurement Reply (DMR) from a Delay Measurement Message (DMM).
    ///
    /// The reply includes:
    /// - t1: Original send time (from DMM)
    /// - t2: Time DMM was received (now)
    /// - t3: Time DMR is being sent (now, same as t2 for simplicity)
    fn reply_delay_measurement(payload: &[u8]) -> Option<TransportMessage> {
        let request = DelayMeasurementPayload::decode(payload)?;

        let now = Self::now_ns();
        let reply = DelayMeasurementPayload::new_reply(
            request.seq,
            request.t1, // Echo original t1
            now,        // t2: receive time
            now,        // t3: send time (same as t2 for immediate reply)
        );

        let oam = Oam {
            id: oam_id::DMR,
            body: ZExtBody::ZBuf(ZBuf::from(reply.encode().to_vec())),
            ext_qos: Default::default(),
        };

        Some(TransportMessage {
            body: TransportBody::OAM(oam),
        })
    }

    /// Generate a Synthetic Loss Reply (SLR) from a Synthetic Loss Message (SLM).
    ///
    /// The reply includes the sender's tx_counter and our rx_counter.
    fn reply_synthetic_loss(payload: &[u8], rx_probe_count: u64) -> Option<TransportMessage> {
        let request = SyntheticLossPayload::decode(payload)?;

        let reply = SyntheticLossPayload::new_reply(
            request.seq,
            request.tx_counter, // Echo sender's tx count
            rx_probe_count,     // Our rx count
        );

        let oam = Oam {
            id: oam_id::SLR,
            body: ZExtBody::ZBuf(ZBuf::from(reply.encode().to_vec())),
            ext_qos: Default::default(),
        };

        Some(TransportMessage {
            body: TransportBody::OAM(oam),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh_buffers::buffer::SplitBuffer;

    #[test]
    fn test_loopback_reply() {
        let request = LoopbackPayload::new(42, 1234567890);
        let payload = request.encode();

        let reply = OamResponder::handle_probe(oam_id::LBM, &payload, 0);
        assert!(reply.is_some());

        let reply = reply.unwrap();
        match reply.body {
            TransportBody::OAM(oam) => {
                assert_eq!(oam.id, oam_id::LBR);
                match oam.body {
                    ZExtBody::ZBuf(buf) => {
                        let bytes: Vec<u8> = buf.slices().flat_map(|s| s.iter().copied()).collect();
                        let decoded = LoopbackPayload::decode(&bytes).unwrap();
                        assert_eq!(decoded.seq, 42);
                        assert_eq!(decoded.timestamp_ns, 1234567890);
                    }
                    _ => panic!("Expected ZBuf body"),
                }
            }
            _ => panic!("Expected OAM message"),
        }
    }

    #[test]
    fn test_delay_reply() {
        let request = DelayMeasurementPayload::new_request(42, 1234567890);
        let payload = request.encode();

        let reply = OamResponder::handle_probe(oam_id::DMM, &payload, 0);
        assert!(reply.is_some());

        let reply = reply.unwrap();
        match reply.body {
            TransportBody::OAM(oam) => {
                assert_eq!(oam.id, oam_id::DMR);
                match oam.body {
                    ZExtBody::ZBuf(buf) => {
                        let bytes: Vec<u8> = buf.slices().flat_map(|s| s.iter().copied()).collect();
                        let decoded = DelayMeasurementPayload::decode(&bytes).unwrap();
                        assert_eq!(decoded.seq, 42);
                        assert_eq!(decoded.t1, 1234567890);
                        assert!(decoded.t2 > 0); // Should have receive time
                        assert!(decoded.t3 > 0); // Should have send time
                    }
                    _ => panic!("Expected ZBuf body"),
                }
            }
            _ => panic!("Expected OAM message"),
        }
    }

    #[test]
    fn test_loss_reply() {
        let request = SyntheticLossPayload::new_request(42, 100);
        let payload = request.encode();

        let reply = OamResponder::handle_probe(oam_id::SLM, &payload, 95);
        assert!(reply.is_some());

        let reply = reply.unwrap();
        match reply.body {
            TransportBody::OAM(oam) => {
                assert_eq!(oam.id, oam_id::SLR);
                match oam.body {
                    ZExtBody::ZBuf(buf) => {
                        let bytes: Vec<u8> = buf.slices().flat_map(|s| s.iter().copied()).collect();
                        let decoded = SyntheticLossPayload::decode(&bytes).unwrap();
                        assert_eq!(decoded.seq, 42);
                        assert_eq!(decoded.tx_counter, 100);
                        assert_eq!(decoded.rx_counter, 95);
                    }
                    _ => panic!("Expected ZBuf body"),
                }
            }
            _ => panic!("Expected OAM message"),
        }
    }

    #[test]
    fn test_unknown_probe() {
        let payload = [0u8; 12];
        let reply = OamResponder::handle_probe(0x9999, &payload, 0);
        assert!(reply.is_none());
    }
}
