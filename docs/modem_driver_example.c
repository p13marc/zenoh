/**
 * Example Modem Driver Implementation in C
 *
 * This file demonstrates how to implement a modem driver using the
 * Zenoh Modem Driver C API. This example implements a tactical radio modem.
 *
 * Compile with:
 *   gcc -o my_modem modem_driver_example.c -lzenoh_modem -lpthread
 *
 * Or link with the Rust library:
 *   gcc -o my_modem modem_driver_example.c -L./target/release -lzenoh_modem_ffi
 */

#include "zenoh_modem_driver.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>
#include <signal.h>
#include <math.h>

/* ============================================================================
 * Driver State
 * ============================================================================ */

typedef struct {
    /* Current state */
    zenoh_modem_state_t state;
    zenoh_modem_state_t previous_state;

    /* Signal metrics (simulated or from real hardware) */
    double snr_db;
    double rssi_dbm;
    uint8_t signal_quality;

    /* Radio configuration */
    double frequency_mhz;
    double tx_power_dbm;
    bool frequency_hopping;
    bool encryption_active;

    /* Bandwidth tracking */
    uint64_t tx_rate_bps;
    uint64_t rx_rate_bps;
    uint64_t max_bandwidth_bps;

    /* Counters */
    uint32_t metrics_seq;
    uint32_t alert_id;
    uint32_t crc_errors;
    uint32_t retransmits;

    /* Thread synchronization */
    pthread_mutex_t lock;

    /* Driver handle for emitting events */
    zenoh_modem_driver_t* driver;

} radio_driver_state_t;

static radio_driver_state_t g_state;
static volatile sig_atomic_t g_running = 1;

/* ============================================================================
 * Signal Handler
 * ============================================================================ */

static void signal_handler(int sig) {
    (void)sig;
    g_running = 0;
    if (g_state.driver) {
        zenoh_modem_driver_stop(g_state.driver);
    }
}

/* ============================================================================
 * Hardware Simulation (replace with real hardware access)
 * ============================================================================ */

static void update_simulated_metrics(radio_driver_state_t* state) {
    /* Simulate slight variations in signal quality */
    state->snr_db += ((double)rand() / RAND_MAX - 0.5) * 0.5;
    if (state->snr_db > 35.0) state->snr_db = 35.0;
    if (state->snr_db < 5.0) state->snr_db = 5.0;

    state->rssi_dbm += ((double)rand() / RAND_MAX - 0.5) * 1.0;
    if (state->rssi_dbm > -40.0) state->rssi_dbm = -40.0;
    if (state->rssi_dbm < -110.0) state->rssi_dbm = -110.0;

    /* Calculate signal quality from SNR */
    state->signal_quality = (uint8_t)fmin(100.0, fmax(0.0,
        (state->snr_db - 5.0) / 30.0 * 100.0));

    /* Simulate occasional errors */
    if (rand() % 100 < 2) {
        state->crc_errors++;
    }
    if (rand() % 100 < 5) {
        state->retransmits++;
    }
}

/* ============================================================================
 * Callback: Get Modem Info
 * ============================================================================ */

static zenoh_modem_info_t get_info(void* ctx) {
    radio_driver_state_t* state = (radio_driver_state_t*)ctx;
    zenoh_modem_info_t info = zenoh_modem_info_init();

    /* Basic identification */
    strncpy(info.modem_id, "radio0", ZENOH_MODEM_MAX_ID_LEN - 1);
    info.modem_type = ZENOH_MODEM_TYPE_RADIO_TACTICAL;
    strncpy(info.model, "Example Tactical Radio TRC-100", ZENOH_MODEM_MAX_STRING_LEN - 1);
    strncpy(info.firmware_version, "2.1.0", ZENOH_MODEM_MAX_ID_LEN - 1);
    strncpy(info.serial_number, "TRC100-2024-001234", ZENOH_MODEM_MAX_ID_LEN - 1);
    info.has_serial_number = true;

    /* Capabilities */
    info.capabilities.supports_reliable_mode = true;
    info.capabilities.supports_throughput_mode = true;
    info.capabilities.supports_power_control = true;
    info.capabilities.supports_frequency_hop = true;
    info.capabilities.supports_encryption = true;
    info.capabilities.max_bandwidth_bps = 2000000;  /* 2 Mbps */
    info.capabilities.min_bandwidth_bps = 9600;     /* 9.6 kbps */
    info.capabilities.metrics_interval_ms = 500;

    /* Initial state */
    pthread_mutex_lock(&state->lock);
    info.initial_state = state->state;

    /* Initial metrics */
    info.initial_metrics.timestamp_ns = zenoh_modem_timestamp_ns();
    info.initial_metrics.seq = 0;
    info.initial_metrics.signal.snr_cdb = zenoh_db_to_cdb(state->snr_db);
    info.initial_metrics.signal.rssi_cdbm = zenoh_db_to_cdb(state->rssi_dbm);
    info.initial_metrics.signal.signal_quality_pct = state->signal_quality;
    info.initial_metrics.bandwidth.tx_rate_bps = state->tx_rate_bps;
    info.initial_metrics.bandwidth.rx_rate_bps = state->rx_rate_bps;
    info.initial_metrics.bandwidth.available_bps = state->max_bandwidth_bps;
    pthread_mutex_unlock(&state->lock);

    return info;
}

