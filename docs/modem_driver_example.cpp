/**
 * Example Modem Driver Implementation in C++
 *
 * This file demonstrates how to implement a modem driver using the
 * Zenoh Modem Driver C++ wrapper. This example implements a LEO satellite modem.
 *
 * Compile with:
 *   g++ -std=c++17 -DZENOH_MODEM_CPP_WRAPPER -o my_sat_modem \
 *       modem_driver_example.cpp -lzenoh_modem -lpthread
 */

#define ZENOH_MODEM_CPP_WRAPPER
#include "zenoh_modem_driver.h"

#include <iostream>
#include <thread>
#include <atomic>
#include <mutex>
#include <chrono>
#include <cmath>
#include <random>
#include <csignal>

/* ============================================================================
 * Satellite Modem Driver Class
 * ============================================================================ */

class LeoSatelliteModem {
public:
    LeoSatelliteModem()
        : state_(ZENOH_MODEM_STATE_INITIALIZING)
        , elevation_deg_(45.0)
        , azimuth_deg_(180.0)
        , snr_db_(18.0)
        , rssi_dbm_(-85.0)
        , doppler_hz_(0)
        , current_satellite_("STARLINK-1234")
        , handover_pending_(false)
        , time_to_handover_ms_(0)
        , metrics_seq_(0)
        , alert_id_(0)
        , running_(true)
        , rng_(std::random_device{}())
    {
        // Start satellite tracking simulation
        tracking_thread_ = std::thread(&LeoSatelliteModem::trackingSim, this);
    }

    ~LeoSatelliteModem() {
        running_ = false;
        if (tracking_thread_.joinable()) {
            tracking_thread_.join();
        }
    }

    // Get modem information
    zenoh_modem_info_t getInfo() {
        zenoh_modem_info_t info = zenoh_modem_info_init();

        strncpy(info.modem_id, "sat0", ZENOH_MODEM_MAX_ID_LEN - 1);
        info.modem_type = ZENOH_MODEM_TYPE_SATELLITE_LEO;
        strncpy(info.model, "Starlink User Terminal Gen2", ZENOH_MODEM_MAX_STRING_LEN - 1);
        strncpy(info.firmware_version, "2024.12.1", ZENOH_MODEM_MAX_ID_LEN - 1);

        info.capabilities.supports_reliable_mode = true;
        info.capabilities.supports_throughput_mode = true;
        info.capabilities.supports_power_control = false;
        info.capabilities.supports_frequency_hop = false;
        info.capabilities.supports_encryption = true;
        info.capabilities.max_bandwidth_bps = 200'000'000;  // 200 Mbps
        info.capabilities.min_bandwidth_bps = 1'000'000;    // 1 Mbps
        info.capabilities.metrics_interval_ms = 500;

        std::lock_guard<std::mutex> lock(mutex_);
        info.initial_state = state_;
        info.initial_metrics = buildMetrics();

        return info;
    }

    // Poll current metrics
    zenoh_modem_metrics_t pollMetrics() {
        std::lock_guard<std::mutex> lock(mutex_);
        return buildMetrics();
    }

    // Get current state
    zenoh_modem_state_t getState() {
        std::lock_guard<std::mutex> lock(mutex_);
        return state_;
    }

    // Handle requests from Zenoh
    zenoh_response_t handleRequest(const zenoh_request_t& request) {
        switch (request.request_type) {
        case ZENOH_REQUEST_TYPE_SET_MODE: {
            auto resp = zenoh_response_success(request.request_id);
            resp.has_data = true;
            resp.data.mode_set.applied_mode = request.data.set_mode.mode;
            
            std::cout << "[SAT] Mode set to: " << static_cast<int>(request.data.set_mode.mode) << "\n";
            return resp;
        }

        case ZENOH_REQUEST_TYPE_TRAFFIC_HINT: {
            auto resp = zenoh_response_success(request.request_id);
            resp.has_data = true;
            resp.data.traffic_hint.accepted = true;

            std::cout << "[SAT] Traffic hint: TX=" << request.data.traffic_hint.expected_tx_bps
                      << " bps, priority=" << static_cast<int>(request.data.traffic_hint.priority) << "\n";
            return resp;
        }

        default:
            return zenoh_response_not_supported(request.request_id);
        }
    }

    // Set driver handle for emitting events
    void setDriver(zenoh::modem::Driver* driver) {
        driver_ = driver;
    }

