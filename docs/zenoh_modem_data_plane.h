/**
 * Zenoh Modem Data Plane C API
 *
 * SUPERSEDED: This header has been superseded by the Link Provider approach.
 * See zenoh_modem_driver.h v1.1.0 for the new link management API.
 *
 * The Link Provider approach uses Zenoh's native Unix transport instead of
 * a custom data plane protocol, providing better integration with Zenoh's
 * QoS, batching, and fragmentation features.
 *
 * This file is retained for historical reference only.
 *
 * Version: 1.0.0 (SUPERSEDED)
 * Protocol Version: 0x01
 *
 * Related: zenoh_modem_driver.h (OAM/Control Plane + Link Management)
 * Superseded By: Link management in zenoh_modem_driver.h v1.1.0
 */

#ifndef ZENOH_MODEM_DATA_PLANE_H
#define ZENOH_MODEM_DATA_PLANE_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Constants
 * ============================================================================ */

#define ZENOH_DP_PROTOCOL_VERSION       0x01
#define ZENOH_DP_MAX_PAYLOAD_SIZE       (1024 * 1024)  /* 1 MB max payload */
#define ZENOH_DP_MAX_TARGET_ADDR_LEN    256
#define ZENOH_DP_MAX_KEY_EXPR_LEN       512
#define ZENOH_DP_MAX_REASON_LEN         256

/* Default timing parameters */
#define ZENOH_DP_DEFAULT_PING_INTERVAL_MS   1000
#define ZENOH_DP_DEFAULT_PING_TIMEOUT_MS    3000
#define ZENOH_DP_DEFAULT_FRAGMENT_TIMEOUT_MS 30000

/* ============================================================================
 * Message Flags
 * ============================================================================ */

#define ZENOH_DP_FLAG_EXPRESS       (1 << 0)  /* Express/bypass queues */
#define ZENOH_DP_FLAG_RELIABLE      (1 << 1)  /* Request delivery confirmation */
#define ZENOH_DP_FLAG_ACK_REQUESTED (1 << 2)  /* Explicit ACK needed */
#define ZENOH_DP_FLAG_FRAGMENT      (1 << 3)  /* This is a fragment */
#define ZENOH_DP_FLAG_LAST_FRAGMENT (1 << 4)  /* Last fragment */
#define ZENOH_DP_FLAG_PRIORITY_MASK (0x07 << 5) /* Priority bits 5-7 */
#define ZENOH_DP_FLAG_PRIORITY_SHIFT 5

/* ============================================================================
 * Enumerations
 * ============================================================================ */

/**
 * Message types for the data plane protocol
 */
typedef enum {
    ZENOH_DP_MSG_DATA_PLANE_READY   = 0x01,
    ZENOH_DP_MSG_DATA_MESSAGE       = 0x02,
    ZENOH_DP_MSG_EXPRESS_MESSAGE    = 0x03,
    ZENOH_DP_MSG_DELIVERY_STATUS    = 0x04,
    ZENOH_DP_MSG_REACHABILITY_UPDATE = 0x05,
    ZENOH_DP_MSG_FLOW_CONTROL       = 0x06,
    ZENOH_DP_MSG_FRAGMENT           = 0x07,
    ZENOH_DP_MSG_TARGET_QUERY       = 0x08,
    ZENOH_DP_MSG_TARGET_RESPONSE    = 0x09,
    ZENOH_DP_MSG_PING               = 0x0A,
    ZENOH_DP_MSG_PONG               = 0x0B,
    ZENOH_DP_MSG_ERROR              = 0xFF
} zenoh_dp_message_type_t;

/**
 * Target types for addressing
 */
typedef enum {
    ZENOH_DP_TARGET_ZENOH_LOCATOR     = 0,
    ZENOH_DP_TARGET_MAC_ADDRESS       = 1,
    ZENOH_DP_TARGET_IP_ADDRESS        = 2,
    ZENOH_DP_TARGET_RADIO_CALLSIGN    = 3,
    ZENOH_DP_TARGET_SATELLITE_TERMINAL = 4,
    ZENOH_DP_TARGET_ACOUSTIC_ADDRESS  = 5,
    ZENOH_DP_TARGET_BROADCAST         = 6,
    ZENOH_DP_TARGET_MULTICAST         = 7,
    ZENOH_DP_TARGET_CUSTOM            = 255
} zenoh_dp_target_type_t;

