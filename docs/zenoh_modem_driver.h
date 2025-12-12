/**
 * Zenoh Modem Driver C API
 *
 * This header provides a C-compatible API for implementing modem drivers
 * that integrate with Zenoh's OAM link quality measurement system.
 *
 * Version: 1.0.0
 * Protocol Version: 0x01
 *
 * Usage:
 *   1. Include this header in your C/C++ modem driver
 *   2. Implement the required callback functions
 *   3. Call zenoh_modem_driver_init() with your callbacks
 *   4. Call zenoh_modem_driver_run() to start the server loop
 *
 * Example:
 *   static zenoh_modem_info_t my_get_info(void* ctx) {
 *       zenoh_modem_info_t info = {0};
 *       strncpy(info.modem_id, "radio0", ZENOH_MODEM_MAX_ID_LEN);
 *       info.modem_type = ZENOH_MODEM_TYPE_RADIO_TACTICAL;
 *       // ... fill other fields
 *       return info;
 *   }
 *
 *   int main() {
 *       zenoh_modem_callbacks_t callbacks = {
 *           .get_info = my_get_info,
 *           .poll_metrics = my_poll_metrics,
 *           .get_state = my_get_state,
 *       };
 *       zenoh_modem_driver_t* driver = zenoh_modem_driver_init(&callbacks, NULL);
 *       zenoh_modem_driver_run(driver, "/run/zenoh/modem.sock");
 *   }
 */

#ifndef ZENOH_MODEM_DRIVER_H
#define ZENOH_MODEM_DRIVER_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Constants
 * ============================================================================ */

#define ZENOH_MODEM_PROTOCOL_VERSION    0x01
#define ZENOH_MODEM_MAX_PAYLOAD_SIZE    65536
#define ZENOH_MODEM_MAX_ID_LEN          64
#define ZENOH_MODEM_MAX_STRING_LEN      256
#define ZENOH_MODEM_MAX_MESSAGE_LEN     1024

/* ============================================================================
 * Enumerations
 * ============================================================================ */

/**
 * Modem types supported by the protocol
 */
typedef enum {
    ZENOH_MODEM_TYPE_RADIO_HF       = 0x01,
    ZENOH_MODEM_TYPE_RADIO_VHF      = 0x02,
    ZENOH_MODEM_TYPE_RADIO_UHF      = 0x03,
    ZENOH_MODEM_TYPE_RADIO_TACTICAL = 0x04,
    ZENOH_MODEM_TYPE_RADIO_MESH     = 0x05,
    ZENOH_MODEM_TYPE_SATELLITE_GEO  = 0x10,
    ZENOH_MODEM_TYPE_SATELLITE_LEO  = 0x11,
    ZENOH_MODEM_TYPE_SATELLITE_MEO  = 0x12,
    ZENOH_MODEM_TYPE_ACOUSTIC       = 0x20,
    ZENOH_MODEM_TYPE_OPTICAL_FSO    = 0x30,
    ZENOH_MODEM_TYPE_CUSTOM         = 0xFF
} zenoh_modem_type_t;

/**
 * Modem state machine states
 */
typedef enum {
    ZENOH_MODEM_STATE_UNKNOWN           = 0,
    ZENOH_MODEM_STATE_INITIALIZING      = 1,
    ZENOH_MODEM_STATE_SEARCHING         = 2,
    ZENOH_MODEM_STATE_SYNCHRONIZING     = 3,
    ZENOH_MODEM_STATE_CONNECTED         = 4,
    ZENOH_MODEM_STATE_DEGRADED          = 5,
    ZENOH_MODEM_STATE_HANDOVER          = 6,
    ZENOH_MODEM_STATE_STANDBY           = 7,
    ZENOH_MODEM_STATE_DISCONNECTED      = 8,
    ZENOH_MODEM_STATE_ERROR             = 9,
    ZENOH_MODEM_STATE_MAINTENANCE       = 10
} zenoh_modem_state_t;

/**
 * Operating modes for the modem
 */
typedef enum {
    ZENOH_OPERATING_MODE_NORMAL      = 0,
    ZENOH_OPERATING_MODE_RELIABLE    = 1,
    ZENOH_OPERATING_MODE_THROUGHPUT  = 2,
    ZENOH_OPERATING_MODE_LOW_POWER   = 3,
    ZENOH_OPERATING_MODE_LOW_LATENCY = 4,
    ZENOH_OPERATING_MODE_STEALTH     = 5
} zenoh_operating_mode_t;

/**
 * Alert severity levels
 */
typedef enum {
    ZENOH_ALERT_SEVERITY_INFO     = 0,
    ZENOH_ALERT_SEVERITY_WARNING  = 1,
    ZENOH_ALERT_SEVERITY_CRITICAL = 2
} zenoh_alert_severity_t;

/**
 * Alert types
 */
typedef enum {
    ZENOH_ALERT_TYPE_SIGNAL_DEGRADING      = 1,
    ZENOH_ALERT_TYPE_HANDOVER_IMMINENT     = 2,
    ZENOH_ALERT_TYPE_LINK_FAILURE_IMMINENT = 3,
    ZENOH_ALERT_TYPE_BANDWIDTH_REDUCTION   = 4,
    ZENOH_ALERT_TYPE_INTERFERENCE_DETECTED = 5,
    ZENOH_ALERT_TYPE_ENCRYPTION_KEY_EXPIRING = 6,
    ZENOH_ALERT_TYPE_HARDWARE_FAULT        = 7,
    ZENOH_ALERT_TYPE_TEMPERATURE_WARNING   = 8,
    ZENOH_ALERT_TYPE_POWER_ISSUE           = 9,
    ZENOH_ALERT_TYPE_CUSTOM                = 255
} zenoh_alert_type_t;

/**
 * Request types from Zenoh
 */
typedef enum {
    ZENOH_REQUEST_TYPE_GET_METRICS     = 1,
    ZENOH_REQUEST_TYPE_GET_STATE       = 2,
    ZENOH_REQUEST_TYPE_GET_INFO        = 3,
    ZENOH_REQUEST_TYPE_SET_MODE        = 4,
    ZENOH_REQUEST_TYPE_TRAFFIC_HINT    = 5,
    ZENOH_REQUEST_TYPE_DIAGNOSTIC_TEST = 6,
    ZENOH_REQUEST_TYPE_CUSTOM          = 255
} zenoh_request_type_t;

