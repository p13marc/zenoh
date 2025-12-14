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

//! OAM (Operations, Administration, Maintenance) module for link quality measurement.
//!
//! This module provides functionality for measuring link quality metrics including:
//! - Round-trip time (RTT)
//! - Jitter (RFC 3550 algorithm)
//! - Packet loss ratio
//!
//! The metrics are collected via OAM probes exchanged between peers and exposed
//! for external controllers to make link selection decisions.

mod link_control;
mod metrics;
mod prober;
mod responder;

pub use link_control::LinkOverrides;
pub use metrics::{LinkQualityMetrics, LinkState};
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
