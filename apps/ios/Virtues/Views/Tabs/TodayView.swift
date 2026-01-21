//
//  TodayView.swift
//  Virtues
//
//  Today's data visualization - satisfies Apple's "user benefit" requirement
//

import SwiftUI
import Charts
import MapKit
import HealthKit

struct TodayView: View {
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var audioManager = AudioManager.shared
    @ObservedObject private var batteryManager = BatteryManager.shared
    @ObservedObject private var barometerManager = BarometerManager.shared
    @ObservedObject private var contactsManager = ContactsManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared

    @Environment(\.selectedTab) private var selectedTab

    @State private var heartRateSamples: [HeartRateSample] = []
    @State private var todaySteps: Int = 0
    @State private var todayWorkouts: [WorkoutSummary] = []
    @State private var locationDataPoints: [LocationDataPoint] = []
    @State private var speechBlocks: [SpeechBlock] = []
    @State private var batteryHistory: [BatteryDataPoint] = []
    @State private var barometerHistory: [BarometerDataPoint] = []
    @State private var todaysContacts: [NewContact] = []
    @State private var isLoading = true
    @State private var mapCameraPosition: MapCameraPosition = .automatic

    var body: some View {
        NavigationView {
            ScrollView(showsIndicators: false) {
                VStack(spacing: 20) {
                    // Hero summary card (always visible)
                    heroSummaryCard

                    // Live environment dashboard (visible if battery or barometer enabled)
                    if batteryManager.isMonitoring || barometerManager.isMonitoring {
                        liveEnvironmentCard
                    }

                    // Loading state
                    if isLoading {
                        ProgressView()
                            .padding(.vertical, 60)
                    } else {
                        // Heart Rate Chart
                        if healthKitManager.isAuthorized {
                            heartRateSection
                        }

                        // Movement Map
                        if locationManager.hasPermission {
                            movementSection
                        }

                        // Audio Recordings
                        if audioManager.hasPermission {
                            recordingsSection
                        }

                        // Battery Status
                        if batteryManager.isMonitoring {
                            batterySection
                        }

                        // Barometer/Altitude
                        if barometerManager.isMonitoring {
                            barometerSection
                        }

                        // Contacts
                        if contactsManager.isEnabled {
                            contactsSection
                        }

                        // Empty state
                        if !healthKitManager.isAuthorized && !locationManager.hasPermission && !audioManager.hasPermission && !batteryManager.isMonitoring && !barometerManager.isMonitoring && !contactsManager.isEnabled {
                            emptyState
                        }
                    }
                }
                .padding(.vertical)
            }
            .background(Color.warmBackground)
            .navigationTitle("Today")
            .navigationBarTitleDisplayMode(.inline)
            .onAppear {
                loadTodayData()
            }
            .refreshable {
                await refreshData()
            }
        }
        .navigationViewStyle(StackNavigationViewStyle())
    }

    private func refreshData() async {
        Haptics.light()
        loadTodayData()
        fitMapToRoute()
        try? await Task.sleep(nanoseconds: 300_000_000)
    }

    // MARK: - Time of Day Greeting

    private var timeOfDayGreeting: String {
        let hour = Calendar.current.component(.hour, from: Date())
        switch hour {
        case 5..<12: return "Good morning"
        case 12..<17: return "Good afternoon"
        case 17..<21: return "Good evening"
        default: return "Good night"
        }
    }

    // MARK: - Hero Summary Card