/**
 * Response status codes
 */
typedef enum {
    ZENOH_RESPONSE_STATUS_SUCCESS           = 0,
    ZENOH_RESPONSE_STATUS_INVALID_REQUEST   = 1,
    ZENOH_RESPONSE_STATUS_NOT_SUPPORTED     = 2,
    ZENOH_RESPONSE_STATUS_BUSY              = 3,
    ZENOH_RESPONSE_STATUS_FAILED            = 4,
    ZENOH_RESPONSE_STATUS_TIMEOUT           = 5,
    ZENOH_RESPONSE_STATUS_PERMISSION_DENIED = 6
} zenoh_response_status_t;

/**
 * Bandwidth change reasons
 */
typedef enum {
    ZENOH_BW_REASON_MODULATION_CHANGE   = 1,
    ZENOH_BW_REASON_FEC_CHANGE          = 2,
    ZENOH_BW_REASON_POWER_CHANGE        = 3,
    ZENOH_BW_REASON_CONGESTION          = 4,
    ZENOH_BW_REASON_INTERFERENCE        = 5,
    ZENOH_BW_REASON_SCHEDULED_EVENT     = 6,
    ZENOH_BW_REASON_WEATHER_IMPACT      = 7,
    ZENOH_BW_REASON_HANDOVER_TRANSITION = 8,
    ZENOH_BW_REASON_USER_REQUESTED      = 9,
    ZENOH_BW_REASON_RECOVERY            = 10
} zenoh_bandwidth_change_reason_t;

/**
 * Configuration types
 */
typedef enum {
    ZENOH_CONFIG_TYPE_METRICS_INTERVAL = 1,
    ZENOH_CONFIG_TYPE_ALERT_THRESHOLDS = 2,
    ZENOH_CONFIG_TYPE_POWER_LIMITS     = 3,
    ZENOH_CONFIG_TYPE_BANDWIDTH_LIMITS = 4,
    ZENOH_CONFIG_TYPE_CUSTOM           = 255
} zenoh_config_type_t;

/**
 * Standard error codes
 */
typedef enum {
    ZENOH_ERROR_OK                   = 0x0000,
    ZENOH_ERROR_UNKNOWN              = 0x0001,
    ZENOH_ERROR_INVALID_MESSAGE      = 0x0002,
    ZENOH_ERROR_UNSUPPORTED_VERSION  = 0x0003,
    ZENOH_ERROR_UNSUPPORTED_TYPE     = 0x0004,
    ZENOH_ERROR_INVALID_PARAMETER    = 0x0005,
    ZENOH_ERROR_TIMEOUT              = 0x0006,
    ZENOH_ERROR_BUSY                 = 0x0007,
    ZENOH_ERROR_NOT_READY            = 0x0008,
    ZENOH_ERROR_HARDWARE_FAULT       = 0x0009,
    ZENOH_ERROR_PERMISSION_DENIED    = 0x000A,
    /* Vendor-specific error codes start at 0x1000 */
    ZENOH_ERROR_VENDOR_BASE          = 0x1000
} zenoh_error_code_t;

/* ============================================================================
 * Core Structures
 * ============================================================================ */

/**
 * Modem capabilities description
 */
typedef struct {
    bool     supports_reliable_mode;
    bool     supports_throughput_mode;
    bool     supports_power_control;
    bool     supports_frequency_hop;
    bool     supports_encryption;
    uint64_t max_bandwidth_bps;
    uint64_t min_bandwidth_bps;
    uint32_t metrics_interval_ms;
} zenoh_modem_capabilities_t;

/**
 * Signal quality metrics
 */
typedef struct {
    int32_t snr_cdb;            /**< SNR in centidecibels (dB * 100) */
    int32_t rssi_cdbm;          /**< RSSI in centi-dBm (dBm * 100) */
    uint8_t signal_quality_pct; /**< Signal quality 0-100% */
    int32_t noise_floor_cdbm;   /**< Noise floor in centi-dBm (optional, 0 if unused) */
    bool    has_noise_floor;    /**< True if noise_floor_cdbm is valid */
} zenoh_signal_metrics_t;

/**
 * Error rate metrics
 */
typedef struct {
    int8_t   ber_exponent;  /**< BER as 10^(-exponent), e.g., 6 = 10^-6 */
    uint32_t per_ppm;       /**< Packet Error Rate in parts per million */
    uint32_t fer_ppm;       /**< Frame Error Rate in parts per million */
    uint32_t crc_errors;    /**< CRC error count since last report */
    uint32_t retransmits;   /**< Retransmit count since last report */
} zenoh_error_metrics_t;

/**
 * Bandwidth metrics
 */
typedef struct {
    uint64_t tx_rate_bps;        /**< Current TX rate in bps */
    uint64_t rx_rate_bps;        /**< Current RX rate in bps */
    uint64_t available_bps;      /**< Estimated available bandwidth */
    uint8_t  utilization_pct;    /**< Link utilization 0-100% */
    uint32_t queue_depth_bytes;  /**< TX queue depth in bytes */
    uint32_t queue_capacity_bytes; /**< TX queue capacity in bytes */
} zenoh_bandwidth_metrics_t;

/**
 * Satellite-specific metrics
 */
typedef struct {
    int16_t  elevation_cdeg;      /**< Elevation in centidegrees (deg * 100) */
    uint16_t azimuth_cdeg;        /**< Azimuth in centidegrees */
    int32_t  doppler_hz;          /**< Doppler shift in Hz */
    uint32_t range_km;            /**< Range to satellite in km (optional) */
    bool     has_range;           /**< True if range_km is valid */
    char     satellite_id[ZENOH_MODEM_MAX_ID_LEN]; /**< Satellite identifier (optional) */
    char     beam_id[ZENOH_MODEM_MAX_ID_LEN];      /**< Beam identifier (optional) */
    bool     handover_pending;    /**< Handover imminent */
    uint32_t time_to_handover_ms; /**< Estimated ms until handover (optional) */
    bool     has_time_to_handover; /**< True if time_to_handover_ms is valid */
} zenoh_satellite_metrics_t;

