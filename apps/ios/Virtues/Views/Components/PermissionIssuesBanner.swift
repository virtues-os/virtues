//
//  PermissionIssuesBanner.swift
//  Virtues
//
//

import SwiftUI

struct PermissionIssuesBanner: View {
    let issue: PermissionIssue

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "info.circle.fill")
                .font(.title2)
                .foregroundColor(.orange)

            VStack(alignment: .leading, spacing: 4) {
                Text("\(issue.type.rawValue) Limited")
                    .font(.headline)
                    .foregroundColor(.primary)

                Text("Some features may not work without this permission")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()
        }
        .padding()
        .background(Color.orange.opacity(0.1))
        .cornerRadius(12)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.orange.opacity(0.2), lineWidth: 1)
        )
    }
}
