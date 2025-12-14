//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::common::ZExtBody;

pub type OamId = u16;

/// OAM message identifiers for link quality measurement probes.
///
/// These IDs follow a request/reply pattern where even IDs are requests
/// and odd IDs are the corresponding replies.
pub mod id {
    use super::OamId;

    /// Loopback Message - Simple ping for RTT measurement
    pub const LBM: OamId = 0x0010;
    /// Loopback Reply - Response to LBM
    pub const LBR: OamId = 0x0011;

    /// Delay Measurement Message - 4-timestamp protocol for one-way delay
    pub const DMM: OamId = 0x0020;
    /// Delay Measurement Reply - Response to DMM with timestamps
    pub const DMR: OamId = 0x0021;

    /// Synthetic Loss Message - Packet loss measurement
    pub const SLM: OamId = 0x0030;
    /// Synthetic Loss Reply - Response to SLM with counters
    pub const SLR: OamId = 0x0031;
}

pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Transport
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// ```text
/// Flags:
/// - E |: Encoding     The encoding of the extension
/// - E/
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|ENC|  OAM    |
/// +-+-+-+---------+
/// ~    id:z16     ~
/// +---------------+
/// ~  [oam_exts]   ~
/// +---------------+
/// %    length     % -- If ENC == u64 || ENC == ZBuf
/// +---------------+
/// ~     [u8]      ~ -- If ENC == ZBuf
/// +---------------+
/// ```
///
/// Encoding:
/// - 0b00: Unit
/// - 0b01: u64
/// - 0b10: ZBuf
/// - 0b11: Reserved
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oam {
    pub id: OamId,
    pub body: ZExtBody,
    pub ext_qos: ext::QoSType,
}

pub mod ext {
    use crate::{common::ZExtZ64, zextz64};

    pub type QoS = zextz64!(0x1, true);
    pub type QoSType = crate::transport::ext::QoSType<{ QoS::ID }>;
}

/// Loopback probe payload for RTT measurement.
///
/// Wire format (12 bytes):
/// ```text
/// +---------------+
/// |   seq: u32    |  Sequence number
/// +---------------+
/// | timestamp: u64|  Sender timestamp (nanoseconds since epoch)
/// +---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackPayload {
    /// Sequence number for matching requests to replies
    pub seq: u32,
    /// Sender timestamp in nanoseconds since UNIX epoch
    pub timestamp_ns: u64,
}

impl LoopbackPayload {
    pub const SIZE: usize = 12; // 4 + 8 bytes

    pub fn new(seq: u32, timestamp_ns: u64) -> Self {
        Self { seq, timestamp_ns }
    }

    /// Encode payload to bytes (little-endian)
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.timestamp_ns.to_le_bytes());
        buf
    }

    /// Decode payload from bytes (little-endian)
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::SIZE {
            return None;
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        let timestamp_ns = u64::from_le_bytes(buf[4..12].try_into().ok()?);
        Some(Self { seq, timestamp_ns })
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self {
            seq: rng.gen(),
            timestamp_ns: rng.gen(),
        }
    }
}

/// Delay measurement payload for one-way delay calculation.
///
/// Uses 4-timestamp protocol (similar to IEEE 1588 PTP):
/// - t1: DMM send time (set by initiator)
/// - t2: DMM receive time (set by responder in DMR)
/// - t3: DMR send time (set by responder in DMR)
/// - t4: DMR receive time (recorded locally by initiator)
///
/// Forward delay = t2 - t1 (requires clock sync)
/// Reverse delay = t4 - t3 (requires clock sync)
/// RTT = (t4 - t1) - (t3 - t2)
///
/// Wire format (28 bytes):
/// ```text
/// +---------------+
/// |   seq: u32    |
/// +---------------+
/// |   t1: u64     |  DMM send time
/// +---------------+
/// |   t2: u64     |  DMM receive time (0 in DMM, filled in DMR)
/// +---------------+
/// |   t3: u64     |  DMR send time (0 in DMM, filled in DMR)
/// +---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelayMeasurementPayload {
    /// Sequence number
    pub seq: u32,
    /// DMM send time (nanoseconds since epoch)
    pub t1: u64,
    /// DMM receive time (nanoseconds since epoch), 0 in request
    pub t2: u64,
    /// DMR send time (nanoseconds since epoch), 0 in request
    pub t3: u64,
}

impl DelayMeasurementPayload {
    pub const SIZE: usize = 28; // 4 + 8 + 8 + 8 bytes

    pub fn new_request(seq: u32, t1: u64) -> Self {
        Self {
            seq,
            t1,
            t2: 0,
            t3: 0,
        }
    }

    pub fn new_reply(seq: u32, t1: u64, t2: u64, t3: u64) -> Self {
        Self { seq, t1, t2, t3 }
    }

    /// Encode payload to bytes (little-endian)
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.t1.to_le_bytes());
        buf[12..20].copy_from_slice(&self.t2.to_le_bytes());
        buf[20..28].copy_from_slice(&self.t3.to_le_bytes());
        buf
    }

    /// Decode payload from bytes (little-endian)
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::SIZE {
            return None;
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        let t1 = u64::from_le_bytes(buf[4..12].try_into().ok()?);
        let t2 = u64::from_le_bytes(buf[12..20].try_into().ok()?);
        let t3 = u64::from_le_bytes(buf[20..28].try_into().ok()?);
        Some(Self { seq, t1, t2, t3 })
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self {
            seq: rng.gen(),
            t1: rng.gen(),
            t2: rng.gen(),
            t3: rng.gen(),
        }
    }
}

/// Synthetic loss measurement payload for packet loss calculation.
///
/// Uses counters to track sent and received probes:
/// - In SLM: tx_counter = total probes sent by initiator, rx_counter = 0
/// - In SLR: tx_counter = echoed from SLM, rx_counter = total probes received by responder
///
/// Wire format (20 bytes):
/// ```text
/// +---------------+
/// |   seq: u32    |
/// +---------------+
/// | tx_counter:u64|  Total probes sent
/// +---------------+
/// | rx_counter:u64|  Total probes received (filled in reply)
/// +---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticLossPayload {
    /// Sequence number
    pub seq: u32,
    /// Total probes sent by sender
    pub tx_counter: u64,
    /// Total probes received by responder (0 in request, filled in reply)
    pub rx_counter: u64,
}

impl SyntheticLossPayload {
    pub const SIZE: usize = 20; // 4 + 8 + 8 bytes

    pub fn new_request(seq: u32, tx_counter: u64) -> Self {
        Self {
            seq,
            tx_counter,
            rx_counter: 0,
        }
    }

    pub fn new_reply(seq: u32, tx_counter: u64, rx_counter: u64) -> Self {
        Self {
            seq,
            tx_counter,
            rx_counter,
        }
    }

    /// Encode payload to bytes (little-endian)
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.tx_counter.to_le_bytes());
        buf[12..20].copy_from_slice(&self.rx_counter.to_le_bytes());
        buf
    }

    /// Decode payload from bytes (little-endian)
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::SIZE {
            return None;
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        let tx_counter = u64::from_le_bytes(buf[4..12].try_into().ok()?);
        let rx_counter = u64::from_le_bytes(buf[12..20].try_into().ok()?);
        Some(Self {
            seq,
            tx_counter,
            rx_counter,
        })
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self {
            seq: rng.gen(),
            tx_counter: rng.gen(),
            rx_counter: rng.gen(),
        }
    }
}

impl Oam {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id: OamId = rng.gen();
        let payload = ZExtBody::rand();
        let ext_qos = ext::QoSType::rand();

        Self {
            id,
            body: payload,
            ext_qos,
        }
    }
}