/**
 * Radio-specific metrics
 */
typedef struct {
    int16_t  tx_power_cdbm;   /**< TX power in centi-dBm */
    uint64_t frequency_hz;    /**< Current frequency in Hz */
    uint32_t bandwidth_hz;    /**< Channel bandwidth in Hz */
    char     modulation[ZENOH_MODEM_MAX_ID_LEN]; /**< Current modulation scheme */
    char     fec_rate[ZENOH_MODEM_MAX_ID_LEN];   /**< FEC rate */
    bool     hopping;         /**< Frequency hopping active */
    bool     crypto_active;   /**< Encryption active */
} zenoh_radio_metrics_t;

/**
 * Acoustic modem-specific metrics
 */
typedef struct {
    uint32_t multipath_spread_us;  /**< Multipath spread in microseconds */
    int16_t  doppler_spread_chz;   /**< Doppler spread in centi-Hz */
    uint32_t propagation_delay_ms; /**< One-way propagation delay in ms */
    int16_t  ambient_noise_cdb;    /**< Ambient noise level in centi-dB */
    int16_t  water_temp_cdc;       /**< Water temperature in centi-Celsius (optional) */
    bool     has_water_temp;       /**< True if water_temp_cdc is valid */
} zenoh_acoustic_metrics_t;

/**
 * Complete modem metrics structure
 */
typedef struct {
    uint64_t                   timestamp_ns;
    uint32_t                   seq;
    zenoh_signal_metrics_t     signal;
    zenoh_error_metrics_t      errors;
    zenoh_bandwidth_metrics_t  bandwidth;

    /* Optional link-type specific metrics */
    zenoh_satellite_metrics_t  satellite;
    bool                       has_satellite;
    zenoh_radio_metrics_t      radio;
    bool                       has_radio;
    zenoh_acoustic_metrics_t   acoustic;
    bool                       has_acoustic;
} zenoh_modem_metrics_t;

/**
 * Modem information structure
 * Note: link_capabilities is defined in the Link Management section below
 */
typedef struct zenoh_modem_info zenoh_modem_info_t;  /* Forward declaration */

/**
 * State change notification
 */
typedef struct {
    uint64_t            timestamp_ns;
    zenoh_modem_state_t previous_state;
    zenoh_modem_state_t current_state;
    char                reason[ZENOH_MODEM_MAX_STRING_LEN]; /**< Optional */
    bool                has_reason;
    uint32_t            error_code;  /**< Optional */
    bool                has_error_code;
} zenoh_state_change_t;

/**
 * Alert data for signal degrading
 */
typedef struct {
    int32_t  current_snr_cdb;
    int32_t  trend_cdb_per_sec;
    uint32_t estimated_failure_ms;
    bool     has_estimated_failure;
} zenoh_alert_signal_degrading_t;

/**
 * Alert data for handover imminent
 */
typedef struct {
    char     current_satellite_id[ZENOH_MODEM_MAX_ID_LEN];
    bool     has_current_satellite_id;
    char     next_satellite_id[ZENOH_MODEM_MAX_ID_LEN];
    bool     has_next_satellite_id;
    uint32_t estimated_duration_ms;
    uint32_t expected_outage_ms;
} zenoh_alert_handover_t;

/**
 * Alert data for bandwidth reduction
 */
typedef struct {
    uint64_t current_bps;
    uint64_t expected_bps;
    char     reason[ZENOH_MODEM_MAX_STRING_LEN];
    uint32_t duration_ms;
    bool     has_duration;
} zenoh_alert_bandwidth_t;

/**
 * Alert union data
 */
typedef union {
    zenoh_alert_signal_degrading_t signal_degrading;
    zenoh_alert_handover_t         handover;
    zenoh_alert_bandwidth_t        bandwidth;
    uint8_t                        custom[ZENOH_MODEM_MAX_PAYLOAD_SIZE];
} zenoh_alert_data_t;

/**
 * Alert structure
 */
typedef struct {
    uint64_t              timestamp_ns;
    uint32_t              alert_id;
    zenoh_alert_severity_t severity;
    zenoh_alert_type_t    alert_type;
    char                  message[ZENOH_MODEM_MAX_MESSAGE_LEN];
    zenoh_alert_data_t    data;
    uint32_t              ttl_ms;
    bool                  has_ttl;
} zenoh_alert_t;

/**
 * Bandwidth update notification
 */
typedef struct {
    uint64_t                        timestamp_ns;
    uint64_t                        available_tx_bps;
    uint64_t                        available_rx_bps;
    uint64_t                        max_tx_bps;
    uint64_t                        max_rx_bps;
    zenoh_bandwidth_change_reason_t reason;
    uint32_t                        duration_estimate_ms;
    bool                            has_duration_estimate;
} zenoh_bandwidth_update_t;

/**
 * Traffic hint request from Zenoh
 */
typedef struct {
    uint64_t expected_tx_bps;
    uint64_t expected_rx_bps;
    uint8_t  priority;          /**< Zenoh priority (0-7) */
    uint32_t duration_ms;
    bool     reliable;
    uint32_t latency_budget_ms;
    bool     has_latency_budget;
} zenoh_traffic_hint_t;

/**
 * Set mode request from Zenoh
 */
typedef struct {
    zenoh_operating_mode_t mode;
    uint32_t               duration_ms;
    bool                   has_duration;
} zenoh_set_mode_request_t;

/**
 * Request from Zenoh
 */
typedef struct {
    uint32_t              request_id;
    zenoh_request_type_t  request_type;
    union {
        zenoh_set_mode_request_t set_mode;
        zenoh_traffic_hint_t     traffic_hint;
        char                     custom[ZENOH_MODEM_MAX_PAYLOAD_SIZE];
    } data;
} zenoh_request_t;

/**
 * Response data for mode set
 */
typedef struct {
    zenoh_operating_mode_t applied_mode;
    uint64_t               effective_until_ns;
    bool                   has_effective_until;
} zenoh_response_mode_set_t;

/**
 * Response data for traffic hint ack
 */