/* ============================================================================
 * Callback: Poll Metrics
 * ============================================================================ */

static zenoh_modem_metrics_t poll_metrics(void* ctx) {
    radio_driver_state_t* state = (radio_driver_state_t*)ctx;
    zenoh_modem_metrics_t metrics = zenoh_modem_metrics_init();

    pthread_mutex_lock(&state->lock);

    /* Update simulated values */
    update_simulated_metrics(state);

    /* Timestamp and sequence */
    metrics.timestamp_ns = zenoh_modem_timestamp_ns();
    metrics.seq = ++state->metrics_seq;

    /* Signal metrics */
    metrics.signal.snr_cdb = zenoh_db_to_cdb(state->snr_db);
    metrics.signal.rssi_cdbm = zenoh_db_to_cdb(state->rssi_dbm);
    metrics.signal.signal_quality_pct = state->signal_quality;
    metrics.signal.noise_floor_cdbm = zenoh_db_to_cdb(-100.0);
    metrics.signal.has_noise_floor = true;

    /* Error metrics */
    metrics.errors.ber_exponent = 6;  /* 10^-6 */
    metrics.errors.per_ppm = zenoh_pct_to_ppm(0.1);  /* 0.1% PER */
    metrics.errors.fer_ppm = zenoh_pct_to_ppm(0.05); /* 0.05% FER */
    metrics.errors.crc_errors = state->crc_errors;
    metrics.errors.retransmits = state->retransmits;

    /* Reset counters after reporting */
    state->crc_errors = 0;
    state->retransmits = 0;

    /* Bandwidth metrics */
    metrics.bandwidth.tx_rate_bps = state->tx_rate_bps;
    metrics.bandwidth.rx_rate_bps = state->rx_rate_bps;
    metrics.bandwidth.available_bps = state->max_bandwidth_bps;
    metrics.bandwidth.utilization_pct = (uint8_t)(
        (state->tx_rate_bps + state->rx_rate_bps) * 100 /
        (2 * state->max_bandwidth_bps));
    metrics.bandwidth.queue_depth_bytes = 1024;
    metrics.bandwidth.queue_capacity_bytes = 65536;

    /* Radio-specific metrics */
    metrics.has_radio = true;
    metrics.radio.tx_power_cdbm = (int16_t)(state->tx_power_dbm * 100);
    metrics.radio.frequency_hz = zenoh_mhz_to_hz(state->frequency_mhz);
    metrics.radio.bandwidth_hz = 25000;  /* 25 kHz channel */
    strncpy(metrics.radio.modulation, "QPSK", ZENOH_MODEM_MAX_ID_LEN - 1);
    strncpy(metrics.radio.fec_rate, "1/2", ZENOH_MODEM_MAX_ID_LEN - 1);
    metrics.radio.hopping = state->frequency_hopping;
    metrics.radio.crypto_active = state->encryption_active;

    pthread_mutex_unlock(&state->lock);

    return metrics;
}

/* ============================================================================
 * Callback: Get State
 * ============================================================================ */

static zenoh_modem_state_t get_state(void* ctx) {
    radio_driver_state_t* state = (radio_driver_state_t*)ctx;
    zenoh_modem_state_t current;

    pthread_mutex_lock(&state->lock);
    current = state->state;
    pthread_mutex_unlock(&state->lock);

    return current;
}

/* ============================================================================
 * Callback: Handle Request
 * ============================================================================ */

