//
//  ManualCodeEntryView.swift
//  Virtues
//
//  "Link a device" (fully-remote enrollment): the new device enters the linking
//  code shown on an already-paired device. The code resolves the box's reach via
//  atlas; once the voucher approves, the bearer is pulled from the box over iroh.
//  See docs/reach-enrollment.md.
//

import SwiftUI

struct ManualCodeEntryView: View {
    /// Called with the canonical (typed) code when the user submits.
    let onEnter: (String) -> Void
    let onCancel: () -> Void

    @State private var code: String = ""

    /// Canonical `XXXXX-XXXXX`: uppercase, alnum only, dash after 5. Matches the
    /// box's `random_link_code` formatting so the hash agrees regardless of how
    /// the user typed it.
    private var canonical: String {
        let alnum = code.uppercased().filter { $0.isLetter || $0.isNumber }.prefix(10)
        if alnum.count > 5 {
            return "\(alnum.prefix(5))-\(alnum.suffix(alnum.count - 5))"
        }
        return String(alnum)
    }

    private var isComplete: Bool {
        code.uppercased().filter { $0.isLetter || $0.isNumber }.count == 10
    }

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 8) {
                Text("Link this device")
                    .font(.title2).bold()
                Text("Open Virtues on a device you've already paired, choose “Link a device,” and enter the code it shows.")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
                    .multilineTextAlignment(.center)
            }

            TextField("XXXXX-XXXXX", text: $code)
                .font(.system(.title, design: .monospaced))
                .multilineTextAlignment(.center)
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
                .onChange(of: code) { _, newVal in
                    // Keep the field showing the canonical grouped form.
                    code = canonical
                }
                .padding()
                .background(Color.warmSurface)
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.warmBorder))
                .cornerRadius(12)

            Button(action: {
                Haptics.light()
                onEnter(canonical)
            }) {
                Text("Continue")
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(isComplete ? Color.warmPrimary : Color.warmForegroundMuted.opacity(0.3))
                    .foregroundColor(.white)
                    .cornerRadius(12)
            }
            .disabled(!isComplete)

            Button("Cancel", action: onCancel)
                .foregroundColor(.warmForegroundMuted)

            Spacer()
        }
        .padding()
    }
}