typedef struct {
    bool     accepted;
    uint64_t adjusted_tx_bps;
    bool     has_adjusted_tx;
    uint64_t adjusted_rx_bps;
    bool     has_adjusted_rx;
} zenoh_response_traffic_hint_t;

/**
 * Response to Zenoh
 */
typedef struct {
    uint32_t               request_id;
    zenoh_response_status_t status;
    char                   error_message[ZENOH_MODEM_MAX_MESSAGE_LEN];
    bool                   has_error_message;
    union {
        zenoh_modem_metrics_t       metrics;
        zenoh_state_change_t        state;
        zenoh_modem_info_t          info;
        zenoh_response_mode_set_t   mode_set;
        zenoh_response_traffic_hint_t traffic_hint;
        uint8_t                     custom[ZENOH_MODEM_MAX_PAYLOAD_SIZE];
    } data;
    bool has_data;
} zenoh_response_t;

/**
 * Configuration for metrics interval
 */
typedef struct {
    uint32_t interval_ms;
} zenoh_config_metrics_interval_t;

/**
 * Configuration for alert thresholds
 */
typedef struct {
    int32_t snr_warning_cdb;
    bool    has_snr_warning;
    int32_t snr_critical_cdb;
    bool    has_snr_critical;
    uint8_t loss_warning_pct;
    bool    has_loss_warning;
    uint8_t loss_critical_pct;
    bool    has_loss_critical;
} zenoh_config_alert_thresholds_t;

/**
 * Configuration from Zenoh
 */
typedef struct {
    uint32_t            config_id;
    zenoh_config_type_t config_type;
    union {
        zenoh_config_metrics_interval_t  metrics_interval;
        zenoh_config_alert_thresholds_t  alert_thresholds;
        uint8_t                          custom[ZENOH_MODEM_MAX_PAYLOAD_SIZE];
    } data;
} zenoh_configure_t;

/**
 * Configuration acknowledgment
 */
typedef struct {
    uint32_t               config_id;
    zenoh_response_status_t status;
    char                   error_message[ZENOH_MODEM_MAX_MESSAGE_LEN];
    bool                   has_error_message;
} zenoh_configure_ack_t;

/* ============================================================================
 * Link Management Structures (Link Provider)
 * ============================================================================ */

/**
 * Link types
 */
typedef enum {
    ZENOH_LINK_TYPE_UNICAST   = 0,
    ZENOH_LINK_TYPE_BROADCAST = 1,
    ZENOH_LINK_TYPE_MULTICAST = 2
} zenoh_link_type_t;

/**
 * Target types for addressing
 */
typedef enum {
    ZENOH_TARGET_TYPE_ZENOH_ID           = 0,
    ZENOH_TARGET_TYPE_MAC_ADDRESS        = 1,
    ZENOH_TARGET_TYPE_IP_ADDRESS         = 2,
    ZENOH_TARGET_TYPE_RADIO_CALLSIGN     = 3,
    ZENOH_TARGET_TYPE_SATELLITE_TERMINAL = 4,
    ZENOH_TARGET_TYPE_ACOUSTIC_ADDRESS   = 5,
    ZENOH_TARGET_TYPE_BROADCAST          = 6,
    ZENOH_TARGET_TYPE_MULTICAST          = 7,
    ZENOH_TARGET_TYPE_CUSTOM             = 255
} zenoh_target_type_t;

/**
 * Link unavailable reasons
 */
typedef enum {
    ZENOH_LINK_UNAVAIL_TARGET_OFFLINE  = 0,
    ZENOH_LINK_UNAVAIL_LINK_DOWN       = 1,
    ZENOH_LINK_UNAVAIL_TIMEOUT         = 2,
    ZENOH_LINK_UNAVAIL_OUT_OF_RANGE    = 3,
    ZENOH_LINK_UNAVAIL_HANDOVER        = 4,
    ZENOH_LINK_UNAVAIL_INTERFERENCE    = 5,
    ZENOH_LINK_UNAVAIL_CONGESTION      = 6,
    ZENOH_LINK_UNAVAIL_AUTH_FAILURE    = 7,
    ZENOH_LINK_UNAVAIL_MAINTENANCE     = 8,
    ZENOH_LINK_UNAVAIL_DRIVER_SHUTDOWN = 9,
    ZENOH_LINK_UNAVAIL_UNKNOWN         = 255
} zenoh_link_unavailable_reason_t;

/**
 * Link state
 */
typedef enum {
    ZENOH_LINK_STATE_ACTIVE     = 0,
    ZENOH_LINK_STATE_DEGRADED   = 1,
    ZENOH_LINK_STATE_CONNECTING = 2,
    ZENOH_LINK_STATE_SUSPENDED  = 3
} zenoh_link_state_t;

/**
 * Reliability mode
 */
typedef enum {
    ZENOH_RELIABILITY_RELIABLE    = 0,
    ZENOH_RELIABILITY_BEST_EFFORT = 1
} zenoh_reliability_t;

#define ZENOH_MODEM_MAX_LINK_ID_LEN   64
#define ZENOH_MODEM_MAX_LOCATOR_LEN   256
#define ZENOH_MODEM_MAX_TARGET_ADDR   64

/**
 * Priority range
 */
typedef struct {
    uint8_t start;              /**< Lowest supported priority (0-7) */
    uint8_t end;                /**< Highest supported priority (0-7) */
} zenoh_priority_range_t;

/**
 * Unicast link capabilities
 */
typedef struct {
    uint32_t max_mtu;
    uint32_t min_mtu;
    bool     supports_priorities;
    zenoh_priority_range_t priority_range;
    bool     has_priority_range;
    bool     supports_express;
    uint8_t  express_priority_threshold;  /**< Priorities 0..N use express */
    bool     has_express_threshold;
    bool     supports_reliable;
    bool     supports_best_effort;
    zenoh_reliability_t default_reliability;
    uint32_t max_targets;
    uint32_t max_pending_bytes;
} zenoh_unicast_capabilities_t;

/**
 * Broadcast link capabilities
 */
typedef struct {
    bool     available;
    uint32_t mtu;
    zenoh_reliability_t reliability;
    zenoh_priority_range_t priority_range;
    bool     has_priority_range;
} zenoh_broadcast_capabilities_t;