/**
 * Congestion control modes
 */
typedef enum {
    ZENOH_DP_CONGESTION_DROP  = 0,  /* Drop if congested */
    ZENOH_DP_CONGESTION_BLOCK = 1   /* Block sender if congested */
} zenoh_dp_congestion_control_t;

/**
 * Delivery status codes
 */
typedef enum {
    ZENOH_DP_DELIVERY_DELIVERED    = 0,  /* Successfully delivered */
    ZENOH_DP_DELIVERY_ACKNOWLEDGED = 1,  /* Target acknowledged */
    ZENOH_DP_DELIVERY_QUEUED       = 2,  /* Queued for transmission */
    ZENOH_DP_DELIVERY_TRANSMITTED  = 3,  /* Transmitted (no ACK expected) */
    ZENOH_DP_DELIVERY_FAILED       = 4,  /* Delivery failed */
    ZENOH_DP_DELIVERY_EXPIRED      = 5,  /* TTL expired */
    ZENOH_DP_DELIVERY_REJECTED     = 6,  /* Target rejected */
    ZENOH_DP_DELIVERY_UNREACHABLE  = 7,  /* Target unreachable */
    ZENOH_DP_DELIVERY_CONGESTED    = 8   /* Dropped due to congestion */
} zenoh_dp_delivery_status_t;

/**
 * Reachability status
 */
typedef enum {
    ZENOH_DP_REACHABLE   = 0,  /* Target is reachable */
    ZENOH_DP_UNREACHABLE = 1,  /* Target is not reachable */
    ZENOH_DP_DEGRADED    = 2,  /* Reachable with degraded quality */
    ZENOH_DP_UNKNOWN     = 3   /* Status unknown */
} zenoh_dp_reachability_status_t;

/**
 * Reasons for unreachability
 */
typedef enum {
    ZENOH_DP_UNREACHABLE_LINK_DOWN      = 0,
    ZENOH_DP_UNREACHABLE_NO_ROUTE       = 1,
    ZENOH_DP_UNREACHABLE_TARGET_OFFLINE = 2,
    ZENOH_DP_UNREACHABLE_TIMEOUT        = 3,
    ZENOH_DP_UNREACHABLE_CONGESTION     = 4,
    ZENOH_DP_UNREACHABLE_HANDOVER       = 5,
    ZENOH_DP_UNREACHABLE_INTERFERENCE   = 6,
    ZENOH_DP_UNREACHABLE_OUT_OF_RANGE   = 7,
    ZENOH_DP_UNREACHABLE_AUTH_FAILURE   = 8,
    ZENOH_DP_UNREACHABLE_MAINTENANCE    = 9,
    ZENOH_DP_UNREACHABLE_UNKNOWN        = 255
} zenoh_dp_unreachable_reason_t;

/**
 * Flow control types
 */
typedef enum {
    ZENOH_DP_FLOW_PAUSE       = 0,  /* Stop sending */
    ZENOH_DP_FLOW_RESUME      = 1,  /* Resume sending */
    ZENOH_DP_FLOW_SLOW_DOWN   = 2,  /* Reduce rate */
    ZENOH_DP_FLOW_SPEED_UP    = 3,  /* Can increase rate */
    ZENOH_DP_FLOW_QUEUE_STATUS = 4  /* Queue status update */
} zenoh_dp_flow_control_type_t;

/**
 * Data plane error codes
 */