    // Simulate connection sequence
    void simulateConnection() {
        std::this_thread::sleep_for(std::chrono::seconds(1));
        transitionState(ZENOH_MODEM_STATE_SEARCHING, "Scanning for satellites");
        
        std::this_thread::sleep_for(std::chrono::seconds(2));
        transitionState(ZENOH_MODEM_STATE_SYNCHRONIZING, "Acquiring signal");
        
        std::this_thread::sleep_for(std::chrono::seconds(1));
        transitionState(ZENOH_MODEM_STATE_CONNECTED, "Connected to " + current_satellite_);
    }

private:
    zenoh_modem_metrics_t buildMetrics() {
        zenoh_modem_metrics_t metrics = zenoh_modem_metrics_init();

        metrics.timestamp_ns = zenoh_modem_timestamp_ns();
        metrics.seq = ++metrics_seq_;

        // Signal metrics
        metrics.signal.snr_cdb = zenoh_db_to_cdb(snr_db_);
        metrics.signal.rssi_cdbm = zenoh_db_to_cdb(rssi_dbm_);
        metrics.signal.signal_quality_pct = static_cast<uint8_t>(
            std::clamp((snr_db_ - 5.0) / 20.0 * 100.0, 0.0, 100.0));

        // Error metrics (simulated low error rates)
        metrics.errors.ber_exponent = 9;  // 10^-9
        metrics.errors.per_ppm = 100;     // 0.01%
        metrics.errors.fer_ppm = 50;

        // Bandwidth metrics
        metrics.bandwidth.tx_rate_bps = 50'000'000;   // 50 Mbps
        metrics.bandwidth.rx_rate_bps = 150'000'000;  // 150 Mbps
        metrics.bandwidth.available_bps = 180'000'000;
        metrics.bandwidth.utilization_pct = 40;
        metrics.bandwidth.queue_depth_bytes = 8192;
        metrics.bandwidth.queue_capacity_bytes = 1'048'576;

        // Satellite-specific metrics
        metrics.has_satellite = true;
        metrics.satellite.elevation_cdeg = zenoh_deg_to_cdeg(elevation_deg_);
        metrics.satellite.azimuth_cdeg = static_cast<uint16_t>(azimuth_deg_ * 100);
        metrics.satellite.doppler_hz = doppler_hz_;
        metrics.satellite.range_km = 550;  // ~550 km altitude
        metrics.satellite.has_range = true;
        strncpy(metrics.satellite.satellite_id, current_satellite_.c_str(),
                ZENOH_MODEM_MAX_ID_LEN - 1);
        metrics.satellite.handover_pending = handover_pending_;
        if (handover_pending_) {
            metrics.satellite.time_to_handover_ms = time_to_handover_ms_;
            metrics.satellite.has_time_to_handover = true;
        }

        return metrics;
    }

    void transitionState(zenoh_modem_state_t new_state, const std::string& reason) {
        zenoh_modem_state_t prev;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            if (state_ == new_state) return;
            prev = state_;
            state_ = new_state;
        }

        auto change = zenoh_state_change_init(prev, new_state);
        strncpy(change.reason, reason.c_str(), ZENOH_MODEM_MAX_STRING_LEN - 1);
        change.has_reason = true;

        if (driver_) {
            driver_->emit_state_change(change);
        }