/**
 * Multicast link capabilities
 */
typedef struct {
    bool     available;
    uint32_t max_groups;
    uint32_t mtu;
    zenoh_reliability_t reliability;
    zenoh_priority_range_t priority_range;
    bool     has_priority_range;
} zenoh_multicast_capabilities_t;

/**
 * Link capabilities (in ModemInfo)
 */
typedef struct {
    char socket_base_path[ZENOH_MODEM_MAX_STRING_LEN];
    zenoh_unicast_capabilities_t   unicast;
    zenoh_broadcast_capabilities_t broadcast;
    bool has_broadcast;
    zenoh_multicast_capabilities_t multicast;
    bool has_multicast;
} zenoh_link_capabilities_t;

/**
 * Target information
 */
typedef struct {
    char                  target_id[ZENOH_MODEM_MAX_LINK_ID_LEN];
    zenoh_target_type_t   target_type;
    uint8_t               address[ZENOH_MODEM_MAX_TARGET_ADDR];
    uint16_t              address_len;
    char                  display_name[ZENOH_MODEM_MAX_STRING_LEN];
    bool                  has_display_name;
    uint8_t               zenoh_id[16];  /**< Remote Zenoh ID if known */
    bool                  has_zenoh_id;
} zenoh_target_info_t;

/**
 * Link properties
 */
typedef struct {
    uint32_t mtu;
    uint64_t bandwidth_bps;
    uint32_t latency_us;
    uint32_t jitter_us;
    bool     has_jitter;
    uint32_t loss_ppm;
    bool     has_loss;
    zenoh_reliability_t reliability;
    zenoh_priority_range_t priority_range;
    bool     has_priority_range;
    bool     supports_express;
} zenoh_link_properties_t;

/**
 * Link available notification
 */
typedef struct {
    uint64_t              timestamp_ns;
    char                  link_id[ZENOH_MODEM_MAX_LINK_ID_LEN];
    zenoh_link_type_t     link_type;
    char                  locator[ZENOH_MODEM_MAX_LOCATOR_LEN];
    zenoh_target_info_t   target;
    zenoh_link_properties_t properties;
} zenoh_link_available_t;

/**
 * Link unavailable details
 */
typedef struct {
    char     error_message[ZENOH_MODEM_MAX_MESSAGE_LEN];
    bool     has_error_message;
    uint32_t estimated_recovery_ms;
    bool     has_estimated_recovery;
    uint64_t last_seen_ns;
    bool     has_last_seen;
    bool     alternate_available;
    bool     has_alternate_available;
} zenoh_link_unavailable_details_t;

/**
 * Link unavailable notification
 */
typedef struct {
    uint64_t                        timestamp_ns;
    char                            link_id[ZENOH_MODEM_MAX_LINK_ID_LEN];
    char                            locator[ZENOH_MODEM_MAX_LOCATOR_LEN];
    zenoh_link_unavailable_reason_t reason;
    zenoh_link_unavailable_details_t details;
    bool                            has_details;
} zenoh_link_unavailable_t;

/**
 * Link properties update (only changed fields)
 */
typedef struct {
    uint32_t mtu;
    bool     has_mtu;
    uint64_t bandwidth_bps;
    bool     has_bandwidth;
    uint32_t latency_us;
    bool     has_latency;
    uint32_t jitter_us;
    bool     has_jitter;
    uint32_t loss_ppm;
    bool     has_loss;
    zenoh_reliability_t reliability;
    bool     has_reliability;
    zenoh_priority_range_t priority_range;
    bool     has_priority_range;
    bool     supports_express;
    bool     has_supports_express;
} zenoh_link_properties_update_t;

/**
 * Link update notification
 */
typedef struct {
    uint64_t timestamp_ns;
    char     link_id[ZENOH_MODEM_MAX_LINK_ID_LEN];
    zenoh_link_properties_update_t updated_properties;
} zenoh_link_update_t;

/**
 * Link info (for query response)
 */
typedef struct {
    char                    link_id[ZENOH_MODEM_MAX_LINK_ID_LEN];
    zenoh_link_type_t       link_type;
    char                    locator[ZENOH_MODEM_MAX_LOCATOR_LEN];
    zenoh_target_info_t     target;
    zenoh_link_properties_t properties;
    zenoh_link_state_t      state;
} zenoh_link_info_t;

/**
 * Modem information structure (full definition)
 * Includes link_capabilities for Link Provider functionality
 */
struct zenoh_modem_info {
    char                       modem_id[ZENOH_MODEM_MAX_ID_LEN];
    zenoh_modem_type_t         modem_type;
    char                       model[ZENOH_MODEM_MAX_STRING_LEN];
    char                       firmware_version[ZENOH_MODEM_MAX_ID_LEN];
    char                       serial_number[ZENOH_MODEM_MAX_ID_LEN]; /**< Optional */
    bool                       has_serial_number;
    zenoh_modem_capabilities_t capabilities;
    zenoh_modem_state_t        initial_state;
    zenoh_modem_metrics_t      initial_metrics;
    zenoh_link_capabilities_t  link_capabilities;  /**< Link provider capabilities */
};

/* ============================================================================
 * Callback Function Types
 * ============================================================================ */

/**
 * Callback to get modem information.
 * Called once when the connection is established.
 *
 * @param ctx User context pointer passed to zenoh_modem_driver_init()
 * @return Modem information structure
 */
typedef zenoh_modem_info_t (*zenoh_get_info_fn)(void* ctx);

/**
 * Callback to poll current metrics.
 * Called periodically at the configured interval.
 *
 * @param ctx User context pointer
 * @return Current modem metrics
 */
typedef zenoh_modem_metrics_t (*zenoh_poll_metrics_fn)(void* ctx);

/**
 * Callback to get current modem state.
 *
 * @param ctx User context pointer
 * @return Current modem state
 */
typedef zenoh_modem_state_t (*zenoh_get_state_fn)(void* ctx);

/**
 * Callback to get current available bandwidth.
 * Optional - if NULL, extracted from poll_metrics.
 *
 * @param ctx User context pointer
 * @return Current bandwidth metrics
 */