typedef enum {
    ZENOH_DP_ERROR_INVALID_TARGET       = 0x0100,
    ZENOH_DP_ERROR_TARGET_UNREACHABLE   = 0x0101,
    ZENOH_DP_ERROR_MTU_EXCEEDED         = 0x0102,
    ZENOH_DP_ERROR_QUEUE_FULL           = 0x0103,
    ZENOH_DP_ERROR_DEADLINE_MISSED      = 0x0104,
    ZENOH_DP_ERROR_FRAGMENTATION_FAILED = 0x0105,
    ZENOH_DP_ERROR_DELIVERY_TIMEOUT     = 0x0106,
    ZENOH_DP_ERROR_ACK_TIMEOUT          = 0x0107,
    ZENOH_DP_ERROR_LINK_DOWN            = 0x0108,
    ZENOH_DP_ERROR_AUTH_FAILED          = 0x0109,
    ZENOH_DP_ERROR_ENCRYPTION_ERROR     = 0x010A,
    ZENOH_DP_ERROR_PROTOCOL_ERROR       = 0x010B,
    ZENOH_DP_ERROR_RESOURCE_EXHAUSTED   = 0x010C,
    ZENOH_DP_ERROR_VENDOR_BASE          = 0x0200
} zenoh_dp_error_code_t;

/* ============================================================================
 * Core Structures
 * ============================================================================ */

/**
 * Target address
 */
typedef struct {
    zenoh_dp_target_type_t target_type;
    uint8_t                address[ZENOH_DP_MAX_TARGET_ADDR_LEN];
    uint16_t               address_len;
} zenoh_dp_target_t;

/**
 * Data plane capabilities
 */
typedef struct {
    uint32_t max_mtu;                   /* Maximum transmission unit */
    uint32_t min_mtu;                   /* Minimum MTU (degraded modes) */
    bool     supports_express;          /* Express path available */
    bool     supports_reliable;         /* Reliable delivery available */
    bool     supports_fragmentation;    /* Can handle fragmentation */
    bool     supports_multicast;        /* Multicast supported */
    uint32_t max_pending_messages;      /* Max messages before backpressure */
    uint32_t max_targets;               /* Max tracked targets */
    uint32_t tx_queue_bytes;            /* TX buffer size */
    uint32_t rx_queue_bytes;            /* RX buffer size */
} zenoh_dp_capabilities_t;

/**
 * Data plane ready message (sent by modem after connect)
 */
typedef struct {
    char                     modem_id[64];   /* Must match OAM modem_id */
    uint8_t                  protocol_version;
    zenoh_dp_capabilities_t  capabilities;
} zenoh_dp_ready_t;

/**
 * Message metadata
 */
typedef struct {
    uint8_t                     zenoh_priority;     /* Zenoh priority 0-7 */
    zenoh_dp_congestion_control_t congestion_control;
    char                        key_expr[ZENOH_DP_MAX_KEY_EXPR_LEN]; /* Optional */
    bool                        has_key_expr;
    uint32_t                    encoding;           /* Zenoh encoding ID */
    bool                        has_encoding;
    uint32_t                    ttl_ms;             /* Time to live */
    bool                        has_ttl;
    uint8_t                     source_id[16];      /* Source identifier */
    uint8_t                     source_id_len;
} zenoh_dp_metadata_t;

/**
 * Data message (regular data transmission)
 */
typedef struct {
    uint64_t            message_id;      /* Unique message ID */
    uint64_t            timestamp_ns;    /* Send timestamp */
    zenoh_dp_target_t   target;          /* Destination */
    uint8_t*            payload;         /* Data payload */
    uint32_t            payload_len;     /* Payload length */
    zenoh_dp_metadata_t metadata;        /* Optional metadata */
    bool                has_metadata;
} zenoh_dp_data_message_t;

/**
 * Express message metadata
 */
typedef struct {
    uint64_t deadline_ns;       /* Absolute deadline (drop if missed) */
    uint8_t  zenoh_priority;    /* Must be 0-2 */
    bool     bypass_queue;      /* Skip TX queue if possible */
    bool     require_ack;       /* Need delivery confirmation */
    uint8_t  max_retries;       /* Retry count before giving up */
} zenoh_dp_express_metadata_t;

/**
 * Express message (high priority)
 */
typedef struct {
    uint64_t                   message_id;
    uint64_t                   timestamp_ns;
    zenoh_dp_target_t          target;
    uint8_t*                   payload;
    uint32_t                   payload_len;
    zenoh_dp_express_metadata_t express_metadata;
} zenoh_dp_express_message_t;

/**
 * Delivery details
 */
