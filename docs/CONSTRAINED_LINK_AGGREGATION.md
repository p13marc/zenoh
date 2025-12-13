# Constrained Link Aggregation Architecture

## Document Information

| Field | Value |
|-------|-------|
| Version | 2.2.0 |
| Status | Draft |
| Date | 2024-12-13 |
| Related | OAM_LINK_QUALITY_PLAN.md, LINK_SCORING_CRATE_PLAN.md |

---

## 1. Problem Statement

Several communication technologies have characteristics that make them fundamentally different from standard network links:

### 1.1 Constrained Link Types

| Characteristic | Acoustic | Slow Satellite | Tactical Radio | Standard |
|----------------|----------|----------------|----------------|----------|
| RTT | 1-10+ sec | 600-800 ms | 100-500 ms | 1-50 ms |
| MTU | 32-256 B | 500-1500 B | 256-1500 B | 1500 B |
| Bandwidth | 0.1-10 kbps | 9.6-64 kbps | 16-256 kbps | Mbps-Gbps |
| Reliability | 70-90% | 90-99% | 80-95% | 99%+ |
| Availability | Intermittent | Variable | Variable | Continuous |
| Encryption | Often none | Usually yes | Usually yes | Varies |

### 1.2 Challenges

Standard Zenoh behavior sends messages immediately when they're ready. For constrained links, this is problematic:

1. **Many small messages** waste limited bandwidth on headers
2. **Real-time data** becomes stale during multi-second transmission
3. **Retransmissions** may arrive after newer data
4. **No encryption** on some physical layers (acoustic modems)

### 1.3 Solution Concept

A **Deferred Link** architecture that:
1. Buffers messages and aggregates by key (snapshot mode)
2. Compresses keys using bitset or ID-based encoding
3. Optionally encrypts data before transmission
4. Supports urgent message bypass via Express flag
5. Uses socket backpressure for natural flow control

### 1.4 Related Standards and Research

This design is informed by existing standards and research for constrained communications:

#### 1.4.1 Underwater Acoustic Specific

