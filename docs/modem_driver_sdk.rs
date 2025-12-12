//! Zenoh Modem Driver SDK
//!
//! This module provides traits and utilities for implementing modem drivers
//! that integrate with Zenoh's OAM link quality measurement system.
//!
//! ## Version History
//! - v1.0.0: Initial SDK with OAM/control plane traits
//! - v1.1.0: Added LinkProvider traits for dynamic link management
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use zenoh_modem_sdk::prelude::*;
//!
//! struct MyRadioDriver {
//!     // Your driver state
//! }
//!
//! impl ModemDriver for MyRadioDriver {
//!     fn info(&self) -> ModemInfo { /* ... */ }
//!     fn poll_metrics(&self) -> ModemMetrics { /* ... */ }
//!     fn state(&self) -> ModemState { /* ... */ }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let driver = MyRadioDriver::new();
//!     let server = ModemServer::new(driver)
//!         .socket_path("/run/zenoh/modem_radio0.sock")
//!         .metrics_interval(Duration::from_millis(500))
//!         .build();
//!
//!     server.run().await.unwrap();
//! }
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{broadcast, RwLock};

// Re-export types from the ICD
pub use crate::modem_driver_types::*;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{
        // Core traits
        AcousticMetrics,
        Alert,
        AlertData,
        AlertEmitter,
        AlertSeverity,
        AlertType,
        BandwidthMetrics,
        BroadcastCapabilities,
        ErrorMetrics,
        // Link types (v1.1.0)
        LinkAvailable,
        LinkCapabilities,
        LinkEventEmitter,
        LinkProperties,
        // Link provider traits (v1.1.0)
        LinkProvider,
        LinkProviderAsync,
        LinkQuery,
        LinkQueryResponse,
        LinkType,
        LinkUnavailable,
        LinkUnavailableReason,
        LinkUpdate,
        ModemDriver,
        ModemDriverAsync,
        ModemInfo,
        ModemMetrics,
        ModemServer,
        ModemServerBuilder,
        ModemState,
        ModemType,
        OperatingMode,
        PriorityRange,
        RadioMetrics,
        Reliability,
        Request,
        RequestData,
        RequestHandler,
        Response,
        ResponseData,
        ResponseStatus,
        SatelliteMetrics,
        SignalMetrics,
        TargetInfo,
        TargetType,
        UnicastCapabilities,
    };
}

// ============================================================================
// CORE TRAITS
// ============================================================================

/// Core trait for synchronous modem drivers.
///
/// Implement this trait for simple modem drivers that can provide metrics
/// synchronously (e.g., reading from shared memory or cached values).
///
/// # Example
///
/// ```rust,ignore
/// struct MyModem {
///     cached_metrics: Mutex<ModemMetrics>,
/// }
///
/// impl ModemDriver for MyModem {
///     fn info(&self) -> ModemInfo {
///         ModemInfo {
///             modem_id: "radio0".into(),
///             modem_type: ModemType::RadioTactical,
///             model: "Harris PRC-152A".into(),
///             firmware_version: "2.1.0".into(),
///             ..Default::default()
///         }
///     }
///
///     fn poll_metrics(&self) -> ModemMetrics {
///         self.cached_metrics.lock().unwrap().clone()
///     }
///
///     fn state(&self) -> ModemState {
///         self.cached_metrics.lock().unwrap().state
///     }
/// }
/// ```
pub trait ModemDriver: Send + Sync + 'static {
    /// Return static modem information.
    /// Called once on connection establishment.
    fn info(&self) -> ModemInfo;

    /// Poll current metrics from the modem.
    /// Called periodically at the configured interval.
    fn poll_metrics(&self) -> ModemMetrics;

    /// Get current modem state.
    /// May be called independently of poll_metrics.
    fn state(&self) -> ModemState;

    /// Get current available bandwidth.
    /// Default implementation extracts from metrics.
    fn available_bandwidth(&self) -> BandwidthMetrics {
        self.poll_metrics().bandwidth
    }

    /// Check if the modem hardware is healthy.
    /// Default returns true if state is not Error.
    fn is_healthy(&self) -> bool {
        !matches!(self.state(), ModemState::Error | ModemState::Disconnected)
    }
}

/// Async trait for modem drivers that require async operations.
///
/// Use this for drivers that need to perform I/O (serial, network, etc.)
/// to retrieve metrics.
///
/// # Example
///
/// ```rust,ignore
/// struct SerialModem {
///     port: tokio_serial::SerialStream,
/// }
///
/// #[async_trait]
/// impl ModemDriverAsync for SerialModem {
///     async fn info(&self) -> ModemInfo {
///         // Query modem via serial
///         self.send_command("AT+INFO").await.parse()
///     }
///
///     async fn poll_metrics(&self) -> ModemMetrics {
///         // Query multiple parameters
///         let snr = self.send_command("AT+SNR?").await;
///         let rssi = self.send_command("AT+RSSI?").await;
///         // ... build metrics
///     }
/// }
/// ```
#[async_trait]
pub trait ModemDriverAsync: Send + Sync + 'static {
    /// Return static modem information (async version).
    async fn info(&self) -> ModemInfo;

    /// Poll current metrics from the modem (async version).
    async fn poll_metrics(&self) -> ModemMetrics;

    /// Get current modem state (async version).
    async fn state(&self) -> ModemState;

    /// Get current available bandwidth (async version).
    async fn available_bandwidth(&self) -> BandwidthMetrics {
        self.poll_metrics().await.bandwidth
    }

    /// Check if the modem hardware is healthy (async version).
    async fn is_healthy(&self) -> bool {
        !matches!(
            self.state().await,
            ModemState::Error | ModemState::Disconnected
        )
    }
}