typedef struct {
    uint32_t latency_us;        /* End-to-end latency */
    bool     has_latency;
    uint8_t  retries;           /* Number of retries used */
    bool     has_retries;
    char     failure_reason[ZENOH_DP_MAX_REASON_LEN];
    bool     has_failure_reason;
    uint8_t  hop_count;         /* Number of hops (mesh) */
    bool     has_hop_count;
} zenoh_dp_delivery_details_t;

/**
 * Delivery status notification
 */
typedef struct {
    uint64_t                    message_id;     /* Original message ID */
    zenoh_dp_delivery_status_t  status;
    uint64_t                    timestamp_ns;
    zenoh_dp_delivery_details_t details;
    bool                        has_details;
} zenoh_dp_delivery_status_msg_t;

/**
 * Reachability details
 */
typedef struct {
    zenoh_dp_unreachable_reason_t reason;
    bool                          has_reason;
    uint32_t                      estimated_recovery_ms;
    bool                          has_estimated_recovery;
    uint64_t                      last_seen_ns;
    bool                          has_last_seen;
    uint8_t                       quality_score;    /* 0-100 if degraded */
    bool                          has_quality_score;
    bool                          alternate_path;
    bool                          has_alternate_path;
} zenoh_dp_reachability_details_t;

/**
 * Reachability update notification
 */
typedef struct {
    uint64_t                       timestamp_ns;
    zenoh_dp_target_t              target;
    zenoh_dp_reachability_status_t status;
    zenoh_dp_reachability_details_t details;
    bool                           has_details;
} zenoh_dp_reachability_update_t;

/**
 * Flow control details
 */
typedef struct {
    /* For Pause/Resume */
    uint32_t duration_ms;
    bool     has_duration;
    char     reason[ZENOH_DP_MAX_REASON_LEN];
    bool     has_reason;
    
    /* For SlowDown/SpeedUp */
    uint64_t suggested_rate_bps;
    bool     has_suggested_rate;
    uint64_t current_rate_bps;
    bool     has_current_rate;
    
    /* For QueueStatus */
    uint32_t queue_depth_bytes;
    bool     has_queue_depth_bytes;
    uint32_t queue_capacity_bytes;
    bool     has_queue_capacity_bytes;
    uint32_t queue_depth_messages;
    bool     has_queue_depth_messages;
    uint8_t  high_water_mark_pct;
    bool     has_high_water_mark;
    uint8_t  low_water_mark_pct;
    bool     has_low_water_mark;
} zenoh_dp_flow_control_details_t;

/**
 * Flow control message
 */
typedef struct {
    uint64_t                       timestamp_ns;
    zenoh_dp_flow_control_type_t   control_type;
    zenoh_dp_target_t              target;      /* Null target = global */
    bool                           has_target;
    zenoh_dp_flow_control_details_t details;
} zenoh_dp_flow_control_t;

/**
 * Fragment message
 */
typedef struct {
    uint64_t original_message_id;   /* ID of complete message */
    uint16_t fragment_index;        /* 0-based index */
    uint16_t total_fragments;       /* Total count */
    uint32_t fragment_offset;       /* Byte offset in original */
    uint8_t* fragment_data;         /* Fragment payload */
    uint32_t fragment_data_len;
    uint32_t original_length;       /* Total original length */
} zenoh_dp_fragment_t;

/**
 * Target query (Zenoh -> Modem)
 */
typedef struct {
    uint32_t           query_id;
    zenoh_dp_target_t* targets;
    uint32_t           target_count;
    uint32_t           timeout_ms;
    bool               detailed;
} zenoh_dp_target_query_t;

/**
 * Target status (in response)
 */
typedef struct {
    zenoh_dp_target_t               target;
    zenoh_dp_reachability_status_t  status;
    zenoh_dp_reachability_details_t details;
    bool                            has_details;
    uint32_t                        rtt_us;
    bool                            has_rtt;
} zenoh_dp_target_status_t;

/**
 * Target response (Modem -> Zenoh)
 */
typedef struct {
    uint32_t               query_id;
    zenoh_dp_target_status_t* results;
    uint32_t               result_count;
} zenoh_dp_target_response_t;

