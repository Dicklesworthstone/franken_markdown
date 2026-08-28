import SwiftUI

private enum ForgeLane: String, CaseIterable, Identifiable {
    case write = "Write"
    case preview = "Preview"
    case inspect = "Inspect"
    var id: Self { self }
}

struct ForgeView: View {
    @StateObject private var renderer = MarkdownRendererModel()
    @State private var lane: ForgeLane = .write
    @FocusState private var editorFocused: Bool

    init() {
        // Deterministic launch-state hook for screenshot/UI gates. Production
        // launches omit it and always open in the useful writing surface.
        let requested = ProcessInfo.processInfo.environment["FMD_INITIAL_LANE"]
        _lane = State(initialValue: ForgeLane(rawValue: requested ?? "") ?? .write)
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                LaboratoryBackground()
                VStack(spacing: 14) {
                    masthead
                    if geometry.size.width >= 760 {
                        wideForge
                    } else {
                        compactForge
                    }
                    footer
                }
                .padding(.horizontal, geometry.size.width >= 760 ? 22 : 14)
                .padding(.top, 12)
            }
        }
        .onChange(of: renderer.source) { _, _ in renderer.scheduleRender() }
        .onReceive(NotificationCenter.default.publisher(for: .renderMarkdownNow)) { _ in
            renderer.renderNow()
        }
    }

    private var masthead: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { brand; Spacer(); statusPill }
            VStack(alignment: .leading, spacing: 10) { brand; statusPill }
        }
    }

    private var brand: some View {
        HStack(spacing: 12) {
            Image("MonsterIcon")
                .resizable()
                .scaledToFill()
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                .shadow(color: Lab.emerald.opacity(0.42), radius: 13)
                .accessibilityLabel("Friendly FrankenMarkdown document monster")
            VStack(alignment: .leading, spacing: 1) {
                Text("FRANKENMARKDOWN")
                    .font(.system(size: Lab.size(20), weight: .black, design: .monospaced))
                    .minimumScaleFactor(0.66)
                    .lineLimit(1)
                    .foregroundStyle(Lab.text)
                Text("DOCUMENT_PRESS // private · offline · Rust")
                    .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.secondary)
            }
        }
    }

    private var statusPill: some View {
        HStack(spacing: 8) {
            Image(systemName: statusSymbol)
            Text(statusText)
                .lineLimit(1)
            if renderer.phase == .rendering { ProgressView().controlSize(.small) }
        }
        .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
        .foregroundStyle(statusColor)
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(Color.black.opacity(0.38), in: Capsule())
        .overlay(Capsule().stroke(statusColor.opacity(0.28)))
    }

    private var compactForge: some View {
        VStack(spacing: 12) {
            Picker("Workspace", selection: $lane) {
                ForEach(ForgeLane.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            switch lane {
            case .write: editorPanel
            case .preview: previewPanel
            case .inspect: inspectorPanel
            }
        }
    }

    private var wideForge: some View {
        HStack(spacing: 14) {
            editorPanel
                .frame(minWidth: 320, maxWidth: .infinity)
            previewPanel
                .frame(minWidth: 360, maxWidth: .infinity)
        }
    }

    private var editorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    LabLabel(text: "01 · The Source")
                    Spacer()
                    Text("\(renderer.source.utf8.count) bytes")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
                TextEditor(text: $renderer.source)
                    .focused($editorFocused)
                    .font(.system(size: Lab.size(14), design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .scrollContentBackground(.hidden)
                    .padding(8)
                    .background(Color.black.opacity(0.42), in: RoundedRectangle(cornerRadius: 12))
                    .frame(minHeight: 320)
                HStack {
                    Button {
                        editorFocused = false
                        renderer.renderNow()
                    } label: {
                        Label("Forge Preview", systemImage: "sparkles.rectangle.stack")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    Spacer()
                    Text("⌘R")
                        .font(.system(size: Lab.size(10), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
            }
        }
    }

    private var previewPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    LabLabel(text: "02 · The Reading View")
                    Spacer()
                    if let elapsed = renderer.elapsedMS {
                        Text(String(format: "%.1f ms · %d bytes", elapsed, renderer.outputBytes))
                            .font(.system(size: Lab.size(9), design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                    }
                }
                RendererWebView(webView: renderer.webView)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(Lab.stroke))
                    .frame(minHeight: 320)
                if renderer.diagnosticCount > 0 {
                    Label("\(renderer.diagnosticCount) source diagnostic(s)", systemImage: "exclamationmark.triangle")
                        .font(.system(size: Lab.size(10), design: .monospaced))
                        .foregroundStyle(Lab.amber)
                }
            }
        }
    }

    private var inspectorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 14) {
                LabLabel(text: "03 · The Press")
                Label("Raw HTML is off", systemImage: "lock.shield")
                Label("No network surface", systemImage: "network.slash")
                Label("The exact Rust/WASM package is bundled", systemImage: "shippingbox")
                Text("Typography, attachments, diagnostics, PDF export, document browsing, widgets, and Shortcuts are the next tracked foundation milestones.")
                    .foregroundStyle(Lab.secondary)
            }
            .font(.system(size: Lab.size(13)))
            .foregroundStyle(Lab.text)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var footer: some View {
        VStack(spacing: 4) {
            Text("Rendered entirely on this device · nothing is uploaded")
            Text("If you like this free app, please show your appreciation by trying out my paid skills site at [JeffreysSkills.md](https://jeffreys-skills.md).")
                .tint(Lab.emerald)
                .frame(maxWidth: 560)
        }
        .font(.system(size: Lab.size(9), design: .monospaced))
        .foregroundStyle(Lab.secondary.opacity(0.78))
        .multilineTextAlignment(.center)
        .padding(.bottom, 8)
    }

    private var statusText: String {
        switch renderer.phase {
        case .loading: "warming the document press"
        case .ready: "Rust press ready"
        case .rendering: "parse · theme · layout · render"
        case .failed(let message): message
        }
    }

    private var statusSymbol: String {
        switch renderer.phase {
        case .loading: "bolt.horizontal.circle"
        case .ready: "checkmark.seal"
        case .rendering: "gearshape.2"
        case .failed: "exclamationmark.triangle"
        }
    }

    private var statusColor: Color {
        switch renderer.phase {
        case .loading, .rendering: Lab.amber
        case .ready: Lab.emerald
        case .failed: Lab.danger
        }
    }
}
