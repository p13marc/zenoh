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

//! Link control for external controller integration.
//!
//! This module provides APIs for an external controller to override
//! link selection decisions in Zenoh.

use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};

use zenoh_protocol::core::{Locator, ZenohIdProto};

/// Link overrides managed by an external controller.
///
/// When overrides are set, they take precedence over the default
/// link selection algorithm.
#[derive(Debug, Default)]
pub struct LinkOverrides {
    /// Forced links per peer: when set, all traffic to the peer uses this link
    forced_links: RwLock<HashMap<ZenohIdProto, Locator>>,
    /// Disabled links: excluded from selection but still connected
    disabled_links: RwLock<HashSet<Locator>>,
}

impl LinkOverrides {
    /// Create a new empty link overrides manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Force all traffic to a specific peer to use a specific link.
    ///
    /// When set, Zenoh will use ONLY this link for the specified peer,
    /// regardless of priority, reliability, or other factors.
    ///
    /// # Arguments
    /// * `peer` - The remote peer ZenohId
    /// * `link` - The locator of the link to use
    pub fn set_forced_link(&self, peer: ZenohIdProto, link: Locator) {
        let mut forced = self.forced_links.write().unwrap();
        forced.insert(peer, link);
        tracing::debug!(
            "Forced link set for peer {}: {}",
            peer,
            forced.get(&peer).unwrap()
        );
    }

    /// Clear the forced link for a peer, returning to default behavior.
    ///
    /// After calling this, Zenoh will use its default link selection
    /// logic for the specified peer.
    pub fn clear_forced_link(&self, peer: &ZenohIdProto) {
        let mut forced = self.forced_links.write().unwrap();
        if forced.remove(peer).is_some() {
            tracing::debug!("Forced link cleared for peer {}", peer);
        }
    }

    /// Get the forced link for a peer, if any.
    pub fn get_forced_link(&self, peer: &ZenohIdProto) -> Option<Locator> {
        let forced = self.forced_links.read().unwrap();
        forced.get(peer).cloned()
    }

    /// Check if a peer has a forced link.
    pub fn has_forced_link(&self, peer: &ZenohIdProto) -> bool {
        let forced = self.forced_links.read().unwrap();
        forced.contains_key(peer)
    }

    /// Disable a link entirely. Traffic will not use this link.
    ///
    /// The link remains connected but is excluded from selection.
    /// OAM probes may continue to be sent for monitoring.
    pub fn disable_link(&self, link: Locator) {
        let mut disabled = self.disabled_links.write().unwrap();
        if disabled.insert(link.clone()) {
            tracing::debug!("Link disabled: {}", link);
        }
    }

    /// Re-enable a previously disabled link.
    pub fn enable_link(&self, link: &Locator) {
        let mut disabled = self.disabled_links.write().unwrap();
        if disabled.remove(link) {
            tracing::debug!("Link enabled: {}", link);
        }
    }

    /// Check if a link is currently disabled.
    pub fn is_disabled(&self, link: &Locator) -> bool {
        let disabled = self.disabled_links.read().unwrap();
        disabled.contains(link)
    }

    /// Get all current link overrides (peer -> forced link mapping).
    pub fn get_all_forced_links(&self) -> HashMap<ZenohIdProto, Locator> {
        let forced = self.forced_links.read().unwrap();
        forced.clone()
    }

    /// Get all currently disabled links.
    pub fn get_disabled_links(&self) -> Vec<Locator> {
        let disabled = self.disabled_links.read().unwrap();
        disabled.iter().cloned().collect()
    }

    /// Clear all overrides (forced links and disabled links).
    pub fn clear_all(&self) {
        {
            let mut forced = self.forced_links.write().unwrap();
            forced.clear();
        }
        {
            let mut disabled = self.disabled_links.write().unwrap();
            disabled.clear();
        }
        tracing::debug!("All link overrides cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_zid() -> ZenohIdProto {
        ZenohIdProto::try_from([1u8; 16]).unwrap()
    }

    fn test_locator() -> Locator {
        "tcp/192.168.1.100:7447".parse().unwrap()
    }

    #[test]
    fn test_forced_link() {
        let overrides = LinkOverrides::new();
        let peer = test_zid();
        let link = test_locator();

        assert!(!overrides.has_forced_link(&peer));
        assert!(overrides.get_forced_link(&peer).is_none());

        overrides.set_forced_link(peer, link.clone());
        assert!(overrides.has_forced_link(&peer));
        assert_eq!(overrides.get_forced_link(&peer), Some(link));

        overrides.clear_forced_link(&peer);
        assert!(!overrides.has_forced_link(&peer));
    }

    #[test]
    fn test_disabled_link() {
        let overrides = LinkOverrides::new();
        let link = test_locator();

        assert!(!overrides.is_disabled(&link));

        overrides.disable_link(link.clone());
        assert!(overrides.is_disabled(&link));

        overrides.enable_link(&link);
        assert!(!overrides.is_disabled(&link));
    }

    #[test]
    fn test_clear_all() {
        let overrides = LinkOverrides::new();
        let peer = test_zid();
        let link = test_locator();

        overrides.set_forced_link(peer, link.clone());
        overrides.disable_link(link.clone());

        assert!(overrides.has_forced_link(&peer));
        assert!(overrides.is_disabled(&link));

        overrides.clear_all();

        assert!(!overrides.has_forced_link(&peer));
        assert!(!overrides.is_disabled(&link));
    }
}