| Standard/Protocol | Relevance |
|-------------------|-----------|
| [JANUS (STANAG 4748)](https://ieeexplore.ieee.org/document/7017134/) | NATO standard for underwater acoustic comms, open message format |
| [WHOI CCL](https://acomms.whoi.edu/wp-content/uploads/sites/20/2014/08/StokeyFreitagGrund_CCL_2005.pdf) | Compact Control Language for AUV command, 32-byte packets |
| [DCCL v3](https://libdccl.org/3.0/) | Dynamic Compact Control Language, bit-level field encoding |
| [Goby-Acomms](https://gobysoft.org/software/dccl/) | Complete underwater acoustic networking framework |

**DCCL (Dynamic Compact Control Language)** is particularly relevant:
- Developed at MIT/WHOI specifically for underwater acoustic modems
- Uses Protocol Buffers IDL with custom bit-level encoding
- Achieves **56% more compact** encoding than standard Protobuf
- Encodes fields using min/max bounds and precision (e.g., depth 0-5000m uses 13 bits vs 32)
- Open source C++ library: https://github.com/GobySoft/dccl

**JANUS (NATO STANAG 4748)**:
- First international digital underwater acoustic standard (2017)
- Center frequency 11520 Hz, bandwidth 4160 Hz
- Designed for node discovery and interoperability ("Channel 16" of underwater)
- Application-layer payload format is user-defined

#### 1.4.2 Space and IoT Standards

| Standard/Protocol | Relevance |
|-------------------|-----------|
| [CCSDS 124.0-B-1](https://ccsds.org/Pubs/124x0b1.pdf) (POCKET+) | Delta encoding, change masks for housekeeping telemetry |
| [MQTT-SN](http://www.steves-internet-guide.com/mqtt-sn-topic-names/) | Predefined topic IDs, short topics (2-byte) |
| [CCSDS XTCE](https://ccsds.org/Pubs/660x2g2.pdf) | Parameter dictionaries, telemetry definitions |
| [CoAP](https://www.mdpi.com/2504-3900/31/1/49) | Compressed headers for IoT |

#### 1.4.3 Data Aggregation Research

| Paper/Approach | Relevance |
|----------------|-----------|
| [Cluster-based aggregation for UASNs](https://ieeexplore.ieee.org/document/6420597/) | Energy-efficient data aggregation in underwater sensor networks |
| [IDACB](https://www.sciencedirect.com/science/article/pii/S1319157817300484) | Improved Data Aggregation for Cluster-Based UWSNs |
| [Feature-level aggregation](https://ieeexplore.ieee.org/document/10608073/) | Spatial correlation for data aggregation |

**Key insight from research**: Underwater sensor networks extensively use data aggregation at cluster heads to reduce transmissions. The "keep latest value" (snapshot) approach aligns with this - redundant older values waste precious bandwidth.

### 1.5 Why Not Use Existing Solutions Directly?

| Solution | Limitation for Our Use Case |
|----------|----------------------------|
| **DCCL** | Requires Protobuf IDL, C++ only, complex integration |
| **JANUS** | Physical/MAC layer focus, no application aggregation |
| **CCSDS POCKET+** | Designed for 500-2000B MTU, stateful (breaks with high loss) |
| **MQTT-SN** | Broker-based architecture, no pub/sub aggregation |

Our approach borrows concepts from these standards:
- **From DCCL**: Bit-level field encoding, bounded ranges, precision
- **From JANUS**: Stateless packets, interoperability focus
- **From POCKET+**: Change masks (→ our bitset), key dictionaries
- **From MQTT-SN**: Predefined topic IDs (→ our key table)

But we need a **simpler, Zenoh-native solution** that:
1. Works within Zenoh's transport layer
2. Supports snapshot mode (keep latest only)
3. Is stateless (high packet loss resilience)
4. Has minimal dependencies

---

## 2. Architecture Overview

### 2.1 Processing Pipeline

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      Deferred Link TX Pipeline                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  NetworkMessage                                                         │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────┐                                                    │
│  │ Express check   │──── Yes ──► [Urgent Path - see below]             │
│  └────────┬────────┘                                                    │
│           │ No                                                          │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │   Aggregator    │  Buffer by key, keep latest (snapshot mode)       │
│  │  HashMap<K,Msg> │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           │ [Timer fires OR buffer full]                                │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ Batch Encoder   │  Bitset or Key-ID based encoding                  │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │   Encryptor     │  Optional: AES-GCM-SIV or ChaCha20-Poly1305       │
│  │   (optional)    │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ send_normal()   │  Blocks until socket ready                        │
│  └─────────────────┘                                                    │
│                                                                         │
│  Urgent Path (Express flag set):                                        │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────┐                                                    │
│  │ Single Message  │  Key ID + Payload (no batching)                   │
│  │ Encoder         │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │   Encryptor     │                                                    │
│  │   (optional)    │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ send_urgent()   │  Immediate, may preempt pending                   │
│  └─────────────────┘                                                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 RX Pipeline

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      Deferred Link RX Pipeline                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Raw bytes from modem                                                   │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────┐                                                    │
│  │   Decryptor     │  Optional: verify + decrypt                       │
│  │   (optional)    │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ Batch Decoder   │  Bitset or Key-ID based decoding                  │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ Reconstruct     │  Build NetworkMessage(s) with full keys           │
│  │ NetworkMessages │                                                    │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  Normal Zenoh RX path                                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Link Selection via Priority

### 3.1 Concept

An external controller can force traffic to specific links by:
1. Configuring each link with a specific priority range
2. Using **QoS override interceptor** on ingress to set message priority
3. Zenoh's link selection routes to the matching link

This means **Priority is reserved for link selection** and cannot be used for urgent/normal distinction.

### 3.2 Example Multi-Link Setup

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Multi-Link Router                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Incoming message                                                       │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────┐                                                    │
│  │ QoS Override    │  Set priority based on controller decision        │
│  │ Interceptor     │  (key, source, link quality, etc.)                │
│  └────────┬────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │ Link Selection  │  Match priority to link's priority range          │
│  └────────┬────────┘                                                    │
│           │                                                             │
│     ┌─────┴─────┬──────────────┐                                        │
│     │           │              │                                        │
│     ▼           ▼              ▼                                        │
│  Priority 0-2  Priority 3-4  Priority 5-7                               │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                                  │
│  │ Fiber   │  │Satellite│  │Acoustic │                                  │
│  │ (fast)  │  │ (slow)  │  │(deferred│                                  │
│  └─────────┘  └─────────┘  └─────────┘                                  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Urgent Message Handling

### 4.1 Express Flag

Since priority is used for link selection, use the **Express flag** for urgent/normal distinction:

| CongestionControl | Meaning |
|-------------------|---------|
| `Drop` (express) | **Urgent**: bypass aggregator, `send_urgent()` |
| `Block` (default) | **Normal**: aggregated, `send_normal()` |

### 4.2 Publisher Usage

```rust
// Urgent alarm - bypass aggregation, send immediately
publisher.put(alarm_data).express().await?;

// Normal telemetry - will be aggregated
publisher.put(sensor_data).await?;  // Default is Block
```

### 4.3 Modem Interface

```rust
#[async_trait]
pub trait ConstrainedModem: Send + Sync {
    /// Send data normally (queued, respects channel availability)
    async fn send_normal(&self, data: &[u8]) -> ZResult<()>;
    
    /// Send data urgently (immediate, may preempt pending transmission)
    async fn send_urgent(&self, data: &[u8]) -> ZResult<()>;
    
    /// Receive data
    async fn recv(&self, buffer: &mut [u8]) -> ZResult<usize>;
}
```

---

## 5. Batch Encoding Formats

Two encoding formats are supported, optimized for different use cases:

### 5.1 Encoding Comparison

| Format | Overhead | Best For | Stateless |
|--------|----------|----------|-----------|
| **Bitset** | N/8 bytes fixed | Fixed-size payloads, many keys | Yes |
| **Key-ID** | 1-2 bytes/key | Variable payloads, few keys | Yes |

### 5.2 Bitset Encoding (Recommended for Acoustic)

Inspired by [CCSDS POCKET+](https://ccsds.org/Pubs/124x0b1.pdf) change masks and [MQTT-SN](http://www.steves-internet-guide.com/mqtt-sn-topic-names/) predefined topics.

**Concept**: Use a bitmask where bit N = 1 means "key N has a value in this batch". Payloads follow in key order.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      Bitset Encoding Format                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌────────┬──────────────────────────┬────────────────────────────────┐│
│  │ Header │ Presence Bitset          │ Payloads (in key order)        ││
│  │ (1B)   │ (ceil(N/8) bytes)        │                                ││
│  └────────┴──────────────────────────┴────────────────────────────────┘│
│                                                                         │
│  Header byte:                                                           │
│  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┐                     │
│  │  7  │  6  │  5  │  4  │  3  │  2  │  1  │  0  │                     │
│  ├─────┼─────┼─────┴─────┴─────┴─────┴─────┼─────┤                     │
│  │ Enc │ Exp │      Bitset Size (0-63)    │ Fmt │                     │
│  └─────┴─────┴─────────────────────────────┴─────┘                     │
│                                                                         │
│  Enc = Encrypted                                                        │
│  Exp = Contains express/urgent message                                  │
│  Fmt = 0: Bitset format, 1: Key-ID format                              │
│  Bitset Size = Number of bytes in bitset (0 = 1 byte, 63 = 64 bytes)   │
│                                                                         │
│  Example: 32 keys configured, keys 0, 2, 4 have values                  │
│  ┌────────┬──────────────────────────────────┬───────┬───────┬───────┐ │
│  │ 0x03   │ 0x15 0x00 0x00 0x00              │ Val0  │ Val2  │ Val4  │ │
│  │ header │ bitset (keys 0,2,4 set)          │ (4B)  │ (4B)  │ (4B)  │ │
│  └────────┴──────────────────────────────────┴───────┴───────┴───────┘ │
│                                                                         │
│  Total: 1 + 4 + 12 = 17 bytes for 3 values                             │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Advantages:**
- **Fixed overhead**: N/8 bytes regardless of how many keys have values
- **No per-key overhead**: No key IDs in payload
- **Simple decoding**: Iterate bits, read payloads in order
- **Stateless**: Each packet is self-contained (no encoder/decoder sync needed)

**Requirements:**
- All payloads must be **fixed size** (configured per key)
- Key table must be **identical** on both sides
- Maximum ~512 keys with 64-byte bitset

### 5.3 Key-ID Encoding (For Variable Payloads)

Similar to [MQTT-SN topic IDs](http://www.steves-internet-guide.com/mqtt-sn-topic-names/).

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      Key-ID Encoding Format                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌────────┬────────┬────────┬─────────┬────────┬────────┬─────────┐    │
│  │ Header │ KeyID  │ Length │ Payload │ KeyID  │ Length │ Payload │    │
│  │ (1B)   │ (1-2B) │ (1B)   │ (var)   │ (1-2B) │ (1B)   │ (var)   │    │
│  └────────┴────────┴────────┴─────────┴────────┴────────┴─────────┘    │
│                                                                         │
│  Header byte:                                                           │
│  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┐                     │
│  │  7  │  6  │  5  │  4  │  3  │  2  │  1  │  0  │                     │
│  ├─────┼─────┼─────┴─────┴─────┴─────┼─────┼─────┤                     │
│  │ Enc │ Exp │     Count (0-15)     │ K2B │ Fmt │                     │
│  └─────┴─────┴───────────────────────┴─────┴─────┘                     │
│                                                                         │
│  Count = Number of key-value pairs (0 = 1, 15 = 16)                    │
│  K2B = 2-byte key IDs (0 = 1-byte, 1 = 2-byte)                         │
│  Fmt = 1: Key-ID format                                                 │
│                                                                         │
│  Example: 3 keys with variable payloads                                 │
│  ┌────────┬──────┬─────┬──────────┬──────┬─────┬────────────┬────┐     │
│  │ 0x21   │ 0x01 │ 0x04│ temp(4B) │ 0x02 │ 0x04│ press(4B)  │... │     │
│  │ header │ key1 │ len │ payload  │ key2 │ len │ payload    │    │     │
│  └────────┴──────┴─────┴──────────┴──────┴─────┴────────────┴────┘     │
│                                                                         │
│  Total: 1 + 3×(1+1+4) = 19 bytes for 3 values                          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Advantages:**
- Supports **variable-size payloads**
- Works with **sparse key usage** (only transmit used keys)
- Supports up to 65535 keys with 2-byte IDs

**Disadvantages:**
- Higher per-key overhead (2-3 bytes per key)
- Slightly more complex parsing

### 5.4 Format Selection Guidelines

| Scenario | Recommended Format |
|----------|-------------------|
| Acoustic (32-256B MTU) with fixed sensors | **Bitset** |
| Acoustic with variable data (images, strings) | Key-ID |
| Satellite with fixed telemetry | **Bitset** |
| Satellite with mixed data | Key-ID |
| Tactical with fixed status reports | **Bitset** |
| < 8 keys per batch | Key-ID (simpler) |
| > 8 keys per batch, fixed size | **Bitset** (more efficient) |

### 5.5 Key Table Configuration

Both formats require a pre-configured key table on both endpoints:

```json5
{
  "key_table": {
    // Key index → (key expression, payload size for bitset)
    "0": { "key": "robot/sensor/temperature", "size": 4 },
    "1": { "key": "robot/sensor/pressure", "size": 4 },
    "2": { "key": "robot/sensor/depth", "size": 4 },
    "3": { "key": "robot/position/lat", "size": 8 },
    "4": { "key": "robot/position/lon", "size": 8 },
    "5": { "key": "robot/status", "size": 1 },
    "6": { "key": "robot/alarm", "size": 2 },
    "7": { "key": "robot/command", "size": 16 }
  }
}
```

### 5.6 Implementation

```rust
/// Key table entry
#[derive(Clone, Debug)]
pub struct KeyEntry {
    pub key_expr: OwnedKeyExpr,
    pub payload_size: usize,  // For bitset encoding (0 = variable)
}

/// Batch encoder supporting both formats
pub struct BatchEncoder {
    /// Key index → entry
    key_table: Vec<KeyEntry>,
    /// Key expression → index (for fast lookup)
    key_to_index: HashMap<OwnedKeyExpr, u16>,
    /// Preferred format
    format: EncodingFormat,
}

#[derive(Clone, Copy, Debug)]
pub enum EncodingFormat {
    /// Bitset format - fixed size payloads
    Bitset,
    /// Key-ID format - variable size payloads
    KeyId,
}

impl BatchEncoder {
    /// Encode multiple messages into a batch
    pub fn encode(&self, messages: &[(u16, &[u8])]) -> Vec<u8> {
        match self.format {
            EncodingFormat::Bitset => self.encode_bitset(messages),
            EncodingFormat::KeyId => self.encode_keyid(messages),
        }
    }
    
    fn encode_bitset(&self, messages: &[(u16, &[u8])]) -> Vec<u8> {
        let num_keys = self.key_table.len();
        let bitset_bytes = (num_keys + 7) / 8;
        
        // Header + bitset + payloads
        let mut buf = Vec::with_capacity(1 + bitset_bytes + messages.len() * 8);
        
        // Header: Fmt=0, bitset size
        let header = ((bitset_bytes - 1) as u8) << 2;  // Fmt=0
        buf.push(header);
        
        // Build bitset
        let mut bitset = vec![0u8; bitset_bytes];
        for (key_idx, _) in messages {
            let byte_idx = (*key_idx as usize) / 8;
            let bit_idx = (*key_idx as usize) % 8;
            bitset[byte_idx] |= 1 << bit_idx;
        }
        buf.extend_from_slice(&bitset);
        
        // Sort messages by key index and append payloads
        let mut sorted: Vec<_> = messages.iter().collect();
        sorted.sort_by_key(|(idx, _)| *idx);
        for (_, payload) in sorted {
            buf.extend_from_slice(payload);
        }
        
        buf
    }
    
    fn encode_keyid(&self, messages: &[(u16, &[u8])]) -> Vec<u8> {
        let use_2byte = self.key_table.len() > 256;
        let count = messages.len().min(16) - 1;
        
        // Header
        let header = 0x01  // Fmt=1
            | if use_2byte { 0x02 } else { 0x00 }
            | ((count as u8) << 2);
        
        let mut buf = Vec::new();
        buf.push(header);
        
        for (key_idx, payload) in messages {
            if use_2byte {
                buf.extend_from_slice(&key_idx.to_le_bytes());
            } else {
                buf.push(*key_idx as u8);
            }
            buf.push(payload.len() as u8);
            buf.extend_from_slice(payload);
        }
        
        buf
    }
}

/// Batch decoder
pub struct BatchDecoder {
    key_table: Vec<KeyEntry>,
}

impl BatchDecoder {
    /// Decode a batch into (key_index, payload) pairs
    pub fn decode(&self, data: &[u8]) -> Option<Vec<(u16, Vec<u8>)>> {
        if data.is_empty() {
            return None;
        }
        
        let header = data[0];
        let format = header & 0x01;
        
        if format == 0 {
            self.decode_bitset(data)
        } else {
            self.decode_keyid(data)
        }
    }
    
    fn decode_bitset(&self, data: &[u8]) -> Option<Vec<(u16, Vec<u8>)>> {
        let header = data[0];
        let bitset_bytes = ((header >> 2) & 0x3F) as usize + 1;
        
        if data.len() < 1 + bitset_bytes {
            return None;
        }
        
        let bitset = &data[1..1 + bitset_bytes];
        let mut payload_offset = 1 + bitset_bytes;
        let mut results = Vec::new();
        
        for key_idx in 0..self.key_table.len() {
            let byte_idx = key_idx / 8;
            let bit_idx = key_idx % 8;
            
            if byte_idx < bitset.len() && (bitset[byte_idx] & (1 << bit_idx)) != 0 {
                let size = self.key_table[key_idx].payload_size;
                if payload_offset + size > data.len() {
                    return None;  // Truncated
                }
                let payload = data[payload_offset..payload_offset + size].to_vec();
                results.push((key_idx as u16, payload));
                payload_offset += size;
            }
        }
        
        Some(results)
    }
    
    fn decode_keyid(&self, data: &[u8]) -> Option<Vec<(u16, Vec<u8>)>> {
        let header = data[0];
        let use_2byte = (header & 0x02) != 0;
        let count = ((header >> 2) & 0x0F) as usize + 1;
        
        let mut offset = 1;
        let mut results = Vec::new();
        
        for _ in 0..count {
            let key_idx = if use_2byte {
                if offset + 2 > data.len() { return None; }
                let idx = u16::from_le_bytes([data[offset], data[offset + 1]]);
                offset += 2;
                idx
            } else {
                if offset >= data.len() { return None; }
                let idx = data[offset] as u16;
                offset += 1;
                idx
            };
            
            if offset >= data.len() { return None; }
            let len = data[offset] as usize;
            offset += 1;
            
            if offset + len > data.len() { return None; }
            let payload = data[offset..offset + len].to_vec();
            offset += len;
            
            results.push((key_idx, payload));
        }
        
        Some(results)
    }
}
```

---

## 6. Optional Encryption

### 6.1 When Needed

| Link Type | Modem Encryption | Zenoh Encryption Needed |
|-----------|------------------|------------------------|
| Acoustic | Usually none | **Yes** |
| Satellite | Usually yes (TRANSEC) | Optional (defense in depth) |
| Tactical Radio | Usually yes (COMSEC) | Optional (defense in depth) |
| Commercial | Varies | Depends on threat model |

### 6.2 Requirements

For constrained links, encryption must be:
1. **Low overhead** - minimal size expansion
2. **Authenticated** - detect tampering/replay
3. **Efficient** - fast on embedded systems
4. **Nonce-misuse resistant** - safe with limited entropy

### 6.3 Recommended Algorithms

| Algorithm | Overhead | Speed | Nonce-Misuse Resistant |
|-----------|----------|-------|------------------------|
| **AES-128-GCM-SIV** | 16B tag | Fast (HW accel) | Yes |
| **ChaCha20-Poly1305** | 16B tag | Fast (no HW needed) | No |
| **AES-128-GCM** | 16B tag | Fast (HW accel) | No |

**Recommendation**: 
- **AES-128-GCM-SIV** if hardware AES available (most modern CPUs)
- **ChaCha20-Poly1305** for embedded without AES hardware

### 6.4 Key Management

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Key Management Options                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Option A: Pre-Shared Key (PSK)                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ - Configured on both ends before deployment                     │   │
│  │ - Simple, no key exchange overhead                              │   │
│  │ - Suitable for static deployments                               │   │
│  │ - Key rotation requires reconfiguration                         │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Option B: Session Key (derived during Zenoh session establishment)    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ - Derived from Zenoh's TLS/DTLS session (if used on other link) │   │
│  │ - Or use out-of-band key agreement                              │   │
│  │ - Forward secrecy possible                                      │   │
│  │ - More complex setup                                            │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Option C: Per-Message Key ID (for key rotation)                       │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ - 1-byte key ID in header selects from key table                │   │
│  │ - Allows gradual key rotation                                   │   │
│  │ - Both sides must have same key table                           │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Recommendation**: Start with PSK (Option A) for simplicity. Add key ID (Option C) for rotation.

### 6.5 Nonce Construction

For AEAD algorithms, nonce must be unique per message:

```rust
/// Nonce construction for constrained links
/// 12 bytes for AES-GCM / ChaCha20-Poly1305
pub struct NonceGenerator {
    /// 4-byte sender ID (truncated ZenohId)
    sender_id: [u8; 4],
    /// 8-byte counter (never reuse with same key)
    counter: AtomicU64,
}

impl NonceGenerator {
    pub fn next(&self) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[0..4].copy_from_slice(&self.sender_id);
        nonce[4..12].copy_from_slice(&self.counter.fetch_add(1, Ordering::SeqCst).to_le_bytes());
        nonce
    }
}
```

**Important**: Counter must persist across restarts or use time-based component to avoid nonce reuse.

### 6.6 Encrypted Wire Format

```
┌──────────────────────────────────────────────────────────────┐
│  Encrypted Message Format                                    │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────┬────────────────┬────────────────────────────────┐│
│  │ Nonce  │  Ciphertext    │  Auth Tag                      ││
│  │ (8B)   │  (variable)    │  (16B)                         ││
│  └────────┴────────────────┴────────────────────────────────┘│
│                                                              │
│  Nonce: only counter portion (sender ID implicit from link)  │
│                                                              │
│  Ciphertext contains the batch-encoded data                  │
│  (bitset or key-id format)                                   │
│                                                              │
│  Total overhead: 8 (nonce) + 16 (tag) = 24 bytes             │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### 6.7 Encryption Implementation

```rust
use aes_gcm_siv::{Aes128GcmSiv, Key, Nonce};
use aes_gcm_siv::aead::{Aead, KeyInit};

pub struct LinkEncryptor {
    cipher: Aes128GcmSiv,
    nonce_gen: NonceGenerator,
}

impl LinkEncryptor {
    pub fn new(key: &[u8; 16], sender_id: [u8; 4]) -> Self {
        let key = Key::<Aes128GcmSiv>::from_slice(key);
        Self {
            cipher: Aes128GcmSiv::new(key),
            nonce_gen: NonceGenerator::new(sender_id),
        }
    }
    
    /// Encrypt data, returns (nonce_counter, ciphertext_with_tag)
    pub fn encrypt(&self, plaintext: &[u8]) -> ZResult<(u64, Vec<u8>)> {
        let nonce_bytes = self.nonce_gen.next();
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| zerror!("Encryption failed: {}", e))?;
        
        let counter = u64::from_le_bytes(nonce_bytes[4..12].try_into().unwrap());
        Ok((counter, ciphertext))
    }
    
    /// Decrypt data
    pub fn decrypt(&self, sender_id: [u8; 4], counter: u64, ciphertext: &[u8]) -> ZResult<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[0..4].copy_from_slice(&sender_id);
        nonce_bytes[4..12].copy_from_slice(&counter.to_le_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| zerror!("Decryption failed: {}", e))
    }
}
```

### 6.8 Anti-Replay Protection

For links with high latency and reordering:

| Method | Window Size | Memory | Suitable For |
|--------|-------------|--------|--------------|
| Sliding window | 64-256 | 8-32 bytes | Most links |
| Time-based | 60-120 sec | 8 bytes | Very high latency (acoustic) |

```rust
pub struct ReplayProtection {
    highest_seq: u64,
    window: u64,  // 64-bit sliding window
}

impl ReplayProtection {
    pub fn check(&mut self, seq: u64) -> bool {
        if seq > self.highest_seq {
            let shift = (seq - self.highest_seq).min(64) as u32;
            self.window = self.window.checked_shl(shift).unwrap_or(0);
            self.window |= 1;
            self.highest_seq = seq;
            true
        } else if seq + 64 > self.highest_seq {
            let offset = (self.highest_seq - seq) as u32;
            let bit = 1u64 << offset;
            if self.window & bit != 0 {
                false  // Replay
            } else {
                self.window |= bit;
                true
            }
        } else {
            false  // Too old
        }
    }
}
```

---

## 7. Aggregator Design

### 7.1 Configuration

```rust
#[derive(Clone, Debug)]
pub struct DeferredLinkConfig {
    /// Maximum time to buffer before flushing
    pub max_buffer_time: Duration,
    
    /// Maximum number of keys to buffer
    pub max_keys: usize,
    
    /// Maximum total payload size to buffer
    pub max_payload_bytes: usize,
    
    /// Keep only latest value per key
    pub snapshot_mode: bool,
    
    /// Encoding format
    pub encoding: EncodingFormat,
    
    /// Key table configuration
    pub key_table: KeyTableConfig,
    
    /// Encryption configuration
    pub encryption: Option<EncryptionConfig>,
    
    /// Disable keep-alive
    pub disable_keepalive: bool,
}

#[derive(Clone, Debug)]
pub struct KeyTableConfig {
    pub entries: Vec<KeyEntryConfig>,
}

#[derive(Clone, Debug)]
pub struct KeyEntryConfig {
    pub key: String,
    pub size: usize,  // 0 for variable (requires Key-ID format)
}

#[derive(Clone, Debug)]
pub struct EncryptionConfig {
    pub algorithm: String,
    pub psk: Option<String>,
    pub key_file: Option<PathBuf>,
    pub replay_window: u32,
}
```

### 7.2 Link Type Presets

```rust
impl DeferredLinkConfig {
    /// Preset for acoustic underwater links (most constrained)
    pub fn acoustic() -> Self {
        Self {
            max_buffer_time: Duration::from_secs(5),
            max_keys: 32,
            max_payload_bytes: 200,
            snapshot_mode: true,
            encoding: EncodingFormat::Bitset,  // Most efficient for acoustic
            key_table: KeyTableConfig::default(),
            encryption: Some(EncryptionConfig::default()),
            disable_keepalive: true,
        }
    }
    
    /// Preset for slow satellite links (e.g., Iridium SBD)
    pub fn slow_satellite() -> Self {
        Self {
            max_buffer_time: Duration::from_secs(10),
            max_keys: 64,
            max_payload_bytes: 340,
            snapshot_mode: true,
            encoding: EncodingFormat::Bitset,
            key_table: KeyTableConfig::default(),
            encryption: None,  // TRANSEC at modem
            disable_keepalive: true,
        }
    }
    
    /// Preset for tactical radio links
    pub fn tactical_radio() -> Self {
        Self {
            max_buffer_time: Duration::from_secs(2),
            max_keys: 64,
            max_payload_bytes: 1000,
            snapshot_mode: true,
            encoding: EncodingFormat::Bitset,
            key_table: KeyTableConfig::default(),
            encryption: None,  // COMSEC at radio
            disable_keepalive: false,
        }
    }
}
```

---

## 8. Configuration Examples

### 8.1 Acoustic Link with Bitset Encoding

```json5
{
  "transport": {
    "unicast": {
      "link": {
        "unixpipe://acoustic_modem": {
          "deferred": {
            "enabled": true,
            "max_buffer_time_ms": 5000,
            "max_keys": 32,
            "max_payload_bytes": 200,
            "snapshot_mode": true,
            "disable_keepalive": true,
            "encoding": "bitset",
            
            "key_table": [
              { "key": "robot/sensor/temperature", "size": 4 },
              { "key": "robot/sensor/pressure", "size": 4 },
              { "key": "robot/sensor/depth", "size": 4 },
              { "key": "robot/position/lat", "size": 8 },
              { "key": "robot/position/lon", "size": 8 },
              { "key": "robot/status", "size": 1 },
              { "key": "robot/alarm", "size": 2 },
              { "key": "robot/command", "size": 16 }
            ],
            
            "encryption": {
              "enabled": true,
              "algorithm": "aes-128-gcm-siv",
              "psk": "0x000102030405060708090a0b0c0d0e0f",
              "replay_window": 256
            }
          },
          "priorities": [5],
          "reliability": "best_effort"
        }
      }
    }
  }
}
```

### 8.2 Satellite Link with Bitset Encoding

```json5
{
  "transport": {
    "unicast": {
      "link": {
        "tcp://192.168.1.100:9600": {
          "deferred": {
            "enabled": true,
            "max_buffer_time_ms": 10000,
            "max_keys": 64,
            "max_payload_bytes": 340,
            "snapshot_mode": true,
            "disable_keepalive": true,
            "encoding": "bitset",
            
            "key_table": [
              { "key": "vessel/position", "size": 16 },
              { "key": "vessel/heading", "size": 4 },
              { "key": "vessel/speed", "size": 4 },
              { "key": "vessel/status", "size": 1 },
              { "key": "command/waypoint", "size": 24 }
            ],
            
            "encryption": {
              "enabled": false
            }
          },
          "priorities": [4],
          "reliability": "best_effort"
        }
      }
    }
  }
}
```

### 8.3 Link with Variable Payloads (Key-ID Encoding)

```json5
{
  "transport": {
    "unicast": {
      "link": {
        "serial:///dev/ttyUSB0:115200": {
          "deferred": {
            "enabled": true,
            "max_buffer_time_ms": 2000,
            "max_keys": 32,
            "max_payload_bytes": 500,
            "snapshot_mode": true,
            "disable_keepalive": false,
            "encoding": "key_id",
            
            "key_table": [
              { "key": "unit/position", "size": 0 },
              { "key": "unit/status", "size": 0 },
              { "key": "intel/report", "size": 0 }
            ],
            
            "encryption": {
              "enabled": false
            }
          },
          "priorities": [3],
          "reliability": "best_effort"
        }
      }
    }
  }
}
```

---

## 9. Size Analysis

### 9.1 Bitset Format Efficiency

For a typical acoustic scenario with 32 configured keys:

| Keys in Batch | Bitset Overhead | Payload | Total | Efficiency |
|---------------|-----------------|---------|-------|------------|
| 1 (4B payload) | 1 + 4 = 5B | 4B | 9B | 44% |
| 4 (4B each) | 1 + 4 = 5B | 16B | 21B | 76% |
| 8 (4B each) | 1 + 4 = 5B | 32B | 37B | 86% |
| 16 (4B each) | 1 + 4 = 5B | 64B | 69B | 93% |

With encryption (+24B overhead):

| Keys in Batch | Total (encrypted) | Efficiency |
|---------------|-------------------|------------|
| 1 | 33B | 12% |
| 4 | 45B | 36% |
| 8 | 61B | 52% |
| 16 | 93B | 69% |

**Conclusion**: Bitset encoding works well when batching multiple keys. Single-key urgent messages should skip batching.

### 9.2 Comparison with Standard Zenoh

| Approach | 4 sensors × 4B payload |
|----------|------------------------|
| Standard Zenoh (full keys) | ~200B (4 × 50B headers) |
| Key-ID encoding | 1 + 4×6 = 25B |
| Bitset encoding | 1 + 4 + 16 = 21B |
| Bitset + encryption | 21 + 24 = 45B |

**Savings**: 77-90% bandwidth reduction vs standard Zenoh.

---

## 10. Implementation Phases

| Phase | Description | Effort |
|-------|-------------|--------|
| 1 | Add `DeferredLinkConfig` to link configuration | 1 day |
| 2 | Implement `Aggregator` buffer with snapshot mode | 2 days |
| 3 | Implement `BatchEncoder` (bitset + key-id) | 2 days |
| 4 | Implement `BatchDecoder` | 1 day |
| 5 | Modify `internal_schedule()` for deferred links | 1 day |
| 6 | Add deferred TX task with Express flag bypass | 2 days |
| 7 | Implement `LinkEncryptor` with AES-GCM-SIV | 2 days |
| 8 | Add anti-replay protection | 1 day |
| 9 | Add modem interface (`send_normal`/`send_urgent`) | 1 day |
| 10 | Testing with simulated constrained links | 2 days |
| **Total** | | **~15 days** |

---

## 11. Decisions Summary

| Question | Decision |
|----------|----------|
| Reliable messages | Ignore reliability flag, treat all as best-effort |
| Control messages | Not aggregated. Keep-alive disabled. |
| Message ordering | Latest timestamp wins (snapshot mode) |
| Urgent messages | **Express flag** bypasses aggregator |
| Encoding format | **Bitset** for fixed payloads, Key-ID for variable |
| Link selection | Priority-based, controlled by QoS override |
| Encryption | Optional, AES-128-GCM-SIV recommended |
| Key management | PSK initially, key ID for rotation |
| Anti-replay | Sliding window (64-256 messages) |
| State management | **Stateless** - each packet self-contained |

---

## 12. References

### Underwater Acoustic Communication

- [JANUS: The NATO Underwater Communications Standard (IEEE)](https://ieeexplore.ieee.org/document/7017134/)
- [JANUS Wiki](https://www.januswiki.com/) - Community resources for STANAG 4748
- [WHOI Compact Control Language (CCL)](https://acomms.whoi.edu/wp-content/uploads/sites/20/2014/08/StokeyFreitagGrund_CCL_2005.pdf) - Original 32-byte AUV command protocol
- [DCCL v3: Dynamic Compact Control Language](https://libdccl.org/3.0/) - Bit-level encoding for acoustic modems
- [DCCL Source Code (GitHub)](https://github.com/GobySoft/dccl)
- [Goby Underwater Autonomy Project](https://gobysoft.org/) - Complete acoustic networking framework
- [DCCL Version 3 Paper (IEEE Oceans 2015)](https://gobysoft.org/dl/oceans2015_dccl.pdf)
- [EvoLogics S2C Technology](https://www.evologics.com/s2c-technology) - Commercial acoustic modem with built-in compression

### Data Aggregation in Underwater Networks

- [Cluster-based Data Aggregation in UASNs (IEEE)](https://ieeexplore.ieee.org/document/6420597/)
- [Data Aggregation in UWSNs: Recent Approaches (ScienceDirect)](https://www.sciencedirect.com/science/article/pii/S1319157817300484)
- [Feature-level Data Aggregation for UASNs (IEEE)](https://ieeexplore.ieee.org/document/10608073/)
- [Energy-efficient Compressed Data Aggregation (Springer)](https://link.springer.com/article/10.1007/s11276-015-1076-z)

### Space and IoT Standards

- [CCSDS 124.0-B-1: Robust Compression of Fixed-Length Housekeeping Data](https://ccsds.org/Pubs/124x0b1.pdf)
- [MQTT-SN Topic Names and Identifiers](http://www.steves-internet-guide.com/mqtt-sn-topic-names/)
- [ESA POCKET+ Overview](https://www.esa.int/Enabling_Support/Space_Engineering_Technology/Data_Compression_and_Decompression_for_Remote_Monitoring_and_Control_POCKET)
- [VisionSpace PocketPlus Implementation](https://github.com/visionspacetec/PocketPlus)
- [CoAP vs MQTT-SN Performance Evaluation](https://www.mdpi.com/2504-3900/31/1/49)
- [CCSDS XTCE Telemetry Dictionaries](https://ccsds.org/Pubs/660x2g2.pdf)

---

## 13. Future Enhancements (Not in First Iteration)

The following patterns are well-established in underwater acoustic and constrained network research but are deferred for simplicity in the first implementation:

### 13.1 Delta Encoding

Instead of sending full values, send only the difference from a previous "key frame". This exploits temporal correlation in sensor data.

**How it works:**
- Periodically send full "key frame" values
- Between key frames, send only deltas (differences)
- If delta exceeds `max_delta`, automatically send new key frame

**Example (DCCL-style):**
```
Temperature (0-40°C, precision=0.1°C):
- Full value:  9 bits (400 possible values)
- Delta ±2°C:  6 bits (40 possible values)
- Savings:     ~33% per sample, 60%+ with temporal correlation
```

**Challenges for acoustic:**
- Packet loss breaks delta chain → need periodic key frames
- High latency means stale reference → larger max_delta needed
- Adds complexity to encoder/decoder state management

**References:**
- [DCCL delta-difference encoding](https://libdccl.org/3.0/)
- [DCCL Version 3 Paper](https://gobysoft.org/dl/oceans2015_dccl.pdf)

### 13.2 Adaptive Sampling Hints

Publishers provide hints about data dynamics to help the aggregator make smarter decisions.

**Hint types:**
```rust
pub struct SamplingHint {
    pub change_rate: ChangeRate,  // Static, Slow, Medium, Fast, Bursty
    pub max_age_ms: Option<u32>,  // Acceptable staleness
    pub freshness_priority: u8,   // Higher = send sooner
}
```

**How aggregator uses hints:**
- `Static` keys: only include if changed
- `Slow` keys: include if changed OR max_age exceeded
- `Fast` keys: always include latest value
- `Bursty` keys: bypass aggregator (use Express)

**Research results:**
- [AdaM Framework](https://dl.acm.org/doi/10.1145/3628353.3628545): 74% data reduction, 89% accuracy
- [L-SIP Algorithm](https://ieeexplore.ieee.org/document/9500326/): up to 79% sample reduction

**Benefits:**
- Skip unchanged slow sensors → bandwidth savings
- Prioritize fast-changing data in limited MTU
- Combine with delta encoding for maximum compression

### 13.3 Lossy Compression with Bounded Error

Accept controlled data loss when error is within acceptable bounds.

**Approach:**
- User specifies acceptable error threshold (e.g., ±0.5°C)
- Encoder quantizes more aggressively
- Much higher compression than lossless

**Research:**
- [SZ4IoT](https://www.researchgate.net/publication/305525249_Fast_Error-Bounded_Lossy_HPC_Data_Compression_with_SZ): Lightweight lossy compression for IoT
- Hybrid approaches: 2.1× lossless, 7.8× lossy compression

---

## 14. Summary

For constrained link aggregation:

1. **Deferred Link concept** - buffer and aggregate before sending
2. **Bitset encoding** - inspired by CCSDS POCKET+ masks, optimized for fixed-size payloads
3. **Key-ID encoding** - similar to MQTT-SN, for variable payloads
4. **Stateless design** - each packet self-contained, no encoder/decoder sync
5. **Express flag for urgent** - bypasses aggregation for alarms
6. **Priority for link selection** - external controller routes via QoS override
7. **Optional encryption** - AES-128-GCM-SIV with 24-byte overhead
8. **Designed for acoustic first** - if it works for 32-256B MTU, it works everywhere