/**
 * Ping message
 */
typedef struct {
    uint64_t timestamp_ns;
    uint32_t seq;
} zenoh_dp_ping_t;

/**
 * Pong message
 */
typedef struct {
    uint64_t timestamp_ns;
    uint32_t seq;
    uint64_t processing_ns;     /* Time spent processing */
} zenoh_dp_pong_t;

/* ============================================================================
 * Callback Function Types
 * ============================================================================ */

/**
 * Callback when data plane connection is established.
 *
 * @param ctx User context
 * @return Data plane capabilities and ready message
 */
typedef zenoh_dp_ready_t (*zenoh_dp_ready_fn)(void* ctx);

/**
 * Callback to transmit a data message.
 *
 * @param ctx User context
 * @param message The message to transmit
 * @return 0 on success (queued), negative on error
 */
typedef int (*zenoh_dp_transmit_fn)(void* ctx, const zenoh_dp_data_message_t* message);

/**
 * Callback to transmit an express message.
 *
 * @param ctx User context
 * @param message The express message to transmit
 * @return 0 on success, negative on error
 */
typedef int (*zenoh_dp_transmit_express_fn)(void* ctx, const zenoh_dp_express_message_t* message);

/**
 * Callback when a message is received from the modem.
 *
 * @param ctx User context
 * @param message The received message
 */
typedef void (*zenoh_dp_receive_fn)(void* ctx, const zenoh_dp_data_message_t* message);

/**
 * Callback to query target reachability.
 *
 * @param ctx User context
 * @param query The query parameters
 * @param response Output: fill with results
 * @return 0 on success, negative on error
 */
typedef int (*zenoh_dp_query_targets_fn)(void* ctx, 
                                         const zenoh_dp_target_query_t* query,
                                         zenoh_dp_target_response_t* response);

/**
 * Collection of data plane callbacks
 */
typedef struct {
    /* Required callbacks */
    zenoh_dp_ready_fn          on_ready;
    zenoh_dp_transmit_fn       transmit;
    zenoh_dp_receive_fn        on_receive;
    
    /* Optional callbacks */
    zenoh_dp_transmit_express_fn transmit_express;  /* NULL if not supported */
    zenoh_dp_query_targets_fn    query_targets;     /* NULL if not supported */
} zenoh_dp_callbacks_t;

/* ============================================================================
 * Data Plane Handle
 * ============================================================================ */

/**
 * Opaque data plane handle
 */
typedef struct zenoh_dp_handle zenoh_dp_handle_t;

/* ============================================================================
 * Lifecycle Functions
 * ============================================================================ */

/**
 * Initialize a data plane handler.
 *
 * @param callbacks Pointer to callback structure
 * @param ctx User context passed to all callbacks
 * @return Handle, or NULL on error
 */
zenoh_dp_handle_t* zenoh_dp_init(
    const zenoh_dp_callbacks_t* callbacks,
    void* ctx
);

/**
 * Set the socket path for the data plane.
 * Default: derived from OAM socket path + "_data"
 *
 * @param handle Data plane handle
 * @param socket_path Path to Unix socket
 */
void zenoh_dp_set_socket_path(
    zenoh_dp_handle_t* handle,
    const char* socket_path
);

/**
 * Set ping interval.
 *
 * @param handle Data plane handle
 * @param interval_ms Ping interval in milliseconds
 */
void zenoh_dp_set_ping_interval(
    zenoh_dp_handle_t* handle,
    uint32_t interval_ms
);

/**
 * Run the data plane server (blocking).
 *
 * @param handle Data plane handle
 * @return 0 on clean shutdown, negative on error
 */
int zenoh_dp_run(zenoh_dp_handle_t* handle);

/**
 * Run the data plane server in background.
 *
 * @param handle Data plane handle
 * @return 0 on success, negative on error
 */
int zenoh_dp_run_background(zenoh_dp_handle_t* handle);

/**
 * Stop the data plane server.
 *
 * @param handle Data plane handle
 */
void zenoh_dp_stop(zenoh_dp_handle_t* handle);

/**
 * Destroy the data plane handler.
 *
 * @param handle Data plane handle
 */