/// Trait for emitting alerts to Zenoh.
///
/// Implement this to provide predictive alerts about link quality changes.
///
/// # Example
///
/// ```rust,ignore
/// impl AlertEmitter for MySatelliteModem {
///     fn subscribe_alerts(&self) -> broadcast::Receiver<Alert> {
///         self.alert_tx.subscribe()
///     }
/// }
///
/// // In your monitoring loop:
/// if elevation < 15.0 && elevation_trend < 0.0 {
///     self.alert_tx.send(Alert {
///         alert_type: AlertType::HandoverImminent,
///         severity: AlertSeverity::Warning,
///         data: AlertData::HandoverImminent {
///             estimated_duration_ms: 5000,
///             expected_outage_ms: 500,
///             ..Default::default()
///         },
///         ..Default::default()
///     });
/// }
/// ```
pub trait AlertEmitter: Send + Sync {
    /// Subscribe to alert notifications.
    /// Returns a broadcast receiver that will receive alerts.
    fn subscribe_alerts(&self) -> broadcast::Receiver<Alert>;
}

/// Trait for handling requests from Zenoh.
///
/// Implement this to respond to mode change requests, traffic hints, etc.
///
/// # Example
///
/// ```rust,ignore
/// impl RequestHandler for MyRadioModem {
///     fn handle_request(&self, request: Request) -> Response {
///         match request.data {
///             RequestData::SetMode { mode, .. } => {
///                 self.set_operating_mode(mode);
///                 Response {
///                     request_id: request.request_id,
///                     status: ResponseStatus::Success,
///                     data: Some(ResponseData::ModeSet {
///                         applied_mode: mode,
///                         effective_until_ns: None,
///                     }),
///                     error_message: None,
///                 }
///             }
///             RequestData::TrafficHint { expected_tx_bps, priority, .. } => {
///                 // Adjust modem parameters based on hint
///                 self.optimize_for_traffic(expected_tx_bps, priority);
///                 Response::success(request.request_id)
///             }
///             _ => Response::not_supported(request.request_id),
///         }
///     }
/// }
/// ```
pub trait RequestHandler: Send + Sync {
    /// Handle a request from Zenoh.
    fn handle_request(&self, request: Request) -> Response;
}

/// Async version of RequestHandler.
#[async_trait]
pub trait RequestHandlerAsync: Send + Sync {
    /// Handle a request from Zenoh (async version).
    async fn handle_request(&self, request: Request) -> Response;
}

/// Trait for modem drivers that support configuration updates.
pub trait Configurable: Send + Sync {
    /// Handle a configuration update from Zenoh.
    fn handle_configure(&self, config: Configure) -> ConfigureAck;
}

// ============================================================================
// LINK PROVIDER TRAITS (v1.1.0)
// ============================================================================

/// Trait for modem drivers that provide dynamic transport links.
///
/// Implement this trait for modems that can create Unix socket endpoints
/// for Zenoh to connect to remote targets. The modem driver is responsible
/// for:
/// - Creating Unix sockets for each reachable target
/// - Notifying Zenoh when links become available/unavailable
/// - Bridging data between Unix sockets and the physical transport
///
/// # Example
///
/// ```rust,ignore
/// struct MyRadioDriver {
///     link_tx: broadcast::Sender<LinkEvent>,
///     active_links: RwLock<HashMap<String, TargetSocket>>,
/// }
///
/// impl LinkProvider for MyRadioDriver {
///     fn link_capabilities(&self) -> LinkCapabilities {
///         LinkCapabilities {
///             socket_base_path: "/run/zenoh/modem/radio0".into(),
///             unicast: UnicastCapabilities {
///                 max_mtu: 1400,
///                 min_mtu: 64,
///                 supports_priorities: true,
///                 priority_range: Some(PriorityRange { min: 0, max: 7 }),
///                 supports_express: true,
///                 express_priority_threshold: Some(2),
///                 supports_reliable: true,
///                 supports_best_effort: true,
///                 default_reliability: Reliability::Reliable,
///                 max_targets: 16,
///                 max_pending_bytes: 65536,
///             },
///             broadcast: None,
///             multicast: None,
///         }
///     }
///
///     fn active_links(&self) -> Vec<LinkAvailable> {
///         self.active_links.read().values()
///             .map(|s| s.to_link_available())
///             .collect()
///     }
///
///     fn create_link(&self, target: &TargetInfo) -> Result<LinkAvailable, LinkError> {
///         // Create Unix socket for target
///         let socket_path = format!("{}/targets/{}.sock",
///             self.link_capabilities().socket_base_path,
///             target.target_id);
///
///         // Set up bridging to physical transport
///         self.setup_target_bridge(&socket_path, target)?;
///
///         Ok(LinkAvailable::new(
///             format!("link-{}", target.target_id),
///             format!("unixpipe/{}", socket_path),
///             target.clone(),
///         ))
///     }
///
///     fn close_link(&self, link_id: &str) -> Result<(), LinkError> {
///         // Clean up socket and stop bridging
///         self.active_links.write().remove(link_id);
///         Ok(())
///     }
/// }
/// ```
pub trait LinkProvider: Send + Sync + 'static {
    /// Get the link capabilities for this modem.
    /// Called once during initialization to understand what the modem can provide.
    fn link_capabilities(&self) -> LinkCapabilities;

    /// Get all currently active links.
    fn active_links(&self) -> Vec<LinkAvailable>;

    /// Create a new link to a target.
    /// The driver should create a Unix socket and set up bridging.
    fn create_link(&self, target: &TargetInfo) -> Result<LinkAvailable, LinkError>;

    /// Close an existing link.
    fn close_link(&self, link_id: &str) -> Result<(), LinkError>;

    /// Handle a link query from Zenoh.
    fn handle_link_query(&self, query: LinkQuery) -> LinkQueryResponse {
        let mut links = self.active_links();

        // Apply filters
        if let Some(link_type) = query.link_type_filter {
            links.retain(|l| l.link_type == link_type);
        }
        if let Some(ref pattern) = query.target_filter {
            links.retain(|l| glob_match(pattern, &l.target.target_id));
        }

        LinkQueryResponse::new(query.query_id, links)
    }
}