typedef zenoh_bandwidth_metrics_t (*zenoh_get_bandwidth_fn)(void* ctx);

/**
 * Callback to handle requests from Zenoh.
 * Optional - if NULL, requests return NOT_SUPPORTED.
 *
 * @param ctx User context pointer
 * @param request The request from Zenoh
 * @return Response to send back
 */
typedef zenoh_response_t (*zenoh_handle_request_fn)(void* ctx, const zenoh_request_t* request);

/**
 * Callback to handle configuration updates from Zenoh.
 * Optional - if NULL, configuration returns NOT_SUPPORTED.
 *
 * @param ctx User context pointer
 * @param config The configuration from Zenoh
 * @return Configuration acknowledgment
 */
typedef zenoh_configure_ack_t (*zenoh_handle_configure_fn)(void* ctx, const zenoh_configure_t* config);

/**
 * Collection of driver callbacks
 */
typedef struct {
    /* Required callbacks */
    zenoh_get_info_fn      get_info;
    zenoh_poll_metrics_fn  poll_metrics;
    zenoh_get_state_fn     get_state;

    /* Optional callbacks */
    zenoh_get_bandwidth_fn   get_bandwidth;      /**< Optional */
    zenoh_handle_request_fn  handle_request;     /**< Optional */
    zenoh_handle_configure_fn handle_configure;  /**< Optional */
} zenoh_modem_callbacks_t;

/* ============================================================================
 * Driver Handle
 * ============================================================================ */

/**
 * Opaque driver handle
 */
typedef struct zenoh_modem_driver zenoh_modem_driver_t;

/* ============================================================================
 * Driver Lifecycle Functions
 * ============================================================================ */

/**
 * Initialize a modem driver.
 *
 * @param callbacks Pointer to callback structure (must remain valid)
 * @param ctx User context pointer passed to all callbacks
 * @return Driver handle, or NULL on error
 */
zenoh_modem_driver_t* zenoh_modem_driver_init(
    const zenoh_modem_callbacks_t* callbacks,
    void* ctx
);

/**
 * Set the metrics push interval.
 * Default is 500ms.
 *
 * @param driver Driver handle
 * @param interval_ms Interval in milliseconds
 */
void zenoh_modem_driver_set_metrics_interval(
    zenoh_modem_driver_t* driver,
    uint32_t interval_ms
);

/**
 * Set the heartbeat interval.
 * Default is 5000ms.
 *
 * @param driver Driver handle
 * @param interval_ms Interval in milliseconds
 */
void zenoh_modem_driver_set_heartbeat_interval(
    zenoh_modem_driver_t* driver,
    uint32_t interval_ms
);

/**
 * Run the driver server (blocking).
 * This function blocks and runs the Unix socket server.
 *
 * @param driver Driver handle
 * @param socket_path Path to Unix socket
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_driver_run(
    zenoh_modem_driver_t* driver,
    const char* socket_path
);

/**
 * Run the driver server in the background.
 * Returns immediately after starting the server thread.
 *
 * @param driver Driver handle
 * @param socket_path Path to Unix socket
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_driver_run_background(
    zenoh_modem_driver_t* driver,
    const char* socket_path
);

/**
 * Stop the driver server.
 *
 * @param driver Driver handle
 */
void zenoh_modem_driver_stop(zenoh_modem_driver_t* driver);

/**
 * Destroy the driver and free resources.
 *
 * @param driver Driver handle
 */
void zenoh_modem_driver_destroy(zenoh_modem_driver_t* driver);

/* ============================================================================
 * Event Emission Functions
 * ============================================================================ */

/**
 * Emit a state change notification.
 * Call this when the modem state changes.
 *
 * @param driver Driver handle
 * @param state_change State change details
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_state_change(
    zenoh_modem_driver_t* driver,
    const zenoh_state_change_t* state_change
);

/**
 * Emit an alert.
 * Call this when an alert condition is detected.
 *
 * @param driver Driver handle
 * @param alert Alert details
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_alert(
    zenoh_modem_driver_t* driver,
    const zenoh_alert_t* alert
);

/**
 * Emit a bandwidth update notification.
 * Call this when available bandwidth changes significantly.
 *
 * @param driver Driver handle
 * @param update Bandwidth update details
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_bandwidth_update(
    zenoh_modem_driver_t* driver,
    const zenoh_bandwidth_update_t* update
);

/* ============================================================================
 * Link Management Functions (Link Provider)
 * ============================================================================ */

/**
 * Emit a link available notification.
 * Call this when a new target becomes reachable and its socket is ready.
 * The driver MUST create the Unix socket before calling this function.
 *
 * @param driver Driver handle
 * @param link_available Link details including locator
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_link_available(
    zenoh_modem_driver_t* driver,
    const zenoh_link_available_t* link_available
);

/**
 * Emit a link unavailable notification.
 * Call this when a target becomes unreachable.
 * The driver SHOULD close the socket after calling this function.
 *
 * @param driver Driver handle
 * @param link_unavailable Link unavailability details
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_link_unavailable(
    zenoh_modem_driver_t* driver,
    const zenoh_link_unavailable_t* link_unavailable
);

/**
 * Emit a link update notification.
 * Call this when link properties change significantly.
 *
 * @param driver Driver handle
 * @param link_update Updated link properties
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_emit_link_update(
    zenoh_modem_driver_t* driver,
    const zenoh_link_update_t* link_update
);

/**
 * Create a Unix socket for a target and prepare link available notification.
 * Helper function that creates the socket and builds the locator string.
 *
 * @param driver Driver handle
 * @param target_id Target identifier
 * @param target Target information
 * @param properties Initial link properties
 * @param link_type Type of link (unicast, broadcast, multicast)
 * @param out_link_available Output: filled link available structure
 * @return Socket fd on success, negative error code on failure
 */
int zenoh_modem_create_target_socket(
    zenoh_modem_driver_t* driver,
    const char* target_id,
    const zenoh_target_info_t* target,
    const zenoh_link_properties_t* properties,
    zenoh_link_type_t link_type,
    zenoh_link_available_t* out_link_available
);