void zenoh_dp_destroy(zenoh_dp_handle_t* handle);

/* ============================================================================
 * Event Emission Functions (Modem -> Zenoh)
 * ============================================================================ */

/**
 * Emit a delivery status notification.
 *
 * @param handle Data plane handle
 * @param status Delivery status details
 * @return 0 on success, negative on error
 */
int zenoh_dp_emit_delivery_status(
    zenoh_dp_handle_t* handle,
    const zenoh_dp_delivery_status_msg_t* status
);

/**
 * Emit a reachability update.
 * Call this when a target becomes reachable/unreachable.
 *
 * @param handle Data plane handle
 * @param update Reachability update details
 * @return 0 on success, negative on error
 */
int zenoh_dp_emit_reachability_update(
    zenoh_dp_handle_t* handle,
    const zenoh_dp_reachability_update_t* update
);

/**
 * Emit a flow control message.
 *
 * @param handle Data plane handle
 * @param flow_control Flow control details
 * @return 0 on success, negative on error
 */
int zenoh_dp_emit_flow_control(
    zenoh_dp_handle_t* handle,
    const zenoh_dp_flow_control_t* flow_control
);

/**
 * Send received data to Zenoh.
 *
 * @param handle Data plane handle
 * @param message Received message
 * @return 0 on success, negative on error
 */
int zenoh_dp_deliver_message(
    zenoh_dp_handle_t* handle,
    const zenoh_dp_data_message_t* message
);

/* ============================================================================
 * Helper Functions
 * ============================================================================ */

/**
 * Create a broadcast target.
 *
 * @return Broadcast target
 */
static inline zenoh_dp_target_t zenoh_dp_target_broadcast(void) {
    zenoh_dp_target_t target = {0};
    target.target_type = ZENOH_DP_TARGET_BROADCAST;
    return target;
}

/**
 * Create a MAC address target.
 *
 * @param mac 6-byte MAC address
 * @return MAC address target
 */
static inline zenoh_dp_target_t zenoh_dp_target_mac(const uint8_t mac[6]) {
    zenoh_dp_target_t target = {0};
    target.target_type = ZENOH_DP_TARGET_MAC_ADDRESS;
    for (int i = 0; i < 6; i++) {
        target.address[i] = mac[i];
    }
    target.address_len = 6;
    return target;
}

/**
 * Create an IPv4 target.
 *
 * @param ip 4-byte IPv4 address
 * @param port Port number
 * @return IPv4 target
 */
static inline zenoh_dp_target_t zenoh_dp_target_ipv4(const uint8_t ip[4], uint16_t port) {
    zenoh_dp_target_t target = {0};
    target.target_type = ZENOH_DP_TARGET_IP_ADDRESS;
    for (int i = 0; i < 4; i++) {
        target.address[i] = ip[i];
    }
    target.address[4] = (port >> 8) & 0xFF;
    target.address[5] = port & 0xFF;
    target.address_len = 6;
    return target;
}

/**
 * Create a radio callsign target.
 *
 * @param callsign Null-terminated callsign string
 * @return Radio callsign target
 */
static inline zenoh_dp_target_t zenoh_dp_target_callsign(const char* callsign) {
    zenoh_dp_target_t target = {0};
    target.target_type = ZENOH_DP_TARGET_RADIO_CALLSIGN;
    size_t len = 0;
    while (callsign[len] && len < ZENOH_DP_MAX_TARGET_ADDR_LEN - 1) {
        target.address[len] = (uint8_t)callsign[len];
        len++;
    }
    target.address_len = (uint16_t)len;
    return target;
}

/**
 * Create an unreachable status update.
 *
 * @param target The unreachable target
 * @param reason Reason for unreachability
 * @return Reachability update structure
 */
static inline zenoh_dp_reachability_update_t zenoh_dp_unreachable(
    zenoh_dp_target_t target,
    zenoh_dp_unreachable_reason_t reason
) {
    zenoh_dp_reachability_update_t update = {0};
    update.timestamp_ns = 0; /* Will be filled by emit function */
    update.target = target;
    update.status = ZENOH_DP_UNREACHABLE;
    update.has_details = true;
    update.details.reason = reason;
    update.details.has_reason = true;
    return update;
}