    private var heroSummaryCard: some View {
        VStack(spacing: 8) {
            Text(timeOfDayGreeting)
                .font(.system(size: 28, weight: .bold, design: .serif))
            Text(Date(), format: .dateTime.weekday(.wide).month(.wide).day())
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
            Text("Your AI is learning about your day")
                .font(.caption)
                .foregroundColor(.warmForegroundSubtle)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(16)
        .padding(.horizontal)
    }

    // MARK: - Live Environment Dashboard

    private var liveEnvironmentCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Live Environment")
                .font(.caption)
                .fontWeight(.medium)
                .foregroundColor(.warmForegroundMuted)
                .textCase(.uppercase)
                .tracking(0.5)

            HStack(spacing: 12) {
                // Battery metric
                if batteryManager.isMonitoring {
                    metricCard(
                        icon: batteryIcon,
                        iconColor: batteryColor,
                        value: "\(Int(batteryManager.batteryLevel * 100))%",
                        label: "Battery"
                    )
                }

                // Altitude metric (if barometer available and monitoring)
                if barometerManager.isMonitoring {
                    metricCard(
                        icon: "mountain.2.fill",
                        iconColor: .warmInfo,
                        value: barometerManager.relativeAltitude.map {
                            String(format: "%.0fm", $0)
                        } ?? "--",
                        label: "Altitude"
                    )

                    metricCard(
                        icon: "gauge.with.dots.needle.33percent",
                        iconColor: .warmForegroundMuted,
                        value: barometerManager.currentPressure.map {
                            String(format: "%.0f", $0)
                        } ?? "--",
                        label: "kPa"
                    )
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    private func metricCard(icon: String, iconColor: Color, value: String, label: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundColor(iconColor)
            Text(value)
                .font(.title2)
                .fontWeight(.semibold)
            Text(label)
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
        .background(Color.warmBackground)
        .cornerRadius(10)
    }

    // MARK: - Heart Rate Section

    private var heartRateSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "heart.fill")
                    .foregroundColor(.warmError)
                Text("Heart Rate")
                    .h3Style()
                Spacer()
                if let latest = heartRateSamples.last {
                    Text("\(Int(latest.bpm)) BPM")
                        .font(.subheadline)
                        .foregroundColor(.warmForegroundMuted)
                }
            }

            if heartRateSamples.isEmpty {
                Text("No heart rate data today")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 40)
            } else {
                Chart(heartRateSamples) { sample in
                    LineMark(
                        x: .value("Time", sample.date),
                        y: .value("BPM", sample.bpm)
                    )
                    .foregroundStyle(Color.warmError)
                }
                .frame(height: 150)
                .chartXAxis {
                    AxisMarks(values: .stride(by: .hour, count: 6)) { value in
                        AxisValueLabel(format: .dateTime.hour())
                    }
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    // MARK: - Steps Card

    private var stepsCard: some View {
        VStack(spacing: 8) {
            Image(systemName: "figure.walk")
                .font(.title2)
                .foregroundColor(.warmSuccess)

            Text("\(todaySteps)")
                .font(.title)
                .fontWeight(.bold)

            Text("steps")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
        .frame(maxWidth: .infinity)
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    // MARK: - Workouts Card

    private var workoutsCard: some View {
        VStack(spacing: 8) {
            Image(systemName: "flame.fill")
                .font(.title2)
                .foregroundColor(.warmWarning)

            Text("\(todayWorkouts.count)")
                .font(.title)
                .fontWeight(.bold)

            Text(todayWorkouts.count == 1 ? "workout" : "workouts")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
        .frame(maxWidth: .infinity)
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    // MARK: - Movement Section

    private var movementSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "location.fill")
                    .foregroundColor(.warmInfo)
                Text("Movement")
                    .h3Style()
                Spacer()
                Text("\(locationDataPoints.count) points")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            if locationDataPoints.isEmpty {
                Text("No location data today")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 40)
            } else {
                Map(position: $mapCameraPosition) {
                    MapPolyline(coordinates: locationCoordinates)
                        .stroke(Color.warmInfo, lineWidth: 3)

                    if let first = locationCoordinates.first {
                        Annotation("Start", coordinate: first) {
                            Circle()
                                .fill(Color.warmSuccess)
                                .frame(width: 12, height: 12)
                        }
                    }

                    if let last = locationCoordinates.last, locationCoordinates.count > 1 {
                        Annotation("Now", coordinate: last) {
                            Circle()
                                .fill(Color.warmError)
                                .frame(width: 12, height: 12)
                        }
                    }
                }
                .frame(height: 200)
                .cornerRadius(8)
                .overlay(alignment: .topTrailing) {
                    Button(action: {
                        Haptics.light()
                        fitMapToRoute()
                    }) {
                        Image(systemName: "location.fill")
                            .font(.system(size: 14))
                            .foregroundColor(.warmPrimary)
                            .padding(8)
                            .background(.ultraThinMaterial)
                            .cornerRadius(8)
                    }
                    .padding(8)
                }
                .onAppear {
                    fitMapToRoute()
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    /// Convert location data points to CLLocationCoordinate2D array for MapPolyline
    private var locationCoordinates: [CLLocationCoordinate2D] {
        locationDataPoints.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) }
    }

    /// Fit map camera to show all location points with padding
    private func fitMapToRoute() {
        guard !locationCoordinates.isEmpty else { return }

        // Calculate bounding rect for all coordinates
        var rect = locationCoordinates.reduce(MKMapRect.null) { rect, coord in
            let point = MKMapPoint(coord)
            let pointRect = MKMapRect(x: point.x, y: point.y, width: 0, height: 0)
            return rect.union(pointRect)
        }

        // Ensure minimum span (~500 meters) so single points don't over-zoom
        let minMeters: Double = 500
        let minSize = minMeters * MKMapPointsPerMeterAtLatitude(locationCoordinates[0].latitude)
        if rect.width < minSize || rect.height < minSize {
            let center = MKMapPoint(x: rect.midX, y: rect.midY)
            rect = MKMapRect(
                x: center.x - minSize / 2,
                y: center.y - minSize / 2,
                width: max(rect.width, minSize),
                height: max(rect.height, minSize)
            )
        }

        // Add padding (15% on each side)
        let paddedRect = rect.insetBy(dx: -rect.width * 0.15, dy: -rect.height * 0.15)

        withAnimation(.easeInOut(duration: 0.3)) {
            mapCameraPosition = .rect(paddedRect)
        }
    }

    // MARK: - Recordings Section

    private var recordingsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "mic.fill")
                    .foregroundColor(.warmSuccess)
                Text("Recordings")
                    .h3Style()
                Spacer()
                Text(formattedRecordingDuration)
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            if speechBlocks.isEmpty {
                Text("No recordings today")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 20)
            } else {
                Chart {
                    ForEach(speechBlocks) { block in
                        RectangleMark(
                            xStart: .value("Start", block.startTime),
                            xEnd: .value("End", block.endTime),
                            y: .value("Recording", "Speech")
                        )
                        .foregroundStyle(Color.warmSuccess.opacity(0.7))
                        .cornerRadius(2)
                    }
                }
                .frame(height: 40)
                .chartXScale(domain: todayTimeRange)
                .chartXAxis {
                    AxisMarks(values: .stride(by: .hour, count: 3)) { _ in
                        AxisValueLabel(format: .dateTime.hour())
                        AxisGridLine()
                    }
                }
                .chartYAxis(.hidden)
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    /// Total recording duration formatted as "Xh Ym" or "Xm"
    private var formattedRecordingDuration: String {
        let totalSeconds = speechBlocks.reduce(0) { $0 + $1.duration }
        let hours = Int(totalSeconds) / 3600
        let minutes = (Int(totalSeconds) % 3600) / 60

        if hours > 0 {
            return "\(hours)h \(minutes)m recorded"
        } else if minutes > 0 {
            return "\(minutes)m recorded"
        } else {
            return "\(speechBlocks.count) segments"
        }
    }

    // MARK: - Battery Section

    private var batterySection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: batteryIcon)
                    .foregroundColor(batteryColor)
                Text("Battery")
                    .h3Style()
                Spacer()
                Text("\(Int(batteryManager.batteryLevel * 100))%")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            if batteryHistory.isEmpty {
                // Show current status even without history
                VStack(spacing: 12) {
                    HStack(spacing: 20) {
                        // Current level gauge
                        ZStack {
                            Circle()
                                .stroke(Color.warmBorder, lineWidth: 8)
                                .frame(width: 80, height: 80)
                            Circle()
                                .trim(from: 0, to: CGFloat(batteryManager.batteryLevel))
                                .stroke(batteryColor, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                                .frame(width: 80, height: 80)
                                .rotationEffect(.degrees(-90))
                            VStack(spacing: 2) {
                                Text("\(Int(batteryManager.batteryLevel * 100))%")
                                    .font(.headline)
                                if batteryManager.batteryState == .charging {
                                    Image(systemName: "bolt.fill")
                                        .font(.caption)
                                        .foregroundColor(.warmSuccess)
                                }
                            }
                        }

                        VStack(alignment: .leading, spacing: 4) {
                            Text(batteryManager.batteryState == .charging ? "Charging" : "On Battery")
                                .font(.subheadline)
                                .fontWeight(.medium)
                            Text("Chart builds over time")
                                .font(.caption)
                                .foregroundColor(.warmForegroundMuted)
                        }
                        Spacer()
                    }
                }
                .padding(.vertical, 8)
            } else {
                // Battery level chart
                Chart(batteryHistory) { dataPoint in
                    AreaMark(
                        x: .value("Time", dataPoint.date),
                        y: .value("Level", dataPoint.level * 100)
                    )
                    .foregroundStyle(
                        LinearGradient(
                            colors: [batteryColor.opacity(0.3), batteryColor.opacity(0.1)],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )

                    LineMark(
                        x: .value("Time", dataPoint.date),
                        y: .value("Level", dataPoint.level * 100)
                    )
                    .foregroundStyle(batteryColor)
                    .lineStyle(StrokeStyle(lineWidth: 2))

                    // Show charging indicator
                    if dataPoint.isCharging {
                        PointMark(
                            x: .value("Time", dataPoint.date),
                            y: .value("Level", dataPoint.level * 100)
                        )
                        .foregroundStyle(Color.warmSuccess)
                        .symbolSize(30)
                    }
                }
                .frame(height: 120)
                .chartYScale(domain: 0...100)
                .chartXAxis {
                    AxisMarks(values: .stride(by: .hour, count: 6)) { _ in
                        AxisValueLabel(format: .dateTime.hour())
                        AxisGridLine()
                    }
                }
                .chartYAxis {
                    AxisMarks(values: [0, 50, 100]) { value in
                        AxisValueLabel {
                            if let intValue = value.as(Int.self) {
                                Text("\(intValue)%")
                                    .font(.caption2)
                            }
                        }
                        AxisGridLine()
                    }
                }
            }

            // Low Power Mode indicator
            if ProcessInfo.processInfo.isLowPowerModeEnabled {
                HStack(spacing: 4) {
                    Image(systemName: "bolt.circle.fill")
                        .foregroundColor(.warmWarning)
                        .font(.caption)
                    Text("Low Power Mode enabled")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    private var batteryIcon: String {
        let level = batteryManager.batteryLevel
        switch batteryManager.batteryState {
        case .charging, .full:
            return "battery.100percent.bolt"
        default:
            if level > 0.75 {
                return "battery.100percent"
            } else if level > 0.5 {
                return "battery.75percent"
            } else if level > 0.25 {
                return "battery.50percent"
            } else {
                return "battery.25percent"
            }
        }
    }

    private var batteryColor: Color {
        let level = batteryManager.batteryLevel
        switch batteryManager.batteryState {
        case .charging, .full:
            return .warmSuccess
        default:
            if level > 0.2 {
                return .warmSuccess
            } else if level > 0.1 {
                return .warmWarning
            } else {
                return .warmError
            }
        }
    }

    // MARK: - Barometer Section

    private var barometerSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "barometer")
                    .foregroundColor(.warmInfo)
                Text("Altitude")
                    .h3Style()
                Spacer()
                if let current = barometerManager.relativeAltitude {
                    Text(String(format: "%.1f m", current))
                        .font(.subheadline)
                        .foregroundColor(.warmForegroundMuted)
                }
            }

            if barometerHistory.isEmpty {
                // Show current values even without history
                VStack(spacing: 12) {
                    HStack(spacing: 20) {
                        VStack(spacing: 8) {
                            if let altitude = barometerManager.relativeAltitude {
                                Text(String(format: "%.1f m", altitude))
                                    .font(.title2)
                                    .fontWeight(.semibold)
                                Text("Relative Altitude")
                                    .font(.caption)
                                    .foregroundColor(.warmForegroundMuted)
                            } else {
                                Text("--")
                                    .font(.title2)
                                    .fontWeight(.semibold)
                                Text("Waiting for data")
                                    .font(.caption)
                                    .foregroundColor(.warmForegroundMuted)
                            }
                        }
                        .frame(maxWidth: .infinity)

                        if let pressure = barometerManager.currentPressure {
                            VStack(spacing: 8) {
                                Text(String(format: "%.1f", pressure))
                                    .font(.title2)
                                    .fontWeight(.semibold)
                                Text("kPa")
                                    .font(.caption)
                                    .foregroundColor(.warmForegroundMuted)
                            }
                            .frame(maxWidth: .infinity)
                        }
                    }

                    Text("Chart builds over time")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
                .padding(.vertical, 8)
            } else {
                Chart(barometerHistory) { dataPoint in
                    LineMark(
                        x: .value("Time", dataPoint.date),
                        y: .value("Altitude", dataPoint.altitudeMeters)
                    )
                    .foregroundStyle(Color.warmInfo)
                    .lineStyle(StrokeStyle(lineWidth: 2))
                }
                .frame(height: 120)
                .chartXAxis {
                    AxisMarks(values: .stride(by: .hour, count: 6)) { _ in
                        AxisValueLabel(format: .dateTime.hour())
                        AxisGridLine()
                    }
                }
                .chartYAxis {
                    AxisMarks { value in
                        AxisValueLabel {
                            if let doubleValue = value.as(Double.self) {
                                Text(String(format: "%.0fm", doubleValue))
                                    .font(.caption2)
                            }
                        }
                        AxisGridLine()
                    }
                }
            }

            // Pressure info
            if let pressure = barometerManager.currentPressure {
                HStack(spacing: 4) {
                    Image(systemName: "gauge")
                        .foregroundColor(.warmForegroundMuted)
                        .font(.caption)
                    Text(String(format: "%.1f kPa", pressure))
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    // MARK: - Contacts Section

    private var contactsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "person.badge.plus")
                    .foregroundColor(.warmPrimary)
                Text("New Contacts")
                    .h3Style()
                Spacer()
                Text("\(todaysContacts.count) today")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            if todaysContacts.isEmpty {
                Text("No contacts synced today")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 20)
            } else {
                // Show first 5 contacts
                let displayContacts = Array(todaysContacts.prefix(5))

                VStack(alignment: .leading, spacing: 8) {
                    ForEach(displayContacts) { contact in
                        HStack(spacing: 10) {
                            Circle()
                                .fill(Color.warmPrimary.opacity(0.2))
                                .frame(width: 32, height: 32)
                                .overlay(
                                    Text(contact.name.prefix(1).uppercased())
                                        .font(.caption)
                                        .fontWeight(.medium)
                                        .foregroundColor(.warmPrimary)
                                )

                            Text(contact.name)
                                .font(.subheadline)

                            Spacer()
                        }
                    }

                    if todaysContacts.count > 5 {
                        Text("+ \(todaysContacts.count - 5) more")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                    }
                }
            }

            // Last sync time
            if let lastSync = contactsManager.lastSyncDate {
                HStack(spacing: 4) {
                    Image(systemName: "arrow.clockwise")
                        .foregroundColor(.warmForegroundMuted)
                        .font(.caption)
                    Text("Synced \(lastSync, format: .relative(presentation: .named))")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .padding(.horizontal)
    }

    // MARK: - Empty State

    private var emptyState: some View {
        VStack(spacing: 20) {
            Image(systemName: "waveform.path.ecg")
                .font(.system(size: 48))
                .foregroundColor(.warmPrimary)

            Text("Start Collecting Data")
                .h3Style()

            Text("Enable sensors in the Data tab to begin tracking your day. Your data stays on-device until you connect to a server.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)

            Button(action: {
                Haptics.light()
                selectedTab.wrappedValue = 0  // Switch to Data tab
            }) {
                HStack {
                    Image(systemName: "plus.circle.fill")
                    Text("Enable Sensors")
                }
                .font(.headline)
                .padding(.horizontal, 24)
                .padding(.vertical, 12)
                .background(Color.warmPrimary)
                .foregroundColor(.white)
                .cornerRadius(10)
            }
        }
        .padding(.vertical, 60)
    }

    // MARK: - Data Loading

    private func loadTodayData() {
        isLoading = true
        loadHeartRate()
        loadSteps()
        loadWorkouts()
        loadLocationTrack()
        loadSpeechBlocks()
        loadBatteryHistory()
        loadBarometerHistory()
        loadTodaysContacts()

        isLoading = false
    }

    private func loadHeartRate() {
        guard healthKitManager.isAuthorized else { return }

        let healthStore = HKHealthStore()
        let heartRateType = HKQuantityType.quantityType(forIdentifier: .heartRate)!

        let calendar = Calendar.current
        let startOfDay = calendar.startOfDay(for: Date())

        let predicate = HKQuery.predicateForSamples(
            withStart: startOfDay,
            end: Date(),
            options: .strictStartDate
        )

        let sortDescriptor = NSSortDescriptor(key: HKSampleSortIdentifierStartDate, ascending: true)

        let query = HKSampleQuery(
            sampleType: heartRateType,
            predicate: predicate,
            limit: HKObjectQueryNoLimit,
            sortDescriptors: [sortDescriptor]
        ) { _, samples, _ in
            guard let samples = samples as? [HKQuantitySample] else { return }

            let heartRates = samples.map { sample in
                HeartRateSample(
                    date: sample.startDate,
                    bpm: sample.quantity.doubleValue(for: HKUnit.count().unitDivided(by: .minute()))
                )
            }

            DispatchQueue.main.async {
                self.heartRateSamples = heartRates
            }
        }

        healthStore.execute(query)
    }

    private func loadSteps() {
        guard healthKitManager.isAuthorized else { return }

        let healthStore = HKHealthStore()
        let stepsType = HKQuantityType.quantityType(forIdentifier: .stepCount)!

        let calendar = Calendar.current
        let startOfDay = calendar.startOfDay(for: Date())

        let predicate = HKQuery.predicateForSamples(
            withStart: startOfDay,
            end: Date(),
            options: .strictStartDate
        )

        let query = HKStatisticsQuery(
            quantityType: stepsType,
            quantitySamplePredicate: predicate,
            options: .cumulativeSum
        ) { _, result, _ in
            guard let sum = result?.sumQuantity() else { return }

            DispatchQueue.main.async {
                self.todaySteps = Int(sum.doubleValue(for: .count()))
            }
        }

        healthStore.execute(query)
    }

    private func loadWorkouts() {
        guard healthKitManager.isAuthorized else { return }

        let healthStore = HKHealthStore()

        let calendar = Calendar.current
        let startOfDay = calendar.startOfDay(for: Date())

        let predicate = HKQuery.predicateForSamples(
            withStart: startOfDay,
            end: Date(),
            options: .strictStartDate
        )

        let sortDescriptor = NSSortDescriptor(key: HKSampleSortIdentifierStartDate, ascending: false)

        let query = HKSampleQuery(
            sampleType: HKWorkoutType.workoutType(),
            predicate: predicate,
            limit: HKObjectQueryNoLimit,
            sortDescriptors: [sortDescriptor]
        ) { _, samples, _ in
            guard let workouts = samples as? [HKWorkout] else { return }

            let summaries = workouts.map { workout in
                WorkoutSummary(
                    type: workout.workoutActivityType.name,
                    duration: workout.duration
                )
            }

            DispatchQueue.main.async {
                self.todayWorkouts = summaries
            }
        }

        healthStore.execute(query)
    }

    private func loadLocationTrack() {
        let points = SQLiteManager.shared.getTodaysLocationTrack()
        locationDataPoints = downsamplePreservingEndpoints(points, maxCount: 1000)
    }

    private func loadSpeechBlocks() {
        let blocks = SQLiteManager.shared.getTodaysSpeechBlocks()
        speechBlocks = downsamplePreservingEndpoints(blocks, maxCount: 500)
    }

    private func loadBatteryHistory() {
        batteryHistory = SQLiteManager.shared.getTodaysBatteryHistory()
    }

    private func loadBarometerHistory() {
        barometerHistory = SQLiteManager.shared.getTodaysBarometerHistory()
    }

    private func loadTodaysContacts() {
        todaysContacts = SQLiteManager.shared.getTodaysNewContacts()
    }

    private func downsamplePreservingEndpoints<T>(_ items: [T], maxCount: Int) -> [T] {
        guard items.count > maxCount, maxCount > 0 else {
            return items
        }

        guard maxCount > 2 else {
            return [items.first, items.last].compactMap { $0 }
        }

        guard let first = items.first, let last = items.last, items.count > 2 else {
            return items
        }

        let middleItems = Array(items[1..<(items.count - 1)])
        let targetMiddleCount = maxCount - 2
        let stride = Int(ceil(Double(middleItems.count) / Double(targetMiddleCount)))

        var sampled: [T] = [first]
        for (index, item) in middleItems.enumerated() where index % stride == 0 {
            sampled.append(item)
        }
        sampled.append(last)

        return sampled
    }

    // MARK: - Time Range

    private var todayTimeRange: ClosedRange<Date> {
        let calendar = Calendar.current
        let today = Date()
        let start = calendar.date(bySettingHour: 6, minute: 0, second: 0, of: today)!
        let end = calendar.date(bySettingHour: 22, minute: 0, second: 0, of: today)!
        return start...end
    }
}

// MARK: - Supporting Types

struct HeartRateSample: Identifiable {
    let id = UUID()
    let date: Date
    let bpm: Double
}

struct WorkoutSummary: Identifiable {
    let id = UUID()
    let type: String
    let duration: TimeInterval
}

extension HKWorkoutActivityType {
    var name: String {
        switch self {
        case .running: return "Run"
        case .walking: return "Walk"
        case .cycling: return "Cycle"
        case .swimming: return "Swim"
        case .hiking: return "Hike"
        case .yoga: return "Yoga"
        case .functionalStrengthTraining: return "Strength"
        case .traditionalStrengthTraining: return "Strength"
        case .highIntensityIntervalTraining: return "HIIT"
        default: return "Workout"
        }
    }
}

#Preview {
    TodayView()
}
