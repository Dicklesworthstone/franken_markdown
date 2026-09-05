import SwiftUI
import UIKit
import UniformTypeIdentifiers

struct MarkdownDocumentStatusBar: View {
    @ObservedObject var session: MarkdownDocumentSession
    let source: String
    let save: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: session.hasCurrentDocument ? "doc.text.fill" : "doc.badge.plus")
                .foregroundStyle(statusColor)
                .frame(width: 24, height: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(session.displayName)
                    .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .lineLimit(1)
                    .truncationMode(.middle)
                HStack(spacing: 6) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 7, height: 7)
                    Text(statusText)
                        .font(.system(size: Lab.size(8.5), weight: .black, design: .monospaced))
                        .foregroundStyle(statusColor)
                }
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 6)
            Button(action: save) {
                ViewThatFits(in: .horizontal) {
                    Label("Save", systemImage: "square.and.arrow.down")
                    Image(systemName: "square.and.arrow.down")
                        .frame(width: 24, height: 24)
                }
                .font(.system(size: Lab.size(11), weight: .bold))
                .frame(minHeight: 44)
            }
            .buttonStyle(.borderedProminent)
            .tint(Lab.emerald)
            .foregroundStyle(Lab.onEmerald)
            .disabled(
                session.isSaving
                    || (session.hasCurrentDocument && !session.isDirty(source: source))
            )
            .accessibilityLabel(session.hasCurrentDocument ? "Save" : "Save new Markdown file")
            .accessibilityHint(
                session.hasCurrentDocument
                    ? "Save changes back to \(session.displayName)"
                    : "Choose a Files location for this Markdown document"
            )
            .accessibilityIdentifier("save-markdown-document")
        }
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .padding(.vertical, 6)
        .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(statusColor.opacity(0.26), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("markdown-document-status")
    }

    private var statusText: String {
        if session.attention == .changedOnDisk { return "CHANGED ON DISK" }
        if session.attention == .unavailable { return "FILE UNAVAILABLE" }
        if session.isSaving { return "SAVING" }
        if session.isDirty(source: source) {
            return session.hasCurrentDocument ? "EDITED" : "UNSAVED"
        }
        return session.hasCurrentDocument ? "SAVED" : "NEW"
    }

    private var statusColor: Color {
        if session.attention != nil { return Lab.danger }
        if session.isSaving || session.isDirty(source: source) { return Lab.amber }
        return Lab.emerald
    }
}

enum TypeScalePresetStep: String, CaseIterable, Identifiable {
    case extraSmall = "XS"
    case small = "SM"
    case medium = "MD"
    case large = "LG"
    case extraLarge = "XL"
    case huge = "2XL"

    var id: Self { self }

    var scale: Double {
        switch self {
        case .extraSmall: 0.75
        case .small: 0.875
        case .medium: 1.0
        case .large: 1.125
        case .extraLarge: 1.25
        case .huge: 1.5
        }
    }

    var label: String {
        switch self {
        case .extraSmall: "XS · 75%"
        case .small: "SM · 87.5%"
        case .medium: "MD · 100%"
        case .large: "LG · 112.5%"
        case .extraLarge: "XL · 125%"
        case .huge: "2XL · 150%"
        }
    }

    static func closest(to scale: Double) -> Self {
        allCases.min(by: { abs($0.scale - scale) < abs($1.scale - scale) }) ?? .medium
    }

    func next(delta: Int) -> Self {
        let all = Self.allCases
        guard let index = all.firstIndex(of: self) else { return self }
        let target = min(all.count - 1, max(0, index + delta))
        return all[target]
    }
}

struct MarkdownRenderFontSizeControl: View {
    @Binding var renderFontScale: Double

    var body: some View {
        let currentStep = TypeScalePresetStep.closest(to: renderFontScale)
        HStack(spacing: 8) {
            Button {
                renderFontScale = currentStep.next(delta: -1).scale
            } label: {
                Image(systemName: "textformat.size.smaller")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.bordered)
            .disabled(currentStep == .extraSmall)
            .accessibilityLabel("Decrease rendered type size to smaller preset")

            Button {
                renderFontScale = 1.0
            } label: {
                Text(currentStep.label)
                    .font(.system(size: Lab.size(11), weight: .black, design: .monospaced))
                    .frame(minWidth: 88)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Rendered type size \(currentStep.label). Tap to reset to standard size")

            Button {
                renderFontScale = currentStep.next(delta: 1).scale
            } label: {
                Image(systemName: "textformat.size.larger")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.bordered)
            .disabled(currentStep == .huge)
            .accessibilityLabel("Increase rendered type size to larger preset")
        }
        .tint(Lab.emerald)
    }
}

struct ShareActivityView: UIViewControllerRepresentable {
    let fileURL: URL

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let contentType = UTType(filenameExtension: fileURL.pathExtension) ?? .data
        let provider = NSItemProvider()
        provider.suggestedName = fileURL.lastPathComponent
        provider.registerFileRepresentation(
            forTypeIdentifier: contentType.identifier,
            fileOptions: [],
            visibility: .all
        ) { completion in
            // A copied file preserves the artifact identity. Sharing a bare HTML URL can
            // otherwise be interpreted as a web link or plain text by destinations.
            completion(fileURL, false, nil)
            return nil
        }
        let configuration = UIActivityItemsConfiguration(itemProviders: [provider])
        return UIActivityViewController(activityItemsConfiguration: configuration)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