static zenoh_response_t handle_request(void* ctx, const zenoh_request_t* request) {
    radio_driver_state_t* state = (radio_driver_state_t*)ctx;

    switch (request->request_type) {
    case ZENOH_REQUEST_TYPE_SET_MODE: {
        zenoh_response_t resp = zenoh_response_success(request->request_id);
        resp.has_data = true;

        pthread_mutex_lock(&state->lock);

        switch (request->data.set_mode.mode) {
        case ZENOH_OPERATING_MODE_RELIABLE:
            /* Switch to reliable mode: lower rate, stronger FEC */
            state->max_bandwidth_bps = 500000;
            printf("[DRIVER] Switched to RELIABLE mode (500 kbps)\n");
            break;

        case ZENOH_OPERATING_MODE_THROUGHPUT:
            /* Switch to throughput mode: higher rate, weaker FEC */
            state->max_bandwidth_bps = 2000000;
            printf("[DRIVER] Switched to THROUGHPUT mode (2 Mbps)\n");
            break;

        case ZENOH_OPERATING_MODE_LOW_POWER:
            /* Reduce TX power */
            state->tx_power_dbm = 10.0;
            printf("[DRIVER] Switched to LOW_POWER mode (10 dBm)\n");
            break;

        case ZENOH_OPERATING_MODE_STEALTH:
            /* Enable frequency hopping, reduce power */
            state->frequency_hopping = true;
            state->tx_power_dbm = 5.0;
            printf("[DRIVER] Switched to STEALTH mode\n");
            break;

        default:
            /* Normal mode */
            state->max_bandwidth_bps = 1000000;
            state->tx_power_dbm = 30.0;
            printf("[DRIVER] Switched to NORMAL mode\n");
            break;
        }

        resp.data.mode_set.applied_mode = request->data.set_mode.mode;
        resp.data.mode_set.has_effective_until = false;

        pthread_mutex_unlock(&state->lock);
        return resp;
    }

    case ZENOH_REQUEST_TYPE_TRAFFIC_HINT: {
        zenoh_response_t resp = zenoh_response_success(request->request_id);
        resp.has_data = true;

        /* Acknowledge traffic hint and potentially adjust modem */
        printf("[DRIVER] Traffic hint: TX=%lu bps, RX=%lu bps, priority=%u\n",
               (unsigned long)request->data.traffic_hint.expected_tx_bps,
               (unsigned long)request->data.traffic_hint.expected_rx_bps,
               request->data.traffic_hint.priority);

        resp.data.traffic_hint.accepted = true;
        resp.data.traffic_hint.has_adjusted_tx = false;
        resp.data.traffic_hint.has_adjusted_rx = false;

        return resp;
    }

    case ZENOH_REQUEST_TYPE_GET_METRICS: {
        zenoh_response_t resp = zenoh_response_success(request->request_id);
        resp.has_data = true;
        resp.data.metrics = poll_metrics(ctx);
        return resp;
    }

    case ZENOH_REQUEST_TYPE_GET_STATE: {
        zenoh_response_t resp = zenoh_response_success(request->request_id);
        resp.has_data = true;

        pthread_mutex_lock(&state->lock);
        resp.data.state = zenoh_state_change_init(state->previous_state, state->state);
        pthread_mutex_unlock(&state->lock);

        return resp;
    }

    case ZENOH_REQUEST_TYPE_GET_INFO: {
        zenoh_response_t resp = zenoh_response_success(request->request_id);
        resp.has_data = true;
        resp.data.info = get_info(ctx);
        return resp;
    }

    default:
        return zenoh_response_not_supported(request->request_id);
    }
}

/* ============================================================================
 * State Transition Helper
 * ============================================================================ */

static void transition_state(radio_driver_state_t* state,
                             zenoh_modem_state_t new_state,
                             const char* reason) {
    pthread_mutex_lock(&state->lock);

    if (state->state == new_state) {
        pthread_mutex_unlock(&state->lock);
        return;
    }

    zenoh_state_change_t change = zenoh_state_change_init(state->state, new_state);
    if (reason) {
        strncpy(change.reason, reason, ZENOH_MODEM_MAX_STRING_LEN - 1);
        change.has_reason = true;
    }

    state->previous_state = state->state;
    state->state = new_state;

    pthread_mutex_unlock(&state->lock);

    /* Emit state change to Zenoh */
    if (state->driver) {
        zenoh_modem_emit_state_change(state->driver, &change);
    }

    printf("[DRIVER] State: %d -> %d (%s)\n",
           change.previous_state, change.current_state,
           reason ? reason : "");
}

/* ============================================================================
 * Alert Emission Helper
 * ============================================================================ */

static void emit_signal_degrading_alert(radio_driver_state_t* state,
                                        double current_snr,
                                        double trend_per_sec) {
    zenoh_alert_t alert = zenoh_alert_init(
        ZENOH_ALERT_TYPE_SIGNAL_DEGRADING,
        ZENOH_ALERT_SEVERITY_WARNING
    );

    pthread_mutex_lock(&state->lock);
    alert.alert_id = ++state->alert_id;
    pthread_mutex_unlock(&state->lock);

    snprintf(alert.message, ZENOH_MODEM_MAX_MESSAGE_LEN,
             "Signal degrading: SNR=%.1f dB, trend=%.2f dB/s",
             current_snr, trend_per_sec);

    alert.data.signal_degrading.current_snr_cdb = zenoh_db_to_cdb(current_snr);
    alert.data.signal_degrading.trend_cdb_per_sec = zenoh_db_to_cdb(trend_per_sec);
    alert.data.signal_degrading.has_estimated_failure = false;

    alert.ttl_ms = 30000;
    alert.has_ttl = true;

    if (state->driver) {
        zenoh_modem_emit_alert(state->driver, &alert);
    }

    printf("[DRIVER] Alert: %s\n", alert.message);
}

