//
//  ContentView.swift
//  Virtues
//
//  Main TabView container - entry point after app launch.
//  Virtues on iOS is a pure data collector: it collects raw streams and uploads
//  them to your box. Viewing/analysis lives on the box's web UI, not here — so
//  there are two tabs (Data = what's collecting + syncing, Settings), no on-device
//  dashboard.
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
    @State private var selectedTab = 0  // Land on Data (the collector), not a dashboard
    @ObservedObject private var audioManager = AudioManager.shared
    @ObservedObject private var deviceManager = DeviceManager.shared

    var body: some View {
        VStack(spacing: 0) {
            // Until the device is paired to a box, nothing can upload — surface
            // that instead of dropping the user on a silent screen with no cue.
            if !deviceManager.isConfigured {
                Button {
                    Haptics.light()
                    selectedTab = 1  // Settings
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "link")
                        Text("Pair with your box to start collecting")
                            .font(.subheadline.weight(.medium))
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .foregroundColor(.warmPrimary)
                    .background(Color.warmPrimary.opacity(0.1))
                }
            }

            TabView(selection: $selectedTab) {
                DataView()
                    .tabItem {
                        Label("Data", systemImage: audioManager.isRecording ? "waveform.path.ecg.rectangle" : "waveform.path.ecg")
                    }
                    .tag(0)

                SettingsView()
                    .tabItem {
                        Label("Settings", systemImage: "gearshape.fill")
                    }
                    .tag(1)
            }
            .environment(\.selectedTab, $selectedTab)
            .tint(.warmPrimary)
            .onChange(of: selectedTab) { _, _ in
                Haptics.selection()
            }
        }
    }
}

#Preview {
    ContentView()
}