/**
 * Create a reachable status update.
 *
 * @param target The reachable target
 * @return Reachability update structure
 */
static inline zenoh_dp_reachability_update_t zenoh_dp_reachable(
    zenoh_dp_target_t target
) {
    zenoh_dp_reachability_update_t update = {0};
    update.target = target;
    update.status = ZENOH_DP_REACHABLE;
    update.has_details = false;
    return update;
}

/**
 * Create a flow control pause message.
 *
 * @param reason Reason for pause (can be NULL)
 * @return Flow control structure
 */
static inline zenoh_dp_flow_control_t zenoh_dp_flow_pause(const char* reason) {
    zenoh_dp_flow_control_t fc = {0};
    fc.control_type = ZENOH_DP_FLOW_PAUSE;
    fc.has_target = false;  /* Global */
    if (reason) {
        size_t len = 0;
        while (reason[len] && len < ZENOH_DP_MAX_REASON_LEN - 1) {
            fc.details.reason[len] = reason[len];
            len++;
        }
        fc.details.has_reason = true;
    }
    return fc;
}

/**
 * Create a flow control resume message.
 *
 * @return Flow control structure
 */
static inline zenoh_dp_flow_control_t zenoh_dp_flow_resume(void) {
    zenoh_dp_flow_control_t fc = {0};
    fc.control_type = ZENOH_DP_FLOW_RESUME;
    fc.has_target = false;
    return fc;
}

/**
 * Create a delivery status for successful transmission.
 *
 * @param message_id Original message ID
 * @return Delivery status structure
 */
static inline zenoh_dp_delivery_status_msg_t zenoh_dp_status_transmitted(uint64_t message_id) {
    zenoh_dp_delivery_status_msg_t status = {0};
    status.message_id = message_id;
    status.status = ZENOH_DP_DELIVERY_TRANSMITTED;
    return status;
}

/**
 * Create a delivery status for unreachable target.
 *
 * @param message_id Original message ID
 * @param reason Failure reason (can be NULL)
 * @return Delivery status structure
 */
static inline zenoh_dp_delivery_status_msg_t zenoh_dp_status_unreachable(
    uint64_t message_id,
    const char* reason
) {
    zenoh_dp_delivery_status_msg_t status = {0};
    status.message_id = message_id;
    status.status = ZENOH_DP_DELIVERY_UNREACHABLE;
    if (reason) {
        status.has_details = true;
        size_t len = 0;
        while (reason[len] && len < ZENOH_DP_MAX_REASON_LEN - 1) {
            status.details.failure_reason[len] = reason[len];
            len++;
        }
        status.details.has_failure_reason = true;
    }
    return status;
}

/**
 * Get current timestamp in nanoseconds.
 *
 * @return Timestamp in nanoseconds since epoch
 */
uint64_t zenoh_dp_timestamp_ns(void);

/**
 * Compare two targets for equality.
 *
 * @param a First target
 * @param b Second target
 * @return true if equal, false otherwise
 */
bool zenoh_dp_target_equals(
    const zenoh_dp_target_t* a,
    const zenoh_dp_target_t* b
);

/**
 * Get priority from flags byte.
 *
 * @param flags Flags byte
 * @return Priority (0-7)
 */
static inline uint8_t zenoh_dp_get_priority(uint8_t flags) {
    return (flags & ZENOH_DP_FLAG_PRIORITY_MASK) >> ZENOH_DP_FLAG_PRIORITY_SHIFT;
}

/**
 * Set priority in flags byte.
 *
 * @param flags Pointer to flags byte
 * @param priority Priority (0-7)
 */
static inline void zenoh_dp_set_priority(uint8_t* flags, uint8_t priority) {
    *flags = (*flags & ~ZENOH_DP_FLAG_PRIORITY_MASK) | 
             ((priority & 0x07) << ZENOH_DP_FLAG_PRIORITY_SHIFT);
}

#ifdef __cplusplus
}
#endif

#endif /* ZENOH_MODEM_DATA_PLANE_H */