/// Async version of LinkProvider.
#[async_trait]
pub trait LinkProviderAsync: Send + Sync + 'static {
    /// Get the link capabilities for this modem (async version).
    async fn link_capabilities(&self) -> LinkCapabilities;

    /// Get all currently active links (async version).
    async fn active_links(&self) -> Vec<LinkAvailable>;

    /// Create a new link to a target (async version).
    async fn create_link(&self, target: &TargetInfo) -> Result<LinkAvailable, LinkError>;

    /// Close an existing link (async version).
    async fn close_link(&self, link_id: &str) -> Result<(), LinkError>;

    /// Handle a link query from Zenoh (async version).
    async fn handle_link_query(&self, query: LinkQuery) -> LinkQueryResponse {
        let mut links = self.active_links().await;

        if let Some(link_type) = query.link_type_filter {
            links.retain(|l| l.link_type == link_type);
        }
        if let Some(ref pattern) = query.target_filter {
            links.retain(|l| glob_match(pattern, &l.target.target_id));
        }

        LinkQueryResponse::new(query.query_id, links)
    }
}

/// Trait for emitting link events to Zenoh.
///
/// Implement this to notify Zenoh when links become available or unavailable,
/// or when link properties change.
pub trait LinkEventEmitter: Send + Sync {
    /// Subscribe to link events.
    fn subscribe_link_events(&self) -> broadcast::Receiver<LinkEvent>;
}

/// Link events emitted by the modem driver.
#[derive(Debug, Clone)]
pub enum LinkEvent {
    /// A new link is available
    Available(LinkAvailable),
    /// A link is no longer available
    Unavailable(LinkUnavailable),
    /// Link properties have changed
    Update(LinkUpdate),
}

/// Error type for link operations.
#[derive(Debug, Clone)]
pub struct LinkError {
    pub kind: LinkErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkErrorKind {
    /// Target is not reachable
    TargetUnreachable,
    /// Maximum number of links exceeded
    ResourceExhausted,
    /// Socket creation failed
    SocketError,
    /// Link already exists
    AlreadyExists,
    /// Link not found
    NotFound,
    /// Operation not supported
    NotSupported,
    /// Internal error
    Internal,
}

impl LinkError {
    pub fn new(kind: LinkErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn target_unreachable(message: impl Into<String>) -> Self {
        Self::new(LinkErrorKind::TargetUnreachable, message)
    }

    pub fn resource_exhausted(message: impl Into<String>) -> Self {
        Self::new(LinkErrorKind::ResourceExhausted, message)
    }

    pub fn socket_error(message: impl Into<String>) -> Self {
        Self::new(LinkErrorKind::SocketError, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(LinkErrorKind::NotFound, message)
    }
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for LinkError {}

/// Simple glob matching for target filters.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return text.starts_with(prefix);
    }
    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return text.ends_with(suffix);
    }
    pattern == text
}

// ============================================================================
// COMPOSITE TRAITS
// ============================================================================

/// Full-featured synchronous modem driver.
///
/// Combines all driver capabilities into a single trait.
pub trait FullModemDriver: ModemDriver + AlertEmitter + RequestHandler + Configurable {}

/// Blanket implementation for types that implement all component traits.
impl<T> FullModemDriver for T where T: ModemDriver + AlertEmitter + RequestHandler + Configurable {}

/// Full-featured async modem driver.
#[async_trait]
pub trait FullModemDriverAsync:
    ModemDriverAsync + AlertEmitter + RequestHandlerAsync + Configurable
{
}

/// Full-featured link-providing modem driver (v1.1.0).
///
/// Combines all driver capabilities including link provider.
pub trait FullLinkModemDriver:
    ModemDriver + AlertEmitter + RequestHandler + Configurable + LinkProvider + LinkEventEmitter
{
}

/// Blanket implementation for full link modem drivers.
impl<T> FullLinkModemDriver for T where
    T: ModemDriver + AlertEmitter + RequestHandler + Configurable + LinkProvider + LinkEventEmitter
{
}

/// Full-featured async link-providing modem driver (v1.1.0).
#[async_trait]
pub trait FullLinkModemDriverAsync:
    ModemDriverAsync
    + AlertEmitter
    + RequestHandlerAsync
    + Configurable
    + LinkProviderAsync
    + LinkEventEmitter
{
}

// ============================================================================
// RESPONSE HELPERS
// ============================================================================

impl Response {
    /// Create a success response.
    pub fn success(request_id: u32) -> Self {
        Self {
            request_id,
            status: ResponseStatus::Success,
            data: None,
            error_message: None,
        }
    }

    /// Create a success response with data.
    pub fn success_with_data(request_id: u32, data: ResponseData) -> Self {
        Self {
            request_id,
            status: ResponseStatus::Success,
            data: Some(data),
            error_message: None,
        }
    }

    /// Create a "not supported" response.
    pub fn not_supported(request_id: u32) -> Self {
        Self {
            request_id,
            status: ResponseStatus::NotSupported,
            data: None,
            error_message: Some("Operation not supported".into()),
        }
    }

    /// Create a "busy" response.
    pub fn busy(request_id: u32) -> Self {
        Self {
            request_id,
            status: ResponseStatus::Busy,
            data: None,
            error_message: Some("Modem is busy".into()),
        }
    }

    /// Create a failure response.
    pub fn failed(request_id: u32, message: impl Into<String>) -> Self {
        Self {
            request_id,
            status: ResponseStatus::Failed,
            data: None,
            error_message: Some(message.into()),
        }
    }
}

// ============================================================================
// MODEM SERVER
// ============================================================================

/// Server that exposes a modem driver over Unix socket.
///
/// Handles the protocol details, allowing driver implementers to focus
/// on the modem-specific logic.
pub struct ModemServer<D> {
    driver: Arc<D>,
    socket_path: std::path::PathBuf,
    metrics_interval: Duration,
    heartbeat_interval: Duration,
}

/// Builder for ModemServer.
pub struct ModemServerBuilder<D> {
    driver: D,
    socket_path: Option<std::path::PathBuf>,
    metrics_interval: Duration,
    heartbeat_interval: Duration,
}

impl<D> ModemServerBuilder<D>
where
    D: ModemDriver,
{
    /// Create a new builder with the given driver.
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            socket_path: None,
            metrics_interval: Duration::from_millis(500),
            heartbeat_interval: Duration::from_secs(5),
        }
    }

    /// Set the Unix socket path.
    pub fn socket_path(mut self, path: impl AsRef<Path>) -> Self {
        self.socket_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the metrics push interval.
    pub fn metrics_interval(mut self, interval: Duration) -> Self {
        self.metrics_interval = interval;
        self
    }

    /// Set the heartbeat interval.
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Build the server.
    pub fn build(self) -> ModemServer<D> {
        let socket_path = self
            .socket_path
            .unwrap_or_else(|| std::path::PathBuf::from("/run/zenoh/modem.sock"));

        ModemServer {
            driver: Arc::new(self.driver),
            socket_path,
            metrics_interval: self.metrics_interval,
            heartbeat_interval: self.heartbeat_interval,
        }
    }
}