        std::cout << "[SAT] State: " << static_cast<int>(prev) << " -> "
                  << static_cast<int>(new_state) << " (" << reason << ")\n";
    }

    void emitHandoverAlert() {
        auto alert = zenoh_alert_init(ZENOH_ALERT_TYPE_HANDOVER_IMMINENT,
                                      ZENOH_ALERT_SEVERITY_WARNING);
        alert.alert_id = ++alert_id_;

        snprintf(alert.message, ZENOH_MODEM_MAX_MESSAGE_LEN,
                 "Handover imminent: %s -> next satellite in %u ms",
                 current_satellite_.c_str(), time_to_handover_ms_);

        strncpy(alert.data.handover.current_satellite_id, current_satellite_.c_str(),
                ZENOH_MODEM_MAX_ID_LEN - 1);
        alert.data.handover.has_current_satellite_id = true;
        alert.data.handover.estimated_duration_ms = 2000;
        alert.data.handover.expected_outage_ms = 200;

        if (driver_) {
            driver_->emit_alert(alert);
        }

        std::cout << "[SAT] Alert: " << alert.message << "\n";
    }

    // Simulate satellite tracking and orbital dynamics
    void trackingSim() {
        std::uniform_real_distribution<> noise(-0.5, 0.5);
        double orbit_phase = 0.0;
        
        while (running_) {
            std::this_thread::sleep_for(std::chrono::milliseconds(100));

            std::lock_guard<std::mutex> lock(mutex_);

            // Simulate satellite pass (elevation changes over time)
            orbit_phase += 0.001;
            elevation_deg_ = 30.0 + 50.0 * std::sin(orbit_phase) + noise(rng_);
            elevation_deg_ = std::clamp(elevation_deg_, 5.0, 90.0);

            // Azimuth slowly changes
            azimuth_deg_ = std::fmod(azimuth_deg_ + 0.05, 360.0);

            // Doppler varies with elevation (simplified)
            doppler_hz_ = static_cast<int32_t>(-50000 * std::cos(orbit_phase));

            // SNR correlates with elevation
            snr_db_ = 10.0 + (elevation_deg_ - 10.0) * 0.2 + noise(rng_);
            snr_db_ = std::clamp(snr_db_, 5.0, 25.0);

            // RSSI also varies
            rssi_dbm_ = -95.0 + (elevation_deg_ - 10.0) * 0.15 + noise(rng_);

            // Simulate handover when elevation drops
            if (elevation_deg_ < 20.0 && !handover_pending_) {
                handover_pending_ = true;
                time_to_handover_ms_ = 30000;
                emitHandoverAlert();
            } else if (handover_pending_) {
                time_to_handover_ms_ = std::max(0u, time_to_handover_ms_ - 100);
                
                if (time_to_handover_ms_ == 0) {
                    // Perform handover
                    transitionState(ZENOH_MODEM_STATE_HANDOVER, "Switching satellites");
                    
                    std::this_thread::sleep_for(std::chrono::milliseconds(200));
                    
                    // New satellite
                    int sat_num = 1000 + (std::uniform_int_distribution<>(0, 9999)(rng_));
                    current_satellite_ = "STARLINK-" + std::to_string(sat_num);
                    handover_pending_ = false;
                    elevation_deg_ = 45.0;  // Reset to mid-elevation
                    orbit_phase = 0.0;

                    transitionState(ZENOH_MODEM_STATE_CONNECTED,
                                    "Connected to " + current_satellite_);
                }
            }

            // Detect degraded conditions
            if (state_ == ZENOH_MODEM_STATE_CONNECTED && snr_db_ < 10.0) {
                transitionState(ZENOH_MODEM_STATE_DEGRADED, "Low SNR");
            } else if (state_ == ZENOH_MODEM_STATE_DEGRADED && snr_db_ >= 12.0) {
                transitionState(ZENOH_MODEM_STATE_CONNECTED, "SNR recovered");
            }
        }
    }

    // State
    std::mutex mutex_;
    zenoh_modem_state_t state_;
    double elevation_deg_;
    double azimuth_deg_;
    double snr_db_;
    double rssi_dbm_;
    int32_t doppler_hz_;
    std::string current_satellite_;
    bool handover_pending_;
    uint32_t time_to_handover_ms_;
    uint32_t metrics_seq_;
    uint32_t alert_id_;
    std::atomic<bool> running_;
    std::mt19937 rng_;
    std::thread tracking_thread_;
    zenoh::modem::Driver* driver_ = nullptr;
};

/* ============================================================================
 * Main
 * ============================================================================ */

static std::atomic<bool> g_running{true};
static zenoh::modem::Driver* g_driver = nullptr;

void signalHandler(int sig) {
    (void)sig;
    g_running = false;
    if (g_driver) {
        g_driver->stop();
    }
}

int main(int argc, char* argv[]) {
    std::string socket_path = "/run/zenoh/modem_sat0.sock";
    if (argc > 1) {
        socket_path = argv[1];
    }

    std::cout << "Zenoh Modem Driver Example - LEO Satellite\n";
    std::cout << "Socket: " << socket_path << "\n";

    // Set up signal handlers
    std::signal(SIGINT, signalHandler);
    std::signal(SIGTERM, signalHandler);

    // Create modem instance
    LeoSatelliteModem modem;

    // Create driver with C++ wrapper
    zenoh::modem::Driver driver(
        [&modem]() { return modem.getInfo(); },
        [&modem]() { return modem.pollMetrics(); },
        [&modem]() { return modem.getState(); }
    );

    driver.set_request_handler([&modem](const zenoh_request_t& req) {
        return modem.handleRequest(req);
    });

    driver.set_metrics_interval(500);
    
    // Link driver to modem for event emission
    modem.setDriver(&driver);
    g_driver = &driver;

    // Simulate connection sequence in background
    std::thread connect_thread([&modem]() {
        modem.simulateConnection();
    });

    // Run driver
    std::cout << "[SAT] Starting driver...\n";
    int ret = driver.run(socket_path);

    if (ret != 0) {
        std::cerr << "Driver error: " << ret << "\n";
    }

    // Cleanup
    std::cout << "[SAT] Shutting down...\n";
    connect_thread.join();

    return ret;
}
