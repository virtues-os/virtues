//
//  StreamStatusBadge.swift
//  Virtues
//
//  Status indicator component for stream info pages
//

import SwiftUI

enum StreamStatus {
    case active
    case syncing
    case warning(String)
    case error(String)
    case disabled

    var color: Color {
        switch self {
        case .active: return .warmSuccess
        case .syncing: return .warmInfo
        case .warning: return .warmWarning
        case .error: return .warmError
        case .disabled: return .warmForegroundSubtle
        }
    }

    var icon: String {
        switch self {
        case .active: return "checkmark.circle.fill"
        case .syncing: return "arrow.triangle.2.circlepath"
        case .warning: return "exclamationmark.triangle.fill"
        case .error: return "xmark.circle.fill"
        case .disabled: return "pause.circle.fill"
        }
    }

    var label: String {
        switch self {
        case .active: return "Active"
        case .syncing: return "Syncing"
        case .warning(let msg): return msg
        case .error(let msg): return msg
        case .disabled: return "Disabled"
        }
    }
}

struct StreamStatusBadge: View {
    let status: StreamStatus
    var showLabel: Bool = true

    var body: some View {
        HStack(spacing: 6) {
            if case .syncing = status {
                ProgressView()
                    .scaleEffect(0.8)
            } else {
                Image(systemName: status.icon)
                    .foregroundColor(status.color)
            }

            if showLabel {
                Text(status.label)
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .foregroundColor(status.color)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(status.color.opacity(0.15))
        .cornerRadius(8)
    }
}

#Preview {
    VStack(spacing: 16) {
        StreamStatusBadge(status: .active)
        StreamStatusBadge(status: .syncing)
        StreamStatusBadge(status: .warning("Sync overdue"))
        StreamStatusBadge(status: .error("Permission denied"))
        StreamStatusBadge(status: .disabled)
    }
    .padding()
}
