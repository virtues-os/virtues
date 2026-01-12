//
//  ContentView.swift
//  Virtues
//
//  Main TabView container - entry point after app launch
//

import SwiftUI

// Environment key for tab navigation
struct SelectedTabKey: EnvironmentKey {
    static let defaultValue: Binding<Int> = .constant(0)
}

extension EnvironmentValues {
    var selectedTab: Binding<Int> {
        get { self[SelectedTabKey.self] }
        set { self[SelectedTabKey.self] = newValue }
    }
}

struct ContentView: View {
    @State private var selectedTab = 1  // Default to "Today" tab
    @ObservedObject private var audioManager = AudioManager.shared

    var body: some View {
        TabView(selection: $selectedTab) {
            DataView()
                .tabItem {
                    Label("Data", systemImage: audioManager.isRecording ? "waveform.path.ecg.rectangle" : "waveform.path.ecg")
                }
                .tag(0)
                .badge(audioManager.isRecording ? "REC" : nil)

            TodayView()
                .tabItem {
                    Label("Today", systemImage: "sun.max.fill")
                }
                .tag(1)

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape.fill")
                }
                .tag(2)
        }
        .environment(\.selectedTab, $selectedTab)
        .tint(.warmPrimary)
        .onChange(of: selectedTab) { _, _ in
            Haptics.selection()
        }
    }
}

#Preview {
    ContentView()
}
