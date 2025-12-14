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

//! # OAM (Operations, Administration, Maintenance) Module
//!
//! This module provides link quality measurement and external controller integration
//! for Zenoh transports. It enables monitoring and control of link selection based
//! on real-time quality metrics.
//!
//! ## Features
//!
//! - **Link Quality Metrics**: RTT, jitter (RFC 3550), packet loss ratio
//! - **Probe Types**: Loopback (LBM/LBR), Delay Measurement (DMM/DMR), Synthetic Loss (SLM/SLR)
//! - **External Controller Integration**: Force/disable links, query metrics via admin space
//! - **Automatic Link Selection**: OAM-aware link selection respects controller overrides
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    External Controller                       │
//! │  (reads metrics from admin space, sets link overrides)       │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Session API                             │
//! │  link_quality_metrics(), set_forced_link(), disable_link()   │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Transport Layer                           │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
//! │  │   Link 1    │  │   Link 2    │  │   Link 3    │          │
//! │  │  OamProber  │  │  OamProber  │  │  OamProber  │          │
//! │  │  Metrics    │  │  Metrics    │  │  Metrics    │          │
//! │  └─────────────┘  └─────────────┘  └─────────────┘          │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Configuration
//!
//! OAM is configured in the zenoh configuration file under `transport.unicast.oam`:
//!
//! ```json5
//! {
//!   transport: {
//!     unicast: {
//!       oam: {
//!         enabled: true,           // Enable OAM probing
//!         probe_interval_ms: 100,  // Send probe every 100ms
//!         probe_timeout_ms: 500,   // Consider probe lost after 500ms
//!         sample_window: 20,       // Use 20 samples for moving average
//!         failure_threshold: 3,    // Mark link failed after 3 consecutive losses
//!         publish_metrics: true,   // Expose metrics in admin space
//!         publish_interval_ms: 500 // Update admin space every 500ms
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! ## Usage
//!
//! ### Reading Link Quality Metrics
//!
//! ```ignore
//! // Get metrics for all links
//! let metrics = session.link_quality_metrics();
//! for m in metrics {
//!     println!("Link {}: RTT={:?}, jitter={:?}, loss={}%",
//!         m.locator, m.rtt_avg, m.jitter, m.loss_ratio * 100.0);
//! }
//!
//! // Get metrics for a specific peer
//! let peer_metrics = session.link_quality_metrics_for_peer(peer_zid);
//! ```
//!
//! ### Controlling Link Selection
//!
//! ```ignore
//! // Force traffic to use a specific link
//! session.set_forced_link(peer_zid, preferred_locator);
//!
//! // Clear forced link (return to automatic selection)
//! session.clear_forced_link(peer_zid);
//!
//! // Disable a problematic link
//! session.disable_link(bad_locator);
//!
//! // Re-enable a link
//! session.enable_link(&bad_locator);
//! ```
//!
//! ### Admin Space Metrics
//!
//! Metrics are exposed in the admin space under `@/<zid>/...` and include:
//! - `oam_metrics`: Array of per-link metrics with RTT, jitter, loss, state
//!
//! Query example:
//! ```ignore
//! session.get("@/*/router/**").await
//! ```
//!
//! ## Probe Types
//!
//! | Type | Request | Reply | Purpose |
//! |------|---------|-------|---------|
//! | Loopback | LBM (0xF001) | LBR (0xF002) | Basic RTT measurement |
//! | Delay | DMM (0xF003) | DMR (0xF004) | One-way delay (requires clock sync) |
//! | Loss | SLM (0xF005) | SLR (0xF006) | Packet loss detection |
//!
//! ## Link States
//!
//! - **Unknown**: Initial state, no probes exchanged yet
//! - **Operational**: Link is responding normally
//! - **Degraded**: Link is responding but with degraded metrics
//! - **Marginal**: Link is marginally operational
//! - **Failed**: Link is not responding (exceeded failure threshold)

mod link_control;
mod metrics;
mod prober;
mod responder;

pub use link_control::LinkOverrides;
pub use metrics::{LinkQualityMetrics, LinkState};
pub(crate) use prober::run_oam_prober;
pub use prober::OamProber;
pub use responder::OamResponder;

use std::time::Duration;
use zenoh_config::OamConf;

/// Runtime OAM configuration derived from zenoh-config
#[derive(Debug, Clone)]
pub struct OamConfig {
    /// Whether OAM probing is enabled
    pub enabled: bool,
    /// Probe interval
    pub probe_interval: Duration,
    /// Probe timeout
    pub probe_timeout: Duration,
    /// Number of samples for moving average calculations
    pub sample_window: usize,
    /// Consecutive probe failures before marking link as down
    pub failure_threshold: u32,
    /// Publish metrics on admin space
    pub publish_metrics: bool,
    /// Metrics publication interval
    pub publish_interval: Duration,
}

impl From<&OamConf> for OamConfig {
    fn from(conf: &OamConf) -> Self {
        Self {
            enabled: *conf.enabled(),
            probe_interval: Duration::from_millis(*conf.probe_interval_ms()),
            probe_timeout: Duration::from_millis(*conf.probe_timeout_ms()),
            sample_window: *conf.sample_window(),
            failure_threshold: *conf.failure_threshold(),
            publish_metrics: *conf.publish_metrics(),
            publish_interval: Duration::from_millis(*conf.publish_interval_ms()),
        }
    }
}

impl Default for OamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            probe_interval: Duration::from_millis(100),
            probe_timeout: Duration::from_millis(500),
            sample_window: 20,
            failure_threshold: 3,
            publish_metrics: true,
            publish_interval: Duration::from_millis(500),
        }
    }
}