/**
 * Close a target socket and emit link unavailable.
 *
 * @param driver Driver handle
 * @param link_id Link identifier
 * @param reason Reason for unavailability
 * @return 0 on success, negative error code on failure
 */
int zenoh_modem_close_target_socket(
    zenoh_modem_driver_t* driver,
    const char* link_id,
    zenoh_link_unavailable_reason_t reason
);

/* ============================================================================
 * Helper Functions
 * ============================================================================ */

/**
 * Get current timestamp in nanoseconds since UNIX epoch.
 *
 * @return Timestamp in nanoseconds
 */
uint64_t zenoh_modem_timestamp_ns(void);

/**
 * Convert dB to centidecibels.
 *
 * @param db Value in dB
 * @return Value in centidecibels
 */
static inline int32_t zenoh_db_to_cdb(double db) {
    return (int32_t)(db * 100.0);
}

/**
 * Convert centidecibels to dB.
 *
 * @param cdb Value in centidecibels
 * @return Value in dB
 */
static inline double zenoh_cdb_to_db(int32_t cdb) {
    return (double)cdb / 100.0;
}

/**
 * Convert MHz to Hz.
 *
 * @param mhz Value in MHz
 * @return Value in Hz
 */
static inline uint64_t zenoh_mhz_to_hz(double mhz) {
    return (uint64_t)(mhz * 1000000.0);
}

/**
 * Convert Hz to MHz.
 *
 * @param hz Value in Hz
 * @return Value in MHz
 */
static inline double zenoh_hz_to_mhz(uint64_t hz) {
    return (double)hz / 1000000.0;
}

/**
 * Convert degrees to centidegrees.
 *
 * @param deg Value in degrees
 * @return Value in centidegrees
 */
static inline int16_t zenoh_deg_to_cdeg(double deg) {
    return (int16_t)(deg * 100.0);
}

/**
 * Convert centidegrees to degrees.
 *
 * @param cdeg Value in centidegrees
 * @return Value in degrees
 */
static inline double zenoh_cdeg_to_deg(int16_t cdeg) {
    return (double)cdeg / 100.0;
}

/**
 * Convert percentage to parts per million.
 *
 * @param pct Percentage (0-100)
 * @return Parts per million
 */
static inline uint32_t zenoh_pct_to_ppm(double pct) {
    return (uint32_t)(pct * 10000.0);
}

/**
 * Convert parts per million to percentage.
 *
 * @param ppm Parts per million
 * @return Percentage (0-100)
 */
static inline double zenoh_ppm_to_pct(uint32_t ppm) {
    return (double)ppm / 10000.0;
}

/* ============================================================================
 * Structure Initialization Helpers
 * ============================================================================ */

/**
 * Initialize a modem info structure with zeros.
 *
 * @return Zero-initialized structure
 */
static inline zenoh_modem_info_t zenoh_modem_info_init(void) {
    zenoh_modem_info_t info = {0};
    return info;
}

/**
 * Initialize a metrics structure with zeros.
 *
 * @return Zero-initialized structure
 */
static inline zenoh_modem_metrics_t zenoh_modem_metrics_init(void) {
    zenoh_modem_metrics_t metrics = {0};
    return metrics;
}

/**
 * Initialize a response structure with success status.
 *
 * @param request_id The request ID to respond to
 * @return Initialized response structure
 */
static inline zenoh_response_t zenoh_response_success(uint32_t request_id) {
    zenoh_response_t resp = {0};
    resp.request_id = request_id;
    resp.status = ZENOH_RESPONSE_STATUS_SUCCESS;
    return resp;
}

/**
 * Initialize a response structure with not supported status.
 *
 * @param request_id The request ID to respond to
 * @return Initialized response structure
 */
static inline zenoh_response_t zenoh_response_not_supported(uint32_t request_id) {
    zenoh_response_t resp = {0};
    resp.request_id = request_id;
    resp.status = ZENOH_RESPONSE_STATUS_NOT_SUPPORTED;
    return resp;
}

/**
 * Initialize an alert structure.
 *
 * @param alert_type Type of alert
 * @param severity Severity level
 * @return Initialized alert structure
 */
static inline zenoh_alert_t zenoh_alert_init(
    zenoh_alert_type_t alert_type,
    zenoh_alert_severity_t severity
) {
    zenoh_alert_t alert = {0};
    alert.timestamp_ns = zenoh_modem_timestamp_ns();
    alert.alert_type = alert_type;
    alert.severity = severity;
    return alert;
}

/**
 * Initialize a state change structure.
 *
 * @param previous Previous state
 * @param current New state
 * @return Initialized state change structure
 */
static inline zenoh_state_change_t zenoh_state_change_init(
    zenoh_modem_state_t previous,
    zenoh_modem_state_t current
) {
    zenoh_state_change_t change = {0};
    change.timestamp_ns = zenoh_modem_timestamp_ns();
    change.previous_state = previous;
    change.current_state = current;
    return change;
}

/**
 * Initialize a link available structure for a unicast target.
 *
 * @param link_id Unique link identifier
 * @param target_id Target identifier  
 * @param locator Zenoh locator string
 * @return Initialized structure
 */
static inline zenoh_link_available_t zenoh_link_available_init(
    const char* link_id,
    const char* locator
) {
    zenoh_link_available_t link = {0};
    link.timestamp_ns = zenoh_modem_timestamp_ns();
    link.link_type = ZENOH_LINK_TYPE_UNICAST;
    if (link_id) {
        size_t len = 0;
        while (link_id[len] && len < ZENOH_MODEM_MAX_LINK_ID_LEN - 1) {
            link.link_id[len] = link_id[len];
            len++;
        }
    }
    if (locator) {
        size_t len = 0;
        while (locator[len] && len < ZENOH_MODEM_MAX_LOCATOR_LEN - 1) {
            link.locator[len] = locator[len];
            len++;
        }
    }
    return link;
}

/**
 * Initialize a link unavailable structure.
 *
 * @param link_id Link identifier
 * @param reason Unavailability reason
 * @return Initialized structure
 */