/* ============================================================================
 * Background Monitor Thread (simulates hardware monitoring)
 * ============================================================================ */

static void* monitor_thread(void* arg) {
    radio_driver_state_t* state = (radio_driver_state_t*)arg;
    double prev_snr = 20.0;

    while (g_running) {
        sleep(1);

        pthread_mutex_lock(&state->lock);
        double current_snr = state->snr_db;
        pthread_mutex_unlock(&state->lock);

        /* Check for signal degradation */
        double trend = current_snr - prev_snr;
        if (trend < -2.0 && current_snr < 15.0) {
            emit_signal_degrading_alert(state, current_snr, trend);
        }

        /* Check for state transitions based on signal quality */
        if (current_snr < 8.0 && state->state == ZENOH_MODEM_STATE_CONNECTED) {
            transition_state(state, ZENOH_MODEM_STATE_DEGRADED, "Low SNR");
        } else if (current_snr >= 12.0 && state->state == ZENOH_MODEM_STATE_DEGRADED) {
            transition_state(state, ZENOH_MODEM_STATE_CONNECTED, "SNR recovered");
        }

        prev_snr = current_snr;
    }

    return NULL;
}

/* ============================================================================
 * Main
 * ============================================================================ */

int main(int argc, char* argv[]) {
    const char* socket_path = "/run/zenoh/modem_radio0.sock";

    if (argc > 1) {
        socket_path = argv[1];
    }

    printf("Zenoh Modem Driver Example - Tactical Radio\n");
    printf("Socket: %s\n", socket_path);

    /* Initialize state */
    memset(&g_state, 0, sizeof(g_state));
    pthread_mutex_init(&g_state.lock, NULL);
    g_state.state = ZENOH_MODEM_STATE_INITIALIZING;
    g_state.snr_db = 25.0;
    g_state.rssi_dbm = -70.0;
    g_state.signal_quality = 80;
    g_state.frequency_mhz = 450.0;
    g_state.tx_power_dbm = 30.0;
    g_state.frequency_hopping = false;
    g_state.encryption_active = true;
    g_state.tx_rate_bps = 500000;
    g_state.rx_rate_bps = 300000;
    g_state.max_bandwidth_bps = 1000000;

    /* Set up signal handler */
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    /* Set up callbacks */
    zenoh_modem_callbacks_t callbacks = {
        .get_info = get_info,
        .poll_metrics = poll_metrics,
        .get_state = get_state,
        .get_bandwidth = NULL,  /* Will use poll_metrics */
        .handle_request = handle_request,
        .handle_configure = NULL  /* Not implemented */
    };

    /* Initialize driver */
    g_state.driver = zenoh_modem_driver_init(&callbacks, &g_state);
    if (!g_state.driver) {
        fprintf(stderr, "Failed to initialize driver\n");
        return 1;
    }

    /* Configure intervals */
    zenoh_modem_driver_set_metrics_interval(g_state.driver, 500);
    zenoh_modem_driver_set_heartbeat_interval(g_state.driver, 5000);

    /* Start monitor thread */
    pthread_t monitor;
    pthread_create(&monitor, NULL, monitor_thread, &g_state);

    /* Simulate initialization sequence */
    printf("[DRIVER] Initializing...\n");
    sleep(1);
    transition_state(&g_state, ZENOH_MODEM_STATE_SEARCHING, "Starting scan");
    sleep(1);
    transition_state(&g_state, ZENOH_MODEM_STATE_SYNCHRONIZING, "Found network");
    sleep(1);
    transition_state(&g_state, ZENOH_MODEM_STATE_CONNECTED, "Synchronized");

    /* Run driver (blocking) */
    printf("[DRIVER] Running...\n");
    int ret = zenoh_modem_driver_run(g_state.driver, socket_path);

    if (ret != 0) {
        fprintf(stderr, "Driver error: %d\n", ret);
    }

    /* Cleanup */
    printf("[DRIVER] Shutting down...\n");
    g_running = 0;
    pthread_join(monitor, NULL);
    zenoh_modem_driver_destroy(g_state.driver);
    pthread_mutex_destroy(&g_state.lock);

    return ret;
}
