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

//! Integration tests for OAM (Operations, Administration, Maintenance) link quality measurement.

#![cfg(feature = "transport_oam")]

use std::{convert::TryFrom, sync::Arc, time::Duration};

use zenoh_core::ztimeout;
use zenoh_link::EndPoint;
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{oam::OamConfig, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

// Transport Handler
#[derive(Default)]
struct SHRouter;

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct SHClient;

impl TransportEventHandler for SHClient {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

fn make_oam_config() -> OamConfig {
    OamConfig {
        enabled: true,
        probe_interval: Duration::from_millis(50),
        probe_timeout: Duration::from_millis(200),
        sample_window: 10,
        failure_threshold: 3,
        publish_metrics: false,
        publish_interval: Duration::from_millis(500),
    }
}

async fn oam_transport_test(endpoint: &EndPoint) {
    /* [ROUTER] */
    let router_id = ZenohIdProto::try_from([1]).unwrap();

    let router_handler = Arc::new(SHRouter);
    let unicast = TransportManager::config_unicast()
        .max_sessions(1)
        .oam(make_oam_config());
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build_test(router_handler.clone())
        .unwrap();

    /* [CLIENT] */
    let client_id = ZenohIdProto::try_from([2]).unwrap();

    let unicast = TransportManager::config_unicast()
        .max_sessions(1)
        .oam(make_oam_config());
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build_test(Arc::new(SHClient))
        .unwrap();

    // Add listener on router
    println!("Adding listener on router...");
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    assert!(res.is_ok());

    // Connect client to router
    println!("Connecting client to router...");
    let res = ztimeout!(client_manager.open_transport_unicast(endpoint.clone()));
    assert!(res.is_ok());
    let client_transport = res.unwrap();

    // Wait for OAM probes to be exchanged
    println!("Waiting for OAM probes...");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get link quality metrics from client transport
    let metrics = client_transport.get_link_quality_metrics();
    println!("Client link quality metrics: {:?}", metrics);
    assert!(metrics.is_ok());
    let metrics = metrics.unwrap();

    // We should have metrics for at least one link
    if !metrics.is_empty() {
        let link_metrics = &metrics[0];
        println!("  Locator: {:?}", link_metrics.locator);
        println!("  RTT current: {:?}", link_metrics.rtt_current);
        println!("  RTT avg: {:?}", link_metrics.rtt_avg);
        println!("  Jitter: {:?}", link_metrics.jitter);
        println!("  Loss ratio: {:?}", link_metrics.loss_ratio);
        println!("  Probes sent: {}", link_metrics.tx_probe_count);
        println!("  Probes received: {}", link_metrics.rx_probe_count);

        // Verify that some probes were sent
        assert!(
            link_metrics.tx_probe_count > 0,
            "Expected some probes to be sent"
        );
    }

    // Get metrics from router side
    let router_transports = router_manager.get_transports_unicast().await;
    if !router_transports.is_empty() {
        let router_transport = &router_transports[0];
        let router_metrics = router_transport.get_link_quality_metrics();
        println!("Router link quality metrics: {:?}", router_metrics);
        assert!(router_metrics.is_ok());
    }

    // Close transports
    println!("Closing transports...");
    let res = ztimeout!(client_transport.close());
    assert!(res.is_ok());

    tokio::time::sleep(SLEEP).await;

    // Close managers
    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn oam_tcp_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 18000).parse().unwrap();
    oam_transport_test(&endpoint).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn oam_udp_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 18001).parse().unwrap();
    oam_transport_test(&endpoint).await;
}