impl<D: ModemDriver> ModemServer<D> {
    /// Create a new server builder.
    pub fn builder(driver: D) -> ModemServerBuilder<D> {
        ModemServerBuilder::new(driver)
    }

    /// Run the server (blocking).
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::net::UnixListener;

        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        // Bind to socket
        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Modem server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let driver = self.driver.clone();
                    let metrics_interval = self.metrics_interval;
                    let heartbeat_interval = self.heartbeat_interval;

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream,
                            driver,
                            metrics_interval,
                            heartbeat_interval,
                        )
                        .await
                        {
                            tracing::error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(
        stream: tokio::net::UnixStream,
        driver: Arc<D>,
        metrics_interval: Duration,
        heartbeat_interval: Duration,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (mut reader, mut writer) = stream.into_split();

        // Send ModemInfo immediately
        let info = driver.info();
        let msg = Message::ModemInfo(info);
        let encoded = encode_message(&msg)?;
        writer.write_all(&encoded).await?;

        // Spawn metrics sender task
        let driver_clone = driver.clone();
        let writer = Arc::new(tokio::sync::Mutex::new(writer));
        let writer_clone = writer.clone();

        let metrics_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(metrics_interval);
            let mut seq: u32 = 0;

            loop {
                interval.tick().await;
                seq = seq.wrapping_add(1);

                let mut metrics = driver_clone.poll_metrics();
                metrics.seq = seq;

                let msg = Message::MetricsUpdate(metrics);
                if let Ok(encoded) = encode_message(&msg) {
                    let mut w = writer_clone.lock().await;
                    if w.write_all(&encoded).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Handle incoming messages
        let mut len_buf = [0u8; 4];
        loop {
            // Read length prefix
            if reader.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;

            if len > MAX_PAYLOAD_SIZE + 4 {
                tracing::error!("Message too large: {}", len);
                break;
            }

            // Read message
            let mut msg_buf = vec![0u8; len];
            if reader.read_exact(&mut msg_buf).await.is_err() {
                break;
            }

            // Decode and handle
            match decode_message(&msg_buf) {
                Ok(msg) => {
                    if let Some(response) = Self::handle_message(&driver, msg) {
                        if let Ok(encoded) = encode_message(&response) {
                            let mut w = writer.lock().await;
                            let _ = w.write_all(&encoded).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Decode error: {}", e);
                }
            }
        }

        metrics_task.abort();
        Ok(())
    }

    fn handle_message(driver: &D, msg: Message) -> Option<Message> {
        match msg {
            Message::Heartbeat(hb) if hb.echo => {
                Some(Message::Heartbeat(Heartbeat::response(hb.seq)))
            }
            Message::Request(req) => {
                let response = Self::handle_request(driver, req);
                Some(Message::Response(response))
            }
            _ => None,
        }
    }

    fn handle_request(driver: &D, request: Request) -> Response {
        match request.data {
            RequestData::GetMetrics {} => {
                let metrics = driver.poll_metrics();
                Response::success_with_data(request.request_id, ResponseData::Metrics(metrics))
            }
            RequestData::GetState {} => {
                let state = driver.state();
                Response::success_with_data(
                    request.request_id,
                    ResponseData::State(StateChange {
                        timestamp_ns: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos() as u64,
                        previous_state: state,
                        current_state: state,
                        reason: None,
                        error_code: None,
                    }),
                )
            }
            RequestData::GetInfo {} => {
                let info = driver.info();
                Response::success_with_data(request.request_id, ResponseData::Info(info))
            }
            _ => Response::not_supported(request.request_id),
        }
    }
}

// ============================================================================
// HELPER TRAITS FOR SPECIFIC MODEM TYPES
// ============================================================================

/// Helper trait for satellite modems.
///
/// Provides default implementations for satellite-specific functionality.
pub trait SatelliteModem: ModemDriver {
    /// Get satellite-specific metrics.
    fn satellite_metrics(&self) -> SatelliteMetrics;

    /// Check if a handover is imminent.
    fn is_handover_imminent(&self) -> bool {
        self.satellite_metrics().handover_pending
    }

    /// Get time until next handover (if known).
    fn time_to_handover(&self) -> Option<Duration> {
        self.satellite_metrics()
            .time_to_handover_ms
            .map(|ms| Duration::from_millis(ms as u64))
    }

    /// Get current satellite elevation.
    fn elevation(&self) -> f64 {
        self.satellite_metrics().elevation_deg()
    }
}

/// Helper trait for radio modems.
pub trait RadioModem: ModemDriver {
    /// Get radio-specific metrics.
    fn radio_metrics(&self) -> RadioMetrics;

    /// Get current TX power in dBm.
    fn tx_power_dbm(&self) -> f64 {
        self.radio_metrics().tx_power_dbm()
    }

    /// Get current frequency in MHz.
    fn frequency_mhz(&self) -> f64 {
        self.radio_metrics().frequency_mhz()
    }

    /// Check if frequency hopping is active.
    fn is_hopping(&self) -> bool {
        self.radio_metrics().hopping
    }

    /// Check if encryption is active.
    fn is_encrypted(&self) -> bool {
        self.radio_metrics().crypto_active
    }
}

/// Helper trait for acoustic modems.
pub trait AcousticModem: ModemDriver {
    /// Get acoustic-specific metrics.
    fn acoustic_metrics(&self) -> AcousticMetrics;

    /// Get propagation delay in milliseconds.
    fn propagation_delay_ms(&self) -> u32 {
        self.acoustic_metrics().propagation_delay_ms
    }

    /// Get multipath spread in milliseconds.
    fn multipath_spread_ms(&self) -> f64 {
        self.acoustic_metrics().multipath_spread_ms()
    }

    /// Estimate distance based on propagation delay.
    /// Assumes speed of sound ~1500 m/s in water.
    fn estimated_distance_meters(&self) -> f64 {
        let delay_s = self.propagation_delay_ms() as f64 / 1000.0;
        delay_s * 1500.0 // one-way, so no division by 2
    }
}

// ============================================================================
// METRICS BUILDERS
// ============================================================================

/// Builder for constructing ModemMetrics.
#[derive(Default)]
pub struct MetricsBuilder {
    metrics: ModemMetrics,
}

impl MetricsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timestamp(mut self, ns: u64) -> Self {
        self.metrics.timestamp_ns = ns;
        self
    }

    pub fn timestamp_now(mut self) -> Self {
        self.metrics.timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        self
    }

    pub fn seq(mut self, seq: u32) -> Self {
        self.metrics.seq = seq;
        self
    }

    pub fn signal(mut self, snr_db: f64, rssi_dbm: f64, quality_pct: u8) -> Self {
        self.metrics.signal = SignalMetrics {
            snr_cdb: (snr_db * 100.0) as i32,
            rssi_cdbm: (rssi_dbm * 100.0) as i32,
            signal_quality_pct: quality_pct,
            noise_floor_cdbm: None,
        };
        self
    }

    pub fn errors(mut self, ber_exp: i8, per_pct: f64, fer_pct: f64) -> Self {
        self.metrics.errors = ErrorMetrics {
            ber_exponent: ber_exp,
            per_ppm: (per_pct * 10000.0) as u32,
            fer_ppm: (fer_pct * 10000.0) as u32,
            crc_errors: 0,
            retransmits: 0,
        };
        self
    }

    pub fn bandwidth(mut self, tx_bps: u64, rx_bps: u64, available_bps: u64) -> Self {
        self.metrics.bandwidth = BandwidthMetrics {
            tx_rate_bps: tx_bps,
            rx_rate_bps: rx_bps,
            available_bps,
            utilization_pct: 0,
            queue_depth_bytes: 0,
            queue_capacity_bytes: 0,
        };
        self
    }

    pub fn satellite(mut self, metrics: SatelliteMetrics) -> Self {
        self.metrics.satellite = Some(metrics);
        self
    }

    pub fn radio(mut self, metrics: RadioMetrics) -> Self {
        self.metrics.radio = Some(metrics);
        self
    }

    pub fn acoustic(mut self, metrics: AcousticMetrics) -> Self {
        self.metrics.acoustic = Some(metrics);
        self
    }

    pub fn build(self) -> ModemMetrics {
        self.metrics
    }
}

/// Builder for SatelliteMetrics.
pub struct SatelliteMetricsBuilder {
    metrics: SatelliteMetrics,
}

impl SatelliteMetricsBuilder {
    pub fn new() -> Self {
        Self {
            metrics: SatelliteMetrics {
                elevation_cdeg: 0,
                azimuth_cdeg: 0,
                doppler_hz: 0,
                range_km: None,
                satellite_id: None,
                beam_id: None,
                handover_pending: false,
                time_to_handover_ms: None,
            },
        }
    }

    pub fn elevation(mut self, degrees: f64) -> Self {
        self.metrics.elevation_cdeg = (degrees * 100.0) as i16;
        self
    }

    pub fn azimuth(mut self, degrees: f64) -> Self {
        self.metrics.azimuth_cdeg = (degrees * 100.0) as u16;
        self
    }

    pub fn doppler(mut self, hz: i32) -> Self {
        self.metrics.doppler_hz = hz;
        self
    }

    pub fn satellite_id(mut self, id: impl Into<String>) -> Self {
        self.metrics.satellite_id = Some(id.into());
        self
    }

    pub fn handover_pending(mut self, pending: bool, time_ms: Option<u32>) -> Self {
        self.metrics.handover_pending = pending;
        self.metrics.time_to_handover_ms = time_ms;
        self
    }

    pub fn build(self) -> SatelliteMetrics {
        self.metrics
    }
}

/// Builder for RadioMetrics.
pub struct RadioMetricsBuilder {
    metrics: RadioMetrics,
}

impl RadioMetricsBuilder {
    pub fn new() -> Self {
        Self {
            metrics: RadioMetrics {
                tx_power_cdbm: 0,
                frequency_hz: 0,
                bandwidth_hz: 0,
                modulation: String::new(),
                fec_rate: String::new(),
                hopping: false,
                crypto_active: false,
            },
        }
    }

    pub fn tx_power(mut self, dbm: f64) -> Self {
        self.metrics.tx_power_cdbm = (dbm * 100.0) as i16;
        self
    }

    pub fn frequency(mut self, hz: u64) -> Self {
        self.metrics.frequency_hz = hz;
        self
    }

    pub fn frequency_mhz(mut self, mhz: f64) -> Self {
        self.metrics.frequency_hz = (mhz * 1_000_000.0) as u64;
        self
    }

    pub fn bandwidth(mut self, hz: u32) -> Self {
        self.metrics.bandwidth_hz = hz;
        self
    }

    pub fn modulation(mut self, mod_type: impl Into<String>) -> Self {
        self.metrics.modulation = mod_type.into();
        self
    }

    pub fn fec_rate(mut self, rate: impl Into<String>) -> Self {
        self.metrics.fec_rate = rate.into();
        self
    }

    pub fn hopping(mut self, enabled: bool) -> Self {
        self.metrics.hopping = enabled;
        self
    }

    pub fn crypto(mut self, enabled: bool) -> Self {
        self.metrics.crypto_active = enabled;
        self
    }

    pub fn build(self) -> RadioMetrics {
        self.metrics
    }
}

// ============================================================================
// LINK BUILDERS (v1.1.0)
// ============================================================================

/// Builder for LinkCapabilities.
pub struct LinkCapabilitiesBuilder {
    caps: LinkCapabilities,
}

impl LinkCapabilitiesBuilder {
    pub fn new(socket_base_path: impl Into<String>) -> Self {
        Self {
            caps: LinkCapabilities {
                socket_base_path: socket_base_path.into(),
                unicast: UnicastCapabilities {
                    max_mtu: 1500,
                    min_mtu: 64,
                    supports_priorities: false,
                    priority_range: None,
                    supports_express: false,
                    express_priority_threshold: None,
                    supports_reliable: true,
                    supports_best_effort: true,
                    default_reliability: Reliability::Reliable,
                    max_targets: 16,
                    max_pending_bytes: 65536,
                },
                broadcast: None,
                multicast: None,
            },
        }
    }

    pub fn mtu_range(mut self, min: u32, max: u32) -> Self {
        self.caps.unicast.min_mtu = min;
        self.caps.unicast.max_mtu = max;
        self
    }

    pub fn with_priorities(mut self, min: u8, max: u8) -> Self {
        self.caps.unicast.supports_priorities = true;
        self.caps.unicast.priority_range = Some(PriorityRange { min, max });
        self
    }

    pub fn with_express(mut self, priority_threshold: u8) -> Self {
        self.caps.unicast.supports_express = true;
        self.caps.unicast.express_priority_threshold = Some(priority_threshold);
        self
    }

    pub fn max_targets(mut self, max: u32) -> Self {
        self.caps.unicast.max_targets = max;
        self
    }

    pub fn max_pending_bytes(mut self, max: u32) -> Self {
        self.caps.unicast.max_pending_bytes = max;
        self
    }

    pub fn default_reliability(mut self, rel: Reliability) -> Self {
        self.caps.unicast.default_reliability = rel;
        self
    }

    pub fn with_broadcast(
        mut self,
        max_mtu: u32,
        max_recipients: u32,
        supports_reliable: bool,
    ) -> Self {
        self.caps.broadcast = Some(BroadcastCapabilities {
            max_mtu,
            max_recipients,
            supports_reliable,
        });
        self
    }

    pub fn with_multicast(mut self, max_mtu: u32, max_groups: u32, max_members: u32) -> Self {
        self.caps.multicast = Some(MulticastCapabilities {
            max_mtu,
            max_groups,
            max_members_per_group: max_members,
        });
        self
    }

    pub fn build(self) -> LinkCapabilities {
        self.caps
    }
}

/// Builder for LinkProperties.
pub struct LinkPropertiesBuilder {
    props: LinkProperties,
}

impl LinkPropertiesBuilder {
    pub fn new(mtu: u32) -> Self {
        Self {
            props: LinkProperties {
                mtu,
                bandwidth_bps: 0,
                latency_us: 0,
                reliability: Reliability::Reliable,
                priority_range: None,
                express_available: false,
            },
        }
    }

    pub fn bandwidth(mut self, bps: u64) -> Self {
        self.props.bandwidth_bps = bps;
        self
    }

    pub fn latency_us(mut self, us: u32) -> Self {
        self.props.latency_us = us;
        self
    }

    pub fn latency_ms(mut self, ms: f64) -> Self {
        self.props.latency_us = (ms * 1000.0) as u32;
        self
    }

    pub fn reliability(mut self, rel: Reliability) -> Self {
        self.props.reliability = rel;
        self
    }

    pub fn with_priorities(mut self, min: u8, max: u8) -> Self {
        self.props.priority_range = Some(PriorityRange { min, max });
        self
    }

    pub fn with_express(mut self) -> Self {
        self.props.express_available = true;
        self
    }

    pub fn build(self) -> LinkProperties {
        self.props
    }
}

/// Builder for TargetInfo.
pub struct TargetInfoBuilder {
    info: TargetInfo,
}

impl TargetInfoBuilder {
    pub fn new(target_id: impl Into<String>) -> Self {
        Self {
            info: TargetInfo {
                target_id: target_id.into(),
                target_type: TargetType::ZenohNode,
                name: None,
                address: None,
            },
        }
    }

    pub fn target_type(mut self, t: TargetType) -> Self {
        self.info.target_type = t;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.info.name = Some(name.into());
        self
    }

    pub fn address(mut self, addr: impl Into<String>) -> Self {
        self.info.address = Some(addr.into());
        self
    }

    pub fn build(self) -> TargetInfo {
        self.info
    }
}

/// Builder for LinkAvailable messages.
pub struct LinkAvailableBuilder {
    msg: LinkAvailable,
}

impl LinkAvailableBuilder {
    pub fn new(link_id: impl Into<String>, target: TargetInfo) -> Self {
        let link_id = link_id.into();
        Self {
            msg: LinkAvailable {
                timestamp_ns: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                link_id: link_id.clone(),
                link_type: LinkType::Unicast,
                locator: String::new(),
                target,
                properties: LinkProperties::new(1500, 0, 0),
            },
        }
    }

    pub fn locator(mut self, locator: impl Into<String>) -> Self {
        self.msg.locator = locator.into();
        self
    }

    /// Build locator from socket path with optional metadata.
    pub fn unix_socket(mut self, socket_path: impl AsRef<str>) -> Self {
        self.msg.locator = format!("unixpipe/{}", socket_path.as_ref());
        self
    }

    /// Build locator with full metadata.
    pub fn unix_socket_with_metadata(
        mut self,
        socket_path: impl AsRef<str>,
        reliability: Option<Reliability>,
        priority_range: Option<&PriorityRange>,
        express_range: Option<&PriorityRange>,
    ) -> Self {
        let mut locator = format!("unixpipe/{}", socket_path.as_ref());
        let mut params = Vec::new();

        if let Some(rel) = reliability {
            params.push(format!(
                "rel={}",
                match rel {
                    Reliability::Reliable => "reliable",
                    Reliability::BestEffort => "best_effort",
                }
            ));
        }

        if let Some(prio) = priority_range {
            params.push(format!("prio={}-{}", prio.min, prio.max));
        }

        if let Some(expr) = express_range {
            params.push(format!("express={}-{}", expr.min, expr.max));
        }

        if !params.is_empty() {
            locator.push('?');
            locator.push_str(&params.join("&"));
        }

        self.msg.locator = locator;
        self
    }

    pub fn link_type(mut self, t: LinkType) -> Self {
        self.msg.link_type = t;
        self
    }

    pub fn properties(mut self, props: LinkProperties) -> Self {
        self.msg.properties = props;
        self
    }

    pub fn build(self) -> LinkAvailable {
        self.msg
    }
}

// ============================================================================
// ALERT HELPERS
// ============================================================================

/// Builder for Alert.
pub struct AlertBuilder {
    alert: Alert,
}

impl AlertBuilder {
    pub fn new(alert_type: AlertType, severity: AlertSeverity) -> Self {
        Self {
            alert: Alert {
                timestamp_ns: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                alert_id: 0,
                severity,
                alert_type,
                message: String::new(),
                data: AlertData::Custom {
                    vendor_type: String::new(),
                    data: HashMap::new(),
                },
                ttl_ms: None,
            },
        }
    }

    pub fn signal_degrading(current_snr_db: f64, trend_db_per_sec: f64) -> Self {
        let mut builder = Self::new(AlertType::SignalDegrading, AlertSeverity::Warning);
        builder.alert.data = AlertData::SignalDegrading {
            current_snr_cdb: (current_snr_db * 100.0) as i32,
            trend_cdb_per_sec: (trend_db_per_sec * 100.0) as i32,
            estimated_failure_ms: None,
        };
        builder.alert.message = format!(
            "Signal degrading: SNR={:.1}dB, trend={:.2}dB/s",
            current_snr_db, trend_db_per_sec
        );
        builder
    }

    pub fn handover_imminent(duration_ms: u32, outage_ms: u32) -> Self {
        let mut builder = Self::new(AlertType::HandoverImminent, AlertSeverity::Warning);
        builder.alert.data = AlertData::HandoverImminent {
            current_satellite_id: None,
            next_satellite_id: None,
            estimated_duration_ms: duration_ms,
            expected_outage_ms: outage_ms,
        };
        builder.alert.message = format!(
            "Handover imminent: duration={}ms, expected outage={}ms",
            duration_ms, outage_ms
        );
        builder
    }

    pub fn bandwidth_reduction(
        current_bps: u64,
        expected_bps: u64,
        reason: impl Into<String>,
    ) -> Self {
        let reason_str = reason.into();
        let mut builder = Self::new(AlertType::BandwidthReduction, AlertSeverity::Warning);
        builder.alert.data = AlertData::BandwidthReduction {
            current_bps,
            expected_bps,
            reason: reason_str.clone(),
            duration_ms: None,
        };
        builder.alert.message = format!(
            "Bandwidth reduction: {} -> {} bps ({})",
            current_bps, expected_bps, reason_str
        );
        builder
    }

    pub fn alert_id(mut self, id: u32) -> Self {
        self.alert.alert_id = id;
        self
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.alert.message = msg.into();
        self
    }

    pub fn ttl(mut self, ms: u32) -> Self {
        self.alert.ttl_ms = Some(ms);
        self
    }

    pub fn severity(mut self, severity: AlertSeverity) -> Self {
        self.alert.severity = severity;
        self
    }

    pub fn build(self) -> Alert {
        self.alert
    }
}

// ============================================================================
// STATE MACHINE HELPERS
// ============================================================================

/// Helper for managing modem state transitions.
pub struct StateMachine {
    current: ModemState,
    previous: ModemState,
    last_change: Instant,
    on_change: Option<Box<dyn Fn(ModemState, ModemState) + Send + Sync>>,
}

impl StateMachine {
    pub fn new(initial: ModemState) -> Self {
        Self {
            current: initial,
            previous: initial,
            last_change: Instant::now(),
            on_change: None,
        }
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(ModemState, ModemState) + Send + Sync + 'static,
    {
        self.on_change = Some(Box::new(callback));
        self
    }

    pub fn current(&self) -> ModemState {
        self.current
    }

    pub fn previous(&self) -> ModemState {
        self.previous
    }

    pub fn time_in_state(&self) -> Duration {
        self.last_change.elapsed()
    }

    pub fn transition(&mut self, new_state: ModemState) -> Option<StateChange> {
        if self.current == new_state {
            return None;
        }

        self.previous = self.current;
        self.current = new_state;
        self.last_change = Instant::now();

        if let Some(ref callback) = self.on_change {
            callback(self.previous, self.current);
        }

        Some(StateChange {
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            previous_state: self.previous,
            current_state: self.current,
            reason: None,
            error_code: None,
        })
    }

    /// Check if valid transition (optional validation).
    pub fn is_valid_transition(from: ModemState, to: ModemState) -> bool {
        use ModemState::*;

        match (from, to) {
            // Any state can go to Error or Disconnected
            (_, Error) | (_, Disconnected) => true,

            // Error can only go to Initializing or Disconnected
            (Error, Initializing) => true,
            (Error, _) => false,

            // Normal progression
            (Unknown, Initializing) => true,
            (Initializing, Searching) => true,
            (Searching, Synchronizing) => true,
            (Synchronizing, Connected) => true,
            (Connected, Degraded) => true,
            (Degraded, Connected) => true,
            (Connected, HandoverInProgress) => true,
            (HandoverInProgress, Connected) => true,
            (HandoverInProgress, Synchronizing) => true,

            // Standby transitions
            (Connected, Standby) => true,
            (Standby, Initializing) => true,

            // Maintenance
            (_, Maintenance) => true,
            (Maintenance, Initializing) => true,

            _ => false,
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new(ModemState::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_builder() {
        let metrics = MetricsBuilder::new()
            .timestamp_now()
            .seq(42)
            .signal(25.5, -70.0, 85)
            .bandwidth(1_000_000, 500_000, 800_000)
            .build();

        assert_eq!(metrics.seq, 42);
        assert!((metrics.signal.snr_db() - 25.5).abs() < 0.01);
        assert_eq!(metrics.bandwidth.tx_rate_bps, 1_000_000);
    }

    #[test]
    fn test_satellite_metrics_builder() {
        let sat = SatelliteMetricsBuilder::new()
            .elevation(45.5)
            .azimuth(180.0)
            .doppler(-5000)
            .satellite_id("STARLINK-1234")
            .handover_pending(true, Some(30000))
            .build();

        assert!((sat.elevation_deg() - 45.5).abs() < 0.01);
        assert!(sat.handover_pending);
        assert_eq!(sat.time_to_handover_ms, Some(30000));
    }

    #[test]
    fn test_state_machine() {
        let mut sm = StateMachine::new(ModemState::Unknown);

        let change = sm.transition(ModemState::Initializing);
        assert!(change.is_some());
        assert_eq!(sm.current(), ModemState::Initializing);
        assert_eq!(sm.previous(), ModemState::Unknown);

        // Same state - no change
        let change = sm.transition(ModemState::Initializing);
        assert!(change.is_none());
    }

    #[test]
    fn test_alert_builder() {
        let alert = AlertBuilder::signal_degrading(15.0, -0.5)
            .alert_id(1)
            .severity(AlertSeverity::Critical)
            .ttl(60000)
            .build();

        assert_eq!(alert.alert_type, AlertType::SignalDegrading);
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert!(alert.message.contains("15.0"));
    }

    #[test]
    fn test_response_helpers() {
        let success = Response::success(1);
        assert_eq!(success.status, ResponseStatus::Success);

        let failed = Response::failed(2, "Something went wrong");
        assert_eq!(failed.status, ResponseStatus::Failed);
        assert!(failed.error_message.unwrap().contains("wrong"));
    }

    // ========================================================================
    // Link Provider Tests (v1.1.0)
    // ========================================================================

    #[test]
    fn test_link_capabilities_builder() {
        let caps = LinkCapabilitiesBuilder::new("/run/zenoh/modem/radio0")
            .mtu_range(64, 1400)
            .with_priorities(0, 7)
            .with_express(2)
            .max_targets(32)
            .with_broadcast(1200, 16, false)
            .build();

        assert_eq!(caps.socket_base_path, "/run/zenoh/modem/radio0");
        assert_eq!(caps.unicast.min_mtu, 64);
        assert_eq!(caps.unicast.max_mtu, 1400);
        assert!(caps.unicast.supports_priorities);
        assert_eq!(caps.unicast.priority_range.as_ref().unwrap().max, 7);
        assert!(caps.unicast.supports_express);
        assert_eq!(caps.unicast.express_priority_threshold, Some(2));
        assert_eq!(caps.unicast.max_targets, 32);
        assert!(caps.broadcast.is_some());
        assert_eq!(caps.broadcast.as_ref().unwrap().max_recipients, 16);
    }

    #[test]
    fn test_link_properties_builder() {
        let props = LinkPropertiesBuilder::new(1400)
            .bandwidth(9600)
            .latency_ms(250.0)
            .reliability(Reliability::Reliable)
            .with_priorities(0, 7)
            .with_express()
            .build();

        assert_eq!(props.mtu, 1400);
        assert_eq!(props.bandwidth_bps, 9600);
        assert_eq!(props.latency_us, 250_000);
        assert!(props.express_available);
        assert!((props.latency_ms() - 250.0).abs() < 0.01);
    }

    #[test]
    fn test_target_info_builder() {
        let target = TargetInfoBuilder::new("node1")
            .target_type(TargetType::Gateway)
            .name("Gateway Node 1")
            .address("192.168.1.1")
            .build();

        assert_eq!(target.target_id, "node1");
        assert_eq!(target.target_type, TargetType::Gateway);
        assert_eq!(target.name, Some("Gateway Node 1".into()));
        assert_eq!(target.address, Some("192.168.1.1".into()));
    }

    #[test]
    fn test_link_available_builder() {
        let target = TargetInfoBuilder::new("node1").build();

        let link = LinkAvailableBuilder::new("link-001", target)
            .unix_socket("/run/zenoh/modem/radio0/targets/node1.sock")
            .link_type(LinkType::Unicast)
            .properties(
                LinkPropertiesBuilder::new(1400)
                    .bandwidth(9600)
                    .latency_ms(250.0)
                    .build(),
            )
            .build();

        assert_eq!(link.link_id, "link-001");
        assert_eq!(
            link.locator,
            "unixpipe//run/zenoh/modem/radio0/targets/node1.sock"
        );
        assert_eq!(link.properties.mtu, 1400);
    }

    #[test]
    fn test_link_available_with_metadata() {
        let target = TargetInfoBuilder::new("node1").build();
        let prio = PriorityRange { min: 0, max: 7 };
        let express = PriorityRange { min: 0, max: 2 };

        let link = LinkAvailableBuilder::new("link-001", target)
            .unix_socket_with_metadata(
                "/run/zenoh/modem/radio0/targets/node1.sock",
                Some(Reliability::Reliable),
                Some(&prio),
                Some(&express),
            )
            .build();

        assert!(link.locator.contains("rel=reliable"));
        assert!(link.locator.contains("prio=0-7"));
        assert!(link.locator.contains("express=0-2"));
    }

    #[test]
    fn test_link_error() {
        let err = LinkError::target_unreachable("Target went out of range");
        assert_eq!(err.kind, LinkErrorKind::TargetUnreachable);
        assert!(err.message.contains("out of range"));

        let err2 = LinkError::socket_error("Failed to bind");
        assert_eq!(err2.kind, LinkErrorKind::SocketError);
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("node*", "node1"));
        assert!(glob_match("node*", "node123"));
        assert!(!glob_match("node*", "gateway1"));
        assert!(glob_match("*way", "gateway"));
        assert!(!glob_match("*way", "gateway1"));
        assert!(glob_match("node1", "node1"));
        assert!(!glob_match("node1", "node2"));
    }

    #[test]
    fn test_link_event() {
        let target = TargetInfoBuilder::new("node1").build();
        let link = LinkAvailable::new("link-001", "unixpipe//test.sock", target);

        let event = LinkEvent::Available(link.clone());
        match event {
            LinkEvent::Available(l) => assert_eq!(l.link_id, "link-001"),
            _ => panic!("wrong event type"),
        }

        let unavail = LinkUnavailable::new("link-001", LinkUnavailableReason::TargetUnreachable);
        let event2 = LinkEvent::Unavailable(unavail);
        match event2 {
            LinkEvent::Unavailable(u) => {
                assert_eq!(u.reason, LinkUnavailableReason::TargetUnreachable)
            }
            _ => panic!("wrong event type"),
        }
    }
}
