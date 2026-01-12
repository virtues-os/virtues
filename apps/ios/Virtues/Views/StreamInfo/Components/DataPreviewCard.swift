//
//  DataPreviewCard.swift
//  Virtues
//
//  Shows a preview of JSON data in a code-styled card
//

import SwiftUI

struct DataPreviewCard: View {
    let title: String
    let jsonString: String
    var maxLines: Int = 15

    private var formattedJSON: String {
        // Truncate to max lines
        let lines = jsonString.components(separatedBy: "\n")
        if lines.count > maxLines {
            let truncated = lines.prefix(maxLines).joined(separator: "\n")
            return truncated + "\n  ..."
        }
        return jsonString
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption)
                .fontWeight(.medium)
                .foregroundColor(.warmForegroundMuted)

            ScrollView(.horizontal, showsIndicators: false) {
                Text(formattedJSON)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundColor(.warmForeground)
                    .padding(12)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.warmBorder)
            .cornerRadius(8)
        }
    }
}

/// Helper to format any Encodable to pretty-printed JSON string
extension Encodable {
    var prettyJSON: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601

        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return string
    }
}

#Preview {
    VStack(spacing: 16) {
        DataPreviewCard(
            title: "Sample Heart Rate Record",
            jsonString: """
            {
              "deviceId": "ABC123",
              "streamType": "healthkit",
              "timestamp": "2025-01-15T10:30:00Z",
              "data": {
                "type": "heart_rate",
                "value": 72,
                "unit": "bpm"
              }
            }
            """
        )

        DataPreviewCard(
            title: "Sample Location Record",
            jsonString: """
            {
              "latitude": 37.7749,
              "longitude": -122.4194,
              "altitude": 10.5,
              "accuracy": 5.0,
              "timestamp": "2025-01-15T10:30:00Z"
            }
            """
        )
    }
    .padding()
}
