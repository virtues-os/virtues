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
    @ObservedObject private var contactsManager = ContactsManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared

    @Environment(\.selectedTab) private var selectedTab

    @State private var heartRateSamples: [HeartRateSample] = []
    @State private var locationDataPoints: [LocationDataPoint] = []
    @State private var speechBlocks: [SpeechBlock] = []
    @State private var todaysContacts: [NewContact] = []
    @State private var isLoading = true
    @State private var mapCameraPosition: MapCameraPosition = .automatic

    var body: some View {
        NavigationView {
            ScrollView(showsIndicators: false) {
                VStack(spacing: 20) {
                    // Hero summary card (always visible)
                    heroSummaryCard

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

                        // Contacts
                        if contactsManager.isEnabled {
                            contactsSection
                        }

                        // Empty state
                        if !healthKitManager.isAuthorized && !locationManager.hasPermission && !audioManager.hasPermission && !contactsManager.isEnabled {
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
        loadLocationTrack()
        loadSpeechBlocks()
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

    private func loadLocationTrack() {
        let points = SQLiteManager.shared.getTodaysLocationTrack()
        locationDataPoints = downsamplePreservingEndpoints(points, maxCount: 1000)
    }

    private func loadSpeechBlocks() {
        let blocks = SQLiteManager.shared.getTodaysSpeechBlocks()
        speechBlocks = downsamplePreservingEndpoints(blocks, maxCount: 500)
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

#Preview {
    TodayView()
}
