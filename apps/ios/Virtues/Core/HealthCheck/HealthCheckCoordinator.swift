//
//  HealthCheckCoordinator.swift
//  Virtues
//
//  Centralized health monitoring for all data collection managers
//  Reduces main thread work and consolidates health check logic
//

import Foundation

/// Coordinates health checks across all managers
final class HealthCheckCoordinator {
    static let shared = HealthCheckCoordinator()

    // MARK: - Properties

    private var registeredManagers: [HealthCheckable] = []
    private var healthCheckTimer: ReliableTimer?
    private let healthCheckInterval: TimeInterval = 30.0
    private let lock = NSLock()

    // MARK: - Health Status Tracking

    private(set) var lastCheckDate: Date?
    private(set) var managerStatuses: [String: HealthStatus] = [:]

    // Exponential backoff for a manager that keeps failing its check. Because
    // `performHealthCheck()` also *attempts recovery* (e.g. AudioManager restarts
    // recording), calling it every 30s on a non-recoverable failure = a restart
    // thrash loop (battery/CPU drain). We instead retry with growing delay.
    //
    // The ceiling doubles as the worst-case recovery-detection latency: while a
    // manager is backed off we neither re-check nor observe it, so if it recovers
    // on its own its status stays stale (and `managerStatuses` reports the old
    // value) until the next attempt. 5 min bounds that lag while still throttling
    // thrash hard (30s → 60s → 120s → 240s → 300s).
    private var consecutiveUnhealthy: [String: Int] = [:]
    private var backoffUntil: [String: Date] = [:]
    private let maxBackoff: TimeInterval = 300  // 5 min ceiling

    private init() {}

    // MARK: - Registration

    /// Register a manager for health monitoring
    /// - Parameter manager: The manager to monitor
    func register(_ manager: HealthCheckable) {
        lock.lock()
        defer { lock.unlock() }

        // Avoid duplicate registrations
        if !registeredManagers.contains(where: { $0 === manager }) {
            registeredManagers.append(manager)
            print("🏥 Registered \(manager.healthCheckName) for health monitoring")
        }
    }

    /// Unregister a manager from health monitoring
    /// - Parameter manager: The manager to remove
    func unregister(_ manager: HealthCheckable) {
        lock.lock()
        defer { lock.unlock() }

        registeredManagers.removeAll { $0 === manager }
        managerStatuses.removeValue(forKey: manager.healthCheckName)
        print("🏥 Unregistered \(manager.healthCheckName) from health monitoring")
    }

    // MARK: - Health Check Coordination

    /// Start the coordinated health check timer
    func startMonitoring() {
        lock.lock()
        let managersCount = registeredManagers.count
        lock.unlock()

        print("🏥 Starting coordinated health monitoring for \(managersCount) managers")

        // Stop any existing timer
        stopMonitoring()

        // Create a single timer that checks all managers
        healthCheckTimer = ReliableTimer.builder()
            .interval(healthCheckInterval)
            .queue(.main)  // Run on main for thread safety with managers
            .handler { [weak self] in
                self?.performAllHealthChecks()
            }
            .build()

        // Perform initial health check
        performAllHealthChecks()
    }

    /// Stop health monitoring
    func stopMonitoring() {
        healthCheckTimer?.cancel()
        healthCheckTimer = nil
        print("🏥 Stopped coordinated health monitoring")
    }

    /// Perform health checks on all registered managers
    private func performAllHealthChecks() {
        lock.lock()
        let managers = registeredManagers
        lock.unlock()

        guard !managers.isEmpty else { return }

        lastCheckDate = Date()
        var healthyCount = 0
        var unhealthyCount = 0
        var disabledCount = 0

        let now = Date()
        for manager in managers {
            let name = manager.healthCheckName

            // Skip (and don't attempt recovery for) a manager still in backoff
            // after repeated failures — prevents the 30s restart thrash loop.
            if let until = backoffUntil[name], until > now {
                continue
            }

            let status = manager.performHealthCheck()

            lock.lock()
            managerStatuses[name] = status
            lock.unlock()

            switch status {
            case .healthy:
                healthyCount += 1
                consecutiveUnhealthy[name] = 0
                backoffUntil[name] = nil

            case .unhealthy(let reason):
                unhealthyCount += 1
                let count = (consecutiveUnhealthy[name] ?? 0) + 1
                consecutiveUnhealthy[name] = count
                // 30s, 60s, 120s … capped at 30 min.
                let delay = min(healthCheckInterval * pow(2, Double(count - 1)), maxBackoff)
                backoffUntil[name] = now.addingTimeInterval(delay)
                print("⚠️ [\(name)] Unhealthy: \(reason) — retry in \(Int(delay))s (attempt \(count))")

            case .disabled:
                disabledCount += 1
                consecutiveUnhealthy[name] = 0
                backoffUntil[name] = nil
            }
        }

        #if DEBUG
        if unhealthyCount > 0 {
            print("🏥 Health check complete: \(healthyCount) healthy, \(unhealthyCount) unhealthy, \(disabledCount) disabled")
        }
        #endif
    }

}