static inline zenoh_link_unavailable_t zenoh_link_unavailable_init(
    const char* link_id,
    zenoh_link_unavailable_reason_t reason
) {
    zenoh_link_unavailable_t unavail = {0};
    unavail.timestamp_ns = zenoh_modem_timestamp_ns();
    unavail.reason = reason;
    if (link_id) {
        size_t len = 0;
        while (link_id[len] && len < ZENOH_MODEM_MAX_LINK_ID_LEN - 1) {
            unavail.link_id[len] = link_id[len];
            len++;
        }
    }
    return unavail;
}

/**
 * Initialize link properties with defaults.
 *
 * @param mtu Maximum transmission unit
 * @param bandwidth_bps Available bandwidth
 * @param latency_us One-way latency
 * @return Initialized structure
 */
static inline zenoh_link_properties_t zenoh_link_properties_init(
    uint32_t mtu,
    uint64_t bandwidth_bps,
    uint32_t latency_us
) {
    zenoh_link_properties_t props = {0};
    props.mtu = mtu;
    props.bandwidth_bps = bandwidth_bps;
    props.latency_us = latency_us;
    props.reliability = ZENOH_RELIABILITY_RELIABLE;
    props.priority_range.start = 0;
    props.priority_range.end = 7;
    props.has_priority_range = true;
    props.supports_express = false;
    return props;
}

/**
 * Build a locator string for a target socket.
 *
 * @param buffer Output buffer
 * @param buffer_size Size of buffer
 * @param base_path Socket base path (e.g., "/run/zenoh/modem/radio0")
 * @param target_id Target identifier
 * @param reliability Reliability mode
 * @param priority_start Start of priority range
 * @param priority_end End of priority range
 * @param express_threshold Express threshold (-1 to disable)
 * @return Number of characters written, or negative on error
 */
static inline int zenoh_build_locator(
    char* buffer,
    size_t buffer_size,
    const char* base_path,
    const char* target_id,
    zenoh_reliability_t reliability,
    int priority_start,
    int priority_end,
    int express_threshold
) {
    const char* rel_str = (reliability == ZENOH_RELIABILITY_RELIABLE) ? "reliable" : "best_effort";
    int written;
    
    if (express_threshold >= 0) {
        written = snprintf(buffer, buffer_size,
            "unixpipe/%s/targets/%s.sock?rel=%s&prio=%d-%d&express=0-%d",
            base_path, target_id, rel_str, priority_start, priority_end, express_threshold);
    } else {
        written = snprintf(buffer, buffer_size,
            "unixpipe/%s/targets/%s.sock?rel=%s&prio=%d-%d",
            base_path, target_id, rel_str, priority_start, priority_end);
    }
    
    return (written < (int)buffer_size) ? written : -1;
}

#ifdef __cplusplus
}

/* ============================================================================
 * C++ Wrapper (optional, for C++ convenience)
 * ============================================================================ */

#ifdef ZENOH_MODEM_CPP_WRAPPER

#include <functional>
#include <memory>
#include <string>

namespace zenoh {
namespace modem {

/**
 * C++ wrapper for modem driver.
 * Uses std::function for callbacks.
 */
class Driver {
public:
    using GetInfoFn = std::function<zenoh_modem_info_t()>;
    using PollMetricsFn = std::function<zenoh_modem_metrics_t()>;
    using GetStateFn = std::function<zenoh_modem_state_t()>;
    using HandleRequestFn = std::function<zenoh_response_t(const zenoh_request_t&)>;

    Driver(GetInfoFn get_info, PollMetricsFn poll_metrics, GetStateFn get_state)
        : get_info_(std::move(get_info))
        , poll_metrics_(std::move(poll_metrics))
        , get_state_(std::move(get_state))
    {
        callbacks_.get_info = &c_get_info;
        callbacks_.poll_metrics = &c_poll_metrics;
        callbacks_.get_state = &c_get_state;
        callbacks_.handle_request = &c_handle_request;
        driver_ = zenoh_modem_driver_init(&callbacks_, this);
    }

    ~Driver() {
        if (driver_) {
            zenoh_modem_driver_destroy(driver_);
        }
    }

    void set_request_handler(HandleRequestFn handler) {
        handle_request_ = std::move(handler);
    }

    void set_metrics_interval(uint32_t interval_ms) {
        zenoh_modem_driver_set_metrics_interval(driver_, interval_ms);
    }

    int run(const std::string& socket_path) {
        return zenoh_modem_driver_run(driver_, socket_path.c_str());
    }

    int run_background(const std::string& socket_path) {
        return zenoh_modem_driver_run_background(driver_, socket_path.c_str());
    }

    void stop() {
        zenoh_modem_driver_stop(driver_);
    }

    int emit_state_change(const zenoh_state_change_t& change) {
        return zenoh_modem_emit_state_change(driver_, &change);
    }

    int emit_alert(const zenoh_alert_t& alert) {
        return zenoh_modem_emit_alert(driver_, &alert);
    }

    int emit_bandwidth_update(const zenoh_bandwidth_update_t& update) {
        return zenoh_modem_emit_bandwidth_update(driver_, &update);
    }

private:
    static zenoh_modem_info_t c_get_info(void* ctx) {
        return static_cast<Driver*>(ctx)->get_info_();
    }

    static zenoh_modem_metrics_t c_poll_metrics(void* ctx) {
        return static_cast<Driver*>(ctx)->poll_metrics_();
    }

    static zenoh_modem_state_t c_get_state(void* ctx) {
        return static_cast<Driver*>(ctx)->get_state_();
    }

    static zenoh_response_t c_handle_request(void* ctx, const zenoh_request_t* req) {
        auto* self = static_cast<Driver*>(ctx);
        if (self->handle_request_) {
            return self->handle_request_(*req);
        }
        return zenoh_response_not_supported(req->request_id);
    }

    GetInfoFn get_info_;
    PollMetricsFn poll_metrics_;
    GetStateFn get_state_;
    HandleRequestFn handle_request_;
    zenoh_modem_callbacks_t callbacks_;
    zenoh_modem_driver_t* driver_;
};

} // namespace modem
} // namespace zenoh

#endif /* ZENOH_MODEM_CPP_WRAPPER */

#endif /* __cplusplus */

#endif /* ZENOH_MODEM_DRIVER_H */
